use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Whether a call returns a JSON body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallReturns {
    Void,
    Data,
}

/// Result of running a call.
#[derive(Debug, Clone, PartialEq)]
pub enum CallOutput {
    Void,
    Data(Value),
}

/// One parameter advertised by an action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParamSpec {
    pub name: String,
    pub ty: String,
}

/// An action a simulation is willing to run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionSpec {
    pub name: String,
    pub summary: String,
    pub mutating: bool,
    pub returns: CallReturns,
    pub params: Vec<ParamSpec>,
}

impl ActionSpec {
    pub fn new(
        name: impl Into<String>,
        summary: impl Into<String>,
        mutating: bool,
        returns: CallReturns,
        params: Vec<ParamSpec>,
    ) -> Self {
        Self {
            name: name.into(),
            summary: summary.into(),
            mutating,
            returns,
            params,
        }
    }
}

/// A plug-in simulation. The host owns time, the journal, and the socket.
pub trait Simulation: Send + Sync {
    fn kind(&self) -> &'static str;
    fn tick(&self) -> u64;
    fn is_stopped(&self) -> bool;
    fn actions(&self) -> Vec<ActionSpec>;
    /// Read-only call. Must not change state or the random stream.
    fn query(&self, name: &str, params: Value) -> Result<CallOutput, String>;
    /// Mutating call. Runs only when the host has decided the tick has arrived.
    fn call(&mut self, name: &str, params: Value) -> Result<CallOutput, String>;
    /// Advance at most `n` atomic ticks. Stop early when [`Self::is_stopped`] becomes true.
    fn advance(&mut self, n: u64) -> Result<(), String>;
    fn status_summary(&self) -> Value;
    /// Complete save document, including the random stream.
    fn sim_json(&self) -> Value;
    fn extras(&self) -> Value;
}
