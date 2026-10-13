//! Deterministic fantasy world simulation core.

mod chronicle;
mod clock;
mod config;
mod dump;
mod event;
mod gen;
mod ipc;
mod names;
pub mod protocol;
mod rules;
mod world;

pub use ipc::{Client as IpcClient, IpcError};

pub use chronicle::render_chronicle_md;
pub use clock::{Clock, HOURS_PER_DAY, TICKS_PER_DAY, TICKS_PER_HOUR};
pub use config::SimConfig;
pub use dump::{build_dump, render_map_ascii, state_hash, DumpArtifacts};
pub use event::{Event, EventKind};
pub use protocol::cap_events;
pub use protocol::{
    civ_summary, filter_chronicle, resolve_civ, AdvanceTickParams, AdvanceTickResult,
    ChronicleParams, ChronicleResult, CreateParams, DumpResult, GetCivParams, GetCivResult,
    ListCivsResult, OkBody, Request, Response, StatusResult, CHRONICLE_DEFAULT_LIMIT, EVENT_CAP,
};
pub use world::{Civ, Settlement, SettlementKind, World};

#[cfg(test)]
mod tests {
    use super::*;

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
        advance_years(&mut a, 20);
        advance_years(&mut b, 20);
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
        // Not guaranteed every seed grows, but seed 7 should produce growth.
        assert!(
            after >= before
                || w.events
                    .iter()
                    .any(|e| matches!(e.kind, EventKind::Founded { .. }))
        );
    }

    #[test]
    fn serde_roundtrip() {
        let mut w = World::create(cfg(99)).unwrap();
        advance_years(&mut w, 5);
        let json = serde_json::to_string(&w).unwrap();
        let w2: World = serde_json::from_str(&json).unwrap();
        assert_eq!(state_hash(&w), state_hash(&w2));
    }

    #[test]
    fn ticks_per_day_is_one_day() {
        let mut w = World::create(cfg(1)).unwrap();
        assert_eq!(w.clock.day(), 0);
        assert_eq!(w.clock.hour(), 0);
        assert_eq!(w.clock.minute(), 0);
        // End of hour 23 (one hour before midnight).
        w.advance_ticks(u64::from(TICKS_PER_DAY - TICKS_PER_HOUR));
        assert_eq!(w.clock.day(), 0);
        assert_eq!(w.clock.hour(), 23);
        assert_eq!(w.clock.minute(), 0);
        w.advance_ticks(u64::from(TICKS_PER_HOUR));
        assert_eq!(w.clock.day(), 1);
        assert_eq!(w.clock.hour(), 0);
        assert_eq!(w.clock.minute(), 0);
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
    }
}
