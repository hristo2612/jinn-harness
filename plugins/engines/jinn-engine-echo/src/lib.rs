//! The echo engine: a `jinn:engine.<id>` provider that answers from the
//! prompt itself. No CLI, no child, no network — so the seam's proofs
//! (accept → `Started` → `Delta`s → `TurnEnd` → `Exited` with usage;
//! `cancel`; `run-get`) run on any box, CI included, where no vendor CLI
//! is authenticated. `Capabilities::external_cli` is FALSE here, and that
//! is the honest declaration a consumer gates on.
//!
//! It exists for the seam's EXTENSION proof: a third engine joins a live
//! composition through a profile entry and a grant alone — no new
//! contract, no edit to `jinn-engine`, nothing on the host.
//!
//! # The sync/async decision (`delay-ms`)
//!
//! The answer is produced SYNCHRONOUSLY inside `run` when `delay-ms` is 0
//! and ASYNCHRONOUSLY across a `jinn:clock` `alarm-at` wake when it is
//! positive. One knob, both shapes, because the seam needs both proofs:
//!
//! - `delay-ms: 0` — the whole run settles inside one `run` call, so a
//!   proof is deterministic with no wall-clock wait and no wake to
//!   schedule. This is the CI shape.
//! - `delay-ms > 0` — the run is genuinely LIVE between the accept and
//!   the wake, so `cancel` has something to kill and `run-get` has a
//!   `running` record to answer. This is the lifecycle shape, and it is
//!   also the shape the CLI providers have by construction.
//!
//! Either way `run` answers a [`jinn_engine::RunAccepted`] at once and never blocks on
//! the answer, and the run's events reach the bus in `seq` order — the
//! records are minted under one lock and then emitted in that order.
//!
//! **The synchronous shape has one hazard, and it is structural** (see
//! `FINDINGS.md` #4, nested dispatch): emitting inside `run` delivers to
//! every listener of [`EVENT_TOPIC`] while the CALLER is parked in its own
//! `services::call`. A consumer that both calls `run` and listens on the
//! topic — `jinn-engine-probe` is exactly that — would have the delivery
//! park on its own busy supervisor until the guest deadline. Such a
//! consumer must be composed against an echo entry with `delay-ms > 0`,
//! where the wake lands after `run` has already answered. `delay-ms: 0`
//! is for a driver that does not listen: a host tool, or a consumer that
//! reads the outcome with `run-get`.
//!
//! # Usage is byte counts, never a fabricated token count
//!
//! An echo costs no tokens and calls no model, so [`Usage::input_tokens`]
//! and [`Usage::output_tokens`] carry the BYTE LENGTHS of the prompt and
//! of the emitted reply, and `cost-micro-usd` is 0. They stand in for
//! tokens so the seam's usage path is exercised end to end; they are not
//! a token estimate and must never be read as one.
//!
//! # The spawning shape (`command`)
//!
//! With `command` set the provider does NOT answer from the prompt: it
//! spawns that absolute path through `jinn:process` and the child's
//! stdout is the answer. That is not a second personality, it is the one
//! thing the echo shape cannot prove — a REAL child. It makes this
//! provider the seam's process-lifecycle witness on a box with no vendor
//! CLI, and therefore in CI and in an independent verification that
//! (rightly) refuses to spend a metered vendor fixture:
//!
//! - `cancel` and a suspend have a live pid in the host's process table
//!   to kill, so "the child is dead" is checked rather than asserted.
//! - An executable outside the entry's `jinn:process` exec allowlist is
//!   a REAL kernel refusal on the record, not a simulated one.
//! - The child's explicit environment is deliberately EMPTY, so whatever
//!   it can see arrived through the grant's env allowlist and nothing
//!   else — an env leak is observable, not argued.
//!
//! The prompt is not written to such a child (its stdin is closed at
//! once): these children are witnesses, not engines.
//!
//! # Secrets
//!
//! A request's `secrets` are resolved through the granted `jinn:keystore`
//! exactly as a CLI provider resolves them at spawn time — which makes
//! this provider the seam's REFUSAL proof without a CLI: a denied prefix
//! or an absent key becomes a typed `Refused` / `Invalid` answer and a
//! `failed` run record. Only the resolved value's LENGTH is ever
//! reported (in the accept answer's `extra`); the value never leaves
//! [`resolve_secrets`].

use std::collections::BTreeMap;
use std::sync::Mutex;

use jinn_engine::{
    engine_contract, Answer, CancelRequest, Capabilities, Description, EngineError, ErrorCode,
    Event, Extensions, RunEvent, RunRequest, Runs, Usage, API_VERSION, EVENT_TOPIC, OP_CANCEL,
    OP_DESCRIBE, OP_RUN, OP_RUN_GET,
};
use serde::Deserialize;

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::process::{ChildStream, ProcessError, ReadResult, Signal, WaitResult};
use jinn::plugin::types::{DispatchMode, KernelError, Selector};
use jinn::plugin::{clock, effects, events, keystore, process, services};

/// Bytes per `read` call while draining a spawned child.
const READ_CHUNK: u32 = 8_192;

/// This package, as `describe` names it for an operator reading a swap.
const PROVIDER: &str = "engines/jinn-engine-echo";
const EFFECT_TOKEN: u64 = 1;
/// The token every deferred finish is scheduled under; a wake carrying
/// any other token is not ours.
const ALARM_TOKEN: u64 = 2;
/// The kernel's alarm wake topic, bound from `kernel-pin/wit/plugin.wit`
/// (`jinn:clock`), whose declaration is its one home.
const WAKE_TOPIC: &str = "jinn:clock/alarm";

/// This provider's entry config.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct EchoConfig {
    /// The engine id served — the second half of the contract name, and
    /// the only place the id lives (the definition's rule).
    engine: String,
    /// A fixed reply. Absent, the reply is the prompt echoed back.
    #[serde(default)]
    reply: Option<String>,
    /// 0 settles the run inside `run`; positive defers it to a wake (see
    /// the module doc's sync/async decision).
    #[serde(default)]
    delay_ms: u64,
    /// Finished records kept before the oldest are dropped (R9).
    #[serde(default = "default_keep_runs")]
    keep_runs: usize,
    /// What `describe` advertises.
    #[serde(default = "default_models")]
    models: Vec<String>,
    /// The model a request that names none runs under. Absent, the first
    /// advertised model — the profile writes both from one source, and
    /// this honours the one it wrote.
    #[serde(default)]
    default_model: Option<String>,
    /// Bytes per `Delta` — the streaming grain, so a proof can watch more
    /// than one chunk cross the bus.
    #[serde(default = "default_chunk_bytes")]
    chunk_bytes: usize,
    /// The absolute path of a child to spawn instead of answering from
    /// the prompt — the spawning shape (see the module doc). Absent, this
    /// provider touches no host process at all.
    #[serde(default)]
    command: Option<String>,
    /// That child's argv.
    #[serde(default)]
    args: Vec<String>,
    /// How often a spawned child is drained and waited on. Unused by the
    /// answering shape, which has no child to poll.
    #[serde(default = "default_poll_ms")]
    poll_ms: u64,
}

fn default_poll_ms() -> u64 {
    250
}

fn default_keep_runs() -> usize {
    8
}

fn default_models() -> Vec<String> {
    vec!["echo-1".to_owned()]
}

fn default_chunk_bytes() -> usize {
    64
}

/// One accepted run's settled plan: what it will answer and when. Held
/// only while a run is deferred; the synchronous shape builds one and
/// consumes it in the same call.
#[derive(Clone, Debug)]
struct Plan {
    run_id: String,
    /// The reply the output budget admits, already cut.
    reply: String,
    /// Whether the budget cut it.
    truncated: bool,
    /// The typed cut the registry answered, replayed onto the run so a
    /// listener sees it (the definition's [`Event::Truncated`]).
    truncation: Option<Event>,
    /// The prompt's byte length — the input half of [`Usage`].
    prompt_bytes: u64,
    /// The `now` the finish is due at.
    due_ms: u64,
}

static CONFIG: Mutex<Option<EchoConfig>> = Mutex::new(None);
static RUNS: Mutex<Option<Runs>> = Mutex::new(None);
static PENDING: Mutex<Vec<Plan>> = Mutex::new(Vec::new());
/// Refused emits since activation. A refused emit never fails a run — the
/// run record still carries every event, and `run-get` is the recovery
/// lane — but it is never silent either: `describe` reports the count.
static EMIT_FAILURES: Mutex<u64> = Mutex::new(0);

/// Bus records minted inside a `run` call and held until this provider's
/// OWN fiber wakes. A provider must never emit from inside a handler the
/// caller is parked in: if the caller listens on this topic — the probe
/// consumer does — the delivery parks on ITS busy supervisor and the
/// whole call chain waits out the guest deadline (FINDINGS.md #4, nested
/// dispatch). The caller's own instance is then destroyed by that fault,
/// which is how one emit takes down a consumer permanently.
static DEFERRED: Mutex<Vec<RunEvent>> = Mutex::new(Vec::new());
/// The live children of the spawning shape, by run id. A spawned child is
/// a KERNEL registration: the kernel kills it on suspend and on dispose,
/// so nothing here outlives an incarnation and `activate` starts empty.
static CHILDREN: Mutex<BTreeMap<String, Child>> = Mutex::new(BTreeMap::new());

/// One spawned child and what its run has cost so far.
#[derive(Clone, Debug)]
struct Child {
    handle: u64,
    prompt_bytes: u64,
    read_bytes: u64,
}

fn fault(context: &str, error: KernelError) -> GuestFault {
    GuestFault::Failed(format!("{context}: {error:?}"))
}

fn config() -> EchoConfig {
    CONFIG
        .lock()
        .unwrap()
        .clone()
        .expect("activate holds the config")
}

/// The model a request that names none runs under: the configured one,
/// else the first advertised.
fn default_model(config: &EchoConfig) -> Option<String> {
    config
        .default_model
        .clone()
        .or_else(|| config.models.first().cloned())
}

/// The reply one echo run answers: the configured fixed reply when there
/// is one, otherwise the prompt behind a marker. Pure — the whole of the
/// provider's "model".
fn echo_reply(fixed: Option<&str>, prompt: &str) -> String {
    match fixed {
        Some(reply) => reply.to_owned(),
        None => format!("echo: {prompt}"),
    }
}

/// `text` split into pieces of about `max` bytes, never mid-character (a
/// piece rounds UP to the next character boundary rather than splitting a
/// code point). Empty text yields no pieces — an empty answer streams
/// nothing, which is honest, not a missing event.
fn chunks(text: &str, max: usize) -> Vec<&str> {
    let max = max.max(1);
    let mut pieces = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = (start + max).min(text.len());
        while end < text.len() && !text.is_char_boundary(end) {
            end += 1;
        }
        pieces.push(&text[start..end]);
        start = end;
    }
    pieces
}

/// The prefix of `reply` the output budget admits, and whether anything
/// was cut. Character-safe: the cut walks back to a boundary.
fn budget_cut(reply: &str, output_bytes: u64) -> (&str, bool) {
    let cap = usize::try_from(output_bytes).unwrap_or(usize::MAX);
    if reply.len() <= cap {
        return (reply, false);
    }
    let mut end = cap;
    while end > 0 && !reply.is_char_boundary(end) {
        end -= 1;
    }
    (&reply[..end], true)
}

/// Puts one bus record on the wire. A refusal is counted, never fatal.
fn emit(record: &RunEvent) {
    let payload = serde_json::to_vec(record).expect("a run event encodes");
    if events::emit(EVENT_TOPIC, DispatchMode::Emit, &Selector::All, &payload).is_err() {
        *EMIT_FAILURES.lock().unwrap() += 1;
    }
}

/// Records `events` against the run and emits the bus records IN
/// SEQUENCE. Minting happens under one lock and emitting after it: the
/// order on the bus is the order of `seq`, and no host call is made while
/// the registry is held.
fn record_and_emit(run_id: &str, events: Vec<Event>) {
    let records: Vec<RunEvent> = {
        let mut held = RUNS.lock().unwrap();
        let runs = held.as_mut().expect("activate holds the registry");
        events
            .into_iter()
            .filter_map(|event| runs.record(run_id, event))
            .collect()
    };
    for record in &records {
        emit(record);
    }
}

/// Records `events` and holds their bus records for this provider's next
/// wake — see [`DEFERRED`]. Same minting discipline as
/// [`record_and_emit`]; only the emit moves off the caller's stack.
fn record_deferred(run_id: &str, events: Vec<Event>) {
    let records: Vec<RunEvent> = {
        let mut held = RUNS.lock().unwrap();
        let runs = held.as_mut().expect("activate holds the registry");
        events
            .into_iter()
            .filter_map(|event| runs.record(run_id, event))
            .collect()
    };
    DEFERRED.lock().unwrap().extend(records);
}

/// Puts everything held for this fiber on the wire, in sequence. Called
/// FIRST on every wake, so a run's `started` always precedes its rest.
fn flush_deferred() {
    let records = std::mem::take(&mut *DEFERRED.lock().unwrap());
    for record in &records {
        emit(record);
    }
}

/// Marks a run failed before it ever answered and emits the reason.
fn fail_and_emit(run_id: &str, reason: &str) {
    let record = {
        let mut held = RUNS.lock().unwrap();
        held.as_mut()
            .expect("activate holds the registry")
            .fail(run_id, reason)
    };
    if let Some(record) = record {
        emit(&record);
    }
}

/// Drops the oldest finished records once the configured window is full.
fn prune(keep_runs: usize) {
    RUNS.lock()
        .unwrap()
        .as_mut()
        .expect("activate holds the registry")
        .retain_recent(keep_runs);
}

/// A run's state, or `None` when the registry no longer holds it.
fn is_terminal(run_id: &str) -> Option<bool> {
    RUNS.lock()
        .unwrap()
        .as_ref()
        .expect("activate holds the registry")
        .get(run_id)
        .map(|record| record.state.is_terminal())
}

/// The record as `run-get` answers it.
fn record_json(run_id: &str) -> Option<serde_json::Value> {
    RUNS.lock()
        .unwrap()
        .as_ref()
        .expect("activate holds the registry")
        .get(run_id)
        .map(|record| serde_json::to_value(record).expect("a run record encodes"))
}

/// Resolves the request's secret REFERENCES through the granted
/// `jinn:keystore`, the way a CLI provider resolves them at spawn time,
/// and answers each child variable's resolved BYTE LENGTH. The value
/// itself never leaves this function — not into the answer, not into an
/// error, not into an event.
fn resolve_secrets(
    request: &RunRequest,
) -> Result<serde_json::Map<String, serde_json::Value>, EngineError> {
    let mut lengths = serde_json::Map::new();
    for (name, reference) in &request.secrets {
        let key = reference.secret.as_str();
        match keystore::get(key) {
            Ok(value) => {
                lengths.insert(name.clone(), serde_json::json!(value.len()));
            }
            // A denied prefix or an attenuated grant: the kernel refused,
            // and the refusal is already a ledger event.
            Err(keystore::KeystoreError::Denied(detail)) => {
                return Err(EngineError::new(
                    ErrorCode::Refused,
                    format!("secret {name}: key {key:?} denied ({detail})"),
                ));
            }
            // An absent key is a malformed REQUEST, not a missing run:
            // `NotFound` in this seam means no such run or engine.
            Err(keystore::KeystoreError::NotFound) => {
                return Err(EngineError::new(
                    ErrorCode::Invalid,
                    format!("secret {name}: key {key:?} is not in the keystore"),
                ));
            }
            Err(keystore::KeystoreError::Invalid(detail)) => {
                return Err(EngineError::new(
                    ErrorCode::Invalid,
                    format!("secret {name}: key {key:?} is malformed ({detail})"),
                ));
            }
            Err(keystore::KeystoreError::Failed(detail)) => {
                return Err(EngineError::new(
                    ErrorCode::Failed,
                    format!("secret {name}: keystore failed ({detail})"),
                ));
            }
        }
    }
    Ok(lengths)
}

/// Settles one planned run: the deltas, the turn end, and the exit with
/// its usage.
fn finish(plan: &Plan, chunk_bytes: usize, keep_runs: usize) {
    // The cut goes on the wire FIRST and typed: a consumer sees events,
    // and an answer that simply stops reads as a whole one.
    let mut events: Vec<Event> = plan.truncation.clone().into_iter().collect();
    events.extend(chunks(&plan.reply, chunk_bytes)
        .into_iter()
        .map(|piece| Event::Delta {
            text: piece.to_owned(),
        }));
    events.push(Event::TurnEnd { text: None });
    events.push(Event::Exited {
        status: 0,
        // An echo has no engine to report a failed turn.
        error: None,
        // Byte counts standing in for tokens — see the module doc. An
        // echo runs no model, so a token count here would be invented.
        usage: Usage {
            input_tokens: plan.prompt_bytes,
            output_tokens: plan.reply.len() as u64,
            ..Usage::default()
        },
        truncated: plan.truncated,
    });
    record_and_emit(&plan.run_id, events);
    prune(keep_runs);
}

/// Forgets a child without killing it — used once the kernel or a kill
/// has already ended it.
fn forget_child(run_id: &str) {
    CHILDREN.lock().unwrap().remove(run_id);
}

/// Kills a live child and records why the run ended. The kill is the
/// kernel's (`jinn:process`), so the inverse obligation — SIGKILL and
/// reap, no process of that handle left in the table — is the kernel's
/// too.
fn kill_and_cancel(run_id: &str, reason: &str) {
    if let Some(child) = CHILDREN.lock().unwrap().remove(run_id) {
        let _ = process::kill(child.handle, Signal::Terminate);
    }
    record_and_emit(
        run_id,
        vec![Event::Cancelled {
            reason: reason.to_owned(),
        }],
    );
}

/// Spawns the configured child for `run_id` and puts the run live. A
/// refused spawn is the KERNEL's refusal surfaced typed (`Refused`) and a
/// failed record — never a run that quietly did not happen.
fn spawn_run(
    config: &EchoConfig,
    run_id: &str,
    request: &RunRequest,
    now_ms: u64,
) -> Result<(), Answer> {
    let command = config.command.as_deref().unwrap_or_default();
    // No explicit environment, ever: what the child can see is exactly
    // what the entry's env policy admits, so an env leak is a fact about
    // the grant and not about this code.
    let handle = match process::spawn(command, &config.args, request.cwd.as_deref(), &[]) {
        Ok(handle) => handle,
        Err(ProcessError::Denied(detail)) => {
            fail_and_emit(run_id, "spawn denied");
            prune(config.keep_runs);
            return Err(Answer::error(EngineError::new(
                ErrorCode::Refused,
                format!("spawn of {command:?} denied: {detail}"),
            )));
        }
        Err(error) => {
            fail_and_emit(run_id, "spawn failed");
            prune(config.keep_runs);
            return Err(Answer::error(EngineError::unavailable(format!(
                "{command:?} cannot run here: {error:?}"
            ))));
        }
    };
    // A witness child is not an engine: it is told nothing and its stdin
    // ends at once, so it never blocks on a prompt that is not coming.
    let _ = process::close_stdin(handle);
    CHILDREN.lock().unwrap().insert(
        run_id.to_owned(),
        Child {
            handle,
            prompt_bytes: request.prompt.len() as u64,
            read_bytes: 0,
        },
    );
    let model = request.model.clone().or_else(|| default_model(config));
    record_deferred(run_id, vec![Event::Started { model }]);
    if let Err(error) = clock::alarm_at(now_ms.saturating_add(config.poll_ms), ALARM_TOKEN) {
        kill_and_cancel(run_id, &format!("no poll alarm: {error:?}"));
        prune(config.keep_runs);
        return Err(Answer::error(EngineError::new(
            ErrorCode::Failed,
            format!("could not schedule the run's poll: {error:?}"),
        )));
    }
    Ok(())
}

/// Everything the child has to say right now, and its byte count.
fn drain_child(handle: u64) -> (String, u64) {
    let mut bytes = Vec::new();
    while let Ok(ReadResult::Data(chunk)) = process::read(handle, ChildStream::Stdout, READ_CHUNK) {
        bytes.extend_from_slice(&chunk);
    }
    // stderr is drained and dropped: the host's per-stream buffer is
    // BOUNDED, so a stream nobody reads backpressures the child into a
    // stall. The seam has no event for a diagnostic line.
    while let Ok(ReadResult::Data(_)) = process::read(handle, ChildStream::Stderr, READ_CHUNK) {}
    let read = bytes.len() as u64;
    (String::from_utf8_lossy(&bytes).into_owned(), read)
}

/// One poll of every live child: drain, charge the output budget, and
/// settle the ones that have exited. Answers whether any child is still
/// running (so the wake re-arms).
fn poll_children(config: &EchoConfig, now_ms: u64) -> bool {
    let live: Vec<(String, Child)> = CHILDREN
        .lock()
        .unwrap()
        .iter()
        .map(|(run_id, child)| (run_id.clone(), child.clone()))
        .collect();
    let mut running = false;
    for (run_id, child) in live {
        // Cancelled between the accept and this wake.
        if is_terminal(&run_id) != Some(false) {
            forget_child(&run_id);
            continue;
        }
        let (text, read) = drain_child(child.handle);
        if !text.is_empty() {
            record_and_emit(&run_id, vec![Event::Delta { text }]);
        }
        let cut = {
            let mut held = RUNS.lock().unwrap();
            held.as_mut()
                .expect("activate holds the registry")
                .read(&run_id, read)
        };
        CHILDREN
            .lock()
            .unwrap()
            .entry(run_id.clone())
            .and_modify(|held| held.read_bytes = held.read_bytes.saturating_add(read));
        if let Some(cut) = cut {
            // The cut on the wire FIRST, then the end it caused.
            record_and_emit(&run_id, vec![cut]);
            kill_and_cancel(&run_id, "budget");
            prune(config.keep_runs);
            continue;
        }
        let over_budget = RUNS
            .lock()
            .unwrap()
            .as_ref()
            .expect("activate holds the registry")
            .over_wall_budget(&run_id, now_ms);
        if over_budget {
            kill_and_cancel(&run_id, "budget");
            prune(config.keep_runs);
            continue;
        }
        match process::wait(child.handle, 0) {
            // The exit is the PROCESS's, never the stream's guess.
            Ok(WaitResult::Exited(status)) => {
                let (tail, tail_read) = drain_child(child.handle);
                if !tail.is_empty() {
                    record_and_emit(&run_id, vec![Event::Delta { text: tail }]);
                }
                let output = child.read_bytes.saturating_add(read).saturating_add(tail_read);
                let truncated = RUNS
                    .lock()
                    .unwrap()
                    .as_ref()
                    .and_then(|runs| runs.get(&run_id).map(|record| record.truncated))
                    .unwrap_or_default();
                record_and_emit(
                    &run_id,
                    vec![
                        Event::TurnEnd { text: None },
                        Event::Exited {
                            status,
                            // Byte counts standing in for tokens, as the
                            // answering shape does — never a token guess.
                            usage: Usage {
                                input_tokens: child.prompt_bytes,
                                output_tokens: output,
                                ..Usage::default()
                            },
                            truncated,
                            error: None,
                        },
                    ],
                );
                forget_child(&run_id);
                prune(config.keep_runs);
            }
            Ok(WaitResult::Running) => running = true,
            // The handle is gone: the kernel reaped the child (a suspend,
            // a dispose). Honest, not a success.
            Err(error) => {
                kill_and_cancel(&run_id, &format!("child lost: {error:?}"));
                prune(config.keep_runs);
            }
        }
    }
    running
}

/// `run`: accept at once, then produce the answer here or at a wake.
fn on_run(payload: &[u8]) -> Answer {
    let request: RunRequest = match serde_json::from_slice(payload) {
        Ok(request) => request,
        Err(error) => {
            return Answer::error(EngineError::new(
                ErrorCode::Invalid,
                format!("malformed run request: {error}"),
            ))
        }
    };
    let config = config();
    // The route is the contract name; a request for another engine landed
    // on the wrong provider.
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
                format!("clock now: {error:?}"),
            ))
        }
    };
    let mut accepted = {
        let mut held = RUNS.lock().unwrap();
        held.as_mut()
            .expect("activate holds the registry")
            .accept(&request, now_ms)
    };
    let run_id = accepted.run_id.clone();
    // Secrets before the answer, exactly where a CLI provider resolves
    // them: a refusal is a typed answer AND a failed record, and the
    // caller can still read it — the run id rides in the error's extra.
    let secrets = match resolve_secrets(&request) {
        Ok(lengths) => lengths,
        Err(mut error) => {
            fail_and_emit(&run_id, "secrets");
            prune(config.keep_runs);
            error
                .extra
                .insert("run-id".to_owned(), serde_json::json!(run_id));
            return Answer::error(error);
        }
    };
    // The spawning shape answers from a REAL child; the answering shape
    // from the prompt. One `run`, one accept, two ways to produce it.
    if config.command.is_some() {
        if let Err(answer) = spawn_run(&config, &run_id, &request, now_ms) {
            return answer;
        }
        if !secrets.is_empty() {
            accepted
                .extra
                .insert("secrets".to_owned(), serde_json::Value::Object(secrets));
        }
        return Answer::ok(accepted);
    }
    let reply = echo_reply(config.reply.as_deref(), &request.prompt);
    // The registry does the output accounting (it marks the record
    // truncated); the cut is what actually goes on the bus.
    let truncation = {
        let mut held = RUNS.lock().unwrap();
        held.as_mut()
            .expect("activate holds the registry")
            .read(&run_id, reply.len() as u64)
    };
    let (cut, cut_here) = budget_cut(&reply, request.budget.output_bytes);
    let plan = Plan {
        run_id: run_id.clone(),
        reply: cut.to_owned(),
        truncated: truncation.is_some() || cut_here,
        truncation,
        prompt_bytes: request.prompt.len() as u64,
        due_ms: now_ms.saturating_add(config.delay_ms),
    };
    let model = accepted.model.clone().or_else(|| default_model(&config));
    if config.delay_ms == 0 {
        // The synchronous shape emits from inside the call by
        // construction; it is only safe for a caller that does NOT listen
        // on the topic (see [`DEFERRED`], and this module's doc).
        record_and_emit(&run_id, vec![Event::Started { model }]);
        finish(&plan, config.chunk_bytes, config.keep_runs);
    } else {
        record_deferred(&run_id, vec![Event::Started { model }]);
        // The run stays live until the wake: `cancel` has something to
        // kill and `run-get` answers `running`.
        PENDING.lock().unwrap().push(plan);
        if let Err(error) = clock::alarm_at(now_ms.saturating_add(config.delay_ms), ALARM_TOKEN) {
            PENDING.lock().unwrap().retain(|held| held.run_id != run_id);
            fail_and_emit(&run_id, &format!("alarm refused: {error:?}"));
            prune(config.keep_runs);
            return Answer::error(EngineError::new(
                ErrorCode::Failed,
                format!("could not schedule the run's finish: {error:?}"),
            ));
        }
    }
    if !secrets.is_empty() {
        // LENGTHS only — the resolution happened, the values did not
        // travel.
        accepted
            .extra
            .insert("secrets".to_owned(), serde_json::Value::Object(secrets));
    }
    Answer::ok(accepted)
}

/// `cancel`: a live run is cancelled and said so on the bus; a finished
/// one answers its record unchanged (a terminal run is never re-labelled).
fn on_cancel(payload: &[u8]) -> Answer {
    let request: CancelRequest = match serde_json::from_slice(payload) {
        Ok(request) => request,
        Err(error) => {
            return Answer::error(EngineError::new(
                ErrorCode::Invalid,
                format!("malformed cancel request: {error}"),
            ))
        }
    };
    match is_terminal(&request.run_id) {
        None => Answer::error(EngineError::new(
            ErrorCode::NotFound,
            format!("no run {:?}", request.run_id),
        )),
        Some(true) => Answer::ok(record_json(&request.run_id)),
        Some(false) => {
            PENDING
                .lock()
                .unwrap()
                .retain(|plan| plan.run_id != request.run_id);
            // The spawning shape has a child to kill, and the kernel's
            // kill obligation is SIGKILL AND REAP: after this, no process
            // of that handle exists in the host's process table.
            kill_and_cancel(&request.run_id, "cancel");
            let answer = Answer::ok(record_json(&request.run_id));
            prune(config().keep_runs);
            answer
        }
    }
}

struct Echo;

impl Guest for Echo {
    fn activate(config: Vec<u8>) -> Result<(), GuestFault> {
        let config: EchoConfig = serde_json::from_slice(&config)
            .map_err(|error| GuestFault::Failed(format!("malformed config: {error}")))?;
        if config.engine.is_empty() {
            return Err(GuestFault::Failed(
                "config.engine is the engine id this provider serves; it cannot be empty"
                    .to_owned(),
            ));
        }
        let contract = engine_contract(&config.engine);
        *RUNS.lock().unwrap() = Some(Runs::new(config.engine.clone()));
        *PENDING.lock().unwrap() = Vec::new();
        // A spawned child is a kernel registration: none survives an
        // incarnation, so a fresh one starts holding none.
        *CHILDREN.lock().unwrap() = BTreeMap::new();
        *EMIT_FAILURES.lock().unwrap() = 0;
        *CONFIG.lock().unwrap() = Some(config);
        effects::register("jinn-engine-echo on duty", EFFECT_TOKEN)
            .map_err(|error| fault("effect", error))?;
        // NO alarm here: a provider holds no schedule of its own. The only
        // alarms this guest ever requests are the per-run finishes of the
        // deferred shape, and each is requested from `run`.
        services::provide(&contract).map_err(|error| fault("provide", error))?;
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
        let (Some(instant), true, true) = (instant, topic == WAKE_TOPIC, token == ALARM_TOKEN)
        else {
            return Err(GuestFault::Failed(format!(
                "unexpected event {topic:?} (token {token}, {} bytes)",
                payload.len()
            )));
        };
        let now_ms = u64::from_le_bytes(instant);
        // Anything minted inside a `run` call goes on the wire here, on
        // this provider's own fiber (see [`DEFERRED`]).
        flush_deferred();
        let due = {
            let mut held = PENDING.lock().unwrap();
            let (due, waiting): (Vec<Plan>, Vec<Plan>) = std::mem::take(&mut *held)
                .into_iter()
                .partition(|plan| plan.due_ms <= now_ms);
            *held = waiting;
            due
        };
        let config = config();
        let mut settled = 0_u64;
        for plan in &due {
            // Cancelled between the accept and the wake, or already
            // dropped from the window: nothing to settle.
            if is_terminal(&plan.run_id) != Some(false) {
                continue;
            }
            let over_budget = RUNS
                .lock()
                .unwrap()
                .as_ref()
                .expect("activate holds the registry")
                .over_wall_budget(&plan.run_id, now_ms);
            if over_budget {
                // The wall bound is the provider's to enforce (R9): the
                // run ends `cancelled`, reason `budget`, never a late
                // success.
                record_and_emit(
                    &plan.run_id,
                    vec![Event::Cancelled {
                        reason: "budget".to_owned(),
                    }],
                );
                prune(config.keep_runs);
            } else {
                finish(plan, config.chunk_bytes, config.keep_runs);
            }
            settled += 1;
        }
        // The spawning shape's children are drained and waited on here,
        // on this provider's OWN fiber, and the wake re-arms while any is
        // still running.
        if poll_children(&config, now_ms) {
            if let Err(error) = clock::alarm_at(now_ms.saturating_add(config.poll_ms), ALARM_TOKEN) {
                return Err(GuestFault::Failed(format!("poll alarm: {error:?}")));
            }
        }
        Ok(
            serde_json::to_vec(&serde_json::json!({ "settled": settled }))
                .expect("the wake summary encodes"),
        )
    }

    fn handle_call(
        _caller: u64,
        _contract: String,
        operation: String,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, GuestFault> {
        let answer = match operation.as_str() {
            OP_DESCRIBE => {
                let config = config();
                let mut extra = Extensions::new();
                extra.insert(
                    "emit-failures".to_owned(),
                    serde_json::json!(*EMIT_FAILURES.lock().unwrap()),
                );
                extra.insert("delay-ms".to_owned(), serde_json::json!(config.delay_ms));
                Answer::ok(Description {
                    api_version: API_VERSION.to_owned(),
                    engine: config.engine.clone(),
                    provider: PROVIDER.to_owned(),
                    default_model: default_model(&config),
                    models: config.models.clone(),
                    capabilities: Capabilities {
                        streaming: true,
                        tool_calls: false,
                        cancel: true,
                        usage: true,
                        // The whole point: no CLI on the host, so a run
                        // is never environment-gated.
                        external_cli: config.command.is_some(),
                        ..Capabilities::default()
                    },
                    extra,
                })
            }
            OP_RUN => on_run(&payload),
            // The definition offers exactly one `{run-id}` request shape,
            // and `run-get` and `cancel` both want it.
            OP_RUN_GET => match serde_json::from_slice::<CancelRequest>(&payload) {
                Ok(request) => match record_json(&request.run_id) {
                    Some(record) => Answer::ok(record),
                    None => Answer::error(EngineError::new(
                        ErrorCode::NotFound,
                        format!("no run {:?}", request.run_id),
                    )),
                },
                Err(error) => Answer::error(EngineError::new(
                    ErrorCode::Invalid,
                    format!("malformed run-get request: {error}"),
                )),
            },
            OP_CANCEL => on_cancel(&payload),
            other => Answer::error(EngineError::new(
                ErrorCode::Invalid,
                format!("unknown operation {other:?}"),
            )),
        };
        Ok(answer.encode())
    }

    fn snapshot() -> Vec<u8> {
        // A run record is per incarnation by the definition's own rule
        // (run ids mint from 0 again after a restart), so a successor
        // starts with an empty registry rather than inheriting ids it
        // would re-mint.
        Vec::new()
    }

    fn restore(_blob: Vec<u8>) -> Result<(), GuestFault> {
        Ok(())
    }
}

export!(Echo);
