use serde::{Deserialize, Serialize};

use crate::config::SimConfig;
use crate::event::Event;
use crate::world::Civ;

pub const EVENT_CAP: usize = 256;
pub const CHRONICLE_DEFAULT_LIMIT: usize = 50;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Request {
    Create(CreateParams),
    Status,
    ListCivs,
    GetCiv(GetCivParams),
    Chronicle(ChronicleParams),
    AdvanceTick(AdvanceTickParams),
    Dump,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateParams {
    #[serde(flatten)]
    pub config: SimConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvanceTickParams {
    pub n: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCivParams {
    /// Civ id (decimal) or exact name (case-insensitive).
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChronicleParams {
    /// Inclusive lower bound on event tick. Default: 0.
    #[serde(default)]
    pub since_tick: Option<u64>,
    /// Max events to return (ascending). Default: [`CHRONICLE_DEFAULT_LIMIT`].
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Response {
    Ok(OkBody),
    Err { error: String },
}

// Note: Ok is untagged against Err by shape (`result` vs `error`).

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum OkBody {
    Created { tick: u64, civs: usize },
    Status(StatusResult),
    ListCivs(ListCivsResult),
    GetCiv(GetCivResult),
    Chronicle(ChronicleResult),
    AdvanceTick(AdvanceTickResult),
    Dump(DumpResult),
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResult {
    pub tick: u64,
    pub day: u64,
    pub year: u64,
    pub hour: u32,
    pub minute: u32,
    pub days_per_year: u32,
    pub civ_count: usize,
    pub alive_civs: usize,
    pub event_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CivSummary {
    pub id: u32,
    pub name: String,
    pub alive: bool,
    pub population: u32,
    pub food: f32,
    pub tech: f32,
    pub strength: f32,
    pub settlements: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListCivsResult {
    pub tick: u64,
    pub civs: Vec<CivSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCivResult {
    pub tick: u64,
    pub civ: Civ,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChronicleResult {
    pub tick: u64,
    pub since_tick: u64,
    pub events: Vec<Event>,
    pub total_matching: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvanceTickResult {
    pub tick: u64,
    pub day: u64,
    pub year: u64,
    pub events_len: usize,
    pub events: Vec<Event>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DumpResult {
    pub chronicle_md: String,
    pub map_txt: String,
    pub state_json: String,
    pub state_hash: String,
}

pub fn cap_events(events: &[Event]) -> (Vec<Event>, bool) {
    if events.len() <= EVENT_CAP {
        (events.to_vec(), false)
    } else {
        let start = events.len() - EVENT_CAP;
        (events[start..].to_vec(), true)
    }
}

pub fn civ_summary(civ: &Civ) -> CivSummary {
    CivSummary {
        id: civ.id,
        name: civ.name.clone(),
        alive: civ.alive,
        population: civ.population,
        food: civ.food,
        tech: civ.tech,
        strength: civ.strength,
        settlements: civ.settlements.len(),
    }
}

pub fn resolve_civ<'a>(civs: &'a [Civ], query: &str) -> Result<&'a Civ, String> {
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

pub fn filter_chronicle(events: &[Event], since_tick: u64, limit: usize) -> ChronicleSlice {
    let matching: Vec<&Event> = events.iter().filter(|e| e.tick >= since_tick).collect();
    let total_matching = matching.len();
    let truncated = total_matching > limit;
    let events: Vec<Event> = matching.into_iter().take(limit).cloned().collect();
    ChronicleSlice {
        events,
        total_matching,
        truncated,
    }
}

pub struct ChronicleSlice {
    pub events: Vec<Event>,
    pub total_matching: usize,
    pub truncated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::EventKind;
    use crate::world::{Settlement, SettlementKind};

    fn sample_civ(id: u32, name: &str) -> Civ {
        Civ {
            id,
            name: name.into(),
            alive: true,
            population: 100,
            food: 1.0,
            tech: 1.0,
            strength: 10.0,
            settlements: vec![Settlement {
                x: 1,
                y: 2,
                kind: SettlementKind::Capital,
            }],
        }
    }

    #[test]
    fn resolve_civ_by_id_and_name() {
        let civs = vec![sample_civ(0, "Corfolk"), sample_civ(1, "Belormoor")];
        assert_eq!(resolve_civ(&civs, "0").unwrap().name, "Corfolk");
        assert_eq!(resolve_civ(&civs, "belormoor").unwrap().id, 1);
        assert!(resolve_civ(&civs, "nope").is_err());
    }

    #[test]
    fn filter_chronicle_pages_from_since() {
        let events = vec![
            Event {
                tick: 0,
                day: 0,
                year: 0,
                kind: EventKind::WorldBorn { civs: 2 },
            },
            Event {
                tick: 10,
                day: 0,
                year: 0,
                kind: EventKind::Myth {
                    civ: "Corfolk".into(),
                    tale: "stars".into(),
                },
            },
            Event {
                tick: 20,
                day: 0,
                year: 0,
                kind: EventKind::Myth {
                    civ: "Corfolk".into(),
                    tale: "sea".into(),
                },
            },
        ];
        let slice = filter_chronicle(&events, 10, 1);
        assert_eq!(slice.total_matching, 2);
        assert!(slice.truncated);
        assert_eq!(slice.events.len(), 1);
        assert_eq!(slice.events[0].tick, 10);
    }
}
