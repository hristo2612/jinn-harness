//! The operator-API service definition: contract names, operation names,
//! versioned additive request/answer schemas, the typed error, and the
//! seam's pure laws (the entry-patch law, the status shape built from the
//! profile document, the route table). The prose law lives in this
//! crate's README; this code is its schema. Everything on the seam is
//! UTF-8 JSON with kebab-case keys.
//!
//! The definition owns the SCHEMA; a provider owns TRANSPORT only. The
//! route table here names each operation's path and method so that every
//! provider shape (HTTP today, a unix socket later) exposes one surface.

use serde::{Deserialize, Serialize};

/// The schema version every answer carries (`api-version`). Within 0.x
/// every change is strictly additive (kernel R12 discipline).
pub const API_VERSION: &str = "0.1.0";

/// The status/health/ledger contract, provided by `jinn-status`.
pub const STATUS_CONTRACT: &str = "jinn:api-status";
/// The profile get/patch contract, provided by `jinn-profile-edit`.
pub const PROFILE_CONTRACT: &str = "jinn:api-profile";

/// `jinn:api-status` operation: the status report.
pub const OP_STATUS: &str = "status";
/// `jinn:api-status` operation: the health verdict.
pub const OP_HEALTH: &str = "health";
/// `jinn:api-status` operation: a page of the ledger (read-only).
pub const OP_LEDGER_TAIL: &str = "ledger-tail";
/// `jinn:api-profile` operation: the profile document of record.
pub const OP_PROFILE_GET: &str = "get";
/// `jinn:api-profile` operation: patch ONE entry's config atomically.
pub const OP_PATCH_ENTRY: &str = "patch-entry";

/// The largest ledger page a caller may ask for.
pub const LEDGER_TAIL_MAX_LIMIT: u32 = 500;

/// FINDINGS.md entry numbers the seam cites in its typed answers: the
/// kernel exposes no introspection contract (fiber state/uid, provisions,
/// listeners, alarms, readiness) and no live `jinn:ledger` provider.
pub const FINDING_NO_INTROSPECTION: u32 = 19;
/// See [`FINDING_NO_INTROSPECTION`].
pub const FINDING_NO_LEDGER_READER: u32 = 20;

/// The status fields the kernel cannot honestly answer at this pin
/// (FINDINGS.md #19/#20): reported by name, never guessed.
pub const UNAVAILABLE_STATUS_FIELDS: &[&str] = &[
    "fiber-state",
    "fiber-uid",
    "provisions",
    "listeners",
    "alarms",
    "last-ledger-seq",
    "readiness",
];

/// Unknown sibling fields, preserved across a decode → encode round trip
/// (R12 additivity: this reader carries what a newer writer said).
pub type Extensions = serde_json::Map<String, serde_json::Value>;

/// One route of the operator surface: how a transport names an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Route {
    pub method: &'static str,
    /// The path; `{id}` is the one positional parameter.
    pub path: &'static str,
    pub contract: &'static str,
    pub operation: &'static str,
}

/// The route table (v1). Additive: a new operation is a new row.
pub const ROUTES: &[Route] = &[
    Route {
        method: "GET",
        path: "/v1/status",
        contract: STATUS_CONTRACT,
        operation: OP_STATUS,
    },
    Route {
        method: "GET",
        path: "/v1/health",
        contract: STATUS_CONTRACT,
        operation: OP_HEALTH,
    },
    Route {
        method: "GET",
        path: "/v1/ledger/tail",
        contract: STATUS_CONTRACT,
        operation: OP_LEDGER_TAIL,
    },
    Route {
        method: "GET",
        path: "/v1/profile",
        contract: PROFILE_CONTRACT,
        operation: OP_PROFILE_GET,
    },
    Route {
        method: "PATCH",
        path: "/v1/profile/entries/{id}",
        contract: PROFILE_CONTRACT,
        operation: OP_PATCH_ENTRY,
    },
];

/// Matches a method + path (query already stripped) against [`ROUTES`];
/// answers the route and its `{id}` parameter, if the path carries one.
#[must_use]
pub fn route(method: &str, path: &str) -> Option<(&'static Route, Option<String>)> {
    ROUTES.iter().find_map(|candidate| {
        if candidate.method != method {
            return None;
        }
        match candidate.path.split_once("{id}") {
            None => (candidate.path == path).then_some((candidate, None)),
            Some((prefix, suffix)) => {
                let id = path.strip_prefix(prefix)?.strip_suffix(suffix)?;
                (!id.is_empty() && !id.contains('/')).then(|| (candidate, Some(id.to_owned())))
            }
        }
    })
}

/// The typed error class of a refused or failed operation (R3).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    /// The named entry (or route) does not exist.
    NotFound,
    /// The request is malformed.
    Invalid,
    /// The answer cannot be given honestly at this kernel pin; `finding`
    /// names the FINDINGS.md entry.
    Unavailable,
    /// A grant or provider refused the underlying contract call.
    Refused,
}

/// One typed error.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ApiError {
    pub code: ErrorCode,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finding: Option<u32>,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl ApiError {
    #[must_use]
    pub fn new(code: ErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
            finding: None,
            extra: Extensions::new(),
        }
    }

    #[must_use]
    pub fn unavailable(finding: u32, detail: impl Into<String>) -> Self {
        Self {
            finding: Some(finding),
            ..Self::new(ErrorCode::Unavailable, detail)
        }
    }
}

/// The envelope every contract answer crosses the broker in: an `ok`
/// value or a typed `error`. A consumer never fails its fiber to say no.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Answer {
    Ok(serde_json::Value),
    Error(ApiError),
}

impl Answer {
    /// Encodes one answer for the broker wire.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("an answer encodes")
    }

    /// Decodes one broker answer; a malformed one is a typed `refused`.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Self {
        serde_json::from_slice(bytes).unwrap_or_else(|error| {
            Self::Error(ApiError::new(
                ErrorCode::Refused,
                format!("malformed provider answer: {error}"),
            ))
        })
    }
}

/// One profile entry as the status report shows it: the document's
/// authority fields, verbatim (kernel Law 5: pinned by content hash).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct EntryStatus {
    pub id: String,
    #[serde(default)]
    pub package: String,
    #[serde(default)]
    pub hash: String,
    #[serde(default)]
    pub grants: Vec<serde_json::Value>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One provider probe the status consumer is configured to run: resolve
/// the granted contract and, optionally, call one read operation on it.
/// A probe is an observation through the broker, not kernel introspection.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProbeSpec {
    pub contract: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One probe's outcome.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProbeReport {
    pub contract: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    /// The resolve (and the call, if any) succeeded.
    pub live: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<serde_json::Value>,
    /// The kernel's refusal, rendered, when not live.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused: Option<String>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// What the kernel cannot tell a guest at this pin — named, not guessed.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct KernelIntrospection {
    pub unavailable: Vec<String>,
    pub finding: u32,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl KernelIntrospection {
    /// The honest answer at the pinned kernel (FINDINGS.md #19).
    #[must_use]
    pub fn at_this_pin() -> Self {
        Self {
            unavailable: UNAVAILABLE_STATUS_FIELDS
                .iter()
                .map(|field| (*field).to_owned())
                .collect(),
            finding: FINDING_NO_INTROSPECTION,
            extra: Extensions::new(),
        }
    }
}

/// The `status` answer.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct StatusReport {
    pub api_version: String,
    pub entries: Vec<EntryStatus>,
    pub probes: Vec<ProbeReport>,
    pub kernel: KernelIntrospection,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `health` answer: `ok` iff the profile document is readable and
/// every configured probe is live.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct HealthReport {
    pub api_version: String,
    pub ok: bool,
    pub profile_readable: bool,
    pub entries: usize,
    pub probes_live: usize,
    pub probes_total: usize,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `ledger-tail` request (query parameters on the HTTP shape).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct LedgerTailRequest {
    /// Events with `seq` strictly greater than this.
    #[serde(default)]
    pub after: u64,
    /// Page size; clamped to `1..=LEDGER_TAIL_MAX_LIMIT`.
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(flatten)]
    pub extra: Extensions,
}

fn default_limit() -> u32 {
    100
}

/// The `ledger-tail` answer: a page, or the named reason none can be
/// served honestly.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct LedgerTail {
    pub api_version: String,
    pub after: u64,
    pub limit: u32,
    pub events: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_after: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<ApiError>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `get` answer: the profile document of record, verbatim.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProfileDocument {
    pub api_version: String,
    pub profile: serde_json::Value,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The patch of one entry's `config`: `data` is an RFC 7396 merge patch
/// on the entry's settings subtree; `grants`, when present, REPLACES the
/// grant list (authority is never merged — the profile side decides it
/// whole).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ConfigPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grants: Option<Vec<serde_json::Value>>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `patch-entry` request.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PatchEntryRequest {
    pub id: String,
    #[serde(default)]
    pub config: ConfigPatch,
    /// Passed through to the granted `jinn:fs` write (keyed exactly-once
    /// per fiber); empty claims no idempotency.
    #[serde(default)]
    pub idempotency_key: String,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `patch-entry` answer: the entry after the patch, and whether the
/// document changed (an identical patch is answered, not re-written).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PatchEntryAnswer {
    pub api_version: String,
    pub id: String,
    pub entry: serde_json::Value,
    pub changed: bool,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// RFC 7396 JSON merge patch: objects merge recursively, `null` removes,
/// anything else replaces.
pub fn merge_patch(target: &mut serde_json::Value, patch: &serde_json::Value) {
    let serde_json::Value::Object(fields) = patch else {
        *target = patch.clone();
        return;
    };
    if !target.is_object() {
        *target = serde_json::Value::Object(serde_json::Map::new());
    }
    let object = target.as_object_mut().expect("an object");
    for (key, value) in fields {
        if value.is_null() {
            object.remove(key);
        } else {
            merge_patch(
                object.entry(key.clone()).or_insert(serde_json::Value::Null),
                value,
            );
        }
    }
}

/// The profile document's entries, or the typed reason it has none.
fn entries_mut(profile: &mut serde_json::Value) -> Result<&mut Vec<serde_json::Value>, ApiError> {
    profile
        .get_mut("entries")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| ApiError::new(ErrorCode::Invalid, "profile document has no entries array"))
}

/// The entry-patch law: patch exactly ONE entry's config in the document,
/// leaving every other byte of meaning untouched. Answers the patched
/// entry and whether anything changed.
///
/// # Errors
///
/// `not-found` for an unknown entry id; `invalid` for a document without
/// an entries array.
pub fn patch_entry(
    profile: &mut serde_json::Value,
    request: &PatchEntryRequest,
) -> Result<PatchEntryAnswer, ApiError> {
    let entries = entries_mut(profile)?;
    let entry = entries
        .iter_mut()
        .find(|entry| entry["id"] == request.id)
        .ok_or_else(|| {
            ApiError::new(
                ErrorCode::NotFound,
                format!("no profile entry {:?}", request.id),
            )
        })?;
    let before = entry.clone();
    if !entry["config"].is_object() {
        entry["config"] = serde_json::json!({});
    }
    if let Some(data) = &request.config.data {
        merge_patch(&mut entry["config"]["data"], data);
    }
    if let Some(grants) = &request.config.grants {
        entry["config"]["grants"] = serde_json::Value::Array(grants.clone());
    }
    Ok(PatchEntryAnswer {
        api_version: API_VERSION.to_owned(),
        id: request.id.clone(),
        changed: *entry != before,
        entry: entry.clone(),
        extra: Extensions::new(),
    })
}

/// The entries of a profile document as the status report shows them.
///
/// # Errors
///
/// `invalid` for a document without an entries array or with an entry
/// that is not an object.
pub fn entries_status(profile: &serde_json::Value) -> Result<Vec<EntryStatus>, ApiError> {
    profile
        .get("entries")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| ApiError::new(ErrorCode::Invalid, "profile document has no entries array"))?
        .iter()
        .map(|entry| {
            let config = entry.get("config").cloned().unwrap_or_default();
            let mut status: EntryStatus = serde_json::from_value(entry.clone())
                .map_err(|error| ApiError::new(ErrorCode::Invalid, format!("entry: {error}")))?;
            status.grants = config
                .get("grants")
                .and_then(serde_json::Value::as_array)
                .cloned()
                .unwrap_or_default();
            // The config subtree is the entry's settings, not its status.
            status.extra.remove("config");
            Ok(status)
        })
        .collect()
}

/// Renders a profile document for the write-back lane: pretty, newline
/// terminated, stable key order (serde_json's map order is insertion
/// order — a re-render of an unchanged document is byte-identical).
#[must_use]
pub fn render_profile(profile: &serde_json::Value) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(profile).expect("a profile encodes");
    bytes.push(b'\n');
    bytes
}

/// Normalizes a `ledger-tail` request: the page size is clamped into
/// `1..=LEDGER_TAIL_MAX_LIMIT`.
#[must_use]
pub fn normalize_tail(mut request: LedgerTailRequest) -> LedgerTailRequest {
    request.limit = request.limit.clamp(1, LEDGER_TAIL_MAX_LIMIT);
    request
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn merge_patch_follows_rfc_7396() {
        let mut target = json!({ "a": { "b": 1, "c": 2 }, "d": [1, 2], "e": "x" });
        merge_patch(
            &mut target,
            &json!({ "a": { "b": null, "z": 9 }, "d": [3], "f": { "g": true } }),
        );
        assert_eq!(
            target,
            json!({ "a": { "c": 2, "z": 9 }, "d": [3], "e": "x", "f": { "g": true } })
        );
        let mut scalar = json!(1);
        merge_patch(&mut scalar, &json!({ "k": 1 }));
        assert_eq!(scalar, json!({ "k": 1 }), "a non-object target becomes one");
    }

    fn profile() -> serde_json::Value {
        json!({ "entries": [
            { "id": "a", "package": "p/a", "hash": "h", "config": { "grants": ["jinn:fs"],
              "data": { "jobs": [{ "id": "j", "every-ms": 2000 }], "tick-ms": 500 } } },
            { "id": "b", "package": "p/b", "hash": "h2", "config": { "grants": [], "data": {} },
              "future": "kept" }
        ] })
    }

    #[test]
    fn patch_entry_touches_exactly_one_entry_and_reports_change() {
        let mut document = profile();
        let request = PatchEntryRequest {
            id: "a".into(),
            config: ConfigPatch {
                data: Some(json!({ "tick-ms": 250 })),
                ..ConfigPatch::default()
            },
            ..PatchEntryRequest::default()
        };
        let answer = patch_entry(&mut document, &request).expect("patched");
        assert!(answer.changed);
        assert_eq!(answer.entry["config"]["data"]["tick-ms"], 250);
        assert_eq!(
            document["entries"][0]["config"]["data"]["jobs"][0]["every-ms"], 2000,
            "sibling settings untouched"
        );
        assert_eq!(
            document["entries"][1],
            profile()["entries"][1],
            "other entry untouched"
        );
        // An identical patch is a no-op, reported as such.
        let again = patch_entry(&mut document, &request).expect("patched");
        assert!(!again.changed);
    }

    #[test]
    fn grants_replace_never_merge() {
        let mut document = profile();
        let request = PatchEntryRequest {
            id: "a".into(),
            config: ConfigPatch {
                grants: Some(vec![json!("jinn:clock")]),
                ..ConfigPatch::default()
            },
            ..PatchEntryRequest::default()
        };
        patch_entry(&mut document, &request).expect("patched");
        assert_eq!(
            document["entries"][0]["config"]["grants"],
            json!(["jinn:clock"])
        );
    }

    #[test]
    fn patch_entry_errors_are_typed() {
        let mut document = profile();
        let missing = patch_entry(
            &mut document,
            &PatchEntryRequest {
                id: "zz".into(),
                ..PatchEntryRequest::default()
            },
        )
        .expect_err("unknown entry");
        assert_eq!(missing.code, ErrorCode::NotFound);
        let mut broken = json!({ "entries": 5 });
        let invalid = patch_entry(
            &mut broken,
            &PatchEntryRequest {
                id: "a".into(),
                ..PatchEntryRequest::default()
            },
        )
        .expect_err("no entries array");
        assert_eq!(invalid.code, ErrorCode::Invalid);
    }

    #[test]
    fn entries_status_carries_authority_fields_and_unknown_siblings() {
        let entries = entries_status(&profile()).expect("entries");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].grants, vec![json!("jinn:fs")]);
        assert_eq!(
            entries[1].extra["future"], "kept",
            "additive field preserved"
        );
        assert!(
            !entries[0].extra.contains_key("config"),
            "settings are not status"
        );
    }

    #[test]
    fn schemas_preserve_unknown_fields_round_trip() {
        let wire = json!({ "id": "a", "config": { "data": { "x": 1 }, "novel": true },
                           "idempotency-key": "k", "future": [1] });
        let request: PatchEntryRequest = serde_json::from_value(wire.clone()).expect("decodes");
        assert_eq!(request.config.extra["novel"], true);
        assert_eq!(serde_json::to_value(&request).expect("encodes"), wire);
        let report = json!({ "api-version": "0.1.0", "entries": [], "probes": [],
                             "kernel": { "unavailable": [], "finding": 19, "more": 1 }, "extra-top": "y" });
        let decoded: StatusReport = serde_json::from_value(report.clone()).expect("decodes");
        assert_eq!(serde_json::to_value(&decoded).expect("encodes"), report);
    }

    #[test]
    fn the_answer_envelope_is_typed_both_ways() {
        let ok = Answer::Ok(json!({ "n": 1 }));
        assert_eq!(Answer::decode(&ok.encode()), ok);
        let error = Answer::Error(ApiError::unavailable(20, "no ledger reader"));
        let decoded = Answer::decode(&error.encode());
        assert_eq!(decoded, error);
        assert!(matches!(
            Answer::decode(b"garbage"),
            Answer::Error(ApiError {
                code: ErrorCode::Refused,
                ..
            })
        ));
    }

    #[test]
    fn routes_match_by_method_and_path_with_one_parameter() {
        let (status, id) = route("GET", "/v1/status").expect("status route");
        assert_eq!(
            (status.contract, status.operation),
            (STATUS_CONTRACT, OP_STATUS)
        );
        assert!(id.is_none());
        let (patch, id) = route("PATCH", "/v1/profile/entries/cron-scheduler").expect("patch");
        assert_eq!(patch.operation, OP_PATCH_ENTRY);
        assert_eq!(id.as_deref(), Some("cron-scheduler"));
        assert!(
            route("GET", "/v1/profile/entries/x").is_none(),
            "method matters"
        );
        assert!(route("PATCH", "/v1/profile/entries/").is_none(), "empty id");
        assert!(
            route("PATCH", "/v1/profile/entries/a/b").is_none(),
            "one segment"
        );
        assert!(route("GET", "/nope").is_none());
    }

    #[test]
    fn ledger_tail_defaults_and_clamps() {
        let request: LedgerTailRequest = serde_json::from_value(json!({})).expect("defaults");
        assert_eq!((request.after, request.limit), (0, 100));
        assert_eq!(
            normalize_tail(LedgerTailRequest {
                limit: 0,
                ..request.clone()
            })
            .limit,
            1
        );
        assert_eq!(
            normalize_tail(LedgerTailRequest {
                limit: 9_999,
                ..request
            })
            .limit,
            LEDGER_TAIL_MAX_LIMIT
        );
    }

    #[test]
    fn render_is_stable_for_an_unchanged_document() {
        let document = profile();
        assert_eq!(render_profile(&document), render_profile(&document.clone()));
        assert!(render_profile(&document).ends_with(b"}\n"));
    }
}
