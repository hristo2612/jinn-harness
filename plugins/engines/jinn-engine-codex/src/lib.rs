//! The `jinn:engine.<id>` provider for the codex CLI. Every run decision
//! that is not vendor-specific belongs to the definition (`jinn-engine`'s
//! `Runs`: ids, sequencing, answer assembly, budget accounting) and every
//! vendor-specific one to the codec (`jinn-engine-codex-wire`: the argv
//! and the JSONL). What is left — and all this guest is — is kernel
//! plumbing: spawn through `jinn:process`, resolve secret NAMES through
//! `jinn:keystore`, wake on `jinn:clock`, publish on the bus.
//!
//! # The entry is where the machine lives
//!
//! ```json
//! { "engine": "codex", "command": "<absolute path to the codex CLI>",
//!   "models": ["…"], "default-model": "…", "poll-ms": 250, "keep-runs": 8 }
//! ```
//!
//! Not one of those values is in source: the repo is public and holds no
//! machine paths (AGENTS.md §Repo hygiene), and `engine` is the ROUTE —
//! it names the contract this entry provides, so a second codex entry
//! under a different id coexists with this one by profile edit alone.
//!
//! **Grants this entry needs.** `jinn:process` scoped to the codex
//! executable, with an env policy that admits `HOME` **and `PATH`**: the
//! `codex` binary is a node script, so its shebang resolves `node` off
//! `PATH` and a `PATH`-less child dies before it starts. `jinn:clock` for
//! the poll wake, and `jinn:keystore` scoped to the key prefix a run's
//! `secrets` may name. Writing the entry is the profile's business; this
//! note is here so whoever writes it knows.
//!
//! # An idle provider costs zero ledger rows
//!
//! `activate` arms NO alarm. A wake is requested when a run starts and
//! re-requested only while a run is still live, so a provider nobody is
//! using is silent — the discipline FINDINGS.md #23's closure bought for
//! this repo, applied to a poller that has an honest reason to stop.
//!
//! # A run does not survive an incarnation
//!
//! The kernel kills a spawned child on suspend and on dispose alike
//! (`jinn:process` is a kernel registration), so a handle held here is
//! meaningless to the next incarnation. `snapshot` therefore hands off
//! nothing and `restore` accepts nothing: the honest contract is that a
//! restart loses its live runs, not that it pretends to carry them.
//!
//! # Two `started` events, on purpose
//!
//! The provider emits `started` when the child is spawned — the fact the
//! kernel witnessed. The codec emits another when codex announces its own
//! thread — the fact the CLI reported. They are different facts about
//! different layers, both sequenced, and `Runs` folds neither.

use std::collections::BTreeMap;
use std::sync::Mutex;

use jinn_engine::{
    engine_contract, Answer, CancelRequest, Capabilities, Description, EngineError, ErrorCode,
    Event, Extensions, RunEvent, RunRequest, Runs, API_VERSION, EVENT_TOPIC, OP_CANCEL,
    OP_DESCRIBE, OP_RUN, OP_RUN_GET,
};
use jinn_engine_codex_wire::{argv, Decoder};
use serde::Deserialize;

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::keystore::KeystoreError;
use jinn::plugin::process::{ChildStream, ProcessError, ReadResult, Signal, WaitResult};
use jinn::plugin::types::{DispatchMode, Selector};
use jinn::plugin::{clock, effects, events, keystore, process, services};

/// This package, as `describe` names it for an operator reading a swap.
const PROVIDER: &str = "engines/jinn-engine-codex";
/// The kernel's typed alarm delivery.
const WAKE_TOPIC: &str = "jinn:clock/alarm";
const EFFECT_TOKEN: u64 = 1;
/// The token the poll alarm is requested under; a wake carrying any other
/// token is not ours.
const POLL_TOKEN: u64 = 2;
/// Bytes taken per `read`.
const READ_CHUNK: u32 = 8192;
/// Reads per stream per wake — a flooding child never holds the wake
/// open (R9); what is left is read on the next one.
const READS_PER_WAKE: usize = 256;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct Config {
    /// The engine id — the second half of the contract name.
    engine: String,
    /// The CLI's absolute path, from the profile. Never a default: a
    /// guessed path is how a provider fakes an environment it does not
    /// have.
    command: String,
    #[serde(default)]
    models: Vec<String>,
    #[serde(default)]
    default_model: Option<String>,
    #[serde(default = "default_poll_ms")]
    poll_ms: u64,
    #[serde(default = "default_keep_runs")]
    keep_runs: usize,
}

fn default_poll_ms() -> u64 {
    250
}

fn default_keep_runs() -> usize {
    8
}

static CONFIG: Mutex<Option<Config>> = Mutex::new(None);
static RUNS: Mutex<Option<Runs>> = Mutex::new(None);
/// run id → the child's process handle, for the runs that have one.
static CHILDREN: Mutex<BTreeMap<String, u64>> = Mutex::new(BTreeMap::new());
/// run id → its stream decoder.
static DECODERS: Mutex<BTreeMap<String, Decoder>> = Mutex::new(BTreeMap::new());
/// run id → the prompt bytes stdin has not accepted yet. `write-stdin` is
/// non-blocking and answers a COUNT, so a prompt larger than the pipe's
/// buffer is re-offered on later wakes rather than spun on here.
static PENDING: Mutex<BTreeMap<String, Vec<u8>>> = Mutex::new(BTreeMap::new());

fn config() -> Option<Config> {
    CONFIG.lock().unwrap().clone()
}

/// Publishes one run event. The emit needs no grant; a refused emit is
/// not fatal, because the run RECORD already holds the same event and
/// `run-get` still answers the whole truth.
fn publish(record: Option<RunEvent>) {
    if let Some(record) = record {
        let payload = serde_json::to_vec(&record).expect("a run event encodes");
        let _ = events::emit(EVENT_TOPIC, DispatchMode::Emit, &Selector::All, &payload);
    }
}

/// Records an event against a run and publishes it. The registry lock is
/// released BEFORE the emit: a listener may call back into this provider,
/// and holding it across the bus is the nested-dispatch deadlock
/// (FINDINGS.md #4).
fn record_and_emit(run_id: &str, event: Event) {
    let record = RUNS
        .lock()
        .unwrap()
        .as_mut()
        .and_then(|runs| runs.record(run_id, event));
    publish(record);
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

/// Fails a run that never got to run, then refuses the call with the same
/// message. The bus learns first (the record is the run's truth), the
/// caller second.
fn fail(run_id: &str, code: ErrorCode, message: String) -> Answer {
    let record = RUNS
        .lock()
        .unwrap()
        .as_mut()
        .and_then(|runs| runs.fail(run_id, message.clone()));
    publish(record);
    forget(run_id);
    Answer::error(EngineError::new(code, message))
}

/// Drops one run's per-child state. The RECORD stays — `Runs` bounds it
/// with `retain_recent`.
fn forget(run_id: &str) {
    CHILDREN.lock().unwrap().remove(run_id);
    DECODERS.lock().unwrap().remove(run_id);
    PENDING.lock().unwrap().remove(run_id);
}

fn record_of(run_id: &str) -> Option<jinn_engine::RunRecord> {
    RUNS.lock()
        .unwrap()
        .as_ref()
        .and_then(|runs| runs.get(run_id))
        .cloned()
}

/// Offers what stdin has not taken of the prompt and closes it once the
/// whole prompt is in. The prompt is never an argv element: argv is
/// world-readable in the host's process table and a prompt is personal
/// data.
fn pump_stdin(run_id: &str, handle: u64) -> Result<(), String> {
    let mut pending = PENDING.lock().unwrap();
    let Some(rest) = pending.get_mut(run_id) else {
        return Ok(());
    };
    while !rest.is_empty() {
        match process::write_stdin(handle, rest) {
            // The pipe is full right now; the next wake re-offers.
            Ok(0) => return Ok(()),
            Ok(accepted) => {
                let accepted = (accepted as usize).min(rest.len());
                rest.drain(..accepted);
            }
            Err(error) => {
                pending.remove(run_id);
                return Err(format!("stdin: {error:?}"));
            }
        }
    }
    pending.remove(run_id);
    drop(pending);
    process::close_stdin(handle).map_err(|error| format!("close stdin: {error:?}"))
}

/// Takes what one stream has buffered now, feeding stdout to the codec.
/// Answers the decoded events and the STDOUT bytes read (the seam's
/// output budget counts the answer stream; stderr is drained and
/// discarded so a child that writes diagnostics is never blocked by our
/// backpressure, and is not charged for them).
fn drain(run_id: &str, handle: u64, which: ChildStream) -> (Vec<Event>, u64) {
    let stdout = matches!(which, ChildStream::Stdout);
    let mut events = Vec::new();
    let mut bytes = 0_u64;
    for _ in 0..READS_PER_WAKE {
        match process::read(handle, which, READ_CHUNK) {
            Ok(ReadResult::Data(data)) => {
                if !stdout {
                    continue;
                }
                bytes += data.len() as u64;
                if let Some(decoder) = DECODERS.lock().unwrap().get_mut(run_id) {
                    events.extend(decoder.feed(&data));
                }
            }
            Ok(ReadResult::WouldBlock | ReadResult::Eof) | Err(_) => break,
        }
    }
    (events, bytes)
}

/// One drain, published, with its bytes charged to the output budget.
/// Answers `true` once the budget is spent.
fn drain_and_account(run_id: &str, handle: u64) -> bool {
    let (events, bytes) = drain(run_id, handle, ChildStream::Stdout);
    for event in events {
        record_and_emit(run_id, event);
    }
    let _ = drain(run_id, handle, ChildStream::Stderr);
    RUNS.lock()
        .unwrap()
        .as_mut()
        .is_some_and(|runs| runs.read(run_id, bytes))
}

/// Kills a live child and records why it ended.
fn end_child(run_id: &str, handle: u64, reason: &str) {
    let _ = process::kill(handle, Signal::Terminate);
    record_and_emit(
        run_id,
        Event::Cancelled {
            reason: reason.to_owned(),
        },
    );
    forget(run_id);
}

/// The exit, as the PROCESS reports it — never as the stream claimed it.
fn end_exited(run_id: &str, handle: u64, status: i32) {
    // Whatever the child wrote between the last drain and its exit.
    drain_and_account(run_id, handle);
    let (flushed, usage, error) = {
        let mut decoders = DECODERS.lock().unwrap();
        match decoders.get_mut(run_id) {
            Some(decoder) => {
                // An error ITEM does not fail the process, so a status of
                // 0 alone would read as a clean success. The messages the
                // codec kept off the bus are what `Exited { error }` is
                // for.
                let reported = (!decoder.errors().is_empty()).then(|| decoder.errors().join("; "));
                (decoder.flush(), decoder.usage(), reported)
            }
            None => (Vec::new(), jinn_engine::Usage::default(), None),
        }
    };
    for event in flushed {
        record_and_emit(run_id, event);
    }
    let truncated = record_of(run_id).is_some_and(|record| record.truncated);
    record_and_emit(
        run_id,
        Event::Exited {
            status,
            usage,
            truncated,
            error,
        },
    );
    forget(run_id);
}

fn arm(at_ms: u64) -> Result<(), GuestFault> {
    clock::alarm_at(at_ms, POLL_TOKEN)
        .map(|_| ())
        .map_err(|error| GuestFault::Failed(format!("alarm: {error:?}")))
}

/// One poll wake: advance every live run, then re-arm only if one is
/// still live.
fn poll(now_ms: u64, config: &Config) -> Result<(), GuestFault> {
    let live: Vec<String> = RUNS
        .lock()
        .unwrap()
        .as_ref()
        .map(Runs::live_ids)
        .unwrap_or_default();
    for run_id in live {
        let Some(handle) = CHILDREN.lock().unwrap().get(&run_id).copied() else {
            // Accepted, never spawned: there is nothing to poll and the
            // `run` call already answered.
            continue;
        };
        if let Err(detail) = pump_stdin(&run_id, handle) {
            end_child(&run_id, handle, &detail);
            continue;
        }
        if drain_and_account(&run_id, handle) {
            end_child(&run_id, handle, "budget");
            continue;
        }
        let over = RUNS
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|runs| runs.over_wall_budget(&run_id, now_ms));
        if over {
            end_child(&run_id, handle, "budget");
            continue;
        }
        match process::wait(handle, 0) {
            Ok(WaitResult::Exited(status)) => end_exited(&run_id, handle, status),
            Ok(WaitResult::Running) => {}
            // The host no longer knows this child. That is a lost run,
            // recorded as one — never a fabricated exit status.
            Err(error) => {
                let record = RUNS
                    .lock()
                    .unwrap()
                    .as_mut()
                    .and_then(|runs| runs.fail(&run_id, format!("child lost: {error:?}")));
                publish(record);
                forget(&run_id);
            }
        }
    }
    let still_live = RUNS
        .lock()
        .unwrap()
        .as_mut()
        .map(|runs| {
            runs.retain_recent(config.keep_runs);
            runs.live_ids()
        })
        .unwrap_or_default();
    if still_live.is_empty() {
        return Ok(());
    }
    arm(now_ms.saturating_add(config.poll_ms))
}

/// The child's environment: one variable per requested secret, resolved
/// from the keystore at spawn time. Only NAMES cross a request, a
/// profile, or this repo; a value exists here for exactly as long as the
/// spawn call and never reaches a log, an answer, or the bus.
fn resolve_secrets(request: &RunRequest) -> Result<Vec<(String, String)>, (ErrorCode, String)> {
    let mut env = Vec::new();
    for (variable, reference) in &request.secrets {
        let key = &reference.secret;
        match keystore::get(key) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(value) => env.push((variable.clone(), value)),
                Err(_) => {
                    return Err((
                        ErrorCode::Invalid,
                        format!("secret {key:?} is not UTF-8 and cannot be an environment value"),
                    ))
                }
            },
            // A grant or scope refusal is the kernel saying no.
            Err(KeystoreError::Denied(detail)) => {
                return Err((
                    ErrorCode::Refused,
                    format!("keystore denied {key:?}: {detail}"),
                ))
            }
            // The request named a key this host does not hold: the
            // REQUEST is wrong, not the environment.
            Err(KeystoreError::NotFound) => {
                return Err((ErrorCode::Invalid, format!("no such secret {key:?}")))
            }
            Err(other) => return Err((ErrorCode::Failed, format!("keystore {key:?}: {other:?}"))),
        }
    }
    Ok(env)
}

fn op_describe(config: &Config) -> Answer {
    Answer::ok(Description {
        api_version: API_VERSION.to_owned(),
        engine: config.engine.clone(),
        provider: PROVIDER.to_owned(),
        models: config.models.clone(),
        default_model: config.default_model.clone(),
        capabilities: Capabilities {
            // Codex reports ONE completed `agent_message`, never token
            // deltas: this provider emits no `delta` and says so.
            streaming: false,
            // `item.started` / `item.completed` of a tool item — captured
            // from a live run, not assumed.
            tool_calls: true,
            cancel: true,
            usage: true,
            external_cli: true,
        },
        extra: Extensions::new(),
    })
}

fn op_run(config: &Config, payload: &[u8]) -> Answer {
    let request: RunRequest = match serde_json::from_slice(payload) {
        Ok(request) => request,
        Err(error) => {
            return Answer::error(EngineError::new(
                ErrorCode::Invalid,
                format!("malformed run request: {error}"),
            ))
        }
    };
    if request.engine != config.engine {
        return Answer::error(EngineError::new(
            ErrorCode::NotFound,
            format!(
                "this provider serves engine {:?}, not {:?}",
                config.engine, request.engine
            ),
        ));
    }
    let now_ms = match clock::now() {
        Ok(now_ms) => now_ms,
        Err(error) => {
            return Answer::error(EngineError::new(
                ErrorCode::Failed,
                format!("clock: {error:?}"),
            ))
        }
    };
    let accepted = match RUNS.lock().unwrap().as_mut() {
        Some(runs) => runs.accept(&request, now_ms),
        None => {
            return Answer::error(EngineError::new(
                ErrorCode::Failed,
                "the provider is not activated".to_owned(),
            ))
        }
    };
    let run_id = accepted.run_id.clone();

    let env = match resolve_secrets(&request) {
        Ok(env) => env,
        Err((code, message)) => return fail(&run_id, code, message),
    };
    let model = request
        .model
        .clone()
        .or_else(|| config.default_model.clone());
    let args = argv(model.as_deref(), &request.tools);

    let handle = match process::spawn(&config.command, &args, request.cwd.as_deref(), &env) {
        Ok(handle) => handle,
        // The kernel refused the executable, the cwd, or the env policy.
        Err(ProcessError::Denied(detail)) => {
            return fail(
                &run_id,
                ErrorCode::Refused,
                format!("spawn denied: {detail}"),
            )
        }
        // No CLI on this host, or one that will not start: the honest
        // environment gate. A faked run would be worse than no run.
        Err(other) => {
            return fail(
                &run_id,
                ErrorCode::Unavailable,
                format!("the codex CLI could not be spawned: {other:?}"),
            )
        }
    };

    CHILDREN.lock().unwrap().insert(run_id.clone(), handle);
    DECODERS
        .lock()
        .unwrap()
        .insert(run_id.clone(), Decoder::new());
    PENDING
        .lock()
        .unwrap()
        .insert(run_id.clone(), request.prompt.into_bytes());
    if let Err(detail) = pump_stdin(&run_id, handle) {
        let _ = process::kill(handle, Signal::Terminate);
        return fail(&run_id, ErrorCode::Failed, detail);
    }

    record_deferred(&run_id, Event::Started { model });
    if let Err(fault) = arm(now_ms.saturating_add(config.poll_ms)) {
        let _ = process::kill(handle, Signal::Terminate);
        return fail(&run_id, ErrorCode::Failed, format!("{fault:?}"));
    }
    Answer::ok(accepted)
}

/// `run-get` and `cancel` share the one-field `{ "run-id": … }` request
/// (`jinn-engine/README.md` §Operations), so they share its type.
fn run_ref(payload: &[u8]) -> Result<String, Answer> {
    serde_json::from_slice::<CancelRequest>(payload)
        .map(|request| request.run_id)
        .map_err(|error| {
            Answer::error(EngineError::new(
                ErrorCode::Invalid,
                format!("malformed request: {error}"),
            ))
        })
}

fn op_run_get(payload: &[u8]) -> Answer {
    let run_id = match run_ref(payload) {
        Ok(run_id) => run_id,
        Err(answer) => return answer,
    };
    match record_of(&run_id) {
        Some(record) => Answer::ok(record),
        None => Answer::error(EngineError::new(
            ErrorCode::NotFound,
            format!("no such run {run_id:?}"),
        )),
    }
}

fn op_cancel(payload: &[u8]) -> Answer {
    let run_id = match run_ref(payload) {
        Ok(run_id) => run_id,
        Err(answer) => return answer,
    };
    let Some(record) = record_of(&run_id) else {
        return Answer::error(EngineError::new(
            ErrorCode::NotFound,
            format!("no such run {run_id:?}"),
        ));
    };
    if !record.state.is_terminal() {
        let handle = CHILDREN.lock().unwrap().get(&run_id).copied();
        match handle {
            Some(handle) => end_child(&run_id, handle, "cancel"),
            None => {
                record_and_emit(
                    &run_id,
                    Event::Cancelled {
                        reason: "cancel".to_owned(),
                    },
                );
                forget(&run_id);
            }
        }
    }
    // The record AFTER the cancellation: what the run actually is now.
    Answer::ok(record_of(&run_id).unwrap_or(record))
}

struct Provider;

impl Guest for Provider {
    fn activate(config: Vec<u8>) -> Result<(), GuestFault> {
        let config: Config = serde_json::from_slice(&config)
            .map_err(|error| GuestFault::Failed(format!("malformed config: {error}")))?;
        if config.engine.is_empty() || config.command.is_empty() {
            return Err(GuestFault::Failed(
                "config needs a non-empty `engine` and `command`".to_owned(),
            ));
        }
        let contract = engine_contract(&config.engine);
        *RUNS.lock().unwrap() = Some(Runs::new(&config.engine));
        CHILDREN.lock().unwrap().clear();
        DECODERS.lock().unwrap().clear();
        PENDING.lock().unwrap().clear();
        *CONFIG.lock().unwrap() = Some(config);
        effects::register("jinn-engine-codex on duty", EFFECT_TOKEN)
            .map_err(|error| GuestFault::Failed(format!("effect: {error:?}")))?;
        services::provide(&contract)
            .map_err(|error| GuestFault::Failed(format!("provide: {error:?}")))?;
        // No alarm here: an idle provider costs zero ledger rows. A run
        // arms the wake it needs and re-arms only while it is live.
        Ok(())
    }

    fn check(_consumer: u64) -> bool {
        true
    }

    fn undo(_token: u64) -> Result<(), GuestFault> {
        Ok(())
    }

    fn handle_event(token: u64, topic: String, payload: Vec<u8>) -> Result<Vec<u8>, GuestFault> {
        let instant: Option<[u8; 8]> = payload.as_slice().try_into().ok();
        let (Some(instant), true) = (instant, topic == WAKE_TOPIC && token == POLL_TOKEN) else {
            return Err(GuestFault::Failed(format!(
                "unexpected event {topic:?} (token {token}, {} bytes)",
                payload.len()
            )));
        };
        let config =
            config().ok_or_else(|| GuestFault::Failed("a wake before activation".to_owned()))?;
        // Anything minted inside a `run` call goes on the wire here, on
        // this provider's own fiber (see [`DEFERRED`]).
        flush_deferred();
        poll(u64::from_le_bytes(instant), &config)?;
        Ok(Vec::new())
    }

    fn handle_call(
        _caller: u64,
        _contract: String,
        operation: String,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, GuestFault> {
        let config = config().ok_or_else(|| GuestFault::Failed("not activated".to_owned()))?;
        let answer = match operation.as_str() {
            OP_DESCRIBE => op_describe(&config),
            OP_RUN => op_run(&config, &payload),
            OP_RUN_GET => op_run_get(&payload),
            OP_CANCEL => op_cancel(&payload),
            other => return Err(GuestFault::Failed(format!("unknown operation {other:?}"))),
        };
        Ok(answer.encode())
    }

    fn snapshot() -> Vec<u8> {
        // Nothing to hand off: the kernel kills a spawned child with the
        // incarnation, so a successor inherits no live run.
        Vec::new()
    }

    fn restore(_blob: Vec<u8>) -> Result<(), GuestFault> {
        Ok(())
    }
}

export!(Provider);
