use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use clap::Parser;
use serde_json::json;
use sim_core::IpcClient;
use sim_core::{AdvanceParams, CreateParams, OkBody, Request};

const TICKS_PER_DAY: u64 = 1440;

#[derive(Parser, Debug)]
#[command(name = "mundus-client", about = "Run the mundus simulation to a year")]
struct Args {
    #[arg(long, default_value_t = 1)]
    seed: u64,
    #[arg(long, default_value_t = 100)]
    years: u64,
    #[arg(long, default_value_t = 365)]
    days_per_year: u32,
    #[arg(long, default_value_t = 128)]
    width: u32,
    #[arg(long, default_value_t = 128)]
    height: u32,
    #[arg(long, default_value_t = 6)]
    civs: u8,
    /// Ticks per advance request. Default: one year.
    #[arg(long)]
    batch_ticks: Option<u64>,
    #[arg(long, default_value = "out")]
    out: PathBuf,
    /// Attach to this socket instead of spawning sim-server.
    #[arg(short = 'S', long)]
    socket: Option<PathBuf>,
}

fn main() {
    if let Err(e) = run(Args::parse()) {
        eprintln!("mundus-client error: {e}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let ticks_per_year = u64::from(args.days_per_year.max(1)) * TICKS_PER_DAY;
    let batch = args.batch_ticks.unwrap_or(ticks_per_year).max(1);
    let target = args.years.saturating_mul(ticks_per_year);
    let (socket, child) = match &args.socket {
        Some(path) => (path.clone(), None),
        None => {
            let path = std::env::temp_dir().join(format!("sim-{}.sock", std::process::id()));
            let child = spawn_server(&path)?;
            wait_for_socket(&path)?;
            (path, Some(child))
        }
    };
    let result = drive(&args, &socket, ticks_per_year, batch, target);
    shutdown(&socket);
    if let Some(mut child) = child {
        let _ = child.wait();
        let _ = std::fs::remove_file(&socket);
    }
    result
}

fn drive(
    args: &Args,
    socket: &Path,
    ticks_per_year: u64,
    batch: u64,
    target: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = IpcClient::connect(socket)?;
    let config = json!({
        "seed": args.seed,
        "width": args.width,
        "height": args.height,
        "days_per_year": args.days_per_year,
        "civ_count": args.civs,
    });
    match client.call_ok(&Request::Create(CreateParams {
        kind: "mundus".into(),
        config,
    }))? {
        OkBody::Created { tick } => println!("created mundus at tick {tick}"),
        other => return Err(format!("unexpected create: {other:?}").into()),
    }

    loop {
        let status = match client.call_ok(&Request::Status)? {
            OkBody::Status(status) => status,
            other => return Err(format!("unexpected status: {other:?}").into()),
        };
        if status.tick >= target {
            break;
        }
        let n = (target - status.tick).min(batch);
        match client.call_ok(&Request::Advance(AdvanceParams { n }))? {
            OkBody::Advance(step) => {
                let year = status
                    .summary
                    .get("year")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                println!("tick {} year {year} (+{n})", step.tick);
            }
            other => return Err(format!("unexpected advance: {other:?}").into()),
        }
    }

    let state = match client.call_ok(&Request::State)? {
        OkBody::State(state) => state,
        other => return Err(format!("unexpected state: {other:?}").into()),
    };
    std::fs::create_dir_all(&args.out)?;
    if let Some(text) = state.extras.get("chronicle_md").and_then(|v| v.as_str()) {
        std::fs::write(args.out.join("chronicle.md"), text)?;
    }
    if let Some(text) = state.extras.get("map_txt").and_then(|v| v.as_str()) {
        std::fs::write(args.out.join("map.txt"), text)?;
    }
    std::fs::write(
        args.out.join("state.json"),
        serde_json::to_string_pretty(&state)?,
    )?;
    std::fs::write(
        args.out.join("manifest.json"),
        serde_json::to_string_pretty(&json!({
            "kind": "mundus",
            "seed": args.seed,
            "years": args.years,
            "days_per_year": args.days_per_year,
            "ticks_per_year": ticks_per_year,
            "width": args.width,
            "height": args.height,
            "civ_count": args.civs,
            "batch_ticks": batch,
            "state_hash": state.state_hash,
        }))?,
    )?;
    println!("wrote {} ({})", args.out.display(), state.state_hash);
    Ok(())
}

fn shutdown(socket: &Path) {
    if let Ok(mut client) = IpcClient::connect(socket) {
        let _ = client.call_ok(&Request::Shutdown);
    }
}

fn spawn_server(socket: &Path) -> Result<Child, Box<dyn std::error::Error>> {
    let exe = std::env::current_exe()?;
    let bin = exe
        .parent()
        .map(|dir| dir.join("sim-server"))
        .filter(|path| path.exists())
        .unwrap_or_else(|| PathBuf::from("sim-server"));
    Command::new(&bin)
        .arg("--socket")
        .arg(socket)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("failed to spawn {}: {e}", bin.display()).into())
}

fn wait_for_socket(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if path.exists() && IpcClient::connect(path).is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(format!("timeout waiting for {}", path.display()).into())
}
