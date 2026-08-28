//! The `jinn:cron` provider. All scheduling decisions are `jinn-cron`'s
//! pure firing law; this guest only wires them to the kernel surfaces:
//! ticks arrive as events, fires leave as events, state and history persist
//! through the granted `jinn:fs` — every crossing ledger-visible by
//! construction (kernel Law 2).

use std::sync::Mutex;

use jinn_cron::{
    bounded_history, parse_config, plan_tick, read_error_is_absence, run_record_path, FirePayload,
    JobSpec, RunOutcome, RunRecord, SchedulerState, TickPayload, CRON_CONTRACT, HISTORY_CAP,
    HISTORY_PATH, QUARANTINE_DIR, STATE_PATH, TICK_TOPIC,
};

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::types::{DispatchMode, Selector};
use jinn::plugin::{effects, events, fs, services};

const EFFECT_TOKEN: u64 = 1;
const LISTEN_TOKEN: u64 = 2;

static JOBS: Mutex<Vec<JobSpec>> = Mutex::new(Vec::new());
static STATE: Mutex<Option<SchedulerState>> = Mutex::new(None);
static HISTORY: Mutex<Vec<RunRecord>> = Mutex::new(Vec::new());

fn fault(context: &str, error: jinn::plugin::types::KernelError) -> GuestFault {
    GuestFault::Failed(format!("{context}: {error:?}"))
}

fn state() -> SchedulerState {
    STATE.lock().unwrap().clone().unwrap_or_default()
}

/// One persisted document, honestly classified (contract §Persistence
/// honesty): genuinely absent, decoded, or present-but-undecodable with
/// the raw bytes in hand.
enum Loaded<T> {
    Absent,
    Value(T),
    Corrupt { bytes: Vec<u8>, detail: String },
}

/// Reads one persisted JSON document. Absence is the only silent case; a
/// read refusal that is NOT absence (permissions, provider failure) fails
/// the activation loudly — defaulting there could re-fire boundaries the
/// lost state already processed.
fn load_json<T: serde::de::DeserializeOwned>(path: &str) -> Result<Loaded<T>, GuestFault> {
    match fs::read(path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(value) => Ok(Loaded::Value(value)),
            Err(refused) => Ok(Loaded::Corrupt {
                bytes,
                detail: refused.to_string(),
            }),
        },
        Err(refused) => {
            let message = format!("{refused:?}");
            if read_error_is_absence(&message) {
                Ok(Loaded::Absent)
            } else {
                Err(GuestFault::Failed(format!("read {path}: {message}")))
            }
        }
    }
}

/// Quarantines one corrupt document: the original bytes are preserved
/// under [`QUARANTINE_DIR`] and the loss becomes a `state-fault` run
/// record — recorded, never silent.
fn quarantine(path: &str, bytes: &[u8], detail: &str) -> Result<RunRecord, GuestFault> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let preserved = format!("{QUARANTINE_DIR}/{name}");
    fs::write(&preserved, bytes).map_err(|error| fault("quarantine write", error))?;
    Ok(RunRecord {
        job: String::new(),
        scheduled_ms: 0,
        now_ms: 0,
        tick_seq: 0,
        outcome: RunOutcome::StateFault {
            path: path.to_owned(),
            detail: format!("{detail}; original preserved at {preserved}"),
        },
        extra: jinn_cron::Extensions::new(),
    })
}

/// One fire: emit on the job's topic (mode serial, selector all — the
/// contract's dispatch shape) and settle its run record. A refused emit is
/// a recorded outcome, never a lost boundary. `FirePayload` deliberately
/// carries no topic — the topic is routing, not payload; it lives in the
/// job table.
fn emit_fire(fire: &FirePayload) -> RunRecord {
    let topic = JOBS
        .lock()
        .unwrap()
        .iter()
        .find(|job| job.id == fire.job)
        .map(|job| job.topic.clone())
        .unwrap_or_default();
    let payload = serde_json::to_vec(fire).expect("fire payload encodes");
    let outcome = match events::emit(&topic, DispatchMode::Serial, &Selector::All, &payload) {
        Ok(answers) => RunOutcome::Fired {
            answers: answers.len() as u64,
        },
        Err(refused) => RunOutcome::EmitFailed {
            detail: format!("{refused:?}"),
        },
    };
    RunRecord {
        job: fire.job.clone(),
        scheduled_ms: fire.scheduled_ms,
        now_ms: fire.now_ms,
        tick_seq: fire.tick_seq,
        outcome,
        extra: jinn_cron::Extensions::new(),
    }
}

fn persist_state(next: &SchedulerState) -> Result<(), GuestFault> {
    let bytes = serde_json::to_vec(next).expect("state encodes");
    fs::write(STATE_PATH, &bytes).map_err(|error| fault("state write", error))
}

fn persist_history(records: Vec<RunRecord>) -> Result<(), GuestFault> {
    let history = {
        let mut held = HISTORY.lock().unwrap();
        *held = bounded_history(std::mem::take(&mut *held), records, HISTORY_CAP);
        held.clone()
    };
    let bytes = serde_json::to_vec(&history).expect("history encodes");
    fs::write(HISTORY_PATH, &bytes).map_err(|error| fault("history write", error))
}

struct Scheduler;

impl Guest for Scheduler {
    fn activate(config: Vec<u8>) -> Result<(), GuestFault> {
        let (jobs, faults) = parse_config(&config).map_err(GuestFault::Failed)?;
        *JOBS.lock().unwrap() = jobs;
        // Config faults and persistence faults are run records (contract:
        // never silent).
        let mut fault_records: Vec<RunRecord> = faults
            .into_iter()
            .map(|detail| RunRecord {
                job: String::new(),
                scheduled_ms: 0,
                now_ms: 0,
                tick_seq: 0,
                outcome: RunOutcome::ConfigFault { detail },
                extra: jinn_cron::Extensions::new(),
            })
            .collect();
        // Persisted state and history survive restarts (firing law #3).
        // Absence is the only silent default; a corrupt document is
        // preserved under quarantine and recorded (contract §Persistence
        // honesty).
        *STATE.lock().unwrap() = match load_json::<SchedulerState>(STATE_PATH)? {
            Loaded::Value(held) => Some(held),
            Loaded::Absent => None,
            Loaded::Corrupt { bytes, detail } => {
                fault_records.push(quarantine(STATE_PATH, &bytes, &detail)?);
                None
            }
        };
        *HISTORY.lock().unwrap() = match load_json::<Vec<RunRecord>>(HISTORY_PATH)? {
            Loaded::Value(held) => held,
            Loaded::Absent => Vec::new(),
            Loaded::Corrupt { bytes, detail } => {
                fault_records.push(quarantine(HISTORY_PATH, &bytes, &detail)?);
                Vec::new()
            }
        };
        effects::register("cron-scheduler on duty", EFFECT_TOKEN)
            .map_err(|error| fault("effect", error))?;
        services::provide(CRON_CONTRACT).map_err(|error| fault("provide", error))?;
        events::listen(TICK_TOPIC, LISTEN_TOKEN).map_err(|error| fault("listen", error))?;
        if !fault_records.is_empty() {
            persist_history(fault_records)?;
        }
        Ok(())
    }

    fn check(_consumer: u64) -> bool {
        true
    }

    fn undo(_token: u64) -> Result<(), GuestFault> {
        Ok(())
    }

    fn handle_event(_token: u64, _topic: String, payload: Vec<u8>) -> Result<Vec<u8>, GuestFault> {
        let tick: TickPayload = serde_json::from_slice(&payload)
            .map_err(|error| GuestFault::Failed(format!("malformed tick: {error}")))?;
        let plan = plan_tick(&JOBS.lock().unwrap(), &state(), &tick);
        // State BEFORE history and fires: a torn tick loses a record,
        // never doubles a fire (contract §Run history).
        *STATE.lock().unwrap() = Some(plan.state.clone());
        persist_state(&plan.state)?;
        let mut records = plan.records;
        let fired = plan.fires.len() as u64;
        for fire in &plan.fires {
            let record = emit_fire(fire);
            // The per-fire record write: one identifiable granted-write
            // effect per fire — its ledger label names the job and the
            // boundary, making the fire ledger-traceable today (contract
            // §Run history; the kernel bus tap is queued work).
            let path = run_record_path(&fire.job, fire.scheduled_ms);
            let bytes = serde_json::to_vec_pretty(&record).expect("run record encodes");
            fs::write(&path, &bytes).map_err(|error| fault("run-record write", error))?;
            records.push(record);
        }
        if !records.is_empty() {
            persist_history(records)?;
        }
        Ok(serde_json::to_vec(&serde_json::json!({ "fires": fired })).expect("summary encodes"))
    }

    fn handle_call(
        _caller: u64,
        _contract: String,
        operation: String,
        _payload: Vec<u8>,
    ) -> Result<Vec<u8>, GuestFault> {
        match operation.as_str() {
            jinn_cron::OP_JOBS => {
                let held = state();
                let jobs: Vec<serde_json::Value> = JOBS
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|job| {
                        let next = held.last.get(&job.id).map(|last| last + job.every_ms);
                        serde_json::json!({
                            "id": job.id, "every-ms": job.every_ms,
                            "topic": job.topic, "next-ms": next,
                        })
                    })
                    .collect();
                Ok(serde_json::to_vec(&serde_json::json!({ "jobs": jobs })).expect("jobs encode"))
            }
            jinn_cron::OP_HISTORY => {
                Ok(serde_json::to_vec(&*HISTORY.lock().unwrap()).expect("history encodes"))
            }
            other => Err(GuestFault::Failed(format!("unknown operation {other:?}"))),
        }
    }

    fn snapshot() -> Vec<u8> {
        // Mode-1 handoff: the successor resumes the schedule, not a fresh
        // one (firing law #3).
        serde_json::to_vec(&state()).expect("state encodes")
    }

    fn restore(blob: Vec<u8>) -> Result<(), GuestFault> {
        if blob.is_empty() {
            return Ok(());
        }
        let handed: SchedulerState = serde_json::from_slice(&blob)
            .map_err(|error| GuestFault::Failed(format!("malformed handoff: {error}")))?;
        *STATE.lock().unwrap() = Some(handed);
        Ok(())
    }
}

export!(Scheduler);
