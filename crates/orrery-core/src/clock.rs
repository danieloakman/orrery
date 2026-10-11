use serde::{Deserialize, Serialize};

use crate::config::SimConfig;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Clock {
    /// Sole advanced counter.
    pub tick: u64,
    pub ticks_per_day: u32,
    pub days_per_year: u32,
}

impl Clock {
    pub fn new(cfg: &SimConfig) -> Self {
        Self {
            tick: 0,
            ticks_per_day: cfg.ticks_per_day.max(1),
            days_per_year: cfg.days_per_year.max(1),
        }
    }

    pub fn advance(&mut self) {
        self.tick = self.tick.saturating_add(1);
    }

    pub fn day(&self) -> u64 {
        self.tick / u64::from(self.ticks_per_day)
    }

    pub fn hour(&self) -> u32 {
        (self.tick % u64::from(self.ticks_per_day)) as u32
    }

    pub fn year(&self) -> u64 {
        self.day() / u64::from(self.days_per_year)
    }

    pub fn day_of_year(&self) -> u32 {
        (self.day() % u64::from(self.days_per_year)) as u32
    }

    /// True on the first tick of a new year (after year 0 starts at tick 0).
    pub fn is_year_boundary(&self) -> bool {
        self.tick > 0 && self.tick % (u64::from(self.days_per_year) * u64::from(self.ticks_per_day)) == 0
    }
}
