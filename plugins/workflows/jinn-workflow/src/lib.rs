//! `jinn:workflow` — the workflows seam's service definition (phase 2.6).
//!
//! A Workflow is the company's reusable HOW: the procedure that outlives
//! any single run of it. Nothing here knows where runs live or how a
//! node's work is carried out: a RUN STORE provider adds only where the
//! records go, and the work a node dispatches is reached through the
//! TODOS seam's own definition ([`dispatch::todo_contract`]) — never
//! through a provider. That layering is the seam's point, and it is now
//! FOUR deep:
//!
//! ```text
//!   jinn:workflow.<store> -> jinn:todo.<store> -> jinn:session.<store> -> jinn:engine.<id>
//! ```
//!
//! Each layer injects the DEFINITION below it. `jinn-workflow-fs` and
//! `jinn-workflow-memory` know nothing about todos' providers, nothing
//! about sessions, and nothing about engines; the todos seam knows
//! nothing about workflows at all.
//!
//! # One contract per store id
//!
//! The kernel holds ONE provider slot per contract name, so N stores
//! coexisting means N contract names (`FINDINGS.md` #29, with the
//! reasoning's one home in `plugins/engines/README.md`). The seam's name
//! is therefore INSTANCED: `jinn:workflow.<store-id>` ([`store_contract`]),
//! the store id carried in the provider entry's own `config.data.store`
//! and nowhere else. Switch, coexistence and extension are then all
//! profile edits.
//!
//! # The pin
//!
//! **A run executes ONE revision of one definition, for its whole life,
//! and reports which.** `crate::revision` is that law's home and its
//! reasoning; [`RunRecord::definition_revision`] and [`RunRecord::spec`]
//! are where a reader sees it. A definition edited while a run is in
//! flight cannot reach that run, because nothing in this seam reads a
//! workflow's current revision on behalf of a live one.
//!
//! # The honesty law
//!
//! **A claim is derived from proof, never from the absence of a
//! contradiction.** Every state, outcome, count and actor this seam
//! reports makes the dangerous answer require positive evidence and lets
//! everything else fall to the conservative one BY CONSTRUCTION:
//!
//! - A node-state move is legal or it is REFUSED, from an explicit table
//!   ([`NodeState::allows`]) that is nowhere near "any state to any
//!   state". The refusal is typed, names the attempted `from -> to` AND
//!   the node, and is RECORDED — there is no code path in
//!   [`Workflows::plan_node_move`] that produces one without the other.
//! - [`NodeState::Done`] and [`RunStatus::Done`] — the claims that work
//!   was carried out — exist only where a terminal record was written.
//!   [`NodeState::Running`] and [`RunStatus::Running`] are minted only by
//!   the live registry.
//! - A node or run left `running` by a crash is RECORDED interrupted with
//!   a reason before its store serves anything ([`Workflows::plan_recovery`],
//!   and the ordering law in [`journal`]). A store whose recovery append
//!   is refused fails to activate rather than reporting a `running` no
//!   durable line justifies.
//! - A run's ending is derived from its nodes ([`run_ending`]): `done`
//!   requires that every node that RAN reached `done`; a skipped node is
//!   the graph working, and an interrupted one is not.
//! - An ACTOR is declared or absent. A blank is refused rather than
//!   recorded, and absence is never filled in with a default principal.
//! - A journal line that does not decode is never a record. The reader
//!   admits a torn TAIL as absence and REFUSES a hole anywhere else.
//!
//! # Additivity
//!
//! The distribution's wire law, whose one home is `jinn_settings::wire`:
//! every wire type carries a rest map (`extra`) and a decode -> encode
//! round trip is lossless for content this version cannot read. The
//! closed surfaces here are the value spaces ([`NodeState`],
//! [`RunStatus`], [`NodeKind`], [`EdgeKind`], [`FieldKind`],
//! [`ErrorCode`], [`journal::Kind`]), which REFUSE what they cannot name
//! through the one shared [`jinn_settings::closed`].

mod answer;
pub mod dispatch;
mod event;
pub mod journal;
mod node;
mod record;
mod revision;
mod spec;
mod workflows;

#[cfg(test)]
mod additivity_tests;
#[cfg(test)]
mod tests;

pub use answer::{Answer, ErrorCode, Outcome, WorkflowError};
pub use dispatch::todo_contract;
pub use event::{Event, EventKind, EventsPage, RunEvent};
pub use jinn_settings::{
    closed, decode_with_rest, encode_with_rest, optional, put, required, Additive, Extensions,
};
pub use node::{NodeState, Refusal};
pub use record::{
    run_ending, NodeChange, NodeRun, RefusedChange, RunList, RunRecord, RunStatus, RunSummary,
    WorkflowList, WorkflowSummary, INTERRUPTED_NODE_REASON, INTERRUPTED_RUN_REASON,
    LOST_TODO_REASON,
};
pub use revision::{digest, Definition, WorkflowRecord};
pub use spec::{
    Attribution, CancelRequest, DefineRequest, EdgeKind, EdgeSpec, EventsRequest, FieldKind,
    FieldSpec, InputSchema, ListRunsRequest, NodeKind, NodeSpec, RunRequest, StartRequest,
    TodoBinding, WorkflowDefined, WorkflowRequest, WorkflowSpec,
};
pub use workflows::{event_kind, Moved, Recovery, Started, Workflows, EVENT_RING};

/// The answer envelope's version (additive within `0.x`).
pub const API_VERSION: &str = "0.1";

/// The seam's contract-name prefix. A full name is
/// `jinn:workflow.<store-id>` — see [`store_contract`].
pub const WORKFLOW_CONTRACT_PREFIX: &str = "jinn:workflow.";

/// The topic every run store publishes its run events on. One topic for
/// the whole seam: a consumer listens once and routes on the event's own
/// `store` and `run-id`.
pub const EVENT_TOPIC: &str = "jinn:workflow/event";

/// The settings namespace this definition owns (AGENTS.md standing order
/// 4): the operator's workflow defaults.
pub const SETTINGS_NAMESPACE: &str = "workflows";

/// Operation: what this store is and what it can do.
pub const OP_DESCRIBE: &str = "describe";
/// Operation: record a workflow, or a new REVISION of one; answers a
/// [`WorkflowDefined`].
pub const OP_DEFINE: &str = "define";
/// Operation: one workflow's revisions.
pub const OP_GET: &str = "get";
/// Operation: the workflows this store holds.
pub const OP_LIST: &str = "list";
/// Operation: open a run, PINNED to one revision.
pub const OP_START: &str = "start";
/// Operation: one run's record.
pub const OP_GET_RUN: &str = "get-run";
/// Operation: the runs this store holds.
pub const OP_LIST_RUNS: &str = "list-runs";
/// Operation: move one node's state through the table — the operator's
/// lane, and where an illegal move is refused and recorded.
pub const OP_NODE_STATE: &str = "node-state";
/// Operation: end a run, on the record.
pub const OP_CANCEL: &str = "cancel";
/// Operation: one page of a run's event feed.
pub const OP_EVENTS: &str = "events";

/// The contract name store `id` is served under.
#[must_use]
pub fn store_contract(id: &str) -> String {
    format!("{WORKFLOW_CONTRACT_PREFIX}{id}")
}

/// The store id a contract name carries, or `None` when it is not this
/// seam's. An empty id is not a store.
#[must_use]
pub fn store_id_of(contract: &str) -> Option<&str> {
    contract
        .strip_prefix(WORKFLOW_CONTRACT_PREFIX)
        .filter(|id| !id.is_empty())
}

/// One store as the KERNEL's own view shows it: an entry that provides a
/// `jinn:workflow.<id>` contract. See [`stores_in`].
#[derive(Clone, Debug, Default, serde::Deserialize, PartialEq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct StoreSlot {
    pub store: String,
    pub contract: String,
    /// The profile entry serving it — what an operator edits to swap the
    /// implementation.
    pub entry: String,
    /// The rest map (the module doc's additivity law).
    #[serde(flatten)]
    pub extra: Extensions,
}

jinn_settings::additive!(StoreSlot);

/// Every run store live in a composition, from `(entry-id, provisions)`
/// pairs as `jinn:introspect` reports them — the kernel's knowledge, not
/// a table a consumer keeps. Sorted by store id.
#[must_use]
pub fn stores_in<'a, I, P>(entries: I) -> Vec<StoreSlot>
where
    I: IntoIterator<Item = (&'a str, P)>,
    P: IntoIterator<Item = &'a str>,
{
    let mut slots: Vec<StoreSlot> = entries
        .into_iter()
        .flat_map(|(entry, provisions)| {
            provisions
                .into_iter()
                .filter_map(move |contract| {
                    store_id_of(contract).map(|store| StoreSlot {
                        store: store.to_owned(),
                        contract: contract.to_owned(),
                        entry: entry.to_owned(),
                        extra: Extensions::new(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();
    slots.sort_by(|left, right| left.store.cmp(&right.store));
    slots
}
