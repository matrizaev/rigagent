//! Retail replenishment domain model.

mod entities;
mod errors;
mod services;
mod value_objects;

pub use entities::{
    DecisionRun, DecisionRunDetails, DecisionRunStatus, InventoryPosition, Product, ProductDetails,
    RestockOrder, RestockOrderDetails, RestockOrderStatus, SalesOrder, SalesOrderDetails,
};
pub use errors::DomainError;
pub use services::{
    DemandSimulation, DemandSimulator, RestockOption, RestockOptionRequest, RestockOptionScorer,
};
pub use value_objects::{
    ApparelKind, Brand, DecisionHorizonDays, DecisionRunId, DemandBacklog, DemandRatePerDay,
    LeadTimeDays, MoneyCents, RestockOrderId, SalesOrderId, SimulationDate, SizeLabel, Sku,
    SpaceUnits, StockQuantity,
};
