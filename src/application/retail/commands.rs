//! Retail application commands and queries.

use std::path::PathBuf;

use crate::domain::retail::{DecisionHorizonDays, StockQuantity};

/// Seed durable retail state from a scenario file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedRetailScenario {
    /// Scenario file path.
    pub scenario_path: PathBuf,
    /// Whether existing state should be replaced.
    pub reset: bool,
}

/// Advance the deterministic simulation by a number of days.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdvanceSimulation {
    /// Number of days to advance.
    pub days: u64,
}

/// Run one restock decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunRestockDecision {
    /// Demand horizon to evaluate.
    pub horizon: DecisionHorizonDays,
    /// Maximum accepted restock orders.
    pub max_restock_orders: StockQuantity,
}

/// Run repeated simulation and restock decision cycles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunWorkflowCycle {
    /// Total simulation days to advance.
    pub total_days: u64,
    /// Decision interval in simulated days.
    pub decision_interval_days: u64,
    /// Demand horizon to evaluate at each decision.
    pub horizon: DecisionHorizonDays,
    /// Maximum accepted restock orders per decision.
    pub max_restock_orders: StockQuantity,
}
