//! What a RUN is, as read back: the revision it pinned, its nodes, its
//! history, its refused moves, and the status the store reports.
//!
//! # One status field, and no second one to disagree with it
//!
//! [`RunStatus::Running`] and [`crate::NodeState::Running`] are minted
//! ONLY by the live registry, for a run this incarnation started and is
//! driving. A REPLAY reports what the document says — `running`
//! included — because inventing a line nobody wrote is not a reader's
//! job. What makes "never eternally `running`" true is the ORDER a
//! durable store activates in: it replays, plans the recovery
//! ([`crate::Workflows::plan_recovery`]), APPENDS the
//! `running -> interrupted` moves and the run's own ending, and only then
//! provides its contract. A store whose recovery append is refused fails
//! to activate rather than serving a `running` no durable line justifies.
//! So the interrupted answer is a RECORD carrying a reason, not a fold
//! that could disagree with one.
//!
//! That is why this seam needs no second, folded status beside its
//! declared one, and the todos seam one layer down did
//! (`plugins/todos/jinn-todo/src/record.rs`): there the interrupted fact
//! lived in the DISPATCH's vocabulary while the ledger's claim lived in
//! the Todo's `status`, so the two could disagree and a fold had to
//! reconcile them. Here the interruption is a value of the node's own
//! state space, so the recorded state and the reported state are the same
//! field — and the recovery makes the record SAY it, rather than a
//! derivation saying it on the record's behalf.
//!
//! # Every count here is of something counted
//!
//! `nodes`, `history` and `refused` are the length of what the store
//! holds at the moment of the answer. Nothing is stored as a number
//! beside the thing it counts.

use serde::{Deserialize, Serialize};

use crate::{Extensions, NodeKind, NodeState, WorkflowSpec};

/// The reason a node left running at replay carries. One string, one
/// home.
pub const INTERRUPTED_NODE_REASON: &str =
    "the daemon stopped while this node was running; how far it got is not recorded";

/// The reason a run left open at replay carries.
pub const INTERRUPTED_RUN_REASON: &str =
    "the daemon stopped while this run was in flight; the nodes it had not finished are \
     recorded interrupted";

/// The reason a node whose Todo became unreadable carries.
pub const LOST_TODO_REASON: &str =
    "the Todo this node dispatched is no longer readable, so how far it got is not recorded";

/// Where a RUN is. [`Self::Running`] is minted only by the live registry
/// (see the module doc), so a replay cannot produce it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunStatus {
    /// This incarnation is driving it.
    #[default]
    Running,
    /// Every node that ran, ran to `done`. The dangerous claim — that the
    /// procedure was carried out — so it exists only where a terminal
    /// record was written.
    Done,
    /// The run ended and at least one node did not reach `done`.
    Failed,
    /// Abandoned on the record, with a reason.
    Cancelled,
    /// The run was started and no ending was ever recorded.
    Interrupted,
}

impl RunStatus {
    /// Whether the status is an ending.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Running)
    }

    /// Whether an ending in this status MUST carry a reason.
    #[must_use]
    pub fn needs_reason(self) -> bool {
        self.is_terminal() && self != Self::Done
    }

    /// The status's wire tag.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Interrupted => "interrupted",
        }
    }
}

/// One node of one run.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct NodeRun {
    pub node_id: String,
    pub kind: NodeKind,
    pub state: NodeState,
    /// Why the node is where it is. Present on every ending but `done`
    /// (the registry's rule), so no reader ever has to invent one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The Todo this node dispatched, for a [`NodeKind::Dispatch`] node
    /// that got as far as recording one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub todo_store: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub todo_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch_id: Option<String>,
    /// What the node's work answered, where it answered anything.
    #[serde(default)]
    pub answer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_ms: Option<u64>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One recorded node-state move. HISTORY: appended, never edited.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct NodeChange {
    pub seq: u64,
    pub node_id: String,
    pub from: NodeState,
    pub to: NodeState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub at_ms: u64,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One REFUSED node-state move: an attempt, recorded. An attempt is a
/// fact, and an operator reading the ledger sees it even if the caller
/// dropped the answer on the floor.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RefusedChange {
    pub seq: u64,
    pub node_id: String,
    pub from: NodeState,
    pub to: NodeState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    pub at_ms: u64,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One run, whole.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RunRecord {
    #[serde(default)]
    pub api_version: String,
    pub run_id: String,
    pub store: String,
    pub workflow_id: String,
    /// **The pin.** Which revision of the definition this run executes,
    /// for its whole life. See `crate::revision`.
    pub definition_revision: u64,
    /// The digest of that revision's spec, so a reader can compare a run
    /// against a definition without fetching both.
    pub spec_digest: String,
    /// The revision's spec, carried WHOLE. A run executes this and never
    /// re-reads the definition, so an edit mid-flight cannot reach it.
    pub spec: WorkflowSpec,
    pub status: RunStatus,
    /// Present on every ending but `done`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default)]
    pub input: Extensions,
    pub nodes: Vec<NodeRun>,
    pub history: Vec<NodeChange>,
    pub refused: Vec<RefusedChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    pub started_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_ms: Option<u64>,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl RunRecord {
    /// One node of this run.
    #[must_use]
    pub fn node(&self, node_id: &str) -> Option<&NodeRun> {
        self.nodes.iter().find(|node| node.node_id == node_id)
    }
}

/// A run as a LIST shows it: enough to route on, without the pinned spec.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RunSummary {
    pub run_id: String,
    pub workflow_id: String,
    pub definition_revision: u64,
    pub status: RunStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// How many of this run's nodes have reached a terminal state, out of
    /// how many there are. Both counted, neither stored.
    pub nodes_ended: usize,
    pub nodes_total: usize,
    pub started_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_ms: Option<u64>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `list-runs` answer.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RunList {
    #[serde(default)]
    pub api_version: String,
    pub store: String,
    pub runs: Vec<RunSummary>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One workflow as a LIST shows it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct WorkflowSummary {
    pub workflow_id: String,
    pub name: String,
    pub latest_revision: u64,
    pub spec_digest: String,
    pub nodes: usize,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `list` answer.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct WorkflowList {
    #[serde(default)]
    pub api_version: String,
    pub store: String,
    pub workflows: Vec<WorkflowSummary>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// How a run ENDS, given its nodes — derived from what is recorded and
/// nothing else.
///
/// `None` while any node can still move: the run is not over and nothing
/// is claimed. [`RunStatus::Done`] — the claim that the procedure was
/// carried out — requires that every node that RAN reached `done`. A
/// skipped node is not a failure (an edge routed past it, which is the
/// graph working); an interrupted or failed or cancelled one is.
#[must_use]
pub fn run_ending(nodes: &[NodeRun]) -> Option<(RunStatus, Option<String>)> {
    if nodes.iter().any(|node| !node.state.is_terminal()) {
        return None;
    }
    let unfinished: Vec<&NodeRun> = nodes
        .iter()
        .filter(|node| !matches!(node.state, NodeState::Done | NodeState::Skipped))
        .collect();
    match unfinished.split_first() {
        None => Some((RunStatus::Done, None)),
        Some((first, rest)) => {
            let reason = format!(
                "node {:?} ended {}{}{}",
                first.node_id,
                first.state.tag(),
                first
                    .reason
                    .as_deref()
                    .map(|reason| format!(": {reason}"))
                    .unwrap_or_default(),
                if rest.is_empty() {
                    String::new()
                } else {
                    format!(" (and {} more did not reach done)", rest.len())
                }
            );
            Some((RunStatus::Failed, Some(reason)))
        }
    }
}

jinn_settings::closed_value_space!(RunStatus, "a run's `status`", {
    "running" => Self::Running,
    "done" => Self::Done,
    "failed" => Self::Failed,
    "cancelled" => Self::Cancelled,
    "interrupted" => Self::Interrupted,
});

jinn_settings::additive!(
    NodeRun,
    NodeChange,
    RefusedChange,
    RunRecord,
    RunSummary,
    RunList,
    WorkflowSummary,
    WorkflowList,
);

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, state: NodeState, reason: Option<&str>) -> NodeRun {
        NodeRun {
            node_id: id.to_owned(),
            state,
            reason: reason.map(str::to_owned),
            ..NodeRun::default()
        }
    }

    #[test]
    fn a_run_with_a_node_still_moving_ends_nothing() {
        assert!(run_ending(&[
            node("a", NodeState::Done, None),
            node("b", NodeState::Running, None),
        ])
        .is_none());
        assert!(run_ending(&[node("a", NodeState::Pending, None)]).is_none());
    }

    #[test]
    fn done_requires_that_every_node_that_ran_reached_done() {
        assert_eq!(
            run_ending(&[
                node("a", NodeState::Done, None),
                node("b", NodeState::Skipped, Some("routed past")),
            ]),
            Some((RunStatus::Done, None))
        );
        for bad in [
            NodeState::Failed,
            NodeState::Interrupted,
            NodeState::Cancelled,
        ] {
            let (status, reason) = run_ending(&[
                node("a", NodeState::Done, None),
                node("b", bad, Some("the reason")),
            ])
            .expect("an ending");
            assert_eq!(status, RunStatus::Failed, "{bad:?}");
            let reason = reason.expect("a failed run names why");
            assert!(reason.contains("\"b\""), "{reason}");
            assert!(reason.contains(bad.tag()), "{reason}");
            assert!(reason.contains("the reason"), "{reason}");
        }
    }

    #[test]
    fn a_failed_run_names_the_first_node_and_counts_the_rest() {
        let (_, reason) = run_ending(&[
            node("a", NodeState::Failed, Some("boom")),
            node("b", NodeState::Interrupted, Some("crash")),
        ])
        .expect("an ending");
        let reason = reason.expect("a reason");
        assert!(reason.contains("\"a\""), "{reason}");
        assert!(reason.contains("1 more"), "{reason}");
    }

    #[test]
    fn a_run_status_this_version_cannot_name_is_refused() {
        let refused = serde_json::from_value::<RunStatus>(serde_json::json!("nearly-done"))
            .expect_err("closed");
        assert!(refused.to_string().contains("nearly-done"), "{refused}");
    }

    #[test]
    fn done_is_the_only_run_ending_that_explains_itself() {
        for status in [
            RunStatus::Done,
            RunStatus::Failed,
            RunStatus::Cancelled,
            RunStatus::Interrupted,
        ] {
            assert_eq!(status.needs_reason(), status != RunStatus::Done);
            assert!(status.is_terminal());
        }
        assert!(!RunStatus::Running.is_terminal());
        assert!(!RunStatus::Running.needs_reason());
    }
}
