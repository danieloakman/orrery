use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;
use sim_core::IpcClient;
use sim_core::{AdvanceParams, CallParams, OkBody, Request, StateBody};

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

struct DirGuard(PathBuf);

impl Drop for DirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn spawn_server(extra: &[impl AsRef<OsStr>]) -> ServerGuard {
    let socket = std::env::temp_dir().join(format!(
        "sim-test-{}-{}.sock",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));
    let mut child = Command::new(env!("CARGO_BIN_EXE_sim-server"))
        .arg("--socket")
        .arg(&socket)
        .args(extra)
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
    let dir = DirGuard(std::env::temp_dir().join(format!(
        "sim-test-dir-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    )));
    std::fs::create_dir_all(&dir.0).unwrap();
    let state_path = dir.0.join("state.json");
    let restored_path = dir.0.join("restored.json");

    let server = spawn_server(&[
        "--max-actions-per-tick",
        "2",
        "--state-out",
        state_path.to_str().unwrap(),
        "create",
        "--kind",
        "game_of_life",
        "--config",
        r#"{"seed":4,"width":8,"height":8,"density":0.2}"#,
    ]);
    let mut client = IpcClient::connect(&server.socket).unwrap();

    match client.call_ok(&Request::Status).unwrap() {
        OkBody::Status(status) => {
            assert_eq!(status.kind, "game_of_life");
            assert_eq!(status.tick, 0);
        }
        other => panic!("unexpected status: {other:?}"),
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

    client.call_ok(&Request::Shutdown).unwrap();
    drop(client);
    drop(server);

    let state: StateBody =
        serde_json::from_str(&std::fs::read_to_string(&state_path).unwrap()).unwrap();
    assert_eq!(state.journal.len(), 2);
    assert!(state.scheduled.is_empty());
    assert!(state.extras.get("grid_txt").is_some());

    let server = spawn_server(&[
        "--max-actions-per-tick",
        "2",
        "--state-out",
        restored_path.to_str().unwrap(),
        "restore",
        "--file",
        state_path.to_str().unwrap(),
    ]);
    let mut client = IpcClient::connect(&server.socket).unwrap();
    match client.call_ok(&Request::Status).unwrap() {
        OkBody::Status(status) => assert_eq!(status.tick, 5),
        other => panic!("unexpected status: {other:?}"),
    }
    client.call_ok(&Request::Shutdown).unwrap();
    drop(client);
    drop(server);

    let again: StateBody =
        serde_json::from_str(&std::fs::read_to_string(&restored_path).unwrap()).unwrap();
    assert_eq!(again.state_hash, state.state_hash);
}
