//! The composition's shape over HTTP (pin `f8b285b`, jinnd M2-K23;
//! FINDINGS #37 closed by pin-bump 10): each admin route is ONE
//! `jinn:profile-admin` call from this entry — the operator's delegate
//! by the operator's own document (the kits grant it `scope: ["*"]`).
//! The pure half — which write a body names, the wire, the answer — is
//! the definition's (`jinn_api::profile_admin`); this module only
//! resolves the contract and crosses the broker.

use jinn_api::profile_admin::{
    admin_answer, admin_payload, admin_route, administered_answer, ADMIN_CONTRACT,
};
use jinn_api::{Answer, ApiError, ErrorCode};
use jinn_api_http_wire::error_response;

use crate::{answered, jinn::plugin::services};

/// One admin request → one contract call → one response; `None` when the
/// request is the config patch route's (or no route of this surface).
pub(crate) fn dispatch(method: &str, path: &str, body: &[u8]) -> Option<Vec<u8>> {
    // A body that is not JSON is `Null` here: a PATCH falls through to
    // the config route, which refuses it `invalid` in its own words; a
    // POST names no record and is `invalid` below.
    let document = serde_json::from_slice(body).unwrap_or(serde_json::Value::Null);
    let route = match admin_route(method, path, &document)? {
        Ok(route) => route,
        Err(error) => return Some(error_response(&error)),
    };
    let (operation, payload) = admin_payload(&route.id, &route.write);
    // A contract this entry cannot resolve is a grant it does not hold:
    // `refused`, the profile's to widen — never `unavailable`.
    let outcome = services::resolve(ADMIN_CONTRACT)
        .map_err(|error| {
            ApiError::new(
                ErrorCode::Refused,
                format!("{ADMIN_CONTRACT} is not resolvable from this entry (no grant?): {error:?}"),
            )
        })
        .and_then(|handle| {
            services::call(handle, operation, &payload).map_err(|error| {
                ApiError::new(
                    ErrorCode::Refused,
                    format!("{ADMIN_CONTRACT}/{operation} refused: {error:?}"),
                )
            })
        })
        .and_then(|bytes| admin_answer(operation, &bytes));
    Some(answered(&match outcome {
        Ok(seq) => Answer::ok(
            serde_json::to_value(administered_answer(&route.id, operation, seq)).expect("encodes"),
        ),
        Err(error) => Answer::error(error),
    }))
}
