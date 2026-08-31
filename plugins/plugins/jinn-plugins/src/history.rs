//! Attribution: one plugin's ledger lines, and ONLY its own.
//!
//! The kernel fills a ledger row's `entry` column from the fiber the
//! append was charged to (or from an entry passed explicitly). A row with
//! no entry is charged to no plugin — kernel bookkeeping, or a facade
//! effect — and it belongs in NO plugin's history. So the filter is
//! equality on a present entry, never a fallback, and never "everything
//! that is not obviously someone else's".
//!
//! # A disposed plugin's history survives its disposal
//!
//! Nothing here reads the composition. A history is a read of the ledger,
//! which is append-only, so an entry that has left the document still has
//! every line it ever wrote. That is a property of asking the ledger
//! rather than asking the fiber, and it is why `history` is not answered
//! out of `describe`'s snapshot.

use serde::{Deserialize, Serialize};

use crate::lifecycle::Window;
use crate::{Extensions, API_VERSION};

/// The ledger kinds that carry a REASON a lifecycle reading may cite.
/// A known-set with an honest fallthrough, never an exhaustive match:
/// the kernel's kind list grows (R12), and a kind this build does not
/// know must read as history rather than break the reader.
pub const REASON_BEARING: [&str; 4] = [
    "ErrorRecorded",
    "GrantRefused",
    "ArtifactRefused",
    "AmendmentRefused",
];

/// One ledger line as this seam reports it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Line {
    pub seq: u64,
    pub wall_ms: u64,
    /// The entry the kernel charged this line to. Always `Some` in a
    /// history, by construction of the filter.
    pub entry: String,
    pub kind: String,
    pub payload: serde_json::Value,
    pub sensitivity: String,
}

/// The `history` answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct History {
    pub api_version: String,
    pub plugin: String,
    pub lines: Vec<Line>,
    /// The span actually read. A history is only ever "this plugin's
    /// lines WITHIN THIS WINDOW", and the window says so on the wire.
    pub window: Window,
    /// How far this answer's word goes.
    pub qualifier: String,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The qualifier a history carries. Its one home.
pub const HISTORY_QUALIFIER: &str =
    "these are the lines the ledger charged to this entry WITHIN `window`; \
     a line the kernel charged to no entry belongs to no plugin and is never \
     included, and lines outside the window were not read";

impl History {
    /// One plugin's lines out of a ledger page. `rows` is the page as the
    /// kernel gave it: `(seq, wall-ms, entry, kind, payload, sensitivity)`
    /// with `entry` absent where the kernel charged the line to nobody.
    #[must_use]
    pub fn of(plugin: &str, rows: Vec<Line>, window: Window) -> Self {
        Self {
            api_version: API_VERSION.to_owned(),
            plugin: plugin.to_owned(),
            // Equality on a present entry. An absent entry is nobody's.
            lines: rows.into_iter().filter(|row| row.entry == plugin).collect(),
            window,
            qualifier: HISTORY_QUALIFIER.to_owned(),
            extra: Extensions::new(),
        }
    }

    /// The last reason-bearing line for this plugin, if the window holds
    /// one: the newest line whose kind is in [`REASON_BEARING`].
    #[must_use]
    pub fn last_reason(&self) -> Option<&Line> {
        self.lines
            .iter()
            .rev()
            .find(|line| REASON_BEARING.contains(&line.kind.as_str()))
    }
}
