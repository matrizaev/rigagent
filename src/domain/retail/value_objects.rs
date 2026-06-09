//! Retail value objects.

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use chrono::{Days, NaiveDate};

use super::DomainError;

const DEMAND_MILLIS_PER_UNIT: u64 = 1_000;
const MAX_DAY_COUNT: u64 = 3_650;

/// Canonical product SKU.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Sku(String);

impl Sku {
    /// Create a canonical uppercase SKU.
    ///
    /// # Errors
    ///
    /// Returns an error when the SKU is empty after trimming.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let trimmed = value.into().trim().to_uppercase();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyText { field: "sku" });
        }

        Ok(Self(trimmed))
    }

    /// Return the canonical SKU text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for Sku {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for Sku {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

/// Product brand display name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Brand(String);

impl Brand {
    /// Create a brand display name.
    ///
    /// # Errors
    ///
    /// Returns an error when the brand is empty after trimming.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        let trimmed = value.into().trim().to_owned();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyText { field: "brand" });
        }

        Ok(Self(trimmed))
    }

    /// Return the brand display name.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for Brand {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for Brand {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<String> for Brand {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// Closed set of apparel product categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApparelKind {
    /// Shirt or top.
    Shirt,
    /// Pants or trousers.
    Pants,
    /// Jacket or outerwear.
    Jacket,
    /// Dress.
    Dress,
    /// Shoes.
    Shoes,
    /// Accessory.
    Accessory,
}

/// Common apparel size labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SizeLabel {
    /// Extra small.
    Xs,
    /// Small.
    S,
    /// Medium.
    M,
    /// Large.
    L,
    /// Extra large.
    Xl,
    /// Double extra large.
    Xxl,
    /// Numeric size.
    Numeric(u16),
}

/// Non-negative money amount in cents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MoneyCents(u64);

impl MoneyCents {
    /// Create a money amount from cents.
    #[must_use]
    pub const fn new(cents: u64) -> Self {
        Self(cents)
    }

    /// Return the amount in cents.
    #[must_use]
    pub const fn cents(self) -> u64 {
        self.0
    }

    /// Checked addition.
    ///
    /// # Errors
    ///
    /// Returns an error on arithmetic overflow.
    pub fn checked_add(self, other: Self) -> Result<Self, DomainError> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(DomainError::ArithmeticOverflow {
                operation: "money addition",
            })
    }

    /// Checked subtraction.
    ///
    /// # Errors
    ///
    /// Returns an error when `other` is greater than `self`.
    pub fn checked_sub(self, other: Self) -> Result<Self, DomainError> {
        self.0
            .checked_sub(other.0)
            .map(Self)
            .ok_or(DomainError::InsufficientValue {
                operation: "money subtraction",
            })
    }

    /// Checked multiplication by a stock quantity.
    ///
    /// # Errors
    ///
    /// Returns an error on arithmetic overflow.
    pub fn checked_mul_quantity(self, quantity: StockQuantity) -> Result<Self, DomainError> {
        self.0
            .checked_mul(quantity.units())
            .map(Self)
            .ok_or(DomainError::ArithmeticOverflow {
                operation: "money by quantity",
            })
    }
}

impl Display for MoneyCents {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} cents", self.cents())
    }
}

/// Non-negative inventory capacity units.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SpaceUnits(u64);

impl SpaceUnits {
    /// Create capacity units.
    #[must_use]
    pub const fn new(units: u64) -> Self {
        Self(units)
    }

    /// Return the capacity units.
    #[must_use]
    pub const fn units(self) -> u64 {
        self.0
    }

    /// Checked addition.
    ///
    /// # Errors
    ///
    /// Returns an error on arithmetic overflow.
    pub fn checked_add(self, other: Self) -> Result<Self, DomainError> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(DomainError::ArithmeticOverflow {
                operation: "space addition",
            })
    }

    /// Checked multiplication by a stock quantity.
    ///
    /// # Errors
    ///
    /// Returns an error on arithmetic overflow.
    pub fn checked_mul_quantity(self, quantity: StockQuantity) -> Result<Self, DomainError> {
        self.0
            .checked_mul(quantity.units())
            .map(Self)
            .ok_or(DomainError::ArithmeticOverflow {
                operation: "space by quantity",
            })
    }
}

impl Display for SpaceUnits {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.units())
    }
}

/// Non-negative stock quantity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StockQuantity(u64);

impl StockQuantity {
    /// Create a stock quantity.
    #[must_use]
    pub const fn new(units: u64) -> Self {
        Self(units)
    }

    /// Return the quantity units.
    #[must_use]
    pub const fn units(self) -> u64 {
        self.0
    }

    /// Return whether this quantity is zero.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    /// Checked addition.
    ///
    /// # Errors
    ///
    /// Returns an error on arithmetic overflow.
    pub fn checked_add(self, other: Self) -> Result<Self, DomainError> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(DomainError::ArithmeticOverflow {
                operation: "stock addition",
            })
    }

    /// Checked subtraction.
    ///
    /// # Errors
    ///
    /// Returns an error when `other` is greater than `self`.
    pub fn checked_sub(self, other: Self) -> Result<Self, DomainError> {
        self.0
            .checked_sub(other.0)
            .map(Self)
            .ok_or(DomainError::InsufficientValue {
                operation: "stock subtraction",
            })
    }
}

impl Display for StockQuantity {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.units())
    }
}

/// Fixed-point daily demand rate in milli-units per day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DemandRatePerDay(u64);

impl DemandRatePerDay {
    /// Create a demand rate from milli-units per day.
    #[must_use]
    pub const fn from_milli_units(milli_units: u64) -> Self {
        Self(milli_units)
    }

    /// Return milli-units per day.
    #[must_use]
    pub const fn milli_units(self) -> u64 {
        self.0
    }
}

impl FromStr for DemandRatePerDay {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(DomainError::EmptyText {
                field: "daily_demand_rate",
            });
        }

        let (whole, fraction) = match trimmed.split_once('.') {
            Some((whole, fraction)) => (whole, fraction),
            None => (trimmed, ""),
        };

        if whole.is_empty()
            || !whole.chars().all(|character| character.is_ascii_digit())
            || !fraction.chars().all(|character| character.is_ascii_digit())
            || fraction.len() > 3
        {
            return Err(DomainError::InvalidText {
                field: "daily_demand_rate",
                value: trimmed.to_owned(),
            });
        }

        let whole_units = whole
            .parse::<u64>()
            .map_err(|_| DomainError::InvalidText {
                field: "daily_demand_rate",
                value: trimmed.to_owned(),
            })?
            .checked_mul(DEMAND_MILLIS_PER_UNIT)
            .ok_or(DomainError::ArithmeticOverflow {
                operation: "demand rate parse",
            })?;

        let fraction_units = parse_demand_fraction(fraction, trimmed)?;
        whole_units
            .checked_add(fraction_units)
            .map(Self)
            .ok_or(DomainError::ArithmeticOverflow {
                operation: "demand rate parse",
            })
    }
}

fn parse_demand_fraction(fraction: &str, original: &str) -> Result<u64, DomainError> {
    let parsed = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse::<u64>()
            .map_err(|_| DomainError::InvalidText {
                field: "daily_demand_rate",
                value: original.to_owned(),
            })?
    };

    match fraction.len() {
        0 => Ok(0),
        1 => parsed
            .checked_mul(100)
            .ok_or(DomainError::ArithmeticOverflow {
                operation: "demand fraction parse",
            }),
        2 => parsed
            .checked_mul(10)
            .ok_or(DomainError::ArithmeticOverflow {
                operation: "demand fraction parse",
            }),
        3 => Ok(parsed),
        _ => Err(DomainError::InvalidText {
            field: "daily_demand_rate",
            value: original.to_owned(),
        }),
    }
}

/// Fixed-point carried demand below one whole unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DemandBacklog(u64);

impl DemandBacklog {
    /// Empty demand backlog.
    pub const ZERO: Self = Self(0);

    /// Create a backlog from milli-units.
    ///
    /// # Errors
    ///
    /// Returns an error when the backlog is one unit or greater.
    pub const fn from_milli_units(milli_units: u64) -> Result<Self, DomainError> {
        if milli_units >= DEMAND_MILLIS_PER_UNIT {
            return Err(DomainError::InvalidDemandBacklog {
                backlog: Self(milli_units),
            });
        }

        Ok(Self(milli_units))
    }

    /// Return carried milli-units.
    #[must_use]
    pub const fn milli_units(self) -> u64 {
        self.0
    }
}

impl TryFrom<u64> for DemandBacklog {
    type Error = DomainError;

    fn try_from(milli_units: u64) -> Result<Self, Self::Error> {
        Self::from_milli_units(milli_units)
    }
}

/// Non-zero bounded supplier lead time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LeadTimeDays(u64);

impl LeadTimeDays {
    /// Create a bounded lead time.
    ///
    /// # Errors
    ///
    /// Returns an error when the value is zero or above the supported maximum.
    pub fn new(days: u64) -> Result<Self, DomainError> {
        bounded_non_zero_days(days, "restock_lead_time_days").map(Self)
    }

    /// Return the day count.
    #[must_use]
    pub const fn days(self) -> u64 {
        self.0
    }
}

impl TryFrom<u64> for LeadTimeDays {
    type Error = DomainError;

    fn try_from(days: u64) -> Result<Self, Self::Error> {
        Self::new(days)
    }
}

/// Non-zero bounded restock decision horizon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DecisionHorizonDays(u64);

impl DecisionHorizonDays {
    /// Create a bounded decision horizon.
    ///
    /// # Errors
    ///
    /// Returns an error when the value is zero or above the supported maximum.
    pub fn new(days: u64) -> Result<Self, DomainError> {
        bounded_non_zero_days(days, "decision_horizon_days").map(Self)
    }

    /// Return the day count.
    #[must_use]
    pub const fn days(self) -> u64 {
        self.0
    }
}

impl TryFrom<u64> for DecisionHorizonDays {
    type Error = DomainError;

    fn try_from(days: u64) -> Result<Self, Self::Error> {
        Self::new(days)
    }
}

const fn bounded_non_zero_days(days: u64, field: &'static str) -> Result<u64, DomainError> {
    if days == 0 {
        return Err(DomainError::NonPositive { field });
    }

    if days > MAX_DAY_COUNT {
        return Err(DomainError::AboveMaximum {
            field,
            maximum: MAX_DAY_COUNT,
        });
    }

    Ok(days)
}

/// Logical date used by the retail simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SimulationDate(NaiveDate);

impl SimulationDate {
    /// Create a simulation date from a `chrono` date.
    #[must_use]
    pub const fn new(date: NaiveDate) -> Self {
        Self(date)
    }

    /// Return the backing `chrono` date.
    #[must_use]
    pub const fn as_naive_date(self) -> NaiveDate {
        self.0
    }

    /// Add a bounded number of days.
    ///
    /// # Errors
    ///
    /// Returns an error when the date exceeds chrono's supported range.
    pub fn checked_add_days(self, days: u64) -> Result<Self, DomainError> {
        self.0
            .checked_add_days(Days::new(days))
            .map(Self)
            .ok_or(DomainError::DateOverflow { date: self })
    }
}

impl Display for SimulationDate {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.as_naive_date())
    }
}

/// Identifier for a simulated sales order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SalesOrderId(String);

impl SalesOrderId {
    /// Create a validated sales order identifier.
    ///
    /// # Errors
    ///
    /// Returns an error when the identifier is empty after trimming.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        non_empty_id(value, "sales_order_id").map(Self)
    }

    /// Return the identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for SalesOrderId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for SalesOrderId {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<String> for SalesOrderId {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// Identifier for a supplier restock order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RestockOrderId(String);

impl RestockOrderId {
    /// Create a validated restock order identifier.
    ///
    /// # Errors
    ///
    /// Returns an error when the identifier is empty after trimming.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        non_empty_id(value, "restock_order_id").map(Self)
    }

    /// Return the identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for RestockOrderId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for RestockOrderId {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<String> for RestockOrderId {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// Identifier for a restock decision run.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DecisionRunId(String);

impl DecisionRunId {
    /// Create a validated decision run identifier.
    ///
    /// # Errors
    ///
    /// Returns an error when the identifier is empty after trimming.
    pub fn new(value: impl Into<String>) -> Result<Self, DomainError> {
        non_empty_id(value, "decision_run_id").map(Self)
    }

    /// Return the identifier text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for DecisionRunId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for DecisionRunId {
    type Err = DomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

impl TryFrom<String> for DecisionRunId {
    type Error = DomainError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

fn non_empty_id(value: impl Into<String>, field: &'static str) -> Result<String, DomainError> {
    let trimmed = value.into().trim().to_owned();
    if trimmed.is_empty() {
        return Err(DomainError::EmptyText { field });
    }

    Ok(trimmed)
}

#[cfg(test)]
mod tests {
    use super::{DemandBacklog, DemandRatePerDay, Sku};

    #[test]
    fn rejects_empty_sku() {
        assert!(Sku::new("   ").is_err());
    }

    #[test]
    fn parses_decimal_demand_rate_without_float_arithmetic()
    -> Result<(), Box<dyn std::error::Error>> {
        let rate: DemandRatePerDay = "1.250".parse()?;

        assert_eq!(rate.milli_units(), 1_250);
        Ok(())
    }

    #[test]
    fn rejects_full_unit_demand_backlog() {
        assert!(DemandBacklog::from_milli_units(1_000).is_err());
    }
}
