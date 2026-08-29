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

pub mod kernel;

pub use kernel::{
    decode_last_seq, decode_profile_answer, ledger_read_range_payload, profile_patch_payload,
    refusal_is_retryable, IntrospectEntry, LedgerEvent, LedgerPage, Readiness, Registrations,
    INTROSPECT_CONTRACT, KERNEL_PROFILE_CONTRACT, KERNEL_PROFILE_SCOPE_ALL, LEDGER_CONTRACT,
    OP_INTROSPECT_ENTRIES, OP_INTROSPECT_READINESS, OP_KERNEL_PATCH_ENTRY, OP_LEDGER_LAST_SEQ,
    OP_LEDGER_READ_RANGE,
};

/// The schema version every answer carries (`api-version`). Within 0.x
/// every change is strictly additive (kernel R12 discipline). 0.2.0 (pin
/// `57360cc`): introspection fields on every entry, `readiness`,
/// `last-ledger-seq` and `document` on the status report, real ledger
/// pages, the patch applied by the kernel's own `jinn:profile`.
pub const API_VERSION: &str = "0.2.0";

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

/// FINDINGS.md entry numbers the seam cites in its typed answers. #19 (no
/// introspection contract) and #20 (no live `jinn:ledger` reader) are
/// CLOSED at pin `57360cc`; the numbers stay as the vocabulary of
/// `kernel.unavailable`, which is now empty.
pub const FINDING_NO_INTROSPECTION: u32 = 19;
/// See [`FINDING_NO_INTROSPECTION`].
pub const FINDING_NO_LEDGER_READER: u32 = 20;
/// FINDINGS.md #25: the document of record is reachable by a guest only
/// through a `jinn:fs` scope, i.e. only when it sits under the data root;
/// `jinn:introspect` carries no authority fields (package, hash, grants)
/// and `jinn:profile` has no read. Where the document is out of reach the
/// status report says so by this number, never guesses.
pub const FINDING_NO_DOCUMENT_READ: u32 = 25;

/// The status fields the kernel cannot honestly answer at this pin —
/// EMPTY since `57360cc` (FINDINGS.md #19/#20 closed): every field the
/// 0.1.0 list named now lands as an additive sibling on the report.
pub const UNAVAILABLE_STATUS_FIELDS: &[&str] = &[];

/// Unknown sibling fields, preserved across a decode → encode round trip
/// (R12 additivity: this reader carries what a newer writer said).
pub type Extensions = serde_json::Map<String, serde_json::Value>;

/// The settings seam's contract, exposed by the same transport (its
/// answers cross in this seam's envelope shape).
pub const SETTINGS_CONTRACT: &str = "jinn:settings";
/// `jinn:settings` operations exposed as routes.
pub const OP_SETTINGS_NAMESPACES: &str = "namespaces";
/// See [`OP_SETTINGS_NAMESPACES`].
pub const OP_SETTINGS_GET: &str = "get";
/// See [`OP_SETTINGS_NAMESPACES`].
pub const OP_SETTINGS_PATCH: &str = "patch";

/// One route of the operator surface: how a transport names an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Route {
    pub method: &'static str,
    /// The path; `{id}` is the one positional parameter.
    pub path: &'static str,
    pub contract: &'static str,
    pub operation: &'static str,
    /// The request field the path parameter lands in (`id`, `namespace`).
    pub param: &'static str,
    /// Whether the request payload is the JSON body (else the query).
    pub body: bool,
}

/// The route table (v1). Additive: a new operation is a new row.
pub const ROUTES: &[Route] = &[
    Route {
        method: "GET",
        path: "/v1/status",
        contract: STATUS_CONTRACT,
        operation: OP_STATUS,
        param: "id",
        body: false,
    },
    Route {
        method: "GET",
        path: "/v1/health",
        contract: STATUS_CONTRACT,
        operation: OP_HEALTH,
        param: "id",
        body: false,
    },
    Route {
        method: "GET",
        path: "/v1/ledger/tail",
        contract: STATUS_CONTRACT,
        operation: OP_LEDGER_TAIL,
        param: "id",
        body: false,
    },
    Route {
        method: "GET",
        path: "/v1/profile",
        contract: PROFILE_CONTRACT,
        operation: OP_PROFILE_GET,
        param: "id",
        body: false,
    },
    Route {
        method: "PATCH",
        path: "/v1/profile/entries/{id}",
        contract: PROFILE_CONTRACT,
        operation: OP_PATCH_ENTRY,
        param: "id",
        body: true,
    },
    Route {
        method: "GET",
        path: "/v1/settings",
        contract: SETTINGS_CONTRACT,
        operation: OP_SETTINGS_NAMESPACES,
        param: "namespace",
        body: false,
    },
    Route {
        method: "GET",
        path: "/v1/settings/{id}",
        contract: SETTINGS_CONTRACT,
        operation: OP_SETTINGS_GET,
        param: "namespace",
        body: false,
    },
    Route {
        method: "PATCH",
        path: "/v1/settings/{id}",
        contract: SETTINGS_CONTRACT,
        operation: OP_SETTINGS_PATCH,
        param: "namespace",
        body: true,
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

/// The outcome of one contract call: an `ok` value or a typed `error`.
/// Externally tagged on the wire (`{"ok": …}` / `{"error": {…}}`).
/// `Ok` carries a lossless [`serde_json::Value`], so every unknown field
/// inside it survives by construction; `Error` carries its own flattened
/// extension map ([`ApiError::extra`]).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Ok(serde_json::Value),
    Error(ApiError),
}

/// The envelope every contract answer crosses the broker in: the schema
/// version, the [`Outcome`], and a flattened extension map for unknown
/// siblings (R12 additivity at the envelope level). A consumer never
/// fails its fiber to say no.
///
/// Every answer this seam produces ([`Answer::ok`], [`Answer::error`],
/// [`Answer::decode`]'s refusal) carries `api-version`. A foreign answer
/// that omits it decodes as unversioned (`None`) and re-encodes without
/// one — nothing is invented on the writer's behalf.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Answer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
    #[serde(flatten)]
    pub outcome: Outcome,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl Answer {
    /// A versioned `ok` answer.
    #[must_use]
    pub fn ok(value: serde_json::Value) -> Self {
        Self::versioned(Outcome::Ok(value))
    }

    /// A versioned `error` answer.
    #[must_use]
    pub fn error(error: ApiError) -> Self {
        Self::versioned(Outcome::Error(error))
    }

    fn versioned(outcome: Outcome) -> Self {
        Self {
            api_version: Some(API_VERSION.to_owned()),
            outcome,
            extra: Extensions::new(),
        }
    }

    /// Encodes one answer for the broker wire.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("an answer encodes")
    }

    /// Decodes one broker answer; a malformed one is a typed `refused`.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Self {
        serde_json::from_slice(bytes).unwrap_or_else(|error| {
            Self::error(ApiError::new(
                ErrorCode::Refused,
                format!("malformed provider answer: {error}"),
            ))
        })
    }
}

/// One profile entry as the status report shows it: the document's
/// authority fields, verbatim (kernel Law 5: pinned by content hash), and
/// — since 0.2.0 — the kernel's own view of the entry through
/// `jinn:introspect` (`fiber`, `state`, `incarnation`, `provisions`,
/// `registrations`), absent when the entry has no live fiber or the
/// introspection read was refused.
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fiber: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incarnation: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provisions: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registrations: Option<Registrations>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// Whether the document of record was readable for this report, and the
/// typed reason when not (FINDINGS.md #25). When unreadable, `entries`
/// are the kernel's list (introspection fields only) and the authority
/// fields are empty — stated here, never guessed.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct DocumentStatus {
    pub readable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<ApiError>,
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
/// `finding` is the FINDINGS.md entry whose vocabulary the list uses
/// (#19); the list is EMPTY since pin `57360cc`. Both fields stay on the
/// wire for 0.1.0 readers (additivity: never removed, never renamed).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct KernelIntrospection {
    pub unavailable: Vec<String>,
    pub finding: u32,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl KernelIntrospection {
    /// The honest answer at the pinned kernel: nothing unavailable.
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

/// The `status` answer. 0.2.0 adds `readiness` and `last-ledger-seq`
/// (absent when the kernel read was refused) and `document`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct StatusReport {
    pub api_version: String,
    pub entries: Vec<EntryStatus>,
    pub probes: Vec<ProbeReport>,
    pub kernel: KernelIntrospection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readiness: Option<Readiness>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_ledger_seq: Option<u64>,
    #[serde(default)]
    pub document: DocumentStatus,
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

/// The `ledger-tail` answer: one page of the kernel's ledger (events with
/// `id` strictly greater than `after`, at most `limit`), `next-after` set
/// when a further page may exist — or the named reason none can be
/// served (`unavailable`, e.g. the `jinn:ledger` read refused).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct LedgerTail {
    pub api_version: String,
    pub after: u64,
    pub limit: u32,
    pub events: Vec<LedgerEvent>,
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
    /// Accepted for 0.1.0 writers and UNUSED since 0.2.0: the patch is
    /// applied by the kernel's `jinn:profile` as operator intent (no fs
    /// write to key). An identical patch is still answered without a
    /// call (`changed: false`).
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

/// The kernel-side merge patch (`jinn:profile` `patch-entry`, RFC 7396
/// on the entry's `config` subtree) that applies one [`ConfigPatch`]
/// under the entry-patch law: `data` merges, `grants` replaces whole
/// (an array is replaced whole by RFC 7396 — the law and the kernel
/// agree by construction).
#[must_use]
pub fn kernel_merge_patch(patch: &ConfigPatch) -> serde_json::Value {
    let mut merge = serde_json::Map::new();
    if let Some(data) = &patch.data {
        merge.insert("data".into(), data.clone());
    }
    if let Some(grants) = &patch.grants {
        merge.insert("grants".into(), serde_json::Value::Array(grants.clone()));
    }
    serde_json::Value::Object(merge)
}

/// Lays the kernel's view of the composition over the document's entries
/// (matched by id); an entry the kernel lists but the document does not
/// (or the whole list, when the document was unreadable) is appended
/// with empty authority fields.
pub fn merge_introspection(entries: &mut Vec<EntryStatus>, kernel: &[IntrospectEntry]) {
    for seen in kernel {
        let status = match entries.iter_mut().find(|entry| entry.id == seen.id) {
            Some(status) => status,
            None => {
                entries.push(EntryStatus {
                    id: seen.id.clone(),
                    ..EntryStatus::default()
                });
                entries.last_mut().expect("just pushed")
            }
        };
        status.fiber = seen.fiber;
        status.state = seen.state.clone();
        status.incarnation = seen.incarnation;
        status.provisions = Some(seen.provisions.clone());
        status.registrations = Some(seen.registrations);
    }
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
    fn kernel_merge_patch_carries_data_and_replaces_grants() {
        let patch = ConfigPatch {
            data: Some(json!({ "tick-ms": 250, "gone": null })),
            grants: Some(vec![json!("jinn:clock")]),
            ..ConfigPatch::default()
        };
        assert_eq!(
            kernel_merge_patch(&patch),
            json!({ "data": { "tick-ms": 250, "gone": null }, "grants": ["jinn:clock"] })
        );
        assert_eq!(kernel_merge_patch(&ConfigPatch::default()), json!({}));
    }

    #[test]
    fn merge_introspection_lays_the_kernel_view_over_the_document() {
        let mut entries = entries_status(&profile()).expect("entries");
        let kernel = vec![
            IntrospectEntry {
                id: "a".into(),
                fiber: Some(4),
                state: Some("active".into()),
                incarnation: Some(1),
                provisions: vec!["jinn:cron".into()],
                registrations: Registrations {
                    alarms: 1,
                    ..Registrations::default()
                },
                extra: Extensions::new(),
            },
            IntrospectEntry {
                id: "only-kernel".into(),
                ..IntrospectEntry::default()
            },
        ];
        merge_introspection(&mut entries, &kernel);
        assert_eq!(entries[0].fiber, Some(4));
        assert_eq!(entries[0].provisions.as_deref(), Some(&["jinn:cron".to_owned()][..]));
        assert_eq!(entries[0].hash, "h", "authority fields kept");
        assert_eq!(entries[1].fiber, None, "no kernel view for b");
        assert_eq!(entries[2].id, "only-kernel");
        assert!(entries[2].package.is_empty(), "no authority for a kernel-only entry");
    }

    #[test]
    fn schemas_preserve_unknown_fields_round_trip() {
        let wire = json!({ "id": "a", "config": { "data": { "x": 1 }, "novel": true },
                           "idempotency-key": "k", "future": [1] });
        let request: PatchEntryRequest = serde_json::from_value(wire.clone()).expect("decodes");
        assert_eq!(request.config.extra["novel"], true);
        assert_eq!(serde_json::to_value(&request).expect("encodes"), wire);
        let report = json!({ "api-version": "0.1.0", "entries": [], "probes": [],
                             "kernel": { "unavailable": [], "finding": 19, "more": 1 },
                             "document": { "readable": true }, "extra-top": "y" });
        let decoded: StatusReport = serde_json::from_value(report.clone()).expect("decodes");
        assert_eq!(serde_json::to_value(&decoded).expect("encodes"), report);
    }

    #[test]
    fn the_answer_envelope_is_typed_both_ways() {
        let ok = Answer::ok(json!({ "n": 1 }));
        assert_eq!(Answer::decode(&ok.encode()), ok);
        let error = Answer::error(ApiError::unavailable(20, "no ledger reader"));
        let decoded = Answer::decode(&error.encode());
        assert_eq!(decoded, error);
        assert!(matches!(
            Answer::decode(b"garbage").outcome,
            Outcome::Error(ApiError {
                code: ErrorCode::Refused,
                ..
            })
        ));
    }

    #[test]
    fn every_answer_carries_the_api_version_including_errors() {
        let ok = serde_json::to_value(Answer::ok(json!({}))).expect("encodes");
        assert_eq!(ok["api-version"], API_VERSION);
        let error = serde_json::to_value(Answer::error(ApiError::new(ErrorCode::Invalid, "x")))
            .expect("encodes");
        assert_eq!(error["api-version"], API_VERSION, "{error}");
        assert_eq!(error["error"]["code"], "invalid");
        let refused = serde_json::to_value(Answer::decode(b"garbage")).expect("encodes");
        assert_eq!(
            refused["api-version"], API_VERSION,
            "a malformed answer's refusal is versioned too"
        );
    }

    #[test]
    fn the_verifier_probe_decodes_as_ok_and_keeps_its_unknown_sibling() {
        // The exact probe from the round-1 verify: an unknown field beside `ok`.
        let probe = br#"{"ok":{"n":1},"future":true}"#;
        let answer = Answer::decode(probe);
        assert_eq!(answer.outcome, Outcome::Ok(json!({ "n": 1 })), "{answer:?}");
        assert_eq!(answer.extra["future"], true);
        assert_eq!(
            answer.api_version, None,
            "an unversioned writer stays unversioned"
        );
        let wire: serde_json::Value = serde_json::from_slice(&answer.encode()).expect("json");
        assert_eq!(wire, json!({ "ok": { "n": 1 }, "future": true }));
    }

    #[test]
    fn unknown_fields_round_trip_at_every_level_of_an_ok_answer() {
        let wire = json!({
            "api-version": "0.9.0",
            "ok": { "n": 1, "nested": { "deep": { "deeper": [1, { "x": null }] } } },
            "envelope-future": { "shape": "object" }
        });
        let answer: Answer = serde_json::from_value(wire.clone()).expect("decodes");
        assert_eq!(answer.api_version.as_deref(), Some("0.9.0"));
        assert_eq!(answer.extra["envelope-future"]["shape"], "object");
        assert_eq!(serde_json::to_value(&answer).expect("encodes"), wire);
    }

    #[test]
    fn unknown_fields_round_trip_at_every_level_of_an_error_answer() {
        let wire = json!({
            "api-version": "0.9.0",
            "error": {
                "code": "not-found",
                "detail": "gone",
                "finding": 7,
                "variant-future": { "nested": { "deep": true } }
            },
            "envelope-future": ["kept"]
        });
        let answer: Answer = serde_json::from_value(wire.clone()).expect("decodes");
        match &answer.outcome {
            Outcome::Error(error) => {
                assert_eq!(error.code, ErrorCode::NotFound);
                assert_eq!(error.extra["variant-future"]["nested"]["deep"], true);
            }
            other => panic!("expected an error outcome, got {other:?}"),
        }
        assert_eq!(answer.extra["envelope-future"][0], "kept");
        assert_eq!(serde_json::to_value(&answer).expect("encodes"), wire);
    }

    #[test]
    fn an_answer_with_neither_ok_nor_error_is_a_typed_refusal() {
        let answer = Answer::decode(br#"{"api-version":"0.1.0","future":true}"#);
        assert!(matches!(
            answer.outcome,
            Outcome::Error(ApiError {
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
        assert!(patch.body);
        let (settings, ns) = route("PATCH", "/v1/settings/cron").expect("settings patch");
        assert_eq!((settings.contract, settings.param), (SETTINGS_CONTRACT, "namespace"));
        assert_eq!(ns.as_deref(), Some("cron"));
        let (list, none) = route("GET", "/v1/settings").expect("namespaces");
        assert_eq!(list.operation, OP_SETTINGS_NAMESPACES);
        assert!(none.is_none() && !list.body);
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
