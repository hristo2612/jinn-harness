//! The todos seam ON the operator surface: the routes, the store list's
//! schema, and the pure mapping of the todos seam's typed error onto this
//! seam's. The todos CONTRACT itself is not restated here — its one home
//! is `plugins/todos/jinn-todo/README.md`; this module only says how an
//! operator reaches it over a transport.
//!
//! Two parameters, as the sessions surface has: a STORE and a Todo within
//! it. The store is in the path because a composition holds several at
//! once (one contract name per store id), so an operator addresses the
//! store they mean rather than the API guessing a default.
//!
//! # A refused move keeps its shape all the way out
//!
//! An illegal status transition comes back from a store as `refused` with
//! `from` and `to` beside the message. [`todo_api_error`] carries them
//! through verbatim, so an operator reading a 4xx sees the attempted move
//! as DATA and never has to parse prose to know which one it was.

use jinn_todo::{store_contract, ErrorCode as StoreErrorCode, TodoError};
use serde::{Deserialize, Serialize};

use crate::{ApiError, ErrorCode, Extensions, API_VERSION};

/// The todos surface's path prefix.
pub const TODOS_PATH: &str = "/v1/todos";

/// The methods the todos surface answers. A path this table shapes under
/// another method is a method refusal, not a route miss.
pub const TODO_METHODS: [&str; 2] = ["GET", "POST"];

/// One todos route: which operation the path names, in which store.
/// `Stores` is the only one that is not a call on a provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TodoRoute {
    /// `GET /v1/todos` — every store this API may route to.
    Stores,
    /// `GET /v1/todos/{store}`
    List { store: String },
    /// `POST /v1/todos/{store}`
    Create { store: String },
    /// `GET /v1/todos/{store}/{todo}`
    Get { store: String, todo: String },
    /// `POST /v1/todos/{store}/{todo}/status`
    Update { store: String, todo: String },
    /// `POST /v1/todos/{store}/{todo}/comments`
    Comment { store: String, todo: String },
    /// `POST /v1/todos/{store}/{todo}/dispatch`
    Dispatch { store: String, todo: String },
    /// `GET /v1/todos/{store}/{todo}/tree`
    Tree { store: String, todo: String },
    /// `GET /v1/todos/{store}/{todo}/events`
    Events { store: String, todo: String },
}

impl TodoRoute {
    /// The store the route addresses, if it addresses one.
    #[must_use]
    pub fn store(&self) -> Option<&str> {
        match self {
            Self::Stores => None,
            Self::List { store }
            | Self::Create { store }
            | Self::Get { store, .. }
            | Self::Update { store, .. }
            | Self::Comment { store, .. }
            | Self::Dispatch { store, .. }
            | Self::Tree { store, .. }
            | Self::Events { store, .. } => Some(store),
        }
    }

    /// The Todo the route addresses, if it addresses one.
    #[must_use]
    pub fn todo(&self) -> Option<&str> {
        match self {
            Self::Stores | Self::List { .. } | Self::Create { .. } => None,
            Self::Get { todo, .. }
            | Self::Update { todo, .. }
            | Self::Comment { todo, .. }
            | Self::Dispatch { todo, .. }
            | Self::Tree { todo, .. }
            | Self::Events { todo, .. } => Some(todo),
        }
    }

    /// The todos-seam operation the route calls, if it calls one.
    #[must_use]
    pub fn operation(&self) -> Option<&'static str> {
        match self {
            Self::Stores => None,
            Self::List { .. } => jinn_todo::OP_LIST.into(),
            Self::Create { .. } => jinn_todo::OP_CREATE.into(),
            Self::Get { .. } => jinn_todo::OP_GET.into(),
            Self::Update { .. } => jinn_todo::OP_UPDATE.into(),
            Self::Comment { .. } => jinn_todo::OP_COMMENT.into(),
            Self::Dispatch { .. } => jinn_todo::OP_DISPATCH.into(),
            Self::Tree { .. } => jinn_todo::OP_TREE.into(),
            Self::Events { .. } => jinn_todo::OP_EVENTS.into(),
        }
    }

    /// Whether the route's payload comes from the request BODY. Every
    /// other route's payload is its query plus its path parameters — a
    /// read never takes a body, and a write never takes a query.
    #[must_use]
    pub fn takes_body(&self) -> bool {
        matches!(
            self,
            Self::Create { .. }
                | Self::Update { .. }
                | Self::Comment { .. }
                | Self::Dispatch { .. }
        )
    }
}

/// Whether a path belongs to the todos surface at all — the provider asks
/// before it consults the static route table, so a todos path is never
/// answered by another route.
#[must_use]
pub fn is_todos_path(path: &str) -> bool {
    path.strip_prefix(TODOS_PATH)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// Matches a method + path (query already stripped) against the todos
/// surface. `None` is a miss — a malformed path, an unknown shape, or a
/// method this shape does not answer — and the caller answers it typed,
/// never by guessing a neighbouring route.
#[must_use]
pub fn todo_route(method: &str, path: &str) -> Option<TodoRoute> {
    let rest = path.strip_prefix(TODOS_PATH)?;
    if rest.is_empty() {
        return (method == "GET").then_some(TodoRoute::Stores);
    }
    let mut segments = rest.strip_prefix('/')?.split('/');
    let store = segments.next().filter(|segment| !segment.is_empty())?;
    let store = store.to_owned();
    let Some(todo) = segments.next().filter(|segment| !segment.is_empty()) else {
        return match method {
            "GET" => Some(TodoRoute::List { store }),
            "POST" => Some(TodoRoute::Create { store }),
            _ => None,
        };
    };
    let todo = todo.to_owned();
    let Some(collection) = segments.next() else {
        return (method == "GET").then_some(TodoRoute::Get { store, todo });
    };
    if segments.next().is_some() {
        return None;
    }
    match (method, collection) {
        ("POST", "status") => Some(TodoRoute::Update { store, todo }),
        ("POST", "comments") => Some(TodoRoute::Comment { store, todo }),
        ("POST", "dispatch") => Some(TodoRoute::Dispatch { store, todo }),
        ("GET", "tree") => Some(TodoRoute::Tree { store, todo }),
        ("GET", "events") => Some(TodoRoute::Events { store, todo }),
        _ => None,
    }
}

/// The typed refusal for a store id this API may not route to. The GRANT
/// is the authority the kernel enforces; the configured list is the same
/// fact told to the provider, so an id in neither is simply not here —
/// answered without a kernel call.
#[must_use]
pub fn no_such_todo_store(store: &str) -> ApiError {
    ApiError::new(
        ErrorCode::NotFound,
        format!("this API routes to no Todo store {store:?}"),
    )
}

/// The contract name a store may be reached under, or the typed refusal
/// when this API may not route to it.
///
/// # Errors
///
/// `not-found` for a store outside the configured list.
pub fn todo_store_routable(stores: &[String], store: &str) -> Result<String, ApiError> {
    if stores.iter().any(|known| known == store) {
        Ok(store_contract(store))
    } else {
        Err(no_such_todo_store(store))
    }
}

/// The request payload for one route: the caller's document (body or
/// query) with the PATH's Todo written over it. The path supplies the
/// Todo id; a body that names another Todo is not a second opinion.
#[must_use]
pub fn todo_payload(route: &TodoRoute, document: serde_json::Value) -> serde_json::Value {
    let mut payload = match document {
        serde_json::Value::Object(fields) => serde_json::Value::Object(fields),
        _ => serde_json::json!({}),
    };
    if let Some(todo) = route.todo() {
        payload["todo-id"] = serde_json::Value::String(todo.to_owned());
    }
    // `create` takes `{ "spec": … }` and `dispatch` takes
    // `{ "dispatch": … }`. A body already shaped that way is used as it
    // is; a bare document is wrapped, so an operator can POST the thing
    // the definition documents without a wrapper.
    match route {
        TodoRoute::Create { .. } if payload.get("spec").is_none() => {
            payload = serde_json::json!({ "spec": payload });
        }
        TodoRoute::Dispatch { todo, .. } if payload.get("dispatch").is_none() => {
            let actor = payload.get("actor").cloned();
            payload = serde_json::json!({ "todo-id": todo, "dispatch": payload });
            if let Some(actor) = actor {
                payload["actor"] = actor;
            }
        }
        _ => {}
    }
    payload
}

/// The todos seam's typed error as this seam's. `unavailable` — the store
/// is mounted and correct, this host cannot carry the call — stays
/// `unavailable`, and the store's own code rides along verbatim as
/// `store-code` (additive) so `failed` is never mistaken for `refused` by
/// an operator reading the answer. A REFUSED MOVE's `from`/`to` ride
/// along too: the attempt stays data all the way out.
#[must_use]
pub fn todo_api_error(error: &TodoError) -> ApiError {
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
    for named in ["from", "to"] {
        if let Some(value) = error.extra.get(named) {
            mapped.extra.insert(named.to_owned(), value.clone());
        }
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
pub fn decode_todo_answer(bytes: &[u8]) -> Result<serde_json::Value, ApiError> {
    let answer: jinn_todo::Answer = serde_json::from_slice(bytes).map_err(|error| {
        ApiError::new(
            ErrorCode::Refused,
            format!("malformed Todo answer: {error}"),
        )
    })?;
    answer.into_result().map_err(|error| todo_api_error(&error))
}

/// One store on the operator surface: what its provider says about itself
/// (`describe`), or the typed reason it could not say.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TodoStoreEntry {
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

/// The `GET /v1/todos` answer.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TodoStoreList {
    pub api_version: String,
    pub stores: Vec<TodoStoreEntry>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// Assembles the store list from each configured store's `describe`
/// outcome: an unreachable store is a row with a typed error, never a
/// missing row and never a fault. Sorted by store id.
#[must_use]
pub fn todo_store_list<I>(described: I) -> TodoStoreList
where
    I: IntoIterator<Item = (String, Result<serde_json::Value, ApiError>)>,
{
    let mut stores: Vec<TodoStoreEntry> = described
        .into_iter()
        .map(|(store, described)| {
            let (describe, error) = match described {
                Ok(description) => (Some(description), None),
                Err(error) => (None, Some(error)),
            };
            TodoStoreEntry {
                contract: store_contract(&store),
                store,
                describe,
                error,
                extra: Extensions::new(),
            }
        })
        .collect();
    stores.sort_by(|left, right| left.store.cmp(&right.store));
    TodoStoreList {
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
        assert_eq!(todo_route("GET", "/v1/todos"), Some(TodoRoute::Stores));
        assert_eq!(
            todo_route("GET", "/v1/todos/default"),
            Some(TodoRoute::List {
                store: "default".into()
            })
        );
        assert_eq!(
            todo_route("POST", "/v1/todos/default"),
            Some(TodoRoute::Create {
                store: "default".into()
            })
        );
        assert!(matches!(
            todo_route("GET", "/v1/todos/default/default-1"),
            Some(TodoRoute::Get { .. })
        ));
        assert!(matches!(
            todo_route("POST", "/v1/todos/default/default-1/status"),
            Some(TodoRoute::Update { .. })
        ));
        assert!(matches!(
            todo_route("POST", "/v1/todos/default/default-1/comments"),
            Some(TodoRoute::Comment { .. })
        ));
        assert!(matches!(
            todo_route("POST", "/v1/todos/default/default-1/dispatch"),
            Some(TodoRoute::Dispatch { .. })
        ));
        assert!(matches!(
            todo_route("GET", "/v1/todos/default/default-1/tree"),
            Some(TodoRoute::Tree { .. })
        ));
        assert!(matches!(
            todo_route("GET", "/v1/todos/default/default-1/events"),
            Some(TodoRoute::Events { .. })
        ));
    }

    #[test]
    fn a_shape_this_surface_does_not_answer_is_a_miss_not_a_guess() {
        assert!(todo_route("DELETE", "/v1/todos").is_none());
        // A Todo is never DELETEd: the ledger's ending is `cancelled`,
        // recorded, and a removal would be the one edit this seam refuses.
        assert!(todo_route("DELETE", "/v1/todos/default/default-1").is_none());
        assert!(todo_route("GET", "/v1/todos/default/default-1/status").is_none());
        assert!(todo_route("GET", "/v1/todos/default/default-1/vibes").is_none());
        assert!(todo_route("GET", "/v1/todos/default/default-1/tree/1").is_none());
        assert!(!is_todos_path("/v1/todoslist"));
        assert!(is_todos_path("/v1/todos") && is_todos_path("/v1/todos/default"));
    }

    #[test]
    fn the_path_names_the_todo_and_a_body_does_not_get_a_second_opinion() {
        let route = TodoRoute::Update {
            store: "default".into(),
            todo: "default-1".into(),
        };
        let payload = todo_payload(
            &route,
            serde_json::json!({ "todo-id": "default-9", "status": "in-review" }),
        );
        assert_eq!(payload["todo-id"], "default-1");
        assert_eq!(payload["status"], "in-review");
    }

    #[test]
    fn a_bare_document_is_wrapped_and_a_wrapped_one_is_left_alone() {
        let create = TodoRoute::Create {
            store: "default".into(),
        };
        let bare = todo_payload(&create, serde_json::json!({ "title": "port it" }));
        assert_eq!(bare["spec"]["title"], "port it");
        let wrapped = todo_payload(
            &create,
            serde_json::json!({ "spec": { "title": "port it" } }),
        );
        assert_eq!(wrapped["spec"]["title"], "port it");
        assert!(wrapped["spec"].get("spec").is_none());

        let dispatch = TodoRoute::Dispatch {
            store: "default".into(),
            todo: "default-1".into(),
        };
        let bare = todo_payload(
            &dispatch,
            serde_json::json!({ "store": "default", "engine": { "engine": "echo" },
                                "actor": "planner" }),
        );
        assert_eq!(bare["todo-id"], "default-1");
        assert_eq!(bare["dispatch"]["store"], "default");
        assert_eq!(bare["dispatch"]["engine"]["engine"], "echo");
        // The actor belongs to the REQUEST, not to the dispatch spec.
        assert_eq!(bare["actor"], "planner");
    }

    #[test]
    fn a_store_this_api_may_not_route_to_is_refused_without_a_kernel_call() {
        let stores = vec!["default".to_owned(), "memory".to_owned()];
        assert_eq!(
            todo_store_routable(&stores, "default").expect("routable"),
            "jinn:todo.default"
        );
        assert_eq!(
            todo_store_routable(&stores, "other")
                .expect_err("not routable")
                .code,
            ErrorCode::NotFound
        );
    }

    #[test]
    fn a_refused_move_reaches_an_operator_as_data_not_only_as_prose() {
        let mut refused = TodoError::new(
            StoreErrorCode::Refused,
            "this Todo cannot move executing -> done",
        );
        refused
            .extra
            .insert("from".to_owned(), serde_json::json!("executing"));
        refused
            .extra
            .insert("to".to_owned(), serde_json::json!("done"));
        let mapped = todo_api_error(&refused);
        assert_eq!(mapped.code, ErrorCode::Refused);
        assert_eq!(mapped.extra["from"], "executing");
        assert_eq!(mapped.extra["to"], "done");
        assert_eq!(mapped.extra["store-code"], "refused");
    }

    #[test]
    fn an_unreachable_store_is_a_row_with_a_reason_never_a_missing_row() {
        let list = todo_store_list([
            (
                "memory".to_owned(),
                Ok(serde_json::json!({ "durable": false })),
            ),
            (
                "default".to_owned(),
                Err(ApiError::new(ErrorCode::Unavailable, "no provider")),
            ),
        ]);
        assert_eq!(list.stores.len(), 2);
        assert_eq!(list.stores[0].store, "default", "sorted by store id");
        assert!(list.stores[0].describe.is_none() && list.stores[0].error.is_some());
        assert_eq!(list.stores[1].contract, "jinn:todo.memory");
    }

    #[test]
    fn an_unavailable_store_stays_unavailable_and_carries_its_own_code() {
        let mapped = todo_api_error(&TodoError::new(
            StoreErrorCode::Unavailable,
            "the session store is not here",
        ));
        assert_eq!(mapped.code, ErrorCode::Unavailable);
        assert_eq!(mapped.extra["store-code"], "unavailable");
        for code in [StoreErrorCode::Failed, StoreErrorCode::Refused] {
            let mapped = todo_api_error(&TodoError::new(code, "x"));
            assert_eq!(mapped.code, ErrorCode::Refused);
        }
    }
}
