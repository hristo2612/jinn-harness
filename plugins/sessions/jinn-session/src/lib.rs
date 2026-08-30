//! `jinn:session` — the sessions seam's service definition (phase 2.4).
//!
//! A session is a durable, resumable conversation bound to an engine.
//! Nothing here knows where records live or how an engine is spawned: a
//! STORE provider adds only where the records go, and the ENGINE it drives
//! is reached through the engines seam's own definition
//! ([`jinn_engine::engine_contract`]) — never through a provider. That
//! layering is the seam's point: session over engine, each swappable
//! without the other knowing.
//!
//! # One contract per store id
//!
//! The kernel holds ONE provider slot per contract name, so N stores
//! coexisting means N contract names (`FINDINGS.md` #29, and the engines
//! seam's own reasoning in `plugins/engines/README.md` — one home). The
//! seam's name is therefore INSTANCED: `jinn:session.<store-id>`
//! ([`store_contract`]), the store id carried in the provider entry's own
//! `config.data.store` and nowhere else. Switch, coexistence and
//! extension are then all profile edits.
//!
//! # The honesty law
//!
//! **A claim is derived from proof, never from the absence of a
//! contradiction.** Every status, reason and count this seam reports makes
//! the dangerous answer require positive evidence and lets everything else
//! fall to the conservative one BY CONSTRUCTION:
//!
//! - [`TurnStatus::Done`] exists only where a terminal record was written.
//!   A turn read back from a journal with no terminal record is
//!   [`TurnStatus::Interrupted`] with a reason — never `Running`, whatever
//!   the last thing written said.
//! - [`TurnStatus::Running`] is minted only by the LIVE registry
//!   ([`Sessions`]) for a turn this incarnation started. A replay cannot
//!   produce it ([`journal::replay`]), so a daemon that died mid-turn
//!   cannot come back claiming to be working.
//! - A journal line that does not decode is never a record. The reader
//!   admits a torn TAIL (the last line, unterminated) as absence, and
//!   REFUSES a hole anywhere else — a gap in the middle is corruption, not
//!   a tear, and the two must not be answered the same way.
//!
//! # Additivity
//!
//! The distribution's wire law, whose one home is `jinn_settings::wire`
//! and whose prose is the seam READMEs: every wire type carries a rest map
//! (`extra`) and a decode → encode round trip is lossless for content this
//! version cannot read. The closed surfaces here are the value spaces
//! ([`SessionStatus`], [`TurnStatus`], [`ErrorCode`], [`journal::Kind`]),
//! which REFUSE what they cannot name through the one shared
//! [`jinn_settings::closed`].

mod answer;
pub mod drive;
mod event;
pub mod journal;
mod record;
mod sessions;
mod spec;

#[cfg(test)]
mod additivity_tests;
#[cfg(test)]
mod tests;

pub use answer::{Answer, ErrorCode, Outcome, SessionError};
pub use event::{Event, EventKind, EventPage, EventsRequest, SessionEvent};
pub use jinn_settings::{
    closed, decode_with_rest, encode_with_rest, optional, put, required, Additive, Extensions,
};
pub use record::{Page, SessionRecord, SessionStatus, SessionSummary, Turn, TurnStatus};
pub use sessions::{Sessions, DEFAULT_PAGE, EVENT_RING};
pub use spec::{
    Attribution, CancelRequest, CloseRequest, CreateRequest, EngineBinding, GetRequest,
    ListRequest, MessagesRequest, SendRequest, SessionCreated, SessionSpec, TurnAccepted,
};

/// The answer envelope's version (additive within `0.x`).
pub const API_VERSION: &str = "0.1";

/// The seam's contract-name prefix. A full name is
/// `jinn:session.<store-id>` — see [`store_contract`].
pub const SESSION_CONTRACT_PREFIX: &str = "jinn:session.";

/// The topic every store provider publishes its session events on. One
/// topic for the whole seam: a consumer listens once and routes on the
/// event's own `store` and `session-id`.
pub const EVENT_TOPIC: &str = "jinn:session/event";

/// The settings namespace this definition owns (AGENTS.md standing order
/// 4): the operator's session defaults.
pub const SETTINGS_NAMESPACE: &str = "sessions";

/// Operation: what this store is and what it can do.
pub const OP_DESCRIBE: &str = "describe";
/// Operation: open a session; answers a [`SessionCreated`].
pub const OP_CREATE: &str = "create";
/// Operation: send one message; answers a [`TurnAccepted`] at once.
pub const OP_SEND: &str = "send";
/// Operation: one session's record.
pub const OP_GET: &str = "get";
/// Operation: one page of a session's messages.
pub const OP_MESSAGES: &str = "messages";
/// Operation: the sessions this store holds.
pub const OP_LIST: &str = "list";
/// Operation: one page of a session's event feed.
pub const OP_EVENTS: &str = "events";
/// Operation: cancel the turn in flight.
pub const OP_CANCEL: &str = "cancel";
/// Operation: close a session for good.
pub const OP_CLOSE: &str = "close";

/// The contract name store `id` is served under.
#[must_use]
pub fn store_contract(id: &str) -> String {
    format!("{SESSION_CONTRACT_PREFIX}{id}")
}

/// The store id a contract name carries, or `None` when it is not this
/// seam's. An empty id is not a store.
#[must_use]
pub fn store_id_of(contract: &str) -> Option<&str> {
    contract
        .strip_prefix(SESSION_CONTRACT_PREFIX)
        .filter(|id| !id.is_empty())
}

/// One store as the KERNEL's own view shows it: an entry that provides a
/// `jinn:session.<id>` contract. See [`stores_in`].
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

/// Every session store live in a composition, from `(entry-id,
/// provisions)` pairs as `jinn:introspect` reports them — the kernel's
/// knowledge, not a table a consumer keeps. Sorted by store id.
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
