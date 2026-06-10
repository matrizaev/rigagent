//! Retail application use cases.

use crate::domain::retail::{
    DecisionRun, DecisionRunDetails, DemandSimulator, DomainError, InventoryPosition, Product,
    RestockOption, RestockOptionRequest, RestockOptionScorer, RestockOrder, RestockOrderDetails,
    SalesOrder, SalesOrderDetails, Sku, SpaceUnits, StockQuantity,
};

use super::{
    AcceptedRestockOrder, AdvanceSimulation, AdvanceSimulationResult, ApplicationError, Clock,
    DecisionAgentRequest, DecisionResult, EventCount, IdGenerator, ProposedRestockOrder,
    RejectedRestockProposal, ReplenishmentDecisionAgent, RetailSnapshot, RetailStore,
    RunRestockDecision, RunWorkflowCycle, SeedRetailScenario, WorkflowCycleResult,
};

/// Retail workflow application service.
#[derive(Debug)]
pub struct RetailWorkflow<Store, Runs, Agent, Ids, Dates> {
    store: Store,
    decision_runs: Runs,
    decision_agent: Agent,
    id_generator: Ids,
    clock: Dates,
}

impl<Store, Runs, Agent, Ids, Dates> RetailWorkflow<Store, Runs, Agent, Ids, Dates> {
    /// Create a retail workflow service from its adapters.
    #[must_use]
    pub const fn new(
        store: Store,
        decision_runs: Runs,
        decision_agent: Agent,
        id_generator: Ids,
        clock: Dates,
    ) -> Self {
        Self {
            store,
            decision_runs,
            decision_agent,
            id_generator,
            clock,
        }
    }

    /// Return the retail store adapter.
    #[must_use]
    pub const fn store(&self) -> &Store {
        &self.store
    }

    /// Return the decision-run store adapter.
    #[must_use]
    pub const fn decision_runs(&self) -> &Runs {
        &self.decision_runs
    }
}

impl<Store, Runs, Agent, Ids, Dates> RetailWorkflow<Store, Runs, Agent, Ids, Dates>
where
    Store: RetailStore,
    Runs: super::DecisionRunStore,
    Agent: ReplenishmentDecisionAgent,
    Ids: IdGenerator,
    Dates: Clock,
{
    /// Seed durable retail state.
    ///
    /// # Errors
    ///
    /// Returns an error when state already exists without reset or the store fails.
    pub fn seed_scenario(&mut self, command: &SeedRetailScenario) -> Result<(), ApplicationError> {
        if self.store.state_exists()? && !command.reset {
            return Err(ApplicationError::StateAlreadyExists);
        }

        self.store
            .seed_scenario(command.scenario_path.as_path(), command.reset)
    }

    /// Return the current retail snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error when the store cannot load state.
    pub fn get_snapshot(&self) -> Result<RetailSnapshot, ApplicationError> {
        self.store.load_snapshot()
    }

    /// Advance the deterministic simulation.
    ///
    /// # Errors
    ///
    /// Returns an error when domain behavior or a port fails.
    pub fn advance_simulation(
        &mut self,
        command: AdvanceSimulation,
    ) -> Result<AdvanceSimulationResult, ApplicationError> {
        if command.days == 0 {
            return Err(ApplicationError::NonPositiveCommand { field: "days" });
        }

        let mut current_date = self.store.load_snapshot()?.current_date;
        let mut days_advanced = 0_u64;
        let mut received_count = EventCount::new(0);
        let mut sales_count = EventCount::new(0);
        let mut lost_units = StockQuantity::new(0);

        while days_advanced < command.days {
            let received = self.store.receive_due_restocks(current_date)?;
            received_count = received_count
                .checked_add(event_count_from_len(
                    received.len(),
                    "received restock count",
                )?)
                .ok_or(ApplicationError::CountOverflow {
                    operation: "received restock count",
                })?;

            let snapshot = self.store.load_snapshot()?;
            let mut updated_inventory = snapshot.inventory.clone();
            let mut sales_orders = Vec::new();

            for product in &snapshot.products {
                let inventory = inventory_for_mut(&mut updated_inventory, product.sku())?;
                let demand = DemandSimulator::simulate_day(
                    product.demand_rate(),
                    inventory.demand_backlog(),
                )?;
                let fulfilled = inventory.apply_demand_simulation(demand)?;
                let sale = SalesOrder::record(SalesOrderDetails {
                    id: self.id_generator.sales_order_id()?,
                    sale_date: current_date,
                    sku: product.sku().clone(),
                    requested: demand.requested_units,
                    fulfilled,
                    unit_price: product.unit_price(),
                    unit_cost: product.unit_cost(),
                })?;
                lost_units = lost_units.checked_add(sale.lost_units())?;
                sales_orders.push(sale);
            }

            sales_count = sales_count
                .checked_add(event_count_from_len(
                    sales_orders.len(),
                    "sales order count",
                )?)
                .ok_or(ApplicationError::CountOverflow {
                    operation: "sales order count",
                })?;
            self.store
                .record_sales_day(sales_orders, updated_inventory)?;

            current_date = current_date.checked_add_days(1)?;
            self.store.advance_shop_date(current_date)?;
            days_advanced =
                days_advanced
                    .checked_add(1)
                    .ok_or(ApplicationError::CountOverflow {
                        operation: "simulation day count",
                    })?;
        }

        Ok(AdvanceSimulationResult {
            current_date,
            days_advanced,
            received_restock_count: received_count,
            sales_order_count: sales_count,
            lost_units,
        })
    }

    /// Run one restock decision.
    ///
    /// # Errors
    ///
    /// Returns an error when the agent, a port, or domain validation fails.
    pub async fn run_restock_decision(
        &mut self,
        command: RunRestockDecision,
    ) -> Result<DecisionResult, ApplicationError> {
        if command.max_restock_orders.is_zero() {
            return Err(ApplicationError::NonPositiveCommand {
                field: "max_restock_orders",
            });
        }

        let snapshot = self.store.load_snapshot()?;
        let decision_date = self.clock.today()?;
        let decision_run_id = self.id_generator.decision_run_id()?;
        let run = DecisionRun::start(DecisionRunDetails {
            id: decision_run_id.clone(),
            decision_date,
            horizon: command.horizon,
        });
        self.decision_runs.start_decision_run(run)?;

        let decision_result = self
            .run_agent_and_validate(snapshot, decision_date, &decision_run_id, command)
            .await;

        match decision_result {
            Ok(result) => Ok(result),
            Err(error) => {
                self.decision_runs
                    .fail_decision_run(&decision_run_id, error.to_string())?;
                Err(error)
            }
        }
    }

    async fn run_agent_and_validate(
        &mut self,
        snapshot: RetailSnapshot,
        decision_date: crate::domain::retail::SimulationDate,
        decision_run_id: &crate::domain::retail::DecisionRunId,
        command: RunRestockDecision,
    ) -> Result<DecisionResult, ApplicationError> {
        let ranked_options = ranked_options(&snapshot, command.horizon)?;
        let open_orders = self.store.open_restock_orders()?;
        let agent_response = self
            .decision_agent
            .decide(DecisionAgentRequest {
                snapshot: snapshot.clone(),
                ranked_options,
                open_orders,
                max_orders: command.max_restock_orders,
            })
            .await?;

        let mut accepted = Vec::new();
        let mut rejected = Vec::new();
        let mut projected_orders = snapshot.open_restocks.clone();

        for proposal in agent_response.proposed_orders {
            if quantity_from_len(accepted.len(), "accepted proposal count")?
                >= command.max_restock_orders
            {
                break;
            }

            match validate_proposal(
                &snapshot,
                &projected_orders,
                &proposal,
                decision_date,
                decision_run_id,
                &mut self.id_generator,
            ) {
                Ok(order) => {
                    projected_orders.push(order.clone());
                    accepted.push(AcceptedRestockOrder { order });
                }
                Err(error) => rejected.push(RejectedRestockProposal {
                    sku: proposal.sku,
                    quantity: proposal.quantity,
                    reason: error.to_string(),
                }),
            }
        }

        let orders = accepted
            .iter()
            .map(|accepted_order| accepted_order.order.clone())
            .collect::<Vec<_>>();
        self.store.place_restock_orders(orders)?;
        self.decision_runs.complete_decision_run(
            decision_run_id,
            agent_response.summary.clone(),
            quantity_from_len(accepted.len(), "created restock count")?,
        )?;

        Ok(DecisionResult {
            decision_run_id: decision_run_id.clone(),
            accepted_orders: accepted,
            rejected_proposals: rejected,
            summary: agent_response.summary,
        })
    }

    /// Run repeated simulation and decision cycles.
    ///
    /// # Errors
    ///
    /// Returns an error when any decision or simulation step fails.
    pub async fn run_workflow_cycle(
        &mut self,
        command: RunWorkflowCycle,
    ) -> Result<WorkflowCycleResult, ApplicationError> {
        if command.total_days == 0 {
            return Err(ApplicationError::NonPositiveCommand {
                field: "total_days",
            });
        }
        if command.decision_interval_days == 0 {
            return Err(ApplicationError::NonPositiveCommand {
                field: "decision_interval_days",
            });
        }

        let decision_command = RunRestockDecision {
            horizon: command.horizon,
            max_restock_orders: command.max_restock_orders,
        };
        self.run_restock_decision(decision_command).await?;
        let mut decision_runs = EventCount::new(1);

        let mut days_advanced = 0_u64;
        while days_advanced < command.total_days {
            self.advance_simulation(AdvanceSimulation { days: 1 })?;
            days_advanced =
                days_advanced
                    .checked_add(1)
                    .ok_or(ApplicationError::CountOverflow {
                        operation: "workflow day count",
                    })?;
            if checked_rem(days_advanced, command.decision_interval_days)? == 0 {
                self.run_restock_decision(decision_command).await?;
                decision_runs = decision_runs.checked_add(EventCount::new(1)).ok_or(
                    ApplicationError::CountOverflow {
                        operation: "workflow decision count",
                    },
                )?;
            }
        }

        Ok(WorkflowCycleResult {
            days_advanced,
            decision_runs,
            final_date: self.store.load_snapshot()?.current_date,
        })
    }
}

fn ranked_options(
    snapshot: &RetailSnapshot,
    horizon: crate::domain::retail::DecisionHorizonDays,
) -> Result<Vec<RestockOption>, ApplicationError> {
    let total_projected_space = projected_space(snapshot, &snapshot.open_restocks, None)?;
    let mut options = Vec::new();
    for product in &snapshot.products {
        let inventory = inventory_for(&snapshot.inventory, product.sku())?;
        let inbound = open_inbound_quantity(&snapshot.open_restocks, product.sku())?;
        let projected_space_for_sku = product
            .unit_space()
            .checked_mul_quantity(inventory.on_hand().checked_add(inbound)?)?;
        let reserved_capacity = total_projected_space.checked_sub(projected_space_for_sku)?;
        let scored = RestockOptionScorer::score(&RestockOptionRequest {
            product: product.clone(),
            inventory: inventory.clone(),
            open_inbound_quantity: inbound,
            capacity: snapshot.capacity,
            reserved_capacity,
            current_date: snapshot.current_date,
            horizon,
        });
        let Some(option) = scored.or_else(skip_out_of_bounds_option)? else {
            continue;
        };
        options.push(option);
    }

    options.sort_by(compare_restock_options);
    Ok(options)
}

fn compare_restock_options(left: &RestockOption, right: &RestockOption) -> std::cmp::Ordering {
    let left_density = u128::from(left.expected_profit().cents())
        .saturating_mul(u128::from(right.occupied_space().units()));
    let right_density = u128::from(right.expected_profit().cents())
        .saturating_mul(u128::from(left.occupied_space().units()));

    right_density
        .cmp(&left_density)
        .then_with(|| {
            right
                .expected_profit()
                .cents()
                .cmp(&left.expected_profit().cents())
        })
        .then_with(|| left.sku().cmp(right.sku()))
}

fn skip_out_of_bounds_option(
    error: DomainError,
) -> Result<Option<RestockOption>, ApplicationError> {
    match error {
        DomainError::RestockQuantityOutOfBounds { .. } => Ok(None),
        other => Err(ApplicationError::Domain(other)),
    }
}

fn validate_proposal<Ids>(
    snapshot: &RetailSnapshot,
    projected_orders: &[RestockOrder],
    proposal: &ProposedRestockOrder,
    decision_date: crate::domain::retail::SimulationDate,
    decision_run_id: &crate::domain::retail::DecisionRunId,
    id_generator: &mut Ids,
) -> Result<RestockOrder, ApplicationError>
where
    Ids: IdGenerator,
{
    if proposal.quantity.is_zero() {
        return Err(ApplicationError::NonPositiveProposalQuantity {
            sku: proposal.sku.clone(),
        });
    }

    let product = product_for(&snapshot.products, &proposal.sku)?;
    if !product.is_active() {
        return Err(ApplicationError::InactiveProduct {
            sku: proposal.sku.clone(),
        });
    }

    product
        .bounded_order_quantity(proposal.quantity)
        .map_err(|error| match error {
            DomainError::RestockQuantityOutOfBounds { .. } => {
                ApplicationError::ProposalOutOfBounds {
                    sku: proposal.sku.clone(),
                    quantity: proposal.quantity,
                }
            }
            other => ApplicationError::Domain(other),
        })?;

    if projected_orders.iter().any(|order| {
        order.sku() == &proposal.sku
            && order.status() == crate::domain::retail::RestockOrderStatus::Open
    }) {
        return Err(ApplicationError::DuplicateOpenRestockOrder {
            sku: proposal.sku.clone(),
        });
    }

    let projected_capacity = projected_space(snapshot, projected_orders, Some(proposal))?;
    if projected_capacity > snapshot.capacity {
        let used_space = projected_space(snapshot, projected_orders, None)?;
        let required = projected_capacity.checked_sub(used_space)?;
        let available = snapshot.capacity.checked_sub(used_space)?;
        return Err(ApplicationError::CapacityOverflowProposal {
            sku: proposal.sku.clone(),
            required,
            available,
            overflow: required.checked_sub(available)?,
        });
    }

    Ok(RestockOrder::open(RestockOrderDetails {
        id: id_generator.restock_order_id()?,
        sku: proposal.sku.clone(),
        quantity: proposal.quantity,
        ordered_at: decision_date,
        eta: product.restock_eta(decision_date)?,
        decision_run_id: decision_run_id.clone(),
        rationale: proposal.rationale.clone(),
    })?)
}

fn projected_space(
    snapshot: &RetailSnapshot,
    projected_orders: &[RestockOrder],
    proposed: Option<&ProposedRestockOrder>,
) -> Result<SpaceUnits, ApplicationError> {
    let mut total = SpaceUnits::new(0);
    for product in &snapshot.products {
        let inventory = inventory_for(&snapshot.inventory, product.sku())?;
        let mut quantity = inventory.on_hand();
        quantity = quantity.checked_add(open_inbound_quantity(projected_orders, product.sku())?)?;
        if let Some(proposal) = proposed
            && &proposal.sku == product.sku()
        {
            quantity = quantity.checked_add(proposal.quantity)?;
        }
        total = total.checked_add(product.unit_space().checked_mul_quantity(quantity)?)?;
    }

    Ok(total)
}

fn inventory_for<'a>(
    inventory: &'a [InventoryPosition],
    sku: &Sku,
) -> Result<&'a InventoryPosition, ApplicationError> {
    inventory
        .iter()
        .find(|position| position.sku() == sku)
        .ok_or_else(|| ApplicationError::InventoryNotFound { sku: sku.clone() })
}

fn inventory_for_mut<'a>(
    inventory: &'a mut [InventoryPosition],
    sku: &Sku,
) -> Result<&'a mut InventoryPosition, ApplicationError> {
    inventory
        .iter_mut()
        .find(|position| position.sku() == sku)
        .ok_or_else(|| ApplicationError::InventoryNotFound { sku: sku.clone() })
}

fn product_for<'a>(products: &'a [Product], sku: &Sku) -> Result<&'a Product, ApplicationError> {
    products
        .iter()
        .find(|product| product.sku() == sku)
        .ok_or_else(|| ApplicationError::SkuNotFound { sku: sku.clone() })
}

fn open_inbound_quantity(
    open_orders: &[RestockOrder],
    sku: &Sku,
) -> Result<StockQuantity, ApplicationError> {
    let mut total = StockQuantity::new(0);
    for order in open_orders {
        if order.sku() == sku && order.status() == crate::domain::retail::RestockOrderStatus::Open {
            total = total.checked_add(order.quantity())?;
        }
    }
    Ok(total)
}

fn quantity_from_len(
    len: usize,
    operation: &'static str,
) -> Result<StockQuantity, ApplicationError> {
    let units = u64::try_from(len).map_err(|_| ApplicationError::CountOverflow { operation })?;
    Ok(StockQuantity::new(units))
}

fn event_count_from_len(
    len: usize,
    operation: &'static str,
) -> Result<EventCount, ApplicationError> {
    let count = u64::try_from(len).map_err(|_| ApplicationError::CountOverflow { operation })?;
    Ok(EventCount::new(count))
}

fn checked_rem(dividend: u64, divisor: u64) -> Result<u64, ApplicationError> {
    dividend
        .checked_rem(divisor)
        .ok_or(ApplicationError::CountOverflow {
            operation: "workflow interval remainder",
        })
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::path::Path;

    use chrono::NaiveDate;

    use super::*;
    use crate::application::retail::{
        DecisionAgentResponse, DecisionRunStore, ProfitSummary, RetailStore,
    };
    use crate::domain::retail::{
        ApparelKind, Brand, DecisionHorizonDays, DecisionRunId, DecisionRunStatus, DemandBacklog,
        DemandRatePerDay, LeadTimeDays, MoneyCents, RestockOrderId, RestockOrderStatus,
        SalesOrderId, SizeLabel,
    };

    #[derive(Debug, Clone)]
    struct FakeStore {
        exists: bool,
        current_date: crate::domain::retail::SimulationDate,
        capacity: SpaceUnits,
        products: Vec<Product>,
        inventory: Vec<InventoryPosition>,
        restocks: Vec<RestockOrder>,
        sales: Vec<SalesOrder>,
        failures: Vec<FakeStoreFailure>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum FakeStoreFailure {
        LoadSnapshot,
        RecordSalesDay,
        PlaceRestockOrders,
    }

    impl FakeStore {
        fn new(
            current_date: crate::domain::retail::SimulationDate,
            capacity: SpaceUnits,
            products: Vec<Product>,
            inventory: Vec<InventoryPosition>,
        ) -> Self {
            Self {
                exists: true,
                current_date,
                capacity,
                products,
                inventory,
                restocks: Vec::new(),
                sales: Vec::new(),
                failures: Vec::new(),
            }
        }

        fn inject_failure(&mut self, failure: FakeStoreFailure) {
            self.failures.push(failure);
        }

        fn fails(&self, failure: FakeStoreFailure) -> bool {
            self.failures.contains(&failure)
        }
    }

    impl RetailStore for FakeStore {
        fn state_exists(&self) -> Result<bool, ApplicationError> {
            Ok(self.exists)
        }

        fn load_snapshot(&self) -> Result<RetailSnapshot, ApplicationError> {
            if self.fails(FakeStoreFailure::LoadSnapshot) {
                return Err(ApplicationError::store_failure(
                    "load snapshot",
                    "invalid persisted data injected by fake store",
                ));
            }

            Ok(RetailSnapshot {
                current_date: self.current_date,
                capacity: self.capacity,
                products: self.products.clone(),
                inventory: self.inventory.clone(),
                open_restocks: self.open_restock_orders()?,
                recent_sales: self.sales.clone(),
                profit_summary: self.profit_summary()?,
            })
        }

        fn seed_scenario(
            &mut self,
            _scenario_path: &Path,
            _reset: bool,
        ) -> Result<(), ApplicationError> {
            self.exists = true;
            Ok(())
        }

        fn receive_due_restocks(
            &mut self,
            on_date: crate::domain::retail::SimulationDate,
        ) -> Result<Vec<RestockOrder>, ApplicationError> {
            let mut received = Vec::new();
            for order in &mut self.restocks {
                if order.status() == RestockOrderStatus::Open && order.eta() <= on_date {
                    order.receive(on_date)?;
                    let inventory = self
                        .inventory
                        .iter_mut()
                        .find(|position| position.sku() == order.sku())
                        .ok_or_else(|| ApplicationError::InventoryNotFound {
                            sku: order.sku().clone(),
                        })?;
                    inventory.receive_restock(order.quantity())?;
                    received.push(order.clone());
                }
            }
            Ok(received)
        }

        fn record_sales_day(
            &mut self,
            sales_orders: Vec<SalesOrder>,
            inventory: Vec<InventoryPosition>,
        ) -> Result<(), ApplicationError> {
            if self.fails(FakeStoreFailure::RecordSalesDay) {
                return Err(ApplicationError::store_failure(
                    "record sales day",
                    "transaction failure injected by fake store",
                ));
            }

            self.sales.extend(sales_orders);
            self.inventory = inventory;
            Ok(())
        }

        fn place_restock_orders(
            &mut self,
            orders: Vec<RestockOrder>,
        ) -> Result<(), ApplicationError> {
            if self.fails(FakeStoreFailure::PlaceRestockOrders) {
                return Err(ApplicationError::store_failure(
                    "place restock orders",
                    "transaction failure injected by fake store",
                ));
            }

            for order in &orders {
                if self.restocks.iter().any(|existing| {
                    existing.status() == RestockOrderStatus::Open && existing.sku() == order.sku()
                }) {
                    return Err(ApplicationError::DuplicateOpenRestockOrder {
                        sku: order.sku().clone(),
                    });
                }
            }
            self.restocks.extend(orders);
            Ok(())
        }

        fn open_restock_orders(&self) -> Result<Vec<RestockOrder>, ApplicationError> {
            Ok(self
                .restocks
                .iter()
                .filter(|order| order.status() == RestockOrderStatus::Open)
                .cloned()
                .collect())
        }

        fn profit_summary(&self) -> Result<ProfitSummary, ApplicationError> {
            let mut revenue = MoneyCents::new(0);
            let mut cost = MoneyCents::new(0);
            let mut lost_units = StockQuantity::new(0);
            for sale in &self.sales {
                revenue = revenue.checked_add(sale.revenue())?;
                cost = cost.checked_add(sale.cost())?;
                lost_units = lost_units.checked_add(sale.lost_units())?;
            }
            Ok(ProfitSummary {
                revenue,
                cost,
                gross_profit: revenue.checked_sub(cost)?,
                lost_units,
            })
        }

        fn advance_shop_date(
            &mut self,
            next_date: crate::domain::retail::SimulationDate,
        ) -> Result<(), ApplicationError> {
            self.current_date = next_date;
            Ok(())
        }
    }

    #[derive(Debug, Clone, Default)]
    struct FakeDecisionRuns {
        runs: Vec<DecisionRun>,
    }

    impl DecisionRunStore for FakeDecisionRuns {
        fn start_decision_run(&mut self, run: DecisionRun) -> Result<(), ApplicationError> {
            self.runs.push(run);
            Ok(())
        }

        fn complete_decision_run(
            &mut self,
            run_id: &DecisionRunId,
            summary: String,
            created_restock_count: StockQuantity,
        ) -> Result<(), ApplicationError> {
            let run = self
                .runs
                .iter_mut()
                .find(|candidate| candidate.id() == run_id)
                .ok_or_else(|| ApplicationError::DecisionRunNotFound {
                    run_id: run_id.clone(),
                })?;
            run.complete(summary, created_restock_count)?;
            Ok(())
        }

        fn fail_decision_run(
            &mut self,
            run_id: &DecisionRunId,
            summary: String,
        ) -> Result<(), ApplicationError> {
            let run = self
                .runs
                .iter_mut()
                .find(|candidate| candidate.id() == run_id)
                .ok_or_else(|| ApplicationError::DecisionRunNotFound {
                    run_id: run_id.clone(),
                })?;
            run.fail(summary)?;
            Ok(())
        }

        fn decision_run(&self, run_id: &DecisionRunId) -> Result<DecisionRun, ApplicationError> {
            self.runs
                .iter()
                .find(|candidate| candidate.id() == run_id)
                .cloned()
                .ok_or_else(|| ApplicationError::DecisionRunNotFound {
                    run_id: run_id.clone(),
                })
        }
    }

    #[derive(Debug, Clone)]
    struct FakeAgent {
        response: Result<DecisionAgentResponse, ApplicationError>,
    }

    impl ReplenishmentDecisionAgent for FakeAgent {
        async fn decide(
            &self,
            _request: DecisionAgentRequest,
        ) -> Result<DecisionAgentResponse, ApplicationError> {
            self.response.clone()
        }
    }

    #[derive(Debug, Clone)]
    struct FakeIds {
        sales: VecDeque<SalesOrderId>,
        restocks: VecDeque<RestockOrderId>,
        decisions: VecDeque<DecisionRunId>,
    }

    impl FakeIds {
        fn new() -> Result<Self, ApplicationError> {
            Ok(Self {
                sales: VecDeque::from([
                    SalesOrderId::new("sale-1")?,
                    SalesOrderId::new("sale-2")?,
                    SalesOrderId::new("sale-3")?,
                    SalesOrderId::new("sale-4")?,
                ]),
                restocks: VecDeque::from([
                    RestockOrderId::new("restock-1")?,
                    RestockOrderId::new("restock-2")?,
                    RestockOrderId::new("restock-3")?,
                ]),
                decisions: VecDeque::from([
                    DecisionRunId::new("decision-1")?,
                    DecisionRunId::new("decision-2")?,
                    DecisionRunId::new("decision-3")?,
                ]),
            })
        }
    }

    impl IdGenerator for FakeIds {
        fn sales_order_id(&mut self) -> Result<SalesOrderId, ApplicationError> {
            self.sales
                .pop_front()
                .ok_or(ApplicationError::CountOverflow {
                    operation: "sales ID fake",
                })
        }

        fn restock_order_id(&mut self) -> Result<RestockOrderId, ApplicationError> {
            self.restocks
                .pop_front()
                .ok_or(ApplicationError::CountOverflow {
                    operation: "restock ID fake",
                })
        }

        fn decision_run_id(&mut self) -> Result<DecisionRunId, ApplicationError> {
            self.decisions
                .pop_front()
                .ok_or(ApplicationError::CountOverflow {
                    operation: "decision ID fake",
                })
        }
    }

    #[derive(Debug, Clone, Copy)]
    struct FakeClock {
        today: crate::domain::retail::SimulationDate,
    }

    impl Clock for FakeClock {
        fn today(&self) -> Result<crate::domain::retail::SimulationDate, ApplicationError> {
            Ok(self.today)
        }
    }

    type Workflow = RetailWorkflow<FakeStore, FakeDecisionRuns, FakeAgent, FakeIds, FakeClock>;

    fn workflow(store: FakeStore, agent: FakeAgent) -> Result<Workflow, ApplicationError> {
        Ok(RetailWorkflow::new(
            store,
            FakeDecisionRuns::default(),
            agent,
            FakeIds::new()?,
            FakeClock {
                today: clock_date()?,
            },
        ))
    }

    fn test_date() -> Result<crate::domain::retail::SimulationDate, ApplicationError> {
        NaiveDate::from_ymd_opt(2026, 6, 9)
            .map(crate::domain::retail::SimulationDate::new)
            .ok_or_else(|| ApplicationError::store_failure("test date", "invalid test date"))
    }

    fn clock_date() -> Result<crate::domain::retail::SimulationDate, ApplicationError> {
        NaiveDate::from_ymd_opt(2026, 6, 10)
            .map(crate::domain::retail::SimulationDate::new)
            .ok_or_else(|| ApplicationError::store_failure("test clock date", "invalid test date"))
    }

    fn product(
        sku: &str,
        daily_demand: &str,
        unit_space: u64,
    ) -> Result<Product, ApplicationError> {
        Ok(Product::from_details(
            crate::domain::retail::ProductDetails {
                sku: Sku::new(sku)?,
                kind: ApparelKind::Shirt,
                brand: Brand::new("North")?,
                size: SizeLabel::M,
                unit_cost: MoneyCents::new(1_000),
                unit_price: MoneyCents::new(2_500),
                unit_space: SpaceUnits::new(unit_space),
                demand_rate: daily_demand.parse::<DemandRatePerDay>()?,
                lead_time: LeadTimeDays::new(1)?,
                min_order_quantity: StockQuantity::new(1),
                max_order_quantity: StockQuantity::new(50),
                active: true,
            },
        )?)
    }

    fn inactive_product(sku: &str) -> Result<Product, ApplicationError> {
        Ok(Product::from_details(
            crate::domain::retail::ProductDetails {
                sku: Sku::new(sku)?,
                kind: ApparelKind::Shirt,
                brand: Brand::new("North")?,
                size: SizeLabel::M,
                unit_cost: MoneyCents::new(1_000),
                unit_price: MoneyCents::new(2_500),
                unit_space: SpaceUnits::new(1),
                demand_rate: "1.000".parse::<DemandRatePerDay>()?,
                lead_time: LeadTimeDays::new(1)?,
                min_order_quantity: StockQuantity::new(1),
                max_order_quantity: StockQuantity::new(50),
                active: false,
            },
        )?)
    }

    fn store_with_inventory(
        product: &Product,
        on_hand: StockQuantity,
        capacity: SpaceUnits,
    ) -> Result<FakeStore, ApplicationError> {
        Ok(FakeStore::new(
            test_date()?,
            capacity,
            vec![product.clone()],
            vec![InventoryPosition::new(
                product.sku().clone(),
                on_hand,
                DemandBacklog::ZERO,
            )],
        ))
    }

    fn agent_with_proposals(proposed_orders: Vec<ProposedRestockOrder>) -> FakeAgent {
        FakeAgent {
            response: Ok(DecisionAgentResponse {
                proposed_orders,
                summary: "decision complete".to_owned(),
            }),
        }
    }

    fn decision_command() -> Result<RunRestockDecision, ApplicationError> {
        Ok(RunRestockDecision {
            horizon: DecisionHorizonDays::new(14)?,
            max_restock_orders: StockQuantity::new(3),
        })
    }

    #[test]
    fn seed_requires_reset_when_state_exists() -> Result<(), ApplicationError> {
        let product = product("shirt-1", "1.000", 1)?;
        let store = store_with_inventory(&product, StockQuantity::new(0), SpaceUnits::new(100))?;
        let mut workflow = workflow(store, agent_with_proposals(Vec::new()))?;

        let result = workflow.seed_scenario(&SeedRetailScenario {
            scenario_path: "data/retail_scenario.yaml".into(),
            reset: false,
        });

        assert!(matches!(result, Err(ApplicationError::StateAlreadyExists)));
        Ok(())
    }

    #[test]
    fn advance_simulation_receives_due_restock_before_sales() -> Result<(), ApplicationError> {
        let product = product("shirt-1", "1.000", 1)?;
        let mut store =
            store_with_inventory(&product, StockQuantity::new(0), SpaceUnits::new(100))?;
        let date = test_date()?;
        store.restocks.push(RestockOrder::open(RestockOrderDetails {
            id: RestockOrderId::new("existing-restock")?,
            sku: product.sku().clone(),
            quantity: StockQuantity::new(5),
            ordered_at: date,
            eta: date,
            decision_run_id: DecisionRunId::new("decision-existing")?,
            rationale: "due now".to_owned(),
        })?);
        let mut workflow = workflow(store, agent_with_proposals(Vec::new()))?;

        let result = workflow.advance_simulation(AdvanceSimulation { days: 1 })?;

        assert_eq!(result.received_restock_count.count(), 1);
        assert_eq!(result.lost_units.units(), 0);
        let inventory = workflow.store().inventory.first().ok_or_else(|| {
            ApplicationError::store_failure("test inventory lookup", "missing inventory")
        })?;
        assert_eq!(inventory.on_hand().units(), 4);
        Ok(())
    }

    #[test]
    fn advance_simulation_records_lost_sales() -> Result<(), ApplicationError> {
        let product = product("shirt-1", "2.000", 1)?;
        let store = store_with_inventory(&product, StockQuantity::new(0), SpaceUnits::new(100))?;
        let mut workflow = workflow(store, agent_with_proposals(Vec::new()))?;

        let result = workflow.advance_simulation(AdvanceSimulation { days: 1 })?;

        assert_eq!(result.lost_units.units(), 2);
        let sale =
            workflow.store().sales.first().ok_or_else(|| {
                ApplicationError::store_failure("test sales lookup", "missing sale")
            })?;
        assert_eq!(sale.lost_units().units(), 2);
        Ok(())
    }

    #[test]
    fn advance_simulation_surfaces_transaction_failure() -> Result<(), ApplicationError> {
        let product = product("shirt-1", "1.000", 1)?;
        let mut store =
            store_with_inventory(&product, StockQuantity::new(5), SpaceUnits::new(100))?;
        store.inject_failure(FakeStoreFailure::RecordSalesDay);
        let mut workflow = workflow(store, agent_with_proposals(Vec::new()))?;

        let result = workflow.advance_simulation(AdvanceSimulation { days: 1 });

        assert!(matches!(result, Err(ApplicationError::StoreFailure { .. })));
        assert_eq!(workflow.store().sales.len(), 0);
        Ok(())
    }

    #[test]
    fn get_snapshot_surfaces_invalid_persisted_data() -> Result<(), ApplicationError> {
        let product = product("shirt-1", "1.000", 1)?;
        let mut store =
            store_with_inventory(&product, StockQuantity::new(5), SpaceUnits::new(100))?;
        store.inject_failure(FakeStoreFailure::LoadSnapshot);
        let workflow = workflow(store, agent_with_proposals(Vec::new()))?;

        let result = workflow.get_snapshot();

        assert!(matches!(result, Err(ApplicationError::StoreFailure { .. })));
        Ok(())
    }

    #[tokio::test]
    async fn run_decision_persists_accepted_agent_proposals() -> Result<(), ApplicationError> {
        let product = product("shirt-1", "1.000", 1)?;
        let store = store_with_inventory(&product, StockQuantity::new(0), SpaceUnits::new(100))?;
        let mut workflow = workflow(
            store,
            agent_with_proposals(vec![ProposedRestockOrder {
                sku: product.sku().clone(),
                quantity: StockQuantity::new(5),
                rationale: "profitable".to_owned(),
            }]),
        )?;

        let result = workflow.run_restock_decision(decision_command()?).await?;

        assert_eq!(result.accepted_orders.len(), 1);
        assert_eq!(workflow.store().restocks.len(), 1);
        let decision_run_id = result.decision_run_id;
        let restock = workflow.store().restocks.first().ok_or_else(|| {
            ApplicationError::store_failure("test restock lookup", "missing restock")
        })?;
        assert_eq!(restock.ordered_at(), clock_date()?);
        let decision_run =
            workflow
                .decision_runs()
                .runs
                .first()
                .ok_or(ApplicationError::DecisionRunNotFound {
                    run_id: decision_run_id,
                })?;
        assert_eq!(decision_run.decision_date(), clock_date()?);
        assert_eq!(decision_run.status(), DecisionRunStatus::Completed);
        Ok(())
    }

    #[tokio::test]
    async fn run_decision_rejects_capacity_overflow_proposal() -> Result<(), ApplicationError> {
        let product = product("shirt-1", "1.000", 10)?;
        let store = store_with_inventory(&product, StockQuantity::new(9), SpaceUnits::new(100))?;
        let mut workflow = workflow(
            store,
            agent_with_proposals(vec![ProposedRestockOrder {
                sku: product.sku().clone(),
                quantity: StockQuantity::new(2),
                rationale: "too large".to_owned(),
            }]),
        )?;

        let result = workflow.run_restock_decision(decision_command()?).await?;

        assert_eq!(result.accepted_orders.len(), 0);
        assert_eq!(result.rejected_proposals.len(), 1);
        let rejected = result
            .rejected_proposals
            .first()
            .ok_or_else(|| ApplicationError::store_failure("test rejection lookup", "missing"))?;
        assert_eq!(
            rejected.reason,
            "proposal for SKU SHIRT-1 exceeds available capacity: requires 20, available 10, overflow 10"
        );
        assert_eq!(workflow.store().restocks.len(), 0);
        Ok(())
    }

    #[tokio::test]
    async fn run_decision_rejects_inactive_product_proposal() -> Result<(), ApplicationError> {
        let product = inactive_product("shirt-1")?;
        let store = store_with_inventory(&product, StockQuantity::new(0), SpaceUnits::new(100))?;
        let mut workflow = workflow(
            store,
            agent_with_proposals(vec![ProposedRestockOrder {
                sku: product.sku().clone(),
                quantity: StockQuantity::new(2),
                rationale: "inactive".to_owned(),
            }]),
        )?;

        let result = workflow.run_restock_decision(decision_command()?).await?;

        assert_eq!(result.accepted_orders.len(), 0);
        assert_eq!(result.rejected_proposals.len(), 1);
        assert!(
            result
                .rejected_proposals
                .first()
                .is_some_and(|proposal| proposal.reason.contains("inactive"))
        );
        Ok(())
    }

    #[tokio::test]
    async fn run_decision_marks_run_failed_on_restock_transaction_failure()
    -> Result<(), ApplicationError> {
        let product = product("shirt-1", "1.000", 1)?;
        let mut store =
            store_with_inventory(&product, StockQuantity::new(0), SpaceUnits::new(100))?;
        store.inject_failure(FakeStoreFailure::PlaceRestockOrders);
        let mut workflow = workflow(
            store,
            agent_with_proposals(vec![ProposedRestockOrder {
                sku: product.sku().clone(),
                quantity: StockQuantity::new(5),
                rationale: "profitable".to_owned(),
            }]),
        )?;

        let result = workflow.run_restock_decision(decision_command()?).await;

        assert!(matches!(result, Err(ApplicationError::StoreFailure { .. })));
        assert_eq!(
            workflow
                .decision_runs()
                .runs
                .first()
                .ok_or(ApplicationError::DecisionRunNotFound {
                    run_id: DecisionRunId::new("decision-1")?,
                })?
                .status(),
            DecisionRunStatus::Failed
        );
        Ok(())
    }

    #[tokio::test]
    async fn run_decision_marks_run_failed_on_agent_error() -> Result<(), ApplicationError> {
        let product = product("shirt-1", "1.000", 1)?;
        let store = store_with_inventory(&product, StockQuantity::new(0), SpaceUnits::new(100))?;
        let mut workflow = workflow(
            store,
            FakeAgent {
                response: Err(ApplicationError::agent_failure_message("model unavailable")),
            },
        )?;

        let result = workflow.run_restock_decision(decision_command()?).await;

        assert!(matches!(result, Err(ApplicationError::AgentFailure { .. })));
        assert_eq!(
            workflow
                .decision_runs()
                .runs
                .first()
                .ok_or(ApplicationError::DecisionRunNotFound {
                    run_id: DecisionRunId::new("decision-1")?,
                })?
                .status(),
            DecisionRunStatus::Failed
        );
        Ok(())
    }

    #[tokio::test]
    async fn run_cycle_decides_on_day_zero_and_each_interval() -> Result<(), ApplicationError> {
        let product = product("shirt-1", "0.000", 1)?;
        let store = store_with_inventory(&product, StockQuantity::new(0), SpaceUnits::new(100))?;
        let mut workflow = workflow(store, agent_with_proposals(Vec::new()))?;

        let result = workflow
            .run_workflow_cycle(RunWorkflowCycle {
                total_days: 3,
                decision_interval_days: 2,
                horizon: DecisionHorizonDays::new(14)?,
                max_restock_orders: StockQuantity::new(3),
            })
            .await?;

        assert_eq!(result.decision_runs.count(), 2);
        assert_eq!(workflow.decision_runs().runs.len(), 2);
        Ok(())
    }

    #[test]
    fn ranked_options_prefer_profit_per_occupied_space() -> Result<(), ApplicationError> {
        let bulky = product("bulky-1", "1.000", 10)?;
        let compact = product("compact-1", "0.500", 1)?;
        let snapshot = RetailSnapshot {
            current_date: test_date()?,
            capacity: SpaceUnits::new(100),
            products: vec![bulky.clone(), compact.clone()],
            inventory: vec![
                InventoryPosition::new(
                    bulky.sku().clone(),
                    StockQuantity::new(0),
                    DemandBacklog::ZERO,
                ),
                InventoryPosition::new(
                    compact.sku().clone(),
                    StockQuantity::new(0),
                    DemandBacklog::ZERO,
                ),
            ],
            open_restocks: Vec::new(),
            recent_sales: Vec::new(),
            profit_summary: ProfitSummary::zero(),
        };

        let options = ranked_options(&snapshot, DecisionHorizonDays::new(14)?)?;
        let first = options.first().ok_or_else(|| {
            ApplicationError::store_failure("test ranked option lookup", "missing ranked option")
        })?;

        assert_eq!(first.sku(), compact.sku());
        Ok(())
    }

    #[test]
    fn ranked_options_use_global_available_capacity() -> Result<(), ApplicationError> {
        let stocked = product("stocked-1", "1.000", 10)?;
        let candidate = product("candidate-1", "50.000", 4)?;
        let snapshot = RetailSnapshot {
            current_date: test_date()?,
            capacity: SpaceUnits::new(240),
            products: vec![stocked, candidate.clone()],
            inventory: vec![
                InventoryPosition::new(
                    Sku::new("stocked-1")?,
                    StockQuantity::new(19),
                    DemandBacklog::ZERO,
                ),
                InventoryPosition::new(
                    candidate.sku().clone(),
                    StockQuantity::new(0),
                    DemandBacklog::ZERO,
                ),
            ],
            open_restocks: Vec::new(),
            recent_sales: Vec::new(),
            profit_summary: ProfitSummary::zero(),
        };

        let options = ranked_options(&snapshot, DecisionHorizonDays::new(14)?)?;
        let option = options
            .iter()
            .find(|option| option.sku() == candidate.sku())
            .ok_or_else(|| {
                ApplicationError::store_failure("test ranked option lookup", "missing candidate")
            })?;

        assert_eq!(option.quantity(), StockQuantity::new(12));
        assert_eq!(option.occupied_space(), SpaceUnits::new(48));
        Ok(())
    }
}
