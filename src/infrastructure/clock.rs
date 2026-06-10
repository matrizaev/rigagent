//! Clock adapters for application date ports.

use std::time::{SystemTime, UNIX_EPOCH};

use chrono::NaiveDate;

use crate::application::retail::{ApplicationError, Clock};
use crate::domain::retail::SimulationDate;

const SECONDS_PER_DAY: u64 = 86_400;

/// UTC system-clock date adapter.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl SystemClock {
    /// Create a system clock adapter.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl Clock for SystemClock {
    fn today(&self) -> Result<SimulationDate, ApplicationError> {
        let duration = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|source| ApplicationError::clock_failure("read system clock", source))?;
        let days = duration
            .as_secs()
            .checked_div(SECONDS_PER_DAY)
            .ok_or_else(|| {
                ApplicationError::clock_failure(
                    "convert system time to date",
                    ClockAdapterError::InvalidDayDivisor,
                )
            })?;
        let epoch = NaiveDate::from_ymd_opt(1970, 1, 1).ok_or_else(|| {
            ApplicationError::clock_failure(
                "construct Unix epoch date",
                ClockAdapterError::InvalidEpochDate,
            )
        })?;

        Ok(SimulationDate::new(epoch).checked_add_days(days)?)
    }
}

#[derive(Debug, thiserror::Error)]
enum ClockAdapterError {
    #[error("seconds per day divisor must not be zero")]
    InvalidDayDivisor,
    #[error("Unix epoch date is not representable")]
    InvalidEpochDate,
}
