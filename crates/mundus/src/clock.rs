use serde::{Deserialize, Serialize};

use crate::config::SimConfig;

/// One tick is always one minute. Not configurable.
pub const TICKS_PER_HOUR: u32 = 60;
/// 24 hours × 60 minutes.
pub const TICKS_PER_DAY: u32 = 24 * TICKS_PER_HOUR;
pub const HOURS_PER_DAY: u32 = 24;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Clock {
    /// Sole advanced counter. Unit: minutes.
    pub tick: u64,
    pub days_per_year: u32,
}

impl Clock {
    pub fn new(cfg: &SimConfig) -> Self {
        Self {
            tick: 0,
            days_per_year: cfg.days_per_year.max(1),
        }
    }

    pub fn advance(&mut self) {
        self.tick = self.tick.saturating_add(1);
    }

    pub fn day(&self) -> u64 {
        self.tick / u64::from(TICKS_PER_DAY)
    }

    /// Hour of day, 0..=23.
    pub fn hour(&self) -> u32 {
        ((self.tick / u64::from(TICKS_PER_HOUR)) % u64::from(HOURS_PER_DAY)) as u32
    }

    /// Minute of hour, 0..=59.
    pub fn minute(&self) -> u32 {
        (self.tick % u64::from(TICKS_PER_HOUR)) as u32
    }

    pub fn year(&self) -> u64 {
        self.day() / u64::from(self.days_per_year)
    }

    pub fn day_of_year(&self) -> u32 {
        (self.day() % u64::from(self.days_per_year)) as u32
    }

    pub fn ticks_per_year(&self) -> u64 {
        u64::from(self.days_per_year) * u64::from(TICKS_PER_DAY)
    }

    /// True on the first tick of a new year (after year 0 starts at tick 0).
    pub fn is_year_boundary(&self) -> bool {
        self.tick > 0 && self.tick.is_multiple_of(self.ticks_per_year())
    }
}
