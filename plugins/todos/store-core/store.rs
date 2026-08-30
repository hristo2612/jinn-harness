//! The guest-side store, shared by both providers (see this directory's
//! README for why it is source and not a crate). Everything here is a
//! host call or the sequencing around one; the semantics are the
//! definition's (`jinn_todo::Todos`, `jinn_todo::journal`,
//! `jinn_todo::dispatch`).
//!
//! # The three disciplines this file exists to hold in one place
//!
//! **A store drives a SESSION and never opens one itself.** Every
//! dispatch is a call on the SESSIONS seam's definition: the dispatch
//! spec's own store id becomes a contract name through
//! `jinn_todo::dispatch::session_contract`, and whatever provider holds
//! that slot answers. There is no engine knowledge here at all — the
//! session resolves that — which is what makes the three-layer stack
//! compose.
//!
//! **A refusal is recorded before it is answered.** An illegal status
//! move goes to the journal and the bus as a `transition-refused`, and
//! only then comes back to the caller as a typed error. The registry
//! makes that hard to get wrong (`Moved::Refused` carries the record),
//! and this file is where the durable half lands.
//!
//! **Nothing is emitted from inside a caller's dispatch.** The sessions
//! seam publishes a turn's progress on its own topic; a store that
//! LISTENED there would emit its own events from inside that fiber's
//! delivery — the nested-dispatch class this repo keeps finding
//! (`FINDINGS.md` #4, and #32 at this pin). So the store POLLS the
//! session's `get` on its own clock wake, and every bus record minted
//! while a caller is in this guest is held in [`DEFERRED`] until that
//! wake.

use std::collections::BTreeMap;
use std::sync::Mutex;

use jinn_todo::{
    dispatch as translate, Answer, CommentRequest, CreateRequest, DispatchRequest, DispatchStatus,
    ErrorCode, EventKind, EventsRequest, Extensions, GetRequest, ListRequest, Moved, StatusChange,
    TodoError, TodoEvent, TodoRecord, Todos, TreeRequest, UpdateRequest, API_VERSION, EVENT_TOPIC,
    OP_COMMENT, OP_CREATE, OP_DISPATCH, OP_EVENTS, OP_GET, OP_LIST, OP_TREE, OP_UPDATE,
};
use serde::Deserialize;

use crate::jinn::plugin::types::{DispatchMode, Selector};
use crate::jinn::plugin::{clock, events, services};
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
    /// grant's scope. Read by `jinn-todo-fs`'s journal and by nothing in
    /// the ephemeral store, which is why it is dead code in exactly one
    /// of the two crates that include this file.
    #[serde(default)]
    #[allow(dead_code)]
    pub dir: Option<String>,
    /// How often a live dispatch's session is polled.
    #[serde(default = "default_poll_ms")]
    pub poll_ms: u64,
}

fn default_poll_ms() -> u64 {
    250
}

/// One dispatch being driven right now. Per incarnation: a session's turn
/// belongs to the incarnation that started it, so nothing here is
/// restored — which is exactly why a replayed dispatch is `interrupted`.
#[derive(Clone, Debug)]
pub struct Drive {
    pub dispatch_id: String,
    pub contract: String,
    pub session_id: String,
    pub turn_id: String,
}

pub static CONFIG: Mutex<Option<StoreConfig>> = Mutex::new(None);
pub static TODOS: Mutex<Option<Todos>> = Mutex::new(None);
pub static DRIVING: Mutex<BTreeMap<String, Drive>> = Mutex::new(BTreeMap::new());
/// Bus records minted inside a call, held until this provider's own fiber
/// wakes (see the module doc).
pub static DEFERRED: Mutex<Vec<TodoEvent>> = Mutex::new(Vec::new());
/// Refused emits since activation. Never fatal — the feed and the record
/// still hold the event — and never silent: `describe` reports it.
pub static EMIT_FAILURES: Mutex<u64> = Mutex::new(0);

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

fn with_todos<T>(act: impl FnOnce(&mut Todos) -> T) -> T {
    let mut held = TODOS.lock().unwrap();
    act(held.as_mut().expect("activate holds the registry"))
}

/// Puts one bus record on the wire. A refusal is counted, never fatal.
fn emit(record: &TodoEvent) {
    let payload = serde_json::to_vec(record).expect("a Todo event encodes");
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

/// Records one event against a Todo and HOLDS it for the next wake.
fn record_event(todo_id: &str, kind: EventKind) {
    let record = with_todos(|todos| todos.record_event(todo_id, kind));
    DEFERRED.lock().unwrap().push(record);
}

/// This incarnation's `now`.
///
/// # Errors
///
/// The clock refused.
pub fn now_ms() -> Result<u64, TodoError> {
    clock::now().map_err(|error| TodoError::new(ErrorCode::Failed, format!("clock now: {error:?}")))
}

/// Schedules this store's next poll.
fn wake_at(at_ms: u64) -> Result<(), TodoError> {
    clock::alarm_at(at_ms, ALARM_TOKEN).map_err(|error| {
        TodoError::new(
            ErrorCode::Failed,
            format!("this store could not schedule its own poll: {error:?}"),
        )
    })?;
    Ok(())
}

/// The todos error class a sessions error maps onto. `unavailable` stays
/// `unavailable` — the store is fine and this host cannot carry the
/// dispatch — so it never reads as a refusal of the Todo.
fn session_code(code: jinn_session::ErrorCode) -> ErrorCode {
    match code {
        jinn_session::ErrorCode::Invalid => ErrorCode::Invalid,
        jinn_session::ErrorCode::NotFound => ErrorCode::NotFound,
        jinn_session::ErrorCode::Refused => ErrorCode::Refused,
        jinn_session::ErrorCode::Unavailable => ErrorCode::Unavailable,
        jinn_session::ErrorCode::Failed => ErrorCode::Failed,
    }
}

/// One call on the sessions seam's DEFINITION. A composition that holds
/// no such session store is an ordinary typed answer naming it — never a
/// fault.
///
/// # Errors
///
/// The contract is unresolvable, the call was refused, or the session
/// store answered a typed error (which rides along as `session-code`).
pub fn session_call(
    contract: &str,
    operation: &str,
    payload: &[u8],
) -> Result<serde_json::Value, TodoError> {
    let handle = services::resolve(contract).map_err(|error| {
        TodoError::new(
            ErrorCode::Unavailable,
            format!("{contract} is not resolvable: {error:?}"),
        )
    })?;
    let bytes = services::call(handle, operation, payload).map_err(|error| {
        TodoError::new(
            ErrorCode::Refused,
            format!("{contract}/{operation} refused: {error:?}"),
        )
    })?;
    let answer: jinn_session::Answer = serde_json::from_slice(&bytes).map_err(|error| {
        TodoError::new(
            ErrorCode::Failed,
            format!("malformed session answer: {error}"),
        )
    })?;
    answer.into_result().map_err(|error| {
        let mut mapped = TodoError::new(session_code(error.code), error.message);
        if let Ok(code) = serde_json::to_value(error.code) {
            mapped.extra.insert("session-code".to_owned(), code);
        }
        mapped
    })
}

/// `create`: the Todo, its journal's first line, and its `created` event.
/// The journal is written BEFORE the Todo is answered for, so a Todo a
/// caller holds an id for is a Todo that survives a crash.
fn on_create(payload: &[u8]) -> Result<serde_json::Value, TodoError> {
    let request: CreateRequest = decode(payload, "create")?;
    let now = now_ms()?;
    let spec = request.spec;
    let created = with_todos(|todos| todos.create(spec.clone(), now))?;
    if let Err(error) = journal::created(&created.todo_id, &spec, now) {
        // The durable record did not land, so the Todo does not exist.
        // Nothing half-created is left for a later replay to disagree
        // with.
        with_todos(|todos| todos.forget(&created.todo_id));
        return Err(error);
    }
    record_event(
        &created.todo_id,
        EventKind::Created {
            title: spec.title.clone(),
        },
    );
    // The `created` event is on this fiber's deferred list; a wake has to
    // come for it to reach the bus.
    wake_at(now)?;
    Ok(serde_json::to_value(created).expect("encodes"))
}

/// Writes one status move to the journal and puts it on the bus. The
/// `closed` that a terminal move implies rides along from the
/// definition's own `move_events`, so a provider cannot emit one without
/// the other.
fn land_change(todo_id: &str, change: &StatusChange, now: u64) -> Result<(), TodoError> {
    journal::status_changed(todo_id, change, now)?;
    for kind in Todos::move_events(change) {
        record_event(todo_id, kind);
    }
    Ok(())
}

/// `update`: one status move through the table.
///
/// A REFUSAL is recorded and only then answered — the journal line and
/// the bus event land before the caller hears `refused`, so an operator
/// reading the ledger sees the attempt even if the caller drops the
/// answer on the floor.
fn on_update(payload: &[u8]) -> Result<serde_json::Value, TodoError> {
    let request: UpdateRequest = decode(payload, "update")?;
    let actor = request.attribution.check()?;
    let now = now_ms()?;
    let todo_id = request.todo_id.clone();
    let moved = with_todos(|todos| {
        todos.update(&todo_id, request.status, actor, request.note.clone(), now)
    })?;
    match moved {
        Moved::Changed(change) => {
            land_change(&todo_id, &change, now)?;
            wake_at(now)?;
            record_of(&todo_id)
        }
        Moved::Refused(refused, error) => {
            journal::transition_refused(&todo_id, &refused, now)?;
            record_event(
                &todo_id,
                EventKind::TransitionRefused {
                    from: refused.from,
                    to: refused.to,
                    actor: refused.actor.clone(),
                },
            );
            wake_at(now)?;
            Err(error)
        }
    }
}

/// `comment`.
fn on_comment(payload: &[u8]) -> Result<serde_json::Value, TodoError> {
    let request: CommentRequest = decode(payload, "comment")?;
    let actor = request.attribution.check()?;
    let now = now_ms()?;
    let todo_id = request.todo_id.clone();
    let comment = with_todos(|todos| todos.comment(&todo_id, &request.body, actor, now))?;
    journal::commented(&todo_id, &comment, now)?;
    record_event(
        &todo_id,
        EventKind::Commented {
            comment_id: comment.comment_id.clone(),
            actor: comment.actor.clone(),
        },
    );
    wake_at(now)?;
    record_of(&todo_id)
}

/// `dispatch`: the three-layer composition, in the order restart honesty
/// requires.
///
/// The `dispatch-started` line is on disk before a session has been asked
/// for anything, so a daemon that dies at ANY point after this comes back
/// with a started dispatch and no ending — which the journal's replay
/// reads as `interrupted` with a reason, and the fold reports as
/// `blocked`. Opening the session first and recording after would leave a
/// window where a crash loses the dispatch entirely, and an absent
/// dispatch is a worse lie than an interrupted one: it says nothing
/// happened.
fn on_dispatch(payload: &[u8]) -> Result<serde_json::Value, TodoError> {
    let request: DispatchRequest = decode(payload, "dispatch")?;
    let actor = request.attribution.check()?;
    let now = now_ms()?;
    let todo_id = request.todo_id.clone();
    let spec = request.dispatch;
    let (dispatch, change) =
        with_todos(|todos| todos.begin_dispatch(&todo_id, &spec, actor, now))?;
    if let Some(change) = &change {
        land_change(&todo_id, change, now)?;
    }
    if let Err(error) = journal::dispatch_started(&todo_id, &dispatch, now) {
        // The dispatch was never durably started, so it is ended here
        // rather than left in flight with nothing driving it.
        end_dispatch(
            &todo_id,
            &dispatch.dispatch_id,
            DispatchStatus::Failed,
            Some(error.message.clone()),
            String::new(),
            now,
        )?;
        return Err(error);
    }
    record_event(
        &todo_id,
        EventKind::Dispatched {
            dispatch_id: dispatch.dispatch_id.clone(),
            session_store: dispatch.session_store.clone(),
            engine: dispatch.engine.clone(),
        },
    );
    let contract = translate::session_contract(&spec);
    let todo = record_of(&todo_id)?;
    let record: TodoRecord = serde_json::from_value(todo).expect("this seam's own record decodes");
    match open_and_send(&contract, &spec, &todo_id, &record) {
        Ok((session_id, turn_id)) => {
            with_todos(|todos| {
                if let Some(dispatch) = todos.dispatch_mut(&todo_id, &dispatch.dispatch_id) {
                    dispatch.session_id = Some(session_id.clone());
                    dispatch.turn_id = Some(turn_id.clone());
                }
            });
            DRIVING.lock().unwrap().insert(
                todo_id.clone(),
                Drive {
                    dispatch_id: dispatch.dispatch_id.clone(),
                    contract,
                    session_id,
                    turn_id,
                },
            );
            wake_at(now.saturating_add(config().poll_ms))?;
            record_of(&todo_id)
        }
        Err(error) => {
            // The session refused. The dispatch ends FAILED with the
            // session's own reason — never left running, and never
            // silently dropped: the caller reads the refusal, and so
            // does the ledger.
            end_dispatch(
                &todo_id,
                &dispatch.dispatch_id,
                DispatchStatus::Failed,
                Some(error.message.clone()),
                String::new(),
                now,
            )?;
            wake_at(now)?;
            Err(error)
        }
    }
}

/// Opens the session and sends the brief. Two calls on the sessions
/// seam's definition and nothing else.
fn open_and_send(
    contract: &str,
    spec: &jinn_todo::DispatchSpec,
    todo_id: &str,
    todo: &TodoRecord,
) -> Result<(String, String), TodoError> {
    let created = session_call(
        contract,
        jinn_session::OP_CREATE,
        &serde_json::to_vec(&translate::create_request(spec, todo_id)).expect("encodes"),
    )?;
    let session_id = created
        .get("session-id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            TodoError::new(
                ErrorCode::Failed,
                "the session store opened a session and named no session-id",
            )
        })?
        .to_owned();
    let accepted = session_call(
        contract,
        jinn_session::OP_SEND,
        &serde_json::to_vec(&translate::send_request(spec, &session_id, todo)).expect("encodes"),
    )?;
    let turn_id = accepted
        .get("turn-id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            TodoError::new(
                ErrorCode::Failed,
                "the session store accepted a turn and named no turn-id",
            )
        })?
        .to_owned();
    Ok((session_id, turn_id))
}

/// Ends one dispatch: the registry, the journal, and the event — in that
/// order, so nothing is announced that is not recorded.
fn end_dispatch(
    todo_id: &str,
    dispatch_id: &str,
    status: DispatchStatus,
    reason: Option<String>,
    answer: String,
    now: u64,
) -> Result<(), TodoError> {
    let dispatch = with_todos(|todos| {
        todos.end_dispatch(todo_id, dispatch_id, status, reason.clone(), answer, now)
    })?;
    journal::dispatch_ended(todo_id, &dispatch, now)?;
    record_event(
        todo_id,
        EventKind::DispatchEnded {
            dispatch_id: dispatch_id.to_owned(),
            status,
            reason,
        },
    );
    Ok(())
}

/// One poll of every dispatch in flight: read the session, end what has
/// ended. Answers whether anything is still being driven.
///
/// # Errors
///
/// Only what the store cannot recover from; a session that cannot be read
/// ENDS that dispatch (failed, with the reason) rather than leaving it in
/// flight forever.
pub fn poll_once(now: u64) -> Result<bool, TodoError> {
    let driving: Vec<(String, Drive)> = DRIVING
        .lock()
        .unwrap()
        .iter()
        .map(|(todo, drive)| (todo.clone(), drive.clone()))
        .collect();
    for (todo_id, drive) in driving {
        let read = session_call(
            &drive.contract,
            jinn_session::OP_GET,
            &serde_json::to_vec(&serde_json::json!({ "session-id": drive.session_id }))
                .expect("encodes"),
        );
        let record: jinn_session::SessionRecord = match read.and_then(|value| {
            serde_json::from_value(value).map_err(|error| {
                TodoError::new(
                    ErrorCode::Failed,
                    format!("malformed session record: {error}"),
                )
            })
        }) {
            Ok(record) => record,
            Err(error) => {
                // The session is unreadable — its store was swapped, the
                // provider is gone. That is not a reason to keep
                // claiming the dispatch is running.
                DRIVING.lock().unwrap().remove(&todo_id);
                end_dispatch(
                    &todo_id,
                    &drive.dispatch_id,
                    DispatchStatus::Failed,
                    Some(format!("{}: {}", translate::LOST_SESSION_REASON, error.message)),
                    String::new(),
                    now,
                )?;
                continue;
            }
        };
        let Some((status, reason, answer)) = translate::ended(&record, &drive.turn_id) else {
            continue;
        };
        DRIVING.lock().unwrap().remove(&todo_id);
        end_dispatch(&todo_id, &drive.dispatch_id, status, reason, answer, now)?;
    }
    Ok(!DRIVING.lock().unwrap().is_empty())
}

fn record_of(todo_id: &str) -> Result<serde_json::Value, TodoError> {
    with_todos(|todos| todos.record(todo_id))
        .map(|record| serde_json::to_value(record).expect("encodes"))
        .ok_or_else(|| TodoError::new(ErrorCode::NotFound, format!("{todo_id:?} is not here")))
}

fn decode<T: serde::de::DeserializeOwned>(payload: &[u8], what: &str) -> Result<T, TodoError> {
    serde_json::from_slice(payload).map_err(|error| {
        TodoError::new(
            ErrorCode::Invalid,
            format!("malformed {what} request: {error}"),
        )
    })
}

/// Every operation but `describe`, which each provider answers with its
/// own declaration.
pub fn dispatch(operation: &str, payload: &[u8]) -> Answer {
    let outcome = match operation {
        OP_CREATE => on_create(payload),
        OP_UPDATE => on_update(payload),
        OP_COMMENT => on_comment(payload),
        OP_DISPATCH => on_dispatch(payload),
        OP_GET => decode::<GetRequest>(payload, "get").and_then(|request| record_of(&request.todo_id)),
        OP_TREE => decode::<TreeRequest>(payload, "tree").and_then(|request| {
            with_todos(|todos| todos.tree(&request.todo_id))
                .map(|tree| serde_json::to_value(tree).expect("encodes"))
                .ok_or_else(|| {
                    TodoError::new(
                        ErrorCode::NotFound,
                        format!("{:?} is not here", request.todo_id),
                    )
                })
        }),
        OP_EVENTS => decode::<EventsRequest>(payload, "events").and_then(|request| {
            with_todos(|todos| todos.events_since(&request.todo_id, request.after, request.limit))
                .map(|page| serde_json::to_value(page).expect("encodes"))
                .ok_or_else(|| {
                    TodoError::new(
                        ErrorCode::NotFound,
                        format!("{:?} is not here", request.todo_id),
                    )
                })
        }),
        OP_LIST => decode::<ListRequest>(payload, "list").map(|request| {
            serde_json::to_value(with_todos(|todos| todos.list(&request))).expect("encodes")
        }),
        other => Err(TodoError::new(
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
        "todos": with_todos(|todos| todos.ids().count()),
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
    let config: StoreConfig = serde_json::from_slice(config_bytes)
        .map_err(|error| format!("malformed config: {error}"))?;
    if config.store.is_empty() {
        return Err("config.store is the store id this provider serves; it cannot be empty".into());
    }
    *TODOS.lock().unwrap() = Some(Todos::new(config.store.clone()));
    // A session's turn belongs to the incarnation that started it, so a
    // fresh incarnation drives nothing and adopts no dispatch.
    *DRIVING.lock().unwrap() = BTreeMap::new();
    *DEFERRED.lock().unwrap() = Vec::new();
    *EMIT_FAILURES.lock().unwrap() = 0;
    *CONFIG.lock().unwrap() = Some(config.clone());
    journal::adopt_all(&config).map_err(|error| error.message)?;
    Ok(config)
}

/// One wake: everything held goes on the wire, every live dispatch is
/// polled, and the poll re-arms while anything is still being driven.
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
