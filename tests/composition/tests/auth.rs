//! THE DOOR (harness packet 2.8, PLA-343): `jinn-api-http` calls the
//! pinned kernel's `jinn:auth` `verify` with the credential the connection
//! presented BEFORE issuing any dispatch on that connection's behalf —
//! one call per request, no grant cached across requests. The kernel
//! supplies the authority (M2-K21: one credential, one decision point,
//! every decision an `AuthDecided` row, deny by default); the transport
//! owes the check, and this suite is where that obligation is PROVEN
//! (the contract's own "WHAT A TRANSPORT OWES" paragraph names it).
//!
//! Every proof boots the operator profile through the REAL pinned daemon
//! (AGENTS.md standing order 3), with the launcher's half provisioned by
//! the rig — `<data>.operator-token`, 0600 — and drives the API over
//! loopback as an operator's `curl` would. Evidence is the wire (parsed
//! into the seam's own types) and the ledger (parsed row by row).
//!
//! Self-skips LOUDLY when no jinnd checkout holding the pinned commit is
//! reachable (KERNEL-PIN.md Gate 2).

use std::path::PathBuf;
use std::sync::OnceLock;

use composition::api::{get, request_as, Response};
use composition::daemon::{jinnd_source, pinned_commit, pinned_daemon};
use composition::kit::{
    fresh_api_root, revoke_credential, rotate_credential, suite_credential, Daemon, LedgerRow,
};
use jinn_api::{ApiError, ErrorCode, AUTH_CONTRACT, OP_VERIFY};
use sha2::{Digest, Sha256};

const PROVIDER: &str = "jinn-api-http";
/// A credential that is not the operator's. Shaped like one so the only
/// thing wrong with it is its value.
const WRONG: &str = "not-the-operator-credential-0xBADC0FFEE-wrong";
/// A credential the operator rotates TO.
const ROTATED: &str = "rotated-operator-credential-0xCAFEBABE-the-second";

/// The pinned daemon binary, or a LOUD skip.
fn gate() -> Option<&'static PathBuf> {
    static BINARY: OnceLock<Option<PathBuf>> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            let commit = pinned_commit().expect("KERNEL-PIN.md parses");
            let Some(source) = jinnd_source(&commit) else {
                eprintln!(
                    "SKIPPED (loudly): real-composition gate found no jinnd checkout holding \
                     pinned commit {commit} — set JINND_DIR, add a sibling ../jinnd, or set \
                     JINND_CLONE_URL (KERNEL-PIN.md Gate 2 discipline)"
                );
                return None;
            };
            Some(pinned_daemon(&source, &commit).expect("the pinned daemon builds"))
        })
        .as_ref()
}

/// Boots a fresh operator root, waits for readiness and for the API's
/// first GRANTED answer, and waits for that request's close to reach the
/// ledger so a proof's baseline sits after it.
fn booted(name: &str) -> Option<(Daemon, u16)> {
    let binary = gate()?;
    let (root, port) = fresh_api_root(name);
    let daemon = Daemon::boot_operator(binary, &root);
    daemon.await_ready();
    assert!(
        daemon.credential().is_file(),
        "the launcher's half: the credential of record is provisioned beside the data root"
    );
    let health = get(port, "/v1/health");
    assert_eq!(health.status, 200, "{}", health.raw);
    daemon.eventually("the boot request's close to land", || {
        daemon.ledger_count("NetClosed") >= 1
    });
    Some((daemon, port))
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// One ledger row's kind, PARSED: the variant name and its fields.
fn kind_of(row: &LedgerRow) -> (String, serde_json::Value) {
    match serde_json::from_str::<serde_json::Value>(&row.kind) {
        Ok(serde_json::Value::Object(object)) if object.len() == 1 => {
            let (name, fields) = object.into_iter().next().expect("one key");
            (name, fields)
        }
        Ok(serde_json::Value::String(unit)) => (unit, serde_json::Value::Null),
        _ => (row.kind.clone(), serde_json::Value::Null),
    }
}

/// One `AuthDecided` row's fields, parsed.
#[derive(Debug, PartialEq, Eq)]
struct Decision {
    seq: u64,
    entry: Option<String>,
    name: Option<String>,
    presented: String,
    granted: bool,
}

fn decision(row: &LedgerRow) -> Option<Decision> {
    let (name, fields) = kind_of(row);
    (name == "AuthDecided").then(|| Decision {
        seq: row.seq,
        entry: row.entry.clone(),
        name: fields["name"].as_str().map(str::to_owned),
        presented: fields["presented"]
            .as_str()
            .expect("presented is a digest string")
            .to_owned(),
        granted: fields["granted"].as_bool().expect("granted is a bool"),
    })
}

fn is_call(row: &LedgerRow, contract: &str, operation: &str) -> bool {
    let (name, fields) = kind_of(row);
    name == "ContractCall" && fields["contract"] == contract && fields["operation"] == operation
}

/// A row that is the CONNECTION's own: its accept, readiness wakes and
/// close, and the provider's `jinn:net` host calls (accept, read, write,
/// close) that serve it — bytes moving, never a dispatch on the
/// connection's behalf.
fn is_transport(row: &LedgerRow) -> bool {
    let (name, fields) = kind_of(row);
    match name.as_str() {
        "NetAccepted" | "NetReadable" | "NetClosed" => true,
        "ContractCall" => fields["contract"] == "jinn:net",
        _ => false,
    }
}

/// A row that is the DOOR's own: the `jinn:auth` resolve, the one
/// `verify` crossing, and the kernel's decision.
fn is_door(row: &LedgerRow) -> bool {
    let (name, fields) = kind_of(row);
    match name.as_str() {
        "AuthDecided" => true,
        "ContractResolved" => fields["contract"] == AUTH_CONTRACT,
        "ContractCall" => fields["contract"] == AUTH_CONTRACT && fields["operation"] == OP_VERIFY,
        _ => false,
    }
}

fn rows_after(daemon: &Daemon, seq: u64) -> Vec<LedgerRow> {
    daemon
        .ledger_rows()
        .into_iter()
        .filter(|row| row.seq > seq)
        .collect()
}

fn last_seq(daemon: &Daemon) -> u64 {
    daemon.ledger_rows().last().map_or(0, |row| row.seq)
}

/// The rows the PROVIDER's entry is charged with, after `baseline`,
/// split into one segment per accepted connection: from its
/// `NetAccepted { handle }` to the `NetClosed` of that same handle (the
/// guest is single-threaded, so one request's rows are contiguous among
/// its own; the listener's own wake and accept between two connections
/// belong to no request and are left out).
fn provider_segments(rows: &[LedgerRow]) -> Vec<Vec<&LedgerRow>> {
    let mut segments: Vec<Vec<&LedgerRow>> = Vec::new();
    let mut open: Option<u64> = None;
    for row in rows
        .iter()
        .filter(|row| row.entry.as_deref() == Some(PROVIDER))
    {
        let (name, fields) = kind_of(row);
        if name == "NetAccepted" {
            open = fields["handle"].as_u64();
            segments.push(vec![row]);
            continue;
        }
        let Some(handle) = open else { continue };
        segments.last_mut().expect("an open segment").push(row);
        if name == "NetClosed" && fields["handle"].as_u64() == Some(handle) {
            open = None;
        }
    }
    segments
}

/// The typed refusal, PARSED off the wire into the seam's own error type:
/// its own class, the challenge header, the versioned envelope, no
/// credential bytes and no answer content.
fn assert_refused(what: &str, answer: &Response, never: &[&str]) -> ApiError {
    assert_eq!(answer.status, 401, "{what}: {}", answer.raw);
    assert_eq!(
        answer.header("www-authenticate"),
        Some("Bearer"),
        "{what}: the challenge names the carrier: {}",
        answer.raw
    );
    let error: ApiError = serde_json::from_value(answer.body["error"].clone())
        .unwrap_or_else(|error| panic!("{what}: a typed error envelope ({error}): {}", answer.raw));
    assert_eq!(
        error.code,
        ErrorCode::Unauthenticated,
        "{what}: its own class, not the allowlist's and not the transport's"
    );
    assert!(!error.detail.is_empty(), "{what}: the kernel's reason");
    assert_eq!(
        answer.body["api-version"],
        jinn_api::API_VERSION,
        "{what}: versioned like every answer"
    );
    assert!(
        answer
            .body
            .as_object()
            .is_some_and(|object| object.len() == 2),
        "{what}: the refusal carries nothing but its envelope: {}",
        answer.raw
    );
    for secret in never {
        assert!(
            !answer.raw.contains(secret),
            "{what}: no credential bytes on the wire"
        );
    }
    error
}

#[test]
fn no_credential_and_a_wrong_credential_are_refused_typed_with_zero_effects_and_one_decision_each()
{
    let Some((daemon, port)) = booted("auth-refused") else {
        return;
    };
    let baseline = last_seq(&daemon);
    let never = [WRONG, suite_credential()];

    let none = request_as(port, "GET", "/v1/status", None, None);
    let wrong = request_as(port, "GET", "/v1/status", None, Some(WRONG));
    // A write is refused the same way: the body never reaches a consumer.
    let write = request_as(
        port,
        "PATCH",
        "/v1/profile/entries/cron-scheduler",
        Some(r#"{"config":{"data":{"tick-ms":123}}}"#),
        Some(WRONG),
    );
    assert_refused("no credential", &none, &never);
    assert_refused("a wrong credential", &wrong, &never);
    assert_refused("a wrong credential on a write", &write, &never);

    daemon.eventually("the refused connections to close on the ledger", || {
        rows_after(&daemon, baseline)
            .iter()
            .filter(|row| kind_of(row).0 == "NetClosed")
            .count()
            >= 3
    });
    let after = rows_after(&daemon, baseline);

    // Exactly ONE `AuthDecided { granted: false }` per request, under the
    // provider's entry, carrying the DIGEST of what was presented — the
    // empty string's for nothing presented, the wrong value's otherwise —
    // and never a name.
    let decisions: Vec<Decision> = after.iter().filter_map(decision).collect();
    assert_eq!(
        decisions.len(),
        3,
        "one decision per request: {decisions:?}"
    );
    for decided in &decisions {
        assert_eq!(decided.entry.as_deref(), Some(PROVIDER), "{decided:?}");
        assert!(!decided.granted, "{decided:?}");
        assert_eq!(decided.name, None, "no principal on a refusal: {decided:?}");
    }
    assert_eq!(
        decisions[0].presented,
        hex_sha256(b""),
        "nothing presented is put to the kernel as nothing: {decisions:?}"
    );
    assert_eq!(decisions[1].presented, hex_sha256(WRONG.as_bytes()));
    assert_eq!(decisions[2].presented, hex_sha256(WRONG.as_bytes()));

    // ZERO effects attributable to the requests: among everything the
    // provider's entry is charged with, each connection is exactly its
    // transport (accept, wakes, the `jinn:net` reads/writes/close) and
    // its door (the `jinn:auth` resolve, ONE `verify` crossing, ONE
    // decision). No consumer crossing, no other resolve, no effect, no
    // fs, no process — every row is one or the other, and nothing else.
    let segments = provider_segments(&after);
    assert_eq!(segments.len(), 3, "one segment per connection: {after:?}");
    for segment in &segments {
        let kinds: Vec<&str> = segment.iter().map(|row| row.kind.as_str()).collect();
        assert!(
            segment.iter().all(|row| is_transport(row) || is_door(row)),
            "nothing but the connection and the door: {kinds:#?}"
        );
        let crossings: Vec<&&LedgerRow> = segment
            .iter()
            .filter(|row| kind_of(row).0 == "ContractCall" && !is_transport(row))
            .collect();
        assert_eq!(crossings.len(), 1, "exactly one crossing: {kinds:#?}");
        assert!(
            is_call(crossings[0], AUTH_CONTRACT, OP_VERIFY),
            "and it is the verify: {}",
            crossings[0].kind
        );
        assert_eq!(
            segment.iter().filter(|row| decision(row).is_some()).count(),
            1,
            "exactly one decision: {kinds:#?}"
        );
        assert_eq!(
            kind_of(segment.last().expect("a row")).0,
            "NetClosed",
            "the connection closed on the refusal: {kinds:#?}"
        );
    }
    assert!(
        !after.iter().any(|row| {
            row.entry.as_deref() == Some(PROVIDER)
                && matches!(
                    kind_of(row).0.as_str(),
                    "EffectRegistered" | "FsWrite" | "FsRead" | "ProcessSpawned" | "ProfilePatched"
                )
        }),
        "no effect and no world mutation on the provider's account: {after:?}"
    );
    assert_eq!(
        daemon.ledger_count("ProfilePatched"),
        0,
        "the refused write never reached the editor"
    );

    // No credential bytes anywhere: not in a ledger row, not in the log.
    let ledger = daemon.ledger_kinds().join("\n");
    let log = daemon.log();
    for secret in never {
        assert!(!ledger.contains(secret), "the ledger carries no credential");
        assert!(
            !log.contains(secret),
            "the daemon log carries no credential"
        );
    }
    daemon.interrupt();
}

#[test]
fn the_right_credential_is_granted_and_every_request_is_one_verify_before_its_dispatch() {
    let Some((daemon, port)) = booted("auth-granted") else {
        return;
    };
    let baseline = last_seq(&daemon);
    let requests = [
        ("GET", "/v1/health", "jinn:api-status", "health"),
        ("GET", "/v1/status", "jinn:api-status", "status"),
        ("GET", "/v1/profile", "jinn:api-profile", "get"),
    ];
    for (method, target, _, _) in requests {
        let answer = request_as(port, method, target, None, Some(suite_credential()));
        assert_eq!(answer.status, 200, "{method} {target}: {}", answer.raw);
    }
    daemon.eventually("the granted connections to close on the ledger", || {
        rows_after(&daemon, baseline)
            .iter()
            .filter(|row| kind_of(row).0 == "NetClosed")
            .count()
            >= requests.len()
    });
    let after = rows_after(&daemon, baseline);

    // One `AuthDecided { granted: true, name: operator }` per decision,
    // and one decision per request — the grant is never cached.
    let decisions: Vec<Decision> = after.iter().filter_map(decision).collect();
    assert_eq!(decisions.len(), requests.len(), "{decisions:?}");
    for decided in &decisions {
        assert_eq!(decided.entry.as_deref(), Some(PROVIDER), "{decided:?}");
        assert!(decided.granted, "{decided:?}");
        assert_eq!(decided.name.as_deref(), Some("operator"), "{decided:?}");
        assert_eq!(
            decided.presented,
            hex_sha256(suite_credential().as_bytes()),
            "the digest of what was presented, never the bytes"
        );
    }

    // Per connection: the verify crossing and its decision come BEFORE
    // the consumer crossing — ordering on the record, not a claim.
    let segments = provider_segments(&after);
    assert_eq!(segments.len(), requests.len(), "{after:?}");
    for (segment, (_, target, contract, operation)) in segments.iter().zip(requests) {
        let verify = segment
            .iter()
            .find(|row| is_call(row, AUTH_CONTRACT, OP_VERIFY))
            .unwrap_or_else(|| panic!("{target}: a verify crossing"));
        let decided = segment
            .iter()
            .find(|row| decision(row).is_some())
            .unwrap_or_else(|| panic!("{target}: a decision"));
        let dispatch = segment
            .iter()
            .find(|row| is_call(row, contract, operation))
            .unwrap_or_else(|| panic!("{target}: the consumer crossing"));
        assert!(
            verify.seq < decided.seq && decided.seq < dispatch.seq,
            "{target}: verify ({}) < decided ({}) < dispatch ({})",
            verify.seq,
            decided.seq,
            dispatch.seq
        );
        assert_eq!(
            segment
                .iter()
                .filter(|row| is_call(row, AUTH_CONTRACT, OP_VERIFY))
                .count(),
            1,
            "{target}: one verify per request"
        );
    }
    let ledger = daemon.ledger_kinds().join("\n");
    assert!(
        !ledger.contains(suite_credential()),
        "the credential's bytes are in no ledger row"
    );
    assert!(
        !daemon.log().contains(suite_credential()),
        "the credential's bytes are in no log line"
    );
    daemon.interrupt();
}

#[test]
fn rotation_and_revocation_take_effect_on_the_next_request_without_a_restart() {
    let Some((daemon, port)) = booted("auth-rotated") else {
        return;
    };
    let baseline = last_seq(&daemon);
    let data_root = daemon.data_root.clone();
    let never = [WRONG, ROTATED, suite_credential()];

    // ROTATION: overwrite the credential of record; the old value refuses
    // and the new one grants from the very next request.
    rotate_credential(&data_root, ROTATED);
    let stale = request_as(port, "GET", "/v1/health", None, Some(suite_credential()));
    let mismatch = assert_refused("the rotated-away credential", &stale, &never);
    let fresh = request_as(port, "GET", "/v1/health", None, Some(ROTATED));
    assert_eq!(fresh.status, 200, "{}", fresh.raw);
    assert_eq!(fresh.body["ok"], true, "{}", fresh.raw);

    // REVOCATION: delete it; everything refuses from the next request on,
    // and the reason names a DIFFERENT precondition than a mismatch does.
    revoke_credential(&data_root);
    let revoked = request_as(port, "GET", "/v1/health", None, Some(ROTATED));
    let absent = assert_refused("a revoked credential", &revoked, &never);
    assert_ne!(
        absent.detail, mismatch.detail,
        "the reason names WHICH precondition failed"
    );
    let anything = request_as(port, "GET", "/v1/health", None, Some(suite_credential()));
    assert_refused("anything after revocation", &anything, &never);

    // RESTORE: the operator provisions again; the door opens again.
    rotate_credential(&data_root, suite_credential());
    let restored = request_as(port, "GET", "/v1/health", None, Some(suite_credential()));
    assert_eq!(restored.status, 200, "{}", restored.raw);

    // No restart was involved: no fiber transitioned after the baseline,
    // the provider's incarnation is the boot's, and the reconcile never
    // ran (the credential is not the profile). Five requests since the
    // baseline: stale, fresh, revoked, anything, restored.
    daemon.eventually("the last connection to close on the ledger", || {
        rows_after(&daemon, baseline)
            .iter()
            .filter(|row| kind_of(row).0 == "NetClosed")
            .count()
            >= 5
    });
    let after = rows_after(&daemon, baseline);
    assert!(
        !after.iter().any(|row| matches!(
            kind_of(row).0.as_str(),
            "FiberTransition" | "FiberSuspended"
        )),
        "no fiber cycled: {after:?}"
    );
    assert_eq!(daemon.restart_count(PROVIDER), 0);
    assert_eq!(
        daemon.log_count("reconciled"),
        1,
        "only the boot reconcile ever ran: {}",
        daemon.log()
    );
    let decisions: Vec<Decision> = after.iter().filter_map(decision).collect();
    assert_eq!(
        decisions.iter().map(|d| d.granted).collect::<Vec<_>>(),
        [false, true, false, false, true],
        "one decision per request, each answering the file AS IT IS NOW: {decisions:?}"
    );
    let ledger = daemon.ledger_kinds().join("\n");
    let log = daemon.log();
    for secret in never {
        assert!(!ledger.contains(secret), "the ledger carries no credential");
        assert!(
            !log.contains(secret),
            "the daemon log carries no credential"
        );
    }
    daemon.interrupt();
}
