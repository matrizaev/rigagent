//! Application-owned errors for retail use cases.

use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::sync::Arc;

use thiserror::Error;

use crate::domain::retail::{DecisionRunId, DomainError, Sku, SpaceUnits, StockQuantity};

/// Shared source error stored without coupling application code to adapter error types.
#[derive(Debug, Clone)]
pub struct SharedError {
    source: Arc<dyn Error + Send + Sync + 'static>,
}

impl SharedError {
    /// Create a shared source error.
    #[must_use]
    pub fn new(source: impl Error + Send + Sync + 'static) -> Self {
        Self {
            source: Arc::new(source),
        }
    }
}

impl Display for SharedError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.source, formatter)
    }
}

impl Error for SharedError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

#[derive(Debug, Error, Clone)]
#[error("{0}")]
struct MessageError(String);

/// Retail application failures.
#[derive(Debug, Error, Clone)]
pub enum ApplicationError {
    /// A domain invariant failed while executing a use case.
    #[error("domain invariant failed")]
    Domain(#[from] DomainError),
    /// Durable state already exists and reset was not requested.
    #[error("retail state already exists; pass reset to seed again")]
    StateAlreadyExists,
    /// A product SKU was not found in the loaded snapshot.
    #[error("SKU {sku} was not found")]
    SkuNotFound {
        /// Missing SKU.
        sku: Sku,
    },
    /// A product exists but cannot currently be restocked.
    #[error("SKU {sku} is inactive")]
    InactiveProduct {
        /// Inactive SKU.
        sku: Sku,
    },
    /// Inventory was not found for a product SKU.
    #[error("inventory for SKU {sku} was not found")]
    InventoryNotFound {
        /// Missing inventory SKU.
        sku: Sku,
    },
    /// A proposal tried to order a non-positive quantity.
    #[error("proposal for SKU {sku} has non-positive quantity")]
    NonPositiveProposalQuantity {
        /// Proposed SKU.
        sku: Sku,
    },
    /// A proposal duplicates an already-open restock order.
    #[error("SKU {sku} already has an open restock order")]
    DuplicateOpenRestockOrder {
        /// Duplicate SKU.
        sku: Sku,
    },
    /// A proposal would exceed projected available capacity.
    #[error(
        "proposal for SKU {sku} exceeds available capacity: requires {required}, available {available}, overflow {overflow}"
    )]
    CapacityOverflowProposal {
        /// Proposed SKU.
        sku: Sku,
        /// Space required by the proposal.
        required: SpaceUnits,
        /// Space available before the proposal.
        available: SpaceUnits,
        /// Space units by which the proposal exceeds available capacity.
        overflow: SpaceUnits,
    },
    /// A store port failed.
    #[error("retail store failed during {operation}: {source}")]
    StoreFailure {
        /// Failed operation.
        operation: &'static str,
        /// Source failure from the adapter.
        #[source]
        source: SharedError,
    },
    /// A decision-run store port failed.
    #[error("decision run store failed during {operation}: {source}")]
    DecisionRunStoreFailure {
        /// Failed operation.
        operation: &'static str,
        /// Source failure from the adapter.
        #[source]
        source: SharedError,
    },
    /// A clock port failed.
    #[error("clock failed during {operation}: {source}")]
    ClockFailure {
        /// Failed operation.
        operation: &'static str,
        /// Source failure from the clock adapter.
        #[source]
        source: SharedError,
    },
    /// The decision agent failed.
    #[error("decision agent failed: {source}")]
    AgentFailure {
        /// Source failure from the agent adapter.
        #[source]
        source: SharedError,
    },
    /// A decision run could not be found.
    #[error("decision run {run_id} was not found")]
    DecisionRunNotFound {
        /// Missing decision run identifier.
        run_id: DecisionRunId,
    },
    /// A numeric command argument must be greater than zero.
    #[error("{field} must be greater than zero")]
    NonPositiveCommand {
        /// Invalid command field.
        field: &'static str,
    },
    /// A count could not fit inside the domain quantity type.
    #[error("count overflow while computing {operation}")]
    CountOverflow {
        /// Failed operation.
        operation: &'static str,
    },
    /// A proposed restock quantity was rejected by domain product bounds.
    #[error("proposal for SKU {sku} quantity {quantity} is outside product bounds")]
    ProposalOutOfBounds {
        /// Proposed SKU.
        sku: Sku,
        /// Proposed quantity.
        quantity: StockQuantity,
    },
}

impl ApplicationError {
    /// Create a store failure from a displayable message.
    #[must_use]
    pub fn store_failure(operation: &'static str, message: impl Into<String>) -> Self {
        Self::StoreFailure {
            operation,
            source: SharedError::new(MessageError(message.into())),
        }
    }

    /// Create a decision-run store failure from a displayable message.
    #[must_use]
    pub fn decision_run_store_failure(operation: &'static str, message: impl Into<String>) -> Self {
        Self::DecisionRunStoreFailure {
            operation,
            source: SharedError::new(MessageError(message.into())),
        }
    }

    /// Create a clock failure from a source error.
    #[must_use]
    pub fn clock_failure(
        operation: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::ClockFailure {
            operation,
            source: SharedError::new(source),
        }
    }

    /// Create an agent failure from a source error.
    #[must_use]
    pub fn agent_failure(source: impl Error + Send + Sync + 'static) -> Self {
        Self::AgentFailure {
            source: SharedError::new(source),
        }
    }

    /// Create an agent failure from a displayable message.
    #[must_use]
    pub fn agent_failure_message(message: impl Into<String>) -> Self {
        Self::agent_failure(MessageError(message.into()))
    }
}
