//! The sessions seam ON the operator surface: the routes, the store
//! list's schema, and the pure mapping of the sessions seam's typed error
//! onto this seam's. The sessions CONTRACT itself is not restated here —
//! its one home is `plugins/sessions/jinn-session/README.md`; this module
//! only says how an operator reaches it over a transport.
//!
//! Two parameters, as the engines surface has: a STORE and a session
//! within it. The store is in the path because a composition holds
//! several at once (one contract name per store id), so an operator
//! addresses the store they mean rather than the API guessing a default.

use jinn_session::{store_contract, ErrorCode as StoreErrorCode, SessionError};
use serde::{Deserialize, Serialize};

use crate::{ApiError, ErrorCode, Extensions, API_VERSION};

/// The sessions surface's path prefix.
pub const SESSIONS_PATH: &str = "/v1/sessions";

/// The methods the sessions surface answers. A path this table shapes
/// under another method is a method refusal, not a route miss.
pub const SESSION_METHODS: [&str; 3] = ["GET", "POST", "DELETE"];

/// One sessions route: which operation the path names, in which store.
/// `Stores` is the only one that is not a call on a provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionRoute {
    /// `GET /v1/sessions` — every store this API may route to.
    Stores,
    /// `GET /v1/sessions/{store}`
    List { store: String },
    /// `POST /v1/sessions/{store}`
    Create { store: String },
    /// `GET /v1/sessions/{store}/{session}`
    Get { store: String, session: String },
    /// `DELETE /v1/sessions/{store}/{session}`
    Close { store: String, session: String },
    /// `POST /v1/sessions/{store}/{session}/turns`
    Send { store: String, session: String },
    /// `DELETE /v1/sessions/{store}/{session}/turns`
    Cancel { store: String, session: String },
    /// `GET /v1/sessions/{store}/{session}/messages`
    Messages { store: String, session: String },
    /// `GET /v1/sessions/{store}/{session}/events`
    Events { store: String, session: String },
}

impl SessionRoute {
    /// The store the route addresses, if it addresses one.
    #[must_use]
    pub fn store(&self) -> Option<&str> {
        match self {
            Self::Stores => None,
            Self::List { store }
            | Self::Create { store }
            | Self::Get { store, .. }
            | Self::Close { store, .. }
            | Self::Send { store, .. }
            | Self::Cancel { store, .. }
            | Self::Messages { store, .. }
            | Self::Events { store, .. } => Some(store),
        }
    }

    /// The session the route addresses, if it addresses one.
    #[must_use]
    pub fn session(&self) -> Option<&str> {
        match self {
            Self::Stores | Self::List { .. } | Self::Create { .. } => None,
            Self::Get { session, .. }
            | Self::Close { session, .. }
            | Self::Send { session, .. }
            | Self::Cancel { session, .. }
            | Self::Messages { session, .. }
            | Self::Events { session, .. } => Some(session),
        }
    }

    /// The sessions-seam operation the route calls, if it calls one.
    #[must_use]
    pub fn operation(&self) -> Option<&'static str> {
        match self {
            Self::Stores => None,
            Self::List { .. } => jinn_session::OP_LIST.into(),
            Self::Create { .. } => jinn_session::OP_CREATE.into(),
            Self::Get { .. } => jinn_session::OP_GET.into(),
            Self::Close { .. } => jinn_session::OP_CLOSE.into(),
            Self::Send { .. } => jinn_session::OP_SEND.into(),
            Self::Cancel { .. } => jinn_session::OP_CANCEL.into(),
            Self::Messages { .. } => jinn_session::OP_MESSAGES.into(),
            Self::Events { .. } => jinn_session::OP_EVENTS.into(),
        }
    }

    /// Whether the route's payload comes from the request BODY. Every
    /// other route's payload is its query plus its path parameters — a
    /// read never takes a body, and a write never takes a query.
    #[must_use]
    pub fn takes_body(&self) -> bool {
        matches!(self, Self::Create { .. } | Self::Send { .. })
    }
}

/// Whether a path belongs to the sessions surface at all — the provider
/// asks before it consults the static route table, so a sessions path is
/// never answered by another route.
#[must_use]
pub fn is_sessions_path(path: &str) -> bool {
    path.strip_prefix(SESSIONS_PATH)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// Matches a method + path (query already stripped) against the sessions
/// surface. `None` is a miss — a malformed path, an unknown shape, or a
/// method this shape does not answer — and the caller answers it typed,
/// never by guessing a neighbouring route.
#[must_use]
pub fn session_route(method: &str, path: &str) -> Option<SessionRoute> {
    let rest = path.strip_prefix(SESSIONS_PATH)?;
    if rest.is_empty() {
        return (method == "GET").then_some(SessionRoute::Stores);
    }
    let mut segments = rest.strip_prefix('/')?.split('/');
    let store = segments.next().filter(|segment| !segment.is_empty())?;
    let store = store.to_owned();
    let Some(session) = segments.next().filter(|segment| !segment.is_empty()) else {
        return match method {
            "GET" => Some(SessionRoute::List { store }),
            "POST" => Some(SessionRoute::Create { store }),
            _ => None,
        };
    };
    let session = session.to_owned();
    let Some(collection) = segments.next() else {
        return match method {
            "GET" => Some(SessionRoute::Get { store, session }),
            "DELETE" => Some(SessionRoute::Close { store, session }),
            _ => None,
        };
    };
    if segments.next().is_some() {
        return None;
    }
    match (method, collection) {
        ("POST", "turns") => Some(SessionRoute::Send { store, session }),
        ("DELETE", "turns") => Some(SessionRoute::Cancel { store, session }),
        ("GET", "messages") => Some(SessionRoute::Messages { store, session }),
        ("GET", "events") => Some(SessionRoute::Events { store, session }),
        _ => None,
    }
}

/// The typed refusal for a store id this API may not route to. The GRANT
/// is the authority the kernel enforces; the configured list is the same
/// fact told to the provider, so an id in neither is simply not here —
/// answered without a kernel call.
#[must_use]
pub fn no_such_store(store: &str) -> ApiError {
    ApiError::new(
        ErrorCode::NotFound,
        format!("this API routes to no session store {store:?}"),
    )
}

/// The contract name a store may be reached under, or the typed refusal
/// when this API may not route to it.
///
/// # Errors
///
/// `not-found` for a store outside the configured list.
pub fn store_routable(stores: &[String], store: &str) -> Result<String, ApiError> {
    if stores.iter().any(|known| known == store) {
        Ok(store_contract(store))
    } else {
        Err(no_such_store(store))
    }
}

/// The request payload for one route: the caller's document (body or
/// query) with the PATH's session written over it. The path supplies the
/// session id; a body that names another session is not a second opinion.
#[must_use]
pub fn session_payload(route: &SessionRoute, document: serde_json::Value) -> serde_json::Value {
    let mut payload = match document {
        serde_json::Value::Object(fields) => serde_json::Value::Object(fields),
        _ => serde_json::json!({}),
    };
    if let Some(session) = route.session() {
        payload["session-id"] = serde_json::Value::String(session.to_owned());
    }
    // `create` takes `{ "spec": … }`. A body that is already shaped that
    // way is used as it is; a bare spec is wrapped, so an operator can
    // POST the thing the definition documents without a wrapper.
    if matches!(route, SessionRoute::Create { .. }) && payload.get("spec").is_none() {
        payload = serde_json::json!({ "spec": payload });
    }
    payload
}

/// The sessions seam's typed error as this seam's. `unavailable` — the
/// store is mounted and correct, this host cannot carry the call — stays
/// `unavailable` and so stays distinguishable from every other refusal,
/// and the store's own code rides along verbatim as `store-code`
/// (additive) so `failed` is never mistaken for `refused` by an operator
/// reading the answer.
#[must_use]
pub fn session_api_error(error: &SessionError) -> ApiError {
    let code = match error.code {
        StoreErrorCode::Invalid => ErrorCode::Invalid,
        StoreErrorCode::NotFound => ErrorCode::NotFound,
        StoreErrorCode::Refused | StoreErrorCode::Failed => ErrorCode::Refused,
        StoreErrorCode::Unavailable => ErrorCode::Unavailable,
    };
    let mut mapped = ApiError::new(code, error.message.clone());
    if let Ok(store_code) = serde_json::to_value(error.code) {
        mapped.extra.insert("store-code".into(), store_code);
    }
    mapped
}

/// One store answer decoded into this seam's outcome: the `ok` value, or
/// the typed error its code maps onto. A malformed answer is `refused` —
/// the provider spoke, and not this contract.
///
/// # Errors
///
/// The mapped [`ApiError`].
pub fn decode_session_answer(bytes: &[u8]) -> Result<serde_json::Value, ApiError> {
    let answer: jinn_session::Answer = serde_json::from_slice(bytes).map_err(|error| {
        ApiError::new(
            ErrorCode::Refused,
            format!("malformed session answer: {error}"),
        )
    })?;
    answer
        .into_result()
        .map_err(|error| session_api_error(&error))
}

/// One store on the operator surface: what its provider says about
/// itself (`describe`), or the typed reason it could not say.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct StoreEntry {
    pub store: String,
    /// The contract name it is served under — what a profile edit swaps.
    pub contract: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub describe: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ApiError>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `GET /v1/sessions` answer.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct StoreList {
    pub api_version: String,
    pub stores: Vec<StoreEntry>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// Assembles the store list from each configured store's `describe`
/// outcome: an unreachable store is a row with a typed error, never a
/// missing row and never a fault. Sorted by store id.
#[must_use]
pub fn store_list<I>(described: I) -> StoreList
where
    I: IntoIterator<Item = (String, Result<serde_json::Value, ApiError>)>,
{
    let mut stores: Vec<StoreEntry> = described
        .into_iter()
        .map(|(store, described)| {
            let (describe, error) = match described {
                Ok(description) => (Some(description), None),
                Err(error) => (None, Some(error)),
            };
            StoreEntry {
                contract: store_contract(&store),
                store,
                describe,
                error,
                extra: Extensions::new(),
            }
        })
        .collect();
    stores.sort_by(|left, right| left.store.cmp(&right.store));
    StoreList {
        api_version: API_VERSION.to_owned(),
        stores,
        extra: Extensions::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_surface_shapes_every_route_it_documents() {
        assert_eq!(
            session_route("GET", "/v1/sessions"),
            Some(SessionRoute::Stores)
        );
        assert_eq!(
            session_route("GET", "/v1/sessions/fs"),
            Some(SessionRoute::List { store: "fs".into() })
        );
        assert_eq!(
            session_route("POST", "/v1/sessions/fs"),
            Some(SessionRoute::Create { store: "fs".into() })
        );
        assert_eq!(
            session_route("GET", "/v1/sessions/fs/fs-1"),
            Some(SessionRoute::Get {
                store: "fs".into(),
                session: "fs-1".into()
            })
        );
        assert_eq!(
            session_route("DELETE", "/v1/sessions/fs/fs-1"),
            Some(SessionRoute::Close {
                store: "fs".into(),
                session: "fs-1".into()
            })
        );
        assert_eq!(
            session_route("POST", "/v1/sessions/fs/fs-1/turns"),
            Some(SessionRoute::Send {
                store: "fs".into(),
                session: "fs-1".into()
            })
        );
        assert_eq!(
            session_route("DELETE", "/v1/sessions/fs/fs-1/turns"),
            Some(SessionRoute::Cancel {
                store: "fs".into(),
                session: "fs-1".into()
            })
        );
        assert!(matches!(
            session_route("GET", "/v1/sessions/fs/fs-1/messages"),
            Some(SessionRoute::Messages { .. })
        ));
        assert!(matches!(
            session_route("GET", "/v1/sessions/fs/fs-1/events"),
            Some(SessionRoute::Events { .. })
        ));
    }

    #[test]
    fn a_shape_this_surface_does_not_answer_is_a_miss_not_a_guess() {
        // A method the shape does not answer is a miss here and a 405 at
        // the transport — never a neighbouring route's answer.
        assert!(session_route("DELETE", "/v1/sessions").is_none());
        assert!(session_route("POST", "/v1/sessions/fs/fs-1/messages").is_none());
        // An unknown collection, and a path with more segments than the
        // surface has.
        assert!(session_route("GET", "/v1/sessions/fs/fs-1/vibes").is_none());
        assert!(session_route("GET", "/v1/sessions/fs/fs-1/turns/t1").is_none());
        // Not this surface at all.
        assert!(!is_sessions_path("/v1/sessionsomething"));
        assert!(is_sessions_path("/v1/sessions") && is_sessions_path("/v1/sessions/fs"));
    }

    #[test]
    fn the_path_names_the_session_and_a_body_does_not_get_a_second_opinion() {
        let route = SessionRoute::Send {
            store: "fs".into(),
            session: "fs-1".into(),
        };
        let payload = session_payload(
            &route,
            serde_json::json!({ "session-id": "fs-9", "message": "hello" }),
        );
        assert_eq!(payload["session-id"], "fs-1");
        assert_eq!(payload["message"], "hello");
    }

    #[test]
    fn a_bare_spec_is_wrapped_and_a_wrapped_one_is_left_alone() {
        let route = SessionRoute::Create { store: "fs".into() };
        let bare = session_payload(
            &route,
            serde_json::json!({ "engine": { "engine": "echo" } }),
        );
        assert_eq!(bare["spec"]["engine"]["engine"], "echo");
        let wrapped = session_payload(
            &route,
            serde_json::json!({ "spec": { "engine": { "engine": "echo" } } }),
        );
        assert_eq!(wrapped["spec"]["engine"]["engine"], "echo");
        assert!(wrapped["spec"].get("spec").is_none());
    }

    #[test]
    fn a_store_this_api_may_not_route_to_is_refused_without_a_kernel_call() {
        let stores = vec!["fs".to_owned(), "memory".to_owned()];
        assert_eq!(
            store_routable(&stores, "fs").expect("routable"),
            "jinn:session.fs"
        );
        let refused = store_routable(&stores, "other").expect_err("not routable");
        assert_eq!(refused.code, ErrorCode::NotFound);
    }

    #[test]
    fn an_unreachable_store_is_a_row_with_a_reason_never_a_missing_row() {
        let list = store_list([
            (
                "memory".to_owned(),
                Ok(serde_json::json!({ "durable": false })),
            ),
            (
                "fs".to_owned(),
                Err(ApiError::new(ErrorCode::Unavailable, "no provider")),
            ),
        ]);
        assert_eq!(list.stores.len(), 2);
        assert_eq!(list.stores[0].store, "fs", "sorted by store id");
        assert!(list.stores[0].describe.is_none() && list.stores[0].error.is_some());
        assert_eq!(list.stores[1].contract, "jinn:session.memory");
    }

    #[test]
    fn an_unavailable_store_stays_unavailable_and_carries_its_own_code() {
        let mapped = session_api_error(&SessionError::new(
            StoreErrorCode::Unavailable,
            "the engine is not here",
        ));
        assert_eq!(mapped.code, ErrorCode::Unavailable);
        assert_eq!(mapped.extra["store-code"], "unavailable");
        // `failed` and `refused` both map onto this seam's `refused`, and
        // the store's own code is what tells them apart.
        for code in [StoreErrorCode::Failed, StoreErrorCode::Refused] {
            let mapped = session_api_error(&SessionError::new(code, "x"));
            assert_eq!(mapped.code, ErrorCode::Refused);
            assert_eq!(
                mapped.extra["store-code"],
                serde_json::to_value(code).expect("encodes")
            );
        }
    }
}
