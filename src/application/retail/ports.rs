//! Ports used by retail application use cases.

use std::future::Future;
use std::path::Path;

use crate::domain::retail::{
    DecisionRun, DecisionRunId, RestockOption, RestockOrder, RestockOrderId, SalesOrder,
    SalesOrderId, SimulationDate, Sku, StockQuantity,
};

use super::{ApplicationError, ProfitSummary, RetailSnapshot};

/// Durable retail state store.
pub trait RetailStore {
    /// Return whether durable state already exists.
    ///
    /// # Errors
    ///
    /// Returns an error when the adapter cannot inspect state.
    fn state_exists(&self) -> Result<bool, ApplicationError>;

    /// Load the current retail snapshot.
    ///
    /// # Errors
    ///
    /// Returns an error when the adapter cannot load state.
    fn load_snapshot(&self) -> Result<RetailSnapshot, ApplicationError>;

    /// Seed durable state from a scenario path.
    ///
    /// # Errors
    ///
    /// Returns an error when scenario loading or persistence fails.
    fn seed_scenario(&mut self, scenario_path: &Path, reset: bool) -> Result<(), ApplicationError>;

    /// Receive restocks due on or before the given date.
    ///
    /// # Errors
    ///
    /// Returns an error when receiving or persistence fails.
    fn receive_due_restocks(
        &mut self,
        on_date: SimulationDate,
    ) -> Result<Vec<RestockOrder>, ApplicationError>;

    /// Record sales and updated inventory for one simulated day.
    ///
    /// # Errors
    ///
    /// Returns an error when persistence fails.
    fn record_sales_day(
        &mut self,
        sales_orders: Vec<SalesOrder>,
        inventory: Vec<crate::domain::retail::InventoryPosition>,
    ) -> Result<(), ApplicationError>;

    /// Place accepted restock orders.
    ///
    /// # Errors
    ///
    /// Returns an error when persistence fails.
    fn place_restock_orders(&mut self, orders: Vec<RestockOrder>) -> Result<(), ApplicationError>;

    /// Return current open restock orders.
    ///
    /// # Errors
    ///
    /// Returns an error when persistence fails.
    fn open_restock_orders(&self) -> Result<Vec<RestockOrder>, ApplicationError>;

    /// Return aggregate profit summary.
    ///
    /// # Errors
    ///
    /// Returns an error when persistence fails.
    fn profit_summary(&self) -> Result<ProfitSummary, ApplicationError>;

    /// Advance the logical shop date.
    ///
    /// # Errors
    ///
    /// Returns an error when persistence fails.
    fn advance_shop_date(&mut self, next_date: SimulationDate) -> Result<(), ApplicationError>;
}

/// Decision-run persistence store.
pub trait DecisionRunStore {
    /// Persist a started decision run.
    ///
    /// # Errors
    ///
    /// Returns an error when persistence fails.
    fn start_decision_run(&mut self, run: DecisionRun) -> Result<(), ApplicationError>;

    /// Mark a decision run completed.
    ///
    /// # Errors
    ///
    /// Returns an error when persistence fails.
    fn complete_decision_run(
        &mut self,
        run_id: &DecisionRunId,
        summary: String,
        created_restock_count: StockQuantity,
    ) -> Result<(), ApplicationError>;

    /// Mark a decision run failed.
    ///
    /// # Errors
    ///
    /// Returns an error when persistence fails.
    fn fail_decision_run(
        &mut self,
        run_id: &DecisionRunId,
        summary: String,
    ) -> Result<(), ApplicationError>;

    /// Load a decision run.
    ///
    /// # Errors
    ///
    /// Returns an error when the run does not exist or cannot be loaded.
    fn decision_run(&self, run_id: &DecisionRunId) -> Result<DecisionRun, ApplicationError>;
}

/// Request sent to a replenishment decision agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionAgentRequest {
    /// Snapshot available to the decision agent.
    pub snapshot: RetailSnapshot,
    /// Ranked deterministic restock options.
    pub ranked_options: Vec<RestockOption>,
    /// Open supplier restock orders.
    pub open_orders: Vec<RestockOrder>,
    /// Maximum orders the agent may propose.
    pub max_orders: StockQuantity,
}

/// Agent restock proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProposedRestockOrder {
    /// Proposed SKU.
    pub sku: Sku,
    /// Proposed order quantity.
    pub quantity: StockQuantity,
    /// Agent rationale.
    pub rationale: String,
}

/// Response returned by a replenishment decision agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionAgentResponse {
    /// Proposed restock orders.
    pub proposed_orders: Vec<ProposedRestockOrder>,
    /// Agent summary.
    pub summary: String,
}

/// Restock decision agent port.
pub trait ReplenishmentDecisionAgent {
    /// Decide which restock orders to propose.
    fn decide(
        &self,
        request: DecisionAgentRequest,
    ) -> impl Future<Output = Result<DecisionAgentResponse, ApplicationError>>;
}

/// Date supplier for application behavior that needs real time.
pub trait Clock {
    /// Return today's date.
    ///
    /// # Errors
    ///
    /// Returns an error when the adapter cannot provide a date.
    fn today(&self) -> Result<SimulationDate, ApplicationError>;
}

/// Identifier generator for application-created entities.
pub trait IdGenerator {
    /// Generate a sales order ID.
    ///
    /// # Errors
    ///
    /// Returns an error when ID generation fails.
    fn sales_order_id(&mut self) -> Result<SalesOrderId, ApplicationError>;

    /// Generate a restock order ID.
    ///
    /// # Errors
    ///
    /// Returns an error when ID generation fails.
    fn restock_order_id(&mut self) -> Result<RestockOrderId, ApplicationError>;

    /// Generate a decision run ID.
    ///
    /// # Errors
    ///
    /// Returns an error when ID generation fails.
    fn decision_run_id(&mut self) -> Result<DecisionRunId, ApplicationError>;
}
