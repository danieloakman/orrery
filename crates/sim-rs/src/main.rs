use std::path::PathBuf;

use clap::Parser;
use serde_json::Value;
use sim_core::Simulation;
use sim_server::{serve, ServeConfig};

#[derive(Parser, Debug)]
#[command(name = "sim-server", about = "Simulation host (Unix socket)")]
struct Args {
    /// Unix socket path.
    #[arg(long, default_value = "/tmp/sim.sock")]
    socket: PathBuf,
    /// Mutating calls one connection may make on a single tick. 0 means unlimited.
    #[arg(long, default_value_t = 0)]
    max_actions_per_tick: u32,
}

fn main() {
    let args = Args::parse();
    let config = ServeConfig {
        socket: args.socket,
        max_actions_per_tick: args.max_actions_per_tick,
    };
    if let Err(e) = serve(config, create_sim, restore_sim) {
        eprintln!("sim-server error: {e}");
        std::process::exit(1);
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
