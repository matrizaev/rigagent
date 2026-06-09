//! Domain-owned errors for retail invariants.

use thiserror::Error;

use super::{
    DecisionRunId, DemandBacklog, MoneyCents, RestockOrderId, RestockOrderStatus, SimulationDate,
    Sku, SpaceUnits, StockQuantity,
};

/// Retail domain invariant violations and impossible transitions.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DomainError {
    /// A text value was empty after trimming.
    #[error("{field} must not be empty")]
    EmptyText {
        /// Name of the invalid field.
        field: &'static str,
    },
    /// A text value used an unsupported format.
    #[error("{field} has invalid format")]
    InvalidText {
        /// Name of the invalid field.
        field: &'static str,
        /// Original invalid value.
        value: String,
    },
    /// A numeric value must be greater than zero.
    #[error("{field} must be greater than zero")]
    NonPositive {
        /// Name of the invalid field.
        field: &'static str,
    },
    /// A numeric value exceeded its allowed maximum.
    #[error("{field} exceeds maximum {maximum}")]
    AboveMaximum {
        /// Name of the invalid field.
        field: &'static str,
        /// Inclusive maximum value.
        maximum: u64,
    },
    /// A checked arithmetic operation overflowed.
    #[error("arithmetic overflow while computing {operation}")]
    ArithmeticOverflow {
        /// Business operation being computed.
        operation: &'static str,
    },
    /// A checked subtraction would produce a negative value.
    #[error("insufficient value while computing {operation}")]
    InsufficientValue {
        /// Business operation being computed.
        operation: &'static str,
    },
    /// A product restock order quantity is outside the product's configured bounds.
    #[error("restock quantity {requested} for {sku} is outside bounds {minimum}..={maximum}")]
    RestockQuantityOutOfBounds {
        /// Product SKU.
        sku: Sku,
        /// Requested quantity.
        requested: StockQuantity,
        /// Minimum allowed quantity.
        minimum: StockQuantity,
        /// Maximum allowed quantity.
        maximum: StockQuantity,
    },
    /// Inventory would exceed the available stock-space capacity.
    #[error("inventory capacity exceeded: requested {requested}, capacity {capacity}")]
    CapacityExceeded {
        /// Requested occupied capacity.
        requested: SpaceUnits,
        /// Available capacity.
        capacity: SpaceUnits,
    },
    /// A restock order transition is not valid from its current status.
    #[error("restock order {order_id} cannot transition from {status:?}")]
    InvalidRestockTransition {
        /// Restock order identifier.
        order_id: RestockOrderId,
        /// Current restock status.
        status: RestockOrderStatus,
    },
    /// A restock order cannot be received before its ETA.
    #[error("restock order {order_id} cannot be received on {received_on} before ETA {eta}")]
    RestockReceivedBeforeEta {
        /// Restock order identifier.
        order_id: RestockOrderId,
        /// Attempted receipt date.
        received_on: SimulationDate,
        /// Earliest valid receipt date.
        eta: SimulationDate,
    },
    /// A decision run transition is not valid from its current status.
    #[error("decision run {run_id} cannot transition from {status:?}")]
    InvalidDecisionRunTransition {
        /// Decision run identifier.
        run_id: DecisionRunId,
        /// Current decision-run status.
        status: super::DecisionRunStatus,
    },
    /// A demand backlog had invalid fixed-point units.
    #[error("demand backlog {backlog:?} must remain below one unit")]
    InvalidDemandBacklog {
        /// Invalid demand backlog.
        backlog: DemandBacklog,
    },
    /// A date operation would exceed chrono's supported range.
    #[error("date arithmetic overflow from {date}")]
    DateOverflow {
        /// Starting date.
        date: SimulationDate,
    },
    /// A sales record cannot fulfill more units than requested.
    #[error("fulfilled quantity {fulfilled} exceeds requested quantity {requested}")]
    FulfilledExceedsRequested {
        /// Requested sales quantity.
        requested: StockQuantity,
        /// Fulfilled sales quantity.
        fulfilled: StockQuantity,
    },
    /// A product price cannot be lower than its unit cost.
    #[error("unit price {unit_price} is lower than unit cost {unit_cost}")]
    NegativeMargin {
        /// Unit cost.
        unit_cost: MoneyCents,
        /// Unit price.
        unit_price: MoneyCents,
    },
}
