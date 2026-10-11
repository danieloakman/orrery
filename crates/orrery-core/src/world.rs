use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::clock::Clock;
use crate::config::SimConfig;
use crate::event::{Event, EventKind};
use crate::gen::{generate_map, land_tiles, Map};
use crate::names::culture_name;
use crate::rules::{self, SystemSchedule};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SettlementKind {
    Capital,
    Village,
    Ruin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settlement {
    pub x: u32,
    pub y: u32,
    pub kind: SettlementKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Civ {
    pub id: u32,
    pub name: String,
    pub alive: bool,
    pub population: u32,
    pub food: f32,
    pub tech: f32,
    pub strength: f32,
    pub settlements: Vec<Settlement>,
}

fn default_rng() -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(0)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct World {
    pub config: SimConfig,
    pub clock: Clock,
    pub map: Map,
    pub civs: Vec<Civ>,
    pub events: Vec<Event>,
    pub schedule: SystemSchedule,
    /// Live RNG; skipped in serde dumps (reseeded to dummy on load).
    #[serde(skip, default = "default_rng")]
    pub rng: ChaCha8Rng,
}

#[derive(Debug, Error)]
pub enum WorldError {
    #[error("{0}")]
    Config(String),
    #[error("not enough hospitable land to place civs")]
    NoLand,
}

impl World {
    pub fn create(config: SimConfig) -> Result<Self, WorldError> {
        config.validate().map_err(WorldError::Config)?;
        let mut rng = ChaCha8Rng::seed_from_u64(config.seed);
        let map = generate_map(&mut rng, config.width, config.height);
        let mut land = land_tiles(&map);
        if land.len() < config.civ_count as usize {
            return Err(WorldError::NoLand);
        }
        for i in (1..land.len()).rev() {
            let j = rng.gen_range(0..=i);
            land.swap(i, j);
        }

        let mut civs = Vec::new();
        let mut used_names = std::collections::BTreeSet::new();
        for id in 0..config.civ_count {
            let mut name = culture_name(&mut rng);
            while !used_names.insert(name.clone()) {
                name = culture_name(&mut rng);
            }
            let (x, y) = land[id as usize];
            let food = map.biome(x, y).food_mult() * 10.0;
            civs.push(Civ {
                id: id as u32,
                name,
                alive: true,
                population: 80 + rng.gen_range(0..40),
                food,
                tech: rng.gen_range(0.8..1.4),
                strength: rng.gen_range(8.0..14.0),
                settlements: vec![Settlement {
                    x,
                    y,
                    kind: SettlementKind::Capital,
                }],
            });
        }

        let clock = Clock::new(&config);
        let mut world = Self {
            config,
            clock,
            map,
            civs,
            events: Vec::new(),
            schedule: SystemSchedule::new(),
            rng,
        };
        let civs_n = world.civs.len() as u8;
        world.push_event(EventKind::WorldBorn { civs: civs_n });
        for c in world.civs.clone() {
            let cap = &c.settlements[0];
            world.push_event(EventKind::Founded {
                civ: c.name.clone(),
                x: cap.x,
                y: cap.y,
            });
        }
        Ok(world)
    }

    pub fn push_event(&mut self, kind: EventKind) {
        self.events.push(Event {
            tick: self.clock.tick,
            day: self.clock.day(),
            year: self.clock.year(),
            kind,
        });
    }

    /// Sole mutation path for a single history tick.
    pub fn step(&mut self) -> Vec<Event> {
        let start = self.events.len();
        rules::run_due(self);
        self.clock.advance();
        self.events[start..].to_vec()
    }

    pub fn advance_ticks(&mut self, n: u64) -> Vec<Event> {
        let start = self.events.len();
        for _ in 0..n {
            rules::run_due(self);
            self.clock.advance();
        }
        self.events[start..].to_vec()
    }

    pub fn alive_civ_count(&self) -> usize {
        self.civs.iter().filter(|c| c.alive).count()
    }

    pub fn settlement_owner(&self, x: u32, y: u32) -> Option<u32> {
        for c in &self.civs {
            if !c.alive {
                continue;
            }
            if c.settlements
                .iter()
                .any(|s| s.x == x && s.y == y && s.kind != SettlementKind::Ruin)
            {
                return Some(c.id);
            }
        }
        None
    }
}
