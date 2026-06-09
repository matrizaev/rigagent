//! Application-owned errors for retail use cases.

use thiserror::Error;

use crate::domain::retail::{DecisionRunId, DomainError, Sku, SpaceUnits, StockQuantity};

/// Retail application failures.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
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
    /// A proposal would exceed projected capacity.
    #[error("proposal for SKU {sku} exceeds capacity: requested {requested}, capacity {capacity}")]
    CapacityOverflowProposal {
        /// Proposed SKU.
        sku: Sku,
        /// Requested occupied space.
        requested: SpaceUnits,
        /// Available capacity.
        capacity: SpaceUnits,
    },
    /// A store port failed.
    #[error("retail store failed during {operation}: {message}")]
    StoreFailure {
        /// Failed operation.
        operation: &'static str,
        /// Failure detail from the adapter.
        message: String,
    },
    /// A decision-run store port failed.
    #[error("decision run store failed during {operation}: {message}")]
    DecisionRunStoreFailure {
        /// Failed operation.
        operation: &'static str,
        /// Failure detail from the adapter.
        message: String,
    },
    /// The decision agent failed.
    #[error("decision agent failed: {message}")]
    AgentFailure {
        /// Failure detail from the adapter.
        message: String,
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
