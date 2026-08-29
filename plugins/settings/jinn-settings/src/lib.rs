//! The `jinn:settings` service definition: names, the closed schema
//! language, typed secret references, layered resolution, the patch
//! plan, and the wire schemas — as pure functions. The prose law lives in
//! this crate's README; this code is its schema. Everything on the seam
//! is UTF-8 JSON with kebab-case keys.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

mod schema;

pub use schema::{validate, Field, Kind, Schema};

/// The schema version every answer carries.
pub const API_VERSION: &str = "0.1.0";
/// The contract a settings provider provides.
pub const SETTINGS_CONTRACT: &str = "jinn:settings";
/// The overlay store's contract (the hot layer's home in the profile).
pub const STORE_CONTRACT: &str = "jinn:settings-store";
/// `jinn:settings` operation: declare (or re-declare) a namespace and
/// answer its resolved settings.
pub const OP_DECLARE: &str = "declare";
/// `jinn:settings` operation: one namespace's resolved settings + layers.
pub const OP_GET: &str = "get";
/// `jinn:settings` operation: merge-patch one namespace, validated
/// against its schema BEFORE apply.
pub const OP_PATCH: &str = "patch";
/// `jinn:settings` operation: the declared namespaces.
pub const OP_NAMESPACES: &str = "namespaces";
/// `jinn:settings-store` operation: the overlays the store entry holds.
pub const OP_OVERLAYS: &str = "overlays";
/// Emitted (serial, all) after an applied patch, payload [`Changed`].
pub const CHANGED_TOPIC: &str = "jinn:settings/changed";
/// Emitted (emit, all) after a refused patch, payload [`Refused`] — the
/// refusal's ledger record (`DispatchTrace`), beside the typed answer.
pub const REFUSED_TOPIC: &str = "jinn:settings/refused";
/// The key of a typed secret reference: `{ "$secret": "<keystore key>" }`.
pub const SECRET_REF_KEY: &str = "$secret";

/// Unknown sibling fields, preserved across a decode → encode round trip.
pub type Extensions = serde_json::Map<String, serde_json::Value>;

/// A typed secret reference: names a keystore key, never carries the
/// secret. Resolution is the keystore seam's; the settings document holds
/// only the name.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct SecretRef {
    #[serde(rename = "$secret")]
    pub secret: String,
}

/// Whether `value` is a well-formed secret reference.
#[must_use]
pub fn is_secret_ref(value: &serde_json::Value) -> bool {
    serde_json::from_value::<SecretRef>(value.clone()).is_ok_and(|reference| {
        !reference.secret.is_empty() && value.as_object().is_some_and(|object| object.len() == 1)
    })
}

/// A namespace as its owner declares it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Declaration {
    pub namespace: String,
    /// The profile entry whose `config.data` is the namespace's entry layer
    /// — the entry a restart-path patch lands in.
    pub entry: String,
    pub schema: Schema,
    /// The bottom layer.
    #[serde(default)]
    pub defaults: serde_json::Value,
    /// Top-level keys the owner absorbs in place from a `changed` event: a
    /// patch touching only these lands in the overlay (hot path); any
    /// other key patches the entry and restarts the owner.
    #[serde(default)]
    pub hot_keys: Vec<String>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The three layers, bottom to top; each an object (or absent).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Layers {
    #[serde(default)]
    pub defaults: serde_json::Value,
    #[serde(default)]
    pub entry: serde_json::Value,
    #[serde(default)]
    pub overlay: serde_json::Value,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// RFC 7396 merge: objects merge recursively, `null` removes, anything
/// else replaces.
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

/// Layered resolution: defaults, then the entry's data, then the overlay,
/// each merged over the last (a higher layer's key wins; objects merge
/// recursively; a `null` in a higher layer removes).
#[must_use]
pub fn resolve(layers: &Layers) -> serde_json::Value {
    let mut resolved = serde_json::Value::Object(serde_json::Map::new());
    for layer in [&layers.defaults, &layers.entry, &layers.overlay] {
        if layer.is_object() {
            merge_patch(&mut resolved, layer);
        }
    }
    resolved
}

/// Which layer a patch lands in.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Applied {
    /// The overlay: the owner absorbs the `changed` event in place.
    Hot,
    /// The entry: the loader restarts the owner on the new config.
    Restart,
}

/// One of the three layers, by name — the layer a `shadowed` refusal
/// points at.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayerName {
    Defaults,
    Entry,
    Overlay,
}

/// The layer a `patch` addresses EXPLICITLY: the entry (restart path)
/// or the overlay (hot path). Absent, the keys choose (§The patch law).
/// The defaults are the owner's declaration and are not addressable.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PatchLayer {
    Entry,
    Overlay,
}

impl PatchLayer {
    #[must_use]
    pub fn applied(self) -> Applied {
        match self {
            Self::Entry => Applied::Restart,
            Self::Overlay => Applied::Hot,
        }
    }

    #[must_use]
    pub fn name(self) -> LayerName {
        match self {
            Self::Entry => LayerName::Entry,
            Self::Overlay => LayerName::Overlay,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Self::Entry => "entry",
            Self::Overlay => "overlay",
        }
    }
}

/// The exact call that clears the shadowing layer — `patch(namespace,
/// patch, layer)` — after which the refused patch, retried as it was,
/// lands. Executable through the seam as it stands; never advice that
/// returns the same refusal.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Recovery {
    pub namespace: String,
    pub patch: serde_json::Value,
    pub layer: PatchLayer,
}

/// Why a patch was refused as inconsistent: after landing in its layer,
/// `key` would resolve from `layer` instead of to the requested value.
/// `recovery` is the call that clears `layer` (absent when `layer` is
/// the defaults: a declared default cannot be removed, only set).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Shadowed {
    pub key: String,
    pub layer: LayerName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<Box<Recovery>>,
}

/// The typed error class of a refused or failed operation.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    NotFound,
    Invalid,
    #[default]
    Refused,
    Unavailable,
}

/// One typed error.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct SettingsError {
    pub code: ErrorCode,
    pub detail: String,
    /// Present on an `invalid` refusal of a patch the layers could not
    /// apply consistently (§The consistency guarantee in the README).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadowed: Option<Shadowed>,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl SettingsError {
    #[must_use]
    pub fn new(code: ErrorCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
            shadowed: None,
            extra: Extensions::new(),
        }
    }
}

/// The plan for one patch: the layer it lands in, that layer's new
/// value, and the settings the layers resolve to AFTER it lands —
/// computed from the post-state layers, so what a provider reports and
/// emits is by construction what the next `get` resolves.
#[derive(Clone, Debug, PartialEq)]
pub struct PatchPlan {
    pub applied: Applied,
    pub resolved: serde_json::Value,
    /// The new overlay (hot) or the new entry layer (restart).
    pub layer: serde_json::Value,
}

/// Plans one patch with the keys choosing the layer: [`plan_patch_in`]
/// with no explicit layer.
///
/// # Errors
///
/// As [`plan_patch_in`].
pub fn plan_patch(
    declaration: &Declaration,
    layers: &Layers,
    patch: &serde_json::Value,
) -> Result<PatchPlan, SettingsError> {
    plan_patch_in(declaration, layers, patch, None)
}

/// Plans one patch: the patch must be an object; the RESULT the caller
/// asks for (the resolved settings with the patch laid over them) must
/// validate against the schema; the patch lands in `layer` when given
/// (the overlay admits only hot keys to SET — the owner would never
/// honor a cold key there — and any key to clear), else a patch whose
/// every top-level key is a hot key lands in the overlay and any other
/// in the entry; and the layers must resolve to the asked-for result
/// once that layer is written — otherwise the patch is refused WHOLE as
/// `shadowed` (which key, which layer, and the call that clears it).
/// With an explicit layer a removal is the operator clearing THAT layer
/// and is never refused as shadowed: the plan reports what still
/// resolves. Nothing is applied here.
///
/// # Errors
///
/// `invalid` for a non-object patch, a result the schema refuses, a cold
/// key set in the overlay, or a result a layer above (or, for a
/// keys-chosen removal, below) the landing layer would shadow.
pub fn plan_patch_in(
    declaration: &Declaration,
    layers: &Layers,
    patch: &serde_json::Value,
    layer: Option<PatchLayer>,
) -> Result<PatchPlan, SettingsError> {
    let Some(keys) = patch.as_object() else {
        return Err(SettingsError::new(
            ErrorCode::Invalid,
            "a settings patch is a JSON object (RFC 7396 merge patch)",
        ));
    };
    let mut intended = resolve(layers);
    merge_patch(&mut intended, patch);
    let is_hot = |key: &String| declaration.hot_keys.iter().any(|hot| hot == key);
    let applied = match layer {
        Some(PatchLayer::Overlay) => {
            if let Some(cold) = keys
                .iter()
                .find(|(key, value)| !value.is_null() && !is_hot(key))
                .map(|(key, _)| key)
            {
                return Err(SettingsError::new(
                    ErrorCode::Invalid,
                    format!(
                        "{cold:?} is not a hot key: only a hot key lands in the overlay (the \
                         owner absorbs it in place); set it in the entry (layer: entry) or \
                         let the keys choose the layer"
                    ),
                ));
            }
            Applied::Hot
        }
        Some(PatchLayer::Entry) => Applied::Restart,
        None if !keys.is_empty() && keys.keys().all(is_hot) => Applied::Hot,
        None => Applied::Restart,
    };
    let base = match applied {
        Applied::Hot => &layers.overlay,
        Applied::Restart => &layers.entry,
    };
    let mut written = if base.is_object() {
        base.clone()
    } else {
        serde_json::Value::Object(serde_json::Map::new())
    };
    merge_patch(&mut written, patch);
    let after = match applied {
        Applied::Hot => Layers {
            overlay: written.clone(),
            ..layers.clone()
        },
        Applied::Restart => Layers {
            entry: written.clone(),
            ..layers.clone()
        },
    };
    let resolved = resolve(&after);
    // The schema decides membership of what the document WILL resolve:
    // the asked-for result when the keys choose the layer; the post-state
    // resolution when a layer is addressed explicitly (a removal there
    // may leave a lower layer supplying the key).
    let membership = match layer {
        None => &intended,
        Some(_) => &resolved,
    };
    validate(&declaration.schema, membership).map_err(|detail| {
        SettingsError::new(
            ErrorCode::Invalid,
            format!("schema refused the result: {detail}"),
        )
    })?;
    let consistent = match layer {
        None => resolved == intended,
        Some(_) => keys
            .iter()
            .filter(|(_, value)| !value.is_null())
            .all(|(key, _)| resolved.get(key) == intended.get(key)),
    };
    if !consistent {
        return Err(shadowed(
            &declaration.namespace,
            &intended,
            &resolved,
            &after,
        ));
    }
    Ok(PatchPlan {
        applied,
        resolved,
        layer: written,
    })
}

/// Names the first top-level key whose post-state value differs from the
/// asked-for one, the layer that value resolves from, and — unless that
/// layer is the defaults — the exact call that clears it.
fn shadowed(
    namespace: &str,
    intended: &serde_json::Value,
    resolved: &serde_json::Value,
    after: &Layers,
) -> SettingsError {
    let empty = serde_json::Map::new();
    let intended_keys = intended.as_object().unwrap_or(&empty);
    let resolved_keys = resolved.as_object().unwrap_or(&empty);
    let key = intended_keys
        .keys()
        .chain(resolved_keys.keys())
        .find(|key| intended_keys.get(*key) != resolved_keys.get(*key))
        .cloned()
        .unwrap_or_default();
    let holds = |layer: &serde_json::Value| layer.get(&key).is_some();
    let clears = if holds(&after.overlay) {
        Some(PatchLayer::Overlay)
    } else if holds(&after.entry) {
        Some(PatchLayer::Entry)
    } else {
        None
    };
    let (layer, detail, recovery) = match clears {
        Some(clears) => {
            let patch = serde_json::json!({ key.clone(): null });
            let detail = format!(
                "{key:?} is shadowed by the {} layer: the patch would not resolve to the \
                 requested value, so nothing was applied. Recovery: patch({namespace:?}, \
                 {patch}, layer: {}), then retry this patch",
                clears.word(),
                clears.word()
            );
            let recovery = Recovery {
                namespace: namespace.to_owned(),
                patch,
                layer: clears,
            };
            (clears.name(), detail, Some(Box::new(recovery)))
        }
        None => (
            LayerName::Defaults,
            format!(
                "{key:?} is a declared default and cannot be removed, so nothing was applied: \
                 set it to the value wanted instead"
            ),
            None,
        ),
    };
    let mut refused = SettingsError::new(ErrorCode::Invalid, detail);
    refused.shadowed = Some(Shadowed {
        key,
        layer,
        recovery,
    });
    refused
}

/// The `declare` request: the declaration and the owner's CURRENT entry
/// layer (its `config.data` as activated).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct DeclareRequest {
    #[serde(flatten)]
    pub declaration: Declaration,
    #[serde(default)]
    pub current: serde_json::Value,
}

/// The `get`/`declare` answer: the resolved settings, the layers they came
/// from, and the namespace's revision (bumped per applied patch within a
/// provider incarnation).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Resolved {
    pub api_version: String,
    pub namespace: String,
    pub entry: String,
    pub settings: serde_json::Value,
    pub layers: Layers,
    pub revision: u64,
    #[serde(default)]
    pub hot_keys: Vec<String>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `get` request.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct GetRequest {
    pub namespace: String,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `patch` request.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PatchRequest {
    pub namespace: String,
    #[serde(default)]
    pub patch: serde_json::Value,
    /// The layer to address explicitly; absent, the keys choose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<PatchLayer>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `patch` answer.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Patched {
    pub api_version: String,
    pub namespace: String,
    pub applied: Option<Applied>,
    pub settings: serde_json::Value,
    pub revision: u64,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `namespaces` answer.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Namespaces {
    pub api_version: String,
    pub namespaces: BTreeMap<String, NamespaceSummary>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One declared namespace, summarized.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct NamespaceSummary {
    pub entry: String,
    pub revision: u64,
    #[serde(default)]
    pub hot_keys: Vec<String>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `jinn:settings/changed` payload: the owner absorbs `settings` in
/// place (hot) — it never calls back into the provider from the handler
/// (the nested-dispatch class, FINDINGS.md #4/#26).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Changed {
    pub namespace: String,
    pub applied: Option<Applied>,
    pub settings: serde_json::Value,
    pub revision: u64,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `jinn:settings/refused` payload.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Refused {
    pub namespace: String,
    pub error: SettingsError,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The overlay store's `overlays` answer: per namespace, the overlay
/// object.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Overlays {
    #[serde(default)]
    pub overlays: BTreeMap<String, serde_json::Value>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The answer envelope: `{"api-version", "ok": …}` or `{"api-version",
/// "error": {…}}` — the operator-API seam's shape, so a transport that
/// speaks that envelope carries this seam unchanged.
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

/// One outcome.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Ok(serde_json::Value),
    Error(SettingsError),
}

impl Answer {
    #[must_use]
    pub fn ok<T: Serialize>(value: T) -> Self {
        Self::versioned(Outcome::Ok(
            serde_json::to_value(value).expect("an answer encodes"),
        ))
    }

    #[must_use]
    pub fn error(error: SettingsError) -> Self {
        Self::versioned(Outcome::Error(error))
    }

    fn versioned(outcome: Outcome) -> Self {
        Self {
            api_version: Some(API_VERSION.to_owned()),
            outcome,
            extra: Extensions::new(),
        }
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("an answer encodes")
    }

    /// Decodes one answer; a malformed one is a typed `refused`.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Self {
        serde_json::from_slice(bytes).unwrap_or_else(|error| {
            Self::error(SettingsError::new(
                ErrorCode::Refused,
                format!("malformed settings answer: {error}"),
            ))
        })
    }
}

#[cfg(test)]
mod tests;
