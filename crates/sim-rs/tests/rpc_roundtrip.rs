use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;
use sim_core::IpcClient;
use sim_core::{AdvanceParams, CallParams, CreateParams, OkBody, Request, RestoreParams};

struct ServerGuard {
    child: Child,
    socket: PathBuf,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.socket);
    }
}

fn spawn_server() -> ServerGuard {
    let socket = std::env::temp_dir().join(format!(
        "sim-test-{}-{}.sock",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));
    let mut child = Command::new(env!("CARGO_BIN_EXE_sim-server"))
        .arg("--socket")
        .arg(&socket)
        .arg("--max-actions-per-tick")
        .arg("2")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn sim-server");

    let start = Instant::now();
    let ready = loop {
        if socket.exists() && IpcClient::connect(&socket).is_ok() {
            break true;
        }
        if start.elapsed() >= Duration::from_secs(10) {
            break false;
        }
        thread::sleep(Duration::from_millis(20));
    };
    if ready {
        ServerGuard { child, socket }
    } else {
        let _ = child.kill();
        let _ = child.wait();
        panic!("timeout waiting for {}", socket.display());
    }
}

#[test]
fn life_schedule_advance_state_restore() {
    let server = spawn_server();
    let mut client = IpcClient::connect(&server.socket).unwrap();

    match client
        .call_ok(&Request::Create(CreateParams {
            kind: "game_of_life".into(),
            config: json!({
                "seed": 4,
                "width": 8,
                "height": 8,
                "density": 0.2
            }),
        }))
        .unwrap()
    {
        OkBody::Created { tick } => assert_eq!(tick, 0),
        other => panic!("unexpected create: {other:?}"),
    }

    let actions = match client.call_ok(&Request::Actions).unwrap() {
        OkBody::Actions { actions } => actions,
        other => panic!("unexpected actions: {other:?}"),
    };
    assert!(actions.iter().any(|action| action.name == "set_cell"));

    client
        .call_ok(&Request::Call(CallParams {
            name: "set_cell".into(),
            params: json!({ "x": 1, "y": 1, "alive": true }),
            at_tick: Some(3),
        }))
        .unwrap();
    client
        .call_ok(&Request::Call(CallParams {
            name: "set_cell".into(),
            params: json!({ "x": 2, "y": 1, "alive": true }),
            at_tick: Some(3),
        }))
        .unwrap();
    let limited = client.call_ok(&Request::Call(CallParams {
        name: "set_cell".into(),
        params: json!({ "x": 3, "y": 1, "alive": true }),
        at_tick: Some(3),
    }));
    assert!(limited.is_err());

    match client
        .call_ok(&Request::Advance(AdvanceParams { n: 5 }))
        .unwrap()
    {
        OkBody::Advance(step) => {
            assert_eq!(step.tick, 5);
            assert!(step.failed.is_empty());
        }
        other => panic!("unexpected advance: {other:?}"),
    }

    let state = match client.call_ok(&Request::State).unwrap() {
        OkBody::State(state) => state,
        other => panic!("unexpected state: {other:?}"),
    };
    assert_eq!(state.journal.len(), 2);
    assert!(state.scheduled.is_empty());
    assert!(state.extras.get("grid_txt").is_some());

    client.call_ok(&Request::Shutdown).unwrap();
    drop(client);
    drop(server);

    let server = spawn_server();
    let mut client = IpcClient::connect(&server.socket).unwrap();
    match client
        .call_ok(&Request::Restore(RestoreParams {
            kind: state.kind,
            sim: state.sim,
            journal: state.journal,
            scheduled: state.scheduled,
        }))
        .unwrap()
    {
        OkBody::Restored { tick } => assert_eq!(tick, 5),
        other => panic!("unexpected restore: {other:?}"),
    }
    let again = match client.call_ok(&Request::State).unwrap() {
        OkBody::State(state) => state,
        other => panic!("unexpected state: {other:?}"),
    };
    assert_eq!(again.state_hash, state.state_hash);
}
