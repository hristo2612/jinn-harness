//! THE BUNDLE (packet UI-1): read once per incarnation — at `activate`, or
//! on the kernel's witnessed Active transition (FINDINGS.md #45) — served
//! before the door, no crossing. `docs/notes/2026-09-02-a-byte-is-not-a-dispatch.md`.

use jinn_api_http_wire::{framed, Request};
use jinn_plugins::{Transition, TRANSITIONS_TOPIC};
use jinn_ui::{
    is_api_path, serve, verify, Files, Manifest, Static, BUNDLE_CONTRACT, MIME_TEXT, OP_BUNDLE,
    OP_MANIFEST,
};

use crate::exports::jinn::plugin::lifecycle::GuestFault;
use crate::jinn::plugin::types::KernelError;
use crate::jinn::plugin::{events, services};

/// The listen token of the kernel's transitions publish.
pub(crate) const TRANSITIONS_TOKEN: u64 = 2;

/// The verified bundle this incarnation serves.
pub(crate) struct Bundle {
    pub(crate) manifest: Manifest,
    files: Files,
}


/// A refusal that means "not yet": the kernel will say when it has moved.
/// A handle gone stale between resolve and call is one (the provider's
/// generation landed in between); the next read resolves afresh. So is a
/// provider whose seat is sealed or closing (`inactive-context`: a swap in
/// flight) and a provider that failed while answering (`provider-failed`:
/// its instance trapped, hung or is gone — contained to ITS fiber per R11,
/// which the kernel fails or restarts; this transport rests active
/// without a bundle and reads on the witnessed Active transition rather
/// than dying of a sibling's fault). What stays a fault is this entry's
/// own: a grant refused, a cycle, a malformed answer, a verify mismatch.
fn not_yet(error: &KernelError) -> bool {
    matches!(
        error,
        KernelError::MissingDependency(_)
            | KernelError::Restarting(_)
            | KernelError::Gone(_)
            | KernelError::Suspended(_)
            | KernelError::Stalled(_)
            | KernelError::InactiveContext
            | KernelError::ProviderFailed(_)
    ) || format!("{error:?}").contains("stale handle")
}

/// One read: `manifest`, `bundle`, the check. `Ok(None)` when the provider
/// is not live yet or answers the `held` hash; any other refusal is R11's.
pub(crate) fn read(held: Option<&str>) -> Result<Option<Bundle>, GuestFault> {
    let fault = |context: &str, error: &dyn std::fmt::Debug| {
        GuestFault::Failed(format!("{BUNDLE_CONTRACT}: {context}: {error:?}"))
    };
    let handle = services::resolve(BUNDLE_CONTRACT).map_err(|error| fault("resolve", &error))?;
    let manifest = match services::call(handle, OP_MANIFEST, &[]) {
        Ok(bytes) => bytes,
        Err(error) if not_yet(&error) => return Ok(None),
        Err(error) => return Err(fault(OP_MANIFEST, &error)),
    };
    let manifest: Manifest =
        serde_json::from_slice(&manifest).map_err(|error| fault("manifest decode", &error))?;
    if held == Some(manifest.bundle_sha256.as_str()) {
        return Ok(None);
    }
    let blob = services::call(handle, OP_BUNDLE, &[]).map_err(|error| fault(OP_BUNDLE, &error))?;
    let files = verify(&manifest, &blob).map_err(|error| fault("verify", &error))?;
    Ok(Some(Bundle { manifest, files }))
}

/// At activation: read now, or subscribe to the kernel's transitions and
/// read again (the second attempt closes the window before the listen).
pub(crate) fn load() -> Result<Option<Bundle>, GuestFault> {
    if let Some(bundle) = read(None)? {
        return Ok(Some(bundle));
    }
    events::listen(TRANSITIONS_TOPIC, TRANSITIONS_TOKEN)
        .map_err(|error| GuestFault::Failed(format!("{TRANSITIONS_TOPIC}: listen: {error:?}")))?;
    read(None)
}

/// Whether a witnessed transition is the bundle entry reaching Active.
pub(crate) fn completes(entry: &str, payload: &[u8]) -> bool {
    serde_json::from_slice::<Transition>(payload)
        .is_ok_and(|seen| seen.entry == entry && seen.to.eq_ignore_ascii_case("active"))
}

/// Whether the transport answers this request itself, with no door and
/// no crossing: everything that is not the operator API.
pub(crate) fn is_static(request: &Request) -> bool {
    !is_api_path(&request.path)
}

/// The static answer; no bundle mounted is a typed 503 on every page.
pub(crate) fn answer(bundle: Option<&Bundle>, request: &Request) -> Vec<u8> {
    let text = |status: u16, detail: &str| framed(status, MIME_TEXT, None, detail.as_bytes());
    if request.method != "GET" {
        return text(405, "static paths answer GET only");
    }
    // No bundle: `/` says so; any other path is 2.8's typed route miss.
    let Some(bundle) = bundle else {
        return if request.path == "/" {
            text(503, "no UI bundle is mounted in this profile")
        } else {
            crate::route_miss(false)
        };
    };
    match serve(&bundle.manifest, &bundle.files, &request.path) {
        Static::File {
            mime, cache, body, ..
        } => framed(200, mime, Some(cache), body),
        Static::NotFound => text(404, "no such asset"),
    }
}
