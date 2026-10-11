use serde::{Deserialize, Serialize};

/// Initial world configuration. Fixed for the life of a world in the POC.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SimConfig {
    pub seed: u64,
    pub width: u32,
    pub height: u32,
    /// Subdivisions of a sim day. Default 24 = hourly.
    pub ticks_per_day: u32,
    pub days_per_year: u32,
    pub civ_count: u8,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            seed: 1,
            width: 128,
            height: 128,
            ticks_per_day: 24,
            days_per_year: 365,
            civ_count: 6,
        }
    }
}

impl SimConfig {
    pub fn ticks_per_year(&self) -> u64 {
        u64::from(self.days_per_year) * u64::from(self.ticks_per_day.max(1))
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.width < 8 || self.height < 8 {
            return Err("map must be at least 8x8".into());
        }
        if self.ticks_per_day == 0 {
            return Err("ticks_per_day must be >= 1".into());
        }
        if self.days_per_year == 0 {
            return Err("days_per_year must be >= 1".into());
        }
        if self.civ_count == 0 || self.civ_count > 16 {
            return Err("civ_count must be 1..=16".into());
        }
        Ok(())
    }
}
