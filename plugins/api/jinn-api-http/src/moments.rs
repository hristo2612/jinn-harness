//! THE MOMENTS (packet UI-2, `docs/plans/ui-malleability-arc.md` §9.1):
//! `POST /v1/moments/<domain>/<topic>` behind the door exactly as every
//! `/v1` request, then ONE `waterfall` walk on the topic the path names
//! and the folded payload as the answer. The vocabulary is `jinn-ui`'s
//! and closed: a path it does not name is a 404 with no dispatch; a body
//! off the topic's schema is a 422 with no dispatch; a walk the kernel
//! refuses whole (`restarting`, `gone`, `suspended`, `stalled`, `cycle`)
//! is a `503 unavailable` naming the refusal — FAIL-CLOSED, never the
//! unmodified payload, because a validator extension is defeated by
//! fail-open. A walk with no listener answers the body.

use jinn_api::{ApiError, ErrorCode};
use jinn_api_http_wire::{error_response, response};
use jinn_ui::{moment_topic, refused_detail, validate_moment};

use crate::jinn::plugin::events;
use crate::jinn::plugin::types::{DispatchMode, KernelError, Selector};
use crate::route_miss;

/// One moment: the door has already opened for this request.
pub(crate) fn dispatch(method: &str, path: &str, body: &[u8]) -> Vec<u8> {
    let Some(topic) = moment_topic(path) else {
        return route_miss(false);
    };
    if method != "POST" {
        return route_miss(true);
    }
    if let Err(detail) = validate_moment(topic, body) {
        return error_response(&ApiError::new(ErrorCode::Invalid, detail));
    }
    match events::emit(topic, DispatchMode::Waterfall, &Selector::All, body) {
        // The waterfall's one output is the folded payload; with no
        // listener the kernel answers the payload itself.
        Ok(outputs) => response(200, &outputs.into_iter().next().unwrap_or_default()),
        Err(error) => error_response(&refusal(topic, &error)),
    }
}

/// A refused walk as the seam's typed error: the five whole-walk refusals
/// are `unavailable` with the case named first in `detail` and repeated
/// as the typed `refusal` field; anything else the kernel can answer is
/// the class its own case names.
fn refusal(topic: &str, error: &KernelError) -> ApiError {
    let (code, refusal, target) = match error {
        KernelError::Restarting(t) => (ErrorCode::Unavailable, "restarting", target_of(t)),
        KernelError::Gone(t) => (ErrorCode::Unavailable, "gone", target_of(t)),
        KernelError::Suspended(t) => (ErrorCode::Unavailable, "suspended", target_of(t)),
        KernelError::Stalled(t) => (ErrorCode::Unavailable, "stalled", target_of(t)),
        KernelError::Cycle(cycle) => (
            ErrorCode::Unavailable,
            "cycle",
            format!("{} awaits {} through {:?}", cycle.waiter, cycle.target, cycle.through),
        ),
        KernelError::GrantRefused(detail) => {
            return ApiError::new(ErrorCode::Refused, format!("emit refused: {detail}"))
        }
        KernelError::Invalid(detail) => {
            return ApiError::new(ErrorCode::Invalid, format!("emit refused: {detail}"))
        }
        other => (ErrorCode::Unavailable, "unavailable", format!("{other:?}")),
    };
    let mut error = ApiError::new(code, refused_detail(refusal, topic, &target));
    error
        .extra
        .insert("refusal".into(), serde_json::Value::String(refusal.into()));
    error
}

fn target_of(target: &crate::jinn::plugin::types::RefusedTarget) -> String {
    format!(
        "entry {} incarnation {} on {}",
        target.entry, target.incarnation, target.topic
    )
}
