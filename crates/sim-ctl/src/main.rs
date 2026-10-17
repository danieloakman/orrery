use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde_json::Value;
use sim_core::IpcClient;
use sim_core::{CallParams, OkBody, Request};

const DEFAULT_SOCKET: &str = "/tmp/sim.sock";

#[derive(Parser, Debug)]
#[command(name = "sim-ctl", about = "Connect to a simulation server as a player")]
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
    };
    if cli.json {
        println!("{}", serde_json::to_string_pretty(&body)?);
        return Ok(());
    }
    match body {
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
        OkBody::Advance(_) | OkBody::Shutdown => {
            return Err("unexpected server response".into());
        }
    }
    Ok(())
}
