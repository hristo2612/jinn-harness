//! What a session IS, as read back: its turns, its status, and one page of
//! its messages. The statuses are CLOSED value spaces — a status this
//! version cannot name is refused, never folded onto a neighbour.

use jinn_engine::Usage;
use serde::{Deserialize, Serialize};

use crate::{Extensions, API_VERSION};

/// Where a TURN is. The dangerous answer is [`Self::Done`] — it claims the
/// engine finished and the answer is whole — so it exists only where a
/// terminal record was actually written. Everything else falls to a
/// conservative value, and a turn recovered from a journal with no
/// terminal record is [`Self::Interrupted`], never [`Self::Running`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TurnStatus {
    /// In flight IN THIS INCARNATION. Minted only by the live registry;
    /// a replay cannot produce it.
    #[default]
    Running,
    /// The engine finished and the answer is whole. Requires proof.
    Done,
    /// The engine ran and failed, with a reason.
    Failed,
    /// A caller cancelled it.
    Cancelled,
    /// The daemon stopped while this turn was in flight. The conservative
    /// answer for a started turn with no terminal record — an honest "we
    /// do not know how far it got", never a silent `done`.
    Interrupted,
}

impl TurnStatus {
    /// Whether the turn is over. Only [`Self::Running`] is not.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Running)
    }
}

/// Where a SESSION is, derived from its turns and whether it was closed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionStatus {
    /// Open, nothing in flight.
    #[default]
    Idle,
    /// A turn is in flight in this incarnation.
    Running,
    /// Closed for good; `send` is refused.
    Closed,
    /// The last turn failed or was interrupted; the session is open and
    /// the operator can see why.
    Failed,
}

/// One turn of a session.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Turn {
    pub turn_id: String,
    /// Counts from 0 within a session; a reader orders on it, never on
    /// arrival.
    pub seq: u64,
    pub status: TurnStatus,
    /// What the caller sent.
    pub message: String,
    /// What the engine answered so far. A prefix while running; whole
    /// only where the status is [`TurnStatus::Done`].
    #[serde(default)]
    pub answer: String,
    /// Why it ended, when the ending needs a reason. Present for every
    /// non-`done` terminal status, so a reader never has to infer one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The engine run this turn drove, when one was accepted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default)]
    pub usage: Usage,
    #[serde(default)]
    pub started_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_ms: Option<u64>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One session's record: what `get`, `cancel` and `close` answer.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct SessionRecord {
    #[serde(default)]
    pub api_version: String,
    pub session_id: String,
    pub store: String,
    pub engine: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    pub status: SessionStatus,
    /// How many turns the session holds — the count of the log, not of
    /// this answer.
    #[serde(default)]
    pub turns: u64,
    /// The turns themselves, oldest first.
    #[serde(default)]
    pub log: Vec<Turn>,
    #[serde(default)]
    pub created_ms: u64,
    #[serde(default)]
    pub metadata: Extensions,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One session in a `list` answer: the record without its log.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct SessionSummary {
    pub session_id: String,
    pub store: String,
    pub engine: String,
    pub status: SessionStatus,
    #[serde(default)]
    pub turns: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(default)]
    pub created_ms: u64,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One page of a session's messages. `next-offset` is present only when
/// there IS a next page: absence is the end of the log, so a reader never
/// has to compare a count it was not given.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Page {
    #[serde(default)]
    pub api_version: String,
    pub session_id: String,
    pub offset: u64,
    /// The turns in this page, oldest first.
    #[serde(default)]
    pub messages: Vec<Turn>,
    /// Every turn the session holds.
    #[serde(default)]
    pub total: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<u64>,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl Page {
    /// One page over `turns`, bounded by `limit` and stamped with this
    /// version. `next_offset` is set only where a further turn exists.
    #[must_use]
    pub fn of(session_id: &str, turns: &[Turn], offset: u64, limit: u64) -> Self {
        let total = turns.len() as u64;
        let start = usize::try_from(offset.min(total)).unwrap_or(usize::MAX);
        let end = usize::try_from((offset.saturating_add(limit)).min(total)).unwrap_or(usize::MAX);
        Self {
            api_version: API_VERSION.to_owned(),
            session_id: session_id.to_owned(),
            offset,
            messages: turns[start..end].to_vec(),
            total,
            next_offset: (end as u64 != total).then_some(end as u64),
            extra: Extensions::new(),
        }
    }
}

jinn_settings::closed_value_space!(TurnStatus, "a turn's `status`", {
    "running" => Self::Running,
    "done" => Self::Done,
    "failed" => Self::Failed,
    "cancelled" => Self::Cancelled,
    "interrupted" => Self::Interrupted,
});
jinn_settings::closed_value_space!(SessionStatus, "a session's `status`", {
    "idle" => Self::Idle,
    "running" => Self::Running,
    "closed" => Self::Closed,
    "failed" => Self::Failed,
});

jinn_settings::additive!(Turn, SessionRecord, SessionSummary, Page);
