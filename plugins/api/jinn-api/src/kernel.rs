//! The kernel operator contracts' wire shapes (pin `3fd7b05`, jinnd
//! M2-K8), as pure encode/decode so both guests and host tests share one
//! reading of each bundle: `jinn:introspect` (JSON answers), `jinn:ledger`
//! (`u64-LE from-id ++ u32-LE limit` → JSON page; `last-seq` → `u64-LE`),
//! and `jinn:profile` — `patch-entry` (`u32-LE id length ++ id ++
//! merge-patch JSON` → one tag byte: `2` accepted, then the
//! `ProfilePatched` row's u64-LE ledger sequence; `1` refused, then the
//! reason's UTF-8; `0` applied is the 0.1.0 answer a 0.2.0 provider never
//! gives) and the 0.2.0 reads `entry` (`u32-LE id length ++ id` → the
//! entry record as JSON text, or the JSON `null`) and `document` (EMPTY
//! payload → `{ "entries": [ <entry>… ] }` as JSON text).

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
/// `jinn:profile` operation: ONE entry's authority fields as the document
/// of record holds them (0.2.0, FINDINGS.md #25 closed).
pub const OP_KERNEL_ENTRY: &str = "entry";
/// `jinn:profile` operation: the document of record's entries the
/// caller's scope admits (0.2.0, FINDINGS.md #25 closed).
pub const OP_KERNEL_DOCUMENT: &str = "document";
/// The operation class a read-only `jinn:profile` viewer's grant names
/// (`ops`, pin `3fd7b05`): the reads and NOT `patch-entry` — authority
/// exactly as wide as its use (FINDINGS.md #24 closed).
pub const KERNEL_PROFILE_READ_OPS: [&str; 2] = [OP_KERNEL_ENTRY, OP_KERNEL_DOCUMENT];
/// The operation class the operator EDITOR's grant names: the reads and
/// the write.
pub const KERNEL_PROFILE_EDIT_OPS: [&str; 3] =
    [OP_KERNEL_PATCH_ENTRY, OP_KERNEL_ENTRY, OP_KERNEL_DOCUMENT];
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

/// One entry as the document of record holds it (`jinn:profile` 0.2.0's
/// `entry` / `document` answers, pin `3fd7b05`): the AUTHORITY fields
/// `jinn:introspect` does not carry — the pinned package and its content
/// hash, the grants as written, the config subtree — plus the entry's
/// place in the tree. Decodes additively (R12): an unknown sibling of a
/// newer kernel lands in `extra`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProfileEntryRecord {
    pub id: String,
    #[serde(default)]
    pub package: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub grants: Vec<serde_json::Value>,
    #[serde(default)]
    pub config: serde_json::Value,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(flatten)]
    pub extra: Extensions,
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

/// The `entry` request wire (`jinn:profile` 0.2.0): the id alone.
#[must_use]
pub fn profile_entry_payload(id: &str) -> Vec<u8> {
    let mut wire = (id.len() as u32).to_le_bytes().to_vec();
    wire.extend(id.as_bytes());
    wire
}

/// The `document` answer: `{ "entries": [ <entry>… ] }` as the kernel
/// wrote it, carried whole (every [`ProfileEntryRecord`] field and any
/// additive sibling survives). A read the caller's scope does not admit
/// never reaches here — it is a ledgered grant refusal, an ERROR on the
/// wire, not a decodable answer.
///
/// # Errors
///
/// A description of a malformed answer.
pub fn decode_profile_document(bytes: &[u8]) -> Result<serde_json::Value, String> {
    serde_json::from_slice(bytes).map_err(|error| format!("malformed document answer: {error}"))
}

/// The `entry` answer: the entry's record, or `None` for the JSON `null`
/// an unknown id answers.
///
/// # Errors
///
/// A description of a malformed answer.
pub fn decode_profile_entry(bytes: &[u8]) -> Result<Option<serde_json::Value>, String> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("malformed entry answer: {error}"))?;
    Ok((!value.is_null()).then_some(value))
}

/// The `patch-entry` answer (`jinn:profile` 0.2.0, pin `3fd7b05`):
/// `Some(seq)` — ACCEPTED, the document committed and the patched fiber's
/// restart is scheduled, `seq` the `ProfilePatched` record's ledger
/// sequence the restart's transitions land after; `None` — the 0.1.0
/// `applied` answer, retained for 0.1.0 readers and never given by the
/// pinned provider; `Err` — refused, with the kernel's reason.
///
/// # Errors
///
/// The refusal reason (tag `1`), or a description of a malformed answer.
pub fn decode_profile_answer(bytes: &[u8]) -> Result<Option<u64>, String> {
    match bytes.split_first() {
        Some((0, _)) => Ok(None),
        Some((1, reason)) => Err(String::from_utf8_lossy(reason).into_owned()),
        Some((2, sequence)) => sequence
            .try_into()
            .map(|bytes| Some(u64::from_le_bytes(bytes)))
            .map_err(|_| {
                format!(
                    "malformed patch-entry answer: accepted with {} sequence bytes",
                    sequence.len()
                )
            }),
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
        // 0.2.0 (pin 3fd7b05): `accepted(seq)` is what the pinned
        // provider answers — the document committed, the patched fiber's
        // restart is SCHEDULED, and `seq` is the `ProfilePatched` row the
        // restart's transitions land after.
        let mut accepted = vec![2u8];
        accepted.extend(4242u64.to_le_bytes());
        assert_eq!(decode_profile_answer(&accepted), Ok(Some(4242)));
        // `applied` is retained for 0.1.0 readers and never answered here.
        assert_eq!(decode_profile_answer(&[0]), Ok(None));
        assert_eq!(
            decode_profile_answer(b"\x01no such entry"),
            Err("no such entry".to_owned())
        );
        assert!(decode_profile_answer(&[]).is_err());
        assert!(decode_profile_answer(&[2, 1, 2]).is_err());
        assert!(refusal_is_retryable("loader conflict: retry"));
        assert!(!refusal_is_retryable("scope does not admit"));
    }

    #[test]
    fn the_profile_read_wires_match_the_bundle() {
        // `entry`: u32-LE id length ++ the id's UTF-8 bytes.
        assert_eq!(profile_entry_payload("ab"), [2, 0, 0, 0, b'a', b'b']);
        // `document`: EMPTY payload, answered as JSON text
        // `{ "entries": [ <entry>… ] }` for the entries the scope admits.
        let document =
            decode_profile_document(br#"{"entries":[{"id":"a"}]}"#).expect("a document answer");
        assert_eq!(document["entries"][0]["id"], "a");
        assert!(decode_profile_document(b"not json").is_err());
        // `entry`: the record of one entry, or the JSON `null` an unknown
        // id answers — decoded additively, kebab-case like every sibling.
        let record = decode_profile_entry(
            br#"{"id":"a","package":"api/x","version":"0.1.0","hash":"h",
                 "grants":["jinn:fs"],"config":{"grants":["jinn:fs"]},
                 "disabled":false,"parent":null,"novel":1}"#,
        )
        .expect("an entry answer")
        .expect("a known id");
        let entry: ProfileEntryRecord = serde_json::from_value(record).expect("the record shape");
        assert_eq!(entry.package, "api/x");
        assert_eq!(entry.version, "0.1.0");
        assert_eq!(entry.hash, "h");
        assert_eq!(entry.grants, vec![serde_json::json!("jinn:fs")]);
        assert_eq!(entry.config["grants"], serde_json::json!(["jinn:fs"]));
        assert!(!entry.disabled);
        assert_eq!(entry.parent, None);
        assert_eq!(entry.extra["novel"], 1, "an additive sibling survives");
        assert_eq!(decode_profile_entry(b"null"), Ok(None));
        assert!(decode_profile_entry(b"{").is_err());
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
