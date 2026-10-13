use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use clap::Parser;
use mundus_core::protocol::{
    cap_events, civ_summary, filter_chronicle, resolve_civ, AdvanceTickResult, ChronicleParams,
    ChronicleResult, CreateParams, DumpResult, GetCivParams, GetCivResult, ListCivsResult, OkBody,
    Request, Response, StatusResult, CHRONICLE_DEFAULT_LIMIT,
};
use mundus_core::{build_dump, SimConfig, World};

#[derive(Parser, Debug)]
#[command(
    name = "mundus-server",
    about = "Mundus simulation server (Unix socket)"
)]
struct Args {
    /// Unix socket path
    #[arg(long, default_value = "/tmp/mundus.sock")]
    socket: PathBuf,
}

struct ServerState {
    world: Option<World>,
    shutdown: bool,
    socket: PathBuf,
}

fn main() {
    let args = Args::parse();
    if let Err(e) = run(args) {
        eprintln!("mundus-server error: {e}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let path = args.socket;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    // Accept with timeout so we can notice shutdown without a wake connect.
    listener.set_nonblocking(false)?;
    println!("mundus-server listening on {}", path.display());

    let state = Arc::new(Mutex::new(ServerState {
        world: None,
        shutdown: false,
        socket: path.clone(),
    }));

    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let state = Arc::clone(&state);
                thread::spawn(move || {
                    if let Err(e) = handle_client(stream, &state) {
                        eprintln!("client error: {e}");
                    }
                });
            }
            Err(e) => {
                eprintln!("accept error: {e}");
            }
        }
        if state.lock().map(|s| s.shutdown).unwrap_or(false) {
            break;
        }
    }

    let _ = std::fs::remove_file(&path);
    println!("mundus-server shutdown");
    Ok(())
}

fn handle_client(
    stream: UnixStream,
    state: &Arc<Mutex<ServerState>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }
        let req: Request = match serde_json::from_str(line.trim_end()) {
            Ok(r) => r,
            Err(e) => {
                write_resp(
                    &mut writer,
                    Response::Err {
                        error: e.to_string(),
                    },
                )?;
                continue;
            }
        };

        let (resp, do_shutdown) = dispatch(state, req);
        write_resp(&mut writer, resp)?;
        if do_shutdown {
            break;
        }
    }
    Ok(())
}

fn write_resp(writer: &mut UnixStream, resp: Response) -> Result<(), Box<dyn std::error::Error>> {
    let mut s = serde_json::to_string(&resp)?;
    s.push('\n');
    writer.write_all(s.as_bytes())?;
    writer.flush()?;
    Ok(())
}

fn dispatch(state: &Arc<Mutex<ServerState>>, req: Request) -> (Response, bool) {
    let mut guard = match state.lock() {
        Ok(g) => g,
        Err(_) => {
            return (
                Response::Err {
                    error: "lock poisoned".into(),
                },
                false,
            )
        }
    };

    match req {
        Request::Create(CreateParams { config }) => match create_world(&mut guard, config) {
            Ok(body) => (Response::Ok(body), false),
            Err(e) => (Response::Err { error: e }, false),
        },
        Request::Status => match status(&guard) {
            Ok(body) => (Response::Ok(body), false),
            Err(e) => (Response::Err { error: e }, false),
        },
        Request::ListCivs => match list_civs(&guard) {
            Ok(body) => (Response::Ok(body), false),
            Err(e) => (Response::Err { error: e }, false),
        },
        Request::GetCiv(p) => match get_civ(&guard, p) {
            Ok(body) => (Response::Ok(body), false),
            Err(e) => (Response::Err { error: e }, false),
        },
        Request::Chronicle(p) => match chronicle(&guard, p) {
            Ok(body) => (Response::Ok(body), false),
            Err(e) => (Response::Err { error: e }, false),
        },
        Request::AdvanceTick(p) => match advance(&mut guard, p.n) {
            Ok(body) => (Response::Ok(body), false),
            Err(e) => (Response::Err { error: e }, false),
        },
        Request::Dump => match dump_world(&guard) {
            Ok(body) => (Response::Ok(body), false),
            Err(e) => (Response::Err { error: e }, false),
        },
        Request::Shutdown => {
            let sock = guard.socket.clone();
            guard.shutdown = true;
            drop(guard);
            // Unblock accept loop
            let _ = UnixStream::connect(sock);
            (Response::Ok(OkBody::Shutdown), true)
        }
    }
}

fn create_world(state: &mut ServerState, config: SimConfig) -> Result<OkBody, String> {
    if state.world.is_some() {
        return Err("world already created".into());
    }
    let world = World::create(config).map_err(|e| e.to_string())?;
    let civs = world.civs.len();
    let tick = world.clock.tick;
    state.world = Some(world);
    Ok(OkBody::Created { tick, civs })
}

fn status(state: &ServerState) -> Result<OkBody, String> {
    let w = state.world.as_ref().ok_or("no world; call create first")?;
    Ok(OkBody::Status(StatusResult {
        tick: w.clock.tick,
        day: w.clock.day(),
        year: w.clock.year(),
        hour: w.clock.hour(),
        minute: w.clock.minute(),
        days_per_year: w.config.days_per_year,
        civ_count: w.civs.len(),
        alive_civs: w.alive_civ_count(),
        event_count: w.events.len(),
    }))
}

fn list_civs(state: &ServerState) -> Result<OkBody, String> {
    let w = state.world.as_ref().ok_or("no world; call create first")?;
    let mut civs: Vec<_> = w.civs.iter().map(civ_summary).collect();
    civs.sort_by_key(|c| c.id);
    Ok(OkBody::ListCivs(ListCivsResult {
        tick: w.clock.tick,
        civs,
    }))
}

fn get_civ(state: &ServerState, params: GetCivParams) -> Result<OkBody, String> {
    let w = state.world.as_ref().ok_or("no world; call create first")?;
    let civ = resolve_civ(&w.civs, &params.query)?.clone();
    Ok(OkBody::GetCiv(GetCivResult {
        tick: w.clock.tick,
        civ,
    }))
}

fn chronicle(state: &ServerState, params: ChronicleParams) -> Result<OkBody, String> {
    let w = state.world.as_ref().ok_or("no world; call create first")?;
    let since_tick = params.since_tick.unwrap_or(0);
    let limit = params.limit.unwrap_or(CHRONICLE_DEFAULT_LIMIT).max(1);
    let slice = filter_chronicle(&w.events, since_tick, limit);
    Ok(OkBody::Chronicle(ChronicleResult {
        tick: w.clock.tick,
        since_tick,
        events: slice.events,
        total_matching: slice.total_matching,
        truncated: slice.truncated,
    }))
}

fn advance(state: &mut ServerState, n: u64) -> Result<OkBody, String> {
    let w = state.world.as_mut().ok_or("no world; call create first")?;
    let batch = w.advance_ticks(n);
    let events_len = batch.len();
    let (events, truncated) = cap_events(&batch);
    Ok(OkBody::AdvanceTick(AdvanceTickResult {
        tick: w.clock.tick,
        day: w.clock.day(),
        year: w.clock.year(),
        events_len,
        events,
        truncated,
    }))
}

fn dump_world(state: &ServerState) -> Result<OkBody, String> {
    let w = state.world.as_ref().ok_or("no world; call create first")?;
    let arts = build_dump(w).map_err(|e| e.to_string())?;
    Ok(OkBody::Dump(DumpResult {
        chronicle_md: arts.chronicle_md,
        map_txt: arts.map_txt,
        state_json: arts.state_json,
        state_hash: arts.state_hash,
    }))
}
