//! The `jinn:api-status` consumer. Answers `status`, `health`, and
//! `ledger-tail` from the kernel's own knowledge: the composition through
//! the granted `jinn:introspect` (every entry's fiber, state,
//! incarnation, provisions, registrations, and the daemon's readiness —
//! FINDINGS.md #19 closed), the ledger through the granted `jinn:ledger`
//! reader (paged, receipted — #20 closed), the document of record's
//! authority fields through the granted `jinn:profile` `document` read,
//! provider probes through granted contracts (a `resolve` + one read
//! call), and the engines the composition holds — read off the kernel's
//! own `provisions`, never a table this plugin keeps. Each is a ledgered
//! crossing (Law 2); nothing is guessed.
//!
//! Since pin `3fd7b05` (jinnd M2-K8) that document read is `jinn:profile`
//! `document` under an `ops = ["document"]` READ-ONLY grant, not a
//! `jinn:fs` read of the file: the entry holds no path scope on the
//! document and no write authority over it at all. That retires both the
//! data-root coupling (#25 — the document is now readable wherever it
//! sits) and the excess write authority every viewer used to carry (#24).
//! A viewer mounted without the grant still ANSWERS: the read is typed
//! `unavailable`, never a fault.
//!
//! Probes and kernel reads happen inside `handle-call` (the caller is
//! the HTTP provider, a third instance; the kernel providers never call
//! a guest): not reentrant on the caller, so the FINDINGS.md #4 deadlock
//! shape does not arise.

use std::sync::Mutex;

use jinn_api::{
    decode_last_seq, decode_profile_document, entries_status, ledger_read_range_payload,
    merge_introspection, normalize_tail, Answer, ApiError, DocumentStatus, EngineSlot, EntryStatus,
    ErrorCode, HealthReport, IntrospectEntry, KernelIntrospection, LedgerPage, LedgerTail,
    LedgerTailRequest, ProbeReport, ProbeSpec, Readiness, StatusReport, API_VERSION,
    FINDING_NO_DOCUMENT_READ, INTROSPECT_CONTRACT, KERNEL_PROFILE_CONTRACT, LEDGER_CONTRACT,
    OP_HEALTH, OP_INTROSPECT_ENTRIES, OP_INTROSPECT_READINESS, OP_KERNEL_DOCUMENT,
    OP_LEDGER_LAST_SEQ, OP_LEDGER_READ_RANGE, OP_LEDGER_TAIL, OP_STATUS, STATUS_CONTRACT,
};
use serde::Deserialize;

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::{effects, services};

const EFFECT_TOKEN: u64 = 1;

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct StatusConfig {
    /// Provider probes to run on every `status`/`health`.
    #[serde(default)]
    probes: Vec<ProbeSpec>,
}

static CONFIG: Mutex<Option<StatusConfig>> = Mutex::new(None);

fn config() -> StatusConfig {
    CONFIG.lock().unwrap().clone().unwrap_or_default()
}

fn fault(context: &str, error: impl std::fmt::Debug) -> GuestFault {
    GuestFault::Failed(format!("{context}: {error:?}"))
}

/// One granted kernel read: resolve, call, decode the JSON answer.
fn kernel_read<T: serde::de::DeserializeOwned>(
    contract: &str,
    operation: &str,
    payload: &[u8],
) -> Result<T, ApiError> {
    let handle = services::resolve(contract)
        .map_err(|error| ApiError::new(ErrorCode::Refused, format!("{contract}: {error:?}")))?;
    let bytes = services::call(handle, operation, payload).map_err(|error| {
        ApiError::new(
            ErrorCode::Refused,
            format!("{contract}/{operation}: {error:?}"),
        )
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        ApiError::new(
            ErrorCode::Invalid,
            format!("{contract}/{operation}: malformed answer: {error}"),
        )
    })
}

/// The document of record through the kernel's own `jinn:profile`
/// `document` read — the entries this entry's scope admits, wherever the
/// document sits (FINDINGS.md #25 closed). A grant this entry does not
/// hold, or one whose scope or `ops` refuse the read, is the typed
/// `unavailable` answer: a status consumer mounted without the read still
/// reports the kernel's view, with the authority fields empty and the
/// reason stated. A malformed answer is `invalid` — a distinct thing.
fn profile_document() -> Result<serde_json::Value, ApiError> {
    let handle = services::resolve(KERNEL_PROFILE_CONTRACT).map_err(|error| {
        ApiError::unavailable(
            FINDING_NO_DOCUMENT_READ,
            format!("{KERNEL_PROFILE_CONTRACT} is not resolvable from this entry: {error:?}"),
        )
    })?;
    let bytes = services::call(handle, OP_KERNEL_DOCUMENT, &[]).map_err(|error| {
        ApiError::unavailable(
            FINDING_NO_DOCUMENT_READ,
            format!("{KERNEL_PROFILE_CONTRACT}/{OP_KERNEL_DOCUMENT} refused: {error:?}"),
        )
    })?;
    decode_profile_document(&bytes)
        .map_err(|error| ApiError::new(ErrorCode::Invalid, format!("profile: {error}")))
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

/// The entries: the document's authority fields where the document is
/// readable, with the kernel's view laid over them by id — and, from
/// that same view, the engines this composition holds. An engine is in
/// the list because an ENTRY PROVIDES its `jinn:engine.<id>` contract
/// (`jinn_engine::engines_in`), so it appears when the entry is mounted
/// and is gone when the entry is: the coexistence and extension proofs
/// are observable through the API without this plugin keeping a table of
/// its own. The contract's one home is
/// `plugins/engines/jinn-engine/README.md`.
fn entries() -> (Vec<EntryStatus>, DocumentStatus, Vec<EngineSlot>) {
    let (mut entries, document) =
        match profile_document().and_then(|document| entries_status(&document)) {
            Ok(entries) => (
                entries,
                DocumentStatus {
                    readable: true,
                    ..DocumentStatus::default()
                },
            ),
            Err(error) => (
                Vec::new(),
                DocumentStatus {
                    readable: false,
                    unavailable: Some(error),
                    extra: jinn_api::Extensions::new(),
                },
            ),
        };
    let mut engines = Vec::new();
    if let Ok(kernel) =
        kernel_read::<Vec<IntrospectEntry>>(INTROSPECT_CONTRACT, OP_INTROSPECT_ENTRIES, &[])
    {
        engines = jinn_engine::engines_in(kernel.iter().map(|entry| {
            (
                entry.id.as_str(),
                entry.provisions.iter().map(String::as_str),
            )
        }));
        merge_introspection(&mut entries, &kernel);
    }
    (entries, document, engines)
}

fn last_seq() -> Option<u64> {
    let handle = services::resolve(LEDGER_CONTRACT).ok()?;
    decode_last_seq(&services::call(handle, OP_LEDGER_LAST_SEQ, &[]).ok()?)
}

fn status(config: &StatusConfig) -> StatusReport {
    let (entries, document, engines) = entries();
    StatusReport {
        api_version: API_VERSION.to_owned(),
        entries,
        probes: config.probes.iter().map(probe).collect(),
        kernel: KernelIntrospection::at_this_pin(),
        readiness: kernel_read::<Readiness>(INTROSPECT_CONTRACT, OP_INTROSPECT_READINESS, &[]).ok(),
        last_ledger_seq: last_seq(),
        document,
        engines,
        extra: jinn_api::Extensions::new(),
    }
}

/// `ok` iff the kernel lists every entry Active, every probe is live,
/// and the document is either readable or honestly out of reach.
fn health(config: &StatusConfig) -> HealthReport {
    let (entries, document, _) = entries();
    let probes: Vec<ProbeReport> = config.probes.iter().map(probe).collect();
    let live = probes.iter().filter(|report| report.live).count();
    let all_active = !entries.is_empty()
        && entries
            .iter()
            .all(|entry| entry.state.as_deref() == Some("active"));
    HealthReport {
        api_version: API_VERSION.to_owned(),
        ok: all_active && live == probes.len(),
        profile_readable: document.readable,
        entries: entries.len(),
        probes_live: live,
        probes_total: probes.len(),
        extra: jinn_api::Extensions::new(),
    }
}

/// One page of the kernel's ledger through the granted reader: events
/// with `id > after`, at most `limit`; the read is receipted on the
/// ledger under this entry (`LedgerConsumed`).
fn ledger_tail(payload: &[u8]) -> Result<LedgerTail, ApiError> {
    let request: LedgerTailRequest = serde_json::from_slice(payload)
        .map_err(|error| ApiError::new(ErrorCode::Invalid, format!("ledger-tail: {error}")))?;
    let request = normalize_tail(request);
    let mut tail = LedgerTail {
        api_version: API_VERSION.to_owned(),
        after: request.after,
        limit: request.limit,
        ..LedgerTail::default()
    };
    match kernel_read::<LedgerPage>(
        LEDGER_CONTRACT,
        OP_LEDGER_READ_RANGE,
        &ledger_read_range_payload(request.after.saturating_add(1), request.limit),
    ) {
        Ok(page) => {
            tail.next_after = (page.events.len() as u32 == request.limit)
                .then(|| page.next_from.saturating_sub(1));
            tail.events = page.events;
        }
        Err(error) => tail.unavailable = Some(error),
    }
    Ok(tail)
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
            OP_STATUS => Answer::ok(serde_json::to_value(status(&config)).expect("encodes")),
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
