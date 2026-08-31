//! The workflow events on the seam's event topic: what a listener
//! receives, and the one hand-coded rest map that carries a kind this
//! version cannot read straight through it.
//!
//! A run is watched while it happens, by readers that were written at
//! different times. So the feed's law is the wire law: what this version
//! can name it names, and everything else rides through verbatim under
//! its own tag. That is why an unknown kind is [`EventKind::Unknown`] and
//! not a decode error — the journal refuses a line it cannot replay
//! (`crate::journal`), because replaying a different run than the
//! document holds would be a lie, but a listener that skipped an event it
//! could not read would simply be told less than happened.

use serde::{Deserialize, Serialize};

use crate::{
    decode_with_rest, encode_with_rest, optional, put, required, Additive, Extensions, NodeState,
    RunStatus, API_VERSION,
};

/// What a workflow event SAYS. The rest of the event lives in the one
/// rest map on [`Event`], never in a field per variant.
#[derive(Clone, Debug, PartialEq)]
pub enum EventKind {
    /// A workflow revision was recorded. Carries the revision so a
    /// listener knows WHICH definition it was told about.
    Defined { workflow_id: String, revision: u64 },
    /// A run opened, on the revision it PINNED. Both are carried: a
    /// listener given only the workflow could not tell which definition
    /// the run will execute for the rest of its life.
    RunStarted { workflow_id: String, revision: u64 },
    /// A node's work began.
    NodeStarted { node_id: String },
    /// A node's work ended. `outcome` is the node's own state, so an
    /// ending that was not `done` says which ending it was, and carries
    /// the reason the registry required of it.
    NodeEnded {
        node_id: String,
        outcome: NodeState,
        reason: Option<String>,
    },
    /// A node-state move the ledger REFUSED. On the bus for the same
    /// reason it is in the record: an attempt to claim a step was carried
    /// out by a path the transition table forbids is something an
    /// operator should be able to see. `from` and `to` are BOTH carried,
    /// because a message naming one half leaves the other to be guessed.
    NodeTransitionRefused {
        node_id: String,
        from: NodeState,
        to: NodeState,
        actor: Option<String>,
    },
    /// The run reached a terminal status.
    RunEnded {
        status: RunStatus,
        reason: Option<String>,
    },
    /// A kind this version does not know: its tag and its whole payload,
    /// kept and counted — never dropped, never guessed.
    Unknown { kind: String },
}

/// One workflow event: what this version can read plus everything it
/// cannot, verbatim.
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
const KNOWN_KINDS: [&str; 6] = [
    "defined",
    "run-started",
    "node-started",
    "node-ended",
    "node-transition-refused",
    "run-ended",
];

impl Event {
    /// The `kind` tag this event goes on the wire under.
    #[must_use]
    pub fn kind_tag(&self) -> &str {
        match &self.kind {
            EventKind::Defined { .. } => KNOWN_KINDS[0],
            EventKind::RunStarted { .. } => KNOWN_KINDS[1],
            EventKind::NodeStarted { .. } => KNOWN_KINDS[2],
            EventKind::NodeEnded { .. } => KNOWN_KINDS[3],
            EventKind::NodeTransitionRefused { .. } => KNOWN_KINDS[4],
            EventKind::RunEnded { .. } => KNOWN_KINDS[5],
            EventKind::Unknown { kind } => kind,
        }
    }

    fn to_map(&self) -> Extensions {
        let mut known = Extensions::new();
        put(&mut known, "kind", self.kind_tag());
        match &self.kind {
            EventKind::Defined {
                workflow_id,
                revision,
            }
            | EventKind::RunStarted {
                workflow_id,
                revision,
            } => {
                put(&mut known, "workflow-id", workflow_id);
                put(&mut known, "revision", revision);
            }
            EventKind::NodeStarted { node_id } => put(&mut known, "node-id", node_id),
            EventKind::NodeEnded {
                node_id,
                outcome,
                reason,
            } => {
                put(&mut known, "node-id", node_id);
                put(&mut known, "outcome", outcome);
                if let Some(reason) = reason {
                    put(&mut known, "reason", reason);
                }
            }
            EventKind::NodeTransitionRefused {
                node_id,
                from,
                to,
                actor,
            } => {
                put(&mut known, "node-id", node_id);
                put(&mut known, "from", from);
                put(&mut known, "to", to);
                put_actor(&mut known, actor.as_deref());
            }
            EventKind::RunEnded { status, reason } => {
                put(&mut known, "status", status);
                if let Some(reason) = reason {
                    put(&mut known, "reason", reason);
                }
            }
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
                "defined" => EventKind::Defined {
                    workflow_id: required(map, "workflow-id")?,
                    revision: required(map, "revision")?,
                },
                "run-started" => EventKind::RunStarted {
                    workflow_id: required(map, "workflow-id")?,
                    revision: required(map, "revision")?,
                },
                "node-started" => EventKind::NodeStarted {
                    node_id: required(map, "node-id")?,
                },
                "node-ended" => EventKind::NodeEnded {
                    node_id: required(map, "node-id")?,
                    outcome: required(map, "outcome")?,
                    reason: optional(map, "reason")?,
                },
                "node-transition-refused" => EventKind::NodeTransitionRefused {
                    node_id: required(map, "node-id")?,
                    from: required(map, "from")?,
                    to: required(map, "to")?,
                    actor: optional(map, "actor")?,
                },
                "run-ended" => EventKind::RunEnded {
                    status: required(map, "status")?,
                    reason: optional(map, "reason")?,
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

/// One run event with its attribution, as a listener receives it. The
/// envelope declares no rest map of its OWN: the flattened [`Event`]
/// already is one, and two rest maps at one level would each swallow the
/// other's fields.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RunEvent {
    #[serde(default)]
    pub api_version: String,
    pub store: String,
    pub run_id: String,
    /// Counts from 0 per run — a listener orders and de-duplicates on it,
    /// never on arrival.
    pub seq: u64,
    #[serde(flatten)]
    pub event: Event,
}

impl Additive for RunEvent {
    fn rest(&self) -> &Extensions {
        self.event.rest()
    }
}

impl RunEvent {
    #[must_use]
    pub fn new(store: &str, run_id: &str, seq: u64, kind: EventKind) -> Self {
        Self {
            api_version: API_VERSION.to_owned(),
            store: store.to_owned(),
            run_id: run_id.to_owned(),
            seq,
            event: kind.into(),
        }
    }
}

/// One page of a run's event feed.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct EventsPage {
    #[serde(default)]
    pub api_version: String,
    pub store: String,
    pub run_id: String,
    #[serde(default)]
    pub events: Vec<RunEvent>,
    /// How many events the store has DROPPED from the front of this run's
    /// ring. A feed that lost history says so, so a reader is never told a
    /// gap is quiet.
    #[serde(default)]
    pub dropped: u64,
    #[serde(flatten)]
    pub extra: Extensions,
}

jinn_settings::additive!(EventsPage);

#[cfg(test)]
mod tests {
    use super::*;

    fn unknown(kind: &str) -> EventKind {
        EventKind::Unknown {
            kind: kind.to_owned(),
        }
    }

    fn every_kind() -> Vec<EventKind> {
        vec![
            EventKind::Defined {
                workflow_id: "wf-1".to_owned(),
                revision: 1,
            },
            EventKind::RunStarted {
                workflow_id: "wf-1".to_owned(),
                revision: 1,
            },
            EventKind::NodeStarted {
                node_id: "review".to_owned(),
            },
            EventKind::NodeEnded {
                node_id: "review".to_owned(),
                outcome: NodeState::Done,
                reason: None,
            },
            EventKind::NodeEnded {
                node_id: "review".to_owned(),
                outcome: NodeState::Failed,
                reason: Some("the check did not pass".to_owned()),
            },
            EventKind::NodeTransitionRefused {
                node_id: "review".to_owned(),
                from: NodeState::Pending,
                to: NodeState::Done,
                actor: Some("the producer".to_owned()),
            },
            EventKind::RunEnded {
                status: RunStatus::Done,
                reason: None,
            },
            EventKind::RunEnded {
                status: RunStatus::Failed,
                reason: Some("node \"review\" ended failed".to_owned()),
            },
            unknown("run-paused"),
        ]
    }

    #[test]
    fn every_kind_round_trips_under_its_own_tag() {
        for kind in every_kind() {
            let event = Event::from(kind);
            let doc = serde_json::to_value(&event).expect("an event encodes");
            assert_eq!(doc["kind"], serde_json::json!(event.kind_tag()));
            let back: Event = serde_json::from_value(doc).expect("decodes");
            assert_eq!(back, event);
        }
    }

    #[test]
    fn a_kind_this_version_cannot_name_is_kept_whole_and_counted_as_itself() {
        let doc = serde_json::json!({
            "kind": "run-paused",
            "node-id": "review",
            "until-ms": 1_700_000_000_u64,
        });
        let event: Event = serde_json::from_value(doc.clone()).expect("an unknown kind decodes");
        // Named as what it is, not folded onto a neighbour: `run-paused`
        // is not `run-ended`, and a listener told it had ended would act
        // on a run that is still there.
        assert_eq!(event.kind, unknown("run-paused"));
        assert_eq!(event.kind_tag(), "run-paused");
        // Its whole payload is kept, and re-encoding gives back the very
        // document that arrived.
        assert_eq!(event.rest()["node-id"], "review");
        assert_eq!(event.rest()["until-ms"], 1_700_000_000_u64);
        assert_eq!(serde_json::to_value(&event).expect("re-encodes"), doc);
    }

    #[test]
    fn a_known_kind_carries_what_this_version_cannot_read_verbatim() {
        let doc = serde_json::json!({
            "kind": "node-ended",
            "node-id": "review",
            "outcome": "failed",
            "reason": "the check did not pass",
            "attempt": 2,
        });
        let event: Event = serde_json::from_value(doc.clone()).expect("decodes");
        assert_eq!(
            event.kind,
            EventKind::NodeEnded {
                node_id: "review".to_owned(),
                outcome: NodeState::Failed,
                reason: Some("the check did not pass".to_owned()),
            }
        );
        assert_eq!(event.rest()["attempt"], 2);
        assert_eq!(serde_json::to_value(&event).expect("re-encodes"), doc);
    }

    #[test]
    fn an_actor_nobody_declared_is_an_absent_key_never_a_placeholder() {
        let event = Event::from(EventKind::NodeTransitionRefused {
            node_id: "review".to_owned(),
            from: NodeState::Pending,
            to: NodeState::Done,
            actor: None,
        });
        let doc = serde_json::to_value(&event).expect("encodes");
        assert!(doc.get("actor").is_none(), "{doc}");
        assert_eq!(doc["from"], "pending");
        assert_eq!(doc["to"], "done");
        let back: Event = serde_json::from_value(doc).expect("decodes");
        assert_eq!(back, event);
    }

    #[test]
    fn an_ending_that_explains_itself_writes_no_empty_reason() {
        let doc = serde_json::to_value(Event::from(EventKind::RunEnded {
            status: RunStatus::Done,
            reason: None,
        }))
        .expect("encodes");
        assert_eq!(doc["status"], "done");
        assert!(doc.get("reason").is_none(), "{doc}");
    }

    #[test]
    fn a_run_event_carries_its_store_its_run_and_its_sequence() {
        let event = RunEvent::new(
            "default",
            "default-1",
            3,
            EventKind::RunStarted {
                workflow_id: "wf-1".to_owned(),
                revision: 2,
            },
        );
        assert_eq!(event.api_version, API_VERSION);
        let doc = serde_json::to_value(&event).expect("encodes");
        assert_eq!(doc["kind"], "run-started");
        assert_eq!(doc["store"], "default");
        assert_eq!(doc["run-id"], "default-1");
        assert_eq!(doc["seq"], 3);
        assert_eq!(doc["revision"], 2);
        let back: RunEvent = serde_json::from_value(doc).expect("decodes");
        assert_eq!(back, event);
    }

    #[test]
    fn a_page_that_lost_history_says_so_and_still_carries_what_it_cannot_name() {
        let page = EventsPage {
            api_version: API_VERSION.to_owned(),
            store: "default".to_owned(),
            run_id: "default-1".to_owned(),
            events: vec![
                RunEvent::new(
                    "default",
                    "default-1",
                    0,
                    EventKind::NodeStarted {
                        node_id: "review".to_owned(),
                    },
                ),
                RunEvent::new("default", "default-1", 1, unknown("run-paused")),
            ],
            dropped: 5,
            extra: Extensions::new(),
        };
        let doc = serde_json::to_value(&page).expect("encodes");
        assert_eq!(doc["dropped"], 5);
        assert_eq!(doc["events"][1]["kind"], "run-paused");
        let back: EventsPage = serde_json::from_value(doc).expect("decodes");
        assert_eq!(back, page);
    }
}
