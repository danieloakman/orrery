use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use clap::{Parser, Subcommand};
use orrery_core::protocol::{AdvanceTickParams, CreateParams, OkBody, Request};
use orrery_core::{IpcClient, SimConfig};

#[derive(Parser, Debug)]
#[command(name = "orrery", about = "Orrery full-tilt simulation runner")]
struct Cli {
    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Create a world on the server, advance at full tilt, dump artifacts
    Run(RunArgs),
}

#[derive(Parser, Debug)]
struct RunArgs {
    #[arg(long, default_value_t = 1)]
    seed: u64,
    #[arg(long, default_value_t = 100)]
    years: u64,
    #[arg(long, default_value_t = 24)]
    ticks_per_day: u32,
    #[arg(long, default_value_t = 365)]
    days_per_year: u32,
    #[arg(long, default_value_t = 128)]
    width: u32,
    #[arg(long, default_value_t = 128)]
    height: u32,
    #[arg(long, default_value_t = 6)]
    civs: u8,
    /// Ticks per advance_tick RPC (default: one year)
    #[arg(long)]
    batch_ticks: Option<u64>,
    /// Connect to an existing server socket. If omitted, spawn orrery-server.
    #[arg(long)]
    socket: Option<PathBuf>,
    #[arg(long, default_value = "out")]
    out: PathBuf,
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = match cli.cmd {
        Commands::Run(args) => run(args),
    } {
        eprintln!("orrery error: {e}");
        std::process::exit(1);
    }
}

fn run(args: RunArgs) -> Result<(), Box<dyn std::error::Error>> {
    let config = SimConfig {
        seed: args.seed,
        width: args.width,
        height: args.height,
        ticks_per_day: args.ticks_per_day,
        days_per_year: args.days_per_year,
        civ_count: args.civs,
    };
    config.validate().map_err(|e| e)?;

    let ticks_per_year = config.ticks_per_year();
    let batch_n = args.batch_ticks.unwrap_or(ticks_per_year).max(1);
    let target_tick = args.years.saturating_mul(ticks_per_year);

    let (socket, child) = match &args.socket {
        Some(p) => (p.clone(), None),
        None => {
            let sock = std::env::temp_dir().join(format!("orrery-{}.sock", std::process::id()));
            let child = spawn_server(&sock)?;
            wait_for_socket(&sock, Duration::from_secs(10))?;
            (sock, Some(child))
        }
    };

    let mut client = IpcClient::connect(&socket)?;

    match client.call_ok(&Request::Create(CreateParams {
        config: config.clone(),
    }))? {
        OkBody::Created { civs, .. } => {
            println!("created world seed={} civs={civs}", config.seed);
        }
        other => return Err(format!("unexpected create response: {other:?}").into()),
    }

    loop {
        let status = match client.call_ok(&Request::Status)? {
            OkBody::Status(s) => s,
            other => return Err(format!("unexpected status: {other:?}").into()),
        };
        if status.tick >= target_tick {
            break;
        }
        let remaining = target_tick - status.tick;
        let n = remaining.min(batch_n);
        match client.call_ok(&Request::AdvanceTick(AdvanceTickParams { n }))? {
            OkBody::AdvanceTick(r) => {
                println!(
                    "year {}  tick {}  events+{}{}",
                    r.year,
                    r.tick,
                    r.events_len,
                    if r.truncated { " (truncated)" } else { "" }
                );
            }
            other => return Err(format!("unexpected advance: {other:?}").into()),
        }
    }

    let dump = match client.call_ok(&Request::Dump)? {
        OkBody::Dump(d) => d,
        other => return Err(format!("unexpected dump: {other:?}").into()),
    };

    std::fs::create_dir_all(&args.out)?;
    std::fs::write(args.out.join("chronicle.md"), &dump.chronicle_md)?;
    std::fs::write(args.out.join("map.txt"), &dump.map_txt)?;
    std::fs::write(args.out.join("state.json"), &dump.state_json)?;
    std::fs::write(args.out.join("manifest.json"), serde_json::to_string_pretty(&serde_json::json!({
        "seed": config.seed,
        "years": args.years,
        "ticks_per_day": config.ticks_per_day,
        "days_per_year": config.days_per_year,
        "width": config.width,
        "height": config.height,
        "civ_count": config.civ_count,
        "batch_ticks": batch_n,
        "state_hash": dump.state_hash,
    }))?)?;

    println!("wrote {} (hash {})", args.out.display(), dump.state_hash);

    let _ = client.call_ok(&Request::Shutdown);
    if let Some(mut child) = child {
        let _ = child.wait();
        let _ = std::fs::remove_file(&socket);
    }

    Ok(())
}

fn spawn_server(socket: &PathBuf) -> Result<Child, Box<dyn std::error::Error>> {
    let exe = std::env::current_exe()?;
    let server_exe = exe
        .parent()
        .map(|p| p.join("orrery-server"))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("orrery-server"));

    let child = Command::new(&server_exe)
        .arg("--socket")
        .arg(socket)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| {
            format!(
                "failed to spawn {}: {e}. Build with `cargo build -p orrery-server` or pass --socket",
                server_exe.display()
            )
        })?;
    Ok(child)
}

fn wait_for_socket(path: &PathBuf, timeout: Duration) -> Result<(), Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if path.exists() {
            if IpcClient::connect(path).is_ok() {
                return Ok(());
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(format!("timeout waiting for server at {}", path.display()).into())
}
