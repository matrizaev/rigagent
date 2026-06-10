//! SQLite persistence backed by Diesel.

mod schema;

use std::error::Error as StdError;
use std::fmt::{self, Display, Formatter};
use std::path::Path;
use std::sync::Arc;

use chrono::NaiveDate;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::{OptionalExtension, SqliteConnection};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use thiserror::Error;

use crate::application::retail::{
    ApplicationError, DecisionRunStore, ProfitSummary, RetailSnapshot, RetailStore, SharedError,
};
use crate::domain::retail::{
    ApparelKind, Brand, DecisionHorizonDays, DecisionRun, DecisionRunDetails, DecisionRunId,
    DecisionRunStatus, DemandBacklog, DemandRatePerDay, DomainError, InventoryPosition,
    LeadTimeDays, MoneyCents, Product, ProductDetails, RestockOrder, RestockOrderDetails,
    RestockOrderId, RestockOrderStatus, SalesOrder, SalesOrderDetails, SalesOrderId,
    SimulationDate, SizeLabel, Sku, SpaceUnits, StockQuantity,
};
use crate::infrastructure::scenario::{ScenarioError, ScenarioYamlLoader};
use schema::{decision_runs, inventory, products, restock_orders, sales_orders, shop_state};

/// Embedded Diesel migrations for retail persistence.
pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

type SqlitePool = Pool<ConnectionManager<SqliteConnection>>;
type SqlitePooledConnection = PooledConnection<ConnectionManager<SqliteConnection>>;

/// Shared source error stored by persistence errors.
#[derive(Debug, Clone)]
pub struct ErrorSource {
    source: Arc<dyn StdError + Send + Sync + 'static>,
}

impl ErrorSource {
    fn new(source: impl StdError + Send + Sync + 'static) -> Self {
        Self {
            source: Arc::new(source),
        }
    }

    fn message(message: impl Into<String>) -> Self {
        Self::new(MessageError(message.into()))
    }

    fn boxed(source: Box<dyn StdError + Send + Sync + 'static>) -> Self {
        Self {
            source: Arc::from(source),
        }
    }
}

impl Display for ErrorSource {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.source, formatter)
    }
}

impl StdError for ErrorSource {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(self.source.as_ref())
    }
}

#[derive(Debug, Error, Clone)]
#[error("{0}")]
struct MessageError(String);

/// Persistence adapter errors.
#[derive(Debug, Error)]
pub enum InfrastructureError {
    /// A row was not found.
    #[error("{entity} was not found: {source}")]
    NotFound {
        /// Missing entity.
        entity: &'static str,
        /// Source failure.
        #[source]
        source: ErrorSource,
    },
    /// A unique constraint failed.
    #[error("unique constraint failed during {operation}: {source}")]
    UniqueViolation {
        /// Failed operation.
        operation: &'static str,
        /// Source failure.
        #[source]
        source: ErrorSource,
    },
    /// A foreign-key constraint failed.
    #[error("foreign-key constraint failed during {operation}: {source}")]
    ForeignKeyViolation {
        /// Failed operation.
        operation: &'static str,
        /// Source failure.
        #[source]
        source: ErrorSource,
    },
    /// Embedded migrations failed.
    #[error("migration failed: {source}")]
    MigrationFailed {
        /// Source failure.
        #[source]
        source: ErrorSource,
    },
    /// The `SQLite` connection pool failed.
    #[error("connection pool failed: {source}")]
    PoolFailed {
        /// Source failure.
        #[source]
        source: ErrorSource,
    },
    /// Diesel failed to serialize or deserialize persisted data.
    #[error("serialization failed during {operation}: {source}")]
    SerializationFailed {
        /// Failed operation.
        operation: &'static str,
        /// Source failure.
        #[source]
        source: ErrorSource,
    },
    /// Persisted data violates domain invariants.
    #[error("invalid persisted data in {entity}: {source}")]
    InvalidPersistedData {
        /// Invalid entity.
        entity: &'static str,
        /// Domain conversion failure.
        #[source]
        source: DomainError,
    },
}

impl InfrastructureError {
    fn diesel(operation: &'static str, error: diesel::result::Error) -> Self {
        match &error {
            diesel::result::Error::NotFound => Self::NotFound {
                entity: operation,
                source: ErrorSource::new(error),
            },
            diesel::result::Error::DatabaseError(kind, _) => match kind {
                diesel::result::DatabaseErrorKind::UniqueViolation => Self::UniqueViolation {
                    operation,
                    source: ErrorSource::new(error),
                },
                diesel::result::DatabaseErrorKind::ForeignKeyViolation => {
                    Self::ForeignKeyViolation {
                        operation,
                        source: ErrorSource::new(error),
                    }
                }
                _ => Self::SerializationFailed {
                    operation,
                    source: ErrorSource::new(error),
                },
            },
            _ => Self::SerializationFailed {
                operation,
                source: ErrorSource::new(error),
            },
        }
    }
}

impl From<diesel::result::Error> for InfrastructureError {
    fn from(error: diesel::result::Error) -> Self {
        Self::diesel("diesel transaction", error)
    }
}

impl From<InfrastructureError> for ApplicationError {
    fn from(error: InfrastructureError) -> Self {
        Self::StoreFailure {
            operation: "diesel persistence",
            source: SharedError::new(error),
        }
    }
}

impl From<ScenarioError> for ApplicationError {
    fn from(error: ScenarioError) -> Self {
        Self::StoreFailure {
            operation: "load scenario",
            source: SharedError::new(error),
        }
    }
}

/// Seed state accepted by the Diesel persistence adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeedRetailState {
    /// Initial shop date.
    pub current_date: SimulationDate,
    /// Total stock-space capacity.
    pub capacity: SpaceUnits,
    /// Product catalog.
    pub products: Vec<Product>,
    /// Initial inventory.
    pub inventory: Vec<InventoryPosition>,
}

/// Diesel-backed implementation of the retail store port.
#[derive(Debug, Clone)]
pub struct DieselRetailStore {
    pool: SqlitePool,
}

/// Diesel-backed implementation of the decision-run store port.
#[derive(Debug, Clone)]
pub struct DieselDecisionRunStore {
    pool: SqlitePool,
}

impl DieselRetailStore {
    /// Create a store from an existing pool.
    #[must_use]
    pub const fn from_pool(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Return a clone of the underlying pool.
    #[must_use]
    pub fn pool(&self) -> SqlitePool {
        self.pool.clone()
    }

    /// Seed retail state in one transaction.
    ///
    /// # Errors
    ///
    /// Returns an error when the transaction or domain-to-row mapping fails.
    pub fn seed_state(
        &self,
        state: &SeedRetailState,
        reset: bool,
    ) -> Result<(), InfrastructureError> {
        let mut connection = self.connection()?;
        connection.transaction::<_, InfrastructureError, _>(|connection| {
            if reset {
                clear_state(connection)?;
            }

            diesel::insert_into(shop_state::table)
                .values(ShopStateInsert::try_from(state)?)
                .execute(connection)?;

            for product in &state.products {
                diesel::insert_into(products::table)
                    .values(ProductInsert::try_from(product)?)
                    .execute(connection)?;
            }

            for position in &state.inventory {
                diesel::insert_into(inventory::table)
                    .values(InventoryInsert::try_from(position)?)
                    .execute(connection)?;
            }

            Ok(())
        })
    }

    fn connection(&self) -> Result<SqlitePooledConnection, InfrastructureError> {
        let mut connection = self
            .pool
            .get()
            .map_err(|error| InfrastructureError::PoolFailed {
                source: ErrorSource::new(error),
            })?;
        diesel::sql_query("PRAGMA foreign_keys = ON")
            .execute(&mut connection)
            .map_err(|error| InfrastructureError::diesel("enable foreign keys", error))?;
        Ok(connection)
    }
}

impl DieselDecisionRunStore {
    /// Create a decision-run store from an existing pool.
    #[must_use]
    pub const fn from_pool(pool: SqlitePool) -> Self {
        Self { pool }
    }

    fn connection(&self) -> Result<SqlitePooledConnection, InfrastructureError> {
        let mut connection = self
            .pool
            .get()
            .map_err(|error| InfrastructureError::PoolFailed {
                source: ErrorSource::new(error),
            })?;
        diesel::sql_query("PRAGMA foreign_keys = ON")
            .execute(&mut connection)
            .map_err(|error| InfrastructureError::diesel("enable foreign keys", error))?;
        Ok(connection)
    }
}

/// Create a `SQLite` r2d2 pool.
///
/// # Errors
///
/// Returns an error when r2d2 cannot create the pool.
pub fn create_pool(database_url: impl Into<String>) -> Result<SqlitePool, InfrastructureError> {
    let manager = ConnectionManager::<SqliteConnection>::new(database_url.into());
    Pool::builder()
        .build(manager)
        .map_err(|error| InfrastructureError::PoolFailed {
            source: ErrorSource::new(error),
        })
}

/// Run embedded migrations against a pool.
///
/// # Errors
///
/// Returns an error when a pooled connection or migration execution fails.
pub fn run_migrations(pool: &SqlitePool) -> Result<(), InfrastructureError> {
    let mut connection = pool
        .get()
        .map_err(|error| InfrastructureError::PoolFailed {
            source: ErrorSource::new(error),
        })?;
    connection
        .run_pending_migrations(MIGRATIONS)
        .map_err(|error| InfrastructureError::MigrationFailed {
            source: ErrorSource::boxed(error),
        })?;
    Ok(())
}

impl RetailStore for DieselRetailStore {
    fn state_exists(&self) -> Result<bool, ApplicationError> {
        let mut connection = self.connection()?;
        let exists = shop_state::table
            .select(shop_state::id)
            .first::<i32>(&mut connection)
            .optional()
            .map_err(|error| InfrastructureError::diesel("state exists", error))?
            .is_some();
        Ok(exists)
    }

    fn load_snapshot(&self) -> Result<RetailSnapshot, ApplicationError> {
        let mut connection = self.connection()?;
        let shop = shop_state::table
            .first::<ShopStateRow>(&mut connection)
            .map_err(|error| InfrastructureError::diesel("shop state", error))?;
        if shop.id != 1 {
            return Err(InfrastructureError::SerializationFailed {
                operation: "shop_state.id",
                source: ErrorSource::message("shop state row must use id 1"),
            }
            .into());
        }
        let product_rows = products::table
            .order(products::sku.asc())
            .load::<ProductRow>(&mut connection)
            .map_err(|error| InfrastructureError::diesel("products", error))?;
        let product_values = product_rows
            .iter()
            .map(Product::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        let inventory_values = inventory::table
            .order(inventory::sku.asc())
            .load::<InventoryRow>(&mut connection)
            .map_err(|error| InfrastructureError::diesel("inventory", error))?
            .iter()
            .map(InventoryPosition::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        let open_restocks = restock_orders::table
            .filter(restock_orders::status.eq("open"))
            .order(restock_orders::eta_date.asc())
            .load::<RestockOrderRow>(&mut connection)
            .map_err(|error| InfrastructureError::diesel("open restock orders", error))?
            .iter()
            .map(RestockOrder::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        let sales = load_sales(&mut connection, &product_values)?;
        let profit_summary = profit_summary_from_sales(&sales)?;

        Ok(RetailSnapshot {
            current_date: shop.current_date()?,
            capacity: SpaceUnits::new(u64_from_i64(
                shop.capacity_space_units,
                "shop_state.capacity_space_units",
            )?),
            products: product_values,
            inventory: inventory_values,
            open_restocks,
            recent_sales: sales,
            profit_summary,
        })
    }

    fn seed_scenario(&mut self, scenario_path: &Path, reset: bool) -> Result<(), ApplicationError> {
        let state = ScenarioYamlLoader::load(scenario_path)?;
        self.seed_state(&state, reset)?;
        Ok(())
    }

    fn receive_due_restocks(
        &mut self,
        on_date: SimulationDate,
    ) -> Result<Vec<RestockOrder>, ApplicationError> {
        let mut connection = self.connection()?;
        Ok(
            connection.transaction::<_, InfrastructureError, _>(|connection| {
                let due_rows = restock_orders::table
                    .filter(restock_orders::status.eq("open"))
                    .filter(restock_orders::eta_date.le(date_to_string(on_date)))
                    .order(restock_orders::eta_date.asc())
                    .load::<RestockOrderRow>(connection)
                    .map_err(|error| InfrastructureError::diesel("due restocks", error))?;
                let mut received = Vec::new();

                for row in due_rows {
                    let mut order = RestockOrder::try_from(&row)?;
                    order.receive(on_date).map_err(|source| {
                        InfrastructureError::InvalidPersistedData {
                            entity: "restock_orders",
                            source,
                        }
                    })?;
                    let current_inventory = inventory::table
                        .filter(inventory::sku.eq(row.sku.clone()))
                        .first::<InventoryRow>(connection)
                        .map_err(|error| {
                            InfrastructureError::diesel("inventory for restock", error)
                        })?;
                    let mut position = InventoryPosition::try_from(&current_inventory)?;
                    position
                        .receive_restock(order.quantity())
                        .map_err(|source| InfrastructureError::InvalidPersistedData {
                            entity: "inventory",
                            source,
                        })?;

                    diesel::update(inventory::table.filter(inventory::sku.eq(row.sku.clone())))
                        .set(inventory::on_hand.eq(i64_from_u64(
                            position.on_hand().units(),
                            "inventory.on_hand",
                        )?))
                        .execute(connection)
                        .map_err(|error| InfrastructureError::diesel("update inventory", error))?;
                    diesel::update(restock_orders::table.filter(restock_orders::id.eq(row.id)))
                        .set(restock_orders::status.eq("received"))
                        .execute(connection)
                        .map_err(|error| InfrastructureError::diesel("receive restock", error))?;
                    received.push(order);
                }

                Ok(received)
            })?,
        )
    }

    fn record_sales_day(
        &mut self,
        sales_orders: Vec<SalesOrder>,
        updated_inventory: Vec<InventoryPosition>,
    ) -> Result<(), ApplicationError> {
        let mut connection = self.connection()?;
        Ok(
            connection.transaction::<_, InfrastructureError, _>(|connection| {
                for sale in &sales_orders {
                    diesel::insert_into(sales_orders::table)
                        .values(SalesOrderInsert::try_from(sale)?)
                        .execute(connection)
                        .map_err(|error| {
                            InfrastructureError::diesel("insert sales order", error)
                        })?;
                }
                for position in &updated_inventory {
                    diesel::update(
                        inventory::table.filter(inventory::sku.eq(position.sku().as_str())),
                    )
                    .set((
                        inventory::on_hand.eq(i64_from_u64(
                            position.on_hand().units(),
                            "inventory.on_hand",
                        )?),
                        inventory::demand_backlog_milli_units.eq(i64_from_u64(
                            position.demand_backlog().milli_units(),
                            "inventory.demand_backlog_milli_units",
                        )?),
                    ))
                    .execute(connection)
                    .map_err(|error| {
                        InfrastructureError::diesel("update sales inventory", error)
                    })?;
                }
                Ok(())
            })?,
        )
    }

    fn place_restock_orders(&mut self, orders: Vec<RestockOrder>) -> Result<(), ApplicationError> {
        let mut connection = self.connection()?;
        Ok(
            connection.transaction::<_, InfrastructureError, _>(|connection| {
                for order in &orders {
                    diesel::insert_into(restock_orders::table)
                        .values(RestockOrderInsert::try_from(order)?)
                        .execute(connection)
                        .map_err(|error| {
                            InfrastructureError::diesel("insert restock order", error)
                        })?;
                }
                Ok(())
            })?,
        )
    }

    fn open_restock_orders(&self) -> Result<Vec<RestockOrder>, ApplicationError> {
        let mut connection = self.connection()?;
        let orders = restock_orders::table
            .filter(restock_orders::status.eq("open"))
            .order(restock_orders::eta_date.asc())
            .load::<RestockOrderRow>(&mut connection)
            .map_err(|error| InfrastructureError::diesel("open restocks", error))?
            .iter()
            .map(RestockOrder::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(orders)
    }

    fn profit_summary(&self) -> Result<ProfitSummary, ApplicationError> {
        let snapshot = self.load_snapshot()?;
        Ok(snapshot.profit_summary)
    }

    fn advance_shop_date(&mut self, next_date: SimulationDate) -> Result<(), ApplicationError> {
        let mut connection = self.connection()?;
        diesel::update(shop_state::table.filter(shop_state::id.eq(1)))
            .set(shop_state::current_date.eq(date_to_string(next_date)))
            .execute(&mut connection)
            .map_err(|error| InfrastructureError::diesel("advance shop date", error))?;
        Ok(())
    }
}

impl DecisionRunStore for DieselDecisionRunStore {
    fn start_decision_run(&mut self, run: DecisionRun) -> Result<(), ApplicationError> {
        let mut connection = self.connection()?;
        diesel::insert_into(decision_runs::table)
            .values(DecisionRunInsert::try_from(&run)?)
            .execute(&mut connection)
            .map_err(|error| InfrastructureError::diesel("start decision run", error))?;
        Ok(())
    }

    fn complete_decision_run(
        &mut self,
        run_id: &DecisionRunId,
        summary: String,
        created_restock_count: StockQuantity,
    ) -> Result<(), ApplicationError> {
        let mut connection = self.connection()?;
        diesel::update(decision_runs::table.filter(decision_runs::id.eq(run_id.as_str())))
            .set((
                decision_runs::status.eq("completed"),
                decision_runs::summary.eq(Some(summary)),
                decision_runs::created_restock_count.eq(i64_from_u64(
                    created_restock_count.units(),
                    "decision_runs.created_restock_count",
                )?),
            ))
            .execute(&mut connection)
            .map_err(|error| InfrastructureError::diesel("complete decision run", error))?;
        Ok(())
    }

    fn fail_decision_run(
        &mut self,
        run_id: &DecisionRunId,
        summary: String,
    ) -> Result<(), ApplicationError> {
        let mut connection = self.connection()?;
        diesel::update(decision_runs::table.filter(decision_runs::id.eq(run_id.as_str())))
            .set((
                decision_runs::status.eq("failed"),
                decision_runs::summary.eq(Some(summary)),
            ))
            .execute(&mut connection)
            .map_err(|error| InfrastructureError::diesel("fail decision run", error))?;
        Ok(())
    }

    fn decision_run(&self, run_id: &DecisionRunId) -> Result<DecisionRun, ApplicationError> {
        let mut connection = self.connection()?;
        let row = decision_runs::table
            .filter(decision_runs::id.eq(run_id.as_str()))
            .first::<DecisionRunRow>(&mut connection)
            .map_err(|error| InfrastructureError::diesel("decision run", error))?;
        Ok(DecisionRun::try_from(&row)?)
    }
}

fn clear_state(connection: &mut SqliteConnection) -> Result<(), diesel::result::Error> {
    diesel::delete(restock_orders::table).execute(connection)?;
    diesel::delete(sales_orders::table).execute(connection)?;
    diesel::delete(decision_runs::table).execute(connection)?;
    diesel::delete(inventory::table).execute(connection)?;
    diesel::delete(products::table).execute(connection)?;
    diesel::delete(shop_state::table).execute(connection)?;
    Ok(())
}

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = shop_state)]
struct ShopStateRow {
    id: i32,
    current_date: String,
    capacity_space_units: i64,
}

impl ShopStateRow {
    fn current_date(&self) -> Result<SimulationDate, InfrastructureError> {
        parse_date(&self.current_date, "shop_state.current_date")
    }
}

#[derive(Debug, Insertable)]
#[diesel(table_name = shop_state)]
struct ShopStateInsert {
    id: i32,
    current_date: String,
    capacity_space_units: i64,
}

impl TryFrom<&SeedRetailState> for ShopStateInsert {
    type Error = InfrastructureError;

    fn try_from(state: &SeedRetailState) -> Result<Self, Self::Error> {
        Ok(Self {
            id: 1,
            current_date: date_to_string(state.current_date),
            capacity_space_units: i64_from_u64(
                state.capacity.units(),
                "shop_state.capacity_space_units",
            )?,
        })
    }
}

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = products)]
struct ProductRow {
    sku: String,
    item_type: String,
    brand: String,
    size: String,
    unit_cost_cents: i64,
    unit_price_cents: i64,
    space_units: i64,
    daily_demand_milli_units: i64,
    restock_lead_time_days: i64,
    min_order_quantity: i64,
    max_order_quantity: i64,
    active: bool,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = products)]
struct ProductInsert {
    sku: String,
    item_type: String,
    brand: String,
    size: String,
    unit_cost_cents: i64,
    unit_price_cents: i64,
    space_units: i64,
    daily_demand_milli_units: i64,
    restock_lead_time_days: i64,
    min_order_quantity: i64,
    max_order_quantity: i64,
    active: bool,
}

impl TryFrom<&Product> for ProductInsert {
    type Error = InfrastructureError;

    fn try_from(product: &Product) -> Result<Self, Self::Error> {
        Ok(Self {
            sku: product.sku().as_str().to_owned(),
            item_type: product.kind().to_string(),
            brand: product.brand().as_str().to_owned(),
            size: product.size().to_string(),
            unit_cost_cents: i64_from_u64(product.unit_cost().cents(), "products.unit_cost_cents")?,
            unit_price_cents: i64_from_u64(
                product.unit_price().cents(),
                "products.unit_price_cents",
            )?,
            space_units: i64_from_u64(product.unit_space().units(), "products.space_units")?,
            daily_demand_milli_units: i64_from_u64(
                product.demand_rate().milli_units(),
                "products.daily_demand_milli_units",
            )?,
            restock_lead_time_days: i64_from_u64(
                product.lead_time().days(),
                "products.restock_lead_time_days",
            )?,
            min_order_quantity: i64_from_u64(
                product.min_order_quantity().units(),
                "products.min_order_quantity",
            )?,
            max_order_quantity: i64_from_u64(
                product.max_order_quantity().units(),
                "products.max_order_quantity",
            )?,
            active: product.is_active(),
        })
    }
}

impl TryFrom<&ProductRow> for Product {
    type Error = InfrastructureError;

    fn try_from(row: &ProductRow) -> Result<Self, Self::Error> {
        Self::from_details(ProductDetails {
            sku: Sku::new(row.sku.clone()).map_invalid("products")?,
            kind: row
                .item_type
                .parse::<ApparelKind>()
                .map_invalid("products")?,
            brand: Brand::new(row.brand.clone()).map_invalid("products")?,
            size: row.size.parse::<SizeLabel>().map_invalid("products")?,
            unit_cost: MoneyCents::new(u64_from_i64(
                row.unit_cost_cents,
                "products.unit_cost_cents",
            )?),
            unit_price: MoneyCents::new(u64_from_i64(
                row.unit_price_cents,
                "products.unit_price_cents",
            )?),
            unit_space: SpaceUnits::new(u64_from_i64(row.space_units, "products.space_units")?),
            demand_rate: DemandRatePerDay::from_milli_units(u64_from_i64(
                row.daily_demand_milli_units,
                "products.daily_demand_milli_units",
            )?),
            lead_time: LeadTimeDays::new(u64_from_i64(
                row.restock_lead_time_days,
                "products.restock_lead_time_days",
            )?)
            .map_invalid("products")?,
            min_order_quantity: StockQuantity::new(u64_from_i64(
                row.min_order_quantity,
                "products.min_order_quantity",
            )?),
            max_order_quantity: StockQuantity::new(u64_from_i64(
                row.max_order_quantity,
                "products.max_order_quantity",
            )?),
            active: row.active,
        })
        .map_invalid("products")
    }
}

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = inventory)]
struct InventoryRow {
    sku: String,
    on_hand: i64,
    demand_backlog_milli_units: i64,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = inventory)]
struct InventoryInsert {
    sku: String,
    on_hand: i64,
    demand_backlog_milli_units: i64,
}

impl TryFrom<&InventoryPosition> for InventoryInsert {
    type Error = InfrastructureError;

    fn try_from(position: &InventoryPosition) -> Result<Self, Self::Error> {
        Ok(Self {
            sku: position.sku().as_str().to_owned(),
            on_hand: i64_from_u64(position.on_hand().units(), "inventory.on_hand")?,
            demand_backlog_milli_units: i64_from_u64(
                position.demand_backlog().milli_units(),
                "inventory.demand_backlog_milli_units",
            )?,
        })
    }
}

impl TryFrom<&InventoryRow> for InventoryPosition {
    type Error = InfrastructureError;

    fn try_from(row: &InventoryRow) -> Result<Self, Self::Error> {
        Ok(Self::new(
            Sku::new(row.sku.clone()).map_invalid("inventory")?,
            StockQuantity::new(u64_from_i64(row.on_hand, "inventory.on_hand")?),
            DemandBacklog::from_milli_units(u64_from_i64(
                row.demand_backlog_milli_units,
                "inventory.demand_backlog_milli_units",
            )?)
            .map_invalid("inventory")?,
        ))
    }
}

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = restock_orders)]
struct RestockOrderRow {
    id: String,
    sku: String,
    quantity: i64,
    ordered_at: String,
    eta_date: String,
    status: String,
    decision_run_id: String,
    rationale: String,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = restock_orders)]
struct RestockOrderInsert {
    id: String,
    sku: String,
    quantity: i64,
    ordered_at: String,
    eta_date: String,
    status: String,
    decision_run_id: String,
    rationale: String,
}

impl TryFrom<&RestockOrder> for RestockOrderInsert {
    type Error = InfrastructureError;

    fn try_from(order: &RestockOrder) -> Result<Self, Self::Error> {
        Ok(Self {
            id: order.id().as_str().to_owned(),
            sku: order.sku().as_str().to_owned(),
            quantity: i64_from_u64(order.quantity().units(), "restock_orders.quantity")?,
            ordered_at: date_to_string(order.ordered_at()),
            eta_date: date_to_string(order.eta()),
            status: restock_status_to_string(order.status()),
            decision_run_id: order.decision_run_id().as_str().to_owned(),
            rationale: order.rationale().to_owned(),
        })
    }
}

impl TryFrom<&RestockOrderRow> for RestockOrder {
    type Error = InfrastructureError;

    fn try_from(row: &RestockOrderRow) -> Result<Self, Self::Error> {
        let status = restock_status_from_string(&row.status)?;
        let mut order = Self::open(RestockOrderDetails {
            id: RestockOrderId::new(row.id.clone()).map_invalid("restock_orders")?,
            sku: Sku::new(row.sku.clone()).map_invalid("restock_orders")?,
            quantity: StockQuantity::new(u64_from_i64(row.quantity, "restock_orders.quantity")?),
            ordered_at: parse_date(&row.ordered_at, "restock_orders.ordered_at")?,
            eta: parse_date(&row.eta_date, "restock_orders.eta_date")?,
            decision_run_id: DecisionRunId::new(row.decision_run_id.clone())
                .map_invalid("restock_orders")?,
            rationale: row.rationale.clone(),
        })
        .map_invalid("restock_orders")?;

        match status {
            RestockOrderStatus::Open => {}
            RestockOrderStatus::Received => {
                order.receive(order.eta()).map_invalid("restock_orders")?;
            }
            RestockOrderStatus::Cancelled => order.cancel().map_invalid("restock_orders")?,
        }
        Ok(order)
    }
}

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = sales_orders)]
struct SalesOrderRow {
    id: String,
    sale_date: String,
    sku: String,
    quantity_requested: i64,
    quantity_fulfilled: i64,
    revenue_cents: i64,
    cost_cents: i64,
    lost_units: i64,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = sales_orders)]
struct SalesOrderInsert {
    id: String,
    sale_date: String,
    sku: String,
    quantity_requested: i64,
    quantity_fulfilled: i64,
    revenue_cents: i64,
    cost_cents: i64,
    lost_units: i64,
}

impl TryFrom<&SalesOrder> for SalesOrderInsert {
    type Error = InfrastructureError;

    fn try_from(sale: &SalesOrder) -> Result<Self, Self::Error> {
        Ok(Self {
            id: sale.id().as_str().to_owned(),
            sale_date: date_to_string(sale.sale_date()),
            sku: sale.sku().as_str().to_owned(),
            quantity_requested: i64_from_u64(
                sale.requested().units(),
                "sales_orders.quantity_requested",
            )?,
            quantity_fulfilled: i64_from_u64(
                sale.fulfilled().units(),
                "sales_orders.quantity_fulfilled",
            )?,
            revenue_cents: i64_from_u64(sale.revenue().cents(), "sales_orders.revenue_cents")?,
            cost_cents: i64_from_u64(sale.cost().cents(), "sales_orders.cost_cents")?,
            lost_units: i64_from_u64(sale.lost_units().units(), "sales_orders.lost_units")?,
        })
    }
}

#[derive(Debug, Queryable, Selectable)]
#[diesel(table_name = decision_runs)]
struct DecisionRunRow {
    id: String,
    decision_date: String,
    horizon_days: i64,
    status: String,
    summary: Option<String>,
    created_restock_count: i64,
}

#[derive(Debug, Insertable)]
#[diesel(table_name = decision_runs)]
struct DecisionRunInsert {
    id: String,
    decision_date: String,
    horizon_days: i64,
    status: String,
    summary: Option<String>,
    created_restock_count: i64,
}

impl TryFrom<&DecisionRun> for DecisionRunInsert {
    type Error = InfrastructureError;

    fn try_from(run: &DecisionRun) -> Result<Self, Self::Error> {
        Ok(Self {
            id: run.id().as_str().to_owned(),
            decision_date: date_to_string(run.decision_date()),
            horizon_days: i64_from_u64(run.horizon().days(), "decision_runs.horizon_days")?,
            status: decision_status_to_string(run.status()),
            summary: run.summary().map(str::to_owned),
            created_restock_count: i64_from_u64(
                run.created_restock_count().units(),
                "decision_runs.created_restock_count",
            )?,
        })
    }
}

impl TryFrom<&DecisionRunRow> for DecisionRun {
    type Error = InfrastructureError;

    fn try_from(row: &DecisionRunRow) -> Result<Self, Self::Error> {
        let status = decision_status_from_string(&row.status)?;
        let mut run = Self::start(DecisionRunDetails {
            id: DecisionRunId::new(row.id.clone()).map_invalid("decision_runs")?,
            decision_date: parse_date(&row.decision_date, "decision_runs.decision_date")?,
            horizon: DecisionHorizonDays::new(u64_from_i64(
                row.horizon_days,
                "decision_runs.horizon_days",
            )?)
            .map_invalid("decision_runs")?,
        });

        match status {
            DecisionRunStatus::Started => {}
            DecisionRunStatus::Completed => run
                .complete(
                    row.summary.clone().ok_or_else(|| {
                        InfrastructureError::SerializationFailed {
                            operation: "decision_runs.summary",
                            source: ErrorSource::message("completed run has no summary"),
                        }
                    })?,
                    StockQuantity::new(u64_from_i64(
                        row.created_restock_count,
                        "decision_runs.created_restock_count",
                    )?),
                )
                .map_invalid("decision_runs")?,
            DecisionRunStatus::Failed => run
                .fail(row.summary.clone().ok_or_else(|| {
                    InfrastructureError::SerializationFailed {
                        operation: "decision_runs.summary",
                        source: ErrorSource::message("failed run has no summary"),
                    }
                })?)
                .map_invalid("decision_runs")?,
        }
        Ok(run)
    }
}

trait MapInvalid<T> {
    fn map_invalid(self, entity: &'static str) -> Result<T, InfrastructureError>;
}

impl<T> MapInvalid<T> for Result<T, DomainError> {
    fn map_invalid(self, entity: &'static str) -> Result<T, InfrastructureError> {
        self.map_err(|source| InfrastructureError::InvalidPersistedData { entity, source })
    }
}

fn load_sales(
    connection: &mut SqliteConnection,
    product_values: &[Product],
) -> Result<Vec<SalesOrder>, InfrastructureError> {
    sales_orders::table
        .order(sales_orders::sale_date.desc())
        .load::<SalesOrderRow>(connection)
        .map_err(|error| InfrastructureError::diesel("sales orders", error))?
        .iter()
        .map(|row| {
            SalesOrder::try_from(PersistedSalesOrder {
                row,
                product_values,
            })
        })
        .collect()
}

struct PersistedSalesOrder<'a> {
    row: &'a SalesOrderRow,
    product_values: &'a [Product],
}

impl TryFrom<PersistedSalesOrder<'_>> for SalesOrder {
    type Error = InfrastructureError;

    fn try_from(persisted: PersistedSalesOrder<'_>) -> Result<Self, Self::Error> {
        let row = persisted.row;
        let product = persisted
            .product_values
            .iter()
            .find(|candidate| candidate.sku().as_str() == row.sku)
            .ok_or(InfrastructureError::NotFound {
                entity: "sales_orders.product",
                source: ErrorSource::message("sales order references an unknown product"),
            })?;
        let sale = Self::record(SalesOrderDetails {
            id: SalesOrderId::new(row.id.clone()).map_invalid("sales_orders")?,
            sale_date: parse_date(&row.sale_date, "sales_orders.sale_date")?,
            sku: Sku::new(row.sku.clone()).map_invalid("sales_orders")?,
            requested: StockQuantity::new(u64_from_i64(
                row.quantity_requested,
                "sales_orders.quantity_requested",
            )?),
            fulfilled: StockQuantity::new(u64_from_i64(
                row.quantity_fulfilled,
                "sales_orders.quantity_fulfilled",
            )?),
            unit_price: product.unit_price(),
            unit_cost: product.unit_cost(),
        })
        .map_invalid("sales_orders")?;

        if sale.revenue().cents() != u64_from_i64(row.revenue_cents, "sales_orders.revenue_cents")?
            || sale.cost().cents() != u64_from_i64(row.cost_cents, "sales_orders.cost_cents")?
            || sale.lost_units().units() != u64_from_i64(row.lost_units, "sales_orders.lost_units")?
        {
            return Err(InfrastructureError::SerializationFailed {
                operation: "sales_orders totals",
                source: ErrorSource::message("persisted totals do not match product economics"),
            });
        }

        Ok(sale)
    }
}

fn profit_summary_from_sales(sales: &[SalesOrder]) -> Result<ProfitSummary, InfrastructureError> {
    let mut revenue = MoneyCents::new(0);
    let mut cost = MoneyCents::new(0);
    let mut lost_units = StockQuantity::new(0);
    for sale in sales {
        revenue = revenue
            .checked_add(sale.revenue())
            .map_invalid("sales_orders")?;
        cost = cost.checked_add(sale.cost()).map_invalid("sales_orders")?;
        lost_units = lost_units
            .checked_add(sale.lost_units())
            .map_invalid("sales_orders")?;
    }
    Ok(ProfitSummary {
        revenue,
        cost,
        gross_profit: revenue.checked_sub(cost).map_invalid("sales_orders")?,
        lost_units,
    })
}

fn parse_date(value: &str, operation: &'static str) -> Result<SimulationDate, InfrastructureError> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(SimulationDate::new)
        .map_err(|error| InfrastructureError::SerializationFailed {
            operation,
            source: ErrorSource::new(error),
        })
}

fn date_to_string(date: SimulationDate) -> String {
    date.as_naive_date().to_string()
}

fn i64_from_u64(value: u64, operation: &'static str) -> Result<i64, InfrastructureError> {
    i64::try_from(value).map_err(|_| InfrastructureError::SerializationFailed {
        operation,
        source: ErrorSource::message("value does not fit in SQLite integer"),
    })
}

fn u64_from_i64(value: i64, operation: &'static str) -> Result<u64, InfrastructureError> {
    u64::try_from(value).map_err(|_| InfrastructureError::SerializationFailed {
        operation,
        source: ErrorSource::message("negative persisted integer"),
    })
}

fn restock_status_to_string(status: RestockOrderStatus) -> String {
    match status {
        RestockOrderStatus::Open => "open",
        RestockOrderStatus::Received => "received",
        RestockOrderStatus::Cancelled => "cancelled",
    }
    .to_owned()
}

fn restock_status_from_string(value: &str) -> Result<RestockOrderStatus, InfrastructureError> {
    match value {
        "open" => Ok(RestockOrderStatus::Open),
        "received" => Ok(RestockOrderStatus::Received),
        "cancelled" => Ok(RestockOrderStatus::Cancelled),
        _ => Err(InfrastructureError::SerializationFailed {
            operation: "restock_orders.status",
            source: ErrorSource::message("unknown restock status"),
        }),
    }
}

fn decision_status_to_string(status: DecisionRunStatus) -> String {
    match status {
        DecisionRunStatus::Started => "started",
        DecisionRunStatus::Completed => "completed",
        DecisionRunStatus::Failed => "failed",
    }
    .to_owned()
}

fn decision_status_from_string(value: &str) -> Result<DecisionRunStatus, InfrastructureError> {
    match value {
        "started" => Ok(DecisionRunStatus::Started),
        "completed" => Ok(DecisionRunStatus::Completed),
        "failed" => Ok(DecisionRunStatus::Failed),
        _ => Err(InfrastructureError::SerializationFailed {
            operation: "decision_runs.status",
            source: ErrorSource::message("unknown decision status"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use diesel::RunQueryDsl;
    use uuid::Uuid;

    use super::*;

    fn database_url() -> String {
        format!("/tmp/rigagent-retail-{}.sqlite", Uuid::new_v4())
    }

    fn test_date() -> Result<SimulationDate, InfrastructureError> {
        parse_date("2026-06-09", "test date")
    }

    fn product() -> Result<Product, InfrastructureError> {
        Product::from_details(ProductDetails {
            sku: Sku::new("TSH-ACME-M-BLK").map_invalid("test")?,
            kind: ApparelKind::Shirt,
            brand: Brand::new("Acme").map_invalid("test")?,
            size: SizeLabel::M,
            unit_cost: MoneyCents::new(1_200),
            unit_price: MoneyCents::new(2_999),
            unit_space: SpaceUnits::new(2),
            demand_rate: DemandRatePerDay::from_milli_units(2_750),
            lead_time: LeadTimeDays::new(2).map_invalid("test")?,
            min_order_quantity: StockQuantity::new(2),
            max_order_quantity: StockQuantity::new(20),
            active: true,
        })
        .map_invalid("test")
    }

    fn seed_state() -> Result<SeedRetailState, InfrastructureError> {
        let product = product()?;
        Ok(SeedRetailState {
            current_date: test_date()?,
            capacity: SpaceUnits::new(100),
            products: vec![product.clone()],
            inventory: vec![InventoryPosition::new(
                product.sku().clone(),
                StockQuantity::new(10),
                DemandBacklog::ZERO,
            )],
        })
    }

    fn stores() -> Result<(DieselRetailStore, DieselDecisionRunStore), InfrastructureError> {
        let pool = create_pool(database_url())?;
        run_migrations(&pool)?;
        let store = DieselRetailStore::from_pool(pool.clone());
        let runs = DieselDecisionRunStore::from_pool(pool);
        Ok((store, runs))
    }

    fn started_decision_run(
        runs: &mut DieselDecisionRunStore,
    ) -> Result<DecisionRunId, ApplicationError> {
        let run_id = DecisionRunId::new("decision-1")?;
        runs.start_decision_run(DecisionRun::start(DecisionRunDetails {
            id: run_id.clone(),
            decision_date: test_date()?,
            horizon: DecisionHorizonDays::new(14)?,
        }))?;
        Ok(run_id)
    }

    fn restock_order(
        run_id: &DecisionRunId,
        quantity: StockQuantity,
    ) -> Result<RestockOrder, InfrastructureError> {
        let product = product()?;
        let date = test_date()?;
        RestockOrder::open(RestockOrderDetails {
            id: RestockOrderId::new("restock-1").map_invalid("test")?,
            sku: product.sku().clone(),
            quantity,
            ordered_at: date,
            eta: date,
            decision_run_id: run_id.clone(),
            rationale: "test restock".to_owned(),
        })
        .map_invalid("test")
    }

    fn sale(id: &str) -> Result<SalesOrder, InfrastructureError> {
        let product = product()?;
        SalesOrder::record(SalesOrderDetails {
            id: SalesOrderId::new(id).map_invalid("test")?,
            sale_date: test_date()?,
            sku: product.sku().clone(),
            requested: StockQuantity::new(3),
            fulfilled: StockQuantity::new(2),
            unit_price: product.unit_price(),
            unit_cost: product.unit_cost(),
        })
        .map_invalid("test")
    }

    #[test]
    fn seeds_state_with_real_migrations() -> Result<(), Box<dyn std::error::Error>> {
        let (store, _) = stores()?;
        store.seed_state(&seed_state()?, true)?;

        let snapshot = store.load_snapshot()?;

        assert!(store.state_exists()?);
        assert_eq!(snapshot.products.len(), 1);
        assert_eq!(snapshot.inventory.len(), 1);
        assert_eq!(snapshot.current_date, test_date()?);
        Ok(())
    }

    #[test]
    fn seeds_scenario_yaml_through_store_port() -> Result<(), Box<dyn std::error::Error>> {
        let (mut store, _) = stores()?;
        let path = std::env::current_dir()?.join("data/retail_scenario.yaml");

        store.seed_scenario(&path, true)?;
        let snapshot = store.load_snapshot()?;

        assert_eq!(snapshot.current_date, parse_date("2026-06-09", "test")?);
        assert_eq!(snapshot.products.len(), 4);
        assert_eq!(snapshot.inventory.len(), 4);
        Ok(())
    }

    #[test]
    fn advances_shop_date() -> Result<(), Box<dyn std::error::Error>> {
        let (mut store, _) = stores()?;
        store.seed_state(&seed_state()?, true)?;
        let next_date = test_date()?.checked_add_days(2)?;

        store.advance_shop_date(next_date)?;

        assert_eq!(store.load_snapshot()?.current_date, next_date);
        Ok(())
    }

    #[test]
    fn receives_due_restock_and_updates_inventory() -> Result<(), Box<dyn std::error::Error>> {
        let (mut store, mut runs) = stores()?;
        store.seed_state(&seed_state()?, true)?;
        let run_id = started_decision_run(&mut runs)?;
        store.place_restock_orders(vec![restock_order(&run_id, StockQuantity::new(5))?])?;

        let received = store.receive_due_restocks(test_date()?)?;
        let snapshot = store.load_snapshot()?;
        let position = snapshot.inventory.first().ok_or("missing inventory")?;

        assert_eq!(received.len(), 1);
        assert_eq!(position.on_hand().units(), 15);
        assert_eq!(snapshot.open_restocks.len(), 0);
        Ok(())
    }

    #[test]
    fn records_sales_day_and_profit_summary() -> Result<(), Box<dyn std::error::Error>> {
        let (mut store, _) = stores()?;
        store.seed_state(&seed_state()?, true)?;
        let product = product()?;
        let updated_inventory = vec![InventoryPosition::new(
            product.sku().clone(),
            StockQuantity::new(8),
            DemandBacklog::from_milli_units(250).map_invalid("test")?,
        )];

        store.record_sales_day(vec![sale("sale-1")?], updated_inventory)?;
        let summary = store.profit_summary()?;

        assert_eq!(summary.revenue.cents(), 5_998);
        assert_eq!(summary.cost.cents(), 2_400);
        assert_eq!(summary.gross_profit.cents(), 3_598);
        Ok(())
    }

    #[test]
    fn rolls_back_sales_day_on_duplicate_sale_id() -> Result<(), Box<dyn std::error::Error>> {
        let (mut store, _) = stores()?;
        store.seed_state(&seed_state()?, true)?;
        let duplicate_sale = sale("sale-1")?;

        let result = store.record_sales_day(
            vec![duplicate_sale.clone(), duplicate_sale],
            seed_state()?.inventory,
        );

        assert!(result.is_err());
        assert_eq!(store.load_snapshot()?.recent_sales.len(), 0);
        Ok(())
    }

    #[test]
    fn maps_foreign_key_violation_for_restock_order() -> Result<(), Box<dyn std::error::Error>> {
        let (mut store, _) = stores()?;
        store.seed_state(&seed_state()?, true)?;
        let missing_run_id = DecisionRunId::new("missing-run")?;

        let result = store
            .place_restock_orders(vec![restock_order(&missing_run_id, StockQuantity::new(5))?]);

        let source = match result {
            Err(ApplicationError::StoreFailure { source, .. }) => source,
            other => return Err(format!("expected store failure, got {other:?}").into()),
        };
        let infrastructure = std::error::Error::source(&source)
            .and_then(|source| source.downcast_ref::<InfrastructureError>())
            .ok_or("expected infrastructure error source")?;

        assert!(matches!(
            infrastructure,
            InfrastructureError::ForeignKeyViolation { .. }
        ));
        Ok(())
    }

    #[test]
    fn maps_unique_violation_for_duplicate_product_seed() -> Result<(), Box<dyn std::error::Error>>
    {
        let (store, _) = stores()?;
        store.seed_state(&seed_state()?, true)?;

        let result = store.seed_state(&seed_state()?, false);

        assert!(matches!(
            result,
            Err(InfrastructureError::UniqueViolation { .. })
        ));
        Ok(())
    }

    #[test]
    fn rejects_invalid_persisted_product_kind() -> Result<(), Box<dyn std::error::Error>> {
        let (store, _) = stores()?;
        store.seed_state(&seed_state()?, true)?;
        let mut connection = store.connection()?;

        diesel::update(products::table)
            .set(products::item_type.eq("unknown"))
            .execute(&mut connection)?;

        let result = store.load_snapshot();

        assert!(result.is_err());
        Ok(())
    }
}
