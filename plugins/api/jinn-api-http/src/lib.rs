//! The `jinn-api` HTTP provider. Transport only: one `jinn:net` loopback
//! listener at the configured port (the grant's bind range is the profile
//! side's authority decision — a port outside it, or a non-loopback host,
//! is refused at the broker on the record and this fiber fails its
//! activation, contained per R11), polled from one `jinn:clock` periodic
//! alarm (the bundle's v0.1 non-blocking shape; FINDINGS.md #23 names the
//! latency it costs). Every request becomes exactly ONE granted contract
//! call on a consumer of the seam — a ledgered crossing (Law 2) — and the
//! consumer's typed answer becomes the response. Bytes are data plane.
//!
//! World `jinn:plugin@0.4.0`: the listener and its connections are kernel
//! registrations — released on suspend, re-listened by the next
//! `activate`.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use jinn_api::{route, Answer, ApiError, ErrorCode, Outcome, OP_PATCH_ENTRY};
use jinn_api_http_wire::{error_answer_response, error_response, parse, response, Parse};
use serde::Deserialize;

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::{clock, effects, net, services};

const EFFECT_TOKEN: u64 = 1;
const ALARM_TOKEN: u64 = 2;
/// Where the kernel delivers alarm wakes.
const WAKE_TOPIC: &str = "jinn:clock/alarm";
/// One read's size; a request head or body larger than the wire caps is
/// refused typed by the codec, never buffered without bound.
const READ_CHUNK: u32 = 16 * 1024;
/// Polls a silent connection survives before it is closed.
const IDLE_POLLS: u32 = 40;
/// Bounded write retries when the socket accepts nothing (R1: never spin).
const WRITE_RETRIES: u32 = 1_000;

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct HttpConfig {
    port: u16,
    #[serde(default = "default_host")]
    host: String,
    /// The accept/read poll period; the granted clock floor bounds it.
    #[serde(default = "default_poll_ms")]
    poll_ms: u64,
}

fn default_host() -> String {
    "127.0.0.1".into()
}

fn default_poll_ms() -> u64 {
    250
}

/// One accepted connection and the request bytes it has sent so far.
struct Conn {
    handle: u64,
    buffer: Vec<u8>,
    idle: u32,
}

static LISTENER: AtomicU64 = AtomicU64::new(0);
static CONNS: Mutex<Vec<Conn>> = Mutex::new(Vec::new());

fn fault(context: &str, error: impl std::fmt::Debug) -> GuestFault {
    GuestFault::Failed(format!("{context}: {error:?}"))
}

/// One request → one contract call → one response.
fn dispatch(method: &str, path: &str, query: serde_json::Value, body: &[u8]) -> Vec<u8> {
    let Some((route, id)) = route(method, path) else {
        let known_path = jinn_api::ROUTES
            .iter()
            .any(|candidate| candidate.path == path || route(candidate.method, path).is_some());
        let (status, detail) = if known_path {
            (405, "method not allowed on this path")
        } else {
            (404, "no such route")
        };
        return response(
            status,
            &Answer::error(ApiError::new(ErrorCode::NotFound, detail)).encode(),
        );
    };
    // The request payload: the query for reads, the JSON body (plus the
    // path's id) for the patch.
    let payload = if route.operation == OP_PATCH_ENTRY {
        let mut document: serde_json::Value = match serde_json::from_slice(body) {
            Ok(serde_json::Value::Object(object)) => serde_json::Value::Object(object),
            Ok(_) => {
                return error_response(&ApiError::new(
                    ErrorCode::Invalid,
                    "body must be a JSON object",
                ))
            }
            Err(error) => {
                return error_response(&ApiError::new(ErrorCode::Invalid, format!("body: {error}")))
            }
        };
        document["id"] = serde_json::Value::String(id.unwrap_or_default());
        document
    } else {
        query
    };
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
    match &answer.outcome {
        Outcome::Ok(value) => response(200, &serde_json::to_vec(value).expect("encodes")),
        Outcome::Error(_) => error_answer_response(&answer),
    }
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

/// One poll: accept what is pending, read what each connection sent,
/// answer every complete request, close what is done or dead.
fn poll() -> Result<(), GuestFault> {
    let listener = LISTENER.load(Ordering::SeqCst);
    let mut conns = CONNS.lock().unwrap();
    while let net::AcceptResult::Connection(handle) =
        net::accept(listener).map_err(|error| fault("accept", error))?
    {
        conns.push(Conn {
            handle,
            buffer: Vec::new(),
            idle: 0,
        });
    }
    let mut done = Vec::new();
    for conn in conns.iter_mut() {
        let mut eof = false;
        let mut progressed = false;
        loop {
            match net::read(conn.handle, READ_CHUNK).map_err(|error| fault("read", error))? {
                net::ReadResult::Data(bytes) => {
                    conn.buffer.extend_from_slice(&bytes);
                    progressed = true;
                }
                net::ReadResult::Eof => {
                    eof = true;
                    break;
                }
                net::ReadResult::WouldBlock => break,
            }
        }
        let outcome = match parse(&conn.buffer) {
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
        conn.idle = if progressed { 0 } else { conn.idle + 1 };
        match outcome {
            Some(wire) => {
                finish(conn.handle, &wire)?;
                done.push(conn.handle);
            }
            None if eof || conn.idle > IDLE_POLLS => {
                net::close(conn.handle).map_err(|error| fault("close", error))?;
                done.push(conn.handle);
            }
            None => {}
        }
    }
    conns.retain(|conn| !done.contains(&conn.handle));
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
        clock::alarm_every(config.poll_ms, ALARM_TOKEN).map_err(|error| fault("alarm", error))?;
        Ok(())
    }

    fn check(_consumer: u64) -> bool {
        true
    }

    fn undo(_token: u64) -> Result<(), GuestFault> {
        Ok(())
    }

    fn handle_event(token: u64, topic: String, payload: Vec<u8>) -> Result<Vec<u8>, GuestFault> {
        if topic != WAKE_TOPIC || token != ALARM_TOKEN || payload.len() != 8 {
            return Err(GuestFault::Failed(format!(
                "unexpected event {topic:?} (token {token}, {} bytes)",
                payload.len()
            )));
        }
        poll()?;
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
