//! A real scheduled job: on each fire, probe the data root and write a
//! cumulative health report. The observable surface a guest can actually
//! probe is exactly its granted `jinn:fs` scope, and with the 0.2.0 bundle
//! that surface is honest-wide: write, read back, compare; `meta` the
//! probe's size; `list` the report directory and the scheduler's per-fire
//! run records; `meta` the history log. No process (FINDINGS.md #5) and no
//! clock of its own (time is the scheduler's business); the report is
//! honest about being that.
//!
//! The cron-plane peek (`jinn:cron` resolve + `jobs` call) happens in
//! `activate` and ONLY there: calling back into the scheduler while
//! handling a fire deadlocks the seam until the kernel's guest deadline
//! kills the call (FINDINGS.md #4). World `jinn:plugin@0.3.0`: the report
//! is the entry's continuing record — a daemon stop suspends this fiber
//! and retains it; only removal from the profile withdraws it.

use std::sync::Mutex;

use jinn_cron::{FirePayload, CRON_CONTRACT, HISTORY_PATH, OP_JOBS};
use serde::Deserialize;

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::{effects, events, fs, services};

const EFFECT_TOKEN: u64 = 1;
const LISTEN_TOKEN: u64 = 2;
/// No idempotency claim: every report write carries a new fire count.
const NO_KEY: &str = "";

#[derive(Deserialize)]
struct SnapshotConfig {
    /// The job topic to serve.
    topic: String,
    /// Report directory under the granted scope.
    #[serde(default = "default_dir")]
    dir: String,
    /// Operator-lane restart nonce: bumping it re-activates this fiber
    /// (and re-takes the boot peek) without touching anything else.
    #[serde(default)]
    #[allow(dead_code)]
    nonce: u64,
}

fn default_dir() -> String {
    "health".into()
}

static DIR: Mutex<String> = Mutex::new(String::new());

fn fault(context: &str, error: jinn::plugin::types::KernelError) -> GuestFault {
    GuestFault::Failed(format!("{context}: {error:?}"))
}

fn fs_fault(context: &str, error: fs::FsError) -> GuestFault {
    GuestFault::Failed(format!("{context}: {error:?}"))
}

/// The boot peek: the scheduler's live job table, or an honest
/// `unavailable` marker (no gating exists for sibling readiness — the peek
/// is opportunistic by design).
fn cron_peek() -> serde_json::Value {
    let refused = |error: jinn::plugin::types::KernelError| serde_json::json!({ "unavailable": format!("{error:?}") });
    match services::resolve(CRON_CONTRACT) {
        Ok(handle) => match services::call(handle, OP_JOBS, &[]) {
            Ok(answer) => serde_json::from_slice(&answer)
                .unwrap_or_else(|error| serde_json::json!({ "malformed": error.to_string() })),
            Err(error) => refused(error),
        },
        Err(error) => refused(error),
    }
}

/// One `meta` answer as report JSON, or the typed absence / refusal as an
/// honest marker — never a folded message.
fn meta_json(path: &str) -> serde_json::Value {
    match fs::meta(path) {
        Ok(meta) => serde_json::json!({
            "size": meta.size, "modified-ms": meta.modified_ms, "is-dir": meta.is_dir,
        }),
        Err(fs::FsError::NotFound) => serde_json::json!({ "absent": true }),
        Err(refused) => serde_json::json!({ "unavailable": format!("{refused:?}") }),
    }
}

/// One `list` answer: the entry names (the bundle answers them sorted), or
/// the honest marker.
fn list_json(path: &str) -> serde_json::Value {
    match fs::list(path) {
        Ok(entries) => serde_json::json!({
            "count": entries.len(),
            "entries": entries.iter().map(|entry| entry.path.clone()).collect::<Vec<_>>(),
        }),
        Err(fs::FsError::NotFound) => serde_json::json!({ "absent": true }),
        Err(refused) => serde_json::json!({ "unavailable": format!("{refused:?}") }),
    }
}

/// The running fire count from the prior report: absent is the typed
/// first-fire answer, an undecodable report restarts the count, any other
/// refusal is loud.
fn prior_fires(report_path: &str) -> Result<u64, GuestFault> {
    match fs::read(report_path) {
        Ok(bytes) => Ok(serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|report| report["fires"].as_u64())
            .unwrap_or(0)),
        Err(fs::FsError::NotFound) => Ok(0),
        Err(refused) => Err(fs_fault("report read", refused)),
    }
}

struct Snapshot;

impl Guest for Snapshot {
    fn activate(config: Vec<u8>) -> Result<(), GuestFault> {
        let config: SnapshotConfig = serde_json::from_slice(&config)
            .map_err(|error| GuestFault::Failed(format!("malformed config: {error}")))?;
        *DIR.lock().unwrap() = config.dir.clone();
        effects::register("health-snapshot on duty", EFFECT_TOKEN)
            .map_err(|error| fault("effect", error))?;
        events::listen(&config.topic, LISTEN_TOKEN).map_err(|error| fault("listen", error))?;
        let boot = serde_json::json!({ "cron": cron_peek() });
        fs::write(
            &format!("{}/boot.json", config.dir),
            &serde_json::to_vec(&boot).expect("boot encodes"),
            NO_KEY,
        )
        .map_err(|error| fs_fault("boot write", error))?;
        Ok(())
    }

    fn check(_consumer: u64) -> bool {
        true
    }

    fn undo(_token: u64) -> Result<(), GuestFault> {
        Ok(())
    }

    fn handle_event(_token: u64, _topic: String, payload: Vec<u8>) -> Result<Vec<u8>, GuestFault> {
        let fire: FirePayload = serde_json::from_slice(&payload)
            .map_err(|error| GuestFault::Failed(format!("malformed fire: {error}")))?;
        let dir = DIR.lock().unwrap().clone();
        // The probe: write, read back, compare, then `meta` agrees on the
        // size — the writability check the granted scope supports.
        let probe_path = format!("{dir}/probe.txt");
        let probe = fire.tick_seq.to_string().into_bytes();
        fs::write(&probe_path, &probe, NO_KEY).map_err(|error| fs_fault("probe write", error))?;
        let probe_ok = fs::read(&probe_path).is_ok_and(|read| read == probe)
            && fs::meta(&probe_path).is_ok_and(|meta| meta.size == probe.len() as u64);
        let report_path = format!("{dir}/report.json");
        let fires = prior_fires(&report_path)? + 1;
        // The wider surface: this directory, the fired job's run records,
        // and the scheduler's history log — enumerated and stat'ed, not
        // inferred.
        let report = serde_json::json!({
            "fires": fires,
            "probe-ok": probe_ok,
            "last": fire,
            "dir": list_json(&dir),
            "run-records": list_json(&format!("cron/runs/{}", fire.job)),
            "history-log": meta_json(HISTORY_PATH),
        });
        fs::write(
            &report_path,
            &serde_json::to_vec_pretty(&report).expect("report encodes"),
            NO_KEY,
        )
        .map_err(|error| fs_fault("report write", error))?;
        Ok(b"ok".to_vec())
    }

    fn handle_call(
        _caller: u64,
        _contract: String,
        operation: String,
        _payload: Vec<u8>,
    ) -> Result<Vec<u8>, GuestFault> {
        Err(GuestFault::Failed(format!(
            "unknown operation {operation:?}"
        )))
    }

    fn snapshot() -> Vec<u8> {
        Vec::new()
    }

    fn restore(_blob: Vec<u8>) -> Result<(), GuestFault> {
        Ok(())
    }
}

export!(Snapshot);
