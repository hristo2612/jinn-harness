//! The MOMENT vocabulary (UI-2, `docs/plans/ui-malleability-arc.md` §9.2):
//! four closed topics, each payload's schema, and the path law. Pure
//! types — the prose law (fail-closed, the walk, what an extension may
//! do) is this crate's README; this is its schema.
//!
//! A moment is a `waterfall` walk on a `jinn:ui/<topic>` topic that the
//! transport dispatches for an authenticated `POST /v1/moments/<domain>/<topic>`
//! and answers with the FOLDED payload. The vocabulary is CLOSED (R3):
//! `moment_topic` maps a path to a topic for exactly the topics named
//! here and to nothing for anything else — an unnamed path is a 404 with
//! no dispatch, never a topic forwarded for the kernel to refuse.

use serde::{Deserialize, Serialize};

use crate::Extensions;

/// The route family's root; every moment is `POST` under it.
pub const MOMENTS_PATH: &str = "/v1/moments";
/// Inventory §4.3 moment 1: the composer's send, before the optimistic
/// bubble (§6 traces it).
pub const TOPIC_BEFORE_SEND: &str = "jinn:ui/before-send";
/// Inventory §4.3 moment 3: a session about to be created, with the
/// sessions seam's `SessionSpec` as its payload.
pub const TOPIC_BEFORE_CREATE_SESSION: &str = "jinn:ui/before-create-session";
/// Inventory §4.3 moment 19: a settings namespace about to be patched —
/// reached by the Settings page before its ported write.
pub const TOPIC_BEFORE_PATCH_SETTINGS: &str = "jinn:ui/before-patch-settings";
/// The shell's offered navigation, after availability is derived.
pub const TOPIC_AFTER_BUILD_NAVIGATION: &str = "jinn:ui/after-build-navigation";
/// Every topic a moment may be dispatched on. Closed.
pub const MOMENT_TOPICS: [&str; 4] = [
    TOPIC_AFTER_BUILD_NAVIGATION,
    TOPIC_BEFORE_SEND,
    TOPIC_BEFORE_CREATE_SESSION,
    TOPIC_BEFORE_PATCH_SETTINGS,
];
/// The five refusals a walk can be answered with whole, each a typed
/// `kernel-error` case (`kernel-pin/wit/plugin.wit`; M2-K9, M2-K10). A
/// refused walk is `503 unavailable` naming the case, never the
/// unmodified payload.
pub const WALK_REFUSALS: [&str; 5] = ["restarting", "gone", "suspended", "stalled", "cycle"];

/// `jinn:ui/before-send`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BeforeSend {
    pub text: String,
    #[serde(default)]
    pub attachments: Vec<serde_json::Value>,
    pub session_id: String,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// `jinn:ui/before-create-session`: the sessions seam's own request
/// shape, from its one home.
pub type BeforeCreateSession = jinn_session::SessionSpec;

/// `jinn:ui/before-patch-settings`: the namespace and the merge patch the
/// Settings page is about to send (`PATCH /v1/settings/{ns}`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BeforePatchSettings {
    pub namespace: String,
    pub patch: serde_json::Map<String, serde_json::Value>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// Navigation descriptors carry no URLs or authority beyond their offered IDs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NavigationItem {
    pub id: String,
    pub label: String,
    pub provided: bool,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The shell's offered desktop and mobile destinations, folded by listeners.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AfterBuildNavigation {
    pub items: Vec<NavigationItem>,
    pub mobile_items: Vec<NavigationItem>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// Whether a path is under the moments route family at all (the
/// transport's surface split; a miss under it is still this family's
/// typed 404).
#[must_use]
pub fn is_moments_path(path: &str) -> bool {
    path == MOMENTS_PATH
        || path
            .strip_prefix(MOMENTS_PATH)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// The path law: `/v1/moments/<domain>/<topic>` names `jinn:<domain>/<topic>`
/// for EXACTLY the topics in [`MOMENT_TOPICS`], byte for byte — no case
/// folding, no `..`, no trailing slash — and nothing else.
#[must_use]
pub fn moment_topic(path: &str) -> Option<&'static str> {
    let rest = path.strip_prefix(MOMENTS_PATH)?.strip_prefix('/')?;
    MOMENT_TOPICS
        .iter()
        .copied()
        .find(|topic| topic.strip_prefix("jinn:") == Some(rest))
}

/// The payload's schema check, BEFORE the walk: a miss is the seam's
/// `invalid` (422) with no dispatch. The schema binds the client's
/// input; the walk's OUTPUT is the listeners' and is not re-checked.
///
/// # Errors
///
/// The body is not the topic's shape, with serde's reason.
pub fn validate_moment(topic: &str, body: &[u8]) -> Result<(), String> {
    let shaped = match topic {
        TOPIC_AFTER_BUILD_NAVIGATION => {
            serde_json::from_slice::<AfterBuildNavigation>(body).map(drop)
        }
        TOPIC_BEFORE_SEND => serde_json::from_slice::<BeforeSend>(body).map(drop),
        TOPIC_BEFORE_CREATE_SESSION => {
            serde_json::from_slice::<BeforeCreateSession>(body).map(drop)
        }
        TOPIC_BEFORE_PATCH_SETTINGS => {
            serde_json::from_slice::<BeforePatchSettings>(body).map(drop)
        }
        other => return Err(format!("{other} is not a moment topic")),
    };
    shaped.map_err(|error| format!("{topic}: {error}"))
}

/// The `detail` of a refused walk's `unavailable` answer: the refusal's
/// name first, so a client reads its next move off the first word.
#[must_use]
pub fn refused_detail(refusal: &str, topic: &str, target: &str) -> String {
    format!("{refusal}: the walk on {topic} was refused whole by the kernel ({target}); nothing was delivered and the payload is NOT answered unmodified")
}
