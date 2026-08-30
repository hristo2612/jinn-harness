//! The session events on [`crate::EVENT_TOPIC`]: what a listener
//! receives, and the one hand-coded rest map that carries a kind this
//! version cannot read straight through it.

use serde::{Deserialize, Serialize};

use crate::{
    decode_with_rest, encode_with_rest, optional, put, required, Additive, Extensions, API_VERSION,
};

/// What a session event SAYS. The rest of the event lives in the one rest
/// map on [`Event`], never in a field per variant.
#[derive(Clone, Debug, PartialEq)]
pub enum EventKind {
    /// A session opened.
    Created { engine: String },
    /// A turn started.
    TurnStarted { turn_id: String, message: String },
    /// A chunk of the answer.
    Delta { turn_id: String, text: String },
    /// A turn finished whole.
    TurnEnded {
        turn_id: String,
        usage: jinn_engine::Usage,
    },
    /// A turn ended without an answer, and why.
    TurnFailed { turn_id: String, reason: String },
    /// The session closed.
    Closed,
    /// A kind this version does not know: its tag and its whole payload,
    /// kept and counted — never dropped, never guessed.
    Unknown { kind: String },
}

/// One session event: what this version can read plus everything it
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
    "created",
    "turn-started",
    "delta",
    "turn-ended",
    "turn-failed",
    "closed",
];

impl Event {
    /// The `kind` tag this event goes on the wire under.
    #[must_use]
    pub fn kind_tag(&self) -> &str {
        match &self.kind {
            EventKind::Created { .. } => KNOWN_KINDS[0],
            EventKind::TurnStarted { .. } => KNOWN_KINDS[1],
            EventKind::Delta { .. } => KNOWN_KINDS[2],
            EventKind::TurnEnded { .. } => KNOWN_KINDS[3],
            EventKind::TurnFailed { .. } => KNOWN_KINDS[4],
            EventKind::Closed => KNOWN_KINDS[5],
            EventKind::Unknown { kind } => kind,
        }
    }

    fn to_map(&self) -> Extensions {
        let mut known = Extensions::new();
        put(&mut known, "kind", self.kind_tag());
        match &self.kind {
            EventKind::Created { engine } => put(&mut known, "engine", engine),
            EventKind::TurnStarted { turn_id, message } => {
                put(&mut known, "turn-id", turn_id);
                put(&mut known, "message", message);
            }
            EventKind::Delta { turn_id, text } => {
                put(&mut known, "turn-id", turn_id);
                put(&mut known, "text", text);
            }
            EventKind::TurnEnded { turn_id, usage } => {
                put(&mut known, "turn-id", turn_id);
                put(&mut known, "usage", usage);
            }
            EventKind::TurnFailed { turn_id, reason } => {
                put(&mut known, "turn-id", turn_id);
                put(&mut known, "reason", reason);
            }
            // Nothing of these is known but the tag; an unknown kind's
            // whole payload is in the rest map, added next like any other.
            EventKind::Closed | EventKind::Unknown { .. } => {}
        }
        encode_with_rest(known, &self.extra)
    }

    fn from_map(map: Extensions) -> Result<Self, String> {
        let (kind, extra) = decode_with_rest(map, |map| {
            let tag: String = required(map, "kind")?;
            Ok(match tag.as_str() {
                "created" => EventKind::Created {
                    engine: required(map, "engine")?,
                },
                "turn-started" => EventKind::TurnStarted {
                    turn_id: required(map, "turn-id")?,
                    message: required(map, "message")?,
                },
                "delta" => EventKind::Delta {
                    turn_id: required(map, "turn-id")?,
                    text: required(map, "text")?,
                },
                "turn-ended" => EventKind::TurnEnded {
                    turn_id: required(map, "turn-id")?,
                    usage: optional(map, "usage")?,
                },
                "turn-failed" => EventKind::TurnFailed {
                    turn_id: required(map, "turn-id")?,
                    reason: required(map, "reason")?,
                },
                "closed" => EventKind::Closed,
                _ => EventKind::Unknown { kind: tag },
            })
        })?;
        Ok(Self { kind, extra })
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

/// One session event with its attribution, as a listener receives it. The
/// envelope declares no rest map of its OWN: the flattened [`Event`]
/// already is one, and two rest maps at one level would each swallow the
/// other's fields.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct SessionEvent {
    #[serde(default)]
    pub api_version: String,
    pub store: String,
    pub session_id: String,
    /// Counts from 0 per session — a listener orders and de-duplicates on
    /// it, never on arrival.
    pub seq: u64,
    #[serde(flatten)]
    pub event: Event,
}

impl Additive for SessionEvent {
    fn rest(&self) -> &Extensions {
        self.event.rest()
    }
}

impl SessionEvent {
    #[must_use]
    pub fn new(store: &str, session_id: &str, seq: u64, kind: EventKind) -> Self {
        Self {
            api_version: API_VERSION.to_owned(),
            store: store.to_owned(),
            session_id: session_id.to_owned(),
            seq,
            event: kind.into(),
        }
    }
}
