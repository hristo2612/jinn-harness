//! Where a Todo IS, and the EXPLICIT table of the moves that are legal.
//!
//! # Why a table and not a string
//!
//! A Todo's status is the company's claim about a piece of work: `done`
//! says it is finished, `executing` says it is underway. A free string
//! lets any caller mint any claim, and "any status to any status" lets a
//! caller mint the DANGEROUS ones — a producer closing their own work, a
//! cancelled Todo quietly reopening — without anyone having to notice.
//! So the moves are enumerated, exhaustively, in one place ([`allows`]),
//! and everything not enumerated is refused NAMING THE ATTEMPT
//! ([`Refusal`]).
//!
//! # The three laws the table encodes
//!
//! - **A producer does not close their own work.** `executing → done` is
//!   NOT legal; the route is `executing → in-review → done`, which is the
//!   company's own doctrine (a reviewer closes, never the producer).
//! - **A terminal status is terminal.** `done` and `cancelled` have no
//!   exits at all. A closed Todo whose history could still change would
//!   make every past reading of it provisional; the honest way back is a
//!   NEW Todo that links to it.
//! - **A status change is a change.** `x → x` is not in any row: it would
//!   append an event that records nothing happening, and an append-only
//!   history whose lines can mean nothing is a history you have to read
//!   twice.

use serde::Serialize;

/// Where a Todo is. A CLOSED value space: a status this version cannot
/// name is REFUSED, never folded onto a neighbour — the neighbour of
/// `blocked` might be `done`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// Recorded, not started. Every Todo opens here.
    #[default]
    Backlog,
    /// Work is underway.
    Executing,
    /// The producer says it is finished; a reviewer has not agreed yet.
    InReview,
    /// Open, and something outside it must move first.
    Blocked,
    /// A reviewer closed it. Terminal.
    Done,
    /// Abandoned, on the record. Terminal.
    Cancelled,
}

impl Status {
    /// Whether the status is an ending. A terminal status has no legal
    /// exit ([`Self::allows`] answers empty for one), so the two facts
    /// cannot disagree.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Cancelled)
    }

    /// Every status this one may legally move TO. The whole table, in one
    /// place, exhaustive over the enum — a new status cannot be added
    /// without a row, because the match would not compile.
    #[must_use]
    pub fn allows(self) -> &'static [Status] {
        match self {
            Self::Backlog => &[Self::Executing, Self::Blocked, Self::Cancelled],
            // NOT `done`: a producer does not close their own work.
            Self::Executing => &[Self::InReview, Self::Blocked, Self::Cancelled],
            // Back to `executing` is rework — the reviewer sending it
            // back, which is a real move and stays legal.
            Self::InReview => &[Self::Done, Self::Executing, Self::Blocked, Self::Cancelled],
            Self::Blocked => &[Self::Executing, Self::Backlog, Self::Cancelled],
            Self::Done | Self::Cancelled => &[],
        }
    }

    /// Whether `self → to` is a legal move.
    #[must_use]
    pub fn allows_move_to(self, to: Status) -> bool {
        self.allows().contains(&to)
    }

    /// The move, or the typed refusal that names it.
    ///
    /// # Errors
    ///
    /// [`Refusal`] — `self → to` is not in the table.
    pub fn transition(self, to: Status) -> Result<Status, Refusal> {
        if self.allows_move_to(to) {
            Ok(to)
        } else {
            Err(Refusal { from: self, to })
        }
    }

    /// The status as it goes on the wire — the SAME name the closed value
    /// space decodes, so a refusal message and a record cannot drift.
    ///
    /// # Panics
    ///
    /// Never: the enum's own encoding is a string.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::Backlog => "backlog",
            Self::Executing => "executing",
            Self::InReview => "in-review",
            Self::Blocked => "blocked",
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }
}

/// An illegal move, refused. Carries the ATTEMPT — `from` and `to`, both
/// of them — because an operator reading a refusal needs to know which
/// move was refused, and a message that named only one half would leave
/// them guessing the other.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Refusal {
    pub from: Status,
    pub to: Status,
}

impl Refusal {
    /// The refusal in words, naming the attempt and the moves that WOULD
    /// have been legal. A terminal `from` says so rather than offering an
    /// empty list, which reads like an omission.
    #[must_use]
    pub fn message(&self) -> String {
        let attempt = format!(
            "this Todo cannot move {} -> {}",
            self.from.tag(),
            self.to.tag()
        );
        if self.from == self.to {
            return format!("{attempt}: a status change is a change, and this is not one");
        }
        if self.from.is_terminal() {
            return format!(
                "{attempt}: {} is terminal, and a closed Todo's history does not reopen \
                 (record a new Todo linked to this one)",
                self.from.tag()
            );
        }
        let legal: Vec<&str> = self
            .from
            .allows()
            .iter()
            .map(|status| status.tag())
            .collect();
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

jinn_settings::closed_value_space!(Status, "a Todo's `status`", {
    "backlog" => Self::Backlog,
    "executing" => Self::Executing,
    "in-review" => Self::InReview,
    "blocked" => Self::Blocked,
    "done" => Self::Done,
    "cancelled" => Self::Cancelled,
});

#[cfg(test)]
mod tests {
    use super::*;

    const EVERY: [Status; 6] = [
        Status::Backlog,
        Status::Executing,
        Status::InReview,
        Status::Blocked,
        Status::Done,
        Status::Cancelled,
    ];

    #[test]
    fn a_producer_cannot_close_their_own_work() {
        let refused = Status::Executing
            .transition(Status::Done)
            .expect_err("executing -> done is not the company's route to done");
        assert_eq!(refused.from, Status::Executing);
        assert_eq!(refused.to, Status::Done);
        // The message names the ATTEMPT, both halves.
        assert!(
            refused.message().contains("executing -> done"),
            "{}",
            refused
        );
        // And the route that IS legal.
        assert_eq!(
            Status::Executing
                .transition(Status::InReview)
                .and_then(|status| status.transition(Status::Done)),
            Ok(Status::Done)
        );
    }

    #[test]
    fn a_terminal_status_has_no_exit_at_all() {
        for from in [Status::Done, Status::Cancelled] {
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
    fn a_status_change_is_a_change() {
        for status in EVERY {
            let refused = status.transition(status).expect_err("x -> x is not a move");
            assert_eq!((refused.from, refused.to), (status, status));
        }
    }

    #[test]
    fn the_table_is_not_any_status_to_any_status() {
        // The point of a table: most moves are NOT legal.
        let legal: usize = EVERY.iter().map(|status| status.allows().len()).sum();
        assert!(
            legal < EVERY.len() * EVERY.len() / 2,
            "a table that admits half the grid is not a table"
        );
        // Spot-checks of moves that must never be legal.
        for (from, to) in [
            (Status::Backlog, Status::Done),
            (Status::Backlog, Status::InReview),
            (Status::Blocked, Status::Done),
            (Status::Done, Status::Executing),
            (Status::Cancelled, Status::Backlog),
        ] {
            assert!(from.transition(to).is_err(), "{from:?} -> {to:?}");
        }
    }

    #[test]
    fn is_terminal_and_the_table_cannot_disagree() {
        for status in EVERY {
            assert_eq!(status.is_terminal(), status.allows().is_empty());
        }
    }

    #[test]
    fn every_status_round_trips_through_its_own_tag() {
        for status in EVERY {
            let encoded = serde_json::to_value(status).expect("encodes");
            assert_eq!(encoded, serde_json::json!(status.tag()));
            let decoded: Status = serde_json::from_value(encoded).expect("decodes");
            assert_eq!(decoded, status);
        }
    }

    #[test]
    fn a_status_this_version_cannot_name_is_refused_not_folded() {
        let refused = serde_json::from_value::<Status>(serde_json::json!("nearly-done"))
            .expect_err("a closed value space refuses");
        let message = refused.to_string();
        assert!(message.contains("status"), "{message}");
        assert!(message.contains("nearly-done"), "{message}");
    }
}
