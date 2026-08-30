//! What a Todo IS, as read back: its history, its comments, its refused
//! moves, its dispatch, and the two statuses a reader is given.
//!
//! # Two statuses, both named, so neither can lie
//!
//! A Todo carries a `declared-status` — the `to` of the last
//! `status-changed` record, which is HISTORY and is never rewritten — and
//! a `status`, which is what the store reports now. They differ in
//! exactly one case, and the case is the interrupted dispatch:
//!
//! > A Todo whose declared status is `executing` and whose dispatch
//! > replayed [`DispatchStatus::Interrupted`] reports
//! > [`Status::Blocked`], carrying the dispatch's reason.
//!
//! That is [`reported_status`], and it is the whole of "never eternally
//! `executing`". It is a DERIVATION, not a repair: the journal is not
//! edited, the declared status is still shown, and the reason a reader is
//! given is the dispatch's own. A store that instead rewrote history to
//! say `blocked` would have destroyed the evidence that the work was ever
//! started.
//!
//! # Every count here is of something counted
//!
//! `comments` and `children` are the length of what the store holds, at
//! the moment of the answer. Nothing is stored as a number beside the
//! thing it counts, because two homes for one fact is how a count starts
//! disagreeing with its list.

use serde::{Deserialize, Serialize};

use crate::{Extensions, Status, API_VERSION};

/// Where a DISPATCH is. The dangerous answer is [`Self::Done`] — it
/// claims the session finished the work — so it exists only where a
/// terminal record was written. [`Self::Running`] is minted only by the
/// live registry for a dispatch THIS incarnation started; a replay cannot
/// produce it (`crate::journal`), so a daemon that died mid-dispatch
/// cannot come back claiming to be working.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DispatchStatus {
    /// In flight IN THIS INCARNATION.
    #[default]
    Running,
    /// The session's turn finished whole.
    Done,
    /// The session ran and failed, with a reason.
    Failed,
    /// A caller cancelled it.
    Cancelled,
    /// The daemon stopped while this dispatch was in flight. The
    /// conservative answer for a started dispatch with no ending.
    Interrupted,
}

impl DispatchStatus {
    /// Whether the dispatch is over. Only [`Self::Running`] is not.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Running)
    }
}

/// One dispatch of a Todo to a session: the three-layer stack's middle
/// link, recorded.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Dispatch {
    pub dispatch_id: String,
    /// The SESSION store it was sent to — `jinn:session.<store>`.
    pub session_store: String,
    /// The engine the session was bound to. Recorded so a reader can see
    /// WHICH engine ran the work without re-reading the session.
    pub engine: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    pub status: DispatchStatus,
    /// Why it ended, for every non-`done` ending. Present by the
    /// registry's rule, so a reader never has to infer one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// What the session answered. A prefix while running; whole only
    /// where the status is [`DispatchStatus::Done`].
    #[serde(default)]
    pub answer: String,
    #[serde(default)]
    pub started_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_ms: Option<u64>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One status move, as history holds it. Append-only: a move is a NEW
/// record, and no later record edits this one.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct StatusChange {
    /// Counts from 0 within a Todo; a reader orders on it, never on
    /// arrival.
    pub seq: u64,
    pub from: Status,
    pub to: Status,
    /// The principal who asked. `null` is "nobody was declared" and is
    /// never a principal (`crate::spec`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default)]
    pub at_ms: u64,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One move the LEDGER refused, recorded. A refusal is a fact about what
/// was attempted, and this seam keeps it: an operator reading a Todo can
/// see that something tried to close it from `executing` and was told no.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RefusedChange {
    pub seq: u64,
    pub from: Status,
    pub to: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(default)]
    pub at_ms: u64,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One comment.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Comment {
    pub comment_id: String,
    pub seq: u64,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(default)]
    pub at_ms: u64,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One Todo's record: what `get`, `update`, `comment` and `dispatch`
/// answer.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TodoRecord {
    #[serde(default)]
    pub api_version: String,
    pub todo_id: String,
    pub store: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub acceptance: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub department: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
    /// What the store reports NOW — the fold (see the module doc).
    pub status: Status,
    /// The `to` of the last `status-changed` record. History, verbatim.
    pub declared_status: Status,
    /// Why [`Self::status`] differs from [`Self::declared_status`], when
    /// it does. Absent when they agree — never an empty string standing
    /// in for "no reason".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_reason: Option<String>,
    /// Every status move, oldest first. Append-only.
    #[serde(default)]
    pub history: Vec<StatusChange>,
    /// Every move the ledger refused, oldest first.
    #[serde(default)]
    pub refused: Vec<RefusedChange>,
    #[serde(default)]
    pub comments: Vec<Comment>,
    /// Every dispatch of this Todo, oldest first. A Todo sent to a
    /// session twice holds both: the earlier one is history, and history
    /// is not overwritten by the next attempt.
    #[serde(default)]
    pub dispatches: Vec<Dispatch>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(default)]
    pub created_ms: u64,
    #[serde(default)]
    pub metadata: Extensions,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One Todo in a `list` answer: the record without its history.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TodoSummary {
    pub todo_id: String,
    pub store: String,
    pub title: String,
    pub status: Status,
    pub declared_status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub department: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
    #[serde(default)]
    pub comments: u64,
    #[serde(default)]
    pub created_ms: u64,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `list` answer.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TodoList {
    #[serde(default)]
    pub api_version: String,
    pub store: String,
    #[serde(default)]
    pub todos: Vec<TodoSummary>,
    /// Every Todo this store holds, before the filter. A filtered answer
    /// that reported only its own length would let an operator read "3
    /// todos" as "3 todos exist".
    #[serde(default)]
    pub total: u64,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One node of a `tree` answer: a Todo and the Todos parented to it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TreeNode {
    #[serde(flatten)]
    pub todo: TodoSummary,
    #[serde(default)]
    pub children: Vec<TreeNode>,
}

/// The `tree` answer.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Tree {
    #[serde(default)]
    pub api_version: String,
    pub store: String,
    pub root: TreeNode,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl Tree {
    /// A tree stamped with this version.
    #[must_use]
    pub fn of(store: &str, root: TreeNode) -> Self {
        Self {
            api_version: API_VERSION.to_owned(),
            store: store.to_owned(),
            root,
            extra: Extensions::new(),
        }
    }
}

/// The reason a Todo blocked by an interrupted dispatch carries. One
/// string, one home: the store reports it, the API relays it, and a test
/// asserts on it.
pub const INTERRUPTED_STATUS_REASON: &str =
    "this Todo's dispatch was interrupted when the daemon stopped; how far it got is not \
     recorded, so it is blocked rather than still executing";

/// What a store REPORTS, from what history DECLARES and what the dispatch
/// turned out to be. The module doc states the law; this is the whole of
/// it, in one place, so the API, the record and the summary cannot each
/// fold differently.
#[must_use]
pub fn reported_status(declared: Status, dispatch: Option<&Dispatch>) -> (Status, Option<String>) {
    let interrupted =
        dispatch.is_some_and(|dispatch| dispatch.status == DispatchStatus::Interrupted);
    if declared == Status::Executing && interrupted {
        // `executing -> blocked` is a LEGAL move (`Status::allows`), so
        // the fold can never report a state the table would refuse.
        debug_assert!(Status::Executing.allows_move_to(Status::Blocked));
        return (Status::Blocked, Some(INTERRUPTED_STATUS_REASON.to_owned()));
    }
    (declared, None)
}

jinn_settings::closed_value_space!(DispatchStatus, "a dispatch's `status`", {
    "running" => Self::Running,
    "done" => Self::Done,
    "failed" => Self::Failed,
    "cancelled" => Self::Cancelled,
    "interrupted" => Self::Interrupted,
});

jinn_settings::additive!(
    Dispatch,
    StatusChange,
    RefusedChange,
    Comment,
    TodoRecord,
    TodoSummary,
    TodoList,
    Tree,
);

#[cfg(test)]
mod tests {
    use super::*;

    fn dispatch(status: DispatchStatus) -> Dispatch {
        Dispatch {
            dispatch_id: "d1".to_owned(),
            status,
            ..Dispatch::default()
        }
    }

    #[test]
    fn an_interrupted_dispatch_is_never_reported_as_still_executing() {
        let (status, reason) = reported_status(
            Status::Executing,
            Some(&dispatch(DispatchStatus::Interrupted)),
        );
        assert_eq!(status, Status::Blocked);
        assert_eq!(reason.as_deref(), Some(INTERRUPTED_STATUS_REASON));
    }

    #[test]
    fn every_other_reading_is_history_untouched() {
        // A dispatch that ENDED is not an interruption: the work ran, and
        // the Todo is exactly where its history says.
        for ended in [
            DispatchStatus::Done,
            DispatchStatus::Failed,
            DispatchStatus::Cancelled,
            DispatchStatus::Running,
        ] {
            assert_eq!(
                reported_status(Status::Executing, Some(&dispatch(ended))),
                (Status::Executing, None)
            );
        }
        // And an interrupted dispatch does not move a Todo that was not
        // executing: only `executing` is the claim that needs correcting.
        for declared in [
            Status::Backlog,
            Status::InReview,
            Status::Blocked,
            Status::Done,
        ] {
            assert_eq!(
                reported_status(declared, Some(&dispatch(DispatchStatus::Interrupted))),
                (declared, None)
            );
        }
        assert_eq!(
            reported_status(Status::Executing, None),
            (Status::Executing, None)
        );
    }

    #[test]
    fn a_dispatch_status_round_trips_and_refuses_what_it_cannot_name() {
        for status in [
            DispatchStatus::Running,
            DispatchStatus::Done,
            DispatchStatus::Failed,
            DispatchStatus::Cancelled,
            DispatchStatus::Interrupted,
        ] {
            let encoded = serde_json::to_value(status).expect("encodes");
            assert_eq!(
                serde_json::from_value::<DispatchStatus>(encoded).expect("decodes"),
                status
            );
            assert_eq!(status.is_terminal(), status != DispatchStatus::Running);
        }
        assert!(serde_json::from_value::<DispatchStatus>(serde_json::json!("maybe")).is_err());
    }
}
