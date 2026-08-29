//! The `jinn:settings` provider backed by the profile document. Every
//! layer has one home: defaults come with the owner's declaration, the
//! entry layer is the owner's `config.data` (declared as activated), the
//! overlay is the `jinn-settings-store` entry's `config.data.overlays`
//! (read through `jinn:settings-store` on every resolution — never
//! cached, so an operator's direct edit of the store entry shows on the
//! next read). A patch is planned by the definition (validated whole
//! BEFORE apply), then written through the kernel's `jinn:profile`:
//! `{ data: <patch> }` on the OWNER entry (restart path — the loader
//! restarts the owner on its new config) or `{ data: { overlays: { ns:
//! <patch> } } }` on the STORE entry (hot path — the trivial store fiber
//! restarts, the owner absorbs the `changed` event in place). Both are
//! `ProfilePatched` on the ledger under this entry; both leave the
//! profile the single source of truth. Refusals are typed answers AND a
//! `jinn:settings/refused` emit (its `DispatchTrace` is the ledger
//! record).
//!
//! Declarations live in this incarnation's memory: an owner re-declares
//! on every alarm wake (its `declare` answers the resolved settings), so
//! a provider restart or a provider SWAP heals within one owner wake
//! and `get` answers a typed `not-found` in between — FINDINGS.md #26
//! records why the owner never calls here from `activate`.

use std::collections::BTreeMap;
use std::sync::Mutex;

use jinn_api::{
    decode_profile_answer, profile_patch_payload, refusal_is_retryable, KERNEL_PROFILE_CONTRACT,
    OP_KERNEL_PATCH_ENTRY,
};
use jinn_settings::{
    plan_patch_in, resolve, Answer, Applied, Changed, DeclareRequest, Declaration, ErrorCode,
    GetRequest, Layers, NamespaceSummary, Namespaces, Overlays, PatchRequest, Patched, Refused,
    Resolved, SettingsError, API_VERSION, CHANGED_TOPIC, OP_DECLARE, OP_GET, OP_NAMESPACES,
    OP_OVERLAYS, OP_PATCH, REFUSED_TOPIC, SETTINGS_CONTRACT, STORE_CONTRACT,
};
use serde::Deserialize;

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::types::{DispatchMode, Selector};
use jinn::plugin::{effects, events, services};

const EFFECT_TOKEN: u64 = 1;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct ProviderConfig {
    /// The overlay store's entry id (the hot layer's home).
    store: String,
}

/// One declared namespace in this incarnation.
struct Held {
    declaration: Declaration,
    entry_layer: serde_json::Value,
    revision: u64,
}

static STORE: Mutex<String> = Mutex::new(String::new());
static HELD: Mutex<BTreeMap<String, Held>> = Mutex::new(BTreeMap::new());

fn fault(context: &str, error: impl std::fmt::Debug) -> GuestFault {
    GuestFault::Failed(format!("{context}: {error:?}"))
}

fn error(code: ErrorCode, detail: impl Into<String>) -> SettingsError {
    SettingsError::new(code, detail)
}

/// The overlay layer of one namespace, read from the store entry now.
fn overlay(namespace: &str) -> Result<serde_json::Value, SettingsError> {
    let handle = services::resolve(STORE_CONTRACT)
        .map_err(|refused| error(ErrorCode::Unavailable, format!("{STORE_CONTRACT}: {refused:?}")))?;
    let bytes = services::call(handle, OP_OVERLAYS, &[])
        .map_err(|refused| error(ErrorCode::Unavailable, format!("{OP_OVERLAYS}: {refused:?}")))?;
    let overlays: Overlays = serde_json::from_slice(&bytes)
        .map_err(|bad| error(ErrorCode::Invalid, format!("store answer: {bad}")))?;
    Ok(overlays
        .overlays
        .get(namespace)
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new())))
}

fn layers(held: &Held) -> Result<Layers, SettingsError> {
    Ok(Layers {
        defaults: held.declaration.defaults.clone(),
        entry: held.entry_layer.clone(),
        overlay: overlay(&held.declaration.namespace)?,
        extra: jinn_settings::Extensions::new(),
    })
}

fn resolved(held: &Held) -> Result<Resolved, SettingsError> {
    let layers = layers(held)?;
    Ok(Resolved {
        api_version: API_VERSION.to_owned(),
        namespace: held.declaration.namespace.clone(),
        entry: held.declaration.entry.clone(),
        settings: resolve(&layers),
        layers,
        revision: held.revision,
        hot_keys: held.declaration.hot_keys.clone(),
        extra: jinn_settings::Extensions::new(),
    })
}

fn declare(payload: &[u8]) -> Result<Resolved, SettingsError> {
    let request: DeclareRequest = serde_json::from_slice(payload)
        .map_err(|bad| error(ErrorCode::Invalid, format!("declare: {bad}")))?;
    if request.declaration.namespace.is_empty() || request.declaration.entry.is_empty() {
        return Err(error(ErrorCode::Invalid, "declare: namespace and entry are required"));
    }
    let mut held = HELD.lock().unwrap();
    let revision = held
        .get(&request.declaration.namespace)
        .map_or(0, |prior| prior.revision);
    let namespace = request.declaration.namespace.clone();
    held.insert(
        namespace.clone(),
        Held {
            declaration: request.declaration,
            entry_layer: request.current,
            revision,
        },
    );
    resolved(&held[&namespace])
}

fn get(payload: &[u8]) -> Result<Resolved, SettingsError> {
    let request: GetRequest = serde_json::from_slice(payload)
        .map_err(|bad| error(ErrorCode::Invalid, format!("get: {bad}")))?;
    let held = HELD.lock().unwrap();
    let Some(entry) = held.get(&request.namespace) else {
        return Err(error(
            ErrorCode::NotFound,
            format!("namespace {:?} is not declared in this provider incarnation", request.namespace),
        ));
    };
    resolved(entry)
}

fn namespaces() -> Namespaces {
    Namespaces {
        api_version: API_VERSION.to_owned(),
        namespaces: HELD
            .lock()
            .unwrap()
            .iter()
            .map(|(namespace, held)| {
                (
                    namespace.clone(),
                    NamespaceSummary {
                        entry: held.declaration.entry.clone(),
                        revision: held.revision,
                        hot_keys: held.declaration.hot_keys.clone(),
                        extra: jinn_settings::Extensions::new(),
                    },
                )
            })
            .collect(),
        extra: jinn_settings::Extensions::new(),
    }
}

/// One `jinn:profile` patch of `entry`, typed either way. Since pin
/// `3fd7b05` (0.2.0, FINDINGS.md #26) the call ACCEPTS and never awaits
/// the patched fiber's restart, so the two-hop deadlock class is gone;
/// the answered `ProfilePatched` sequence rides out as `patched-seq`.
fn kernel_patch(entry: &str, merge: &serde_json::Value) -> Result<Option<u64>, SettingsError> {
    let handle = services::resolve(KERNEL_PROFILE_CONTRACT).map_err(|refused| {
        error(ErrorCode::Refused, format!("{KERNEL_PROFILE_CONTRACT}: {refused:?}"))
    })?;
    let bytes = services::call(handle, OP_KERNEL_PATCH_ENTRY, &profile_patch_payload(entry, merge))
        .map_err(|refused| error(ErrorCode::Refused, format!("patch-entry: {refused:?}")))?;
    decode_profile_answer(&bytes).map_err(|reason| {
        let mut refused = error(ErrorCode::Refused, format!("patch-entry {entry:?} refused: {reason}"));
        refused.extra.insert(
            "retryable".into(),
            serde_json::Value::Bool(refusal_is_retryable(&reason)),
        );
        refused
    })
}

fn emit<T: serde::Serialize>(topic: &str, mode: DispatchMode, payload: &T) {
    // A refused or unheard emit is not this provider's failure: the
    // answer already carries the outcome, the emit is its record.
    let _ = events::emit(
        topic,
        mode,
        &Selector::All,
        &serde_json::to_vec(payload).expect("payload encodes"),
    );
}

fn patch(payload: &[u8]) -> Result<Patched, SettingsError> {
    let request: PatchRequest = serde_json::from_slice(payload)
        .map_err(|bad| error(ErrorCode::Invalid, format!("patch: {bad}")))?;
    let planned = {
        let held = HELD.lock().unwrap();
        let Some(entry) = held.get(&request.namespace) else {
            return Err(error(
                ErrorCode::NotFound,
                format!("namespace {:?} is not declared in this provider incarnation", request.namespace),
            ));
        };
        let layers = layers(entry)?;
        plan_patch_in(&entry.declaration, &layers, &request.patch, request.layer)
            .map(|plan| (plan, entry.declaration.entry.clone()))
    };
    let (plan, owner) = match planned {
        Ok(planned) => planned,
        Err(refused) => {
            emit(
                REFUSED_TOPIC,
                DispatchMode::Emit,
                &Refused {
                    namespace: request.namespace.clone(),
                    error: refused.clone(),
                    extra: jinn_settings::Extensions::new(),
                },
            );
            return Err(refused);
        }
    };
    let (target, merge) = match plan.applied {
        Applied::Restart => (owner, serde_json::json!({ "data": request.patch })),
        Applied::Hot => (
            STORE.lock().unwrap().clone(),
            serde_json::json!({ "data": { "overlays": { request.namespace.clone(): request.patch } } }),
        ),
    };
    let sequence = match kernel_patch(&target, &merge) {
        Ok(sequence) => sequence,
        Err(refused) => {
            emit(
                REFUSED_TOPIC,
                DispatchMode::Emit,
                &Refused {
                    namespace: request.namespace.clone(),
                    error: refused.clone(),
                    extra: jinn_settings::Extensions::new(),
                },
            );
            return Err(refused);
        }
    };
    let revision = {
        let mut held = HELD.lock().unwrap();
        let entry = held.get_mut(&request.namespace).expect("declared above");
        entry.revision += 1;
        if plan.applied == Applied::Restart {
            entry.entry_layer = plan.layer.clone();
        }
        entry.revision
    };
    // The owner absorbs the HOT layer in place from this event, so the
    // answer waits for it (serial). On the RESTART path the notice is
    // fire-and-forget: the owner re-declares on its own wake either way,
    // and since pin `3fd7b05` the patch does not await the restart it
    // schedules (FINDINGS.md #26), so a serial delivery here would be
    // aimed at a fiber the loader is replacing underneath it — the
    // dispatch waits for an incarnation that is waiting to be swapped,
    // and the operator's call stalls to its deadline. The blocking
    // `patch-entry` used to hide this by finishing the restart first.
    // FINDINGS.md #31 — this is a workaround, and it is only correct
    // because this provider KNOWS which layer it just wrote.
    let notice = match plan.applied {
        Applied::Hot => DispatchMode::Serial,
        Applied::Restart => DispatchMode::Emit,
    };
    emit(
        CHANGED_TOPIC,
        notice,
        &Changed {
            namespace: request.namespace.clone(),
            applied: Some(plan.applied),
            settings: plan.resolved.clone(),
            revision,
            extra: jinn_settings::Extensions::new(),
        },
    );
    let mut extra = jinn_settings::Extensions::new();
    if let Some(sequence) = sequence {
        extra.insert("patched-seq".into(), serde_json::json!(sequence));
    }
    Ok(Patched {
        api_version: API_VERSION.to_owned(),
        namespace: request.namespace,
        applied: Some(plan.applied),
        settings: plan.resolved,
        revision,
        extra,
    })
}

fn answer<T: serde::Serialize>(result: Result<T, SettingsError>) -> Answer {
    match result {
        Ok(value) => Answer::ok(value),
        Err(refused) => Answer::error(refused),
    }
}

struct Provider;

impl Guest for Provider {
    fn activate(config: Vec<u8>) -> Result<(), GuestFault> {
        let parsed: ProviderConfig = serde_json::from_slice(&config)
            .map_err(|bad| GuestFault::Failed(format!("malformed config: {bad}")))?;
        *STORE.lock().unwrap() = parsed.store;
        HELD.lock().unwrap().clear();
        effects::register("jinn-settings-profile on duty", EFFECT_TOKEN)
            .map_err(|refused| fault("effect", refused))?;
        services::provide(SETTINGS_CONTRACT).map_err(|refused| fault("provide", refused))?;
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
        let answered = match operation.as_str() {
            OP_DECLARE => answer(declare(&payload)),
            OP_GET => answer(get(&payload)),
            OP_PATCH => answer(patch(&payload)),
            OP_NAMESPACES => Answer::ok(namespaces()),
            other => Answer::error(error(
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

export!(Provider);
