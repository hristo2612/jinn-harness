//! The plugins seam's SERVICE DEFINITION: the `jinn:plugins.<catalog>`
//! contract's vocabulary, the LIFECYCLE READING law
//! ([`lifecycle`]) and its legal-transition table ([`transition`]), the
//! grant reading and its source ([`entry`]), and the ledger attribution
//! rule ([`history`]). Pure types + logic, no host calls.
//!
//! # What this seam is for
//!
//! The North Star sentence is a kernel that makes a machine *legible,
//! reversible, and safe for an agent to operate and reshape*. Every other
//! seam proves a provider is swappable in a TEST. This one makes the
//! plugin tree legible and operable through the surface a person or an
//! agent actually uses — which is what makes the swap something an
//! operator can do rather than something a suite asserts.
//!
//! # The catalog is the swappable part
//!
//! A catalog answers WHICH plugins there are and what each declares. Two
//! providers answer it differently — `jinn-plugins-profile` derives it
//! from the live document of record, `jinn-plugins-static` serves a fixed
//! one for tests and for a read-only appliance — and NEITHER knows about
//! todos, sessions, engines or workflows. The lifecycle, the provisions
//! and the history are read from the kernel by both, identically,
//! through this definition.
//!
//! # The contract name carries the catalog id
//!
//! `jinn:plugins.<catalog>`, so several catalogs are live at once and an
//! operator addresses the one they mean. It is also what makes a provider
//! swap expressible AS A CONFIG EDIT: which package answers
//! `jinn:plugins.main` is decided by the `catalog` field of each
//! provider's own config, and config is the one subtree
//! `jinn:profile.patch-entry` may write. See `FINDINGS.md` #37.

pub mod catalog;
pub mod entry;
pub mod history;
pub mod lifecycle;
pub mod transition;

#[cfg(test)]
mod tests;

pub use catalog::{Catalog, Source};
pub use entry::{Entry, Grant, GrantSource, Grants, Listing, ReadWindow, JOIN_QUALIFIER};
pub use history::{History, Line};
pub use lifecycle::{Lifecycle, Reason, Snapshot, Unserved, Window};
pub use transition::{legal_next, may_follow};

use serde::{Deserialize, Serialize};

/// Unknown sibling fields, preserved across a decode → encode round trip
/// (R12 additivity: this reader carries what a newer writer said).
pub type Extensions = serde_json::Map<String, serde_json::Value>;

/// This seam's contract version. Additive only.
pub const API_VERSION: &str = "0.1.0";

/// The `jinn:plugins.<catalog-id>` contract prefix.
pub const PLUGINS_CONTRACT_PREFIX: &str = "jinn:plugins.";

/// The settings namespace this definition owns (AGENTS.md standing order
/// 4: a service definition owns its namespace).
pub const SETTINGS_NAMESPACE: &str = "plugins";

/// Operation: every plugin this catalog holds.
pub const OP_LIST: &str = "list";
/// Operation: one plugin, its declared effects and what it has done.
pub const OP_DESCRIBE: &str = "describe";
/// Operation: one plugin's ledger lines, and only its own.
pub const OP_HISTORY: &str = "history";
/// Operation: the catalog's own word about itself.
pub const OP_DESCRIBE_CATALOG: &str = "describe-catalog";

/// The `jinn:plugins.<id>` contract name for a catalog id.
#[must_use]
pub fn catalog_contract(id: &str) -> String {
    format!("{PLUGINS_CONTRACT_PREFIX}{id}")
}

/// The catalog id a contract name carries, or `None` when it is not this
/// seam's. An empty id is not a catalog.
#[must_use]
pub fn catalog_id_of(contract: &str) -> Option<&str> {
    contract
        .strip_prefix(PLUGINS_CONTRACT_PREFIX)
        .filter(|id| !id.is_empty())
}

/// Why a catalog call was refused. Callers classify by CASE, never by
/// folding a message. A CLOSED value space.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    /// Malformed request, or a value this seam will not answer.
    Invalid,
    /// No such plugin in this catalog.
    NotFound,
    /// The kernel refused a grant this answer needs.
    Refused,
    /// The catalog is mounted and correct; this host cannot answer —
    /// a read it depends on is not reachable from this entry. NEVER a
    /// stand-in for a reading that happened and came back empty.
    Unavailable,
    /// The catalog tried and it failed.
    Failed,
}

/// A typed refusal.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct PluginsError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl PluginsError {
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            extra: Extensions::new(),
        }
    }

    /// The refusal a catalog owes when a read it depends on refused. The
    /// contract that refused rides as DATA so a caller never parses
    /// prose to learn WHICH authority is missing — and so this can never
    /// be mistaken for "the thing has no grants" or "the thing is not
    /// active".
    #[must_use]
    pub fn unreadable(contract: &str, detail: impl std::fmt::Display) -> Self {
        let mut error = Self::new(
            ErrorCode::Unavailable,
            format!("{contract} is not readable from this entry: {detail}"),
        );
        error
            .extra
            .insert("contract".to_owned(), serde_json::json!(contract));
        error
    }
}

/// One answer on the wire: the value, or the typed refusal.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Ok(serde_json::Value),
    Error(PluginsError),
}

/// The envelope every operation answers in.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Answer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
    #[serde(flatten)]
    pub outcome: Outcome,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl Answer {
    #[must_use]
    pub fn ok(value: serde_json::Value) -> Self {
        Self::versioned(Outcome::Ok(value))
    }

    #[must_use]
    pub fn error(error: PluginsError) -> Self {
        Self::versioned(Outcome::Error(error))
    }

    fn versioned(outcome: Outcome) -> Self {
        Self {
            api_version: Some(API_VERSION.to_owned()),
            outcome,
            extra: Extensions::new(),
        }
    }

    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("an answer encodes")
    }

    /// Decodes one broker answer; a malformed one is a typed `failed`,
    /// never a silently empty success.
    #[must_use]
    pub fn decode(bytes: &[u8]) -> Self {
        serde_json::from_slice(bytes).unwrap_or_else(|error| {
            Self::error(PluginsError::new(
                ErrorCode::Failed,
                format!("malformed catalog answer: {error}"),
            ))
        })
    }
}
