//! THE BUNDLE (packet UI-1): read ONCE — at `activate`, or on the
//! provider's `provided` event when the provider activates later (sibling
//! order is unspecified, FINDINGS.md #7) — verified fail-closed, held for
//! the incarnation's life, and served on every non-`/v1` GET BEFORE the
//! door with NO crossing: a byte is never a dispatch on the connection's
//! behalf, and a bearer on a static path is never read. Never per request:
//! a top-level navigation carries no bearer, and a read it caused would be
//! the crossing the door forbids. The rationale is
//! `docs/notes/2026-09-02-a-byte-is-not-a-dispatch.md`.

use jinn_api_http_wire::{framed, Request};
use jinn_ui::{
    is_api_path, serve, verify, Files, Manifest, Static, BUNDLE_CONTRACT, MIME_TEXT, OP_BUNDLE,
    OP_MANIFEST, PROVIDED_TOPIC as TOPIC,
};

use crate::exports::jinn::plugin::lifecycle::GuestFault;
use crate::jinn::plugin::types::KernelError;
use crate::jinn::plugin::{events, services};

/// The listen token of the provider's `provided` event.
pub(crate) const PROVIDED_TOKEN: u64 = 2;

/// The verified bundle this incarnation serves.
pub(crate) struct Bundle {
    manifest: Manifest,
    files: Files,
}

/// Reads and verifies the bundle: one `manifest` crossing, one `bundle`
/// crossing, then the check. `Ok(None)` when no provider is live YET —
/// the read completes on the `provided` event, which this registers a
/// listen for. Any other refusal — a grant missing, a blob that does not
/// verify — is this fiber's typed failure (R11: this entry, nothing else).
pub(crate) fn load() -> Result<Option<Bundle>, GuestFault> {
    let fault = |context: &str, error: &dyn std::fmt::Debug| {
        GuestFault::Failed(format!("{BUNDLE_CONTRACT}: {context}: {error:?}"))
    };
    let handle = services::resolve(BUNDLE_CONTRACT).map_err(|error| fault("resolve", &error))?;
    let manifest = match services::call(handle, OP_MANIFEST, &[]) {
        Ok(bytes) => bytes,
        Err(KernelError::MissingDependency(_)) => {
            events::listen(TOPIC, PROVIDED_TOKEN).map_err(|error| fault("listen", &error))?;
            return Ok(None);
        }
        Err(error) => return Err(fault(OP_MANIFEST, &error)),
    };
    let manifest: Manifest =
        serde_json::from_slice(&manifest).map_err(|error| fault("manifest decode", &error))?;
    let blob = services::call(handle, OP_BUNDLE, &[]).map_err(|error| fault(OP_BUNDLE, &error))?;
    let files = verify(&manifest, &blob).map_err(|error| fault("verify", &error))?;
    Ok(Some(Bundle { manifest, files }))
}

/// Whether the transport answers this request itself, with no door and
/// no crossing: everything that is not the operator API.
pub(crate) fn is_static(request: &Request) -> bool {
    !is_api_path(&request.path)
}

/// The static answer. `None` for the bundle means this profile mounts no
/// UI: `/v1` keeps serving and every page is a typed 503.
pub(crate) fn answer(bundle: Option<&Bundle>, request: &Request) -> Vec<u8> {
    let text = |status: u16, detail: &str| framed(status, MIME_TEXT, None, detail.as_bytes());
    if request.method != "GET" {
        return text(405, "static paths answer GET only");
    }
    let Some(bundle) = bundle else {
        return text(503, "no UI bundle is mounted in this profile");
    };
    match serve(&bundle.manifest, &bundle.files, &request.path) {
        Static::File {
            mime, cache, body, ..
        } => framed(200, mime, Some(cache), body),
        Static::NotFound => text(404, "no such asset"),
    }
}
