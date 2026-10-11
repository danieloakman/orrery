use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::event::EventKind;
use crate::world::{Settlement, SettlementKind, World};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSchedule {
    pub last_yearly_tick: Option<u64>,
}

impl SystemSchedule {
    pub fn new() -> Self {
        Self {
            last_yearly_tick: None,
        }
    }
}

impl Default for SystemSchedule {
    fn default() -> Self {
        Self::new()
    }
}

/// Run systems due at the world's current tick (before clock advances).
pub fn run_due(world: &mut World) {
    let ticks_per_year = world.config.ticks_per_year();
    let due_yearly = match world.schedule.last_yearly_tick {
        None => world.clock.tick == 0 || world.clock.tick >= ticks_per_year,
        Some(last) => world.clock.tick.saturating_sub(last) >= ticks_per_year,
    };

    if due_yearly {
        let dt = match world.schedule.last_yearly_tick {
            None => world.clock.tick.max(1),
            Some(last) => world.clock.tick.saturating_sub(last).max(1),
        };
        yearly_systems(world, dt);
        world.schedule.last_yearly_tick = Some(world.clock.tick);
    }
}

fn yearly_systems(world: &mut World, dt_ticks: u64) {
    let years = (dt_ticks as f32) / (world.config.ticks_per_year() as f32);
    let years = years.max(0.25);

    growth(world, years);
    expand(world);
    conflict(world);
    collapse(world);
    maybe_myth(world);
}

fn growth(world: &mut World, years: f32) {
    let food_mults: Vec<f32> = world
        .civs
        .iter()
        .map(|civ| {
            civ.settlements
                .iter()
                .filter(|s| s.kind != SettlementKind::Ruin)
                .map(|s| world.map.biome(s.x, s.y).food_mult())
                .fold(0.0f32, f32::max)
                .max(0.2)
        })
        .collect();

    for (civ, food_mult) in world.civs.iter_mut().zip(food_mults) {
        if !civ.alive {
            continue;
        }
        let growth_rate = 0.04 * food_mult * civ.tech * years;
        let add = ((civ.population as f32) * growth_rate).round() as i32;
        civ.population = (civ.population as i32 + add).clamp(1, 500_000) as u32;
        civ.food = (civ.food + food_mult * years * 2.0 - years).clamp(0.0, 100.0);
        civ.strength = (civ.strength
            + years * 0.3 * civ.tech
            + (civ.population as f32).sqrt() * 0.01)
            .clamp(1.0, 10_000.0);
        civ.tech = (civ.tech + years * 0.02).min(5.0);
    }
}

fn expand(world: &mut World) {
    let dirs = [(-1i32, 0), (1, 0), (0, -1), (0, 1)];
    // Collect actions to avoid borrow issues
    let mut actions: Vec<(u32, u32, u32)> = Vec::new(); // civ_id, x, y

    for civ in world.civs.iter().filter(|c| c.alive) {
        if civ.population < 120 {
            continue;
        }
        if civ.settlements.iter().filter(|s| s.kind != SettlementKind::Ruin).count() >= 12 {
            continue;
        }
        // Chance scaled by strength
        let chance = (0.15 + civ.tech * 0.05).min(0.55);
        if !world.rng.gen_bool(chance as f64) {
            continue;
        }
        let Some(base) = civ
            .settlements
            .iter()
            .filter(|s| s.kind != SettlementKind::Ruin)
            .max_by_key(|s| match s.kind {
                SettlementKind::Capital => 2u8,
                SettlementKind::Village => 1,
                SettlementKind::Ruin => 0,
            })
        else {
            continue;
        };
        let mut candidates = Vec::new();
        for (dx, dy) in dirs {
            let nx = base.x as i32 + dx;
            let ny = base.y as i32 + dy;
            if !world.map.in_bounds(nx, ny) {
                continue;
            }
            let ux = nx as u32;
            let uy = ny as u32;
            if !world.map.biome(ux, uy).hospitable() {
                continue;
            }
            if world.settlement_owner(ux, uy).is_some() {
                continue;
            }
            candidates.push((ux, uy));
        }
        if candidates.is_empty() {
            continue;
        }
        let pick = candidates[world.rng.gen_range(0..candidates.len())];
        actions.push((civ.id, pick.0, pick.1));
    }

    for (civ_id, x, y) in actions {
        if world.settlement_owner(x, y).is_some() {
            continue;
        }
        if let Some(civ) = world.civs.iter_mut().find(|c| c.id == civ_id && c.alive) {
            let name = civ.name.clone();
            civ.settlements.push(Settlement {
                x,
                y,
                kind: SettlementKind::Village,
            });
            civ.population = civ.population.saturating_sub(20).max(40);
            world.push_event(EventKind::Expanded { civ: name, x, y });
        }
    }
}

fn conflict(world: &mut World) {
    // Find contested adjacency: two alive civs with neighboring settlements
    let mut pairs: Vec<(u32, u32)> = Vec::new();
    let alive: Vec<&crate::world::Civ> = world.civs.iter().filter(|c| c.alive).collect();
    for i in 0..alive.len() {
        for j in (i + 1)..alive.len() {
            let a = alive[i];
            let b = alive[j];
            if adjacent_settlements(a, b) {
                let mut pair = [a.id, b.id];
                pair.sort_unstable();
                pairs.push((pair[0], pair[1]));
            }
        }
    }
    pairs.sort_unstable();
    pairs.dedup();

    for (id_a, id_b) in pairs {
        let roll = world.rng.gen::<f32>();
        if roll > 0.35 {
            continue;
        }
        let (str_a, name_a) = {
            let c = world.civs.iter().find(|c| c.id == id_a).unwrap();
            (c.strength, c.name.clone())
        };
        let (str_b, name_b) = {
            let c = world.civs.iter().find(|c| c.id == id_b).unwrap();
            (c.strength, c.name.clone())
        };
        let jitter = world.rng.gen_range(-2.0..2.0);
        let a_wins = str_a + jitter >= str_b;
        let (victor_id, loser_id, victor, loser) = if a_wins {
            (id_a, id_b, name_a.clone(), name_b.clone())
        } else {
            (id_b, id_a, name_b.clone(), name_a.clone())
        };

        world.push_event(EventKind::War {
            attacker: name_a,
            defender: name_b,
            victor: victor.clone(),
        });

        // Victor takes one village from loser if any
        let stolen = {
            let loser_civ = world.civs.iter_mut().find(|c| c.id == loser_id).unwrap();
            if let Some(idx) = loser_civ
                .settlements
                .iter()
                .position(|s| s.kind == SettlementKind::Village)
            {
                Some(loser_civ.settlements.remove(idx))
            } else {
                None
            }
        };
        if let Some(site) = stolen {
            if let Some(v) = world.civs.iter_mut().find(|c| c.id == victor_id) {
                v.settlements.push(site);
                v.strength += 1.0;
            }
        }
        if let Some(l) = world.civs.iter_mut().find(|c| c.id == loser_id) {
            l.strength = (l.strength * 0.85).max(1.0);
            l.population = (l.population as f32 * 0.9) as u32;
        }
        let _ = (victor, loser);
    }
}

fn adjacent_settlements(a: &crate::world::Civ, b: &crate::world::Civ) -> bool {
    for sa in a.settlements.iter().filter(|s| s.kind != SettlementKind::Ruin) {
        for sb in b.settlements.iter().filter(|s| s.kind != SettlementKind::Ruin) {
            let dx = (sa.x as i32 - sb.x as i32).abs();
            let dy = (sa.y as i32 - sb.y as i32).abs();
            if dx + dy == 1 {
                return true;
            }
        }
    }
    false
}

fn collapse(world: &mut World) {
    let mut fell: Vec<(String, String, Vec<(u32, u32)>)> = Vec::new();
    for civ in world.civs.iter_mut().filter(|c| c.alive) {
        let sites = civ
            .settlements
            .iter()
            .filter(|s| s.kind != SettlementKind::Ruin)
            .count();
        let starving = civ.food < 1.0 && civ.population < 40;
        let crushed = civ.strength < 2.0 && sites <= 1 && civ.population < 50;
        if starving || crushed {
            let cause = if starving {
                "famine".into()
            } else {
                "decline".into()
            };
            let ruins: Vec<(u32, u32)> = civ
                .settlements
                .iter()
                .map(|s| (s.x, s.y))
                .collect();
            for s in &mut civ.settlements {
                s.kind = SettlementKind::Ruin;
            }
            civ.alive = false;
            civ.population = 0;
            fell.push((civ.name.clone(), cause, ruins));
        }
    }
    for (name, cause, ruins) in fell {
        world.push_event(EventKind::Fell {
            civ: name.clone(),
            cause,
        });
        for (x, y) in ruins {
            world.push_event(EventKind::Ruin {
                civ: name.clone(),
                x,
                y,
            });
        }
    }
}

fn maybe_myth(world: &mut World) {
    if !world.rng.gen_bool(0.12) {
        return;
    }
    let alive: Vec<String> = world
        .civs
        .iter()
        .filter(|c| c.alive)
        .map(|c| c.name.clone())
        .collect();
    if alive.is_empty() {
        return;
    }
    let civ = alive[world.rng.gen_range(0..alive.len())].clone();
    let tales = [
        "a twin moon rose and the rivers ran silver for a night",
        "stone-singers carved a path through the hills",
        "a plague of moths ate the war-banners",
        "the elders dreamed of a green star and planted orchards",
        "a bridge of ice stood for seven days across the marsh",
    ];
    let tale = tales[world.rng.gen_range(0..tales.len())].to_string();
    world.push_event(EventKind::Myth { civ, tale });
}
