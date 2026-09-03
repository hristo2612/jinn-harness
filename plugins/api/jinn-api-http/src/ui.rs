//! THE BUNDLE (packet UI-1): read ONCE per incarnation, at `activate`,
//! served before the door with no crossing
//! (`docs/notes/2026-09-02-a-byte-is-not-a-dispatch.md`). The entry
//! DECLARES the bundle it injects (`injects: ["jinn:ui-bundle"]` beside
//! its grants; pin `a53a352`, M2-K24), so the kernel activates this
//! transport only once the provider is Active and restarts it when the
//! bundle is swapped (`docs/notes/2026-09-03-a-declaration-is-a-gate.md`).

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

/// The one read: `manifest`, `bundle`, the check. Every refusal is this
/// entry's own fault and fails its activation (R11): the kernel's gate on
/// the declared `injects` makes "the provider is Active" a premise here,
/// not a hope — a provider that is missing, loading or sealed for a swap
/// is a fiber the kernel has not activated yet, never one that reads
/// `not yet` and waits.
pub(crate) fn read() -> Result<Bundle, GuestFault> {
    let fault = |context: &str, error: &dyn std::fmt::Debug| {
        GuestFault::Failed(format!("{BUNDLE_CONTRACT}: {context}: {error:?}"))
    };
    let handle = services::resolve(BUNDLE_CONTRACT).map_err(|error| fault("resolve", &error))?;
    let manifest =
        services::call(handle, OP_MANIFEST, &[]).map_err(|error| fault(OP_MANIFEST, &error))?;
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
