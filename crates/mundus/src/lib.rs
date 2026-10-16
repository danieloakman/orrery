//! Fantasy civilisation simulation.

mod chronicle;
mod clock;
mod config;
mod dump;
mod event;
mod gen;
mod names;
mod rules;
mod world;

pub use clock::{Clock, HOURS_PER_DAY, TICKS_PER_DAY, TICKS_PER_HOUR};
pub use config::SimConfig;
pub use dump::{render_map_ascii, state_hash};
pub use event::{Event, EventKind};
pub use world::{Civ, Settlement, SettlementKind, World, WorldError};

use serde_json::{json, Value};
use sim_core::{ActionSpec, CallOutput, CallReturns, ParamSpec, Simulation};

const CHRONICLE_DEFAULT_LIMIT: usize = 50;

impl Simulation for World {
    fn kind(&self) -> &'static str {
        "mundus"
    }

    fn tick(&self) -> u64 {
        self.clock.tick
    }

    fn is_stopped(&self) -> bool {
        false
    }

    fn actions(&self) -> Vec<ActionSpec> {
        vec![
            ActionSpec::new(
                "list_civs",
                "Summaries of every civilisation",
                false,
                CallReturns::Data,
                vec![],
            ),
            ActionSpec::new(
                "get_civ",
                "One civilisation by id or exact name",
                false,
                CallReturns::Data,
                vec![ParamSpec {
                    name: "query".into(),
                    ty: "string".into(),
                }],
            ),
            ActionSpec::new(
                "chronicle",
                "Events at or after a tick",
                false,
                CallReturns::Data,
                vec![
                    ParamSpec {
                        name: "since_tick".into(),
                        ty: "u64".into(),
                    },
                    ParamSpec {
                        name: "limit".into(),
                        ty: "u64".into(),
                    },
                ],
            ),
        ]
    }

    fn query(&self, name: &str, params: Value) -> Result<CallOutput, String> {
        match name {
            "list_civs" => Ok(CallOutput::Data(list_civs(self))),
            "get_civ" => Ok(CallOutput::Data(get_civ(self, &params)?)),
            "chronicle" => Ok(CallOutput::Data(chronicle(self, &params)?)),
            other => Err(format!("unknown action {other}")),
        }
    }

    fn call(&mut self, name: &str, _params: Value) -> Result<CallOutput, String> {
        Err(format!("action {name} is read-only"))
    }

    fn advance(&mut self, n: u64) -> Result<(), String> {
        self.advance_ticks(n);
        Ok(())
    }

    fn status_summary(&self) -> Value {
        json!({
            "day": self.clock.day(),
            "year": self.clock.year(),
            "hour": self.clock.hour(),
            "minute": self.clock.minute(),
            "days_per_year": self.config.days_per_year,
            "civ_count": self.civs.len(),
            "alive_civs": self.alive_civ_count(),
            "event_count": self.events.len(),
        })
    }

    fn sim_json(&self) -> Value {
        serde_json::to_value(self).expect("world serializes")
    }

    fn extras(&self) -> Value {
        json!({
            "chronicle_md": chronicle::render_chronicle_md(self),
            "map_txt": dump::render_map_ascii(self),
        })
    }
}

impl World {
    pub fn from_config_value(config: Value) -> Result<Self, String> {
        let config: SimConfig = serde_json::from_value(config).map_err(|e| e.to_string())?;
        Self::create(config).map_err(|e| e.to_string())
    }

    pub fn from_sim_value(sim: Value) -> Result<Self, String> {
        serde_json::from_value(sim).map_err(|e| e.to_string())
    }
}

fn list_civs(world: &World) -> Value {
    let mut civs: Vec<_> = world.civs.iter().map(civ_summary).collect();
    civs.sort_by_key(|c| c["id"].as_u64().unwrap_or(0));
    json!({ "tick": world.clock.tick, "civs": civs })
}

fn civ_summary(civ: &Civ) -> Value {
    json!({
        "id": civ.id,
        "name": civ.name,
        "alive": civ.alive,
        "population": civ.population,
        "food": civ.food,
        "tech": civ.tech,
        "strength": civ.strength,
        "settlements": civ.settlements.len(),
    })
}

fn get_civ(world: &World, params: &Value) -> Result<Value, String> {
    let query = params
        .get("query")
        .and_then(Value::as_str)
        .ok_or("get_civ requires string query")?;
    let civ = resolve_civ(&world.civs, query)?;
    Ok(json!({ "tick": world.clock.tick, "civ": civ }))
}

fn chronicle(world: &World, params: &Value) -> Result<Value, String> {
    let since_tick = optional_u64(params, "since_tick")?.unwrap_or(0);
    let limit = optional_u64(params, "limit")?
        .map(|n| usize::try_from(n).unwrap_or(usize::MAX))
        .unwrap_or(CHRONICLE_DEFAULT_LIMIT)
        .max(1);
    let matching: Vec<_> = world
        .events
        .iter()
        .filter(|e| e.tick >= since_tick)
        .collect();
    let total_matching = matching.len();
    let truncated = total_matching > limit;
    let events: Vec<_> = matching.into_iter().take(limit).cloned().collect();
    Ok(json!({
        "tick": world.clock.tick,
        "since_tick": since_tick,
        "events": events,
        "total_matching": total_matching,
        "truncated": truncated,
    }))
}

fn optional_u64(params: &Value, key: &str) -> Result<Option<u64>, String> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("{key} must be a u64")),
        Some(_) => Err(format!("{key} must be a u64")),
    }
}

fn resolve_civ<'a>(civs: &'a [Civ], query: &str) -> Result<&'a Civ, String> {
    let q = query.trim();
    if q.is_empty() {
        return Err("empty civ query".into());
    }
    if let Ok(id) = q.parse::<u32>() {
        if let Some(c) = civs.iter().find(|c| c.id == id) {
            return Ok(c);
        }
        return Err(format!("no civ with id {id}"));
    }
    let lower = q.to_ascii_lowercase();
    let matches: Vec<_> = civs
        .iter()
        .filter(|c| c.name.to_ascii_lowercase() == lower)
        .collect();
    match matches.as_slice() {
        [c] => Ok(c),
        [] => Err(format!("no civ named {q:?}")),
        _ => Err(format!("ambiguous civ name {q:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sim_core::Simulation;

    fn cfg(seed: u64) -> SimConfig {
        SimConfig {
            seed,
            width: 64,
            height: 64,
            days_per_year: 365,
            civ_count: 5,
        }
    }

    fn advance_years(world: &mut World, years: u64) {
        let n = years * world.config.ticks_per_year();
        world.advance_ticks(n);
    }

    #[test]
    fn same_seed_same_hash() {
        let mut a = World::create(cfg(42)).unwrap();
        let mut b = World::create(cfg(42)).unwrap();
        let n = 20 * a.config.ticks_per_year();
        Simulation::advance(&mut a, n).unwrap();
        Simulation::advance(&mut b, n).unwrap();
        assert_eq!(state_hash(&a), state_hash(&b));
        assert_eq!(a.events.len(), b.events.len());
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = World::create(cfg(1)).unwrap();
        let mut b = World::create(cfg(2)).unwrap();
        advance_years(&mut a, 30);
        advance_years(&mut b, 30);
        assert_ne!(state_hash(&a), state_hash(&b));
    }

    #[test]
    fn growth_increases_population_over_years() {
        let mut w = World::create(cfg(7)).unwrap();
        let before: u32 = w.civs.iter().map(|c| c.population).sum();
        advance_years(&mut w, 10);
        let after: u32 = w
            .civs
            .iter()
            .filter(|c| c.alive)
            .map(|c| c.population)
            .sum();
        assert!(
            after >= before
                || w.events
                    .iter()
                    .any(|e| matches!(e.kind, EventKind::Founded { .. }))
        );
    }

    #[test]
    fn serde_roundtrip_keeps_rng() {
        let mut w = World::create(cfg(99)).unwrap();
        advance_years(&mut w, 5);
        let json = serde_json::to_string(&w).unwrap();
        let mut w2: World = serde_json::from_str(&json).unwrap();
        assert_eq!(state_hash(&w), state_hash(&w2));
        advance_years(&mut w, 3);
        advance_years(&mut w2, 3);
        assert_eq!(state_hash(&w), state_hash(&w2));
    }

    #[test]
    fn restore_then_advance_matches_twin() {
        let mut live = World::create(cfg(11)).unwrap();
        advance_years(&mut live, 4);
        let saved = live.sim_json();
        let mut restored = World::from_sim_value(saved).unwrap();
        advance_years(&mut live, 2);
        advance_years(&mut restored, 2);
        assert_eq!(state_hash(&live), state_hash(&restored));
    }

    #[test]
    fn ticks_per_day_is_one_day() {
        let mut w = World::create(cfg(1)).unwrap();
        assert_eq!(w.clock.day(), 0);
        w.advance_ticks(u64::from(TICKS_PER_DAY - TICKS_PER_HOUR));
        assert_eq!(w.clock.day(), 0);
        assert_eq!(w.clock.hour(), 23);
        w.advance_ticks(u64::from(TICKS_PER_HOUR));
        assert_eq!(w.clock.day(), 1);
        assert_eq!(w.clock.hour(), 0);
    }

    #[test]
    fn server_never_auto_stops_when_civs_die() {
        let mut w = World::create(cfg(3)).unwrap();
        for c in &mut w.civs {
            c.alive = false;
            c.population = 0;
            c.strength = 0.0;
        }
        advance_years(&mut w, 2);
        assert_eq!(w.clock.year(), 2);
        assert!(!Simulation::is_stopped(&w));
    }
}
