//! Identifier adapters for application-created retail entities.

use uuid::Uuid;

use crate::application::retail::{ApplicationError, IdGenerator};
use crate::domain::retail::{DecisionRunId, RestockOrderId, SalesOrderId};

/// UUID-backed retail identifier generator.
#[derive(Debug, Clone, Copy, Default)]
pub struct UuidRetailIdGenerator;

impl UuidRetailIdGenerator {
    /// Create a UUID-backed identifier generator.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl IdGenerator for UuidRetailIdGenerator {
    fn sales_order_id(&mut self) -> Result<SalesOrderId, ApplicationError> {
        Ok(SalesOrderId::new(format!("sale-{}", Uuid::new_v4()))?)
    }

    fn restock_order_id(&mut self) -> Result<RestockOrderId, ApplicationError> {
        Ok(RestockOrderId::new(format!("restock-{}", Uuid::new_v4()))?)
    }

    fn decision_run_id(&mut self) -> Result<DecisionRunId, ApplicationError> {
        Ok(DecisionRunId::new(format!("decision-{}", Uuid::new_v4()))?)
    }
}
