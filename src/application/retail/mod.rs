//! Retail replenishment application layer.

mod commands;
mod errors;
mod ports;
mod read_models;
mod use_cases;

pub use commands::{AdvanceSimulation, RunRestockDecision, RunWorkflowCycle, SeedRetailScenario};
pub use errors::{ApplicationError, SharedError};
pub use ports::{
    Clock, DecisionAgentRequest, DecisionAgentResponse, DecisionRunStore, IdGenerator,
    ProposedRestockOrder, ReplenishmentDecisionAgent, RetailStore,
};
pub use read_models::{
    AcceptedRestockOrder, AdvanceSimulationResult, DecisionResult, EventCount, GetRetailSnapshot,
    ProfitSummary, RejectedRestockProposal, RetailSnapshot, WorkflowCycleResult,
};
pub use use_cases::RetailWorkflow;
