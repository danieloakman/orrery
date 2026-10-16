//! Conway's Game of Life. One tick is one generation.

use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sim_core::{ActionSpec, CallOutput, CallReturns, ParamSpec, Simulation};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifeConfig {
    pub seed: u64,
    pub width: u32,
    pub height: u32,
    #[serde(default = "default_density")]
    pub density: f64,
    #[serde(default)]
    pub readonly: bool,
    #[serde(default)]
    pub max_ticks: Option<u64>,
}

fn default_density() -> f64 {
    0.3
}

impl LifeConfig {
    fn validate(&self) -> Result<(), String> {
        if self.width == 0 || self.height == 0 || self.width > 1024 || self.height > 1024 {
            return Err("width and height must be 1..=1024".into());
        }
        if !(0.0..=1.0).contains(&self.density) {
            return Err("density must be 0.0..=1.0".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Life {
    config: LifeConfig,
    tick: u64,
    cells: Vec<bool>,
    rng: ChaCha8Rng,
}

impl Life {
    pub fn create(config: LifeConfig) -> Result<Self, String> {
        config.validate()?;
        let mut rng = ChaCha8Rng::seed_from_u64(config.seed);
        let n = (config.width as usize)
            .checked_mul(config.height as usize)
            .ok_or("grid is too large")?;
        let cells = (0..n).map(|_| rng.gen_bool(config.density)).collect();
        Ok(Self {
            config,
            tick: 0,
            cells,
            rng,
        })
    }

    pub fn from_config_value(config: Value) -> Result<Self, String> {
        let config: LifeConfig = serde_json::from_value(config).map_err(|e| e.to_string())?;
        Self::create(config)
    }

    pub fn from_sim_value(sim: Value) -> Result<Self, String> {
        serde_json::from_value(sim).map_err(|e| e.to_string())
    }

    fn index(&self, x: u32, y: u32) -> Result<usize, String> {
        if x >= self.config.width || y >= self.config.height {
            return Err(format!(
                "cell ({x},{y}) is outside {}x{}",
                self.config.width, self.config.height
            ));
        }
        Ok(y as usize * self.config.width as usize + x as usize)
    }

    fn generation(&mut self) {
        let w = self.config.width as i32;
        let h = self.config.height as i32;
        let mut next = vec![false; self.cells.len()];
        for y in 0..h {
            for x in 0..w {
                let mut n = 0;
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        if dx == 0 && dy == 0 {
                            continue;
                        }
                        let nx = x + dx;
                        let ny = y + dy;
                        if nx < 0 || ny < 0 || nx >= w || ny >= h {
                            continue;
                        }
                        let i = ny as usize * self.config.width as usize + nx as usize;
                        if self.cells[i] {
                            n += 1;
                        }
                    }
                }
                let i = y as usize * self.config.width as usize + x as usize;
                next[i] = matches!((self.cells[i], n), (true, 2 | 3) | (false, 3));
            }
        }
        self.cells = next;
        self.tick = self.tick.saturating_add(1);
    }

    fn grid_txt(&self) -> String {
        let mut out = String::new();
        for y in 0..self.config.height {
            for x in 0..self.config.width {
                let i = y as usize * self.config.width as usize + x as usize;
                out.push(if self.cells[i] { '#' } else { '.' });
            }
            out.push('\n');
        }
        out
    }
}

impl Simulation for Life {
    fn kind(&self) -> &'static str {
        "game_of_life"
    }

    fn tick(&self) -> u64 {
        self.tick
    }

    fn is_stopped(&self) -> bool {
        self.config.max_ticks.is_some_and(|max| self.tick >= max)
    }

    fn actions(&self) -> Vec<ActionSpec> {
        let mut actions = vec![ActionSpec::new(
            "get_grid",
            "Read the grid",
            false,
            CallReturns::Data,
            vec![],
        )];
        if !self.config.readonly {
            actions.push(ActionSpec::new(
                "set_cell",
                "Set one cell alive or dead",
                true,
                CallReturns::Void,
                vec![
                    ParamSpec {
                        name: "x".into(),
                        ty: "u64".into(),
                    },
                    ParamSpec {
                        name: "y".into(),
                        ty: "u64".into(),
                    },
                    ParamSpec {
                        name: "alive".into(),
                        ty: "bool".into(),
                    },
                ],
            ));
        }
        actions
    }

    fn query(&self, name: &str, _params: Value) -> Result<CallOutput, String> {
        match name {
            "get_grid" => Ok(CallOutput::Data(json!({
                "width": self.config.width,
                "height": self.config.height,
                "cells": self.cells,
            }))),
            other => Err(format!("unknown action {other}")),
        }
    }

    fn call(&mut self, name: &str, params: Value) -> Result<CallOutput, String> {
        match name {
            "set_cell" if !self.config.readonly => {
                let x = required_u64(&params, "x")?;
                let y = required_u64(&params, "y")?;
                let alive = params
                    .get("alive")
                    .and_then(Value::as_bool)
                    .ok_or("set_cell requires bool alive")?;
                let x = u32::try_from(x).map_err(|_| "x does not fit u32")?;
                let y = u32::try_from(y).map_err(|_| "y does not fit u32")?;
                let i = self.index(x, y)?;
                self.cells[i] = alive;
                Ok(CallOutput::Void)
            }
            other => Err(format!("unknown action {other}")),
        }
    }

    fn advance(&mut self, n: u64) -> Result<(), String> {
        for _ in 0..n {
            if self.is_stopped() {
                break;
            }
            self.generation();
        }
        Ok(())
    }

    fn status_summary(&self) -> Value {
        let alive = self.cells.iter().filter(|c| **c).count();
        json!({
            "width": self.config.width,
            "height": self.config.height,
            "alive": alive,
            "readonly": self.config.readonly,
            "max_ticks": self.config.max_ticks,
        })
    }

    fn sim_json(&self) -> Value {
        serde_json::to_value(self).expect("life serializes")
    }

    fn extras(&self) -> Value {
        json!({ "grid_txt": self.grid_txt() })
    }
}

fn required_u64(params: &Value, key: &str) -> Result<u64, String> {
    params
        .get(key)
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("set_cell requires u64 {key}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use sim_core::Simulation;

    fn hash(life: &Life) -> String {
        let bytes = serde_json::to_vec(life).unwrap();
        hex::encode(Sha256::digest(bytes))
    }

    fn cfg(seed: u64) -> LifeConfig {
        LifeConfig {
            seed,
            width: 16,
            height: 16,
            density: 0.4,
            readonly: false,
            max_ticks: None,
        }
    }

    #[test]
    fn same_seed_same_hash() {
        let mut a = Life::create(cfg(3)).unwrap();
        let mut b = Life::create(cfg(3)).unwrap();
        a.advance(20).unwrap();
        b.advance(20).unwrap();
        assert_eq!(hash(&a), hash(&b));
    }

    #[test]
    fn set_cell_then_advance_is_deterministic() {
        let mut a = Life::create(cfg(5)).unwrap();
        let mut b = Life::create(cfg(5)).unwrap();
        a.call("set_cell", json!({ "x": 1, "y": 2, "alive": true }))
            .unwrap();
        b.call("set_cell", json!({ "x": 1, "y": 2, "alive": true }))
            .unwrap();
        a.advance(4).unwrap();
        b.advance(4).unwrap();
        assert_eq!(hash(&a), hash(&b));
    }

    #[test]
    fn readonly_omits_set_cell() {
        let mut config = cfg(1);
        config.readonly = true;
        let mut life = Life::create(config).unwrap();
        assert!(life.actions().iter().all(|a| a.name != "set_cell"));
        assert!(life
            .call("set_cell", json!({ "x": 0, "y": 0, "alive": true }))
            .is_err());
    }

    #[test]
    fn max_ticks_stops_advance() {
        let mut config = cfg(1);
        config.max_ticks = Some(3);
        let mut life = Life::create(config).unwrap();
        life.advance(10).unwrap();
        assert_eq!(life.tick(), 3);
        assert!(life.is_stopped());
        life.advance(1).unwrap();
        assert_eq!(life.tick(), 3);
        assert!(life.query("get_grid", json!({})).is_ok());
    }

    #[test]
    fn restore_then_advance_matches_twin() {
        let mut live = Life::create(cfg(8)).unwrap();
        live.advance(6).unwrap();
        live.call("set_cell", json!({ "x": 0, "y": 0, "alive": true }))
            .unwrap();
        let mut restored = Life::from_sim_value(live.sim_json()).unwrap();
        live.advance(5).unwrap();
        restored.advance(5).unwrap();
        assert_eq!(hash(&live), hash(&restored));
    }
}
