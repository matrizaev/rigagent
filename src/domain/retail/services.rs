//! Retail domain services.

use super::{
    DecisionHorizonDays, DemandBacklog, DemandRatePerDay, DomainError, InventoryPosition,
    MoneyCents, Product, SimulationDate, SpaceUnits, StockQuantity,
};

const DEMAND_MILLIS_PER_UNIT: u64 = 1_000;

/// Result of simulating demand for one day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DemandSimulation {
    /// Whole requested units for the day.
    pub requested_units: StockQuantity,
    /// Fractional demand carried to the next day.
    pub remaining_backlog: DemandBacklog,
}

/// Deterministic demand simulator.
#[derive(Debug, Clone, Copy, Default)]
pub struct DemandSimulator;

impl DemandSimulator {
    /// Convert a demand rate and backlog into whole requested units and remaining backlog.
    ///
    /// # Errors
    ///
    /// Returns an error on arithmetic overflow or invalid backlog units.
    pub fn simulate_day(
        rate: DemandRatePerDay,
        backlog: DemandBacklog,
    ) -> Result<DemandSimulation, DomainError> {
        let total_milli_units = rate
            .milli_units()
            .checked_add(backlog.milli_units())
            .ok_or(DomainError::ArithmeticOverflow {
                operation: "daily demand simulation",
            })?;

        let requested_units = total_milli_units
            .checked_div(DEMAND_MILLIS_PER_UNIT)
            .map(StockQuantity::new)
            .ok_or(DomainError::ArithmeticOverflow {
                operation: "daily demand whole units",
            })?;
        let remaining_milli_units = total_milli_units
            .checked_rem(DEMAND_MILLIS_PER_UNIT)
            .ok_or(DomainError::ArithmeticOverflow {
                operation: "daily demand backlog",
            })?;

        Ok(DemandSimulation {
            requested_units,
            remaining_backlog: DemandBacklog::from_milli_units(remaining_milli_units)?,
        })
    }
}

/// Inputs for scoring a candidate restock option.
#[derive(Debug, Clone)]
pub struct RestockOptionRequest {
    /// Product being scored.
    pub product: Product,
    /// Current inventory position.
    pub inventory: InventoryPosition,
    /// Currently open inbound stock for this SKU.
    pub open_inbound_quantity: StockQuantity,
    /// Total available stock-space capacity.
    pub capacity: SpaceUnits,
    /// Capacity already occupied by other SKUs and open inbound orders.
    pub reserved_capacity: SpaceUnits,
    /// Decision date.
    pub current_date: SimulationDate,
    /// Decision horizon.
    pub horizon: DecisionHorizonDays,
}

/// Ranked candidate restock option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestockOption {
    sku: super::Sku,
    quantity: StockQuantity,
    expected_incremental_units: StockQuantity,
    expected_profit: MoneyCents,
    occupied_space: SpaceUnits,
}

impl RestockOption {
    /// Return option SKU.
    #[must_use]
    pub const fn sku(&self) -> &super::Sku {
        &self.sku
    }

    /// Return suggested quantity.
    #[must_use]
    pub const fn quantity(&self) -> StockQuantity {
        self.quantity
    }

    /// Return expected incremental units sold.
    #[must_use]
    pub const fn expected_incremental_units(&self) -> StockQuantity {
        self.expected_incremental_units
    }

    /// Return expected gross profit.
    #[must_use]
    pub const fn expected_profit(&self) -> MoneyCents {
        self.expected_profit
    }

    /// Return occupied stock-space units.
    #[must_use]
    pub const fn occupied_space(&self) -> SpaceUnits {
        self.occupied_space
    }
}

/// Deterministic restock option scorer.
#[derive(Debug, Clone, Copy, Default)]
pub struct RestockOptionScorer;

impl RestockOptionScorer {
    /// Score one candidate restock using deterministic inventory economics.
    ///
    /// # Errors
    ///
    /// Returns an error on arithmetic overflow, invalid product bounds, or capacity overflow.
    pub fn score(request: &RestockOptionRequest) -> Result<Option<RestockOption>, DomainError> {
        if !request.product.is_active() {
            return Ok(None);
        }

        let eta = request.product.restock_eta(request.current_date)?;
        if eta
            > request
                .current_date
                .checked_add_days(request.horizon.days())?
        {
            return Ok(None);
        }

        let projected_units = request
            .inventory
            .on_hand()
            .checked_add(request.open_inbound_quantity)?;
        let projected_space_for_sku = request
            .product
            .unit_space()
            .checked_mul_quantity(projected_units)?;
        let projected_space = request
            .reserved_capacity
            .checked_add(projected_space_for_sku)?;
        if projected_space > request.capacity {
            return Err(DomainError::CapacityExceeded {
                requested: projected_space,
                capacity: request.capacity,
            });
        }

        let free_space_units = request
            .capacity
            .units()
            .checked_sub(projected_space.units())
            .ok_or(DomainError::InsufficientValue {
                operation: "available restock capacity",
            })?;
        let unit_space = request.product.unit_space().units();
        if unit_space == 0 {
            return Ok(None);
        }

        let capacity_limited_quantity = free_space_units
            .checked_div(unit_space)
            .map(StockQuantity::new)
            .ok_or(DomainError::ArithmeticOverflow {
                operation: "capacity-limited quantity",
            })?;

        if capacity_limited_quantity.is_zero() {
            return Ok(None);
        }

        let requested_quantity = if capacity_limited_quantity > request.product.max_order_quantity()
        {
            request.product.max_order_quantity()
        } else {
            capacity_limited_quantity
        };

        if requested_quantity < request.product.min_order_quantity() {
            return Ok(None);
        }

        let requested_quantity = request.product.bounded_order_quantity(requested_quantity)?;
        let demand_limited_quantity = demand_within_horizon(
            request.product.demand_rate(),
            request.horizon,
            request.inventory.demand_backlog(),
        )?;
        let incremental_units = if requested_quantity > demand_limited_quantity {
            demand_limited_quantity
        } else {
            requested_quantity
        };

        if incremental_units.is_zero() {
            return Ok(None);
        }

        let expected_profit = request
            .product
            .unit_margin()?
            .checked_mul_quantity(incremental_units)?;
        let occupied_space = request
            .product
            .unit_space()
            .checked_mul_quantity(requested_quantity)?;

        Ok(Some(RestockOption {
            sku: request.product.sku().clone(),
            quantity: requested_quantity,
            expected_incremental_units: incremental_units,
            expected_profit,
            occupied_space,
        }))
    }
}

fn demand_within_horizon(
    rate: DemandRatePerDay,
    horizon: DecisionHorizonDays,
    backlog: DemandBacklog,
) -> Result<StockQuantity, DomainError> {
    let horizon_demand =
        rate.milli_units()
            .checked_mul(horizon.days())
            .ok_or(DomainError::ArithmeticOverflow {
                operation: "horizon demand",
            })?;
    let total = horizon_demand.checked_add(backlog.milli_units()).ok_or(
        DomainError::ArithmeticOverflow {
            operation: "horizon demand with backlog",
        },
    )?;
    total
        .checked_div(DEMAND_MILLIS_PER_UNIT)
        .map(StockQuantity::new)
        .ok_or(DomainError::ArithmeticOverflow {
            operation: "horizon whole demand",
        })
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::{
        DemandBacklog, DemandRatePerDay, DemandSimulator, RestockOptionRequest, RestockOptionScorer,
    };
    use crate::domain::retail::{
        ApparelKind, Brand, DecisionHorizonDays, InventoryPosition, LeadTimeDays, MoneyCents,
        Product, ProductDetails, SimulationDate, SizeLabel, Sku, SpaceUnits, StockQuantity,
    };

    fn product() -> Result<Product, Box<dyn std::error::Error>> {
        Ok(Product::from_details(ProductDetails {
            sku: Sku::new("shirt-1")?,
            kind: ApparelKind::Shirt,
            brand: Brand::new("North")?,
            size: SizeLabel::M,
            unit_cost: MoneyCents::new(1_000),
            unit_price: MoneyCents::new(2_500),
            unit_space: SpaceUnits::new(2),
            demand_rate: "1.250".parse::<DemandRatePerDay>()?,
            lead_time: LeadTimeDays::new(1)?,
            min_order_quantity: StockQuantity::new(1),
            max_order_quantity: StockQuantity::new(50),
            active: true,
        })?)
    }

    #[test]
    fn carries_fractional_demand_backlog() -> Result<(), Box<dyn std::error::Error>> {
        let rate: DemandRatePerDay = "1.250".parse()?;

        let day_one = DemandSimulator::simulate_day(rate, DemandBacklog::ZERO)?;
        let day_two = DemandSimulator::simulate_day(rate, day_one.remaining_backlog)?;

        assert_eq!(day_one.requested_units.units(), 1);
        assert_eq!(day_one.remaining_backlog.milli_units(), 250);
        assert_eq!(day_two.requested_units.units(), 1);
        assert_eq!(day_two.remaining_backlog.milli_units(), 500);
        Ok(())
    }

    #[test]
    fn scores_restock_option_inside_capacity() -> Result<(), Box<dyn std::error::Error>> {
        let product = product()?;
        let date = NaiveDate::from_ymd_opt(2026, 6, 9).ok_or("valid test date")?;
        let option = RestockOptionScorer::score(&RestockOptionRequest {
            product: product.clone(),
            inventory: InventoryPosition::new(
                product.sku().clone(),
                StockQuantity::new(5),
                DemandBacklog::ZERO,
            ),
            open_inbound_quantity: StockQuantity::new(0),
            capacity: SpaceUnits::new(60),
            reserved_capacity: SpaceUnits::new(0),
            current_date: SimulationDate::new(date),
            horizon: DecisionHorizonDays::new(14)?,
        })?
        .ok_or("expected restock option")?;

        assert_eq!(option.sku(), product.sku());
        assert!(option.expected_profit().cents() > 0);
        Ok(())
    }

    #[test]
    fn caps_restock_option_at_product_max_when_capacity_is_larger()
    -> Result<(), Box<dyn std::error::Error>> {
        let product = product()?;
        let date = NaiveDate::from_ymd_opt(2026, 6, 9).ok_or("valid test date")?;
        let option = RestockOptionScorer::score(&RestockOptionRequest {
            product: product.clone(),
            inventory: InventoryPosition::new(
                product.sku().clone(),
                StockQuantity::new(0),
                DemandBacklog::ZERO,
            ),
            open_inbound_quantity: StockQuantity::new(0),
            capacity: SpaceUnits::new(1_000),
            reserved_capacity: SpaceUnits::new(0),
            current_date: SimulationDate::new(date),
            horizon: DecisionHorizonDays::new(14)?,
        })?
        .ok_or("expected restock option")?;

        assert_eq!(option.quantity(), product.max_order_quantity());
        Ok(())
    }

    #[test]
    fn caps_restock_option_by_reserved_capacity() -> Result<(), Box<dyn std::error::Error>> {
        let product = Product::from_details(ProductDetails {
            sku: Sku::new("shirt-1")?,
            kind: ApparelKind::Shirt,
            brand: Brand::new("North")?,
            size: SizeLabel::M,
            unit_cost: MoneyCents::new(1_000),
            unit_price: MoneyCents::new(2_500),
            unit_space: SpaceUnits::new(2),
            demand_rate: DemandRatePerDay::from_milli_units(50_000),
            lead_time: LeadTimeDays::new(1)?,
            min_order_quantity: StockQuantity::new(1),
            max_order_quantity: StockQuantity::new(50),
            active: true,
        })?;
        let date = NaiveDate::from_ymd_opt(2026, 6, 9).ok_or("valid test date")?;

        let option = RestockOptionScorer::score(&RestockOptionRequest {
            product: product.clone(),
            inventory: InventoryPosition::new(
                product.sku().clone(),
                StockQuantity::new(0),
                DemandBacklog::ZERO,
            ),
            open_inbound_quantity: StockQuantity::new(0),
            capacity: SpaceUnits::new(100),
            reserved_capacity: SpaceUnits::new(94),
            current_date: SimulationDate::new(date),
            horizon: DecisionHorizonDays::new(14)?,
        })?
        .ok_or("expected restock option")?;

        assert_eq!(option.quantity(), StockQuantity::new(3));
        assert_eq!(option.occupied_space(), SpaceUnits::new(6));
        Ok(())
    }
}
