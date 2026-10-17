use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use clap::Parser;
use serde_json::json;
use sim_core::IpcClient;
use sim_core::{AdvanceParams, OkBody, Request, StateBody};

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
    /// Attach to a server that already owns a simulation.
    /// This client advances it and shuts it down. The save point is the server's `--state-out`.
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
    let config = json!({
        "seed": args.seed,
        "width": args.width,
        "height": args.height,
        "days_per_year": args.days_per_year,
        "civ_count": args.civs,
    });
    let (socket, child, state_path) = match &args.socket {
        Some(path) => (path.clone(), None, None),
        None => {
            std::fs::create_dir_all(&args.out)?;
            let path = std::env::temp_dir().join(format!("sim-{}.sock", std::process::id()));
            let state_path = args.out.join("state.json");
            let child = spawn_server(&path, &config, &state_path)?;
            wait_for_socket(&path)?;
            (path, Some(child), Some(state_path))
        }
    };
    let driven = drive(&socket, batch, target, child.is_some());
    let stopped = shutdown(&socket);
    if let Some(mut child) = child {
        let _ = child.wait();
        let _ = std::fs::remove_file(&socket);
    }
    driven?;
    stopped?;
    if let Some(state_path) = state_path {
        write_outputs(&args, ticks_per_year, batch, &state_path)?;
    }
    Ok(())
}

fn drive(
    socket: &Path,
    batch: u64,
    target: u64,
    spawned: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = IpcClient::connect(socket)?;
    if spawned {
        match client.call_ok(&Request::Status)? {
            OkBody::Status(status) => println!("created mundus at tick {}", status.tick),
            other => return Err(format!("unexpected status: {other:?}").into()),
        }
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

    Ok(())
}

fn write_outputs(
    args: &Args,
    ticks_per_year: u64,
    batch: u64,
    state_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let state: StateBody = serde_json::from_str(&std::fs::read_to_string(state_path)?)?;
    if let Some(text) = state.extras.get("chronicle_md").and_then(|v| v.as_str()) {
        std::fs::write(args.out.join("chronicle.md"), text)?;
    }
    if let Some(text) = state.extras.get("map_txt").and_then(|v| v.as_str()) {
        std::fs::write(args.out.join("map.txt"), text)?;
    }
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

fn shutdown(socket: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = IpcClient::connect(socket)?;
    match client.call_ok(&Request::Shutdown)? {
        OkBody::Shutdown => Ok(()),
        other => Err(format!("unexpected shutdown: {other:?}").into()),
    }
}

fn spawn_server(
    socket: &Path,
    config: &serde_json::Value,
    state_out: &Path,
) -> Result<Child, Box<dyn std::error::Error>> {
    let exe = std::env::current_exe()?;
    let bin = exe
        .parent()
        .map(|dir| dir.join("sim-server"))
        .filter(|path| path.exists())
        .unwrap_or_else(|| PathBuf::from("sim-server"));
    Command::new(&bin)
        .arg("--socket")
        .arg(socket)
        .arg("--state-out")
        .arg(state_out)
        .arg("create")
        .arg("--kind")
        .arg("mundus")
        .arg("--config")
        .arg(config.to_string())
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
