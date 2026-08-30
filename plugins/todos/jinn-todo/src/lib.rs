//! `jinn:todo` — the todos seam's service definition (phase 2.5).
//!
//! A Todo is the company's work ledger: the durable record of what was
//! asked, who owns it, and what happened. Nothing here knows where
//! records live or how work is carried out: a STORE provider adds only
//! where the records go, and the SESSION a Todo is dispatched to is
//! reached through the sessions seam's own definition
//! ([`jinn_session::store_contract`]) — never through a provider. That
//! layering is the seam's point, and it is three deep:
//!
//! ```text
//!   jinn:todo.<store>  ->  jinn:session.<store>  ->  jinn:engine.<id>
//! ```
//!
//! Each layer injects the DEFINITION below it. `jinn-todo-fs` and
//! `jinn-todo-memory` know nothing about sessions' providers, and the
//! sessions seam knows nothing about todos at all.
//!
//! # One contract per store id
//!
//! The kernel holds ONE provider slot per contract name, so N stores
//! coexisting means N contract names (`FINDINGS.md` #29, with the
//! reasoning's one home in `plugins/engines/README.md`). The seam's name
//! is therefore INSTANCED: `jinn:todo.<store-id>` ([`store_contract`]),
//! the store id carried in the provider entry's own `config.data.store`
//! and nowhere else. Switch, coexistence and extension are then all
//! profile edits.
//!
//! # The honesty law
//!
//! **A claim is derived from proof, never from the absence of a
//! contradiction.** Every status, count and actor this seam reports makes
//! the dangerous answer require positive evidence and lets everything
//! else fall to the conservative one BY CONSTRUCTION:
//!
//! - A status move is legal or it is REFUSED, from an explicit table
//!   ([`Status::allows`]) that is nowhere near "any status to any
//!   status". The refusal is typed, names the attempted `from -> to`, and
//!   is RECORDED — there is no code path in [`Todos::update`] that
//!   produces one without the other.
//! - [`DispatchStatus::Done`] — the claim that the work was carried out —
//!   exists only where a terminal record was written. A dispatch read
//!   back from a journal with no ending is [`DispatchStatus::Interrupted`]
//!   with a reason, and [`DispatchStatus::Running`] is minted only by the
//!   LIVE registry, so a replay cannot produce it at all.
//! - A Todo whose dispatch was interrupted never reads `executing`: the
//!   fold ([`reported_status`]) reports `blocked` with the reason, while
//!   `declared-status` still shows what history says. Two named fields,
//!   neither rewritten.
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
//! closed surfaces here are the value spaces ([`Status`],
//! [`DispatchStatus`], [`ErrorCode`], [`journal::Kind`]), which REFUSE
//! what they cannot name through the one shared [`jinn_settings::closed`].

mod answer;
pub mod dispatch;
mod event;
pub mod journal;
mod record;
mod spec;
mod status;
mod todos;

#[cfg(test)]
mod additivity_tests;
#[cfg(test)]
mod tests;

pub use answer::{Answer, ErrorCode, Outcome, TodoError};
pub use dispatch::session_contract;
pub use event::{Event, EventKind, EventPage, EventsRequest, TodoEvent};
pub use jinn_settings::{
    closed, decode_with_rest, encode_with_rest, optional, put, required, Additive, Extensions,
};
pub use record::{
    reported_status, Comment, Dispatch, DispatchStatus, RefusedChange, StatusChange, TodoList,
    TodoRecord, TodoSummary, Tree, TreeNode, INTERRUPTED_STATUS_REASON,
};
pub use spec::{
    Attribution, CommentRequest, CreateRequest, DispatchRequest, DispatchSpec, GetRequest,
    ListRequest, TodoCreated, TodoSpec, TreeRequest, UpdateRequest,
};
pub use status::{Refusal, Status};
pub use todos::{event_kind, Dispatching, Moved, Todos, EVENT_RING};

/// The answer envelope's version (additive within `0.x`).
pub const API_VERSION: &str = "0.1";

/// The seam's contract-name prefix. A full name is `jinn:todo.<store-id>`
/// — see [`store_contract`].
pub const TODO_CONTRACT_PREFIX: &str = "jinn:todo.";

/// The topic every store provider publishes its Todo events on. One topic
/// for the whole seam: a consumer listens once and routes on the event's
/// own `store` and `todo-id`.
pub const EVENT_TOPIC: &str = "jinn:todo/event";

/// The settings namespace this definition owns (AGENTS.md standing order
/// 4): the operator's Todo defaults.
pub const SETTINGS_NAMESPACE: &str = "todos";

/// Operation: what this store is and what it can do.
pub const OP_DESCRIBE: &str = "describe";
/// Operation: record a Todo; answers a [`TodoCreated`].
pub const OP_CREATE: &str = "create";
/// Operation: move a Todo's status; answers its [`TodoRecord`].
pub const OP_UPDATE: &str = "update";
/// Operation: add one comment.
pub const OP_COMMENT: &str = "comment";
/// Operation: one Todo's record.
pub const OP_GET: &str = "get";
/// Operation: the Todos this store holds.
pub const OP_LIST: &str = "list";
/// Operation: one Todo and everything parented beneath it.
pub const OP_TREE: &str = "tree";
/// Operation: send a Todo to a session — the three-layer composition.
pub const OP_DISPATCH: &str = "dispatch";
/// Operation: one page of a Todo's event feed.
pub const OP_EVENTS: &str = "events";

/// The contract name store `id` is served under.
#[must_use]
pub fn store_contract(id: &str) -> String {
    format!("{TODO_CONTRACT_PREFIX}{id}")
}

/// The store id a contract name carries, or `None` when it is not this
/// seam's. An empty id is not a store.
#[must_use]
pub fn store_id_of(contract: &str) -> Option<&str> {
    contract
        .strip_prefix(TODO_CONTRACT_PREFIX)
        .filter(|id| !id.is_empty())
}

/// One store as the KERNEL's own view shows it: an entry that provides a
/// `jinn:todo.<id>` contract. See [`stores_in`].
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

/// Every Todo store live in a composition, from `(entry-id, provisions)`
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
