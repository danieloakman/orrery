//! End-to-end smoke test: spawn the server binary, exercise the RPC surface.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use mundus_core::protocol::{
    AdvanceTickParams, ChronicleParams, CreateParams, GetCivParams, OkBody, Request,
};
use mundus_core::{IpcClient, SimConfig, TICKS_PER_DAY};

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
        "mundus-test-{}-{}.sock",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));
    let mut child = Command::new(env!("CARGO_BIN_EXE_mundus-server"))
        .arg("--socket")
        .arg(&socket)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mundus-server");

    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if socket.exists() && IpcClient::connect(&socket).is_ok() {
            return ServerGuard { child, socket };
        }
        thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&socket);
    panic!("timeout waiting for server at {}", socket.display());
}

#[test]
fn create_observe_advance_dump_shutdown() {
    let server = spawn_server();
    let mut client = IpcClient::connect(&server.socket).expect("connect");

    let config = SimConfig {
        seed: 7,
        width: 32,
        height: 32,
        days_per_year: 365,
        civ_count: 3,
    };

    match client
        .call_ok(&Request::Create(CreateParams {
            config: config.clone(),
        }))
        .expect("create")
    {
        OkBody::Created { tick, civs } => {
            assert_eq!(tick, 0);
            assert_eq!(civs, 3);
        }
        other => panic!("unexpected create: {other:?}"),
    }

    match client.call_ok(&Request::Status).expect("status") {
        OkBody::Status(s) => {
            assert_eq!(s.tick, 0);
            assert_eq!(s.day, 0);
            assert_eq!(s.hour, 0);
            assert_eq!(s.minute, 0);
            assert_eq!(s.year, 0);
            assert_eq!(s.days_per_year, 365);
            assert_eq!(s.civ_count, 3);
        }
        other => panic!("unexpected status: {other:?}"),
    }

    let first_name = match client.call_ok(&Request::ListCivs).expect("list_civs") {
        OkBody::ListCivs(r) => {
            assert_eq!(r.civs.len(), 3);
            assert_eq!(r.civs[0].id, 0);
            r.civs[0].name.clone()
        }
        other => panic!("unexpected list_civs: {other:?}"),
    };

    match client
        .call_ok(&Request::GetCiv(GetCivParams {
            query: first_name.clone(),
        }))
        .expect("get_civ by name")
    {
        OkBody::GetCiv(r) => {
            assert_eq!(r.civ.id, 0);
            assert_eq!(r.civ.name, first_name);
            assert!(!r.civ.settlements.is_empty());
        }
        other => panic!("unexpected get_civ: {other:?}"),
    }

    match client
        .call_ok(&Request::GetCiv(GetCivParams { query: "0".into() }))
        .expect("get_civ by id")
    {
        OkBody::GetCiv(r) => assert_eq!(r.civ.id, 0),
        other => panic!("unexpected get_civ id: {other:?}"),
    }

    match client
        .call_ok(&Request::Chronicle(ChronicleParams {
            since_tick: Some(0),
            limit: Some(10),
        }))
        .expect("chronicle")
    {
        OkBody::Chronicle(r) => {
            assert!(!r.events.is_empty());
            assert!(r.total_matching >= r.events.len());
        }
        other => panic!("unexpected chronicle: {other:?}"),
    }

    // One day in minutes.
    match client
        .call_ok(&Request::AdvanceTick(AdvanceTickParams {
            n: u64::from(TICKS_PER_DAY),
        }))
        .expect("advance")
    {
        OkBody::AdvanceTick(r) => {
            assert_eq!(r.tick, u64::from(TICKS_PER_DAY));
            assert_eq!(r.day, 1);
            assert_eq!(r.year, 0);
        }
        other => panic!("unexpected advance: {other:?}"),
    }

    match client.call_ok(&Request::Dump).expect("dump") {
        OkBody::Dump(d) => {
            assert!(d.chronicle_md.contains("Mundus Chronicle"));
            assert!(!d.map_txt.is_empty());
            assert!(d.state_json.contains("\"tick\""));
            assert_eq!(d.state_hash.len(), 64);
        }
        other => panic!("unexpected dump: {other:?}"),
    }

    match client.call_ok(&Request::Shutdown).expect("shutdown") {
        OkBody::Shutdown => {}
        other => panic!("unexpected shutdown: {other:?}"),
    }
}
