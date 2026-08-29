//! The `jinn:cron` provider. All scheduling decisions are `jinn-cron`'s
//! pure firing law; this guest only wires them to the kernel surfaces:
//! time arrives through the granted `jinn:clock` (a `now` read at
//! activation, then the wakes of one periodic alarm), fires leave as
//! events, state and history persist through the granted `jinn:fs`
//! (0.2.0: typed `not-found`, keyed writes, and `append` for the history
//! log — one O(1) append per tick, never a rewrite). Every contract
//! crossing, every wake, and every fire emit (`DispatchTrace`) is a ledger
//! event; the per-fire run-record write, whose label names the job and
//! boundary, is the fire's outcome document (contract §Run history).
//!
//! Alarms do not survive a kernel restart (the clock contract's honest
//! bound): the activate-time plan is this guest's re-entry — a catch-up
//! lands at once, never one period later (FINDINGS.md #13).
//!
//! World `jinn:plugin@0.3.0` (suspend ≠ dispose): a daemon stop or a
//! config-edit restart SUSPENDS this fiber — the persisted state and
//! history stay on disk for the entry, `undo` never runs on suspension —
//! and only the entry's removal from the profile withdraws them.
//!
//! Settings (phase 2.2): the job table is CONSUMED through `jinn:settings`
//! — this guest declares the `cron` namespace (schema, defaults, hot keys)
//! and takes the RESOLVED settings (defaults < its entry config < the
//! overlay) from the provider's answer on every alarm wake, and absorbs a
//! `jinn:settings/changed` event in place. The entry config it activated
//! on is the entry LAYER of that resolution, never bypassed. It never
//! calls the provider from `activate`: the provider patches this entry
//! synchronously (the restart path), and an activation that called back
//! would be the nested-dispatch deadlock (FINDINGS.md #4, #26) — so the
//! first resolution happens on a one-shot alarm right after activation,
//! and the activation plan runs on the entry layer alone. With no
//! provider reachable (the cron-only profile, or boot ordering —
//! FINDINGS.md #7) the entry layer is the whole truth and every wake
//! retries the declaration.

use std::sync::Mutex;

use jinn_cron::{
    bounded_history, history_line, parse_config, parse_history_lines, parse_legacy_history,
    plan_tick, run_record_path, settings_declaration, FirePayload, JobSpec, ParsedConfig,
    RunOutcome, RunRecord, SchedulerState, TickPayload, CRON_CONTRACT, HISTORY_CAP, HISTORY_PATH,
    LEGACY_HISTORY_PATH, QUARANTINE_DIR, SETTINGS_CHANGED_TOPIC, SETTINGS_NAMESPACE, STATE_PATH,
    WAKE_TOPIC,
};
use jinn_settings::{Answer as SettingsAnswer, Changed, DeclareRequest, Outcome, Resolved};

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::types::{DispatchMode, Selector};
use jinn::plugin::{clock, effects, events, fs, services};

const EFFECT_TOKEN: u64 = 1;
/// The token the alarm is requested under; a wake carrying any other
/// token is not ours.
const ALARM_TOKEN: u64 = 2;
/// The one-shot alarm right after activation: the first settings
/// resolution (never from `activate` itself — see the crate doc).
const SETTINGS_TOKEN: u64 = 3;
/// The `jinn:settings/changed` listener.
const CHANGED_TOKEN: u64 = 4;
/// No idempotency claim: each state write and history append is a new
/// effect by construction (a tick never repeats within a fiber).
const NO_KEY: &str = "";

static JOBS: Mutex<Vec<JobSpec>> = Mutex::new(Vec::new());
/// This entry's id (the kernel names it in the declaration), and the raw
/// entry config it activated on — the entry LAYER it declares.
static ENTRY: Mutex<String> = Mutex::new(String::new());
static ENTRY_LAYER: Mutex<Option<serde_json::Value>> = Mutex::new(None);
/// The settings revision last applied (`None`: entry layer only).
static SETTINGS_REVISION: Mutex<Option<u64>> = Mutex::new(None);
static STATE: Mutex<Option<SchedulerState>> = Mutex::new(None);
/// The bounded window the `history` operation serves; the log on disk is
/// the append-only lane it is seeded from.
static HISTORY: Mutex<Vec<RunRecord>> = Mutex::new(Vec::new());
/// Wake editions since activation (`tick-seq`); the activate plan is 0.
static WAKES: Mutex<u64> = Mutex::new(0);

fn fault(context: &str, error: jinn::plugin::types::KernelError) -> GuestFault {
    GuestFault::Failed(format!("{context}: {error:?}"))
}

fn fs_fault(context: &str, error: fs::FsError) -> GuestFault {
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

/// Reads one persisted document. Absence is the bundle's TYPED answer
/// (`not-found`) and the only silent case; any other refusal (denied,
/// provider failure) fails the activation loudly — defaulting there could
/// re-fire boundaries the lost state already processed.
fn load<T>(path: &str, decode: impl FnOnce(&[u8]) -> Result<T, String>) -> Result<Loaded<T>, GuestFault> {
    match fs::read(path) {
        Ok(bytes) => Ok(match decode(&bytes) {
            Ok(value) => Loaded::Value(value),
            Err(detail) => Loaded::Corrupt { bytes, detail },
        }),
        Err(fs::FsError::NotFound) => Ok(Loaded::Absent),
        Err(refused) => Err(fs_fault(&format!("read {path}"), refused)),
    }
}

/// Quarantines one corrupt document: the original bytes are preserved
/// under [`QUARANTINE_DIR`] and the loss becomes a `state-fault` run
/// record — recorded, never silent.
fn quarantine(path: &str, bytes: &[u8], detail: &str) -> Result<RunRecord, GuestFault> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let preserved = format!("{QUARANTINE_DIR}/{name}");
    fs::write(&preserved, bytes, NO_KEY).map_err(|error| fs_fault("quarantine write", error))?;
    Ok(RunRecord {
        job: String::new(),
        scheduled_ms: 0,
        now_ms: 0,
        tick_seq: 0,
        outcome: RunOutcome::StateFault {
            path: path.to_owned(),
            detail: format!("{detail}; original preserved at {preserved}"),
            extra: jinn_cron::Extensions::new(),
        },
        extra: jinn_cron::Extensions::new(),
    })
}

/// Loads one document into a value, quarantining a corrupt one into
/// `faults` and answering `absent` for a missing one.
fn load_or_quarantine<T>(
    path: &str,
    decode: impl FnOnce(&[u8]) -> Result<T, String>,
    absent: T,
    faults: &mut Vec<RunRecord>,
) -> Result<T, GuestFault> {
    Ok(match load(path, decode)? {
        Loaded::Value(held) => held,
        Loaded::Absent => absent,
        Loaded::Corrupt { bytes, detail } => {
            faults.push(quarantine(path, &bytes, &detail)?);
            absent
        }
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
            extra: jinn_cron::Extensions::new(),
        },
        Err(refused) => RunOutcome::EmitFailed {
            detail: format!("{refused:?}"),
            extra: jinn_cron::Extensions::new(),
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
    fs::write(STATE_PATH, &bytes, NO_KEY).map_err(|error| fs_fault("state write", error))
}

/// Extends the window and appends the new records to the log — ONE
/// `append` effect per tick, sized by the tick's records, never by the
/// history (the O(n)-per-fire rewrite is retired; FINDINGS.md #3).
fn persist_history(records: Vec<RunRecord>) -> Result<(), GuestFault> {
    let lines: Vec<u8> = records.iter().flat_map(history_line).collect();
    {
        let mut held = HISTORY.lock().unwrap();
        *held = bounded_history(std::mem::take(&mut *held), records, HISTORY_CAP);
    }
    fs::append(HISTORY_PATH, &lines, NO_KEY).map_err(|error| fs_fault("history append", error))
}

/// One consultation of the firing law at `tick`: persist state, emit the
/// due fires, settle their records. Returns the fire count.
fn run_tick(tick: &TickPayload) -> Result<u64, GuestFault> {
    let plan = plan_tick(&JOBS.lock().unwrap(), &state(), tick);
    // State BEFORE history and fires: a torn tick loses a record, never
    // doubles a fire (contract §Run history).
    *STATE.lock().unwrap() = Some(plan.state.clone());
    persist_state(&plan.state)?;
    let mut records = plan.records;
    let fired = plan.fires.len() as u64;
    for fire in &plan.fires {
        let record = emit_fire(fire);
        // The per-fire record write: one identifiable granted-write effect
        // per fire — its ledger label names the job and the boundary; the
        // kernel's DispatchTrace on the emit is the audit line, this is
        // the outcome document (contract §Run history). The boundary IS
        // the fire's identity, so it is the write's idempotency key.
        let path = run_record_path(&fire.job, fire.scheduled_ms);
        let bytes = serde_json::to_vec_pretty(&record).expect("run record encodes");
        fs::write(&path, &bytes, &path).map_err(|error| fs_fault("run-record write", error))?;
        records.push(record);
    }
    if !records.is_empty() {
        persist_history(records)?;
    }
    Ok(fired)
}

/// Lays resolved settings over the job table: the resolved `jobs` replace
/// the table (config faults become run records, never silent); answers
/// whether the table changed. `tick-ms` is a restart-path key and is not
/// touched here (the alarm is the fiber's).
fn apply_settings(settings: &serde_json::Value, revision: u64) -> Result<bool, GuestFault> {
    let parsed = parse_config(&serde_json::to_vec(settings).expect("settings encode"))
        .map_err(|detail| GuestFault::Failed(format!("resolved settings: {detail}")))?;
    let changed = {
        let mut jobs = JOBS.lock().unwrap();
        let changed = *jobs != parsed.jobs;
        *jobs = parsed.jobs;
        changed
    };
    *SETTINGS_REVISION.lock().unwrap() = Some(revision);
    if !parsed.faults.is_empty() {
        persist_history(
            parsed
                .faults
                .into_iter()
                .map(|detail| RunRecord {
                    job: String::new(),
                    scheduled_ms: 0,
                    now_ms: 0,
                    tick_seq: 0,
                    outcome: RunOutcome::ConfigFault {
                        detail,
                        extra: jinn_cron::Extensions::new(),
                    },
                    extra: jinn_cron::Extensions::new(),
                })
                .collect(),
        )?;
    }
    Ok(changed)
}

/// One declaration on the provider (idempotent upsert; the answer is the
/// resolved settings), applied. `Ok(false)` when no provider answers —
/// the entry layer stays the truth and the next wake retries.
fn sync_settings() -> Result<bool, GuestFault> {
    let Ok(handle) = services::resolve(jinn_settings::SETTINGS_CONTRACT) else {
        return Ok(false);
    };
    let request = DeclareRequest {
        declaration: settings_declaration(&ENTRY.lock().unwrap()),
        current: ENTRY_LAYER.lock().unwrap().clone().unwrap_or_default(),
    };
    let Ok(bytes) = services::call(
        handle,
        jinn_settings::OP_DECLARE,
        &serde_json::to_vec(&request).expect("declare encodes"),
    ) else {
        return Ok(false);
    };
    match SettingsAnswer::decode(&bytes).outcome {
        Outcome::Ok(value) => {
            let resolved: Resolved = serde_json::from_value(value)
                .map_err(|bad| GuestFault::Failed(format!("declare answer: {bad}")))?;
            apply_settings(&resolved.settings, resolved.revision)
        }
        Outcome::Error(_) => Ok(false),
    }
}

/// A settings sync followed by a plan when the table changed (a job that
/// appeared starts its schedule now, one that left never fires again).
fn sync_and_plan(now_ms: u64) -> Result<(), GuestFault> {
    if sync_settings()? {
        let seq = {
            let mut wakes = WAKES.lock().unwrap();
            *wakes += 1;
            *wakes
        };
        run_tick(&TickPayload {
            seq,
            now_ms,
            extra: jinn_cron::Extensions::new(),
        })?;
    }
    Ok(())
}

struct Scheduler;

impl Guest for Scheduler {
    fn activate(config: Vec<u8>) -> Result<(), GuestFault> {
        let ParsedConfig {
            jobs,
            faults,
            tick_ms,
        } = parse_config(&config).map_err(GuestFault::Failed)?;
        *JOBS.lock().unwrap() = jobs;
        *WAKES.lock().unwrap() = 0;
        *SETTINGS_REVISION.lock().unwrap() = None;
        *ENTRY_LAYER.lock().unwrap() = serde_json::from_slice(&config).ok();
        // The entry id the settings seam names (`entry-id` in config,
        // written by the kit); absent, the namespace's entry is unnamed
        // and a restart-path patch has no target.
        *ENTRY.lock().unwrap() = serde_json::from_slice::<serde_json::Value>(&config)
            .ok()
            .and_then(|value| value[jinn_cron::ENTRY_ID_KEY].as_str().map(str::to_owned))
            .unwrap_or_default();
        // Config faults and persistence faults are run records (contract:
        // never silent).
        let mut fault_records: Vec<RunRecord> = faults
            .into_iter()
            .map(|detail| RunRecord {
                job: String::new(),
                scheduled_ms: 0,
                now_ms: 0,
                tick_seq: 0,
                outcome: RunOutcome::ConfigFault {
                    detail,
                    extra: jinn_cron::Extensions::new(),
                },
                extra: jinn_cron::Extensions::new(),
            })
            .collect();
        // Persisted state and history survive restarts (firing law #3).
        // Absence is the only silent default; a corrupt document is
        // preserved under quarantine and recorded (contract §Persistence
        // honesty).
        *STATE.lock().unwrap() = load_or_quarantine(
            STATE_PATH,
            |bytes| serde_json::from_slice(bytes).map_err(|error| error.to_string()),
            None,
            &mut fault_records,
        )?;
        // The window: the legacy array (read once, never written again)
        // seeds it, then the append log — in that order, so a pre-0.2.0
        // root carries its records forward without duplicating them.
        let legacy = load_or_quarantine(
            LEGACY_HISTORY_PATH,
            |bytes| parse_legacy_history(bytes),
            Vec::new(),
            &mut fault_records,
        )?;
        let log = load_or_quarantine(
            HISTORY_PATH,
            |bytes| parse_history_lines(bytes),
            Vec::new(),
            &mut fault_records,
        )?;
        *HISTORY.lock().unwrap() = bounded_history(legacy, log, HISTORY_CAP);
        effects::register("cron-scheduler on duty", EFFECT_TOKEN)
            .map_err(|error| fault("effect", error))?;
        services::provide(CRON_CONTRACT).map_err(|error| fault("provide", error))?;
        if !fault_records.is_empty() {
            persist_history(fault_records)?;
        }
        // Time enters: one plan at the clock's `now` (edition 0) — the
        // restart re-entry, since a periodic alarm's first wake is one
        // full period out and alarms drop on restart — then the alarm
        // itself, an effect the kernel cancels with this fiber (R5).
        let now_ms = clock::now().map_err(|error| fault("clock now", error))?;
        run_tick(&TickPayload {
            seq: 0,
            now_ms,
            extra: jinn_cron::Extensions::new(),
        })?;
        clock::alarm_every(tick_ms, ALARM_TOKEN).map_err(|error| fault("alarm", error))?;
        // The settings seam, one wake later (never from here): a listener
        // for applied patches and a one-shot alarm for the first
        // resolution. Both are kernel registrations released with the
        // incarnation. A refused listen (no grant — the cron-only
        // profile) is the honest "no settings seam here".
        let _ = events::listen(SETTINGS_CHANGED_TOPIC, CHANGED_TOKEN);
        clock::alarm_at(now_ms, SETTINGS_TOKEN).map_err(|error| fault("settings alarm", error))?;
        Ok(())
    }

    fn check(_consumer: u64) -> bool {
        true
    }

    fn undo(_token: u64) -> Result<(), GuestFault> {
        Ok(())
    }

    fn handle_event(token: u64, topic: String, payload: Vec<u8>) -> Result<Vec<u8>, GuestFault> {
        // An applied settings patch: absorb the resolved settings in
        // place — never calling back into the provider (it is mid-call).
        if topic == SETTINGS_CHANGED_TOPIC && token == CHANGED_TOKEN {
            let changed: Changed = serde_json::from_slice(&payload)
                .map_err(|bad| GuestFault::Failed(format!("malformed changed event: {bad}")))?;
            if changed.namespace == SETTINGS_NAMESPACE && apply_settings(&changed.settings, changed.revision)? {
                let now_ms = clock::now().map_err(|error| fault("clock now", error))?;
                let seq = {
                    let mut wakes = WAKES.lock().unwrap();
                    *wakes += 1;
                    *wakes
                };
                run_tick(&TickPayload {
                    seq,
                    now_ms,
                    extra: jinn_cron::Extensions::new(),
                })?;
            }
            return Ok(b"applied".to_vec());
        }
        // Only the kernel's typed wake of OUR alarms is time; anything
        // else on this entry point is a contract violation, refused loudly.
        let instant: Option<[u8; 8]> = payload.as_slice().try_into().ok();
        let (Some(instant), true) = (instant, topic == WAKE_TOPIC) else {
            return Err(GuestFault::Failed(format!(
                "unexpected event {topic:?} (token {token}, {} bytes)",
                payload.len()
            )));
        };
        let now_ms = u64::from_le_bytes(instant);
        if token == SETTINGS_TOKEN {
            sync_and_plan(now_ms)?;
            return Ok(Vec::new());
        }
        if token != ALARM_TOKEN {
            return Err(GuestFault::Failed(format!("a wake for an unknown alarm token {token}")));
        }
        // Every wake re-declares: the provider's answer IS the job table
        // (a provider restart or swap heals here), then the firing law.
        sync_settings()?;
        let seq = {
            let mut wakes = WAKES.lock().unwrap();
            *wakes += 1;
            *wakes
        };
        let fired = run_tick(&TickPayload {
            seq,
            now_ms,
            extra: jinn_cron::Extensions::new(),
        })?;
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
                Ok(serde_json::to_vec(&serde_json::json!({
                    "jobs": jobs,
                    "settings-revision": *SETTINGS_REVISION.lock().unwrap(),
                }))
                .expect("jobs encode"))
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
