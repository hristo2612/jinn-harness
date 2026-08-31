//! The legal-transition table for the lifecycle READING, and the moves a
//! reading admits.
//!
//! Todo status and workflow node state each carry an explicit table
//! because they are states this distribution MOVES. A lifecycle reading
//! is not moved by anyone here — the kernel moves the fiber and we read
//! it — so this table says something narrower and more useful: which
//! reading may legally FOLLOW which, for one entry, so an operator or an
//! agent knows what can happen next and a reader can say when the kernel
//! told it something the table does not admit.
//!
//! The table is written from the kernel's own fiber law
//! (`Pending → Loading → Active | Failed`, `Active → Unloading →
//! Disposed | Pending`), so it is the fiber machine seen through the
//! reading vocabulary — never a second, parallel state machine.

use crate::lifecycle::Lifecycle;

/// Every reading's name, in one place, so the table below is total.
pub const NAMES: [&str; 11] = [
    "not-mounted",
    "mounted",
    "no-incarnation",
    "activating",
    "active",
    "restarting",
    "suspended",
    "failed",
    "interrupted",
    "disposed",
    "unrecognised",
];

/// The legal successors of each reading, for ONE entry observed twice.
///
/// `unrecognised` is a successor of everything and admits everything: a
/// state this table does not know cannot constrain what follows it, and
/// pretending otherwise would turn an honest "the kernel said a word I do
/// not know" into a false claim about the machine.
#[must_use]
pub fn legal_next(from: &str) -> &'static [&'static str] {
    match from {
        // An entry the catalog names but the composition does not report
        // may be mounted at any moment, or stay absent.
        "not-mounted" => &[
            "not-mounted",
            "mounted",
            "no-incarnation",
            "activating",
            "unrecognised",
        ],
        "mounted" => &[
            "mounted",
            "activating",
            "no-incarnation",
            "interrupted",
            "failed",
            "disposed",
            "not-mounted",
            "unrecognised",
        ],
        // No live incarnation: the entry can be re-mounted, removed, or
        // stay as it is. It cannot become `active` without loading first.
        "no-incarnation" => &[
            "no-incarnation",
            "mounted",
            "activating",
            "not-mounted",
            "unrecognised",
        ],
        "activating" => &[
            "activating",
            "active",
            "failed",
            "restarting",
            "suspended",
            "interrupted",
            "no-incarnation",
            "unrecognised",
        ],
        "active" => &[
            "active",
            "restarting",
            "suspended",
            "interrupted",
            "failed",
            "disposed",
            "no-incarnation",
            "unrecognised",
        ],
        "restarting" => &[
            "restarting",
            "activating",
            "active",
            "failed",
            "interrupted",
            "disposed",
            "no-incarnation",
            "unrecognised",
        ],
        "suspended" => &[
            "suspended",
            "activating",
            "active",
            "interrupted",
            "disposed",
            "no-incarnation",
            "unrecognised",
        ],
        // A failure is not retried against an unchanged environment (R9),
        // so `active` is never the NEXT reading after `failed` — the
        // environment must move, and that shows as a fresh activation.
        "failed" => &[
            "failed",
            "activating",
            "no-incarnation",
            "not-mounted",
            "disposed",
            "unrecognised",
        ],
        "interrupted" => &[
            "interrupted",
            "disposed",
            "no-incarnation",
            "activating",
            "not-mounted",
            "failed",
            "unrecognised",
        ],
        // Disposal is terminal for the incarnation; the entry itself may
        // leave the document or be mounted afresh.
        "disposed" => &[
            "disposed",
            "not-mounted",
            "no-incarnation",
            "activating",
            "unrecognised",
        ],
        "unrecognised" => &NAMES,
        _ => &[],
    }
}

/// Whether `to` may legally follow `from` for one entry.
#[must_use]
pub fn may_follow(from: &str, to: &str) -> bool {
    legal_next(from).contains(&to)
}

impl Lifecycle {
    /// The readings that may legally follow this one — what `describe`
    /// answers, so an operator or an agent reads the possible next moves
    /// instead of inferring them.
    #[must_use]
    pub fn legal_next(&self) -> &'static [&'static str] {
        legal_next(self.name())
    }
}
