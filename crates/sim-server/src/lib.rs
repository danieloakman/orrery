//! Socket host. One process, one session, many connections.
//!
//! The process owns the simulation. Callers build a [`Session`] (create or
//! restore) and pass it in. Players send `status`, `actions`, and `call`.

mod session;

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::thread;

use sim_core::{AdvanceParams, CallParams, OkBody, Request, Response, StateBody};

pub use session::{hash_sim, Session};

pub struct ServeConfig {
    pub socket: PathBuf,
    /// `0` means a connection may make any number of mutating calls on a tick.
    pub max_actions_per_tick: u32,
    /// Save point written on `shutdown`.
    pub state_out: Option<PathBuf>,
}

struct ServerInner {
    session: Session,
    shutdown: bool,
    socket: PathBuf,
    state_out: Option<PathBuf>,
    max_actions_per_tick: u32,
}

/// Listen until a client sends `shutdown`. The session already exists.
pub fn serve(config: ServeConfig, session: Session) -> Result<(), Box<dyn std::error::Error>> {
    let path = config.socket;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    println!("sim-server listening on {}", path.display());

    let state = Arc::new(RwLock::new(ServerInner {
        session,
        shutdown: false,
        socket: path.clone(),
        state_out: config.state_out,
        max_actions_per_tick: config.max_actions_per_tick,
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
            Err(e) => eprintln!("accept error: {e}"),
        }
        if state.read().map(|s| s.shutdown).unwrap_or(false) {
            break;
        }
    }

    let _ = std::fs::remove_file(&path);
    println!("sim-server shutdown");
    Ok(())
}

fn handle_client(
    stream: UnixStream,
    state: &Arc<RwLock<ServerInner>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;
    let mut line = String::new();
    let mut budget = session::Budget::default();

    loop {
        line.clear();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }
        let req: Request = match serde_json::from_str(line.trim_end()) {
            Ok(req) => req,
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
        let (resp, shutdown) = dispatch(state, &mut budget, req);
        write_resp(&mut writer, resp)?;
        if shutdown {
            break;
        }
    }
    Ok(())
}

fn write_resp(writer: &mut UnixStream, resp: Response) -> Result<(), Box<dyn std::error::Error>> {
    let mut line = serde_json::to_string(&resp)?;
    line.push('\n');
    writer.write_all(line.as_bytes())?;
    writer.flush()?;
    Ok(())
}

fn dispatch(
    state: &Arc<RwLock<ServerInner>>,
    budget: &mut session::Budget,
    req: Request,
) -> (Response, bool) {
    let readonly = matches!(req, Request::Status | Request::Actions | Request::Call(_));
    if readonly {
        if let Request::Call(ref params) = req {
            if let Some(resp) = call_readonly(state, params) {
                return (resp, false);
            }
        } else {
            return (read_dispatch(state, &req), false);
        }
    }
    write_dispatch(state, budget, req)
}

fn call_readonly(state: &Arc<RwLock<ServerInner>>, params: &CallParams) -> Option<Response> {
    let guard = match state.read() {
        Ok(guard) => guard,
        Err(_) => return Some(err("lock poisoned")),
    };
    let session = &guard.session;
    let spec = session
        .actions()
        .into_iter()
        .find(|a| a.name == params.name)?;
    if spec.mutating || params.at_tick.is_some() {
        return None;
    }
    let result = session.query(&params.name, params.params_or_empty());
    Some(match result {
        Ok(body) => Response::Ok(OkBody::Call(body)),
        Err(error) => Response::Err { error },
    })
}

fn read_dispatch(state: &Arc<RwLock<ServerInner>>, req: &Request) -> Response {
    let guard = match state.read() {
        Ok(guard) => guard,
        Err(_) => return err("lock poisoned"),
    };
    let session = &guard.session;
    match req {
        Request::Status => Response::Ok(OkBody::Status(session.status())),
        Request::Actions => Response::Ok(OkBody::Actions {
            actions: session.actions(),
        }),
        _ => err("internal: not a read"),
    }
}

fn write_dispatch(
    state: &Arc<RwLock<ServerInner>>,
    budget: &mut session::Budget,
    req: Request,
) -> (Response, bool) {
    let mut guard = match state.write() {
        Ok(guard) => guard,
        Err(_) => return (err("lock poisoned"), false),
    };
    match req {
        Request::Call(params) => {
            let limit = guard.max_actions_per_tick;
            match guard.session.call(
                &params.name,
                params.params_or_empty(),
                params.at_tick,
                budget,
                limit,
            ) {
                Ok(body) => (Response::Ok(OkBody::Call(body)), false),
                Err(error) => (err(error), false),
            }
        }
        Request::Advance(AdvanceParams { n }) => match guard.session.advance(n) {
            Ok(body) => (Response::Ok(OkBody::Advance(body)), false),
            Err(error) => (err(error), false),
        },
        Request::Shutdown => {
            let socket = guard.socket.clone();
            let write_error = match guard.state_out.as_ref() {
                Some(path) => write_state(path, &guard.session.state()).err(),
                None => None,
            };
            guard.shutdown = true;
            drop(guard);
            let _ = UnixStream::connect(socket);
            match write_error {
                Some(error) => (err(error), true),
                None => (Response::Ok(OkBody::Shutdown), true),
            }
        }
        Request::Status | Request::Actions => (err("internal: read on write path"), false),
    }
}

fn write_state(path: &Path, state: &StateBody) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }
    let mut json = serde_json::to_string_pretty(state).map_err(|e| e.to_string())?;
    json.push('\n');
    std::fs::write(path, json).map_err(|e| e.to_string())
}

fn err(error: impl Into<String>) -> Response {
    Response::Err {
        error: error.into(),
    }
}
