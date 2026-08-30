//! The workflows seam ON the operator surface: the routes, the store
//! list's schema, and the pure mapping of the workflows seam's typed
//! error onto this seam's. The workflows CONTRACT itself is not restated
//! here — its one home is `plugins/workflows/jinn-workflow/README.md`;
//! this module only says how an operator reaches it over a transport.
//!
//! Two parameters, as the todos surface has: a STORE and, within it,
//! either a workflow (the reusable procedure) or a run (one execution of
//! one pinned revision of it). The store is in the path because a
//! composition holds several at once (one contract name per store id), so
//! an operator addresses the store they mean rather than the API guessing
//! a default.
//!
//! # `runs` is a RESERVED path segment
//!
//! A workflow is addressed at `/v1/workflows/{store}/{workflow}` and a
//! run at `/v1/workflows/{store}/runs/{run}`, so the two shapes would
//! collide for a workflow whose id were the literal `runs`. The rule that
//! resolves it is stated once, here, and enforced in [`workflow_route`]:
//! a second segment of exactly `runs` is ALWAYS the run collection, never
//! a workflow id. Nothing is lost by it, because a workflow id is minted
//! by its store as `<store>-w<n>` and can therefore never be `runs`; and
//! a rule that reads the same way for every store is worth more to an
//! operator than a name no store hands out. `Define` needs no check of
//! its own for the same reason: an operator does not choose the id.
//!
//! # A refused node move keeps its shape all the way out
//!
//! An illegal node-state transition comes back from a store as `refused`
//! with `node`, `from` and `to` beside the message.
//! [`workflow_api_error`] carries all three through verbatim, so an
//! operator reading a 4xx sees WHICH node and WHICH move as DATA and
//! never has to parse prose to learn either.

use jinn_workflow::{store_contract, ErrorCode as StoreErrorCode, WorkflowError};
use serde::{Deserialize, Serialize};

use crate::{ApiError, ErrorCode, Extensions, API_VERSION};

/// The workflows surface's path prefix.
pub const WORKFLOWS_PATH: &str = "/v1/workflows";

/// The path segment reserved for the run collection (see the module doc).
pub const RUNS_SEGMENT: &str = "runs";

/// The methods the workflows surface answers. A path this table shapes
/// under another method is a method refusal, not a route miss.
pub const WORKFLOW_METHODS: [&str; 2] = ["GET", "POST"];

/// One workflows route: which operation the path names, in which store.
/// `Stores` is the only one that is not a call on a provider.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkflowRoute {
    /// `GET /v1/workflows` — every store this API may route to.
    Stores,
    /// `GET /v1/workflows/{store}`
    List { store: String },
    /// `POST /v1/workflows/{store}`
    Define { store: String },
    /// `GET /v1/workflows/{store}/{workflow}`
    Get { store: String, workflow: String },
    /// `POST /v1/workflows/{store}/{workflow}/runs`
    Start { store: String, workflow: String },
    /// `GET /v1/workflows/{store}/runs`
    Runs { store: String },
    /// `GET /v1/workflows/{store}/runs/{run}`
    GetRun { store: String, run: String },
    /// `POST /v1/workflows/{store}/runs/{run}/cancel`
    Cancel { store: String, run: String },
    /// `POST /v1/workflows/{store}/runs/{run}/nodes/{node}/state`
    NodeState {
        store: String,
        run: String,
        node: String,
    },
    /// `GET /v1/workflows/{store}/runs/{run}/events`
    Events { store: String, run: String },
}

impl WorkflowRoute {
    /// The store the route addresses, if it addresses one.
    #[must_use]
    pub fn store(&self) -> Option<&str> {
        match self {
            Self::Stores => None,
            Self::List { store }
            | Self::Define { store }
            | Self::Get { store, .. }
            | Self::Start { store, .. }
            | Self::Runs { store }
            | Self::GetRun { store, .. }
            | Self::Cancel { store, .. }
            | Self::NodeState { store, .. }
            | Self::Events { store, .. } => Some(store),
        }
    }

    /// The workflow the route addresses, if it addresses one. A run route
    /// names no workflow: which workflow — and which REVISION of it — a
    /// run executes is a fact of the run's own record, pinned at `start`,
    /// and the path never gets to state it a second time.
    #[must_use]
    pub fn workflow(&self) -> Option<&str> {
        match self {
            Self::Get { workflow, .. } | Self::Start { workflow, .. } => Some(workflow),
            _ => None,
        }
    }

    /// The run the route addresses, if it addresses one.
    #[must_use]
    pub fn run(&self) -> Option<&str> {
        match self {
            Self::GetRun { run, .. }
            | Self::Cancel { run, .. }
            | Self::NodeState { run, .. }
            | Self::Events { run, .. } => Some(run),
            _ => None,
        }
    }

    /// The node the route addresses, if it addresses one. Only the
    /// node-state move does.
    #[must_use]
    pub fn node(&self) -> Option<&str> {
        match self {
            Self::NodeState { node, .. } => Some(node),
            _ => None,
        }
    }

    /// The workflows-seam operation the route calls, if it calls one.
    #[must_use]
    pub fn operation(&self) -> Option<&'static str> {
        match self {
            Self::Stores => None,
            Self::List { .. } => jinn_workflow::OP_LIST.into(),
            Self::Define { .. } => jinn_workflow::OP_DEFINE.into(),
            Self::Get { .. } => jinn_workflow::OP_GET.into(),
            Self::Start { .. } => jinn_workflow::OP_START.into(),
            Self::Runs { .. } => jinn_workflow::OP_LIST_RUNS.into(),
            Self::GetRun { .. } => jinn_workflow::OP_GET_RUN.into(),
            Self::Cancel { .. } => jinn_workflow::OP_CANCEL.into(),
            Self::NodeState { .. } => jinn_workflow::OP_NODE_STATE.into(),
            Self::Events { .. } => jinn_workflow::OP_EVENTS.into(),
        }
    }

    /// Whether the route's payload comes from the request BODY. Every
    /// other route's payload is its query plus its path parameters — a
    /// read never takes a body, and a write never takes a query.
    #[must_use]
    pub fn takes_body(&self) -> bool {
        matches!(
            self,
            Self::Define { .. } | Self::Start { .. } | Self::Cancel { .. } | Self::NodeState { .. }
        )
    }
}

/// Whether a path belongs to the workflows surface at all — the provider
/// asks before it consults the static route table, so a workflows path is
/// never answered by another route.
#[must_use]
pub fn is_workflows_path(path: &str) -> bool {
    path.strip_prefix(WORKFLOWS_PATH)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// Matches a method + path (query already stripped) against the workflows
/// surface. `None` is a miss — a malformed path, an unknown shape, or a
/// method this shape does not answer — and the caller answers it typed,
/// never by guessing a neighbouring route.
#[must_use]
pub fn workflow_route(method: &str, path: &str) -> Option<WorkflowRoute> {
    let rest = path.strip_prefix(WORKFLOWS_PATH)?;
    if rest.is_empty() {
        return (method == "GET").then_some(WorkflowRoute::Stores);
    }
    let mut segments = rest.strip_prefix('/')?.split('/');
    let store = segments.next().filter(|segment| !segment.is_empty())?;
    let store = store.to_owned();
    let Some(second) = segments.next().filter(|segment| !segment.is_empty()) else {
        return match method {
            "GET" => Some(WorkflowRoute::List { store }),
            "POST" => Some(WorkflowRoute::Define { store }),
            _ => None,
        };
    };
    // The reservation, in one place: a second segment of exactly `runs`
    // is the run collection and cannot be read as a workflow id.
    if second == RUNS_SEGMENT {
        return run_route(method, store, &mut segments);
    }
    let workflow = second.to_owned();
    let Some(collection) = segments.next() else {
        return (method == "GET").then_some(WorkflowRoute::Get { store, workflow });
    };
    if segments.next().is_some() {
        return None;
    }
    match (method, collection) {
        ("POST", RUNS_SEGMENT) => Some(WorkflowRoute::Start { store, workflow }),
        _ => None,
    }
}

/// The run half of the table, entered once the reserved `runs` segment
/// has been read: the collection itself, one run, and the three things
/// that are done TO a run.
fn run_route<'a>(
    method: &str,
    store: String,
    segments: &mut impl Iterator<Item = &'a str>,
) -> Option<WorkflowRoute> {
    let Some(run) = segments.next().filter(|segment| !segment.is_empty()) else {
        return (method == "GET").then_some(WorkflowRoute::Runs { store });
    };
    let run = run.to_owned();
    let Some(collection) = segments.next() else {
        return (method == "GET").then_some(WorkflowRoute::GetRun { store, run });
    };
    // A node's state is the one route with a second parameter under the
    // run, so it is shaped before the flat collections below.
    if collection == "nodes" {
        let node = segments.next().filter(|segment| !segment.is_empty())?;
        let node = node.to_owned();
        if segments.next() != Some("state") || segments.next().is_some() {
            return None;
        }
        return (method == "POST").then_some(WorkflowRoute::NodeState { store, run, node });
    }
    if segments.next().is_some() {
        return None;
    }
    match (method, collection) {
        ("POST", "cancel") => Some(WorkflowRoute::Cancel { store, run }),
        ("GET", "events") => Some(WorkflowRoute::Events { store, run }),
        _ => None,
    }
}

/// The typed refusal for a store id this API may not route to. The GRANT
/// is the authority the kernel enforces; the configured list is the same
/// fact told to the provider, so an id in neither is simply not here —
/// answered without a kernel call.
#[must_use]
pub fn no_such_workflow_store(store: &str) -> ApiError {
    ApiError::new(
        ErrorCode::NotFound,
        format!("this API routes to no workflow store {store:?}"),
    )
}

/// The contract name a store may be reached under, or the typed refusal
/// when this API may not route to it.
///
/// # Errors
///
/// `not-found` for a store outside the configured list.
pub fn workflow_store_routable(stores: &[String], store: &str) -> Result<String, ApiError> {
    if stores.iter().any(|known| known == store) {
        Ok(store_contract(store))
    } else {
        Err(no_such_workflow_store(store))
    }
}

/// The request payload for one route: the caller's document (body or
/// query) with the PATH's ids written over it. The path supplies the
/// workflow, the run and the node; a body that names another one of any
/// of them is not a second opinion.
#[must_use]
pub fn workflow_payload(route: &WorkflowRoute, document: serde_json::Value) -> serde_json::Value {
    let mut payload = match document {
        serde_json::Value::Object(fields) => serde_json::Value::Object(fields),
        _ => serde_json::json!({}),
    };
    if let Some(workflow) = route.workflow() {
        payload["workflow-id"] = serde_json::Value::String(workflow.to_owned());
    }
    if let Some(run) = route.run() {
        payload["run-id"] = serde_json::Value::String(run.to_owned());
    }
    if let Some(node) = route.node() {
        payload["node-id"] = serde_json::Value::String(node.to_owned());
    }
    // `define` takes `{ "spec": … }` with an optional `workflow-id` beside
    // it naming the workflow this is a new REVISION of. A body already
    // shaped that way is used as it is; a bare spec is wrapped, so an
    // operator can POST the thing the definition documents without a
    // wrapper — and the `workflow-id` is lifted back out, because it
    // belongs to the request rather than to the spec.
    //
    // `start` is deliberately NOT wrapped the same way: its document
    // carries `revision`, `input` and `actor` side by side, so wrapping a
    // bare body would put the other two out of reach.
    if matches!(route, WorkflowRoute::Define { .. }) && payload.get("spec").is_none() {
        let workflow_id = payload.get("workflow-id").cloned();
        payload = serde_json::json!({ "spec": payload });
        if let Some(workflow_id) = workflow_id {
            payload["workflow-id"] = workflow_id;
        }
    }
    payload
}

/// The workflows seam's typed error as this seam's. `unavailable` — the
/// store is mounted and correct, this host cannot carry the call — stays
/// `unavailable`, and the store's own code rides along verbatim as
/// `store-code` (additive) so `failed` is never mistaken for `refused` by
/// an operator reading the answer. A REFUSED NODE MOVE's `node`, `from`
/// and `to` ride along too: the attempt stays data all the way out.
#[must_use]
pub fn workflow_api_error(error: &WorkflowError) -> ApiError {
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
    for named in ["node", "from", "to"] {
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
pub fn decode_workflow_answer(bytes: &[u8]) -> Result<serde_json::Value, ApiError> {
    let answer: jinn_workflow::Answer = serde_json::from_slice(bytes).map_err(|error| {
        ApiError::new(
            ErrorCode::Refused,
            format!("malformed workflow answer: {error}"),
        )
    })?;
    answer
        .into_result()
        .map_err(|error| workflow_api_error(&error))
}

/// One store on the operator surface: what its provider says about itself
/// (`describe`), or the typed reason it could not say.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct WorkflowStoreEntry {
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

/// The `GET /v1/workflows` answer.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct WorkflowStoreList {
    pub api_version: String,
    pub stores: Vec<WorkflowStoreEntry>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// Assembles the store list from each configured store's `describe`
/// outcome: an unreachable store is a row with a typed error, never a
/// missing row and never a fault. Sorted by store id.
#[must_use]
pub fn workflow_store_list<I>(described: I) -> WorkflowStoreList
where
    I: IntoIterator<Item = (String, Result<serde_json::Value, ApiError>)>,
{
    let mut stores: Vec<WorkflowStoreEntry> = described
        .into_iter()
        .map(|(store, described)| {
            let (describe, error) = match described {
                Ok(description) => (Some(description), None),
                Err(error) => (None, Some(error)),
            };
            WorkflowStoreEntry {
                contract: store_contract(&store),
                store,
                describe,
                error,
                extra: Extensions::new(),
            }
        })
        .collect();
    stores.sort_by(|left, right| left.store.cmp(&right.store));
    WorkflowStoreList {
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
            workflow_route("GET", "/v1/workflows"),
            Some(WorkflowRoute::Stores)
        );
        assert_eq!(
            workflow_route("GET", "/v1/workflows/default"),
            Some(WorkflowRoute::List {
                store: "default".into()
            })
        );
        assert_eq!(
            workflow_route("POST", "/v1/workflows/default"),
            Some(WorkflowRoute::Define {
                store: "default".into()
            })
        );
        assert_eq!(
            workflow_route("GET", "/v1/workflows/default/default-w1"),
            Some(WorkflowRoute::Get {
                store: "default".into(),
                workflow: "default-w1".into()
            })
        );
        assert_eq!(
            workflow_route("POST", "/v1/workflows/default/default-w1/runs"),
            Some(WorkflowRoute::Start {
                store: "default".into(),
                workflow: "default-w1".into()
            })
        );
        assert_eq!(
            workflow_route("GET", "/v1/workflows/default/runs"),
            Some(WorkflowRoute::Runs {
                store: "default".into()
            })
        );
        assert_eq!(
            workflow_route("GET", "/v1/workflows/default/runs/default-r1"),
            Some(WorkflowRoute::GetRun {
                store: "default".into(),
                run: "default-r1".into()
            })
        );
        assert_eq!(
            workflow_route("POST", "/v1/workflows/default/runs/default-r1/cancel"),
            Some(WorkflowRoute::Cancel {
                store: "default".into(),
                run: "default-r1".into()
            })
        );
        assert_eq!(
            workflow_route(
                "POST",
                "/v1/workflows/default/runs/default-r1/nodes/review/state"
            ),
            Some(WorkflowRoute::NodeState {
                store: "default".into(),
                run: "default-r1".into(),
                node: "review".into()
            })
        );
        assert_eq!(
            workflow_route("GET", "/v1/workflows/default/runs/default-r1/events"),
            Some(WorkflowRoute::Events {
                store: "default".into(),
                run: "default-r1".into()
            })
        );
    }

    #[test]
    fn every_route_names_the_operation_and_the_shape_it_reads() {
        let routes = [
            workflow_route("GET", "/v1/workflows").expect("stores"),
            workflow_route("GET", "/v1/workflows/default").expect("list"),
            workflow_route("POST", "/v1/workflows/default").expect("define"),
            workflow_route("GET", "/v1/workflows/default/default-w1").expect("get"),
            workflow_route("POST", "/v1/workflows/default/default-w1/runs").expect("start"),
            workflow_route("GET", "/v1/workflows/default/runs").expect("runs"),
            workflow_route("GET", "/v1/workflows/default/runs/default-r1").expect("get-run"),
            workflow_route("POST", "/v1/workflows/default/runs/default-r1/cancel").expect("cancel"),
            workflow_route(
                "POST",
                "/v1/workflows/default/runs/default-r1/nodes/review/state",
            )
            .expect("node-state"),
            workflow_route("GET", "/v1/workflows/default/runs/default-r1/events").expect("events"),
        ];
        let operations: Vec<Option<&str>> = routes.iter().map(WorkflowRoute::operation).collect();
        assert_eq!(
            operations,
            [
                None,
                Some(jinn_workflow::OP_LIST),
                Some(jinn_workflow::OP_DEFINE),
                Some(jinn_workflow::OP_GET),
                Some(jinn_workflow::OP_START),
                Some(jinn_workflow::OP_LIST_RUNS),
                Some(jinn_workflow::OP_GET_RUN),
                Some(jinn_workflow::OP_CANCEL),
                Some(jinn_workflow::OP_NODE_STATE),
                Some(jinn_workflow::OP_EVENTS),
            ]
        );
        // Every route but the store list names its store, and only the
        // four writes read a body.
        assert!(routes[1..].iter().all(|route| route.store().is_some()));
        assert!(routes[0].store().is_none());
        let bodies: Vec<bool> = routes.iter().map(WorkflowRoute::takes_body).collect();
        assert_eq!(
            bodies,
            [false, false, true, false, true, false, false, true, true, false]
        );
    }

    #[test]
    fn the_runs_segment_is_reserved_and_never_reads_as_a_workflow_id() {
        // The collection, not a workflow whose id happens to be `runs`.
        assert_eq!(
            workflow_route("GET", "/v1/workflows/default/runs"),
            Some(WorkflowRoute::Runs {
                store: "default".into()
            })
        );
        // And the segment after it is a RUN, not a workflow's `runs`
        // collection, so a `start` can never be reached this way.
        assert_eq!(
            workflow_route("POST", "/v1/workflows/default/runs/runs"),
            None,
            "a POST under the reserved segment names a run, and a run is not started by POSTing \
             to itself"
        );
        assert_eq!(
            workflow_route("GET", "/v1/workflows/default/runs/runs"),
            Some(WorkflowRoute::GetRun {
                store: "default".into(),
                run: "runs".into()
            })
        );
        // A store may be called `runs` without any of this changing:
        // only the SECOND segment is reserved.
        assert_eq!(
            workflow_route("GET", "/v1/workflows/runs/runs"),
            Some(WorkflowRoute::Runs {
                store: "runs".into()
            })
        );
    }

    #[test]
    fn a_shape_this_surface_does_not_answer_is_a_miss_not_a_guess() {
        assert!(workflow_route("DELETE", "/v1/workflows").is_none());
        // A run is never DELETEd: its ending is `cancelled`, recorded,
        // and a removal would be the one edit this seam refuses.
        assert!(workflow_route("DELETE", "/v1/workflows/default/runs/default-r1").is_none());
        assert!(workflow_route("POST", "/v1/workflows/default/default-w1").is_none());
        assert!(workflow_route("GET", "/v1/workflows/default/runs/default-r1/cancel").is_none());
        assert!(workflow_route("POST", "/v1/workflows/default/runs/default-r1/events").is_none());
        assert!(workflow_route("GET", "/v1/workflows/default/default-w1/runs").is_none());
        assert!(workflow_route("GET", "/v1/workflows/default/default-w1/vibes").is_none());
        assert!(workflow_route("GET", "/v1/workflows/default/default-w1/runs/1").is_none());
        assert!(workflow_route("POST", "/v1/workflows/default/runs/r1/nodes/review").is_none());
        assert!(workflow_route("POST", "/v1/workflows/default/runs/r1/nodes//state").is_none());
        assert!(
            workflow_route("POST", "/v1/workflows/default/runs/r1/nodes/review/state/1").is_none()
        );
        assert!(!is_workflows_path("/v1/workflowslist"));
        assert!(is_workflows_path("/v1/workflows") && is_workflows_path("/v1/workflows/default"));
    }

    #[test]
    fn the_path_names_the_ids_and_a_body_does_not_get_a_second_opinion() {
        let route = WorkflowRoute::NodeState {
            store: "default".into(),
            run: "default-r1".into(),
            node: "review".into(),
        };
        let payload = workflow_payload(
            &route,
            serde_json::json!({ "run-id": "default-r9", "node-id": "build", "to": "done",
                                "actor": "planner" }),
        );
        assert_eq!(payload["run-id"], "default-r1");
        assert_eq!(payload["node-id"], "review");
        assert_eq!(payload["to"], "done");
        assert_eq!(payload["actor"], "planner");

        let start = WorkflowRoute::Start {
            store: "default".into(),
            workflow: "default-w1".into(),
        };
        // `start` is not wrapped: its three top-level fields stay where
        // the definition documents them.
        let payload = workflow_payload(
            &start,
            serde_json::json!({ "workflow-id": "default-w9", "revision": 2,
                                "input": { "ticket": "PLA-323" }, "actor": "planner" }),
        );
        assert_eq!(payload["workflow-id"], "default-w1");
        assert_eq!(payload["revision"], 2);
        assert_eq!(payload["input"]["ticket"], "PLA-323");
        assert_eq!(payload["actor"], "planner");
    }

    #[test]
    fn a_bare_definition_is_wrapped_and_a_wrapped_one_is_left_alone() {
        let define = WorkflowRoute::Define {
            store: "default".into(),
        };
        let bare = workflow_payload(&define, serde_json::json!({ "name": "port it" }));
        assert_eq!(bare["spec"]["name"], "port it");
        let wrapped = workflow_payload(
            &define,
            serde_json::json!({ "spec": { "name": "port it" } }),
        );
        assert_eq!(wrapped["spec"]["name"], "port it");
        assert!(wrapped["spec"].get("spec").is_none());
        // A new REVISION of an existing workflow: the id belongs to the
        // request, so it is lifted back out of the wrapper.
        let revision = workflow_payload(
            &define,
            serde_json::json!({ "workflow-id": "default-w1", "name": "port it" }),
        );
        assert_eq!(revision["workflow-id"], "default-w1");
        assert_eq!(revision["spec"]["name"], "port it");
    }

    #[test]
    fn a_read_route_carries_its_query_through_untouched() {
        let events = WorkflowRoute::Events {
            store: "default".into(),
            run: "default-r1".into(),
        };
        let payload = workflow_payload(&events, serde_json::json!({ "after": 12, "limit": 5 }));
        assert_eq!(payload["run-id"], "default-r1");
        assert_eq!(payload["after"], 12);
        assert_eq!(payload["limit"], 5);

        let runs = WorkflowRoute::Runs {
            store: "default".into(),
        };
        let payload = workflow_payload(
            &runs,
            serde_json::json!({ "workflow-id": "default-w1", "status": "running" }),
        );
        assert_eq!(payload["workflow-id"], "default-w1");
        assert_eq!(payload["status"], "running");
    }

    #[test]
    fn a_store_this_api_may_not_route_to_is_refused_without_a_kernel_call() {
        let stores = vec!["default".to_owned(), "memory".to_owned()];
        assert_eq!(
            workflow_store_routable(&stores, "default").expect("routable"),
            "jinn:workflow.default"
        );
        let refused = workflow_store_routable(&stores, "other").expect_err("not routable");
        assert_eq!(refused.code, ErrorCode::NotFound);
        assert!(refused.detail.contains("other"), "{}", refused.detail);
    }

    #[test]
    fn a_refused_node_move_reaches_an_operator_as_data_not_only_as_prose() {
        let refused = WorkflowError::refused_transition(
            "review",
            jinn_workflow::Refusal {
                from: jinn_workflow::NodeState::Pending,
                to: jinn_workflow::NodeState::Done,
            },
        );
        let mapped = workflow_api_error(&refused);
        assert_eq!(mapped.code, ErrorCode::Refused);
        assert_eq!(mapped.extra["node"], "review");
        assert_eq!(mapped.extra["from"], "pending");
        assert_eq!(mapped.extra["to"], "done");
        assert_eq!(mapped.extra["store-code"], "refused");
    }

    #[test]
    fn an_unreachable_store_is_a_row_with_a_reason_never_a_missing_row() {
        let list = workflow_store_list([
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
        assert_eq!(list.stores[1].contract, "jinn:workflow.memory");
    }

    #[test]
    fn an_unavailable_store_stays_unavailable_and_carries_its_own_code() {
        let mapped = workflow_api_error(&WorkflowError::new(
            StoreErrorCode::Unavailable,
            "the Todo store this run dispatches through is not here",
        ));
        assert_eq!(mapped.code, ErrorCode::Unavailable);
        assert_eq!(mapped.extra["store-code"], "unavailable");
        for code in [StoreErrorCode::Failed, StoreErrorCode::Refused] {
            let mapped = workflow_api_error(&WorkflowError::new(code, "x"));
            assert_eq!(mapped.code, ErrorCode::Refused);
        }
    }

    #[test]
    fn a_malformed_answer_is_refused_rather_than_read_as_an_outcome() {
        let error = decode_workflow_answer(b"{").expect_err("not an answer");
        assert_eq!(error.code, ErrorCode::Refused);
        let ok = jinn_workflow::Answer::ok(serde_json::json!({ "run-id": "default-r1" }));
        assert_eq!(
            decode_workflow_answer(&ok.encode()).expect("decodes")["run-id"],
            "default-r1"
        );
    }
}
