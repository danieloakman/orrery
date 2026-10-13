use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use clap::{Parser, Subcommand};
use mundus_core::protocol::{
    AdvanceTickParams, ChronicleParams, CreateParams, GetCivParams, OkBody, Request,
};
use mundus_core::{IpcClient, SimConfig, TICKS_PER_DAY, TICKS_PER_HOUR};

const DEFAULT_SOCKET: &str = "/tmp/mundus.sock";

#[derive(Parser, Debug)]
#[command(
    name = "mundus",
    about = "Mundus simulation client — full-tilt runner and agent CLI"
)]
struct Cli {
    /// Unix socket of a running mundus-server (agent commands).
    /// For `run`, omit to spawn a private server.
    #[arg(short = 'S', long, global = true)]
    socket: Option<PathBuf>,

    /// Emit machine-readable JSON instead of text summaries.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    cmd: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Create a world on the server, advance at full tilt, dump artifacts
    Run(RunArgs),
    /// Create a world on a running server (does not advance time)
    Create(CreateArgs),
    /// World clock and civ counts
    Status,
    /// List all civilizations
    Civs,
    /// Inspect one civilization by id or name
    Civ {
        /// Civ id or exact name
        query: String,
    },
    /// Chronicle events (ascending), optionally since a tick
    Chronicle {
        #[arg(long)]
        since: Option<u64>,
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Advance simulation time on a running server
    Advance(AdvanceArgs),
    /// Dump chronicle/map/state to a directory
    Dump {
        #[arg(long, default_value = "out")]
        out: PathBuf,
    },
    /// Shut down the server
    Shutdown,
}

#[derive(Parser, Debug)]
struct RunArgs {
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
    /// Ticks (minutes) per advance_tick RPC (default: one year)
    #[arg(long)]
    batch_ticks: Option<u64>,
    #[arg(long, default_value = "out")]
    out: PathBuf,
}

#[derive(Parser, Debug)]
struct CreateArgs {
    #[arg(long, default_value_t = 1)]
    seed: u64,
    #[arg(long, default_value_t = 365)]
    days_per_year: u32,
    #[arg(long, default_value_t = 128)]
    width: u32,
    #[arg(long, default_value_t = 128)]
    height: u32,
    #[arg(long, default_value_t = 6)]
    civs: u8,
}

#[derive(Parser, Debug)]
struct AdvanceArgs {
    /// Advance by this many ticks (minutes)
    #[arg(long, group = "amount")]
    ticks: Option<u64>,
    /// Advance by this many days
    #[arg(long, group = "amount")]
    days: Option<u64>,
    /// Advance by this many years (default if no amount flag)
    #[arg(long, group = "amount")]
    years: Option<u64>,
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = dispatch(cli) {
        eprintln!("mundus error: {e}");
        std::process::exit(1);
    }
}

fn dispatch(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.cmd {
        Commands::Run(args) => run_full_tilt(args, cli.socket),
        Commands::Create(args) => {
            let mut client = connect_existing(cli.socket.as_ref())?;
            cmd_create(&mut client, args, cli.json)
        }
        Commands::Status => {
            let mut client = connect_existing(cli.socket.as_ref())?;
            cmd_status(&mut client, cli.json)
        }
        Commands::Civs => {
            let mut client = connect_existing(cli.socket.as_ref())?;
            cmd_civs(&mut client, cli.json)
        }
        Commands::Civ { query } => {
            let mut client = connect_existing(cli.socket.as_ref())?;
            cmd_civ(&mut client, query, cli.json)
        }
        Commands::Chronicle { since, limit } => {
            let mut client = connect_existing(cli.socket.as_ref())?;
            cmd_chronicle(&mut client, since, limit, cli.json)
        }
        Commands::Advance(args) => {
            let mut client = connect_existing(cli.socket.as_ref())?;
            cmd_advance(&mut client, args, cli.json)
        }
        Commands::Dump { out } => {
            let mut client = connect_existing(cli.socket.as_ref())?;
            cmd_dump(&mut client, out, cli.json)
        }
        Commands::Shutdown => {
            let mut client = connect_existing(cli.socket.as_ref())?;
            cmd_shutdown(&mut client, cli.json)
        }
    }
}

fn agent_socket(socket: Option<&PathBuf>) -> PathBuf {
    socket
        .cloned()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET))
}

fn connect_existing(socket: Option<&PathBuf>) -> Result<IpcClient, Box<dyn std::error::Error>> {
    let path = agent_socket(socket);
    IpcClient::connect(&path).map_err(|e| {
        format!(
            "connect {}: {e}. Start a server with `just serve` (or pass -S/--socket)",
            path.display()
        )
        .into()
    })
}

fn print_json(value: &impl serde::Serialize) -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn cmd_create(
    client: &mut IpcClient,
    args: CreateArgs,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = SimConfig {
        seed: args.seed,
        width: args.width,
        height: args.height,
        days_per_year: args.days_per_year,
        civ_count: args.civs,
    };
    config.validate()?;
    let body = client.call_ok(&Request::Create(CreateParams {
        config: config.clone(),
    }))?;
    if json {
        return print_json(&body);
    }
    match body {
        OkBody::Created { tick, civs } => {
            println!(
                "created seed={} civs={civs} tick={tick} (1 tick = 1 minute, {} ticks/day)",
                config.seed, TICKS_PER_DAY
            );
            Ok(())
        }
        other => Err(format!("unexpected create response: {other:?}").into()),
    }
}

fn cmd_status(client: &mut IpcClient, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let body = client.call_ok(&Request::Status)?;
    if json {
        return print_json(&body);
    }
    match body {
        OkBody::Status(s) => {
            println!(
                "year {}  day {}  {:02}:{:02}  tick {}  civs {}/{} alive  events {}",
                s.year, s.day, s.hour, s.minute, s.tick, s.alive_civs, s.civ_count, s.event_count
            );
            Ok(())
        }
        other => Err(format!("unexpected status: {other:?}").into()),
    }
}

fn cmd_civs(client: &mut IpcClient, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let body = client.call_ok(&Request::ListCivs)?;
    if json {
        return print_json(&body);
    }
    match body {
        OkBody::ListCivs(r) => {
            println!("tick {}  {} civs", r.tick, r.civs.len());
            for c in &r.civs {
                let flag = if c.alive { "alive" } else { "dead " };
                println!(
                    "  {:>3}  {:<16}  {flag}  pop {:>5}  food {:>6.1}  tech {:>4.2}  str {:>5.1}  sites {}",
                    c.id, c.name, c.population, c.food, c.tech, c.strength, c.settlements
                );
            }
            Ok(())
        }
        other => Err(format!("unexpected list_civs: {other:?}").into()),
    }
}

fn cmd_civ(
    client: &mut IpcClient,
    query: String,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let body = client.call_ok(&Request::GetCiv(GetCivParams { query }))?;
    if json {
        return print_json(&body);
    }
    match body {
        OkBody::GetCiv(r) => {
            let c = &r.civ;
            let flag = if c.alive { "alive" } else { "dead" };
            println!("tick {}  civ {} ({})  {flag}", r.tick, c.id, c.name);
            println!(
                "  population {}  food {:.1}  tech {:.2}  strength {:.1}",
                c.population, c.food, c.tech, c.strength
            );
            println!("  settlements:");
            for s in &c.settlements {
                println!("    ({},{})  {:?}", s.x, s.y, s.kind);
            }
            Ok(())
        }
        other => Err(format!("unexpected get_civ: {other:?}").into()),
    }
}

fn cmd_chronicle(
    client: &mut IpcClient,
    since: Option<u64>,
    limit: Option<usize>,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let body = client.call_ok(&Request::Chronicle(ChronicleParams {
        since_tick: since,
        limit,
    }))?;
    if json {
        return print_json(&body);
    }
    match body {
        OkBody::Chronicle(r) => {
            println!(
                "tick {}  since {}  showing {}/{}{}",
                r.tick,
                r.since_tick,
                r.events.len(),
                r.total_matching,
                if r.truncated { " (truncated)" } else { "" }
            );
            for e in &r.events {
                println!("  y{} d{} t{}  {}", e.year, e.day, e.tick, e.kind.summary());
            }
            Ok(())
        }
        other => Err(format!("unexpected chronicle: {other:?}").into()),
    }
}

fn cmd_advance(
    client: &mut IpcClient,
    args: AdvanceArgs,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let n = resolve_advance_ticks(client, &args)?;
    let body = client.call_ok(&Request::AdvanceTick(AdvanceTickParams { n }))?;
    if json {
        return print_json(&body);
    }
    match body {
        OkBody::AdvanceTick(r) => {
            let hour = (r.tick / u64::from(TICKS_PER_HOUR)) % 24;
            println!(
                "year {}  day {}  hour {}  tick {}  events+{}{}",
                r.year,
                r.day,
                hour,
                r.tick,
                r.events_len,
                if r.truncated { " (truncated)" } else { "" }
            );
            Ok(())
        }
        other => Err(format!("unexpected advance: {other:?}").into()),
    }
}

fn resolve_advance_ticks(
    client: &mut IpcClient,
    args: &AdvanceArgs,
) -> Result<u64, Box<dyn std::error::Error>> {
    if let Some(ticks) = args.ticks {
        return Ok(ticks.max(1));
    }
    if let Some(days) = args.days {
        return Ok(days.saturating_mul(u64::from(TICKS_PER_DAY)).max(1));
    }
    let days_per_year = match client.call_ok(&Request::Status)? {
        OkBody::Status(s) => s.days_per_year,
        other => return Err(format!("unexpected status: {other:?}").into()),
    };
    let years = args.years.unwrap_or(1).max(1);
    Ok(years
        .saturating_mul(u64::from(days_per_year))
        .saturating_mul(u64::from(TICKS_PER_DAY)))
}

fn cmd_dump(
    client: &mut IpcClient,
    out: PathBuf,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let body = client.call_ok(&Request::Dump)?;
    match body {
        OkBody::Dump(d) => {
            std::fs::create_dir_all(&out)?;
            std::fs::write(out.join("chronicle.md"), &d.chronicle_md)?;
            std::fs::write(out.join("map.txt"), &d.map_txt)?;
            std::fs::write(out.join("state.json"), &d.state_json)?;
            if json {
                print_json(&serde_json::json!({
                    "out": out,
                    "state_hash": d.state_hash,
                }))?;
            } else {
                println!("wrote {} (hash {})", out.display(), d.state_hash);
            }
            Ok(())
        }
        other => Err(format!("unexpected dump: {other:?}").into()),
    }
}

fn cmd_shutdown(client: &mut IpcClient, json: bool) -> Result<(), Box<dyn std::error::Error>> {
    let body = client.call_ok(&Request::Shutdown)?;
    if json {
        return print_json(&body);
    }
    match body {
        OkBody::Shutdown => {
            println!("server shutdown");
            Ok(())
        }
        other => Err(format!("unexpected shutdown: {other:?}").into()),
    }
}

fn run_full_tilt(
    args: RunArgs,
    socket_opt: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
    let config = SimConfig {
        seed: args.seed,
        width: args.width,
        height: args.height,
        days_per_year: args.days_per_year,
        civ_count: args.civs,
    };
    config.validate()?;

    let ticks_per_year = config.ticks_per_year();
    let batch_n = args.batch_ticks.unwrap_or(ticks_per_year).max(1);
    let target_tick = args.years.saturating_mul(ticks_per_year);

    let (socket, child) = match &socket_opt {
        Some(p) => (p.clone(), None),
        None => {
            let sock = std::env::temp_dir().join(format!("mundus-{}.sock", std::process::id()));
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
            println!(
                "created world seed={} civs={civs} (1 tick = 1 minute, {} ticks/day)",
                config.seed, TICKS_PER_DAY
            );
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
                let hour = (r.tick / u64::from(TICKS_PER_HOUR)) % 24;
                println!(
                    "year {}  day {}  hour {}  tick {}  events+{}{}",
                    r.year,
                    r.day,
                    hour,
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
    std::fs::write(
        args.out.join("manifest.json"),
        serde_json::to_string_pretty(&serde_json::json!({
            "seed": config.seed,
            "years": args.years,
            "ticks_per_day": TICKS_PER_DAY,
            "tick_unit": "minute",
            "days_per_year": config.days_per_year,
            "width": config.width,
            "height": config.height,
            "civ_count": config.civ_count,
            "batch_ticks": batch_n,
            "state_hash": dump.state_hash,
        }))?,
    )?;

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
        .map(|p| p.join("mundus-server"))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("mundus-server"));

    let child = Command::new(&server_exe)
        .arg("--socket")
        .arg(socket)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| {
            format!(
                "failed to spawn {}: {e}. Build with `cargo build -p mundus-server` or pass --socket",
                server_exe.display()
            )
        })?;
    Ok(child)
}

fn wait_for_socket(path: &PathBuf, timeout: Duration) -> Result<(), Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if path.exists() && IpcClient::connect(path).is_ok() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(format!("timeout waiting for server at {}", path.display()).into())
}
