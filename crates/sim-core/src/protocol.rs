use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::sim::ActionSpec;

/// A mutating call that has run, or one waiting for its tick.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionRecord {
    pub tick: u64,
    pub name: String,
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FailedCall {
    pub tick: u64,
    pub name: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Request {
    Create(CreateParams),
    Status,
    Actions,
    Call(CallParams),
    Advance(AdvanceParams),
    State,
    Restore(RestoreParams),
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateParams {
    pub kind: String,
    pub config: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallParams {
    pub name: String,
    #[serde(default)]
    pub params: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_tick: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvanceParams {
    pub n: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreParams {
    pub kind: String,
    pub sim: Value,
    #[serde(default)]
    pub journal: Vec<ActionRecord>,
    #[serde(default)]
    pub scheduled: Vec<ActionRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Response {
    Ok(OkBody),
    Err { error: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum OkBody {
    Created { tick: u64 },
    Status(StatusBody),
    Actions { actions: Vec<ActionSpec> },
    Call(CallBody),
    Advance(AdvanceBody),
    State(StateBody),
    Restored { tick: u64 },
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusBody {
    pub kind: String,
    pub tick: u64,
    pub stopped: bool,
    pub summary: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallBody {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_tick: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvanceBody {
    pub tick: u64,
    pub stopped: bool,
    pub failed: Vec<FailedCall>,
}

/// Save point. `sim` is the full simulation document, including its random stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateBody {
    pub kind: String,
    pub sim: Value,
    pub journal: Vec<ActionRecord>,
    pub scheduled: Vec<ActionRecord>,
    pub state_hash: String,
    pub extras: Value,
}

impl CallParams {
    pub fn params_or_empty(&self) -> Value {
        if self.params.is_null() {
            Value::Object(serde_json::Map::new())
        } else {
            self.params.clone()
        }
    }
}
