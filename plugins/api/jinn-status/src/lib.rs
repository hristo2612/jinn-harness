//! The `jinn:api-status` consumer. Answers `status`, `health`, and
//! `ledger-tail` from what a guest can honestly reach at this kernel pin:
//! the profile document (read through the granted `jinn:fs` scope — the
//! same file the daemon reconciles, so the entries shown are the entries
//! of record) and provider probes (a granted `resolve`, optionally one
//! read call, through the broker — each a ledgered crossing). The kernel
//! exposes no introspection contract (fiber state/uid, provisions,
//! listeners, alarms, readiness — FINDINGS.md #19) and no live
//! `jinn:ledger` reader (#20): those fields are answered BY NAME as
//! unavailable, with the finding number, never guessed.
//!
//! Probes happen inside `handle-call` (the caller is the HTTP provider, a
//! third instance): calling a provider from here is not reentrant on the
//! caller, so the FINDINGS.md #4 deadlock shape does not arise.

use std::sync::Mutex;

use jinn_api::{
    entries_status, normalize_tail, Answer, ApiError, ErrorCode, HealthReport, KernelIntrospection,
    LedgerTail, LedgerTailRequest, ProbeReport, ProbeSpec, StatusReport, API_VERSION,
    FINDING_NO_LEDGER_READER, OP_HEALTH, OP_LEDGER_TAIL, OP_STATUS, STATUS_CONTRACT,
};
use serde::Deserialize;

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::{effects, fs, services};

const EFFECT_TOKEN: u64 = 1;

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct StatusConfig {
    /// The profile document, under the granted `jinn:fs` scope.
    #[serde(default = "default_profile_path")]
    profile_path: String,
    /// Provider probes to run on every `status`/`health`.
    #[serde(default)]
    probes: Vec<ProbeSpec>,
}

fn default_profile_path() -> String {
    "profile.json".into()
}

static CONFIG: Mutex<Option<StatusConfig>> = Mutex::new(None);

fn config() -> StatusConfig {
    CONFIG.lock().unwrap().clone().unwrap_or_default()
}

fn fault(context: &str, error: impl std::fmt::Debug) -> GuestFault {
    GuestFault::Failed(format!("{context}: {error:?}"))
}

/// The profile document of record, typed: absent and refused are
/// distinct answers, never a folded message.
fn profile(path: &str) -> Result<serde_json::Value, ApiError> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|error| ApiError::new(ErrorCode::Invalid, format!("profile: {error}"))),
        Err(fs::FsError::NotFound) => Err(ApiError::new(
            ErrorCode::NotFound,
            format!("profile document {path:?} is absent"),
        )),
        Err(refused) => Err(ApiError::new(
            ErrorCode::Refused,
            format!("profile read refused: {refused:?}"),
        )),
    }
}

/// One probe: resolve the granted contract, then the optional read call.
fn probe(spec: &ProbeSpec) -> ProbeReport {
    let mut report = ProbeReport {
        contract: spec.contract.clone(),
        operation: spec.operation.clone(),
        ..ProbeReport::default()
    };
    let handle = match services::resolve(&spec.contract) {
        Ok(handle) => handle,
        Err(error) => {
            report.refused = Some(format!("{error:?}"));
            return report;
        }
    };
    match &spec.operation {
        None => report.live = true,
        Some(operation) => match services::call(handle, operation, &[]) {
            Ok(bytes) => {
                report.live = true;
                report.answer = Some(
                    serde_json::from_slice(&bytes)
                        .unwrap_or_else(|_| serde_json::json!({ "raw-bytes": bytes.len() })),
                );
            }
            Err(error) => report.refused = Some(format!("{error:?}")),
        },
    }
    report
}

fn status(config: &StatusConfig) -> Result<StatusReport, ApiError> {
    let entries = entries_status(&profile(&config.profile_path)?)?;
    Ok(StatusReport {
        api_version: API_VERSION.to_owned(),
        entries,
        probes: config.probes.iter().map(probe).collect(),
        kernel: KernelIntrospection::at_this_pin(),
        extra: jinn_api::Extensions::new(),
    })
}

fn health(config: &StatusConfig) -> HealthReport {
    let entries = profile(&config.profile_path).and_then(|document| entries_status(&document));
    let probes: Vec<ProbeReport> = config.probes.iter().map(probe).collect();
    let live = probes.iter().filter(|report| report.live).count();
    HealthReport {
        api_version: API_VERSION.to_owned(),
        ok: entries.is_ok() && live == probes.len(),
        profile_readable: entries.is_ok(),
        entries: entries.map(|entries| entries.len()).unwrap_or(0),
        probes_live: live,
        probes_total: probes.len(),
        extra: jinn_api::Extensions::new(),
    }
}

/// No `jinn:ledger` provider is live at this pin (FINDINGS.md #20): the
/// page is empty and says why, typed. The request itself is still a
/// ledgered contract call — the operator's read intent is on the record.
fn ledger_tail(payload: &[u8]) -> Result<LedgerTail, ApiError> {
    let request: LedgerTailRequest = serde_json::from_slice(payload)
        .map_err(|error| ApiError::new(ErrorCode::Invalid, format!("ledger-tail: {error}")))?;
    let request = normalize_tail(request);
    Ok(LedgerTail {
        api_version: API_VERSION.to_owned(),
        after: request.after,
        limit: request.limit,
        events: Vec::new(),
        next_after: None,
        unavailable: Some(ApiError::unavailable(
            FINDING_NO_LEDGER_READER,
            "the kernel provides no jinn:ledger reader; the ledger is readable only beside the daemon",
        )),
        extra: jinn_api::Extensions::new(),
    })
}

fn answer<T: serde::Serialize>(result: Result<T, ApiError>) -> Answer {
    match result {
        Ok(value) => Answer::ok(serde_json::to_value(value).expect("an answer encodes")),
        Err(error) => Answer::error(error),
    }
}

struct Status;

impl Guest for Status {
    fn activate(config: Vec<u8>) -> Result<(), GuestFault> {
        let parsed: StatusConfig = serde_json::from_slice(&config)
            .map_err(|error| GuestFault::Failed(format!("malformed config: {error}")))?;
        *CONFIG.lock().unwrap() = Some(parsed);
        effects::register("jinn-status on duty", EFFECT_TOKEN)
            .map_err(|error| fault("effect", error))?;
        services::provide(STATUS_CONTRACT).map_err(|error| fault("provide", error))?;
        Ok(())
    }

    fn check(_consumer: u64) -> bool {
        true
    }

    fn undo(_token: u64) -> Result<(), GuestFault> {
        Ok(())
    }

    fn handle_event(token: u64, topic: String, payload: Vec<u8>) -> Result<Vec<u8>, GuestFault> {
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
        let config = config();
        let answered = match operation.as_str() {
            OP_STATUS => answer(status(&config)),
            OP_HEALTH => Answer::ok(serde_json::to_value(health(&config)).expect("encodes")),
            OP_LEDGER_TAIL => answer(ledger_tail(&payload)),
            other => Answer::error(ApiError::new(
                ErrorCode::NotFound,
                format!("unknown operation {other:?}"),
            )),
        };
        Ok(answered.encode())
    }

    fn snapshot() -> Vec<u8> {
        Vec::new()
    }

    fn restore(_blob: Vec<u8>) -> Result<(), GuestFault> {
        Ok(())
    }
}

export!(Status);
