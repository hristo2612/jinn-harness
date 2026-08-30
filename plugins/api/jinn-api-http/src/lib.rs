//! The `jinn-api` HTTP provider. Transport only: one `jinn:net` loopback
//! listener at the configured port (the grant's bind range is the profile
//! side's authority decision — a port outside it, or a non-loopback host,
//! is refused at the broker on the record and this fiber fails its
//! activation, contained per R11), served from the kernel's READINESS
//! WAKES since pin `57360cc` (jinnd M2-K7, FINDINGS.md #23 closed):
//! `lifecycle.handle-event(handle, "jinn:net/readable", handle)` arrives
//! once per readiness transition — a pending connection on the listener,
//! bytes or EOF on a connection — so this guest holds NO alarm, an idle
//! API costs the ledger nothing, and a request is answered on its own
//! wake, not on the next poll. Every request becomes exactly ONE granted
//! contract call on a consumer of the seam — a ledgered crossing (Law 2)
//! — and the consumer's typed answer becomes the response. Bytes are
//! data plane.
//!
//! World `jinn:plugin@0.4.0`: the listener and its connections are kernel
//! registrations — released on suspend, re-listened by the next
//! `activate`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use jinn_api::{
    decode_engine_answer, decode_session_answer, decode_todo_answer, decode_workflow_answer,
    engine_list, engine_routable, engine_route, is_engines_path, is_sessions_path, is_todos_path,
    is_workflows_path, route, run_id_payload, run_payload, session_payload, session_route,
    store_list, store_routable, todo_payload, todo_route, todo_store_list, todo_store_routable,
    workflow_payload, workflow_route, workflow_store_list, workflow_store_routable, Answer,
    ApiError, EngineRoute, ErrorCode, Outcome, SessionRoute, TodoRoute, WorkflowRoute,
    ENGINE_METHODS, SESSION_METHODS, TODO_METHODS, WORKFLOW_METHODS,
};
use jinn_api_http_wire::{error_answer_response, error_response, parse, response, Parse};
use jinn_engine::{engine_contract, OP_DESCRIBE};
use jinn_session::{store_contract, OP_DESCRIBE as OP_DESCRIBE_STORE};
use jinn_todo::{store_contract as todo_store_contract, OP_DESCRIBE as OP_DESCRIBE_TODO_STORE};
use jinn_workflow::{
    store_contract as workflow_store_contract, OP_DESCRIBE as OP_DESCRIBE_WORKFLOW_STORE,
};
use serde::Deserialize;

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::{effects, net, services};

const EFFECT_TOKEN: u64 = 1;
/// Where the kernel delivers socket readiness (the token IS the handle).
const READABLE_TOPIC: &str = "jinn:net/readable";
/// One read's size; a request head or body larger than the wire caps is
/// refused typed by the codec, never buffered without bound.
const READ_CHUNK: u32 = 16 * 1024;
/// Open connections held at once; beyond it a new peer is closed
/// unanswered rather than buffered without bound (no clock: a silent
/// peer is held until its EOF, so the bound is what caps the hold).
const MAX_CONNS: usize = 64;
/// Bounded write retries when the socket accepts nothing (R1: never spin).
const WRITE_RETRIES: u32 = 1_000;

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct HttpConfig {
    port: u16,
    #[serde(default = "default_host")]
    host: String,
    /// The engines this API may route to, written by the profile from
    /// the SAME source as this entry's `jinn:engine.<id>` grants: the
    /// GRANT is the authority the kernel enforces, this list is that
    /// fact told to the provider so an unroutable id is answered without
    /// spending a kernel call.
    #[serde(default)]
    engines: Vec<String>,
    /// The session stores this API may route to, written by the profile
    /// from the SAME source as this entry's `jinn:session.<id>` grants —
    /// the same discipline as `engines`.
    #[serde(default)]
    stores: Vec<String>,
    /// The TODO stores this API may route to, written from the same
    /// source as this entry's `jinn:todo.<id>` grants. A separate list
    /// from `stores` because the two seams' ids are independent: a
    /// composition may hold a `default` Todo store and no `default`
    /// session store, and neither may stand in for the other.
    #[serde(default)]
    todo_stores: Vec<String>,
    /// The RUN stores this API may route to, written from the same source
    /// as this entry's `jinn:workflow.<id>` grants. A separate list again,
    /// for the same reason `todo_stores` is separate from `stores`: the
    /// seams' store ids are independent, and one may never stand in for
    /// another.
    #[serde(default)]
    workflow_stores: Vec<String>,
}

fn default_host() -> String {
    "127.0.0.1".into()
}

/// One accepted connection and the request bytes it has sent so far.
struct Conn {
    handle: u64,
    buffer: Vec<u8>,
}

static LISTENER: AtomicU64 = AtomicU64::new(0);
static CONNS: Mutex<Vec<Conn>> = Mutex::new(Vec::new());
static ENGINES: Mutex<Vec<String>> = Mutex::new(Vec::new());
static STORES: Mutex<Vec<String>> = Mutex::new(Vec::new());
static TODO_STORES: Mutex<Vec<String>> = Mutex::new(Vec::new());
static WORKFLOW_STORES: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn fault(context: &str, error: impl std::fmt::Debug) -> GuestFault {
    GuestFault::Failed(format!("{context}: {error:?}"))
}

/// The request body as a JSON object; anything else is typed `invalid`.
fn json_object(body: &[u8]) -> Result<serde_json::Value, ApiError> {
    match serde_json::from_slice(body) {
        Ok(serde_json::Value::Object(object)) => Ok(serde_json::Value::Object(object)),
        Ok(_) => Err(ApiError::new(
            ErrorCode::Invalid,
            "body must be a JSON object",
        )),
        Err(error) => Err(ApiError::new(ErrorCode::Invalid, format!("body: {error}"))),
    }
}

/// One answer as a response: `ok` is the value under 200, `error` the
/// envelope verbatim under its mapped status.
fn answered(answer: &Answer) -> Vec<u8> {
    match &answer.outcome {
        Outcome::Ok(value) => response(200, &serde_json::to_vec(value).expect("encodes")),
        Outcome::Error(_) => error_answer_response(answer),
    }
}

/// A route miss on a surface: 405 when the path is one this surface
/// shapes under another method, 404 otherwise. Typed either way.
fn route_miss(known_path: bool) -> Vec<u8> {
    let (status, detail) = if known_path {
        (405, "method not allowed on this path")
    } else {
        (404, "no such route")
    };
    response(
        status,
        &Answer::error(ApiError::new(ErrorCode::NotFound, detail)).encode(),
    )
}

/// One granted engine call: resolve `jinn:engine.<id>`, call it, decode
/// its answer into this seam's outcome. An engine with no provider
/// mounted is an ordinary typed answer naming it — never a fault, never
/// a 500: a composition simply may not hold that engine.
fn engine_call(
    contract: &str,
    operation: &str,
    payload: &[u8],
) -> Result<serde_json::Value, ApiError> {
    let handle = services::resolve(contract).map_err(|error| {
        ApiError::new(
            ErrorCode::Unavailable,
            format!("{contract} is not resolvable: {error:?}"),
        )
    })?;
    let bytes = services::call(handle, operation, payload).map_err(|error| {
        ApiError::new(
            ErrorCode::Refused,
            format!("{contract}/{operation} refused: {error:?}"),
        )
    })?;
    decode_engine_answer(&bytes)
}

/// The engines surface: the engines this API may route to, and one run's
/// life on one of them. Every route is exactly one granted contract call
/// per engine addressed — `list` is one per configured engine.
fn dispatch_engines(method: &str, path: &str, body: &[u8]) -> Vec<u8> {
    let Some(engines_route) = engine_route(method, path) else {
        return route_miss(
            ENGINE_METHODS
                .iter()
                .any(|candidate| engine_route(candidate, path).is_some()),
        );
    };
    let engines = ENGINES.lock().unwrap().clone();
    let outcome = match &engines_route {
        // The list is the kernel's answer per engine: each provider's
        // own `describe`, or the typed reason it could not answer.
        EngineRoute::List => Ok(
            serde_json::to_value(engine_list(engines.iter().map(|engine| {
                (
                    engine.clone(),
                    engine_call(&engine_contract(engine), OP_DESCRIBE, &[]),
                )
            })))
            .expect("encodes"),
        ),
        _ => {
            let engine = engines_route
                .engine()
                .expect("a call route names its engine");
            let operation = engines_route
                .operation()
                .expect("a call route names its operation");
            engine_routable(&engines, engine).and_then(|contract| {
                let payload = match &engines_route {
                    // `describe` takes no request.
                    EngineRoute::Describe { .. } => Vec::new(),
                    EngineRoute::Run { .. } => {
                        serde_json::to_vec(&run_payload(engine, json_object(body)?))
                            .expect("encodes")
                    }
                    EngineRoute::RunGet { run, .. } | EngineRoute::Cancel { run, .. } => {
                        serde_json::to_vec(&run_id_payload(run)).expect("encodes")
                    }
                    EngineRoute::List => unreachable!("the list is answered above"),
                };
                engine_call(&contract, operation, &payload)
            })
        }
    };
    answered(&match outcome {
        Ok(value) => Answer::ok(value),
        Err(error) => Answer::error(error),
    })
}

/// One granted store call: resolve `jinn:session.<id>`, call it, decode
/// its answer into this seam's outcome. A store with no provider mounted
/// is an ordinary typed answer naming it — never a fault, never a 500.
fn store_call(
    contract: &str,
    operation: &str,
    payload: &[u8],
) -> Result<serde_json::Value, ApiError> {
    let handle = services::resolve(contract).map_err(|error| {
        ApiError::new(
            ErrorCode::Unavailable,
            format!("{contract} is not resolvable: {error:?}"),
        )
    })?;
    let bytes = services::call(handle, operation, payload).map_err(|error| {
        ApiError::new(
            ErrorCode::Refused,
            format!("{contract}/{operation} refused: {error:?}"),
        )
    })?;
    decode_session_answer(&bytes)
}

/// The sessions surface: the stores this API may route to, and one
/// session's life in one of them. Every route is exactly one granted
/// contract call per store addressed — the store list is one per
/// configured store.
fn dispatch_sessions(
    method: &str,
    path: &str,
    query: serde_json::Value,
    body: &[u8],
) -> Vec<u8> {
    let Some(sessions_route) = session_route(method, path) else {
        return route_miss(
            SESSION_METHODS
                .iter()
                .any(|candidate| session_route(candidate, path).is_some()),
        );
    };
    let stores = STORES.lock().unwrap().clone();
    let outcome = match &sessions_route {
        // The list is the kernel's answer per store: each provider's own
        // `describe`, or the typed reason it could not answer.
        SessionRoute::Stores => Ok(serde_json::to_value(store_list(stores.iter().map(|store| {
            (
                store.clone(),
                store_call(&store_contract(store), OP_DESCRIBE_STORE, &[]),
            )
        })))
        .expect("encodes")),
        _ => {
            let store = sessions_route
                .store()
                .expect("a call route names its store");
            let operation = sessions_route
                .operation()
                .expect("a call route names its operation");
            store_routable(&stores, store).and_then(|contract| {
                // A write takes the BODY, a read takes the query: a route
                // never reads both, so a caller cannot smuggle a field
                // past the shape its method declares.
                let document = if sessions_route.takes_body() {
                    json_object(body)?
                } else {
                    query
                };
                let payload = serde_json::to_vec(&session_payload(&sessions_route, document))
                    .expect("encodes");
                store_call(&contract, operation, &payload)
            })
        }
    };
    answered(&match outcome {
        Ok(value) => Answer::ok(value),
        Err(error) => Answer::error(error),
    })
}

/// One granted Todo store call: resolve `jinn:todo.<id>`, call it,
/// decode its answer into this seam's outcome. A store with no provider
/// mounted is an ordinary typed answer naming it — never a fault.
fn todo_call(
    contract: &str,
    operation: &str,
    payload: &[u8],
) -> Result<serde_json::Value, ApiError> {
    let handle = services::resolve(contract).map_err(|error| {
        ApiError::new(
            ErrorCode::Unavailable,
            format!("{contract} is not resolvable: {error:?}"),
        )
    })?;
    let bytes = services::call(handle, operation, payload).map_err(|error| {
        ApiError::new(
            ErrorCode::Refused,
            format!("{contract}/{operation} refused: {error:?}"),
        )
    })?;
    decode_todo_answer(&bytes)
}

/// The todos surface: the stores this API may route to, and one Todo's
/// life in one of them. Every route is exactly one granted contract call
/// per store addressed.
fn dispatch_todos(method: &str, path: &str, query: serde_json::Value, body: &[u8]) -> Vec<u8> {
    let Some(todos_route) = todo_route(method, path) else {
        return route_miss(
            TODO_METHODS
                .iter()
                .any(|candidate| todo_route(candidate, path).is_some()),
        );
    };
    let stores = TODO_STORES.lock().unwrap().clone();
    let outcome = match &todos_route {
        TodoRoute::Stores => Ok(serde_json::to_value(todo_store_list(stores.iter().map(
            |store| {
                (
                    store.clone(),
                    todo_call(&todo_store_contract(store), OP_DESCRIBE_TODO_STORE, &[]),
                )
            },
        )))
        .expect("encodes")),
        _ => {
            let store = todos_route.store().expect("a call route names its store");
            let operation = todos_route
                .operation()
                .expect("a call route names its operation");
            todo_store_routable(&stores, store).and_then(|contract| {
                let document = if todos_route.takes_body() {
                    json_object(body)?
                } else {
                    query
                };
                let payload =
                    serde_json::to_vec(&todo_payload(&todos_route, document)).expect("encodes");
                todo_call(&contract, operation, &payload)
            })
        }
    };
    answered(&match outcome {
        Ok(value) => Answer::ok(value),
        Err(error) => Answer::error(error),
    })
}

/// One granted run store call: resolve `jinn:workflow.<id>`, call it,
/// decode its answer into this seam's outcome. A store with no provider
/// mounted is an ordinary typed answer naming it — never a fault.
fn workflow_call(
    contract: &str,
    operation: &str,
    payload: &[u8],
) -> Result<serde_json::Value, ApiError> {
    let handle = services::resolve(contract).map_err(|error| {
        ApiError::new(
            ErrorCode::Unavailable,
            format!("{contract} is not resolvable: {error:?}"),
        )
    })?;
    let bytes = services::call(handle, operation, payload).map_err(|error| {
        ApiError::new(
            ErrorCode::Refused,
            format!("{contract}/{operation} refused: {error:?}"),
        )
    })?;
    decode_workflow_answer(&bytes)
}

/// The workflows surface: the run stores this API may route to, the
/// procedures one of them holds, and the life of one run of one pinned
/// revision. Every route is exactly one granted contract call per store
/// addressed.
fn dispatch_workflows(method: &str, path: &str, query: serde_json::Value, body: &[u8]) -> Vec<u8> {
    let Some(workflows_route) = workflow_route(method, path) else {
        return route_miss(
            WORKFLOW_METHODS
                .iter()
                .any(|candidate| workflow_route(candidate, path).is_some()),
        );
    };
    let stores = WORKFLOW_STORES.lock().unwrap().clone();
    let outcome = match &workflows_route {
        WorkflowRoute::Stores => Ok(serde_json::to_value(workflow_store_list(stores.iter().map(
            |store| {
                (
                    store.clone(),
                    workflow_call(
                        &workflow_store_contract(store),
                        OP_DESCRIBE_WORKFLOW_STORE,
                        &[],
                    ),
                )
            },
        )))
        .expect("encodes")),
        _ => {
            let store = workflows_route
                .store()
                .expect("a call route names its store");
            let operation = workflows_route
                .operation()
                .expect("a call route names its operation");
            workflow_store_routable(&stores, store).and_then(|contract| {
                let document = if workflows_route.takes_body() {
                    json_object(body)?
                } else {
                    query
                };
                let payload = serde_json::to_vec(&workflow_payload(&workflows_route, document))
                    .expect("encodes");
                workflow_call(&contract, operation, &payload)
            })
        }
    };
    answered(&match outcome {
        Ok(value) => Answer::ok(value),
        Err(error) => Answer::error(error),
    })
}

/// One request → one contract call → one response.
fn dispatch(method: &str, path: &str, query: serde_json::Value, body: &[u8]) -> Vec<u8> {
    // The engines surface is routed first: its paths carry two
    // parameters and a per-engine contract, which the static table
    // (one parameter, one contract) cannot shape.
    if is_engines_path(path) {
        return dispatch_engines(method, path, body);
    }
    // The sessions surface has the same shape for the same reason: a
    // store id and a session id, on a per-store contract.
    if is_sessions_path(path) {
        return dispatch_sessions(method, path, query, body);
    }
    // And the todos surface, for the same reason again: a store id, a
    // Todo id, and a per-store contract.
    if is_todos_path(path) {
        return dispatch_todos(method, path, query, body);
    }
    // And the workflows surface: a store id, then either a workflow or a
    // run within it, on a per-store contract.
    if is_workflows_path(path) {
        return dispatch_workflows(method, path, query, body);
    }
    let Some((route, id)) = route(method, path) else {
        return route_miss(
            jinn_api::ROUTES
                .iter()
                .any(|candidate| candidate.path == path || route(candidate.method, path).is_some()),
        );
    };
    // The request payload: the query for reads, the JSON body for a
    // patch; the path parameter lands in the route's named field.
    let mut payload = if route.body {
        match json_object(body) {
            Ok(payload) => payload,
            Err(error) => return error_response(&error),
        }
    } else {
        query
    };
    if let Some(id) = id {
        payload[route.param] = serde_json::Value::String(id);
    }
    let handle = match services::resolve(route.contract) {
        Ok(handle) => handle,
        Err(error) => {
            return error_response(&ApiError::new(
                ErrorCode::Unavailable,
                format!("{} is not resolvable: {error:?}", route.contract),
            ))
        }
    };
    let answer = match services::call(
        handle,
        route.operation,
        &serde_json::to_vec(&payload).expect("encodes"),
    ) {
        Ok(bytes) => Answer::decode(&bytes),
        Err(error) => Answer::error(ApiError::new(
            ErrorCode::Refused,
            format!("{}/{} refused: {error:?}", route.contract, route.operation),
        )),
    };
    answered(&answer)
}

/// Writes the whole response (re-offering what the socket did not take,
/// bounded), then closes the connection.
fn finish(conn: u64, wire: &[u8]) -> Result<(), GuestFault> {
    let mut offered = 0;
    let mut retries = 0;
    while offered < wire.len() {
        let accepted =
            net::write(conn, &wire[offered..]).map_err(|error| fault("write", error))? as usize;
        if accepted == 0 {
            retries += 1;
            if retries > WRITE_RETRIES {
                break;
            }
        }
        offered += accepted;
    }
    net::close(conn).map_err(|error| fault("close", error))
}

/// The listener's wake: accept every pending connection (the accept
/// re-arms the listener's wake), then serve each new connection once —
/// its bytes may already be pending.
fn accept_pending(conns: &mut Vec<Conn>) -> Result<(), GuestFault> {
    let listener = LISTENER.load(Ordering::SeqCst);
    let mut fresh = Vec::new();
    while let net::AcceptResult::Connection(handle) =
        net::accept(listener).map_err(|error| fault("accept", error))?
    {
        if conns.len() + fresh.len() >= MAX_CONNS {
            net::close(handle).map_err(|error| fault("close", error))?;
            continue;
        }
        conns.push(Conn {
            handle,
            buffer: Vec::new(),
        });
        fresh.push(handle);
    }
    for handle in fresh {
        serve(conns, handle)?;
    }
    Ok(())
}

/// A connection's wake: read what it sent (the read re-arms its wake),
/// answer a complete request, close on EOF.
fn serve(conns: &mut Vec<Conn>, handle: u64) -> Result<(), GuestFault> {
    let Some(index) = conns.iter().position(|conn| conn.handle == handle) else {
        return Ok(());
    };
    let mut eof = false;
    loop {
        match net::read(handle, READ_CHUNK).map_err(|error| fault("read", error))? {
            net::ReadResult::Data(bytes) => conns[index].buffer.extend_from_slice(&bytes),
            net::ReadResult::Eof => {
                eof = true;
                break;
            }
            net::ReadResult::WouldBlock => break,
        }
    }
    let outcome = match parse(&conns[index].buffer) {
        Parse::Request(request) => Some(dispatch(
            &request.method,
            &request.path,
            request.query_json(),
            &request.body,
        )),
        Parse::Invalid { status, detail } => Some(response(
            status,
            &serde_json::to_vec(
                &serde_json::json!({ "error": { "code": "invalid", "detail": detail } }),
            )
            .expect("encodes"),
        )),
        Parse::Incomplete => None,
    };
    match outcome {
        Some(wire) => finish(handle, &wire)?,
        None if eof => net::close(handle).map_err(|error| fault("close", error))?,
        None => return Ok(()),
    }
    conns.remove(index);
    Ok(())
}

struct Http;

impl Guest for Http {
    fn activate(config: Vec<u8>) -> Result<(), GuestFault> {
        let config: HttpConfig = serde_json::from_slice(&config)
            .map_err(|error| GuestFault::Failed(format!("malformed config: {error}")))?;
        effects::register("jinn-api-http on duty", EFFECT_TOKEN)
            .map_err(|error| fault("effect", error))?;
        // The bind: a refusal (port outside the grant, non-loopback host,
        // no grant) is the broker's, on the record; this fiber then fails
        // its activation — contained to this entry (R11), never a crash.
        let addr = format!("{}:{}", config.host, config.port);
        let listener =
            net::listen(&addr).map_err(|error| fault(&format!("listen {addr}"), error))?;
        LISTENER.store(listener, Ordering::SeqCst);
        CONNS.lock().unwrap().clear();
        *ENGINES.lock().unwrap() = config.engines;
        *STORES.lock().unwrap() = config.stores;
        *TODO_STORES.lock().unwrap() = config.todo_stores;
        *WORKFLOW_STORES.lock().unwrap() = config.workflow_stores;
        Ok(())
    }

    fn check(_consumer: u64) -> bool {
        true
    }

    fn undo(_token: u64) -> Result<(), GuestFault> {
        Ok(())
    }

    fn handle_event(token: u64, topic: String, payload: Vec<u8>) -> Result<Vec<u8>, GuestFault> {
        // Only the kernel's typed readiness wake of OUR sockets is a
        // reason to touch them; anything else here is a contract
        // violation, refused loudly.
        let handle: Option<[u8; 8]> = payload.as_slice().try_into().ok();
        let (Some(handle), true) = (handle, topic == READABLE_TOPIC) else {
            return Err(GuestFault::Failed(format!(
                "unexpected event {topic:?} (token {token}, {} bytes)",
                payload.len()
            )));
        };
        let handle = u64::from_le_bytes(handle);
        if handle != token {
            return Err(GuestFault::Failed(format!(
                "readiness wake token {token} names another handle {handle}"
            )));
        }
        let mut conns = CONNS.lock().unwrap();
        if handle == LISTENER.load(Ordering::SeqCst) {
            accept_pending(&mut conns)?;
        } else {
            serve(&mut conns, handle)?;
        }
        Ok(Vec::new())
    }

    fn handle_call(
        _caller: u64,
        _contract: String,
        operation: String,
        _payload: Vec<u8>,
    ) -> Result<Vec<u8>, GuestFault> {
        Err(GuestFault::Failed(format!(
            "unknown operation {operation:?}"
        )))
    }

    fn snapshot() -> Vec<u8> {
        Vec::new()
    }

    fn restore(_blob: Vec<u8>) -> Result<(), GuestFault> {
        Ok(())
    }
}

export!(Http);
