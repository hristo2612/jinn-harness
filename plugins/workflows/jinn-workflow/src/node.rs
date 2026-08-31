//! Where a RUN's node is, and the EXPLICIT table of the moves that are
//! legal.
//!
//! # Why a table and not a string
//!
//! A node's state is the company's claim about one step of a procedure:
//! `done` says the step was carried out, `running` says it is underway.
//! A free string lets any caller mint any claim, and "any state to any
//! state" lets a caller mint the DANGEROUS ones — a node that never ran
//! reporting `done`, a finished node quietly restarting — without anyone
//! having to notice. So the moves are enumerated, exhaustively, in one
//! place ([`allows`]), and everything not enumerated is refused NAMING
//! THE ATTEMPT ([`Refusal`]).
//!
//! This is the todos seam's status law one layer up
//! (`plugins/todos/jinn-todo/src/status.rs`), and it is written out again
//! rather than shared because the VALUE SPACES are different: a Todo
//! moves through a company's review doctrine, a node through the
//! lifecycle of one attempt. Sharing the table would have forced one
//! vocabulary to answer for both.
//!
//! # The four laws the table encodes
//!
//! - **`running` is a state a node LEAVES.** Only the live registry
//!   mints it, for a node THIS incarnation started and is driving, and
//!   `running -> interrupted` is in the table precisely so a node a
//!   crash left there can be RECORDED as ended with a reason rather than
//!   read as still working (`crate::journal`, and
//!   [`crate::Workflows::plan_recovery`]).
//! - **A terminal state is terminal.** `done`, `failed`, `interrupted`,
//!   `cancelled` and `skipped` have no exits at all. A run whose finished
//!   nodes could still change would make every past reading of it
//!   provisional; the honest way to run a step again is a NEW run.
//! - **A state change is a change.** `x -> x` is not in any row: it would
//!   append an event that records nothing happening.
//! - **A node that never started cannot claim it finished.** `pending ->
//!   done` and `pending -> failed` are NOT legal. The only endings a
//!   pending node has are the ones that say it never ran (`skipped`,
//!   `cancelled`).

use serde::Serialize;

/// Where one node of a run is. A CLOSED value space: a state this version
/// cannot name is REFUSED, never folded onto a neighbour — the neighbour
/// of `interrupted` might be `done`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NodeState {
    /// In the graph, not started. Every node opens here.
    #[default]
    Pending,
    /// The node's work is underway. Minted ONLY by the live registry.
    Running,
    /// The node's work was carried out. The dangerous claim, so it exists
    /// only where a terminal record was written.
    Done,
    /// The node ran and did not succeed. Always carries a reason.
    Failed,
    /// The node started and no ending was ever recorded. The conservative
    /// answer after a crash — always with a reason.
    Interrupted,
    /// Abandoned on the record, before or during its work.
    Cancelled,
    /// Routed past: no inbound edge to it was taken, so it never ran and
    /// never will in this run.
    Skipped,
}

impl NodeState {
    /// Whether the state is an ending. A terminal state has no legal exit
    /// ([`Self::allows`] answers empty for one), so the two facts cannot
    /// disagree.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Done | Self::Failed | Self::Interrupted | Self::Cancelled | Self::Skipped
        )
    }

    /// Every state this one may legally move TO. The whole table, in one
    /// place, exhaustive over the enum — a new state cannot be added
    /// without a row, because the match would not compile.
    #[must_use]
    pub fn allows(self) -> &'static [NodeState] {
        match self {
            // NOT `done` and NOT `failed`: a node that never started
            // cannot report how its work went.
            Self::Pending => &[Self::Running, Self::Skipped, Self::Cancelled],
            Self::Running => &[Self::Done, Self::Failed, Self::Interrupted, Self::Cancelled],
            Self::Done | Self::Failed | Self::Interrupted | Self::Cancelled | Self::Skipped => &[],
        }
    }

    /// Whether `self -> to` is a legal move.
    #[must_use]
    pub fn allows_move_to(self, to: NodeState) -> bool {
        self.allows().contains(&to)
    }

    /// The move, or the typed refusal that names it.
    ///
    /// # Errors
    ///
    /// [`Refusal`] — `self -> to` is not in the table.
    pub fn transition(self, to: NodeState) -> Result<NodeState, Refusal> {
        if self.allows_move_to(to) {
            Ok(to)
        } else {
            Err(Refusal { from: self, to })
        }
    }

    /// Whether an ending in this state MUST carry a reason. `done` is the
    /// one ending that explains itself; every other ending that carried
    /// none would leave a reader inventing one.
    #[must_use]
    pub fn needs_reason(self) -> bool {
        self.is_terminal() && self != Self::Done
    }

    /// The state as it goes on the wire — the SAME name the closed value
    /// space decodes, so a refusal message and a record cannot drift.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
            Self::Cancelled => "cancelled",
            Self::Skipped => "skipped",
        }
    }
}

/// An illegal move, refused. Carries the ATTEMPT — `from` and `to`, both
/// of them — because an operator reading a refusal needs to know which
/// move was refused, and a message that named only one half would leave
/// them guessing the other.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Refusal {
    pub from: NodeState,
    pub to: NodeState,
}

impl Refusal {
    /// The refusal in words, naming the attempt and the moves that WOULD
    /// have been legal. A terminal `from` says so rather than offering an
    /// empty list, which reads like an omission.
    #[must_use]
    pub fn message(&self) -> String {
        let attempt = format!(
            "this node cannot move {} -> {}",
            self.from.tag(),
            self.to.tag()
        );
        if self.from == self.to {
            return format!("{attempt}: a state change is a change, and this is not one");
        }
        if self.from.is_terminal() {
            return format!(
                "{attempt}: {} is terminal, and a finished node does not run again \
                 (start a new run)",
                self.from.tag()
            );
        }
        let legal: Vec<&str> = self.from.allows().iter().map(|state| state.tag()).collect();
        format!(
            "{attempt}: from {} the legal moves are {}",
            self.from.tag(),
            legal.join(" | ")
        )
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message())
    }
}

jinn_settings::closed_value_space!(NodeState, "a run node's `state`", {
    "pending" => Self::Pending,
    "running" => Self::Running,
    "done" => Self::Done,
    "failed" => Self::Failed,
    "interrupted" => Self::Interrupted,
    "cancelled" => Self::Cancelled,
    "skipped" => Self::Skipped,
});

#[cfg(test)]
mod tests {
    use super::*;

    const EVERY: [NodeState; 7] = [
        NodeState::Pending,
        NodeState::Running,
        NodeState::Done,
        NodeState::Failed,
        NodeState::Interrupted,
        NodeState::Cancelled,
        NodeState::Skipped,
    ];

    #[test]
    fn a_node_that_never_started_cannot_claim_it_finished() {
        for ending in [NodeState::Done, NodeState::Failed, NodeState::Interrupted] {
            let refused = NodeState::Pending
                .transition(ending)
                .expect_err("a pending node did not run");
            assert_eq!(refused.from, NodeState::Pending);
            assert_eq!(refused.to, ending);
            assert!(
                refused
                    .message()
                    .contains(&format!("pending -> {}", ending.tag())),
                "{refused}"
            );
        }
        // The route that IS legal.
        assert_eq!(
            NodeState::Pending
                .transition(NodeState::Running)
                .and_then(|state| state.transition(NodeState::Done)),
            Ok(NodeState::Done)
        );
    }

    #[test]
    fn a_terminal_state_has_no_exit_at_all() {
        for from in [
            NodeState::Done,
            NodeState::Failed,
            NodeState::Interrupted,
            NodeState::Cancelled,
            NodeState::Skipped,
        ] {
            assert!(from.is_terminal());
            assert!(
                from.allows().is_empty(),
                "{from:?} allows {:?}",
                from.allows()
            );
            for to in EVERY {
                let refused = from.transition(to).expect_err("terminal");
                assert!(
                    refused.message().contains("terminal") || from == to,
                    "{refused}"
                );
            }
        }
    }

    #[test]
    fn a_state_change_is_a_change() {
        for state in EVERY {
            let refused = state.transition(state).expect_err("x -> x is not a move");
            assert_eq!((refused.from, refused.to), (state, state));
        }
    }

    #[test]
    fn the_table_is_not_any_state_to_any_state() {
        let legal: usize = EVERY.iter().map(|state| state.allows().len()).sum();
        assert!(
            legal < EVERY.len() * EVERY.len() / 2,
            "a table that admits half the grid is not a table"
        );
        for (from, to) in [
            (NodeState::Pending, NodeState::Done),
            (NodeState::Skipped, NodeState::Running),
            (NodeState::Done, NodeState::Running),
            (NodeState::Interrupted, NodeState::Done),
            (NodeState::Cancelled, NodeState::Pending),
        ] {
            assert!(from.transition(to).is_err(), "{from:?} -> {to:?}");
        }
    }

    #[test]
    fn is_terminal_and_the_table_cannot_disagree() {
        for state in EVERY {
            assert_eq!(state.is_terminal(), state.allows().is_empty());
        }
    }

    #[test]
    fn done_is_the_only_ending_that_explains_itself() {
        for state in EVERY {
            assert_eq!(
                state.needs_reason(),
                state.is_terminal() && state != NodeState::Done,
                "{state:?}"
            );
        }
        assert!(!NodeState::Done.needs_reason());
        assert!(!NodeState::Running.needs_reason());
    }

    #[test]
    fn every_state_round_trips_through_its_own_tag() {
        for state in EVERY {
            let encoded = serde_json::to_value(state).expect("encodes");
            assert_eq!(encoded, serde_json::json!(state.tag()));
            let decoded: NodeState = serde_json::from_value(encoded).expect("decodes");
            assert_eq!(decoded, state);
        }
    }

    #[test]
    fn a_state_this_version_cannot_name_is_refused_not_folded() {
        let refused = serde_json::from_value::<NodeState>(serde_json::json!("nearly-done"))
            .expect_err("a closed value space refuses");
        let message = refused.to_string();
        assert!(message.contains("state"), "{message}");
        assert!(message.contains("nearly-done"), "{message}");
    }
}
