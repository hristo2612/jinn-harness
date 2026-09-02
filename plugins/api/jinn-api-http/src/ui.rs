//! THE BUNDLE (harness packet UI-1). At `activate` this transport resolves
//! `jinn:ui-bundle` — when its profile says a bundle is mounted — reads
//! the manifest and the WHOLE archive as one crossing each, verifies every
//! file's sha256 against the manifest FAIL CLOSED (`jinn_ui::verify`; a
//! mismatch fails this fiber's activation and nothing else, R11), and
//! holds the verified files in memory for the fiber's life. From then on
//! a GET on any non-`/v1` path is answered from that memory by the
//! serving law — BEFORE the door and WITHOUT a crossing: a byte is never
//! a dispatch on the connection's behalf, so the door's contract ("no
//! dispatch before `verify` answers a principal") is untouched, and a
//! bearer presented on a static path is IGNORED — never read, never put
//! to the kernel. Every `/v1/*` request keeps the door exactly as packet
//! 2.8 left it.
//!
//! WHY ONCE, AT ACTIVATION. A browser's top-level navigation cannot carry
//! a bearer header, so a per-request read would be a crossing on an
//! unauthenticated connection's behalf — the very thing the door forbids.
//! Reading at activation as an injected dependency also makes a UI swap a
//! profile edit of the bundle entry's `package` and `hash`: the kernel's
//! epoch gating restarts this transport, which re-reads and serves the
//! new hash (proven in `tests/composition/tests/ui.rs`).

use jinn_api_http_wire::{framed, Request};
use jinn_ui::{
    is_api_path, serve, verify, Files, Manifest, Static, BUNDLE_CONTRACT, MIME_TEXT, OP_BUNDLE,
    OP_MANIFEST,
};

use crate::exports::jinn::plugin::lifecycle::GuestFault;
use crate::jinn::plugin::services;

/// The verified bundle this incarnation serves.
pub(crate) struct Bundle {
    manifest: Manifest,
    files: Files,
}

/// Reads and verifies the bundle: one `manifest` crossing, one `bundle`
/// crossing, then the check. Any refusal — no provider live yet (sibling
/// activation order is unspecified, FINDINGS.md #7), a grant missing, a
/// blob that does not verify — is this fiber's typed activation failure.
pub(crate) fn load() -> Result<Bundle, GuestFault> {
    let fault = |context: &str, error: &dyn std::fmt::Debug| {
        GuestFault::Failed(format!("{BUNDLE_CONTRACT}: {context}: {error:?}"))
    };
    let handle = services::resolve(BUNDLE_CONTRACT).map_err(|error| fault("resolve", &error))?;
    let manifest = services::call(handle, OP_MANIFEST, &[])
        .map_err(|error| fault(OP_MANIFEST, &error))?;
    let manifest: Manifest =
        serde_json::from_slice(&manifest).map_err(|error| fault("manifest decode", &error))?;
    let blob = services::call(handle, OP_BUNDLE, &[]).map_err(|error| fault(OP_BUNDLE, &error))?;
    let files = verify(&manifest, &blob).map_err(|error| fault("verify", &error))?;
    Ok(Bundle { manifest, files })
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
