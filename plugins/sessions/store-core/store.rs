//! The guest-side store, shared by both providers (see this directory's
//! README for why it is source and not a crate). Everything here is a
//! host call or the sequencing around one; the semantics are the
//! definition's (`jinn_session::Sessions`, `jinn_session::journal`,
//! `jinn_session::drive`).
//!
//! # The two disciplines this file exists to hold in one place
//!
//! **A store drives an engine and never spawns one.** Every turn is one
//! call on the ENGINES seam's definition: the session's own binding
//! becomes a contract name through `jinn_engine::engine_contract`, and
//! whatever provider holds that slot answers. There is no
//! `jinn:process` grant here and no CLI knowledge — which is what makes
//! "run the same session spec over another engine" a one-field edit.
//!
//! **Nothing is emitted from inside a caller's dispatch.** The engines
//! seam publishes a run's progress on its own topic; a store that
//! LISTENED there would then emit its own events from inside the engine
//! fiber's delivery — the nested-dispatch class this repo keeps finding
//! (`FINDINGS.md` #4, and #32 at this pin). So the store POLLS `run-get`
//! on its own clock wake, and every bus record minted while a caller is
//! in this guest is held in [`DEFERRED`] until that wake. The cost is one
//! poll period of latency, which is a bound; the alternative is a
//! deadlock, which is not.

use std::collections::BTreeMap;
use std::sync::Mutex;

use jinn_session::{
    drive, Answer, CreateRequest, ErrorCode, EventKind, EventsRequest, Extensions, GetRequest,
    ListRequest, MessagesRequest, SendRequest, SessionError, SessionEvent, Sessions, TurnStatus,
    API_VERSION, EVENT_TOPIC, OP_CANCEL, OP_CLOSE, OP_CREATE, OP_EVENTS, OP_GET, OP_LIST,
    OP_MESSAGES, OP_SEND,
};
use serde::Deserialize;

use crate::jinn::plugin::{clock, events, services};
use crate::jinn::plugin::types::{DispatchMode, Selector};
use crate::journal;

/// The wake token every poll is scheduled under.
pub const ALARM_TOKEN: u64 = 2;
/// The kernel's alarm wake topic, bound from `kernel-pin/wit/plugin.wit`.
pub const WAKE_TOPIC: &str = "jinn:clock/alarm";

/// One store entry's config.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct StoreConfig {
    /// The store id served — the second half of the contract name, and
    /// the only place the id lives (the definition's rule).
    pub store: String,
    /// Where a durable store's journals go, relative to the `jinn:fs`
    /// grant's scope. Read by `jinn-session-fs`'s journal and by nothing
    /// in the ephemeral store, which is why it is dead code in exactly
    /// one of the two crates that include this file.
    #[serde(default)]
    #[allow(dead_code)]
    pub dir: Option<String>,
    /// How often a live turn's engine run is polled.
    #[serde(default = "default_poll_ms")]
    pub poll_ms: u64,
}

fn default_poll_ms() -> u64 {
    250
}

/// One turn being driven right now. Per incarnation: a run id belongs to
/// the engine incarnation that minted it, so nothing here is restored.
#[derive(Clone, Debug)]
pub struct Drive {
    pub turn_id: String,
    pub contract: String,
    pub run_id: String,
    /// How much of the answer has already been reported as a delta, so a
    /// poll emits the NEW text and never re-emits what a reader has.
    pub delivered: usize,
}

pub static CONFIG: Mutex<Option<StoreConfig>> = Mutex::new(None);
pub static SESSIONS: Mutex<Option<Sessions>> = Mutex::new(None);
pub static DRIVING: Mutex<BTreeMap<String, Drive>> = Mutex::new(BTreeMap::new());
/// Bus records minted inside a call, held until this provider's own fiber
/// wakes (see the module doc).
pub static DEFERRED: Mutex<Vec<SessionEvent>> = Mutex::new(Vec::new());
/// Refused emits since activation. Never fatal — the feed and the record
/// still hold the event — and never silent: `describe` reports it.
pub static EMIT_FAILURES: Mutex<u64> = Mutex::new(0);
/// Journals whose torn TAIL was dropped on adoption. Bytes that were
/// never a record, discarded out loud rather than in silence — a durable
/// store's `describe` reports it, and an ephemeral store's is always 0.
pub static HEALED_TAILS: Mutex<u64> = Mutex::new(0);
/// How many documents this store read and found NO complete record in.
///
/// A daemon killed inside a document's very first append leaves bytes
/// that were never a record. Those documents are absence: nothing is
/// adopted from them and nothing is written into them. They are counted
/// so a store that declined to make a record out of a document SAYS SO,
/// and a reader gets evidence of the absence instead of the absence of
/// evidence (`FINDINGS.md` #36).
pub static RECORD_LESS_DOCUMENTS: Mutex<u64> = Mutex::new(0);

/// The config this incarnation activated with.
///
/// # Panics
///
/// Called before `activate`.
pub fn config() -> StoreConfig {
    CONFIG
        .lock()
        .unwrap()
        .clone()
        .expect("activate holds the config")
}

fn with_sessions<T>(act: impl FnOnce(&mut Sessions) -> T) -> T {
    let mut held = SESSIONS.lock().unwrap();
    act(held.as_mut().expect("activate holds the registry"))
}

/// Puts one bus record on the wire. A refusal is counted, never fatal.
fn emit(record: &SessionEvent) {
    let payload = serde_json::to_vec(record).expect("a session event encodes");
    if events::emit(EVENT_TOPIC, DispatchMode::Emit, &Selector::All, &payload).is_err() {
        *EMIT_FAILURES.lock().unwrap() += 1;
    }
}

/// Puts everything held for this fiber on the wire, in sequence order.
pub fn flush_deferred() {
    let records = std::mem::take(&mut *DEFERRED.lock().unwrap());
    for record in &records {
        emit(record);
    }
}

/// Records one event against a session and HOLDS it for the next wake.
fn record_event(session_id: &str, kind: EventKind) {
    let record = with_sessions(|sessions| sessions.record_event(session_id, kind));
    DEFERRED.lock().unwrap().push(record);
}

/// This incarnation's `now`.
///
/// # Errors
///
/// The clock refused.
pub fn now_ms() -> Result<u64, SessionError> {
    clock::now()
        .map_err(|error| SessionError::new(ErrorCode::Failed, format!("clock now: {error:?}")))
}

/// Schedules this store's next poll.
fn wake_at(at_ms: u64) -> Result<(), SessionError> {
    clock::alarm_at(at_ms, ALARM_TOKEN).map_err(|error| {
        SessionError::new(
            ErrorCode::Failed,
            format!("this store could not schedule its own poll: {error:?}"),
        )
    })?;
    Ok(())
}

/// The sessions error class an engines error maps onto. `unavailable`
/// stays `unavailable` — the store is fine and this host cannot carry the
/// run — so it never reads as a refusal of the session.
fn engine_code(code: jinn_engine::ErrorCode) -> ErrorCode {
    match code {
        jinn_engine::ErrorCode::Invalid => ErrorCode::Invalid,
        jinn_engine::ErrorCode::NotFound => ErrorCode::NotFound,
        jinn_engine::ErrorCode::Refused => ErrorCode::Refused,
        jinn_engine::ErrorCode::Unavailable => ErrorCode::Unavailable,
        jinn_engine::ErrorCode::Failed => ErrorCode::Failed,
    }
}

/// One call on the engines seam's DEFINITION. A composition that holds no
/// such engine is an ordinary typed answer naming it — never a fault.
///
/// # Errors
///
/// The contract is unresolvable, the call was refused, or the engine
/// answered a typed error (which rides along as `engine-code`).
pub fn engine_call(
    contract: &str,
    operation: &str,
    payload: &[u8],
) -> Result<serde_json::Value, SessionError> {
    let handle = services::resolve(contract).map_err(|error| {
        SessionError::new(
            ErrorCode::Unavailable,
            format!("{contract} is not resolvable: {error:?}"),
        )
    })?;
    let bytes = services::call(handle, operation, payload).map_err(|error| {
        SessionError::new(
            ErrorCode::Refused,
            format!("{contract}/{operation} refused: {error:?}"),
        )
    })?;
    let answer: jinn_engine::Answer = serde_json::from_slice(&bytes).map_err(|error| {
        SessionError::new(
            ErrorCode::Failed,
            format!("malformed engine answer: {error}"),
        )
    })?;
    answer.into_result().map_err(|error| {
        let mut mapped = SessionError::new(engine_code(error.code), error.message);
        if let Ok(code) = serde_json::to_value(error.code) {
            mapped.extra.insert("engine-code".to_owned(), code);
        }
        mapped
    })
}

/// `create`: the session, its journal's first line, and its `created`
/// event. The journal is written BEFORE the session is answered for, so a
/// session a caller holds an id for is a session that survives a crash.
fn on_create(payload: &[u8]) -> Result<serde_json::Value, SessionError> {
    let request: CreateRequest = decode(payload, "create")?;
    let now = now_ms()?;
    let spec = request.spec;
    let created = with_sessions(|sessions| sessions.create(spec.clone(), now));
    if let Err(error) = journal::created(&created.session_id, &spec, now) {
        // The durable record did not land, so the session does not
        // exist. Nothing half-created is left behind for a later replay
        // to disagree with.
        with_sessions(|sessions| sessions.forget(&created.session_id));
        return Err(error);
    }
    record_event(
        &created.session_id,
        EventKind::Created {
            engine: created.engine.clone(),
        },
    );
    // The `created` event is on this fiber's deferred list; a wake has to
    // come for it to reach the bus.
    wake_at(now)?;
    Ok(serde_json::to_value(created).expect("encodes"))
}

/// `send`: accept the turn, RECORD IT STARTED, then drive the engine.
///
/// The order is the whole of restart honesty. The `turn-started` line is
/// on disk before an engine has been asked for anything, so a daemon that
/// dies at ANY point after this comes back with a started turn and no
/// ending — which the journal's replay reads as `interrupted` with a
/// reason. Driving first and recording after would leave a window where a
/// crash loses the turn entirely, and an absent turn is a worse lie than
/// an interrupted one: it says nothing happened.
fn on_send(payload: &[u8]) -> Result<serde_json::Value, SessionError> {
    let request: SendRequest = decode(payload, "send")?;
    let now = now_ms()?;
    let session_id = request.session_id.clone();
    let spec = with_sessions(|sessions| sessions.spec(&session_id).cloned()).ok_or_else(|| {
        SessionError::new(ErrorCode::NotFound, format!("{session_id:?} is not here"))
    })?;
    let accepted = with_sessions(|sessions| sessions.send(&session_id, &request.message, now))?;
    if let Err(error) = journal::turn_started(&session_id, &accepted.turn_id, &request.message, now)
    {
        // The turn was never durably started, so it is ended here rather
        // than left in flight with nothing driving it.
        end_turn(
            &session_id,
            &accepted.turn_id,
            TurnStatus::Failed,
            Some(error.message.clone()),
            now,
            String::new(),
        )?;
        return Err(error);
    }
    record_event(
        &session_id,
        EventKind::TurnStarted {
            turn_id: accepted.turn_id.clone(),
            message: request.message.clone(),
        },
    );
    let contract = jinn_engine::engine_contract(&spec.engine.engine);
    let run = engine_call(
        &contract,
        jinn_engine::OP_RUN,
        &serde_json::to_vec(&drive::run_request(&spec, &request.message)).expect("encodes"),
    );
    let run_id = match run.and_then(|accepted| {
        accepted
            .get("run-id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                SessionError::new(
                    ErrorCode::Failed,
                    "the engine accepted a run and named no run-id",
                )
            })
    }) {
        Ok(run_id) => run_id,
        Err(error) => {
            // The engine refused. The turn ends FAILED with the engine's
            // own reason — never left running, and never silently
            // dropped: the caller reads the refusal, and so does the log.
            end_turn(
                &session_id,
                &accepted.turn_id,
                TurnStatus::Failed,
                Some(error.message.clone()),
                now,
                String::new(),
            )?;
            return Err(error);
        }
    };
    DRIVING.lock().unwrap().insert(
        session_id.clone(),
        Drive {
            turn_id: accepted.turn_id.clone(),
            contract,
            run_id,
            delivered: 0,
        },
    );
    wake_at(now.saturating_add(config().poll_ms))?;
    Ok(serde_json::to_value(accepted).expect("encodes"))
}

/// Ends one turn: the registry, the journal, and the event — in that
/// order, so nothing is announced that is not recorded.
fn end_turn(
    session_id: &str,
    turn_id: &str,
    status: TurnStatus,
    reason: Option<String>,
    now: u64,
    answer: String,
) -> Result<(), SessionError> {
    with_sessions(|sessions| {
        if let Some(turn) = sessions.turn_mut(session_id, turn_id) {
            turn.answer = answer;
        }
    });
    let turn = with_sessions(|sessions| {
        sessions.end_turn(session_id, turn_id, status, reason.clone(), now)
    })?;
    journal::turn_ended(session_id, &turn, now)?;
    record_event(
        session_id,
        match (status, reason) {
            (TurnStatus::Done, _) => EventKind::TurnEnded {
                turn_id: turn_id.to_owned(),
                usage: turn.usage.clone(),
            },
            (_, reason) => EventKind::TurnFailed {
                turn_id: turn_id.to_owned(),
                // A non-`done` ending carries a reason by the registry's
                // own rule, so this fallback is unreachable; it is here
                // so the type cannot be satisfied by an empty string.
                reason: reason.unwrap_or_else(|| "the turn ended without a recorded reason".into()),
            },
        },
    );
    Ok(())
}

/// One poll of every turn in flight: ask the engine, report new text,
/// end what has ended. Answers whether anything is still being driven.
///
/// # Errors
///
/// Only what the store cannot recover from; an engine that refuses one
/// poll ENDS that turn (failed, with the refusal as the reason) rather
/// than leaving it in flight forever.
pub fn poll_once(now: u64) -> Result<bool, SessionError> {
    let driving: Vec<(String, Drive)> = DRIVING
        .lock()
        .unwrap()
        .iter()
        .map(|(session, drive)| (session.clone(), drive.clone()))
        .collect();
    for (session_id, drive) in driving {
        let record = engine_call(
            &drive.contract,
            jinn_engine::OP_RUN_GET,
            &serde_json::to_vec(&serde_json::json!({ "run-id": drive.run_id })).expect("encodes"),
        );
        let record: jinn_engine::RunRecord = match record
            .and_then(|value| {
                serde_json::from_value(value).map_err(|error| {
                    SessionError::new(
                        ErrorCode::Failed,
                        format!("malformed engine run record: {error}"),
                    )
                })
            }) {
            Ok(record) => record,
            Err(error) => {
                // The run is unreadable — the engine restarted, the
                // record aged out, the provider is gone. That is not a
                // reason to keep claiming the turn is running.
                DRIVING.lock().unwrap().remove(&session_id);
                end_turn(
                    &session_id,
                    &drive.turn_id,
                    TurnStatus::Failed,
                    Some(error.message),
                    now,
                    String::new(),
                )?;
                continue;
            }
        };
        // Only the NEW text is a delta; a reader never sees a chunk twice.
        if record.text.len() > drive.delivered {
            let text = record.text[drive.delivered..].to_owned();
            record_event(
                &session_id,
                EventKind::Delta {
                    turn_id: drive.turn_id.clone(),
                    text,
                },
            );
            if let Some(held) = DRIVING.lock().unwrap().get_mut(&session_id) {
                held.delivered = record.text.len();
            }
        }
        let Some((status, reason)) = drive::ended(&record) else {
            continue;
        };
        with_sessions(|sessions| {
            if let Some(turn) = sessions.turn_mut(&session_id, &drive.turn_id) {
                turn.run_id = Some(drive.run_id.clone());
                turn.usage = record.usage.clone();
            }
        });
        DRIVING.lock().unwrap().remove(&session_id);
        end_turn(
            &session_id,
            &drive.turn_id,
            status,
            reason,
            now,
            record.text.clone(),
        )?;
    }
    Ok(!DRIVING.lock().unwrap().is_empty())
}

/// `cancel`: the engine's run is asked to stop and the turn ends
/// `cancelled` REGARDLESS. A cancel whose engine call failed still
/// cancelled the turn — the alternative is a turn that reads as running
/// with nothing driving it.
fn on_cancel(payload: &[u8]) -> Result<serde_json::Value, SessionError> {
    let request: GetRequest = decode(payload, "cancel")?;
    let now = now_ms()?;
    let session_id = request.session_id.clone();
    let in_flight = with_sessions(|sessions| {
        sessions
            .in_flight(&session_id)
            .map(|turn| turn.turn_id.clone())
    });
    let Some(turn_id) = in_flight else {
        // Nothing in flight: the record as it stands, unchanged. A
        // terminal turn is never re-labelled.
        return record_of(&session_id);
    };
    let drive = DRIVING.lock().unwrap().remove(&session_id);
    let mut note = "cancelled by a caller".to_owned();
    if let Some(drive) = drive {
        if let Err(error) = engine_call(
            &drive.contract,
            jinn_engine::OP_CANCEL,
            &serde_json::to_vec(&serde_json::json!({ "run-id": drive.run_id })).expect("encodes"),
        ) {
            // Said, not swallowed: the turn is cancelled and the reason
            // records that the engine did not confirm it.
            note = format!("cancelled by a caller; the engine did not confirm: {}", error.message);
        }
    }
    end_turn(
        &session_id,
        &turn_id,
        TurnStatus::Cancelled,
        Some(note),
        now,
        String::new(),
    )?;
    wake_at(now)?;
    record_of(&session_id)
}

/// `close`: the session is closed for good and says so.
fn on_close(payload: &[u8]) -> Result<serde_json::Value, SessionError> {
    let request: GetRequest = decode(payload, "close")?;
    let now = now_ms()?;
    let session_id = request.session_id.clone();
    with_sessions(|sessions| sessions.close(&session_id))?;
    journal::closed(&session_id, now)?;
    DRIVING.lock().unwrap().remove(&session_id);
    record_event(&session_id, EventKind::Closed);
    wake_at(now)?;
    record_of(&session_id)
}

fn record_of(session_id: &str) -> Result<serde_json::Value, SessionError> {
    with_sessions(|sessions| sessions.record(session_id))
        .map(|record| serde_json::to_value(record).expect("encodes"))
        .ok_or_else(|| {
            SessionError::new(ErrorCode::NotFound, format!("{session_id:?} is not here"))
        })
}

fn decode<T: serde::de::DeserializeOwned>(
    payload: &[u8],
    what: &str,
) -> Result<T, SessionError> {
    serde_json::from_slice(payload).map_err(|error| {
        SessionError::new(ErrorCode::Invalid, format!("malformed {what} request: {error}"))
    })
}

/// Every operation but `describe`, which each provider answers with its
/// own declaration.
pub fn dispatch(operation: &str, payload: &[u8]) -> Answer {
    let outcome = match operation {
        OP_CREATE => on_create(payload),
        OP_SEND => on_send(payload),
        OP_GET => decode::<GetRequest>(payload, "get")
            .and_then(|request| record_of(&request.session_id)),
        OP_MESSAGES => decode::<MessagesRequest>(payload, "messages").and_then(|request| {
            with_sessions(|sessions| {
                sessions.page(&request.session_id, request.offset, request.limit)
            })
            .map(|page| serde_json::to_value(page).expect("encodes"))
            .ok_or_else(|| {
                SessionError::new(
                    ErrorCode::NotFound,
                    format!("{:?} is not here", request.session_id),
                )
            })
        }),
        OP_EVENTS => decode::<EventsRequest>(payload, "events").and_then(|request| {
            with_sessions(|sessions| {
                sessions.events_since(&request.session_id, request.after, request.limit)
            })
            .map(|page| serde_json::to_value(page).expect("encodes"))
            .ok_or_else(|| {
                SessionError::new(
                    ErrorCode::NotFound,
                    format!("{:?} is not here", request.session_id),
                )
            })
        }),
        OP_LIST => decode::<ListRequest>(payload, "list").map(|request| {
            serde_json::to_value(with_sessions(|sessions| sessions.list(&request)))
                .expect("encodes")
        }),
        OP_CANCEL => on_cancel(payload),
        OP_CLOSE => on_close(payload),
        other => Err(SessionError::new(
            ErrorCode::Invalid,
            format!("unknown operation {other:?}"),
        )),
    };
    match outcome {
        Ok(value) => Answer::ok(value),
        Err(error) => Answer::error(error),
    }
}

/// The store's own `describe`: what it is, and what it PROMISES about
/// where its records live.
pub fn describe(provider: &str, durable: bool) -> Answer {
    let config = config();
    let mut extra = Extensions::new();
    extra.insert(
        "emit-failures".to_owned(),
        serde_json::json!(*EMIT_FAILURES.lock().unwrap()),
    );
    extra.insert("poll-ms".to_owned(), serde_json::json!(config.poll_ms));
    extra.insert(
        "healed-tails".to_owned(),
        serde_json::json!(*HEALED_TAILS.lock().unwrap()),
    );
    extra.insert(
        "documents-without-a-record".to_owned(),
        serde_json::json!(*RECORD_LESS_DOCUMENTS.lock().unwrap()),
    );
    extra.insert(
        "driving".to_owned(),
        serde_json::json!(DRIVING.lock().unwrap().len()),
    );
    Answer::ok(serde_json::json!({
        "api-version": API_VERSION,
        "store": config.store,
        "provider": provider,
        // The store's own word, so a consumer gates on a declaration
        // rather than inferring durability from a package name.
        "durable": durable,
        "sessions": with_sessions(|sessions| sessions.ids().count()),
        "extra": extra,
    }))
}

/// The shared half of `activate`: the config, the empty registry, and the
/// provision. A durable store adopts its journals through
/// `journal::adopt_all` before this answers.
///
/// # Errors
///
/// A malformed config, an empty store id, or a journal this store could
/// not read.
pub fn activate(config_bytes: &[u8]) -> Result<StoreConfig, String> {
    let config: StoreConfig =
        serde_json::from_slice(config_bytes).map_err(|error| format!("malformed config: {error}"))?;
    if config.store.is_empty() {
        return Err("config.store is the store id this provider serves; it cannot be empty".into());
    }
    *SESSIONS.lock().unwrap() = Some(Sessions::new(config.store.clone()));
    // A run id belongs to the engine incarnation that minted it, so a
    // fresh incarnation drives nothing and adopts no run.
    *DRIVING.lock().unwrap() = BTreeMap::new();
    *DEFERRED.lock().unwrap() = Vec::new();
    *EMIT_FAILURES.lock().unwrap() = 0;
    *CONFIG.lock().unwrap() = Some(config.clone());
    journal::adopt_all(&config).map_err(|error| error.message)?;
    Ok(config)
}

/// One wake: everything held goes on the wire, every live run is polled,
/// and the poll re-arms while anything is still being driven.
///
/// # Errors
///
/// The store could not re-arm its own poll.
pub fn on_wake(now: u64) -> Result<bool, String> {
    flush_deferred();
    let driving = poll_once(now).map_err(|error| error.message)?;
    flush_deferred();
    if driving {
        wake_at(now.saturating_add(config().poll_ms)).map_err(|error| error.message)?;
    }
    Ok(driving)
}
