//! The LIFECYCLE READING law: what a catalog is allowed to say about one
//! plugin's life, and what evidence each answer requires.
//!
//! # A reading, not a state machine
//!
//! This seam does not RUN plugins — the kernel does. Every value here is
//! a READING of kernel-owned evidence: the `jinn:introspect` composition
//! snapshot (`state`, `incarnation`, `unserved`), the `jinn:profile`
//! document of record (`disabled`), and the `jinn:ledger` lines
//! attributed to the entry. The reading law below says which evidence
//! licenses which answer, and it is written so that the DANGEROUS answer
//! is the one that needs positive proof.
//!
//! # The dangerous answer is [`Lifecycle::Active`]
//!
//! "This plugin is serving" is the claim an operator acts on. It is
//! therefore the only answer requiring three positive facts at once — the
//! kernel said `active`, an incarnation is installed, and the live
//! incarnation owes NOTHING (`unserved` absent). Every other combination
//! falls to a more conservative answer BY CONSTRUCTION, because the match
//! in [`Lifecycle::read`] has no arm that reaches `Active` any other way.
//! Mounted-but-never-activated is [`Lifecycle::Mounted`]; a fiber that is
//! loading while its incarnation already owes a change nothing will
//! schedule is [`Lifecycle::Interrupted`], never an eternal
//! [`Lifecycle::Activating`].
//!
//! # There is no `unknown`
//!
//! A kernel `state` string this table does not know is answered
//! [`Lifecycle::Unrecognised`] carrying the string VERBATIM — a positive
//! statement about what the kernel said, not a sentinel standing in for a
//! reading that never happened. `jinn:introspect` may add a state under
//! R12 additivity; a catalog that met one and answered `active`, or
//! `unknown`, would be lying in two different directions.

use serde::{Deserialize, Serialize};

/// What an entry's LIVE incarnation already owes, as `jinn:introspect`
/// 0.2.0 reports it. Absent for an entry that can serve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Unserved {
    /// A replacement is scheduled: retry once the restart lands.
    Restarting,
    /// Disposal is owed, and disposal is terminal: never retry.
    Gone,
    /// Suspension is owed: retry after a resume, which may never come.
    Suspended,
    /// A change nothing will schedule from here.
    Stalled,
}

impl Unserved {
    /// The wire spelling `jinn:introspect` uses, or `None` for anything
    /// else. Fail-closed: an unrecognised word is not silently dropped
    /// into a variant, it is not an `Unserved` at all.
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "restarting" => Some(Self::Restarting),
            "gone" => Some(Self::Gone),
            "suspended" => Some(Self::Suspended),
            "stalled" => Some(Self::Stalled),
            _ => None,
        }
    }
}

/// The ledger span a catalog actually consulted while answering. It
/// travels with every answer, because every reason of kind
/// [`Reason::NotFoundInWindow`] is only as strong as the window that was
/// searched (the M2-K12 shape: a limit travels with the evidence it
/// qualifies).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Window {
    /// The lowest ledger sequence consulted.
    pub from: u64,
    /// The highest ledger sequence consulted (`from` when nothing was).
    pub to: u64,
    /// How many lines were read.
    pub scanned: u32,
    /// Whether the read stopped at its cap with lines still unread — so
    /// a `not-found-in-window` under a truncated window means LESS.
    pub truncated: bool,
}

/// Why a plugin is in the state a catalog reports. Every variant names
/// the evidence it came from; there is deliberately no `Unknown`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "from", rename_all = "kebab-case")]
pub enum Reason {
    /// The kernel's own composition snapshot said this and nothing more.
    Composition { unserved: Unserved },
    /// The document of record says this entry is disabled.
    Disabled,
    /// A ledger line said it: its sequence, its kind, and its detail.
    Ledgered {
        seq: u64,
        kind: String,
        detail: String,
    },
    /// Searched, positively, and not there — with the window searched.
    /// This is a statement about a read that HAPPENED, which is why it
    /// is admissible where a sentinel is not.
    NotFoundInWindow { window: Window },
}

/// One plugin's life as a catalog reports it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum Lifecycle {
    /// Named by the catalog and absent from the kernel's composition.
    NotMounted,
    /// In the composition with a fiber that has never left `pending`.
    /// This is the mounted-but-never-activated answer, and it is not
    /// `Active` for the structural reason the module doc gives.
    Mounted,
    /// In the composition with NO live fiber at all. The kernel does not
    /// say why in the snapshot, so the reason comes from the document or
    /// the ledger — and says which.
    NoIncarnation { reason: Reason },
    /// An incarnation is installing, and it owes nothing.
    Activating,
    /// Serving. The only answer requiring three positive facts.
    Active,
    /// The live incarnation owes a replacement.
    Restarting,
    /// The live incarnation owes, or has taken, a suspension.
    Suspended,
    /// The activation failed, with the reason it failed.
    Failed { reason: Reason },
    /// Torn down, or owing a change nothing will schedule, mid-life.
    Interrupted { reason: Reason },
    /// Gone, terminally.
    Disposed { reason: Reason },
    /// The kernel reported a state this table does not know. The word is
    /// carried verbatim rather than folded into a neighbour.
    Unrecognised { kernel_state: String },
}

/// The `jinn:introspect` snapshot for one entry, as this seam reads it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// `entry.state` — absent for an entry with no live fiber.
    pub state: Option<String>,
    /// `entry.incarnation` — absent with no installed incarnation.
    pub incarnation: Option<u64>,
    /// `entry.unserved` — what the live incarnation already owes.
    pub unserved: Option<Unserved>,
    /// `entry.provisions` — the contracts this entry currently serves.
    /// The BINDING half of a swap: which package answers a contract name
    /// is read here, never inferred from a package name.
    pub provisions: Vec<String>,
}

impl Snapshot {
    /// The `jinn:introspect` `entries` answer, by entry id. An element
    /// without a string `id` is not an entry and is dropped rather than
    /// given a made-up one.
    #[must_use]
    pub fn parse_entries(value: &serde_json::Value) -> std::collections::BTreeMap<String, Self> {
        let mut parsed = std::collections::BTreeMap::new();
        let Some(entries) = value.as_array() else {
            return parsed;
        };
        for entry in entries {
            let Some(id) = entry.get("id").and_then(serde_json::Value::as_str) else {
                continue;
            };
            parsed.insert(
                id.to_owned(),
                Self {
                    state: entry
                        .get("state")
                        .and_then(serde_json::Value::as_str)
                        .map(ToOwned::to_owned),
                    incarnation: entry.get("incarnation").and_then(serde_json::Value::as_u64),
                    unserved: entry
                        .get("unserved")
                        .and_then(serde_json::Value::as_str)
                        .and_then(Unserved::parse),
                    provisions: entry
                        .get("provisions")
                        .and_then(serde_json::Value::as_array)
                        .map(|list| {
                            list.iter()
                                .filter_map(|item| item.as_str().map(ToOwned::to_owned))
                                .collect()
                        })
                        .unwrap_or_default(),
                },
            );
        }
        parsed
    }
}

impl Lifecycle {
    /// The reading law. `snapshot` is `None` for an entry the catalog
    /// names that the composition does not report at all.
    ///
    /// `no_fiber` is the reason to give when the kernel reports the entry
    /// with no live fiber, and `failure` the reason to give when it
    /// reports `failed`: both are resolved by the caller from the
    /// document and the ledger, and both are already positive readings —
    /// a [`Reason::NotFoundInWindow`] at worst, never a sentinel.
    #[must_use]
    pub fn read(snapshot: Option<&Snapshot>, no_fiber: Reason, failure: Reason) -> Self {
        let Some(snapshot) = snapshot else {
            return Self::NotMounted;
        };
        let Some(state) = snapshot.state.as_deref() else {
            return Self::NoIncarnation { reason: no_fiber };
        };
        match state {
            "active" => match (snapshot.incarnation, snapshot.unserved) {
                // The only path to `Active`: the kernel said active, an
                // incarnation is installed, and nothing is owed.
                (Some(_), None) => Self::Active,
                (None, None) => Self::NoIncarnation { reason: no_fiber },
                (_, Some(owed)) => Self::owed(owed),
            },
            "pending" => match snapshot.unserved {
                None => Self::Mounted,
                Some(owed) => Self::owed(owed),
            },
            "loading" => match snapshot.unserved {
                // Activating is admissible only while NOTHING is owed;
                // an incarnation that already owes a change it will not
                // get is interrupted, not eternally activating.
                None => Self::Activating,
                Some(owed) => Self::owed(owed),
            },
            "failed" => Self::Failed { reason: failure },
            "unloading" => Self::Interrupted {
                reason: match snapshot.unserved {
                    Some(owed) => Reason::Composition { unserved: owed },
                    None => no_fiber,
                },
            },
            "disposed" => Self::Disposed {
                reason: match snapshot.unserved {
                    Some(owed) => Reason::Composition { unserved: owed },
                    None => no_fiber,
                },
            },
            other => Self::Unrecognised {
                kernel_state: other.to_owned(),
            },
        }
    }

    /// What an owed change reads as, wherever one is owed. `restarting`
    /// and `suspended` are their own answers because they are different
    /// next moves; `gone` and `stalled` are interruptions, and each keeps
    /// the word the kernel used as its reason.
    fn owed(owed: Unserved) -> Self {
        match owed {
            Unserved::Restarting => Self::Restarting,
            Unserved::Suspended => Self::Suspended,
            Unserved::Gone | Unserved::Stalled => Self::Interrupted {
                reason: Reason::Composition { unserved: owed },
            },
        }
    }

    /// The kebab-case name of this reading, without its payload — the
    /// vocabulary the transition table and the API answer are written in.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::NotMounted => "not-mounted",
            Self::Mounted => "mounted",
            Self::NoIncarnation { .. } => "no-incarnation",
            Self::Activating => "activating",
            Self::Active => "active",
            Self::Restarting => "restarting",
            Self::Suspended => "suspended",
            Self::Failed { .. } => "failed",
            Self::Interrupted { .. } => "interrupted",
            Self::Disposed { .. } => "disposed",
            Self::Unrecognised { .. } => "unrecognised",
        }
    }

    /// The reason behind this reading, where the reading has one.
    #[must_use]
    pub fn reason(&self) -> Option<&Reason> {
        match self {
            Self::NoIncarnation { reason }
            | Self::Failed { reason }
            | Self::Interrupted { reason }
            | Self::Disposed { reason } => Some(reason),
            _ => None,
        }
    }

    /// Whether this reading claims the plugin is SERVING. Exactly one
    /// variant may answer true; the rest are conservative by
    /// construction.
    #[must_use]
    pub fn is_serving(&self) -> bool {
        matches!(self, Self::Active)
    }
}
