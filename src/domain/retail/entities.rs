//! Retail entities and aggregates.

use super::{
    ApparelKind, Brand, DecisionHorizonDays, DecisionRunId, DomainError, LeadTimeDays, MoneyCents,
    RestockOrderId, SalesOrderId, SimulationDate, SizeLabel, Sku, SpaceUnits, StockQuantity,
};

/// Product construction details.
#[derive(Debug, Clone)]
pub struct ProductDetails {
    /// Product SKU.
    pub sku: Sku,
    /// Apparel category.
    pub kind: ApparelKind,
    /// Product brand.
    pub brand: Brand,
    /// Product size.
    pub size: SizeLabel,
    /// Supplier unit cost.
    pub unit_cost: MoneyCents,
    /// Customer unit price.
    pub unit_price: MoneyCents,
    /// Capacity consumed by one unit.
    pub unit_space: SpaceUnits,
    /// Deterministic daily demand rate.
    pub demand_rate: super::DemandRatePerDay,
    /// Supplier lead time.
    pub lead_time: LeadTimeDays,
    /// Minimum restock order quantity.
    pub min_order_quantity: StockQuantity,
    /// Maximum restock order quantity.
    pub max_order_quantity: StockQuantity,
    /// Whether the product can currently be restocked.
    pub active: bool,
}

/// Sellable retail product.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Product {
    sku: Sku,
    kind: ApparelKind,
    brand: Brand,
    size: SizeLabel,
    unit_cost: MoneyCents,
    unit_price: MoneyCents,
    unit_space: SpaceUnits,
    demand_rate: super::DemandRatePerDay,
    lead_time: LeadTimeDays,
    min_order_quantity: StockQuantity,
    max_order_quantity: StockQuantity,
    active: bool,
}

impl Product {
    /// Create a product from validated details.
    ///
    /// # Errors
    ///
    /// Returns an error when price is below cost or order bounds are invalid.
    pub fn from_details(details: ProductDetails) -> Result<Self, DomainError> {
        if details.unit_price < details.unit_cost {
            return Err(DomainError::NegativeMargin {
                unit_cost: details.unit_cost,
                unit_price: details.unit_price,
            });
        }

        if details.min_order_quantity.is_zero() {
            return Err(DomainError::NonPositive {
                field: "min_order_quantity",
            });
        }

        if details.max_order_quantity < details.min_order_quantity {
            return Err(DomainError::RestockQuantityOutOfBounds {
                sku: details.sku,
                requested: details.max_order_quantity,
                minimum: details.min_order_quantity,
                maximum: details.max_order_quantity,
            });
        }

        Ok(Self {
            sku: details.sku,
            kind: details.kind,
            brand: details.brand,
            size: details.size,
            unit_cost: details.unit_cost,
            unit_price: details.unit_price,
            unit_space: details.unit_space,
            demand_rate: details.demand_rate,
            lead_time: details.lead_time,
            min_order_quantity: details.min_order_quantity,
            max_order_quantity: details.max_order_quantity,
            active: details.active,
        })
    }

    /// Return the product SKU.
    #[must_use]
    pub const fn sku(&self) -> &Sku {
        &self.sku
    }

    /// Return the apparel category.
    #[must_use]
    pub const fn kind(&self) -> ApparelKind {
        self.kind
    }

    /// Return the product brand.
    #[must_use]
    pub const fn brand(&self) -> &Brand {
        &self.brand
    }

    /// Return the product size.
    #[must_use]
    pub const fn size(&self) -> SizeLabel {
        self.size
    }

    /// Return supplier unit cost.
    #[must_use]
    pub const fn unit_cost(&self) -> MoneyCents {
        self.unit_cost
    }

    /// Return customer unit price.
    #[must_use]
    pub const fn unit_price(&self) -> MoneyCents {
        self.unit_price
    }

    /// Return capacity consumed by one unit.
    #[must_use]
    pub const fn unit_space(&self) -> SpaceUnits {
        self.unit_space
    }

    /// Return the deterministic demand rate.
    #[must_use]
    pub const fn demand_rate(&self) -> super::DemandRatePerDay {
        self.demand_rate
    }

    /// Return supplier lead time.
    #[must_use]
    pub const fn lead_time(&self) -> LeadTimeDays {
        self.lead_time
    }

    /// Return minimum restock order quantity.
    #[must_use]
    pub const fn min_order_quantity(&self) -> StockQuantity {
        self.min_order_quantity
    }

    /// Return maximum restock order quantity.
    #[must_use]
    pub const fn max_order_quantity(&self) -> StockQuantity {
        self.max_order_quantity
    }

    /// Return whether the product is active.
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }

    /// Return unit gross margin.
    ///
    /// # Errors
    ///
    /// Returns an error if persisted product data violates price invariants.
    pub fn unit_margin(&self) -> Result<MoneyCents, DomainError> {
        self.unit_price.checked_sub(self.unit_cost)
    }

    /// Calculate ETA for a restock ordered on `current_date`.
    ///
    /// # Errors
    ///
    /// Returns an error when date arithmetic overflows.
    pub fn restock_eta(&self, current_date: SimulationDate) -> Result<SimulationDate, DomainError> {
        current_date.checked_add_days(self.lead_time.days())
    }

    /// Validate a requested restock quantity against product order bounds.
    ///
    /// # Errors
    ///
    /// Returns an error when the requested quantity is below the minimum or above the maximum.
    pub fn bounded_order_quantity(
        &self,
        requested_quantity: StockQuantity,
    ) -> Result<StockQuantity, DomainError> {
        if requested_quantity < self.min_order_quantity
            || requested_quantity > self.max_order_quantity
        {
            return Err(DomainError::RestockQuantityOutOfBounds {
                sku: self.sku.clone(),
                requested: requested_quantity,
                minimum: self.min_order_quantity,
                maximum: self.max_order_quantity,
            });
        }

        Ok(requested_quantity)
    }
}

/// Current stock and demand carryover for one SKU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InventoryPosition {
    sku: Sku,
    on_hand: StockQuantity,
    demand_backlog: super::DemandBacklog,
}

impl InventoryPosition {
    /// Create an inventory position.
    #[must_use]
    pub const fn new(
        sku: Sku,
        on_hand: StockQuantity,
        demand_backlog: super::DemandBacklog,
    ) -> Self {
        Self {
            sku,
            on_hand,
            demand_backlog,
        }
    }

    /// Return the inventory SKU.
    #[must_use]
    pub const fn sku(&self) -> &Sku {
        &self.sku
    }

    /// Return on-hand stock.
    #[must_use]
    pub const fn on_hand(&self) -> StockQuantity {
        self.on_hand
    }

    /// Return carried demand backlog.
    #[must_use]
    pub const fn demand_backlog(&self) -> super::DemandBacklog {
        self.demand_backlog
    }

    /// Replace carried demand backlog.
    pub const fn set_demand_backlog(&mut self, demand_backlog: super::DemandBacklog) {
        self.demand_backlog = demand_backlog;
    }

    /// Receive a supplier restock.
    ///
    /// # Errors
    ///
    /// Returns an error on arithmetic overflow.
    pub fn receive_restock(&mut self, quantity: StockQuantity) -> Result<(), DomainError> {
        self.on_hand = self.on_hand.checked_add(quantity)?;
        Ok(())
    }

    /// Receive a supplier restock while enforcing total capacity.
    ///
    /// # Errors
    ///
    /// Returns an error when receiving the restock would exceed capacity.
    pub fn receive_restock_with_capacity(
        &mut self,
        product: &Product,
        quantity: StockQuantity,
        capacity: SpaceUnits,
    ) -> Result<(), DomainError> {
        let next_on_hand = self.on_hand.checked_add(quantity)?;
        let requested = product.unit_space().checked_mul_quantity(next_on_hand)?;
        if requested > capacity {
            return Err(DomainError::CapacityExceeded {
                requested,
                capacity,
            });
        }

        self.on_hand = next_on_hand;
        Ok(())
    }

    /// Fulfill demand from on-hand inventory and return fulfilled units.
    ///
    /// # Errors
    ///
    /// Returns an error on arithmetic overflow.
    pub fn fulfill_demand(
        &mut self,
        requested_units: StockQuantity,
    ) -> Result<StockQuantity, DomainError> {
        let fulfilled = if requested_units > self.on_hand {
            self.on_hand
        } else {
            requested_units
        };
        self.on_hand = self.on_hand.checked_sub(fulfilled)?;
        Ok(fulfilled)
    }

    /// Return occupied capacity for this SKU and product.
    ///
    /// # Errors
    ///
    /// Returns an error on arithmetic overflow.
    pub fn occupied_space(&self, product: &Product) -> Result<SpaceUnits, DomainError> {
        product.unit_space().checked_mul_quantity(self.on_hand)
    }
}

/// Supplier restock status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RestockOrderStatus {
    /// Open and not yet received.
    Open,
    /// Received into inventory.
    Received,
    /// Cancelled before receipt.
    Cancelled,
}

/// Restock-order construction details.
#[derive(Debug, Clone)]
pub struct RestockOrderDetails {
    /// Restock order identifier.
    pub id: RestockOrderId,
    /// Product SKU.
    pub sku: Sku,
    /// Ordered quantity.
    pub quantity: StockQuantity,
    /// Order date.
    pub ordered_at: SimulationDate,
    /// Estimated arrival date.
    pub eta: SimulationDate,
    /// Decision run that created the order.
    pub decision_run_id: DecisionRunId,
    /// Human-readable rationale.
    pub rationale: String,
}

/// Supplier restock order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestockOrder {
    id: RestockOrderId,
    sku: Sku,
    quantity: StockQuantity,
    ordered_at: SimulationDate,
    eta: SimulationDate,
    status: RestockOrderStatus,
    decision_run_id: DecisionRunId,
    rationale: String,
}

impl RestockOrder {
    /// Create an open restock order.
    ///
    /// # Errors
    ///
    /// Returns an error when the quantity is zero or rationale is empty.
    pub fn open(details: RestockOrderDetails) -> Result<Self, DomainError> {
        if details.quantity.is_zero() {
            return Err(DomainError::NonPositive {
                field: "restock_quantity",
            });
        }

        let rationale = details.rationale.trim().to_owned();
        if rationale.is_empty() {
            return Err(DomainError::EmptyText { field: "rationale" });
        }

        Ok(Self {
            id: details.id,
            sku: details.sku,
            quantity: details.quantity,
            ordered_at: details.ordered_at,
            eta: details.eta,
            status: RestockOrderStatus::Open,
            decision_run_id: details.decision_run_id,
            rationale,
        })
    }

    /// Return the restock order identifier.
    #[must_use]
    pub const fn id(&self) -> &RestockOrderId {
        &self.id
    }

    /// Return the restock SKU.
    #[must_use]
    pub const fn sku(&self) -> &Sku {
        &self.sku
    }

    /// Return ordered quantity.
    #[must_use]
    pub const fn quantity(&self) -> StockQuantity {
        self.quantity
    }

    /// Return order date.
    #[must_use]
    pub const fn ordered_at(&self) -> SimulationDate {
        self.ordered_at
    }

    /// Return ETA.
    #[must_use]
    pub const fn eta(&self) -> SimulationDate {
        self.eta
    }

    /// Return current status.
    #[must_use]
    pub const fn status(&self) -> RestockOrderStatus {
        self.status
    }

    /// Return creating decision run.
    #[must_use]
    pub const fn decision_run_id(&self) -> &DecisionRunId {
        &self.decision_run_id
    }

    /// Return restock rationale.
    #[must_use]
    pub fn rationale(&self) -> &str {
        &self.rationale
    }

    /// Mark an open order as received.
    ///
    /// # Errors
    ///
    /// Returns an error when the order is not open or the date is before ETA.
    pub fn receive(&mut self, on_date: SimulationDate) -> Result<(), DomainError> {
        if self.status != RestockOrderStatus::Open {
            return Err(DomainError::InvalidRestockTransition {
                order_id: self.id.clone(),
                status: self.status,
            });
        }

        if on_date < self.eta {
            return Err(DomainError::RestockReceivedBeforeEta {
                order_id: self.id.clone(),
                received_on: on_date,
                eta: self.eta,
            });
        }

        self.status = RestockOrderStatus::Received;
        Ok(())
    }

    /// Cancel an open order.
    ///
    /// # Errors
    ///
    /// Returns an error when the order is not open.
    pub fn cancel(&mut self) -> Result<(), DomainError> {
        if self.status != RestockOrderStatus::Open {
            return Err(DomainError::InvalidRestockTransition {
                order_id: self.id.clone(),
                status: self.status,
            });
        }

        self.status = RestockOrderStatus::Cancelled;
        Ok(())
    }
}

/// Sales-order construction details.
#[derive(Debug, Clone)]
pub struct SalesOrderDetails {
    /// Sales order identifier.
    pub id: SalesOrderId,
    /// Sale date.
    pub sale_date: SimulationDate,
    /// Product SKU.
    pub sku: Sku,
    /// Requested customer quantity.
    pub requested: StockQuantity,
    /// Fulfilled customer quantity.
    pub fulfilled: StockQuantity,
    /// Product unit price.
    pub unit_price: MoneyCents,
    /// Product unit cost.
    pub unit_cost: MoneyCents,
}

/// Simulated customer sales order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesOrder {
    id: SalesOrderId,
    sale_date: SimulationDate,
    sku: Sku,
    requested: StockQuantity,
    fulfilled: StockQuantity,
    revenue: MoneyCents,
    cost: MoneyCents,
    lost_units: StockQuantity,
}

impl SalesOrder {
    /// Record a simulated sale.
    ///
    /// # Errors
    ///
    /// Returns an error when fulfilled units exceed requested units or arithmetic overflows.
    pub fn record(details: SalesOrderDetails) -> Result<Self, DomainError> {
        if details.fulfilled > details.requested {
            return Err(DomainError::FulfilledExceedsRequested {
                requested: details.requested,
                fulfilled: details.fulfilled,
            });
        }

        let revenue = details.unit_price.checked_mul_quantity(details.fulfilled)?;
        let cost = details.unit_cost.checked_mul_quantity(details.fulfilled)?;
        let lost_units = details.requested.checked_sub(details.fulfilled)?;

        Ok(Self {
            id: details.id,
            sale_date: details.sale_date,
            sku: details.sku,
            requested: details.requested,
            fulfilled: details.fulfilled,
            revenue,
            cost,
            lost_units,
        })
    }

    /// Return sales order identifier.
    #[must_use]
    pub const fn id(&self) -> &SalesOrderId {
        &self.id
    }

    /// Return sale date.
    #[must_use]
    pub const fn sale_date(&self) -> SimulationDate {
        self.sale_date
    }

    /// Return sales SKU.
    #[must_use]
    pub const fn sku(&self) -> &Sku {
        &self.sku
    }

    /// Return requested units.
    #[must_use]
    pub const fn requested(&self) -> StockQuantity {
        self.requested
    }

    /// Return fulfilled units.
    #[must_use]
    pub const fn fulfilled(&self) -> StockQuantity {
        self.fulfilled
    }

    /// Return revenue.
    #[must_use]
    pub const fn revenue(&self) -> MoneyCents {
        self.revenue
    }

    /// Return cost.
    #[must_use]
    pub const fn cost(&self) -> MoneyCents {
        self.cost
    }

    /// Return gross profit.
    ///
    /// # Errors
    ///
    /// Returns an error if persisted sales data violates revenue/cost invariants.
    pub fn gross_profit(&self) -> Result<MoneyCents, DomainError> {
        self.revenue.checked_sub(self.cost)
    }

    /// Return lost units.
    #[must_use]
    pub const fn lost_units(&self) -> StockQuantity {
        self.lost_units
    }
}

/// Decision run status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecisionRunStatus {
    /// Decision started.
    Started,
    /// Decision completed.
    Completed,
    /// Decision failed.
    Failed,
}

/// Decision-run construction details.
#[derive(Debug, Clone)]
pub struct DecisionRunDetails {
    /// Decision run identifier.
    pub id: DecisionRunId,
    /// Decision date.
    pub decision_date: SimulationDate,
    /// Decision horizon.
    pub horizon: DecisionHorizonDays,
}

/// Restock decision run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionRun {
    id: DecisionRunId,
    decision_date: SimulationDate,
    horizon: DecisionHorizonDays,
    status: DecisionRunStatus,
    summary: Option<String>,
    created_restock_count: StockQuantity,
}

impl DecisionRun {
    /// Start a decision run.
    #[must_use]
    pub fn start(details: DecisionRunDetails) -> Self {
        Self {
            id: details.id,
            decision_date: details.decision_date,
            horizon: details.horizon,
            status: DecisionRunStatus::Started,
            summary: None,
            created_restock_count: StockQuantity::new(0),
        }
    }

    /// Return decision run identifier.
    #[must_use]
    pub const fn id(&self) -> &DecisionRunId {
        &self.id
    }

    /// Return decision date.
    #[must_use]
    pub const fn decision_date(&self) -> SimulationDate {
        self.decision_date
    }

    /// Return decision horizon.
    #[must_use]
    pub const fn horizon(&self) -> DecisionHorizonDays {
        self.horizon
    }

    /// Return current decision status.
    #[must_use]
    pub const fn status(&self) -> DecisionRunStatus {
        self.status
    }

    /// Return decision summary.
    #[must_use]
    pub fn summary(&self) -> Option<&str> {
        self.summary.as_deref()
    }

    /// Return created restock count.
    #[must_use]
    pub const fn created_restock_count(&self) -> StockQuantity {
        self.created_restock_count
    }

    /// Complete the decision run.
    ///
    /// # Errors
    ///
    /// Returns an error when the run is not started.
    pub fn complete(
        &mut self,
        summary: String,
        created_restock_count: StockQuantity,
    ) -> Result<(), DomainError> {
        if self.status != DecisionRunStatus::Started {
            return Err(DomainError::InvalidDecisionRunTransition {
                run_id: self.id.clone(),
                status: self.status,
            });
        }

        self.summary = Some(summary);
        self.created_restock_count = created_restock_count;
        self.status = DecisionRunStatus::Completed;
        Ok(())
    }

    /// Mark the decision run as failed.
    ///
    /// # Errors
    ///
    /// Returns an error when the run is not started.
    pub fn fail(&mut self, summary: String) -> Result<(), DomainError> {
        if self.status != DecisionRunStatus::Started {
            return Err(DomainError::InvalidDecisionRunTransition {
                run_id: self.id.clone(),
                status: self.status,
            });
        }

        self.summary = Some(summary);
        self.status = DecisionRunStatus::Failed;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::{
        ApparelKind, Brand, DecisionHorizonDays, DecisionRun, DecisionRunDetails, DecisionRunId,
        InventoryPosition, LeadTimeDays, MoneyCents, Product, ProductDetails, RestockOrder,
        RestockOrderDetails, RestockOrderId, RestockOrderStatus, SalesOrder, SalesOrderDetails,
        SalesOrderId, SimulationDate, SizeLabel, Sku, SpaceUnits, StockQuantity,
    };
    use crate::domain::retail::{DemandBacklog, DemandRatePerDay};

    fn simulation_date() -> Result<SimulationDate, Box<dyn std::error::Error>> {
        let date = NaiveDate::from_ymd_opt(2026, 6, 9).ok_or("valid test date")?;
        Ok(SimulationDate::new(date))
    }

    fn product() -> Result<Product, Box<dyn std::error::Error>> {
        Ok(Product::from_details(ProductDetails {
            sku: Sku::new("shirt-1")?,
            kind: ApparelKind::Shirt,
            brand: Brand::new("North")?,
            size: SizeLabel::M,
            unit_cost: MoneyCents::new(1_000),
            unit_price: MoneyCents::new(2_500),
            unit_space: SpaceUnits::new(2),
            demand_rate: "1.250".parse::<DemandRatePerDay>()?,
            lead_time: LeadTimeDays::new(3)?,
            min_order_quantity: StockQuantity::new(5),
            max_order_quantity: StockQuantity::new(20),
            active: true,
        })?)
    }

    fn restock_order() -> Result<RestockOrder, Box<dyn std::error::Error>> {
        let product = product()?;
        let ordered_at = simulation_date()?;
        let eta = product.restock_eta(ordered_at)?;

        Ok(RestockOrder::open(RestockOrderDetails {
            id: RestockOrderId::new("restock-1")?,
            sku: product.sku().clone(),
            quantity: StockQuantity::new(10),
            ordered_at,
            eta,
            decision_run_id: DecisionRunId::new("decision-1")?,
            rationale: "profitable restock".to_owned(),
        })?)
    }

    #[test]
    fn rejects_restock_quantity_outside_product_bounds() -> Result<(), Box<dyn std::error::Error>> {
        let product = product()?;

        assert!(
            product
                .bounded_order_quantity(StockQuantity::new(4))
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn receives_open_restock_order() -> Result<(), Box<dyn std::error::Error>> {
        let mut order = restock_order()?;

        order.receive(order.eta())?;

        assert_eq!(order.status(), RestockOrderStatus::Received);
        Ok(())
    }

    #[test]
    fn rejects_receiving_cancelled_restock_order() -> Result<(), Box<dyn std::error::Error>> {
        let mut order = restock_order()?;

        order.cancel()?;

        assert!(order.receive(order.eta()).is_err());
        Ok(())
    }

    #[test]
    fn prevents_inventory_from_exceeding_capacity() -> Result<(), Box<dyn std::error::Error>> {
        let product = product()?;
        let mut inventory = InventoryPosition::new(
            product.sku().clone(),
            StockQuantity::new(3),
            DemandBacklog::ZERO,
        );

        let result = inventory.receive_restock_with_capacity(
            &product,
            StockQuantity::new(3),
            SpaceUnits::new(10),
        );

        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn computes_profit_with_checked_cents_arithmetic() -> Result<(), Box<dyn std::error::Error>> {
        let product = product()?;
        let sale = SalesOrder::record(SalesOrderDetails {
            id: SalesOrderId::new("sale-1")?,
            sale_date: simulation_date()?,
            sku: product.sku().clone(),
            requested: StockQuantity::new(3),
            fulfilled: StockQuantity::new(2),
            unit_price: product.unit_price(),
            unit_cost: product.unit_cost(),
        })?;

        assert_eq!(sale.gross_profit()?.cents(), 3_000);
        assert_eq!(sale.lost_units().units(), 1);
        Ok(())
    }

    #[test]
    fn completes_started_decision_run() -> Result<(), Box<dyn std::error::Error>> {
        let mut run = DecisionRun::start(DecisionRunDetails {
            id: DecisionRunId::new("decision-1")?,
            decision_date: simulation_date()?,
            horizon: DecisionHorizonDays::new(14)?,
        });

        run.complete("created orders".to_owned(), StockQuantity::new(1))?;

        assert_eq!(run.status(), super::DecisionRunStatus::Completed);
        Ok(())
    }
}
