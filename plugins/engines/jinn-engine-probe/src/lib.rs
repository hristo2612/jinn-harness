//! The engines seam's real-duty CONSUMER: every `every-ms` it runs one
//! short prompt through a CONFIGURED engine and records what came back.
//!
//! It is deliberately ignorant of providers. It holds an engine ID, and
//! the id IS the contract name ([`engine_contract`]) — so switching the
//! implementation behind that id, or pointing this entry at a different
//! id, is a profile edit and nothing else. It never learns whether the
//! engine is a CLI, a self-contained guest, or something not written yet.
//!
//! # Two wakes, two jobs
//!
//! - A `jinn:clock` alarm is the SCHEDULE. A consumer with a period is
//!   exactly where `alarm-every` belongs (a provider, which only reacts,
//!   holds no schedule of its own). Its first wake is one full period out
//!   (`FINDINGS.md` #13), so activation also asks for a one-shot at `now`
//!   and the first probe lands immediately.
//! - [`EVENT_TOPIC`] is the RESULT. One listener for the whole seam: every
//!   provider publishes there, and this guest routes on the event's own
//!   `engine` and `run-id` rather than on a topic per engine.
//!
//! Nothing is called from `activate` — not the engine, not the clock
//! beyond its own alarms. An activation that called a sibling is the
//! nested-dispatch deadlock (`FINDINGS.md` #4, #26); the first tick wake
//! is the earliest safe moment.
//!
//! # A missing provider is an OUTCOME
//!
//! No engine mounted, a refused resolve, a typed `Unavailable` from a
//! provider whose CLI is absent: each is a recorded outcome and the probe
//! moves on. The seam's honesty is the point of the probe — it never
//! faults its fiber over an engine that is not there, and it never fakes
//! a run.
//!
//! # What the record proves
//!
//! Besides the run itself, the record answers the seam's ORDERING
//! question: the events of one run arrive with `seq` from 0 and no gaps.
//! The probe keeps the last `seq` it saw per run and reports `order-ok`
//! plus a count of deviations, so a reordering or a dropped delivery is
//! visible in the artifact rather than inferred from a log.
//!
//! The answer text is BOUNDED at `max-answer-bytes` (R9: a long answer
//! can never grow the file without limit). The cut is STATED —
//! `truncated: true` beside `text-bytes`, the full byte length the run
//! actually produced — never a silent shortening.

use std::sync::Mutex;

use jinn_engine::{
    engine_contract, Answer, Budget, Event, RunAccepted, RunEvent, RunRequest, RunState, Usage,
    API_VERSION, EVENT_TOPIC, OP_RUN,
};
use serde::Deserialize;

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::types::KernelError;
use jinn::plugin::{clock, effects, events, fs, services};

const EFFECT_TOKEN: u64 = 1;
/// The schedule's wakes (both the one-shot first probe and the period).
const TICK_TOKEN: u64 = 2;
/// The seam's event listener.
const EVENT_TOKEN: u64 = 3;
/// The kernel's alarm wake topic, bound from `kernel-pin/wit/plugin.wit`
/// (`jinn:clock`), whose declaration is its one home.
const WAKE_TOPIC: &str = "jinn:clock/alarm";
/// No idempotency claim, and deliberately so — the discipline
/// `health-snapshot` states for its report writes. Each outcome is a new
/// effect by construction: a run is finalized exactly once (the in-flight
/// slot is cleared as it settles, so a repeated terminal event finds no
/// run), and a key built from the probe's own tick counter would not even
/// be unique across incarnations, since the counter restarts with the
/// fiber.
const NO_KEY: &str = "";

/// This consumer's entry config.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ProbeConfig {
    /// The engine id to route to — the contract name's second half.
    engine: String,
    /// The one-line prompt every probe run carries.
    prompt: String,
    /// The schedule.
    every_ms: u64,
    /// Report directory under the granted `jinn:fs` scope.
    #[serde(default = "default_dir")]
    dir: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    budget: Budget,
    /// The record's answer-text bound (R9).
    #[serde(default = "default_max_answer_bytes")]
    max_answer_bytes: usize,
}

fn default_dir() -> String {
    "engine-probe".to_owned()
}

fn default_max_answer_bytes() -> usize {
    512
}

/// The run this probe is waiting on, folded as its events arrive.
#[derive(Clone, Debug)]
struct InFlight {
    run_id: String,
    engine: String,
    model: Option<String>,
    /// The probe tick that started it, and the `now` it was accepted at.
    tick: u64,
    started_ms: u64,
    state: RunState,
    status: Option<i32>,
    usage: Usage,
    /// The answer, bounded at `max-answer-bytes`.
    text: String,
    /// The answer's FULL byte length, whether or not it was kept.
    text_bytes: u64,
    /// The probe cut the text to its bound.
    truncated: bool,
    /// The PROVIDER reported it cut the run on its output budget — a
    /// different fact, kept separately.
    provider_truncated: bool,
    /// The terminal detail: why a run was cancelled, or — on a run whose
    /// process exited cleanly — the failure the ENGINE itself reported
    /// (`Event::Exited { error }`). Either way it reaches the record's
    /// `detail`, so a clean status never reads as a success the engine
    /// did not give.
    reason: Option<String>,
    events: u64,
    /// The `seq` the next event of this run must carry.
    next_seq: u64,
    last_seq: Option<u64>,
    order_ok: bool,
    order_faults: u64,
}

impl InFlight {
    fn new(accepted: &RunAccepted, tick: u64, now_ms: u64) -> Self {
        Self {
            run_id: accepted.run_id.clone(),
            engine: accepted.engine.clone(),
            model: accepted.model.clone(),
            tick,
            started_ms: now_ms,
            state: RunState::Starting,
            status: None,
            usage: Usage::default(),
            text: String::new(),
            text_bytes: 0,
            truncated: false,
            provider_truncated: false,
            reason: None,
            events: 0,
            next_seq: 0,
            last_seq: None,
            order_ok: true,
            order_faults: 0,
        }
    }
}

static CONFIG: Mutex<Option<ProbeConfig>> = Mutex::new(None);
static CURRENT: Mutex<Option<InFlight>> = Mutex::new(None);
static TICKS: Mutex<u64> = Mutex::new(0);
/// Events on the shared topic this guest could not decode. The topic
/// carries every provider's traffic, so a payload a newer peer wrote is
/// not this guest's failure — it is counted, never fatal.
static MALFORMED: Mutex<u64> = Mutex::new(0);

fn fault(context: &str, error: KernelError) -> GuestFault {
    GuestFault::Failed(format!("{context}: {error:?}"))
}

fn fs_fault(context: &str, error: fs::FsError) -> GuestFault {
    GuestFault::Failed(format!("{context}: {error:?}"))
}

fn config() -> ProbeConfig {
    CONFIG
        .lock()
        .unwrap()
        .clone()
        .expect("activate holds the config")
}

/// Appends `text` to `buffer` up to `max` BYTES, never mid-character.
/// Answers whether anything was dropped — the caller states the cut in
/// the record rather than silently shortening the answer.
fn append_bounded(buffer: &mut String, text: &str, max: usize) -> bool {
    let room = max.saturating_sub(buffer.len());
    if text.len() <= room {
        buffer.push_str(text);
        return false;
    }
    let mut end = room;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    buffer.push_str(&text[..end]);
    true
}

/// One finished run as the record document.
fn run_record(run: &InFlight, now_ms: u64) -> serde_json::Value {
    serde_json::json!({
        "api-version": API_VERSION,
        "probe-tick": run.tick,
        "engine": run.engine,
        "outcome": match run.state {
            RunState::Exited => "exited",
            RunState::Cancelled => "cancelled",
            RunState::Failed => "failed",
            RunState::Starting | RunState::Running => "timed-out",
        },
        "detail": run.reason,
        "run-id": run.run_id,
        "model": run.model,
        "state": run.state,
        "status": run.status,
        "usage": run.usage,
        "text": run.text,
        "text-bytes": run.text_bytes,
        "truncated": run.truncated,
        "provider-truncated": run.provider_truncated,
        "wall-ms": now_ms.saturating_sub(run.started_ms),
        "events": run.events,
        "order-ok": run.order_ok,
        "order-faults": run.order_faults,
        "last-seq": run.last_seq,
        "malformed-events": *MALFORMED.lock().unwrap(),
    })
}

/// One tick that never became a run: the same document with the run half
/// empty, so `last.json` has exactly one shape whatever happened.
fn no_run_record(engine: &str, tick: u64, outcome: &str, detail: String) -> serde_json::Value {
    serde_json::json!({
        "api-version": API_VERSION,
        "probe-tick": tick,
        "engine": engine,
        "outcome": outcome,
        "detail": detail,
        "run-id": serde_json::Value::Null,
        "model": serde_json::Value::Null,
        "state": serde_json::Value::Null,
        "status": serde_json::Value::Null,
        "usage": Usage::default(),
        "text": "",
        "text-bytes": 0,
        "truncated": false,
        "provider-truncated": false,
        "wall-ms": 0,
        "events": 0,
        "order-ok": true,
        "order-faults": 0,
        "last-seq": serde_json::Value::Null,
        "malformed-events": *MALFORMED.lock().unwrap(),
    })
}

/// Publishes one outcome: `last.json` replaced, one line appended to the
/// append-only history (O(1) per outcome — never a rewrite of the log).
fn publish(record: &serde_json::Value) -> Result<(), GuestFault> {
    let dir = config().dir;
    let mut line = serde_json::to_vec(record).expect("the record encodes");
    line.push(b'\n');
    fs::write(
        &format!("{dir}/last.json"),
        &serde_json::to_vec_pretty(record).expect("the record encodes"),
        NO_KEY,
    )
    .map_err(|error| fs_fault("last.json write", error))?;
    fs::append(&format!("{dir}/history.log"), &line, NO_KEY)
        .map_err(|error| fs_fault("history append", error))
}

/// Starts one run on the configured engine. Every way this can fail to
/// produce a run is a recorded outcome, never a fault.
fn start_run(config: &ProbeConfig, tick: u64, now_ms: u64) -> Result<(), GuestFault> {
    let contract = engine_contract(&config.engine);
    let handle = match services::resolve(&contract) {
        Ok(handle) => handle,
        Err(error) => {
            return publish(&no_run_record(
                &config.engine,
                tick,
                "no-provider",
                format!("{contract} did not resolve: {error:?}"),
            ))
        }
    };
    let request = RunRequest {
        api_version: API_VERSION.to_owned(),
        engine: config.engine.clone(),
        model: config.model.clone(),
        prompt: config.prompt.clone(),
        budget: config.budget,
        // Default-deny, spelled out: a probe never needs a tool, and the
        // policy travels rather than being left to the provider's CLI.
        ..RunRequest::default()
    };
    let payload = serde_json::to_vec(&request).expect("a run request encodes");
    let answered = match services::call(handle, OP_RUN, &payload) {
        Ok(answered) => answered,
        Err(error) => {
            return publish(&no_run_record(
                &config.engine,
                tick,
                "call-failed",
                format!("{OP_RUN} refused: {error:?}"),
            ))
        }
    };
    let answer: Answer = match serde_json::from_slice(&answered) {
        Ok(answer) => answer,
        Err(error) => {
            return publish(&no_run_record(
                &config.engine,
                tick,
                "malformed-answer",
                format!("the answer did not decode: {error}"),
            ))
        }
    };
    // A typed refusal is the seam working, not the probe failing: the
    // provider said `unavailable` (its CLI is absent), `refused` (a
    // grant), `invalid`, and the code is the fact worth recording.
    let value = match answer.into_result() {
        Ok(value) => value,
        Err(error) => {
            return publish(&no_run_record(
                &config.engine,
                tick,
                "refused",
                format!("{:?}: {}", error.code, error.message),
            ))
        }
    };
    let accepted: RunAccepted = match serde_json::from_value(value) {
        Ok(accepted) => accepted,
        Err(error) => {
            return publish(&no_run_record(
                &config.engine,
                tick,
                "malformed-answer",
                format!("the accept did not decode: {error}"),
            ))
        }
    };
    *CURRENT.lock().unwrap() = Some(InFlight::new(&accepted, tick, now_ms));
    Ok(())
}

/// One scheduled probe.
fn tick(now_ms: u64) -> Result<(), GuestFault> {
    let config = config();
    let tick = {
        let mut held = TICKS.lock().unwrap();
        *held += 1;
        *held
    };
    // A run still in flight: either it is young enough to keep waiting on
    // — recorded as skipped, so the schedule's own behaviour is visible —
    // or it has outlived its wall budget and the period, and the probe
    // gives up on it and records that instead of wedging forever.
    let stale = {
        let held = CURRENT.lock().unwrap();
        match held.as_ref() {
            None => None,
            Some(run) => {
                let waited = now_ms.saturating_sub(run.started_ms);
                if waited > config.budget.wall_ms.saturating_add(config.every_ms) {
                    Some(true)
                } else {
                    Some(false)
                }
            }
        }
    };
    match stale {
        None => {}
        Some(false) => {
            let run_id = CURRENT
                .lock()
                .unwrap()
                .as_ref()
                .map(|run| run.run_id.clone())
                .unwrap_or_default();
            return publish(&no_run_record(
                &config.engine,
                tick,
                "skipped",
                format!("run {run_id} is still in flight"),
            ));
        }
        Some(true) => {
            let abandoned = CURRENT.lock().unwrap().take();
            if let Some(mut run) = abandoned {
                run.reason = Some("no terminal event within the wall budget".to_owned());
                publish(&run_record(&run, now_ms))?;
            }
        }
    }
    start_run(&config, tick, now_ms)
}

/// One event off the seam's shared topic, folded into the run we started.
fn absorb(payload: &[u8], now_ms: u64) -> Result<&'static str, GuestFault> {
    let Ok(incoming) = serde_json::from_slice::<RunEvent>(payload) else {
        *MALFORMED.lock().unwrap() += 1;
        return Ok("malformed");
    };
    let max_answer_bytes = config().max_answer_bytes;
    let settled = {
        let mut held = CURRENT.lock().unwrap();
        let Some(run) = held.as_mut() else {
            return Ok("no-run");
        };
        // The topic carries every engine's traffic; this is how a
        // consumer routes without a topic per engine.
        if incoming.run_id != run.run_id || incoming.engine != run.engine {
            return Ok("foreign");
        }
        run.events += 1;
        // The ordering proof: `seq` is per run and starts at 0. A gap or
        // a repeat is recorded, never repaired — the point is to see it.
        if incoming.seq != run.next_seq {
            run.order_ok = false;
            run.order_faults += 1;
        }
        run.next_seq = incoming.seq.saturating_add(1);
        run.last_seq = Some(incoming.seq);
        match &incoming.event {
            Event::Started { model } => {
                run.state = RunState::Running;
                if run.model.is_none() {
                    run.model.clone_from(model);
                }
            }
            Event::Delta { text } => {
                run.text_bytes = run.text_bytes.saturating_add(text.len() as u64);
                run.truncated |= append_bounded(&mut run.text, text, max_answer_bytes);
            }
            Event::TurnEnd { text } => {
                if run.text.is_empty() {
                    if let Some(text) = text {
                        run.text_bytes = run.text_bytes.saturating_add(text.len() as u64);
                        run.truncated |= append_bounded(&mut run.text, text, max_answer_bytes);
                    }
                }
            }
            Event::Exited {
                status,
                usage,
                truncated,
                error,
            } => {
                run.state = RunState::Exited;
                run.status = Some(*status);
                run.usage = *usage;
                run.provider_truncated = *truncated;
                // A clean exit whose ENGINE reported a failure is not a
                // success, and the record must not read as one.
                if let Some(error) = error {
                    run.reason = Some(error.clone());
                }
            }
            Event::Cancelled { reason } => {
                run.state = RunState::Cancelled;
                run.reason = Some(reason.clone());
            }
            // A tool crossing this probe never asks for, and a kind a
            // newer provider knows and this one does not: counted and
            // ordered, never guessed at (the definition's R12 additivity).
            Event::ToolCall { .. } | Event::ToolResult { .. } | Event::Unknown => {}
        }
        if run.state.is_terminal() {
            held.take()
        } else {
            None
        }
    };
    // The write happens with the slot already cleared: a run is finalized
    // exactly once, which is what makes an unkeyed append correct.
    match settled {
        Some(run) => {
            publish(&run_record(&run, now_ms))?;
            Ok("settled")
        }
        None => Ok("folded"),
    }
}

struct Probe;

impl Guest for Probe {
    fn activate(config: Vec<u8>) -> Result<(), GuestFault> {
        let config: ProbeConfig = serde_json::from_slice(&config)
            .map_err(|error| GuestFault::Failed(format!("malformed config: {error}")))?;
        if config.engine.is_empty() {
            return Err(GuestFault::Failed(
                "config.engine is the engine id to route to; it cannot be empty".to_owned(),
            ));
        }
        if config.every_ms == 0 {
            return Err(GuestFault::Failed(
                "config.every-ms is the probe's period; it cannot be 0".to_owned(),
            ));
        }
        let every_ms = config.every_ms;
        *CURRENT.lock().unwrap() = None;
        *TICKS.lock().unwrap() = 0;
        *MALFORMED.lock().unwrap() = 0;
        *CONFIG.lock().unwrap() = Some(config);
        effects::register("jinn-engine-probe on duty", EFFECT_TOKEN)
            .map_err(|error| fault("effect", error))?;
        // The result lane. The topic name is the grant; without it this
        // refuses and the probe would fold nothing.
        events::listen(EVENT_TOPIC, EVENT_TOKEN).map_err(|error| fault("listen", error))?;
        // The schedule. A one-shot at `now` would fire INSIDE the boot
        // reconcile, and a call into a provider that has not yet swapped
        // out of its staging incarnation permanently kills that
        // provider's slot — `FINDINGS.md` #30, the kernel gap this
        // consumer found. The first wake is therefore one period out,
        // where the composition has settled; that also happens to be
        // what a schedule means. `alarm-every`'s own first wake is one
        // period out (`FINDINGS.md` #13), so the schedule alone says it.
        clock::alarm_every(every_ms, TICK_TOKEN).map_err(|error| fault("alarm", error))?;
        Ok(())
    }

    fn check(_consumer: u64) -> bool {
        true
    }

    fn undo(_token: u64) -> Result<(), GuestFault> {
        Ok(())
    }

    fn handle_event(token: u64, topic: String, payload: Vec<u8>) -> Result<Vec<u8>, GuestFault> {
        if token == EVENT_TOKEN && topic == EVENT_TOPIC {
            let now_ms = clock::now().map_err(|error| fault("clock now", error))?;
            return Ok(absorb(&payload, now_ms)?.as_bytes().to_vec());
        }
        let instant: Option<[u8; 8]> = payload.as_slice().try_into().ok();
        let (Some(instant), true, true) = (instant, topic == WAKE_TOPIC, token == TICK_TOKEN)
        else {
            return Err(GuestFault::Failed(format!(
                "unexpected event {topic:?} (token {token}, {} bytes)",
                payload.len()
            )));
        };
        tick(u64::from_le_bytes(instant))?;
        Ok(b"probed".to_vec())
    }

    fn handle_call(
        _caller: u64,
        _contract: String,
        operation: String,
        _payload: Vec<u8>,
    ) -> Result<Vec<u8>, GuestFault> {
        // A consumer provides nothing.
        Err(GuestFault::Failed(format!(
            "unknown operation {operation:?}"
        )))
    }

    fn snapshot() -> Vec<u8> {
        // A run id belongs to the provider's incarnation, so an in-flight
        // run is not handed to a successor: the next tick starts a fresh
        // one and the abandoned run's absence is visible as a gap in the
        // history, not as a lie about a run nobody is watching.
        Vec::new()
    }

    fn restore(_blob: Vec<u8>) -> Result<(), GuestFault> {
        Ok(())
    }
}

export!(Probe);
