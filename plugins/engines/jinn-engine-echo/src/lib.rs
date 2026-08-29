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
//! # Secrets
//!
//! A request's `secrets` are resolved through the granted `jinn:keystore`
//! exactly as a CLI provider resolves them at spawn time — which makes
//! this provider the seam's REFUSAL proof without a CLI: a denied prefix
//! or an absent key becomes a typed `Refused` / `Invalid` answer and a
//! `failed` run record. Only the resolved value's LENGTH is ever
//! reported (in the accept answer's `extra`); the value never leaves
//! [`resolve_secrets`].

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
use jinn::plugin::types::{DispatchMode, KernelError, Selector};
use jinn::plugin::{clock, effects, events, keystore, services};

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
    let mut events: Vec<Event> = chunks(&plan.reply, chunk_bytes)
        .into_iter()
        .map(|piece| Event::Delta {
            text: piece.to_owned(),
        })
        .collect();
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
            cost_micro_usd: 0,
        },
        truncated: plan.truncated,
    });
    record_and_emit(&plan.run_id, events);
    prune(keep_runs);
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
    let reply = echo_reply(config.reply.as_deref(), &request.prompt);
    // The registry does the output accounting (it marks the record
    // truncated); the cut is what actually goes on the bus.
    let spent = {
        let mut held = RUNS.lock().unwrap();
        held.as_mut()
            .expect("activate holds the registry")
            .read(&run_id, reply.len() as u64)
    };
    let (cut, cut_here) = budget_cut(&reply, request.budget.output_bytes);
    let plan = Plan {
        run_id: run_id.clone(),
        reply: cut.to_owned(),
        truncated: spent || cut_here,
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
            record_and_emit(
                &request.run_id,
                vec![Event::Cancelled {
                    reason: "cancel".to_owned(),
                }],
            );
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
                        external_cli: false,
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
