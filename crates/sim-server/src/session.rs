use std::collections::HashMap;

use serde_json::Value;
use sha2::{Digest, Sha256};
use sim_core::{
    ActionRecord, AdvanceBody, CallBody, CallOutput, CallReturns, FailedCall, Simulation,
    StateBody, StatusBody,
};

/// Counts of accepted mutating calls for one connection, keyed by simulation tick.
#[derive(Debug, Default)]
pub struct Budget {
    counts: HashMap<u64, u32>,
}

impl Budget {
    fn allows(&self, tick: u64, limit: u32) -> Result<(), String> {
        if limit == 0 {
            return Ok(());
        }
        let used = self.counts.get(&tick).copied().unwrap_or(0);
        if used >= limit {
            Err(format!("action limit {limit} reached for tick {tick}"))
        } else {
            Ok(())
        }
    }

    fn charge(&mut self, tick: u64) {
        *self.counts.entry(tick).or_insert(0) += 1;
    }
}

pub struct Session {
    sim: Box<dyn Simulation>,
    journal: Vec<ActionRecord>,
    scheduled: Vec<ActionRecord>,
}

impl Session {
    pub fn new(sim: Box<dyn Simulation>) -> Self {
        Self {
            sim,
            journal: Vec::new(),
            scheduled: Vec::new(),
        }
    }

    pub fn tick(&self) -> u64 {
        self.sim.tick()
    }

    pub fn install_history(&mut self, journal: Vec<ActionRecord>, scheduled: Vec<ActionRecord>) {
        self.journal = journal;
        self.scheduled = scheduled;
    }

    pub fn status(&self) -> StatusBody {
        StatusBody {
            kind: self.sim.kind().to_string(),
            tick: self.sim.tick(),
            stopped: self.sim.is_stopped(),
            summary: self.sim.status_summary(),
        }
    }

    pub fn actions(&self) -> Vec<sim_core::ActionSpec> {
        self.sim.actions()
    }

    pub fn state(&self) -> StateBody {
        let sim = self.sim.sim_json();
        StateBody {
            kind: self.sim.kind().to_string(),
            state_hash: hash_sim(&sim),
            sim,
            journal: self.journal.clone(),
            scheduled: self.scheduled.clone(),
            extras: self.sim.extras(),
        }
    }

    pub fn query(&self, name: &str, params: Value) -> Result<CallBody, String> {
        let actions = self.sim.actions();
        let spec = find_action(&actions, name)?;
        if spec.mutating {
            return Err(format!("action {name} is mutating"));
        }
        let output = self.sim.query(name, params)?;
        body_for(spec.returns, output, None)
    }

    pub fn call(
        &mut self,
        name: &str,
        params: Value,
        at_tick: Option<u64>,
        budget: &mut Budget,
        limit: u32,
    ) -> Result<CallBody, String> {
        let actions = self.sim.actions();
        let spec = find_action(&actions, name)?;
        if !spec.mutating {
            return Err(format!(
                "action {name} is read-only and cannot be scheduled"
            ));
        }
        if self.sim.is_stopped() {
            return Err("simulation stopped".into());
        }
        let now = self.sim.tick();
        let tick = at_tick.unwrap_or(now);
        if tick < now {
            return Err(format!("tick {tick} is in the past (now {now})"));
        }
        if at_tick.is_some() && spec.returns == CallReturns::Data {
            return Err(format!(
                "action {name} returns data and cannot be scheduled"
            ));
        }
        budget.allows(tick, limit)?;
        if tick == now {
            let output = self.sim.call(name, params.clone())?;
            let body = body_for(spec.returns, output, None)?;
            self.journal.push(ActionRecord {
                tick,
                name: name.to_string(),
                params,
            });
            budget.charge(tick);
            Ok(body)
        } else {
            self.scheduled.push(ActionRecord {
                tick,
                name: name.to_string(),
                params,
            });
            budget.charge(tick);
            Ok(CallBody {
                data: None,
                at_tick: Some(tick),
            })
        }
    }

    pub fn advance(&mut self, n: u64) -> Result<AdvanceBody, String> {
        if self.sim.is_stopped() {
            return Err("simulation stopped".into());
        }
        let mut failed = Vec::new();
        self.apply_due(&mut failed);
        let target = self.sim.tick().saturating_add(n);
        while self.sim.tick() < target && !self.sim.is_stopped() {
            let now = self.sim.tick();
            let dest = self
                .scheduled
                .iter()
                .map(|call| call.tick)
                .filter(|tick| *tick > now && *tick < target)
                .min()
                .unwrap_or(target);
            let step = dest - now;
            let before = self.sim.tick();
            self.sim.advance(step)?;
            let after = self.sim.tick();
            if after > before.saturating_add(step) {
                return Err("simulation advanced past the requested step".into());
            }
            self.apply_due(&mut failed);
            if after == before {
                break;
            }
        }
        Ok(AdvanceBody {
            tick: self.sim.tick(),
            stopped: self.sim.is_stopped(),
            failed,
        })
    }

    fn apply_due(&mut self, failed: &mut Vec<FailedCall>) {
        let now = self.sim.tick();
        let mut i = 0;
        while i < self.scheduled.len() {
            if self.scheduled[i].tick != now {
                i += 1;
                continue;
            }
            let record = self.scheduled.remove(i);
            match self.run_scheduled(&record) {
                Ok(()) => self.journal.push(record),
                Err(error) => failed.push(FailedCall {
                    tick: record.tick,
                    name: record.name,
                    error,
                }),
            }
        }
    }

    fn run_scheduled(&mut self, record: &ActionRecord) -> Result<(), String> {
        let actions = self.sim.actions();
        let spec = find_action(&actions, &record.name)?;
        if !spec.mutating || spec.returns != CallReturns::Void {
            return Err(format!(
                "action {} cannot run from the schedule",
                record.name
            ));
        }
        match self.sim.call(&record.name, record.params.clone())? {
            CallOutput::Void => Ok(()),
            CallOutput::Data(_) => Err(format!("action {} returned data", record.name)),
        }
    }
}

fn find_action<'a>(
    actions: &'a [sim_core::ActionSpec],
    name: &str,
) -> Result<&'a sim_core::ActionSpec, String> {
    actions
        .iter()
        .find(|action| action.name == name)
        .ok_or_else(|| format!("unknown action {name}"))
}

fn body_for(
    returns: CallReturns,
    output: CallOutput,
    at_tick: Option<u64>,
) -> Result<CallBody, String> {
    match (returns, output) {
        (CallReturns::Void, CallOutput::Void) => Ok(CallBody {
            data: None,
            at_tick,
        }),
        (CallReturns::Data, CallOutput::Data(data)) => Ok(CallBody {
            data: Some(data),
            at_tick,
        }),
        (CallReturns::Void, CallOutput::Data(_)) => Err("void action returned data".into()),
        (CallReturns::Data, CallOutput::Void) => Err("data action returned nothing".into()),
    }
}

pub fn hash_sim(sim: &Value) -> String {
    let bytes = serde_json::to_vec(sim).expect("sim json");
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sim_core::{ActionSpec, CallOutput, CallReturns, Simulation};

    struct Counter {
        tick: u64,
        value: u64,
        max: Option<u64>,
    }

    impl Simulation for Counter {
        fn kind(&self) -> &'static str {
            "counter"
        }
        fn tick(&self) -> u64 {
            self.tick
        }
        fn is_stopped(&self) -> bool {
            self.max.is_some_and(|max| self.tick >= max)
        }
        fn actions(&self) -> Vec<ActionSpec> {
            vec![
                ActionSpec::new("get", "read value", false, CallReturns::Data, vec![]),
                ActionSpec::new("inc", "add one", true, CallReturns::Void, vec![]),
            ]
        }
        fn query(&self, name: &str, _: Value) -> Result<CallOutput, String> {
            match name {
                "get" => Ok(CallOutput::Data(json!(self.value))),
                other => Err(format!("unknown action {other}")),
            }
        }
        fn call(&mut self, name: &str, _: Value) -> Result<CallOutput, String> {
            match name {
                "inc" => {
                    self.value += 1;
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
                self.tick += 1;
            }
            Ok(())
        }
        fn status_summary(&self) -> Value {
            json!({ "value": self.value })
        }
        fn sim_json(&self) -> Value {
            json!({ "tick": self.tick, "value": self.value })
        }
        fn extras(&self) -> Value {
            Value::Null
        }
    }

    fn session() -> Session {
        Session::new(Box::new(Counter {
            tick: 0,
            value: 0,
            max: None,
        }))
    }

    #[test]
    fn scheduled_call_matches_immediate_twin() {
        let mut scheduled = session();
        let mut budget = Budget::default();
        scheduled
            .call("inc", json!({}), Some(2), &mut budget, 0)
            .unwrap();
        scheduled.advance(4).unwrap();

        let mut twin = session();
        let mut budget = Budget::default();
        twin.advance(2).unwrap();
        twin.call("inc", json!({}), None, &mut budget, 0).unwrap();
        twin.advance(2).unwrap();

        assert_eq!(scheduled.state().state_hash, twin.state().state_hash);
        assert_eq!(scheduled.journal.len(), 1);
        assert!(scheduled.scheduled.is_empty());
    }

    #[test]
    fn mutating_budget_is_per_tick_and_ignores_reads() {
        let mut session = session();
        let mut budget = Budget::default();
        session.query("get", json!({})).unwrap();
        session.query("get", json!({})).unwrap();
        session
            .call("inc", json!({}), None, &mut budget, 1)
            .unwrap();
        let err = session
            .call("inc", json!({}), None, &mut budget, 1)
            .unwrap_err();
        assert!(err.contains("action limit"));
        session.advance(1).unwrap();
        session
            .call("inc", json!({}), None, &mut budget, 1)
            .unwrap();
    }

    #[test]
    fn stopped_rejects_advance_and_mutations() {
        let mut session = Session::new(Box::new(Counter {
            tick: 0,
            value: 0,
            max: Some(3),
        }));
        let step = session.advance(10).unwrap();
        assert_eq!(step.tick, 3);
        assert!(step.stopped);
        assert!(session.advance(1).is_err());
        let mut budget = Budget::default();
        assert!(session
            .call("inc", json!({}), None, &mut budget, 0)
            .is_err());
        assert!(session.query("get", json!({})).is_ok());
        assert_eq!(session.state().kind, "counter");
        assert_eq!(session.tick(), 3);
    }

    #[test]
    fn third_scheduled_call_on_same_tick_fails() {
        let mut session = session();
        let mut budget = Budget::default();
        session
            .call("inc", json!({}), Some(5), &mut budget, 2)
            .unwrap();
        session
            .call("inc", json!({}), Some(5), &mut budget, 2)
            .unwrap();
        assert!(session
            .call("inc", json!({}), Some(5), &mut budget, 2)
            .is_err());
    }
}
