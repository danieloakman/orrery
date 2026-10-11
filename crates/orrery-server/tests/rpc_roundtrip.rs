//! End-to-end smoke test: spawn the server binary, exercise the RPC surface.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use orrery_core::protocol::{AdvanceTickParams, CreateParams, OkBody, Request};
use orrery_core::{IpcClient, SimConfig, TICKS_PER_DAY};

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
        "orrery-test-{}-{}.sock",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));
    let mut child = Command::new(env!("CARGO_BIN_EXE_orrery-server"))
        .arg("--socket")
        .arg(&socket)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn orrery-server");

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
fn create_advance_dump_shutdown() {
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
            assert_eq!(s.year, 0);
            assert_eq!(s.civ_count, 3);
        }
        other => panic!("unexpected status: {other:?}"),
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
            assert!(d.chronicle_md.contains("Orrery Chronicle"));
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
