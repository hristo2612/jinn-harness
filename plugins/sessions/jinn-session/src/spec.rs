//! What a caller asks for: the session spec, the engine binding, and the
//! request/answer documents of the operations that take one.

use jinn_engine::{Effort, ToolPolicy};
use serde::{Deserialize, Serialize};

use crate::{Extensions, API_VERSION};

/// Which engine a session's turns run on. The binding names the engines
/// seam's ID — `jinn_engine::engine_contract` turns it into the contract a
/// provider resolves — so a session is bound to a DEFINITION, never to a
/// particular provider. Changing this one field is the whole of "run the
/// same session spec over another engine".
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct EngineBinding {
    /// The engine id: the route, and hence the contract name.
    pub engine: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// Who owns a session. Attribution is recorded, never inferred: a session
/// with no declared owner says so rather than guessing one.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Attribution {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One session as it is asked for.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct SessionSpec {
    #[serde(default)]
    pub api_version: String,
    /// The engine binding — by definition, never by provider.
    pub engine: EngineBinding,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Default-deny, exactly as the engines seam reads it: an absent
    /// policy admits no tool.
    #[serde(default)]
    pub tools: ToolPolicy,
    #[serde(default)]
    pub attribution: Attribution,
    /// Operator metadata, carried verbatim and never interpreted.
    #[serde(default)]
    pub metadata: Extensions,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// `create`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct CreateRequest {
    pub spec: SessionSpec,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `create` answer.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct SessionCreated {
    #[serde(default)]
    pub api_version: String,
    pub session_id: String,
    pub store: String,
    pub engine: String,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// `send`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct SendRequest {
    pub session_id: String,
    pub message: String,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `send` answer: the turn was accepted, here is its handle. The
/// turn's progress arrives on the bus.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TurnAccepted {
    #[serde(default)]
    pub api_version: String,
    pub session_id: String,
    pub turn_id: String,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `{ "session-id": ... }` document. ONE shape names a session, and
/// every operation that takes one reads it: a second spelling would be an
/// interop split between stores for no gain.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct GetRequest {
    pub session_id: String,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// See [`GetRequest`] — the same document, named for `cancel`.
pub type CancelRequest = GetRequest;
/// See [`GetRequest`] — the same document, named for `close`.
pub type CloseRequest = GetRequest;

/// `messages`: one page of a session's log. `offset` counts MESSAGES from
/// the start, never bytes, so a page is stable across a re-read.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct MessagesRequest {
    pub session_id: String,
    #[serde(default)]
    pub offset: u64,
    /// Absent means the store's own page size — never "everything".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// `list`. An absent filter lists every session this store holds.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ListRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl SessionSpec {
    /// The spec as a peer receives it back, with this version stamped on.
    #[must_use]
    pub fn versioned(mut self) -> Self {
        self.api_version = API_VERSION.to_owned();
        self
    }
}

jinn_settings::additive!(
    EngineBinding,
    Attribution,
    SessionSpec,
    CreateRequest,
    SessionCreated,
    SendRequest,
    TurnAccepted,
    GetRequest,
    MessagesRequest,
    ListRequest,
);
