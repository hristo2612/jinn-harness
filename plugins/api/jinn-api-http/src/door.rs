//! THE DOOR (harness packet 2.8, PLA-343). Before this provider issues
//! any dispatch on a connection's behalf, it puts the credential that
//! connection presented to the kernel's `jinn:auth` `verify` — ONE
//! granted crossing per request, never a grant cached across requests —
//! and dispatches only on a `principal`. The kernel owns the authority
//! (M2-K21: the credential of record beside the data root, read on every
//! call, deny by default, every decision an `AuthDecided` row under this
//! entry); this module owns the obligation the contract names as the
//! transport's ("WHAT A TRANSPORT OWES"), and the real-composition suite
//! proves it (`tests/composition/tests/auth.rs`).
//!
//! WHY THE BEARER HEADER. The presented credential is the token of an
//! `Authorization: Bearer` header (RFC 6750) and nothing else on the
//! request: it is what an operator's `curl -H` and a browser's `fetch`
//! carry natively; it never lands in a URL (query strings land in logs,
//! histories and referrers); one header carries one value, which is
//! exactly the contract's shape (one operator, one credential); a cookie
//! would make a session an identity and Basic auth would make a username
//! an account, and the packet forbids both. A request with no such header
//! presents NOTHING, and nothing is still put to the kernel — the door
//! never decides on its own, not even "there is nothing to check": the
//! decision point is one, and every decision is on the record (the row's
//! digest is then the empty string's, which an auditor can recognize).
//!
//! WHAT A REFUSAL IS. The kernel's `unauthenticated(reason)` is answered
//! under the seam's own class (`ErrorCode::Unauthenticated`, 401 with the
//! bearer challenge), carrying the kernel's reason and never the
//! presented bytes — distinct from `refused` (a grant or provider said
//! no; the profile to widen) and from `unavailable`. A door that cannot
//! ASK — the contract unresolvable, the crossing refused at the broker,
//! an answer off the contract's wire — is `refused`: closed, but named
//! as the composition's defect, not the operator's.

use jinn_api::{
    auth_api_error, decode_auth_answer, ApiError, ErrorCode, AUTH_CONTRACT, OP_VERIFY,
};
use jinn_api_http_wire::{error_response, Request};

use crate::jinn::plugin::services;

/// Opens the door for `request`, or answers why not. `Ok(())` means the
/// kernel answered a principal for what the connection presented; the
/// caller may now dispatch on the connection's behalf. `Err(wire)` is
/// the complete response to write and close on — no dispatch happens.
pub(crate) fn admit(request: &Request) -> Result<(), Vec<u8>> {
    let handle = services::resolve(AUTH_CONTRACT).map_err(|error| {
        // No grant, no door: closed, and named as the profile's gap.
        error_response(&ApiError::new(
            ErrorCode::Refused,
            format!("{AUTH_CONTRACT} is not resolvable: {error:?}"),
        ))
    })?;
    let presented = request.bearer.as_deref().unwrap_or_default();
    let answer = services::call(handle, OP_VERIFY, presented.as_bytes()).map_err(|error| {
        error_response(&ApiError::new(
            ErrorCode::Refused,
            format!("{AUTH_CONTRACT}/{OP_VERIFY} refused: {error:?}"),
        ))
    })?;
    decode_auth_answer(&answer)
        .map(|_principal| ())
        .map_err(|error| error_response(&auth_api_error(&error)))
}
