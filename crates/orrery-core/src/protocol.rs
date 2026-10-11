use serde::{Deserialize, Serialize};

use crate::config::SimConfig;
use crate::event::Event;

pub const EVENT_CAP: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Request {
    Create(CreateParams),
    Status,
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
    pub civ_count: usize,
    pub alive_civs: usize,
    pub event_count: usize,
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
