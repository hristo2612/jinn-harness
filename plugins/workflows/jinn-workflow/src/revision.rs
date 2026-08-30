//! **A run is pinned to a definition REVISION, and executes that one for
//! its whole life.**
//!
//! # The cost this module exists to stop paying
//!
//! A workflow is edited while a run of it is in flight. If the run reads
//! "the workflow" rather than "the revision it started on", then every
//! node after the edit executes a procedure the run never agreed to, the
//! nodes before it executed a different one, and nothing anywhere says
//! which. The failure mode is not a crash: it is an operator patching a
//! node's prompt, retrying the node, and watching the OLD prompt run —
//! because the run had silently pinned itself and nothing reported that.
//! That cost was paid on 2026-08-30 against the old gateway.
//!
//! So the pin is explicit, typed and READ BACK:
//!
//! - [`Definition`] is one immutable revision. `define` on an existing
//!   workflow appends revision `n + 1`; it never replaces `n`.
//! - A run records `workflow-id` AND `revision` in its `run-started`
//!   line, and carries the whole [`WorkflowSpec`] it pinned. A run
//!   therefore executes correctly even if every revision were dropped.
//! - `get-run` reports `definition-revision`. A reader never has to infer
//!   which procedure a run is executing, and "latest" is resolved exactly
//!   once, at `start`.
//!
//! # The digest, and what it is NOT
//!
//! [`digest`] is a 64-bit FNV-1a over the revision's canonical JSON. It
//! is a CHANGE DETECTOR — two revisions that differ read differently, and
//! an operator comparing a run's `spec-digest` with a definition's can
//! see at a glance whether they are the same procedure. It is not a
//! cryptographic hash and this seam never treats it as one: the packet's
//! threat model is accidental conditions, not an adversary with write
//! access to the data root forging a definition. The AUTHORITY on what a
//! run executes is the spec the run itself pinned, which is carried
//! whole; the digest is a label on it.

use serde::{Deserialize, Serialize};

use crate::{Extensions, WorkflowSpec, API_VERSION};

/// One immutable revision of a workflow.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Definition {
    #[serde(default)]
    pub api_version: String,
    pub workflow_id: String,
    /// Monotone from 1 within this workflow. A revision is never reused
    /// and never replaced.
    pub revision: u64,
    pub spec: WorkflowSpec,
    /// See the module doc — a label, not an authority.
    pub spec_digest: String,
    pub defined_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl Definition {
    /// One revision, with its digest computed from the spec it carries —
    /// so a `Definition` cannot be built holding a digest of something
    /// else.
    #[must_use]
    pub fn new(
        workflow_id: impl Into<String>,
        revision: u64,
        spec: WorkflowSpec,
        defined_ms: u64,
    ) -> Self {
        let actor = spec.attribution.actor.clone();
        let spec = spec.versioned();
        Self {
            api_version: API_VERSION.to_owned(),
            workflow_id: workflow_id.into(),
            revision,
            spec_digest: digest(&spec),
            spec,
            defined_ms,
            actor,
            extra: Extensions::new(),
        }
    }

    /// Whether this revision's carried spec still matches its own digest.
    /// A revision that fails this is REPORTED, never quietly used: the
    /// spec is the authority, and a disagreement means one of the two was
    /// written by something that is not this seam.
    #[must_use]
    pub fn digest_matches(&self) -> bool {
        self.spec_digest == digest(&self.spec)
    }
}

/// The whole recorded history of one workflow: every revision, in order.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct WorkflowRecord {
    #[serde(default)]
    pub api_version: String,
    pub workflow_id: String,
    pub store: String,
    /// The highest revision recorded — what an absent `revision` resolves
    /// to, once, at `start`.
    pub latest_revision: u64,
    pub revisions: Vec<Definition>,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl WorkflowRecord {
    /// One revision by number, or the latest when `revision` is absent.
    #[must_use]
    pub fn revision(&self, revision: Option<u64>) -> Option<&Definition> {
        match revision {
            Some(wanted) => self.revisions.iter().find(|rev| rev.revision == wanted),
            None => self.revisions.iter().max_by_key(|rev| rev.revision),
        }
    }
}

/// A revision's digest: 64-bit FNV-1a over the spec's canonical JSON,
/// rendered `fnv1a64:<16 hex>`. See the module doc for what this is and
/// is not.
///
/// # Panics
///
/// Never in practice: the seam's own types all encode.
#[must_use]
pub fn digest(spec: &WorkflowSpec) -> String {
    // `serde_json::Value`'s map is ordered (the crate is built with
    // `preserve_order` off in this workspace's lockfile), so encoding a
    // value renders keys in a stable order and the same spec always
    // digests the same. The digest is a label either way — a reordering
    // that changed it would compare unequal and be reported as a
    // difference, never silently accepted as a match.
    let canonical = serde_json::to_vec(&serde_json::to_value(spec).expect("a spec encodes"))
        .expect("a value encodes");
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in canonical {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("fnv1a64:{hash:016x}")
}

jinn_settings::additive!(Definition, WorkflowRecord);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeSpec, WorkflowSpec};

    fn spec(name: &str) -> WorkflowSpec {
        WorkflowSpec {
            name: name.to_owned(),
            nodes: vec![NodeSpec {
                id: "a".to_owned(),
                ..NodeSpec::default()
            }],
            ..WorkflowSpec::default()
        }
    }

    #[test]
    fn a_revision_carries_the_digest_of_the_spec_it_holds() {
        let definition = Definition::new("w-1", 1, spec("first"), 10);
        assert!(definition.digest_matches());
        // A digest that does not match its spec is DETECTED rather than
        // trusted.
        let forged = Definition {
            spec_digest: "fnv1a64:0000000000000000".to_owned(),
            ..definition.clone()
        };
        assert!(!forged.digest_matches());
    }

    #[test]
    fn two_specs_that_differ_digest_differently_and_the_same_one_is_stable() {
        let first = digest(&spec("first"));
        assert_eq!(first, digest(&spec("first")));
        assert_ne!(first, digest(&spec("second")));
        assert!(first.starts_with("fnv1a64:"), "{first}");
    }

    #[test]
    fn a_revision_is_never_replaced_and_the_latest_is_resolved_by_number() {
        let record = WorkflowRecord {
            workflow_id: "w-1".to_owned(),
            latest_revision: 3,
            revisions: vec![
                Definition::new("w-1", 1, spec("first"), 10),
                Definition::new("w-1", 2, spec("second"), 20),
                Definition::new("w-1", 3, spec("third"), 30),
            ],
            ..WorkflowRecord::default()
        };
        assert_eq!(record.revision(Some(1)).expect("rev 1").spec.name, "first");
        assert_eq!(record.revision(None).expect("latest").revision, 3);
        assert!(record.revision(Some(9)).is_none());
        // Revision 1 is still exactly what it was after 2 and 3 landed.
        assert_eq!(
            record.revision(Some(1)).expect("rev 1").spec_digest,
            digest(&spec("first").versioned())
        );
    }
}
