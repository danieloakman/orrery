//! Socket host. One process, one session, many connections.

mod session;

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::thread;

use serde_json::Value;
use sim_core::{
    AdvanceParams, CallParams, CreateParams, OkBody, Request, Response, RestoreParams, Simulation,
};

pub use session::{hash_sim, Session};

pub struct ServeConfig {
    pub socket: PathBuf,
    /// `0` means a connection may make any number of mutating calls on a tick.
    pub max_actions_per_tick: u32,
}

type Factory = Arc<dyn Fn(&str, Value) -> Result<Box<dyn Simulation>, String> + Send + Sync>;

struct ServerInner {
    session: Option<Session>,
    shutdown: bool,
    socket: PathBuf,
    max_actions_per_tick: u32,
    create: Factory,
    restore: Factory,
}

/// Listen until a client sends `shutdown`.
pub fn serve(
    config: ServeConfig,
    create: impl Fn(&str, Value) -> Result<Box<dyn Simulation>, String> + Send + Sync + 'static,
    restore: impl Fn(&str, Value) -> Result<Box<dyn Simulation>, String> + Send + Sync + 'static,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = config.socket;
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    println!("sim-server listening on {}", path.display());

    let state = Arc::new(RwLock::new(ServerInner {
        session: None,
        shutdown: false,
        socket: path.clone(),
        max_actions_per_tick: config.max_actions_per_tick,
        create: Arc::new(create),
        restore: Arc::new(restore),
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
    let readonly = matches!(
        req,
        Request::Status | Request::Actions | Request::State | Request::Call(_)
    );
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
    let session = guard.session.as_ref()?;
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
    let Some(session) = guard.session.as_ref() else {
        return err("no session; call create or restore first");
    };
    match req {
        Request::Status => Response::Ok(OkBody::Status(session.status())),
        Request::Actions => Response::Ok(OkBody::Actions {
            actions: session.actions(),
        }),
        Request::State => Response::Ok(OkBody::State(session.state())),
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
        Request::Create(CreateParams { kind, config }) => {
            if guard.session.is_some() {
                return (err("session already created"), false);
            }
            match (guard.create)(&kind, config) {
                Ok(sim) => {
                    let tick = sim.tick();
                    guard.session = Some(Session::new(sim));
                    (Response::Ok(OkBody::Created { tick }), false)
                }
                Err(error) => (err(error), false),
            }
        }
        Request::Restore(params) => (restore(&mut guard, params), false),
        Request::Call(params) => {
            let limit = guard.max_actions_per_tick;
            let Some(session) = guard.session.as_mut() else {
                return (err("no session; call create or restore first"), false);
            };
            match session.call(
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
        Request::Advance(AdvanceParams { n }) => {
            let Some(session) = guard.session.as_mut() else {
                return (err("no session; call create or restore first"), false);
            };
            match session.advance(n) {
                Ok(body) => (Response::Ok(OkBody::Advance(body)), false),
                Err(error) => (err(error), false),
            }
        }
        Request::Shutdown => {
            let socket = guard.socket.clone();
            guard.shutdown = true;
            drop(guard);
            let _ = UnixStream::connect(socket);
            (Response::Ok(OkBody::Shutdown), true)
        }
        Request::Status | Request::Actions | Request::State => {
            (err("internal: read on write path"), false)
        }
    }
}

fn restore(guard: &mut ServerInner, params: RestoreParams) -> Response {
    if guard.session.is_some() {
        return err("session already created");
    }
    // The journal is kept as the history of edits. It is not replayed to prove it matches `sim`.
    match (guard.restore)(&params.kind, params.sim) {
        Ok(sim) => {
            let tick = sim.tick();
            let mut session = Session::new(sim);
            session.install_history(params.journal, params.scheduled);
            guard.session = Some(session);
            Response::Ok(OkBody::Restored { tick })
        }
        Err(error) => err(error),
    }
}

fn err(error: impl Into<String>) -> Response {
    Response::Err {
        error: error.into(),
    }
}
