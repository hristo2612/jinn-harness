//! The `jinn:engine` seam's `claude` provider. Every run decision is the
//! definition's (`jinn-engine`: the request types, the event vocabulary,
//! and `Runs` — the registry that mints ids, sequences events, assembles
//! the answer and accounts the budgets); every stream and argv decision
//! is `jinn-engine-claude-wire`'s. This guest is the WIRING between them
//! and the kernel surfaces, and holds nothing else.
//!
//! # What the profile holds and the source does not
//!
//! Every machine-specific value lives in the entry's `config.data` —
//! `engine` (which names the contract, `jinn:engine.<id>`), `command`
//! (the CLI's absolute path), `models`, `default-model`, `poll-ms`,
//! `keep-runs`. No path, no model name and no cadence is compiled in, so
//! swapping the binary or the engine id is a profile edit.
//!
//! # An idle provider costs nothing
//!
//! `activate` arms NO alarm. A one-shot `jinn:clock` alarm is armed when
//! a run starts and re-armed on each wake only while a run is still
//! live — the discipline FINDINGS.md #23's closure bought this repo: an
//! idle entry writes zero ledger rows.
//!
//! # Honesty at the environment gate
//!
//! An absent or unrunnable CLI is `ErrorCode::Unavailable` and a failed
//! run record, never a faked answer. A refused `jinn:process` grant is
//! `Refused`. Both are recorded on the run before the caller hears them.
//!
//! # The prompt is never argv
//!
//! `RunRequest::prompt` goes to the child on STDIN (bare `-p` makes the
//! CLI read it there). The host's process table is world-readable and a
//! prompt is personal data.
//!
//! # Lifecycle
//!
//! A run does not survive an incarnation: a spawned child is a KERNEL
//! registration, killed and ledgered on suspend and on dispose alike, so
//! `snapshot`/`restore` hand nothing over and the next `activate`
//! re-declares an empty registry.

use std::collections::BTreeMap;
use std::sync::Mutex;

use jinn_engine::{
    engine_contract, Answer, CancelRequest, Capabilities, Description, EngineError, ErrorCode,
    Event, Extensions, RunRequest, Runs, Usage, API_VERSION, EVENT_TOPIC, OP_CANCEL, OP_DESCRIBE,
    OP_RUN, OP_RUN_GET,
};
use jinn_engine_claude_wire::{argv, parse_config, Decoder, ProviderConfig};

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::keystore::KeystoreError;
use jinn::plugin::process::{ChildStream, ProcessError, ReadResult, Signal, WaitResult};
use jinn::plugin::types::{DispatchMode, Selector};
use jinn::plugin::{clock, effects, events, keystore, process, services};

/// This entry's on-duty effect.
const EFFECT_TOKEN: u64 = 1;
/// The token every poll alarm is requested under; a wake carrying any
/// other token is not ours.
const POLL_TOKEN: u64 = 2;
/// The kernel's own alarm delivery topic.
const WAKE_TOPIC: &str = "jinn:clock/alarm";
/// What `describe` calls this implementation, for an operator reading a
/// swap.
const PROVIDER: &str = "engines/jinn-engine-claude";
/// Bytes taken per `jinn:process` read.
const READ_CHUNK: u32 = 8192;
/// Passes the prompt write may make without the child accepting a single
/// byte before the run is given up on. `write-stdin` is non-blocking, so
/// this is what keeps a stalled child from becoming an unbounded spin.
const STDIN_STALL_PASSES: u32 = 4096;

/// The entry's parsed `config.data`.
static CONFIG: Mutex<Option<ProviderConfig>> = Mutex::new(None);
/// The definition's registry: ids, sequence, record, budgets.
static RUNS: Mutex<Option<Runs>> = Mutex::new(None);
/// `run-id → the child carrying it`.
static CHILDREN: Mutex<BTreeMap<String, Child>> = Mutex::new(BTreeMap::new());

/// One live child and the decoder reading it.
struct Child {
    handle: u64,
    decoder: Decoder,
}

fn fault(context: &str, error: jinn::plugin::types::KernelError) -> GuestFault {
    GuestFault::Failed(format!("{context}: {error:?}"))
}

/// The activated config. Every entry point runs after `activate`.
fn config() -> ProviderConfig {
    CONFIG
        .lock()
        .unwrap()
        .clone()
        .expect("the provider is activated")
}

/// Records one event against the run and puts it on the bus — the
/// sequence and the record stay ONE truth.
///
/// The registry lock is released BEFORE the emit: a listener may call
/// back into this provider, and a lock held across a dispatch is the
/// nested-dispatch deadlock (FINDINGS.md #4, #26). An emit needs no
/// grant; a refusal is the bus's, and losing a run is not worth failing
/// the run over — the record still carries the event.
fn record_and_emit(run_id: &str, event: Event) {
    let emitted = RUNS
        .lock()
        .unwrap()
        .as_mut()
        .and_then(|runs| runs.record(run_id, event));
    if let Some(record) = emitted {
        let payload = serde_json::to_vec(&record).expect("a run event encodes");
        let _ = events::emit(EVENT_TOPIC, DispatchMode::Emit, &Selector::All, &payload);
    }
}


/// Bus records minted inside a `run` call and held until this provider's
/// OWN fiber wakes. A provider must never emit from inside a handler the
/// caller is parked in: if the caller listens on this topic — the probe
/// consumer does — the delivery parks on ITS busy supervisor and the call
/// chain waits out the guest deadline (FINDINGS.md #4, nested dispatch),
/// which destroys the caller's instance. The poll wake is already armed
/// by `run`, so the deferral costs one poll period, never a wake of its
/// own.
static DEFERRED: Mutex<Vec<jinn_engine::RunEvent>> = Mutex::new(Vec::new());

/// Records `event` and holds its bus record for the next wake — see
/// [`DEFERRED`].
fn record_deferred(run_id: &str, event: Event) {
    let emitted = RUNS
        .lock()
        .unwrap()
        .as_mut()
        .and_then(|runs| runs.record(run_id, event));
    if let Some(record) = emitted {
        DEFERRED.lock().unwrap().push(record);
    }
}

/// Puts everything held for this fiber on the wire, in sequence. Called
/// FIRST on every wake, so a run's `started` always precedes its rest.
fn flush_deferred() {
    let records = std::mem::take(&mut *DEFERRED.lock().unwrap());
    for record in &records {
        let payload = serde_json::to_vec(record).expect("a run event encodes");
        let _ = events::emit(EVENT_TOPIC, DispatchMode::Emit, &Selector::All, &payload);
    }
}

/// Marks a run failed before it ever ran, and says so on the bus.
fn fail_and_emit(run_id: &str, reason: &str) {
    let emitted = RUNS
        .lock()
        .unwrap()
        .as_mut()
        .and_then(|runs| runs.fail(run_id, reason));
    if let Some(record) = emitted {
        let payload = serde_json::to_vec(&record).expect("a run event encodes");
        let _ = events::emit(EVENT_TOPIC, DispatchMode::Emit, &Selector::All, &payload);
    }
}

/// A refusal the run wears too: recorded and emitted, then answered.
fn refuse(run_id: &str, code: ErrorCode, message: String) -> Answer {
    fail_and_emit(run_id, &message);
    Answer::error(EngineError::new(code, message))
}

fn live_ids() -> Vec<String> {
    RUNS.lock()
        .unwrap()
        .as_ref()
        .map(Runs::live_ids)
        .unwrap_or_default()
}

fn handle_of(run_id: &str) -> Option<u64> {
    CHILDREN
        .lock()
        .unwrap()
        .get(run_id)
        .map(|child| child.handle)
}

/// Kills the run's child and records why it ended. Used by `cancel` and
/// by both budgets — the seam's one shape for "ended because someone
/// asked, or because a bound was hit".
fn kill_and_cancel(run_id: &str, reason: &str) {
    if let Some(child) = CHILDREN.lock().unwrap().remove(run_id) {
        let _ = process::kill(child.handle, Signal::Terminate);
    }
    record_and_emit(
        run_id,
        Event::Cancelled {
            reason: reason.to_owned(),
        },
    );
}

/// Reads everything the child has to say right now, feeding the codec and
/// emitting what it yields. Answers `false` once the output budget is
/// spent (the child has been killed and the run recorded).
fn drain(run_id: &str) -> bool {
    let Some(handle) = handle_of(run_id) else {
        return true;
    };
    loop {
        let bytes = match process::read(handle, ChildStream::Stdout, READ_CHUNK) {
            Ok(ReadResult::Data(bytes)) => bytes,
            Ok(ReadResult::Eof) => {
                // A pipe may end without a final newline; the codec's
                // last word.
                emit_stream(run_id, flush_decoder(run_id));
                break;
            }
            // Nothing buffered now, or the child is gone: either way
            // there is nothing more to read this wake.
            Ok(ReadResult::WouldBlock) | Err(_) => break,
        };
        let read = bytes.len() as u64;
        let events = {
            let mut children = CHILDREN.lock().unwrap();
            let Some(child) = children.get_mut(run_id) else {
                break;
            };
            child.decoder.feed(&bytes)
        };
        emit_stream(run_id, events);
        // The definition's accounting, not ours — and when the budget is
        // spent it answers the event, so the CUT reaches the bus before
        // the cancellation that follows it (a listener that saw only
        // `cancelled` could not tell a bounded answer from a whole one).
        let cut = RUNS
            .lock()
            .unwrap()
            .as_mut()
            .and_then(|runs| runs.read(run_id, read));
        if let Some(cut) = cut {
            record_and_emit(run_id, cut);
            kill_and_cancel(run_id, "budget");
            return false;
        }
    }
    // stderr is drained and dropped: the host's per-stream buffer is
    // BOUNDED, so a stream nobody reads backpressures the child into a
    // stall. The seam has no event for a diagnostic line, so this is a
    // read, not a record.
    while let Ok(ReadResult::Data(_)) = process::read(handle, ChildStream::Stderr, READ_CHUNK) {}
    true
}

fn flush_decoder(run_id: &str) -> Vec<Event> {
    CHILDREN
        .lock()
        .unwrap()
        .get_mut(run_id)
        .map(|child| child.decoder.flush())
        .unwrap_or_default()
}

/// Puts the codec's events on the run.
///
/// A `Started` from the STREAM is dropped: the seam's `Started` means
/// "the child is spawned and the run is live", which happened once, at
/// `run`. The CLI's own init line repeating it would be a second start
/// that never occurred.
fn emit_stream(run_id: &str, events: Vec<Event>) {
    for event in events {
        if matches!(event, Event::Started { .. }) {
            continue;
        }
        record_and_emit(run_id, event);
    }
}

/// What the codec learned this run cost, and whether the provider cut the
/// stream on the budget.
fn settlement(run_id: &str) -> (Usage, bool, Option<String>) {
    // The stream can report a FAILED turn on a child that exits 0, so the
    // exit status alone would read as a clean success. The codec sees it;
    // the seam has a field for it since `Event::Exited { error }`.
    let (usage, error) = CHILDREN
        .lock()
        .unwrap()
        .get(run_id)
        .map(|child| {
            let error = child.decoder.failed().then(|| {
                format!(
                    "the engine reported a failed turn ({})",
                    child.decoder.result_subtype().unwrap_or("no subtype")
                )
            });
            (child.decoder.usage(), error)
        })
        .unwrap_or_default();
    let truncated = RUNS
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|runs| runs.get(run_id).map(|record| record.truncated))
        .unwrap_or(false);
    (usage, truncated, error)
}

/// One poll wake: every live run drained, settled, or cut on a budget.
fn poll() {
    let config = config();
    let Ok(now) = clock::now() else {
        // Without the clock there is no wall budget and no next wake;
        // the runs stay live and the next `run` re-arms.
        return;
    };
    for run_id in live_ids() {
        if !drain(&run_id) {
            continue;
        }
        let over = RUNS
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|runs| runs.over_wall_budget(&run_id, now));
        if over {
            kill_and_cancel(&run_id, "budget");
            continue;
        }
        let Some(handle) = handle_of(&run_id) else {
            // Live in the registry with no child: the spawn never landed.
            continue;
        };
        match process::wait(handle, 0) {
            Ok(WaitResult::Exited(status)) => {
                // Whatever the child left behind before it went.
                if !drain(&run_id) {
                    continue;
                }
                let (usage, truncated, error) = settlement(&run_id);
                CHILDREN.lock().unwrap().remove(&run_id);
                record_and_emit(
                    &run_id,
                    Event::Exited {
                        status,
                        usage,
                        truncated,
                        error,
                    },
                );
            }
            Ok(WaitResult::Running) => {}
            Err(error) => kill_and_cancel(&run_id, &format!("child lost: {error:?}")),
        }
    }
    {
        let mut runs = RUNS.lock().unwrap();
        if let Some(runs) = runs.as_mut() {
            runs.retain_recent(config.keep_runs);
            // A child map entry outliving its record would be a leak.
            CHILDREN
                .lock()
                .unwrap()
                .retain(|run_id, _| runs.get(run_id).is_some());
        }
    }
    // The alarm exists only while there is something to poll.
    if !live_ids().is_empty() {
        let _ = clock::alarm_at(now.saturating_add(config.poll_ms), POLL_TOKEN);
    }
}

/// `describe`: what this provider is and what it can do.
fn describe() -> Answer {
    let config = config();
    Answer::ok(Description {
        api_version: API_VERSION.to_owned(),
        engine: config.engine,
        provider: PROVIDER.to_owned(),
        models: config.models,
        default_model: config.default_model,
        capabilities: Capabilities {
            streaming: true,
            tool_calls: true,
            cancel: true,
            usage: true,
            external_cli: true,
            ..Capabilities::default()
        },
        extra: Extensions::new(),
    })
}

/// Resolves the request's secret NAMES into the child's environment. The
/// values never touch the profile, the ledger, or an error message.
fn resolve_secrets(run_id: &str, request: &RunRequest) -> Result<Vec<(String, String)>, Answer> {
    let mut env = Vec::new();
    for (variable, reference) in &request.secrets {
        let key = reference.secret.as_str();
        match keystore::get(key) {
            Ok(value) => match String::from_utf8(value) {
                Ok(value) => env.push((variable.clone(), value)),
                Err(_) => {
                    return Err(refuse(
                        run_id,
                        ErrorCode::Invalid,
                        format!("secret {key:?} is not a UTF-8 environment value"),
                    ))
                }
            },
            Err(KeystoreError::Denied(detail)) => {
                return Err(refuse(
                    run_id,
                    ErrorCode::Refused,
                    format!("secret {key:?} denied: {detail}"),
                ))
            }
            Err(KeystoreError::NotFound) => {
                return Err(refuse(
                    run_id,
                    ErrorCode::Invalid,
                    format!("no secret named {key:?}"),
                ))
            }
            Err(other) => {
                return Err(refuse(
                    run_id,
                    ErrorCode::Failed,
                    format!("secret {key:?}: {other:?}"),
                ))
            }
        }
    }
    Ok(env)
}

/// Offers the whole prompt on the child's stdin, then closes it (the
/// child's EOF, which is what makes `-p` answer).
fn deliver_prompt(handle: u64, prompt: &str) -> Result<(), String> {
    let bytes = prompt.as_bytes();
    let mut offered = 0usize;
    let mut stalled = 0u32;
    while offered < bytes.len() {
        match process::write_stdin(handle, &bytes[offered..]) {
            Ok(0) => {
                // Non-blocking: the pipe is full until the child reads.
                stalled += 1;
                if stalled > STDIN_STALL_PASSES {
                    return Err(format!(
                        "the child accepted {offered} of {} prompt bytes and then stalled",
                        bytes.len()
                    ));
                }
            }
            Ok(accepted) => {
                offered += accepted as usize;
                stalled = 0;
            }
            Err(error) => return Err(format!("prompt write: {error:?}")),
        }
    }
    process::close_stdin(handle).map_err(|error| format!("close stdin: {error:?}"))
}

/// `run`: accept, spawn, start streaming.
fn run(payload: &[u8]) -> Answer {
    let config = config();
    let request: RunRequest = match serde_json::from_slice(payload) {
        Ok(request) => request,
        Err(error) => {
            return Answer::error(EngineError::new(
                ErrorCode::Invalid,
                format!("malformed run request: {error}"),
            ))
        }
    };
    // The engine id is the ROUTE. A request for another engine reached
    // the wrong contract.
    if request.engine != config.engine {
        return Answer::error(EngineError::new(
            ErrorCode::NotFound,
            format!(
                "this provider serves engine {:?}, not {:?}",
                config.engine, request.engine
            ),
        ));
    }
    let now = match clock::now() {
        Ok(now) => now,
        Err(error) => {
            return Answer::error(EngineError::new(
                ErrorCode::Failed,
                format!("clock now: {error:?}"),
            ))
        }
    };
    let model = request
        .model
        .clone()
        .or_else(|| config.default_model.clone());
    let mut accepted = RUNS
        .lock()
        .unwrap()
        .as_mut()
        .expect("the provider is activated")
        .accept(&request, now);
    // The run answers with the model it will actually use, defaults
    // included.
    accepted.model.clone_from(&model);
    let run_id = accepted.run_id.clone();

    let env = match resolve_secrets(&run_id, &request) {
        Ok(env) => env,
        Err(answer) => return answer,
    };

    let args = argv(model.as_deref(), &request.tools);
    let handle = match process::spawn(&config.command, &args, request.cwd.as_deref(), &env) {
        Ok(handle) => handle,
        Err(ProcessError::Denied(detail)) => {
            return refuse(
                &run_id,
                ErrorCode::Refused,
                format!("spawn denied: {detail}"),
            )
        }
        Err(error) => {
            // The honest environment gate: this provider is mounted and
            // correct, this box cannot run the CLI.
            return refuse(
                &run_id,
                ErrorCode::Unavailable,
                format!("{} cannot run here: {error:?}", config.command),
            );
        }
    };

    if let Err(detail) = deliver_prompt(handle, &request.prompt) {
        let _ = process::kill(handle, Signal::Terminate);
        return refuse(&run_id, ErrorCode::Failed, detail);
    }

    CHILDREN.lock().unwrap().insert(
        run_id.clone(),
        Child {
            handle,
            decoder: Decoder::new(),
        },
    );
    record_deferred(&run_id, Event::Started { model });
    // The only alarm this provider ever holds, and only from here.
    if let Err(error) = clock::alarm_at(now.saturating_add(config.poll_ms), POLL_TOKEN) {
        kill_and_cancel(&run_id, &format!("no poll alarm: {error:?}"));
        return Answer::error(EngineError::new(
            ErrorCode::Failed,
            format!("poll alarm: {error:?}"),
        ));
    }
    Answer::ok(accepted)
}

/// The `{run-id}` document. The definition names exactly one — the
/// `cancel` request — and `run-get` takes the same shape rather than a
/// second spelling of one fact.
fn run_id_of(payload: &[u8]) -> Result<String, Answer> {
    serde_json::from_slice::<CancelRequest>(payload)
        .map(|request| request.run_id)
        .map_err(|error| {
            Answer::error(EngineError::new(
                ErrorCode::Invalid,
                format!("malformed request: {error}"),
            ))
        })
}

/// `run-get`: one run's record so far.
fn run_get(payload: &[u8]) -> Answer {
    let run_id = match run_id_of(payload) {
        Ok(run_id) => run_id,
        Err(answer) => return answer,
    };
    let record = RUNS
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|runs| runs.get(&run_id).cloned());
    match record {
        Some(record) => Answer::ok(record),
        None => Answer::error(EngineError::new(
            ErrorCode::NotFound,
            format!("no run {run_id:?}"),
        )),
    }
}

/// `cancel`: kill the child and record why.
fn cancel(payload: &[u8]) -> Answer {
    let run_id = match run_id_of(payload) {
        Ok(run_id) => run_id,
        Err(answer) => return answer,
    };
    let known = RUNS
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|runs| runs.get(&run_id).is_some());
    if !known {
        return Answer::error(EngineError::new(
            ErrorCode::NotFound,
            format!("no run {run_id:?}"),
        ));
    }
    kill_and_cancel(&run_id, "cancel");
    let record = RUNS
        .lock()
        .unwrap()
        .as_ref()
        .and_then(|runs| runs.get(&run_id).cloned());
    record.map_or_else(
        || {
            Answer::error(EngineError::new(
                ErrorCode::NotFound,
                format!("no run {run_id:?}"),
            ))
        },
        Answer::ok,
    )
}

struct Provider;

impl Guest for Provider {
    fn activate(config: Vec<u8>) -> Result<(), GuestFault> {
        let parsed = parse_config(&config).map_err(GuestFault::Failed)?;
        // A run does not survive an incarnation: the kernel killed the
        // last one's children, so the registry starts empty.
        *RUNS.lock().unwrap() = Some(Runs::new(&parsed.engine));
        CHILDREN.lock().unwrap().clear();
        let contract = engine_contract(&parsed.engine);
        *CONFIG.lock().unwrap() = Some(parsed);
        effects::register("jinn-engine-claude on duty", EFFECT_TOKEN)
            .map_err(|error| fault("effect", error))?;
        services::provide(&contract).map_err(|error| fault("provide", error))?;
        // No alarm here. An idle provider costs ZERO ledger rows; the
        // first `run` arms the poll.
        Ok(())
    }

    fn check(_consumer: u64) -> bool {
        true
    }

    fn undo(_token: u64) -> Result<(), GuestFault> {
        // Nothing guest-side to reverse: the provision and the alarm are
        // kernel registrations, and a spawned child is one too — the
        // kernel kills it on suspend and on dispose alike.
        Ok(())
    }

    fn handle_event(token: u64, topic: String, payload: Vec<u8>) -> Result<Vec<u8>, GuestFault> {
        if topic == WAKE_TOPIC && token == POLL_TOKEN {
            // Anything minted inside a `run` call goes on the wire here,
            // on this provider's own fiber (see [`DEFERRED`]).
            flush_deferred();
            poll();
            return Ok(Vec::new());
        }
        Err(GuestFault::Failed(format!(
            "unexpected event {topic:?} (token {token}, {} bytes)",
            payload.len()
        )))
    }

    fn handle_call(
        _caller: u64,
        _contract: String,
        operation: String,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, GuestFault> {
        let answer = match operation.as_str() {
            OP_DESCRIBE => describe(),
            OP_RUN => run(&payload),
            OP_RUN_GET => run_get(&payload),
            OP_CANCEL => cancel(&payload),
            other => return Err(GuestFault::Failed(format!("unknown operation {other:?}"))),
        };
        Ok(answer.encode())
    }

    fn snapshot() -> Vec<u8> {
        // A live child cannot be handed to a successor instance, and a
        // record of a run nobody can still read is not worth carrying.
        Vec::new()
    }

    fn restore(_blob: Vec<u8>) -> Result<(), GuestFault> {
        Ok(())
    }
}

export!(Provider);
