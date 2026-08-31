//! The half of a catalog provider that makes HOST CALLS. Shared as
//! SOURCE, not as a crate, for the reason the other seams' `store-core`
//! is (its one home: `plugins/sessions/store-core/README.md`) — a guest
//! generates its own bindings, so a library crate cannot make host calls
//! on its behalf.
//!
//! Everything that is not a host call already lives in `jinn_plugins`.
//! What is left is the three reads and the order they happen in, and it
//! is identical in both providers. The including crate supplies only:
//!
//! - `PROVIDER: &str` — the package name every answer reports as
//!   `served-by`, which is how a provider SWAP is observable in the
//!   answer itself.
//! - `SOURCE: GrantSource` — whether its entry set is the document of
//!   record or its own declaration.
//! - `mod source` with `fn declared(config: &CatalogConfig) ->
//!   Result<Vec<Declared>, PluginsError>`.

use std::sync::Mutex;

use jinn_plugins::{
    catalog::{Catalog, Declared, Description},
    Answer, ErrorCode, History, Line, PluginsError, Snapshot, Window, OP_DESCRIBE,
    OP_DESCRIBE_CATALOG, OP_HISTORY, OP_LIST,
};
use serde::Deserialize;

use crate::jinn::plugin::services;
use crate::source;

/// The kernel contracts a catalog reads. Named here so a grant list and
/// a call can never drift apart.
const INTROSPECT: &str = "jinn:introspect";
/// The document of record's READ view. Only the live catalog is granted
/// it; a fixed catalog holds no `jinn:profile` grant at all, so this
/// helper is compiled into it and never reachable — which is the shape
/// the authority already enforces.
const PROFILE: &str = "jinn:profile";
const OP_DOCUMENT: &str = "document";
const LEDGER: &str = "jinn:ledger";
const OP_ENTRIES: &str = "entries";
const OP_READ_RANGE: &str = "read-range";
const OP_LAST_SEQ: &str = "last-seq";

/// The ledger page cap. `jinn:ledger` clamps a read to 500; a window
/// wider than one page would be a second read this seam does not make,
/// and the answer says how wide the window actually was rather than
/// implying it covered everything.
const MAX_LEDGER_LIMIT: u32 = 500;

/// One catalog entry's config.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CatalogConfig {
    /// The catalog id this provider serves — the half of
    /// `jinn:plugins.<catalog>` an operator addresses. It is in CONFIG,
    /// which is the one subtree `jinn:profile.patch-entry` may write, and
    /// that is what makes a provider swap reachable through the operator
    /// API at all (FINDINGS.md #37).
    pub catalog: String,
    /// How many ledger lines an answer reads. The bound is stated in
    /// every answer's window rather than assumed.
    #[serde(default = "default_ledger_limit")]
    pub ledger_limit: u32,
    /// A fixed catalog's declared entry set. Read by the static provider
    /// only; the profile-derived one has no use for it and never reads
    /// it.
    #[serde(default)]
    #[allow(dead_code)]
    pub entries: Vec<Declared>,
}

fn default_ledger_limit() -> u32 {
    200
}

pub static CONFIG: Mutex<Option<CatalogConfig>> = Mutex::new(None);

/// The config this incarnation activated with.
///
/// # Panics
///
/// If called before `activate`, which cannot happen: the kernel calls
/// `activate` before any dispatch reaches this guest.
pub fn config() -> CatalogConfig {
    CONFIG
        .lock()
        .unwrap()
        .clone()
        .expect("activate holds the config")
}

/// One granted kernel read. A refusal is a TYPED answer naming the
/// contract, never a fault and never a quietly empty reading.
fn read(contract: &str, operation: &str, payload: &[u8]) -> Result<Vec<u8>, PluginsError> {
    let handle = services::resolve(contract)
        .map_err(|error| PluginsError::unreadable(contract, format!("{error:?}")))?;
    services::call(handle, operation, payload)
        .map_err(|error| PluginsError::unreadable(contract, format!("{operation}: {error:?}")))
}

fn json(bytes: &[u8], what: &str) -> Result<serde_json::Value, PluginsError> {
    serde_json::from_slice(bytes)
        .map_err(|error| PluginsError::new(ErrorCode::Failed, format!("malformed {what}: {error}")))
}

/// The document of record, as bytes. A refusal is typed and names
/// `jinn:profile`.
///
/// # Errors
///
/// [`PluginsError`] when the grant refuses or the provider fails.
#[allow(dead_code)]
pub fn read_profile_document() -> Result<Vec<u8>, PluginsError> {
    read(PROFILE, OP_DOCUMENT, &[])
}

/// The composition, by entry id.
fn composition() -> Result<std::collections::BTreeMap<String, Snapshot>, PluginsError> {
    let bytes = read(INTROSPECT, OP_ENTRIES, &[])?;
    Ok(Snapshot::parse_entries(&json(
        &bytes,
        "jinn:introspect entries",
    )?))
}

/// The ledger page this answer is bounded by, and the window it covers.
/// The window is DERIVED FROM THE READ that happened — `to` is the
/// high-water mark the kernel reported, `from` is where this read began,
/// and `truncated` says whether older lines exist that were not read.
fn ledger(limit: u32) -> Result<(Vec<Line>, Window), PluginsError> {
    let bytes = read(LEDGER, OP_LAST_SEQ, &[])?;
    let last: [u8; 8] = bytes.as_slice().try_into().map_err(|_| {
        PluginsError::new(
            ErrorCode::Failed,
            format!("jinn:ledger last-seq answered {} bytes, not 8", bytes.len()),
        )
    })?;
    let last = u64::from_le_bytes(last);
    let limit = limit.clamp(1, MAX_LEDGER_LIMIT);
    let from = last.saturating_sub(u64::from(limit).saturating_sub(1)).max(1);
    let mut payload = from.to_le_bytes().to_vec();
    payload.extend_from_slice(&limit.to_le_bytes());
    let page = json(&read(LEDGER, OP_READ_RANGE, &payload)?, "jinn:ledger page")?;
    let events = page
        .get("events")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            PluginsError::new(
                ErrorCode::Failed,
                "a jinn:ledger page carries no `events` array, which is not an empty page",
            )
        })?;
    let lines: Vec<Line> = events
        .iter()
        .filter_map(|event| {
            // A line the kernel charged to NO entry belongs to no plugin.
            // Dropping it here is the attribution rule applied at the
            // earliest point it can be, never a fallback further down.
            let entry = event.get("entry").and_then(serde_json::Value::as_str)?;
            Some(Line {
                seq: event.get("id").and_then(serde_json::Value::as_u64)?,
                wall_ms: event
                    .get("wall-ms")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or_default(),
                entry: entry.to_owned(),
                kind: event
                    .get("kind")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                payload: event
                    .get("payload")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|text| serde_json::from_str(text).ok())
                    .unwrap_or(serde_json::Value::Null),
                sensitivity: event
                    .get("sensitivity")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("public")
                    .to_owned(),
            })
        })
        .collect();
    Ok((
        lines,
        Window {
            from,
            to: last,
            scanned: u32::try_from(events.len()).unwrap_or(u32::MAX),
            truncated: from > 1,
        },
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct PluginRequest {
    #[serde(default)]
    plugin_id: String,
}

fn requested(payload: &[u8]) -> Result<String, PluginsError> {
    let request: PluginRequest = serde_json::from_slice(payload)
        .map_err(|error| PluginsError::new(ErrorCode::Invalid, format!("request: {error}")))?;
    if request.plugin_id.is_empty() {
        return Err(PluginsError::new(
            ErrorCode::Invalid,
            "plugin-id names the plugin to read; it cannot be empty",
        ));
    }
    Ok(request.plugin_id)
}

fn listing() -> Result<serde_json::Value, PluginsError> {
    let config = config();
    let declared = source::declared(&config)?;
    let snapshots = composition()?;
    let (lines, window) = ledger(config.ledger_limit)?;
    Ok(serde_json::to_value(Catalog::list(
        &config.catalog,
        crate::PROVIDER,
        &declared,
        crate::SOURCE,
        &snapshots,
        &lines,
        window,
    ))
    .expect("a listing encodes"))
}

fn description(payload: &[u8]) -> Result<serde_json::Value, PluginsError> {
    let id = requested(payload)?;
    let config = config();
    let declared = source::declared(&config)?;
    let entry = declared.iter().find(|entry| entry.id == id).ok_or_else(|| {
        PluginsError::new(
            ErrorCode::NotFound,
            format!("{id:?} is not in catalog {:?}", config.catalog),
        )
    })?;
    let snapshots = composition()?;
    let (lines, window) = ledger(config.ledger_limit)?;
    let history = History::of(&id, lines, window);
    let described: Description = Catalog::describe(
        &config.catalog,
        crate::PROVIDER,
        entry,
        crate::SOURCE,
        snapshots.get(&id),
        &history,
        window,
    );
    Ok(serde_json::to_value(described).expect("a description encodes"))
}

/// A plugin's history. Deliberately NOT gated on the catalog's entry
/// set: the ledger is append-only, so a plugin that has left the document
/// still has every line it ever wrote, and asking the catalog first would
/// throw that away exactly when it matters.
fn history(payload: &[u8]) -> Result<serde_json::Value, PluginsError> {
    let id = requested(payload)?;
    let (lines, window) = ledger(config().ledger_limit)?;
    Ok(serde_json::to_value(History::of(&id, lines, window)).expect("a history encodes"))
}

/// The catalog's own word about itself.
fn describe_catalog() -> Result<serde_json::Value, PluginsError> {
    let config = config();
    let declared = source::declared(&config)?;
    Ok(serde_json::json!({
        "api-version": jinn_plugins::API_VERSION,
        "catalog": config.catalog,
        "contract": jinn_plugins::catalog_contract(&config.catalog),
        "served-by": crate::PROVIDER,
        "source": crate::SOURCE,
        "source-qualifier": crate::SOURCE.qualifier(),
        "entries": declared.len(),
        "ledger-limit": config.ledger_limit,
    }))
}

/// One operation.
pub fn dispatch(operation: &str, payload: &[u8]) -> Answer {
    let answered = match operation {
        OP_LIST => listing(),
        OP_DESCRIBE => description(payload),
        OP_HISTORY => history(payload),
        OP_DESCRIBE_CATALOG => describe_catalog(),
        other => Err(PluginsError::new(
            ErrorCode::NotFound,
            format!("unknown operation {other:?}"),
        )),
    };
    match answered {
        Ok(value) => Answer::ok(value),
        Err(error) => Answer::error(error),
    }
}

/// The shared half of `activate`: read the config, latch it, and answer
/// the catalog id the caller then PROVIDES. The provision is the
/// caller's last act, so nothing resolves this catalog before it can
/// answer.
///
/// # Errors
///
/// A config this provider cannot serve.
pub fn activate(config_bytes: &[u8]) -> Result<CatalogConfig, String> {
    let config: CatalogConfig = serde_json::from_slice(config_bytes)
        .map_err(|error| format!("malformed config: {error}"))?;
    if config.catalog.is_empty() {
        return Err(
            "config.catalog is the catalog id this provider serves; it cannot be empty".to_owned(),
        );
    }
    *CONFIG.lock().unwrap() = Some(config.clone());
    Ok(config)
}
