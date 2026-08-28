//! The `jinn:cron` provider. All scheduling decisions are `jinn-cron`'s
//! pure firing law; this guest only wires them to the kernel surfaces:
//! ticks arrive as events, fires leave as events, state and history persist
//! through the granted `jinn:fs` — every crossing ledger-visible by
//! construction (kernel Law 2).

use std::sync::Mutex;

use jinn_cron::{
    bounded_history, parse_config, plan_tick, FirePayload, JobSpec, RunOutcome, RunRecord,
    SchedulerState, TickPayload, CRON_CONTRACT, HISTORY_CAP, HISTORY_PATH, STATE_PATH, TICK_TOPIC,
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
        // Persisted state and history survive restarts (firing law #3);
        // absence is a fresh schedule, not an error.
        *STATE.lock().unwrap() = fs::read(STATE_PATH)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok());
        *HISTORY.lock().unwrap() = fs::read(HISTORY_PATH)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        effects::register("cron-scheduler on duty", EFFECT_TOKEN)
            .map_err(|error| fault("effect", error))?;
        services::provide(CRON_CONTRACT).map_err(|error| fault("provide", error))?;
        events::listen(TICK_TOPIC, LISTEN_TOKEN).map_err(|error| fault("listen", error))?;
        if !faults.is_empty() {
            // Config faults are run records (contract: never silent).
            let records: Vec<RunRecord> = faults
                .into_iter()
                .map(|detail| RunRecord {
                    job: String::new(),
                    scheduled_ms: 0,
                    now_ms: 0,
                    tick_seq: 0,
                    outcome: RunOutcome::ConfigFault { detail },
                })
                .collect();
            persist_history(records)?;
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
            records.push(emit_fire(fire));
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
