//! A real scheduled job: on each fire, probe the data root (write, read
//! back, compare) and write a cumulative health report. The observable
//! surface a guest can actually probe is exactly its granted `jinn:fs`
//! scope — no process, no directory listing (FINDINGS.md #3, #5), and no
//! clock of its own (time is the scheduler's business); the report is
//! honest about being that.
//!
//! The cron-plane peek (`jinn:cron` resolve + `jobs` call) happens in
//! `activate` and ONLY there: calling back into the scheduler while
//! handling a fire deadlocks the seam until the kernel's guest deadline
//! kills the call (FINDINGS.md #4).

use std::sync::Mutex;

use jinn_cron::{FirePayload, CRON_CONTRACT, OP_JOBS};
use serde::Deserialize;

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::{effects, events, fs, services};

const EFFECT_TOKEN: u64 = 1;
const LISTEN_TOKEN: u64 = 2;

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
        )
        .map_err(|error| fault("boot write", error))?;
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
        // The probe: write, read back, compare — the writability check the
        // granted scope actually supports.
        let probe_path = format!("{dir}/probe.txt");
        let probe = fire.tick_seq.to_string().into_bytes();
        fs::write(&probe_path, &probe).map_err(|error| fault("probe write", error))?;
        let probe_ok = fs::read(&probe_path).is_ok_and(|read| read == probe);
        let report_path = format!("{dir}/report.json");
        let fires = fs::read(&report_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .and_then(|report| report["fires"].as_u64())
            .unwrap_or(0);
        let report = serde_json::json!({
            "fires": fires + 1,
            "probe-ok": probe_ok,
            "last": fire,
        });
        fs::write(
            &report_path,
            &serde_json::to_vec_pretty(&report).expect("report encodes"),
        )
        .map_err(|error| fault("report write", error))?;
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
