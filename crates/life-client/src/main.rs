use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use clap::Parser;
use serde_json::json;
use sim_core::IpcClient;
use sim_core::{AdvanceParams, CreateParams, OkBody, Request};

#[derive(Parser, Debug)]
#[command(
    name = "life-client",
    about = "Run Game of Life for a number of generations"
)]
struct Args {
    #[arg(long, default_value_t = 1)]
    seed: u64,
    #[arg(long, default_value_t = 50)]
    ticks: u64,
    #[arg(long, default_value_t = 32)]
    width: u32,
    #[arg(long, default_value_t = 32)]
    height: u32,
    #[arg(long, default_value_t = 0.35)]
    density: f64,
    #[arg(long, default_value = "out/life")]
    out: PathBuf,
    /// Attach to this socket instead of spawning sim-server.
    #[arg(short = 'S', long)]
    socket: Option<PathBuf>,
}

fn main() {
    if let Err(e) = run(Args::parse()) {
        eprintln!("life-client error: {e}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let (socket, child) = match &args.socket {
        Some(path) => (path.clone(), None),
        None => {
            let path = std::env::temp_dir().join(format!("sim-life-{}.sock", std::process::id()));
            let child = spawn_server(&path)?;
            wait_for_socket(&path)?;
            (path, Some(child))
        }
    };
    let result = drive(&args, &socket);
    if let Ok(mut client) = IpcClient::connect(&socket) {
        let _ = client.call_ok(&Request::Shutdown);
    }
    if let Some(mut child) = child {
        let _ = child.wait();
        let _ = std::fs::remove_file(&socket);
    }
    result
}

fn drive(args: &Args, socket: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = IpcClient::connect(socket)?;
    match client.call_ok(&Request::Create(CreateParams {
        kind: "game_of_life".into(),
        config: json!({
            "seed": args.seed,
            "width": args.width,
            "height": args.height,
            "density": args.density,
        }),
    }))? {
        OkBody::Created { tick } => println!("created game of life at tick {tick}"),
        other => return Err(format!("unexpected create: {other:?}").into()),
    }
    match client.call_ok(&Request::Advance(AdvanceParams { n: args.ticks }))? {
        OkBody::Advance(step) => println!("tick {} stopped {}", step.tick, step.stopped),
        other => return Err(format!("unexpected advance: {other:?}").into()),
    }
    let state = match client.call_ok(&Request::State)? {
        OkBody::State(state) => state,
        other => return Err(format!("unexpected state: {other:?}").into()),
    };
    std::fs::create_dir_all(&args.out)?;
    if let Some(text) = state.extras.get("grid_txt").and_then(|v| v.as_str()) {
        std::fs::write(args.out.join("grid.txt"), text)?;
    }
    std::fs::write(
        args.out.join("state.json"),
        serde_json::to_string_pretty(&state)?,
    )?;
    std::fs::write(
        args.out.join("manifest.json"),
        serde_json::to_string_pretty(&json!({
            "kind": "game_of_life",
            "seed": args.seed,
            "ticks": args.ticks,
            "width": args.width,
            "height": args.height,
            "density": args.density,
            "state_hash": state.state_hash,
        }))?,
    )?;
    println!("wrote {} ({})", args.out.display(), state.state_hash);
    Ok(())
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
