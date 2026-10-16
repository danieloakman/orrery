use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde_json::Value;
use sim_core::IpcClient;
use sim_core::{CallParams, CreateParams, OkBody, Request, RestoreParams, StateBody};

const DEFAULT_SOCKET: &str = "/tmp/sim.sock";

#[derive(Parser, Debug)]
#[command(
    name = "sim-ctl",
    about = "Send requests to a running simulation server"
)]
struct Cli {
    /// Unix socket of a running sim-server.
    #[arg(short = 'S', long, global = true, default_value = DEFAULT_SOCKET)]
    socket: PathBuf,
    /// Print raw JSON responses.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Create a session. The server must not already have one.
    Create {
        #[arg(long)]
        kind: String,
        /// JSON object passed to the simulation.
        #[arg(long)]
        config: String,
    },
    /// List calls the current session supports.
    Actions,
    /// Run or schedule a call advertised by `actions`.
    Call {
        name: String,
        /// JSON object of call parameters.
        #[arg(long, default_value = "{}")]
        params: String,
        /// Run the call when the simulation reaches this tick.
        #[arg(long)]
        at_tick: Option<u64>,
    },
    /// Clock and simulation summary.
    Status,
    /// Write the save point to stdout.
    State,
    /// Load a save point. The server must not already have a session.
    Restore {
        #[arg(long)]
        file: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("sim-ctl error: {e}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = IpcClient::connect(&cli.socket)?;
    let body = match cli.cmd {
        Cmd::Create { kind, config } => {
            let config: Value = serde_json::from_str(&config)?;
            client.call_ok(&Request::Create(CreateParams { kind, config }))?
        }
        Cmd::Actions => client.call_ok(&Request::Actions)?,
        Cmd::Call {
            name,
            params,
            at_tick,
        } => {
            let params: Value = serde_json::from_str(&params)?;
            client.call_ok(&Request::Call(CallParams {
                name,
                params,
                at_tick,
            }))?
        }
        Cmd::Status => client.call_ok(&Request::Status)?,
        Cmd::State => client.call_ok(&Request::State)?,
        Cmd::Restore { file } => {
            let text = std::fs::read_to_string(&file)?;
            let state: StateBody = serde_json::from_str(&text)?;
            client.call_ok(&Request::Restore(RestoreParams {
                kind: state.kind,
                sim: state.sim,
                journal: state.journal,
                scheduled: state.scheduled,
            }))?
        }
    };
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&body)?);
        return Ok(());
    }
    match body {
        OkBody::Created { tick } => println!("created at tick {tick}"),
        OkBody::Status(status) => {
            println!(
                "{} tick {} stopped {}",
                status.kind, status.tick, status.stopped
            );
            println!("{}", serde_json::to_string_pretty(&status.summary)?);
        }
        OkBody::Actions { actions } => {
            for action in actions {
                let ret = match action.returns {
                    sim_core::CallReturns::Void => "void",
                    sim_core::CallReturns::Data => "data",
                };
                let mode = if action.mutating { "mutating" } else { "read" };
                println!("{} ({mode}, {ret}) — {}", action.name, action.summary);
            }
        }
        OkBody::Call(call) => {
            if let Some(tick) = call.at_tick {
                println!("scheduled at tick {tick}");
            }
            if let Some(data) = call.data {
                println!("{}", serde_json::to_string_pretty(&data)?);
            } else if call.at_tick.is_none() {
                println!("ok");
            }
        }
        OkBody::State(state) => println!("{}", serde_json::to_string_pretty(&state)?),
        OkBody::Restored { tick } => println!("restored at tick {tick}"),
        OkBody::Advance(_) | OkBody::Shutdown => {}
    }
    Ok(())
}
