//! The kernel operator contracts' wire shapes (pin `57360cc`, jinnd
//! M2-K7), as pure encode/decode so both guests and host tests share one
//! reading of each bundle: `jinn:introspect` (JSON answers), `jinn:ledger`
//! (`u64-LE from-id ++ u32-LE limit` → JSON page; `last-seq` → `u64-LE`),
//! and `jinn:profile` (`u32-LE id length ++ id ++ merge-patch JSON` →
//! one tag byte, `0` applied / `1` refused, then the reason's UTF-8).

use serde::{Deserialize, Serialize};

use crate::Extensions;

/// The kernel's read-only composition contract (FINDINGS.md #19 closed).
pub const INTROSPECT_CONTRACT: &str = "jinn:introspect";
/// `jinn:introspect` operation: every entry as the kernel runs it.
pub const OP_INTROSPECT_ENTRIES: &str = "entries";
/// `jinn:introspect` operation: the daemon's readiness.
pub const OP_INTROSPECT_READINESS: &str = "readiness";
/// The kernel's ledger reader (FINDINGS.md #20 closed).
pub const LEDGER_CONTRACT: &str = "jinn:ledger";
/// `jinn:ledger` operation: one page of events.
pub const OP_LEDGER_READ_RANGE: &str = "read-range";
/// `jinn:ledger` operation: the highest committed sequence.
pub const OP_LEDGER_LAST_SEQ: &str = "last-seq";
/// The kernel's profile-patch contract (FINDINGS.md #21 closed).
pub const KERNEL_PROFILE_CONTRACT: &str = "jinn:profile";
/// `jinn:profile` operation: merge-patch ONE entry's config, applied by
/// the loader as operator intent.
pub const OP_KERNEL_PATCH_ENTRY: &str = "patch-entry";
/// The `jinn:profile` grant an editor holds: every entry, written out
/// (a bare grant patches nothing — the bundle's fail-closed scope).
pub const KERNEL_PROFILE_SCOPE_ALL: &str = "*";

/// One entry as `jinn:introspect` reports it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct IntrospectEntry {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fiber: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incarnation: Option<u64>,
    #[serde(default)]
    pub provisions: Vec<String>,
    #[serde(default)]
    pub registrations: Registrations,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The kernel registrations one live seat holds, by class.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Registrations {
    #[serde(default)]
    pub listeners: u32,
    #[serde(default)]
    pub alarms: u32,
    #[serde(default)]
    pub sockets: u32,
    #[serde(default)]
    pub processes: u32,
}

/// The daemon's readiness as `jinn:introspect` reports it.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Readiness {
    #[serde(default)]
    pub boot_reconciled: bool,
    #[serde(default)]
    pub watcher_armed: bool,
}

/// One ledger event as `jinn:ledger` delivers it: `payload` is the kind's
/// fields as JSON TEXT (the bundle's shape, carried verbatim).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct LedgerEvent {
    pub id: u64,
    #[serde(default)]
    pub wall_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fiber: Option<u64>,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub payload: String,
    #[serde(default)]
    pub sensitivity: String,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One `read-range` page.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct LedgerPage {
    #[serde(default)]
    pub events: Vec<LedgerEvent>,
    #[serde(default)]
    pub next_from: u64,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `read-range` request wire.
#[must_use]
pub fn ledger_read_range_payload(from_id: u64, limit: u32) -> Vec<u8> {
    let mut wire = from_id.to_le_bytes().to_vec();
    wire.extend(limit.to_le_bytes());
    wire
}

/// The `last-seq` answer, or `None` for a malformed one.
#[must_use]
pub fn decode_last_seq(bytes: &[u8]) -> Option<u64> {
    bytes.try_into().ok().map(u64::from_le_bytes)
}

/// The `patch-entry` request wire.
#[must_use]
pub fn profile_patch_payload(id: &str, merge_patch: &serde_json::Value) -> Vec<u8> {
    let mut wire = (id.len() as u32).to_le_bytes().to_vec();
    wire.extend(id.as_bytes());
    wire.extend(merge_patch.to_string().into_bytes());
    wire
}

/// The `patch-entry` answer: applied, or refused with the kernel's reason.
///
/// # Errors
///
/// The refusal reason (tag `1`), or a description of a malformed answer.
pub fn decode_profile_answer(bytes: &[u8]) -> Result<(), String> {
    match bytes.split_first() {
        Some((0, _)) => Ok(()),
        Some((1, reason)) => Err(String::from_utf8_lossy(reason).into_owned()),
        Some((tag, _)) => Err(format!("malformed patch-entry answer: tag {tag}")),
        None => Err("malformed patch-entry answer: empty".to_owned()),
    }
}

/// Whether a `jinn:profile` refusal is the loader's retryable conflict
/// (an operation in flight on the entry or the document).
#[must_use]
pub fn refusal_is_retryable(reason: &str) -> bool {
    reason.contains("retry") || reason.contains("in flight")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ledger_and_profile_wires_match_their_bundles() {
        assert_eq!(
            ledger_read_range_payload(7, 3),
            [7, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0]
        );
        assert_eq!(decode_last_seq(&42u64.to_le_bytes()), Some(42));
        assert_eq!(decode_last_seq(b"short"), None);
        let wire = profile_patch_payload("ab", &serde_json::json!({ "data": { "x": 1 } }));
        assert_eq!(&wire[..4], &[2, 0, 0, 0]);
        assert_eq!(&wire[4..6], b"ab");
        assert_eq!(&wire[6..], br#"{"data":{"x":1}}"#);
        assert_eq!(decode_profile_answer(&[0]), Ok(()));
        assert_eq!(
            decode_profile_answer(b"\x01no such entry"),
            Err("no such entry".to_owned())
        );
        assert!(decode_profile_answer(&[]).is_err());
        assert!(refusal_is_retryable("loader conflict: retry"));
        assert!(!refusal_is_retryable("scope does not admit"));
    }

    #[test]
    fn introspect_and_ledger_records_decode_additively() {
        let entry: IntrospectEntry = serde_json::from_value(serde_json::json!({
            "id": "a", "fiber": 3, "state": "active", "incarnation": 9,
            "provisions": ["jinn:cron"],
            "registrations": { "listeners": 1, "alarms": 1, "sockets": 0, "processes": 0, "future": 1 },
            "novel": true
        }))
        .expect("decodes");
        assert_eq!(entry.registrations.alarms, 1);
        assert_eq!(entry.extra["novel"], true);
        let page: LedgerPage = serde_json::from_value(serde_json::json!({
            "events": [{ "id": 1, "wall-ms": 5, "entry": null, "fiber": null,
                         "kind": "ArtifactLoaded", "payload": "{\"hash\":\"h\"}",
                         "sensitivity": "public" }],
            "next-from": 2
        }))
        .expect("decodes");
        assert_eq!(page.events[0].kind, "ArtifactLoaded");
        assert_eq!(page.next_from, 2);
        let readiness: Readiness = serde_json::from_value(
            serde_json::json!({ "boot-reconciled": true, "watcher-armed": true }),
        )
        .expect("decodes");
        assert!(readiness.boot_reconciled && readiness.watcher_armed);
    }
}
