//! Scenario YAML loading for retail seed data.

use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::str::FromStr;

use chrono::NaiveDate;
use serde::Deserialize;
use thiserror::Error;

use crate::domain::retail::{
    ApparelKind, Brand, DemandBacklog, DemandRatePerDay, DomainError, InventoryPosition,
    LeadTimeDays, MoneyCents, Product, ProductDetails, SimulationDate, SizeLabel, Sku, SpaceUnits,
    StockQuantity,
};
use crate::infrastructure::persistence::SeedRetailState;

/// Errors returned while loading retail scenario YAML.
#[derive(Debug, Error)]
pub enum ScenarioError {
    /// The scenario file could not be read.
    #[error("failed to read scenario file {path}: {source}")]
    ReadFailed {
        /// Scenario file path.
        path: String,
        /// Source IO failure.
        #[source]
        source: std::io::Error,
    },
    /// The scenario YAML could not be parsed.
    #[error("failed to parse scenario YAML: {source}")]
    ParseFailed {
        /// YAML parser failure.
        #[source]
        source: serde_yaml::Error,
    },
    /// Scenario data violates a domain invariant.
    #[error("invalid scenario field {field}: {source}")]
    InvalidDomainValue {
        /// Invalid field path.
        field: &'static str,
        /// Domain invariant failure.
        #[source]
        source: DomainError,
    },
    /// Scenario data uses an invalid scalar value.
    #[error("invalid scenario field {field}: {message}")]
    InvalidValue {
        /// Invalid field path.
        field: &'static str,
        /// Validation detail.
        message: String,
    },
    /// The scenario product list is empty.
    #[error("scenario must include at least one product")]
    EmptyProducts,
    /// The scenario contains the same SKU more than once.
    #[error("scenario contains duplicate SKU {sku}")]
    DuplicateSku {
        /// Duplicate SKU.
        sku: Sku,
    },
    /// Initial stock exceeds configured shop capacity.
    #[error("initial inventory uses {used} space units, capacity is {capacity}")]
    CapacityTooSmall {
        /// Initial occupied stock-space.
        used: SpaceUnits,
        /// Configured shop capacity.
        capacity: SpaceUnits,
    },
}

impl From<serde_yaml::Error> for ScenarioError {
    fn from(source: serde_yaml::Error) -> Self {
        Self::ParseFailed { source }
    }
}

/// YAML-backed retail scenario loader.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScenarioYamlLoader;

impl ScenarioYamlLoader {
    /// Load a retail seed scenario from YAML.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be read, parsed, or converted into domain values.
    pub fn load(path: &Path) -> Result<SeedRetailState, ScenarioError> {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(source) => {
                return Err(ScenarioError::ReadFailed {
                    path: path.display().to_string(),
                    source,
                });
            }
        };
        let scenario = serde_yaml::from_str::<ScenarioDto>(&content)?;
        scenario.try_into()
    }
}

#[derive(Debug, Deserialize)]
struct ScenarioDto {
    shop: ShopDto,
    products: Vec<ProductDto>,
}

impl TryFrom<ScenarioDto> for SeedRetailState {
    type Error = ScenarioError;

    fn try_from(scenario: ScenarioDto) -> Result<Self, Self::Error> {
        if scenario.products.is_empty() {
            return Err(ScenarioError::EmptyProducts);
        }

        let current_date = parse_date(&scenario.shop.start_date, "shop.start_date")?;
        let capacity = SpaceUnits::new(non_negative_u64(
            scenario.shop.capacity_space_units,
            "shop.capacity_space_units",
        )?);
        let mut seen_skus = HashSet::new();
        let mut products = Vec::new();
        let mut inventory = Vec::new();
        let mut occupied = SpaceUnits::new(0);

        for product_dto in scenario.products {
            let product = Product::try_from(&product_dto)?;
            if !seen_skus.insert(product.sku().clone()) {
                return Err(ScenarioError::DuplicateSku {
                    sku: product.sku().clone(),
                });
            }

            let initial_on_hand = StockQuantity::new(non_negative_u64(
                product_dto.initial_on_hand,
                "products.initial_on_hand",
            )?);
            occupied = occupied
                .checked_add(
                    product
                        .unit_space()
                        .checked_mul_quantity(initial_on_hand)
                        .map_domain("products.initial_on_hand")?,
                )
                .map_domain("products.initial_on_hand")?;
            inventory.push(InventoryPosition::new(
                product.sku().clone(),
                initial_on_hand,
                DemandBacklog::ZERO,
            ));
            products.push(product);
        }

        if occupied > capacity {
            return Err(ScenarioError::CapacityTooSmall {
                used: occupied,
                capacity,
            });
        }

        Ok(Self {
            current_date,
            capacity,
            products,
            inventory,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ShopDto {
    start_date: String,
    capacity_space_units: i64,
}

#[derive(Debug, Deserialize)]
struct ProductDto {
    sku: String,
    item_type: String,
    brand: String,
    size: String,
    unit_cost_cents: i64,
    unit_price_cents: i64,
    space_units: i64,
    initial_on_hand: i64,
    daily_demand_rate: String,
    restock_lead_time_days: i64,
    min_order_quantity: i64,
    max_order_quantity: i64,
}

impl TryFrom<&ProductDto> for Product {
    type Error = ScenarioError;

    fn try_from(product: &ProductDto) -> Result<Self, Self::Error> {
        Self::from_details(ProductDetails {
            sku: Sku::new(product.sku.clone()).map_domain("products.sku")?,
            kind: apparel_kind(&product.item_type)?,
            brand: Brand::new(product.brand.clone()).map_domain("products.brand")?,
            size: size_label(&product.size)?,
            unit_cost: MoneyCents::new(non_negative_u64(
                product.unit_cost_cents,
                "products.unit_cost_cents",
            )?),
            unit_price: MoneyCents::new(non_negative_u64(
                product.unit_price_cents,
                "products.unit_price_cents",
            )?),
            unit_space: SpaceUnits::new(non_negative_u64(
                product.space_units,
                "products.space_units",
            )?),
            demand_rate: DemandRatePerDay::from_str(&product.daily_demand_rate)
                .map_domain("products.daily_demand_rate")?,
            lead_time: LeadTimeDays::new(non_negative_u64(
                product.restock_lead_time_days,
                "products.restock_lead_time_days",
            )?)
            .map_domain("products.restock_lead_time_days")?,
            min_order_quantity: StockQuantity::new(non_negative_u64(
                product.min_order_quantity,
                "products.min_order_quantity",
            )?),
            max_order_quantity: StockQuantity::new(non_negative_u64(
                product.max_order_quantity,
                "products.max_order_quantity",
            )?),
            active: true,
        })
        .map_domain("products")
    }
}

trait MapDomain<T> {
    fn map_domain(self, field: &'static str) -> Result<T, ScenarioError>;
}

impl<T> MapDomain<T> for Result<T, DomainError> {
    fn map_domain(self, field: &'static str) -> Result<T, ScenarioError> {
        match self {
            Ok(value) => Ok(value),
            Err(source) => Err(ScenarioError::InvalidDomainValue { field, source }),
        }
    }
}

fn parse_date(value: &str, field: &'static str) -> Result<SimulationDate, ScenarioError> {
    match NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        Ok(date) => Ok(SimulationDate::new(date)),
        Err(error) => Err(invalid_value(field, error)),
    }
}

fn non_negative_u64(value: i64, field: &'static str) -> Result<u64, ScenarioError> {
    u64::try_from(value).map_or_else(
        |_| {
            Err(ScenarioError::InvalidValue {
                field,
                message: "must be non-negative".to_owned(),
            })
        },
        Ok,
    )
}

fn apparel_kind(value: &str) -> Result<ApparelKind, ScenarioError> {
    match value {
        "shirt" => Ok(ApparelKind::Shirt),
        "pants" => Ok(ApparelKind::Pants),
        "jacket" => Ok(ApparelKind::Jacket),
        "dress" => Ok(ApparelKind::Dress),
        "shoes" => Ok(ApparelKind::Shoes),
        "accessory" => Ok(ApparelKind::Accessory),
        _ => Err(ScenarioError::InvalidValue {
            field: "products.item_type",
            message: format!("unsupported apparel kind {value}"),
        }),
    }
}

fn size_label(value: &str) -> Result<SizeLabel, ScenarioError> {
    match value {
        "XS" => Ok(SizeLabel::Xs),
        "S" => Ok(SizeLabel::S),
        "M" => Ok(SizeLabel::M),
        "L" => Ok(SizeLabel::L),
        "XL" => Ok(SizeLabel::Xl),
        "XXL" => Ok(SizeLabel::Xxl),
        numeric => match numeric.parse::<u16>() {
            Ok(size) => Ok(SizeLabel::Numeric(size)),
            Err(error) => Err(invalid_value("products.size", error)),
        },
    }
}

fn invalid_value(field: &'static str, error: impl std::error::Error) -> ScenarioError {
    ScenarioError::InvalidValue {
        field,
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use uuid::Uuid;

    use super::{ScenarioError, ScenarioYamlLoader};

    const VALID_SCENARIO: &str = r#"
shop:
  start_date: "2026-06-09"
  capacity_space_units: 40
products:
  - sku: "TSH-ACME-M-BLK"
    item_type: "shirt"
    brand: "Acme"
    size: "M"
    unit_cost_cents: 1200
    unit_price_cents: 2999
    space_units: 2
    initial_on_hand: 10
    daily_demand_rate: "2.750"
    restock_lead_time_days: 4
    min_order_quantity: 6
    max_order_quantity: 36
"#;

    fn write_scenario(content: &str) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
        let path = std::env::temp_dir().join(format!("rigagent-scenario-{}.yaml", Uuid::new_v4()));
        fs::write(&path, content)?;
        Ok(path)
    }

    #[test]
    fn loads_valid_scenario_yaml() -> Result<(), Box<dyn std::error::Error>> {
        let path = write_scenario(VALID_SCENARIO)?;

        let state = ScenarioYamlLoader::load(&path)?;

        assert_eq!(state.products.len(), 1);
        assert_eq!(state.inventory.len(), 1);
        assert_eq!(state.capacity.units(), 40);
        Ok(())
    }

    #[test]
    fn rejects_duplicate_skus() -> Result<(), Box<dyn std::error::Error>> {
        let duplicate = VALID_SCENARIO.replace(
            "max_order_quantity: 36",
            "max_order_quantity: 36\n  - sku: \"tsh-acme-m-blk\"\n    item_type: \"shirt\"\n    brand: \"Acme\"\n    size: \"L\"\n    unit_cost_cents: 1200\n    unit_price_cents: 2999\n    space_units: 2\n    initial_on_hand: 1\n    daily_demand_rate: \"1.000\"\n    restock_lead_time_days: 4\n    min_order_quantity: 1\n    max_order_quantity: 10",
        );
        let path = write_scenario(&duplicate)?;

        let result = ScenarioYamlLoader::load(&path);

        assert!(matches!(result, Err(ScenarioError::DuplicateSku { .. })));
        Ok(())
    }

    #[test]
    fn rejects_empty_product_list() -> Result<(), Box<dyn std::error::Error>> {
        let path = write_scenario(
            r#"
shop:
  start_date: "2026-06-09"
  capacity_space_units: 40
products: []
"#,
        )?;

        let result = ScenarioYamlLoader::load(&path);

        assert!(matches!(result, Err(ScenarioError::EmptyProducts)));
        Ok(())
    }

    #[test]
    fn rejects_negative_values() -> Result<(), Box<dyn std::error::Error>> {
        let path =
            write_scenario(&VALID_SCENARIO.replace("initial_on_hand: 10", "initial_on_hand: -1"))?;

        let result = ScenarioYamlLoader::load(&path);

        assert!(matches!(result, Err(ScenarioError::InvalidValue { .. })));
        Ok(())
    }

    #[test]
    fn rejects_capacity_too_small_for_initial_stock() -> Result<(), Box<dyn std::error::Error>> {
        let path = write_scenario(
            &VALID_SCENARIO.replace("capacity_space_units: 40", "capacity_space_units: 1"),
        )?;

        let result = ScenarioYamlLoader::load(&path);

        assert!(matches!(
            result,
            Err(ScenarioError::CapacityTooSmall { .. })
        ));
        Ok(())
    }
}
