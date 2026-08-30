//! The `jinn:settings` service definition: names, the closed schema
//! language, typed secret references, layered resolution, the patch
//! plan, and the wire schemas — as pure functions. The prose law lives in
//! this crate's README; this code is its schema. Everything on the seam
//! is UTF-8 JSON with kebab-case keys.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

mod schema;
mod wire;

pub use schema::{validate, Field, Kind, Schema};
pub use wire::{decode_with_rest, encode_with_rest, optional, put, required, Additive};

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

/// A CLOSED surface's refusal, written once and shared by every closed
/// surface in the distribution (this crate's `SecretRef`, the engines
/// seam's value spaces).
///
/// A surface is CLOSED when it has nowhere to put content it cannot
/// name. Its law is then REFUSAL — never a silent drop, never a guess. A
/// drop is the silent-wrong-answer shape in its purest form: the peer
/// that sent the field is told the document was understood. Inside a
/// secret reference it is also a security property, since an unknown key
/// would ride along beside a credential name.
///
/// The message NAMES THE SURFACE, so an operator reading a refusal knows
/// which surface would have to be widened rather than only which value
/// was rejected.
///
/// `unnamed` is the offending content as prose ("the key `x`", "the value
/// `ultra`"); `admits` is what the surface does take.
pub fn closed<E: serde::de::Error>(surface: &str, unnamed: &str, admits: &str) -> E {
    E::custom(format!(
        "{surface} is a closed surface and REFUSES {unnamed} rather than dropping or guessing it \
         (it admits {admits})"
    ))
}

/// How [`closed`] names the secret-reference surface.
pub const SECRET_REF_SURFACE: &str = "a `{\"$secret\": \"<keystore key>\"}` reference";

/// A typed secret reference: names a keystore key, never carries the
/// secret. Resolution is the keystore seam's; the settings document holds
/// only the name.
///
/// The shape is CLOSED: `$secret` alone, and a sibling key is refused on
/// DECODE (see the hand-written [`Deserialize`], and [`closed`] for why
/// refusal rather than preservation). That is the one home for the fact —
/// every consumer of this type, in this seam or another, inherits it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SecretRef {
    #[serde(rename = "$secret")]
    pub secret: String,
}

impl<'de> Deserialize<'de> for SecretRef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut map = Extensions::deserialize(deserializer)?;
        let named = map
            .remove(SECRET_REF_KEY)
            .ok_or_else(|| serde::de::Error::missing_field(SECRET_REF_KEY))?;
        let secret = named
            .as_str()
            .ok_or_else(|| {
                serde::de::Error::custom(format!(
                    "{SECRET_REF_KEY} names a keystore key, so it is a string, not {named}"
                ))
            })?
            .to_owned();
        // The refusal, before the value is ever handed on: a reference
        // with a sibling is not a reference.
        if let Some(sibling) = map.keys().next() {
            return Err(closed(
                SECRET_REF_SURFACE,
                &format!("the key `{sibling}`"),
                SECRET_REF_KEY,
            ));
        }
        Ok(Self { secret })
    }
}

/// Whether `value` is a well-formed secret reference. The SHAPE is the
/// decoder's law (a sibling key is refused there, one home); what is left
/// here is the one thing a decoder cannot judge — a reference that names
/// nothing.
#[must_use]
pub fn is_secret_ref(value: &serde_json::Value) -> bool {
    serde_json::from_value::<SecretRef>(value.clone())
        .is_ok_and(|reference| !reference.secret.is_empty())
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

/// Why a patch was refused as inconsistent: a leaf the patch asks for
/// would resolve from `layer` instead of to the requested value, and
/// `path` is the NODE that layer resolves it with — the leaf itself, or
/// an atomic ancestor of it (README §The shadowing law). `key` is `path`
/// dot-joined (`group.inner`; a top-level key is itself). `recovery` is
/// the call that removes exactly the shadowing node(s) from `layer` — a
/// `null` at that path deletes it alone and preserves every sibling —
/// absent when `layer` is the defaults (a declared default cannot be
/// removed, only set).
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Shadowed {
    pub key: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<String>,
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
    let (asked, cleared) = asking(patch, layer.is_some());
    let mut intended = resolve(layers);
    merge_patch(&mut intended, &asked);
    let is_hot = |key: &String| declaration.hot_keys.iter().any(|hot| hot == key);
    let applied = match layer {
        Some(PatchLayer::Overlay) => {
            // A cold key is refused only when a leaf under it is SET; a
            // removal anywhere below it (a nested recovery) clears.
            if let Some(cold) = asked_leaves(patch, &[])
                .iter()
                .find(|(path, wanted)| wanted.is_some() && !is_hot(&path[0]))
                .map(|(path, _)| path[0].clone())
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
    // The formal definition of shadowing (README §The shadowing law),
    // implemented once. What the patch asks for is the requested
    // resolution `intended` — RFC 7396 applied to the document as it
    // resolves — under every key the patch names; an explicit-layer
    // removal asks for nothing (the operator is clearing that layer).
    // Every leaf where the post-state resolution diverges from it is
    // shadowed, and the layer resolving that leaf — the first in
    // precedence holding it or an ATOMIC value at a strict prefix of it
    // — is the shadowing layer, the node it holds there the shadowing
    // node.
    let target = match applied {
        Applied::Hot => LayerName::Overlay,
        Applied::Restart => LayerName::Entry,
    };
    let mut divergent = Vec::new();
    for key in keys.keys() {
        let path = vec![key.clone()];
        divergence(
            value_at(&intended, &path),
            value_at(&resolved, &path),
            &path,
            &cleared,
            &mut divergent,
        );
    }
    let mut shadowing: Vec<(LayerName, Vec<String>)> = Vec::new();
    for path in divergent {
        let node = match resolver(&after, &path) {
            Some((holder, node)) if holder != target => (holder, node),
            // Unreachable by the resolution law (a leaf that diverges is
            // held by another layer); named honestly if it ever were.
            _ => (target, path),
        };
        if !shadowing.contains(&node) {
            shadowing.push(node);
        }
    }
    if !shadowing.is_empty() {
        return Err(shadowed(&declaration.namespace, shadowing));
    }
    Ok(PatchPlan {
        applied,
        resolved,
        layer: written,
    })
}

/// The value at `path` (empty: the value itself).
fn value_at<'a>(value: &'a serde_json::Value, path: &[String]) -> Option<&'a serde_json::Value> {
    path.iter().try_fold(value, |value, key| value.get(key))
}

/// The leaf paths a merge patch asks for, in patch order: `Some(value)`
/// for a set (a non-object value — an array is atomic under RFC 7396 —
/// or an EMPTY object, an object-valued leaf that sets the key to an
/// object), `None` for a removal. A non-empty object recurses.
fn asked_leaves(
    patch: &serde_json::Value,
    prefix: &[String],
) -> Vec<(Vec<String>, Option<serde_json::Value>)> {
    let Some(fields) = patch.as_object() else {
        return Vec::new();
    };
    fields
        .iter()
        .flat_map(|(key, value)| {
            let path = [prefix, std::slice::from_ref(key)].concat();
            match value {
                serde_json::Value::Null => vec![(path, None)],
                serde_json::Value::Object(fields) if !fields.is_empty() => {
                    asked_leaves(value, &path)
                }
                _ => vec![(path, Some(value.clone()))],
            }
        })
        .collect()
}

/// The patch as an ask on the resolved document (what the requested
/// resolution lays over it), with the paths it asks nothing about. With
/// the keys choosing the layer it is the patch itself. With an explicit
/// layer a `null` is the operator clearing THAT layer and asks nothing
/// of the resolution, so it is dropped — and with it any object that
/// held nothing but removals (the container RFC 7396 creates for them
/// in that layer is part of the clearing); a literal `{}` stays, it
/// asks for an object. The dropped paths are returned beside the ask.
fn asking(patch: &serde_json::Value, explicit: bool) -> (serde_json::Value, Vec<Vec<String>>) {
    let mut cleared = Vec::new();
    let asked = if explicit {
        prune(patch, &[], &mut cleared)
    } else {
        patch.clone()
    };
    (asked, cleared)
}

fn prune(
    patch: &serde_json::Value,
    prefix: &[String],
    cleared: &mut Vec<Vec<String>>,
) -> serde_json::Value {
    let Some(fields) = patch.as_object() else {
        return patch.clone();
    };
    let mut kept = serde_json::Map::new();
    for (key, value) in fields {
        let path = [prefix, std::slice::from_ref(key)].concat();
        match value {
            serde_json::Value::Null => cleared.push(path),
            serde_json::Value::Object(inner) if !inner.is_empty() => {
                let mut below = Vec::new();
                let pruned = prune(value, &path, &mut below);
                if pruned.as_object().is_some_and(|inner| inner.is_empty()) {
                    cleared.push(path);
                } else {
                    cleared.extend(below);
                    kept.insert(key.clone(), pruned);
                }
            }
            _ => {
                kept.insert(key.clone(), value.clone());
            }
        }
    }
    serde_json::Value::Object(kept)
}

/// The leaves under `path` where `got` diverges from `wanted`: two
/// objects are walked together (a leaf either lacks is a divergence);
/// anything else that differs is one divergence at `path` itself — an
/// object where a removal or an atomic was asked for is named whole, so
/// its removal is one node. A path under `cleared` (an explicit-layer
/// removal) asks for nothing.
fn divergence(
    wanted: Option<&serde_json::Value>,
    got: Option<&serde_json::Value>,
    path: &[String],
    cleared: &[Vec<String>],
    out: &mut Vec<Vec<String>>,
) {
    if cleared.iter().any(|node| path.starts_with(node)) {
        return;
    }
    match (
        wanted.and_then(|v| v.as_object()),
        got.and_then(|v| v.as_object()),
    ) {
        (Some(wanted), Some(got)) => {
            let keys = wanted
                .keys()
                .chain(got.keys().filter(|k| !wanted.contains_key(*k)));
            for key in keys {
                let path = [path, std::slice::from_ref(key)].concat();
                divergence(wanted.get(key), got.get(key), &path, cleared, out);
            }
        }
        _ if wanted != got => out.push(path.to_vec()),
        _ => {}
    }
}

/// The layer that resolves `path`, and the node it resolves it with: the
/// layers walked in precedence (overlay, entry, defaults); the first
/// holding `path` itself, or an ATOMIC (non-object; `null` included)
/// value at a strict prefix of it, resolves it — the atomic ancestor
/// leaves nothing below it. One refinement the merge law forces: an
/// atomic a HIGHER layer already replaced with an object at that same
/// prefix resolves nothing (it still wiped every layer below it, so the
/// answer is then absent: `None`). `None` also when no layer holds it.
fn resolver(layers: &Layers, path: &[String]) -> Option<(LayerName, Vec<String>)> {
    // The deepest prefix depth at which a higher layer held an object.
    let mut floor = 0;
    let precedence = [
        (LayerName::Overlay, &layers.overlay),
        (LayerName::Entry, &layers.entry),
        (LayerName::Defaults, &layers.defaults),
    ];
    for (name, layer) in precedence {
        if !layer.is_object() {
            continue;
        }
        let mut node = layer;
        for depth in 1..=path.len() {
            match node.get(&path[depth - 1]) {
                None => {
                    floor = floor.max(depth - 1);
                    break;
                }
                Some(child) if child.is_object() && depth < path.len() => node = child,
                Some(child) if child.is_object() => return Some((name, path.to_vec())),
                Some(_) if depth <= floor => return None,
                Some(_) => return Some((name, path[..depth].to_vec())),
            }
        }
    }
    None
}

/// The merge patch that removes exactly the nodes at `paths` (RFC 7396:
/// a `null` at a nested path deletes that path alone; siblings survive).
/// The nodes are never nested in one another (an atomic has nothing
/// below it), so no `null` overwrites another.
fn removal_of(paths: &[Vec<String>]) -> serde_json::Value {
    let mut patch = serde_json::Value::Object(serde_json::Map::new());
    for path in paths {
        let mut node = &mut patch;
        for key in &path[..path.len() - 1] {
            node = node
                .as_object_mut()
                .expect("an object")
                .entry(key.clone())
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
        }
        node.as_object_mut()
            .expect("an object")
            .insert(path[path.len() - 1].clone(), serde_json::Value::Null);
    }
    patch
}

/// Names the shadowing nodes: `key`/`path` is the first, `layer` the
/// layer holding it, and — unless that layer is the defaults — the
/// recovery is the one call removing every shadowing node in that layer
/// (all of a patch's recoverable nodes lie in one layer: the overlay
/// shadows the entry's sets, the entry or overlay a removal from the
/// other). A node the defaults hold cannot be removed, only set, and is
/// named first when present, with no recovery.
fn shadowed(namespace: &str, nodes: Vec<(LayerName, Vec<String>)>) -> SettingsError {
    let (layer, path) = nodes
        .iter()
        .find(|(layer, _)| *layer == LayerName::Defaults)
        .unwrap_or(&nodes[0])
        .clone();
    let key = path.join(".");
    let clears = match layer {
        LayerName::Overlay => Some(PatchLayer::Overlay),
        LayerName::Entry => Some(PatchLayer::Entry),
        LayerName::Defaults => None,
    };
    let (detail, recovery) = match clears {
        Some(clears) => {
            let removed: Vec<Vec<String>> = nodes
                .iter()
                .filter(|(holder, _)| *holder == layer)
                .map(|(_, node)| node.clone())
                .collect();
            let patch = removal_of(&removed);
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
            (detail, Some(Box::new(recovery)))
        }
        None => (
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
        path,
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
