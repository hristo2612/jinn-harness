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

use jinn_api::{route, Answer, ApiError, ErrorCode, Outcome};
use jinn_api_http_wire::{error_answer_response, error_response, parse, response, Parse};
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
    // The request payload: the query for reads, the JSON body for a
    // patch; the path parameter lands in the route's named field.
    let mut payload = if route.body {
        match serde_json::from_slice(body) {
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
