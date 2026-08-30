//! The Todo events on [`crate::EVENT_TOPIC`]: what a listener receives,
//! and the one hand-coded rest map that carries a kind this version
//! cannot read straight through it.

use serde::{Deserialize, Serialize};

use crate::{
    decode_with_rest, encode_with_rest, optional, put, required, Additive, DispatchStatus,
    Extensions, Status, API_VERSION,
};

/// What a Todo event SAYS. The rest of the event lives in the one rest
/// map on [`Event`], never in a field per variant.
#[derive(Clone, Debug, PartialEq)]
pub enum EventKind {
    /// A Todo was recorded.
    Created { title: String },
    /// A status moved. `from` and `to` are BOTH carried: a listener that
    /// was given only the destination could not tell a move apart from a
    /// restatement.
    StatusChanged {
        from: Status,
        to: Status,
        actor: Option<String>,
    },
    /// A move the ledger REFUSED. On the bus for the same reason it is in
    /// the journal: an attempt to close work by a path the company's law
    /// forbids is something an operator should be able to see.
    TransitionRefused {
        from: Status,
        to: Status,
        actor: Option<String>,
    },
    /// A comment was added.
    Commented {
        comment_id: String,
        actor: Option<String>,
    },
    /// The Todo was sent to a session.
    Dispatched {
        dispatch_id: String,
        session_store: String,
        engine: String,
    },
    /// That dispatch ended.
    DispatchEnded {
        dispatch_id: String,
        status: DispatchStatus,
        reason: Option<String>,
    },
    /// The Todo reached a terminal status.
    Closed { status: Status },
    /// A kind this version does not know: its tag and its whole payload,
    /// kept and counted — never dropped, never guessed.
    Unknown { kind: String },
}

/// One Todo event: what this version can read plus everything it cannot,
/// verbatim.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub kind: EventKind,
    pub extra: Extensions,
}

impl From<EventKind> for Event {
    fn from(kind: EventKind) -> Self {
        Self {
            kind,
            extra: Extensions::new(),
        }
    }
}

impl Additive for Event {
    fn rest(&self) -> &Extensions {
        &self.extra
    }
}

/// The `kind` tags this version knows.
const KNOWN_KINDS: [&str; 7] = [
    "created",
    "status-changed",
    "transition-refused",
    "commented",
    "dispatched",
    "dispatch-ended",
    "closed",
];

impl Event {
    /// The `kind` tag this event goes on the wire under.
    #[must_use]
    pub fn kind_tag(&self) -> &str {
        match &self.kind {
            EventKind::Created { .. } => KNOWN_KINDS[0],
            EventKind::StatusChanged { .. } => KNOWN_KINDS[1],
            EventKind::TransitionRefused { .. } => KNOWN_KINDS[2],
            EventKind::Commented { .. } => KNOWN_KINDS[3],
            EventKind::Dispatched { .. } => KNOWN_KINDS[4],
            EventKind::DispatchEnded { .. } => KNOWN_KINDS[5],
            EventKind::Closed { .. } => KNOWN_KINDS[6],
            EventKind::Unknown { kind } => kind,
        }
    }

    fn to_map(&self) -> Extensions {
        let mut known = Extensions::new();
        put(&mut known, "kind", self.kind_tag());
        match &self.kind {
            EventKind::Created { title } => put(&mut known, "title", title),
            EventKind::StatusChanged { from, to, actor }
            | EventKind::TransitionRefused { from, to, actor } => {
                put(&mut known, "from", from);
                put(&mut known, "to", to);
                put_actor(&mut known, actor.as_deref());
            }
            EventKind::Commented { comment_id, actor } => {
                put(&mut known, "comment-id", comment_id);
                put_actor(&mut known, actor.as_deref());
            }
            EventKind::Dispatched {
                dispatch_id,
                session_store,
                engine,
            } => {
                put(&mut known, "dispatch-id", dispatch_id);
                put(&mut known, "session-store", session_store);
                put(&mut known, "engine", engine);
            }
            EventKind::DispatchEnded {
                dispatch_id,
                status,
                reason,
            } => {
                put(&mut known, "dispatch-id", dispatch_id);
                put(&mut known, "dispatch-status", status);
                if let Some(reason) = reason {
                    put(&mut known, "reason", reason);
                }
            }
            EventKind::Closed { status } => put(&mut known, "status", status),
            // Nothing of an unknown kind is known but the tag; its whole
            // payload is in the rest map, added next like any other.
            EventKind::Unknown { .. } => {}
        }
        encode_with_rest(known, &self.extra)
    }

    fn from_map(map: Extensions) -> Result<Self, String> {
        let (kind, extra) = decode_with_rest(map, |map| {
            let tag: String = required(map, "kind")?;
            Ok(match tag.as_str() {
                "created" => EventKind::Created {
                    title: required(map, "title")?,
                },
                "status-changed" => EventKind::StatusChanged {
                    from: required(map, "from")?,
                    to: required(map, "to")?,
                    actor: optional(map, "actor")?,
                },
                "transition-refused" => EventKind::TransitionRefused {
                    from: required(map, "from")?,
                    to: required(map, "to")?,
                    actor: optional(map, "actor")?,
                },
                "commented" => EventKind::Commented {
                    comment_id: required(map, "comment-id")?,
                    actor: optional(map, "actor")?,
                },
                "dispatched" => EventKind::Dispatched {
                    dispatch_id: required(map, "dispatch-id")?,
                    session_store: required(map, "session-store")?,
                    engine: required(map, "engine")?,
                },
                "dispatch-ended" => EventKind::DispatchEnded {
                    dispatch_id: required(map, "dispatch-id")?,
                    status: required(map, "dispatch-status")?,
                    reason: optional(map, "reason")?,
                },
                "closed" => EventKind::Closed {
                    status: required(map, "status")?,
                },
                _ => EventKind::Unknown { kind: tag },
            })
        })?;
        Ok(Self { kind, extra })
    }
}

/// Writes an actor onto the wire only where one was DECLARED. An absent
/// actor is an absent key, never `""` and never a placeholder name — the
/// sentinel rule (`crate::spec`).
fn put_actor(known: &mut Extensions, actor: Option<&str>) {
    if let Some(actor) = actor {
        put(known, "actor", actor);
    }
}

impl Serialize for Event {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_map().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Event {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_map(Extensions::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// One Todo event with its attribution, as a listener receives it. The
/// envelope declares no rest map of its OWN: the flattened [`Event`]
/// already is one, and two rest maps at one level would each swallow the
/// other's fields.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TodoEvent {
    #[serde(default)]
    pub api_version: String,
    pub store: String,
    pub todo_id: String,
    /// Counts from 0 per Todo — a listener orders and de-duplicates on
    /// it, never on arrival.
    pub seq: u64,
    #[serde(flatten)]
    pub event: Event,
}

impl Additive for TodoEvent {
    fn rest(&self) -> &Extensions {
        self.event.rest()
    }
}

impl TodoEvent {
    #[must_use]
    pub fn new(store: &str, todo_id: &str, seq: u64, kind: EventKind) -> Self {
        Self {
            api_version: API_VERSION.to_owned(),
            store: store.to_owned(),
            todo_id: todo_id.to_owned(),
            seq,
            event: kind.into(),
        }
    }
}

/// `events`: the feed of one Todo's events from `after` onward. Polled,
/// for the reason the sessions seam's feed is (`FINDINGS.md` #4, #32,
/// with one home in `plugins/sessions/jinn-session/src/event.rs`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct EventsRequest {
    pub todo_id: String,
    /// Events after this sequence. Absent means from the beginning —
    /// never "the latest", which would silently drop the backlog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One page of a Todo's event feed.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct EventPage {
    #[serde(default)]
    pub api_version: String,
    pub todo_id: String,
    #[serde(default)]
    pub events: Vec<TodoEvent>,
    /// The sequence to ask `after` next. Always present: a reader that
    /// caught up still needs the cursor to ask again with.
    #[serde(default)]
    pub next_after: u64,
    /// How many events the store has DROPPED from the front of this
    /// Todo's ring. A feed that lost history says so.
    #[serde(default)]
    pub dropped: u64,
    #[serde(flatten)]
    pub extra: Extensions,
}

jinn_settings::additive!(EventsRequest, EventPage);
