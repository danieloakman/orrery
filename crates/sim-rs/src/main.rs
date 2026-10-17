use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde_json::Value;
use sim_core::{Simulation, StateBody};
use sim_server::{serve, ServeConfig, Session};

#[derive(Parser, Debug)]
#[command(
    name = "sim-server",
    about = "Own one simulation and serve it on a Unix socket"
)]
struct Args {
    /// Unix socket path.
    #[arg(long, global = true, default_value = "/tmp/sim.sock")]
    socket: PathBuf,
    /// Mutating calls one connection may make on a single tick. 0 means unlimited.
    #[arg(long, global = true, default_value_t = 0)]
    max_actions_per_tick: u32,
    /// Write the save point here on shutdown.
    #[arg(long, global = true)]
    state_out: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Create a simulation and serve it.
    Create {
        /// Simulation name (`mundus` or `game_of_life`).
        #[arg(long)]
        kind: String,
        /// JSON object passed to the simulation.
        #[arg(long)]
        config: String,
    },
    /// Load a save point and serve it.
    Restore {
        /// Save point from a previous shutdown.
        #[arg(long)]
        file: PathBuf,
    },
}

fn main() {
    let args = Args::parse();
    let session = match session_from(&args.cmd) {
        Ok(session) => session,
        Err(e) => {
            eprintln!("sim-server error: {e}");
            std::process::exit(1);
        }
    };
    let config = ServeConfig {
        socket: args.socket,
        max_actions_per_tick: args.max_actions_per_tick,
        state_out: args.state_out,
    };
    if let Err(e) = serve(config, session) {
        eprintln!("sim-server error: {e}");
        std::process::exit(1);
    }
}

fn session_from(cmd: &Cmd) -> Result<Session, Box<dyn std::error::Error>> {
    match cmd {
        Cmd::Create { kind, config } => {
            let config: Value = serde_json::from_str(config)?;
            let sim = create_sim(kind, config)?;
            let tick = sim.tick();
            println!("created {kind} at tick {tick}");
            Ok(Session::new(sim))
        }
        Cmd::Restore { file } => {
            let text = std::fs::read_to_string(file)?;
            let state: StateBody = serde_json::from_str(&text)?;
            // The journal is kept as the history of edits. It is not replayed.
            let sim = restore_sim(&state.kind, state.sim)?;
            let tick = sim.tick();
            let mut session = Session::new(sim);
            session.install_history(state.journal, state.scheduled);
            println!("restored {} at tick {tick}", state.kind);
            Ok(session)
        }
    }
}

fn create_sim(kind: &str, config: Value) -> Result<Box<dyn Simulation>, String> {
    match kind {
        "mundus" => Ok(Box::new(mundus::World::from_config_value(config)?)),
        "game_of_life" => Ok(Box::new(life::Life::from_config_value(config)?)),
        other => Err(format!("unknown simulation {other}")),
    }
}

fn restore_sim(kind: &str, sim: Value) -> Result<Box<dyn Simulation>, String> {
    match kind {
        "mundus" => Ok(Box::new(mundus::World::from_sim_value(sim)?)),
        "game_of_life" => Ok(Box::new(life::Life::from_sim_value(sim)?)),
        other => Err(format!("unknown simulation {other}")),
    }
}
