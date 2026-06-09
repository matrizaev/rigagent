//! Retail application read models.

use crate::domain::retail::{
    MoneyCents, Product, RestockOrder, SalesOrder, SimulationDate, StockQuantity,
};

/// Query for the current retail snapshot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GetRetailSnapshot;

/// Current retail state used by application use cases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetailSnapshot {
    /// Current logical shop date.
    pub current_date: SimulationDate,
    /// Total stock-space capacity.
    pub capacity: crate::domain::retail::SpaceUnits,
    /// Products in the shop catalog.
    pub products: Vec<Product>,
    /// Inventory positions by SKU.
    pub inventory: Vec<crate::domain::retail::InventoryPosition>,
    /// Open supplier restock orders.
    pub open_restocks: Vec<RestockOrder>,
    /// Recent simulated sales orders.
    pub recent_sales: Vec<SalesOrder>,
    /// Profit summary.
    pub profit_summary: ProfitSummary,
}

/// Aggregate profit summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfitSummary {
    /// Fulfilled revenue.
    pub revenue: MoneyCents,
    /// Fulfilled unit cost.
    pub cost: MoneyCents,
    /// Gross profit.
    pub gross_profit: MoneyCents,
    /// Units lost to stockouts.
    pub lost_units: StockQuantity,
}

impl ProfitSummary {
    /// Empty profit summary.
    #[must_use]
    pub const fn zero() -> Self {
        Self {
            revenue: MoneyCents::new(0),
            cost: MoneyCents::new(0),
            gross_profit: MoneyCents::new(0),
            lost_units: StockQuantity::new(0),
        }
    }
}

/// Result of advancing the simulation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvanceSimulationResult {
    /// New current date after advancement.
    pub current_date: SimulationDate,
    /// Number of simulated days.
    pub days_advanced: u64,
    /// Number of restock orders received.
    pub received_restock_count: StockQuantity,
    /// Number of sales orders recorded.
    pub sales_order_count: StockQuantity,
    /// Units lost to stockouts.
    pub lost_units: StockQuantity,
}

/// Accepted restock order from a decision run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedRestockOrder {
    /// Persisted restock order.
    pub order: RestockOrder,
}

/// Rejected decision-agent proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectedRestockProposal {
    /// Proposed SKU text.
    pub sku: crate::domain::retail::Sku,
    /// Proposed quantity.
    pub quantity: StockQuantity,
    /// Rejection reason.
    pub reason: String,
}

/// Result of one decision run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionResult {
    /// Decision run identifier.
    pub decision_run_id: crate::domain::retail::DecisionRunId,
    /// Accepted restock orders.
    pub accepted_orders: Vec<AcceptedRestockOrder>,
    /// Rejected proposals.
    pub rejected_proposals: Vec<RejectedRestockProposal>,
    /// Decision summary.
    pub summary: String,
}

/// Result of running a workflow cycle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowCycleResult {
    /// Number of simulated days advanced.
    pub days_advanced: u64,
    /// Number of decision runs executed.
    pub decision_runs: StockQuantity,
    /// Final shop date.
    pub final_date: SimulationDate,
}
