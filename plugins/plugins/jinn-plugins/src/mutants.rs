//! The MUTATION HARNESS: inject each named defect, show exactly which
//! honesty checks go red.
//!
//! # Why this exists, and what it stands in for
//!
//! Red-first ordering shows that a test COULD fail. It was missed on this
//! seam's round 1 and cannot be un-missed, so what stands in its place is
//! strictly stronger evidence — and it is an ARTIFACT that runs, not a
//! sentence in a note. Each mutant below is a defective IMPLEMENTATION of
//! the reading, reading the same [`Inputs`] the honest one reads: the
//! round-1 defect is the deleted code restored verbatim, over a real
//! ledger page. Each is then measured with the same [`crate::checks`]
//! predicates the real composition proof runs against the daemon's own
//! answers.
//!
//! A mutant no check catches is a hole in the suite; a check no mutant
//! reaches is unproven law. The sweep at the bottom fails on either.

use crate::catalog::Declared;
use crate::entry::GrantSource;
use crate::history::{History, Line, REASON_BEARING};
use crate::lifecycle::{Snapshot, Window};
use crate::Catalog;

/// What every reading — honest or defective — is computed from.
pub struct Inputs {
    pub declared: Declared,
    pub snapshot: Option<Snapshot>,
    pub history: History,
    pub window: Window,
}

/// One named defect, and the check it must be caught by.
pub struct Mutant {
    /// The defect, named as an implementer would describe it.
    pub name: &'static str,
    /// Where it came from: a round of this packet, or a class the seam
    /// met on an earlier layer.
    pub provenance: &'static str,
    /// The check that must go red on it.
    pub caught_by: &'static str,
    /// The defective reading, over the same inputs the honest one reads.
    pub read: fn(&Inputs) -> serde_json::Value,
}

/// Every defect this seam is built to exclude.
pub const MUTANTS: [Mutant; 6] = [
    Mutant {
        name: "the reason is the last reason-bearing line in the window",
        provenance: "round 1, catalog.rs:134 — the verifier's reproduction",
        caught_by: "no-reason-is-correlated",
        read: fabricated_reason,
    },
    Mutant {
        name: "`active` is read from the kernel's state string alone",
        provenance: "the class the reading law exists to exclude",
        caught_by: "active-needs-positive-proof",
        read: active_without_proof,
    },
    Mutant {
        name: "an unrecognised kernel state folds to `unknown`",
        provenance: "the sentinel class — sessions, todos, workflows",
        caught_by: "no-sentinel-in-the-vocabulary",
        read: unknown_sentinel,
    },
    Mutant {
        name: "a failure whose window held nothing reports no reason",
        provenance: "the absence class, FINDINGS #36",
        caught_by: "every-reading-that-owes-one-has-a-reason",
        read: reasonless_failure,
    },
    Mutant {
        name: "a grant list is reported without its authority",
        provenance: "an appliance's declaration passing for enforcement",
        caught_by: "grants-name-their-authority",
        read: unsourced_grants,
    },
    Mutant {
        name: "the qualifier is dropped from the answer",
        provenance: "M2-K12: a limit that lives only in a README",
        caught_by: "the-limit-travels-in-the-answer",
        read: silent_qualifier,
    },
];

/// The honest reading, serialized exactly as the API answers it.
#[must_use]
pub fn honest(inputs: &Inputs) -> serde_json::Value {
    serde_json::to_value(Catalog::entry(
        &inputs.declared,
        GrantSource::ProfileDocument,
        inputs.snapshot.as_ref(),
        &inputs.history,
        inputs.window,
    ))
    .expect("an entry encodes")
}

/// A failed entry whose window really does hold a plausible sentence to
/// steal — the shape the round-1 defect was found on.
#[must_use]
pub fn a_failure_with_an_unrelated_refusal_in_its_window() -> Inputs {
    let window = Window {
        from: 1,
        to: 20,
        scanned: 20,
        truncated: false,
    };
    Inputs {
        declared: Declared {
            id: "a".to_owned(),
            package: Some("plugins/a".to_owned()),
            grants: vec![crate::entry::Grant {
                contract: "jinn:net".to_owned(),
                scope: None,
                ops: None,
            }],
            disabled: false,
        },
        snapshot: Some(Snapshot {
            state: Some("failed".to_owned()),
            incarnation: None,
            unserved: None,
            provisions: Vec::new(),
        }),
        history: History::of(
            "a",
            vec![Line {
                seq: 3,
                wall_ms: 30,
                entry: "a".to_owned(),
                kind: "GrantRefused".to_owned(),
                payload: serde_json::json!({"detail": "an earlier incarnation's refusal"}),
                sensitivity: "public".to_owned(),
            }],
            window,
        ),
        window,
    }
}

/// The round-1 code, restored: the newest reason-bearing line in the
/// window, presented as this activation's cause. It reads the real
/// ledger page — nothing here is a literal.
fn fabricated_reason(inputs: &Inputs) -> serde_json::Value {
    let mut wire = honest(inputs);
    let cited = inputs
        .history
        .lines
        .iter()
        .rev()
        .find(|line| REASON_BEARING.contains(&line.kind.as_str()));
    if let Some(line) = cited {
        wire["lifecycle"] = serde_json::json!({
            "state": "failed",
            "reason": {
                "from": "ledgered",
                "seq": line.seq,
                "kind": line.kind,
                "detail": line.payload.get("detail").cloned().unwrap_or_default(),
            },
        });
    }
    wire
}

/// The reading law with its three-fact requirement collapsed to one.
fn active_without_proof(inputs: &Inputs) -> serde_json::Value {
    let mut wire = honest(inputs);
    let live = inputs
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.state.clone());
    if live.is_some() {
        wire["lifecycle"] = serde_json::json!({ "state": "active" });
        wire.as_object_mut()
            .expect("an entry object")
            .remove("incarnation");
    }
    wire
}

/// A state this build does not know, folded into a sentinel instead of
/// carried verbatim.
fn unknown_sentinel(inputs: &Inputs) -> serde_json::Value {
    let mut wire = honest(inputs);
    wire["lifecycle"] = serde_json::json!({ "state": "unknown" });
    wire
}

/// A failure that reports the state and drops the reason.
fn reasonless_failure(inputs: &Inputs) -> serde_json::Value {
    let mut wire = honest(inputs);
    let state = wire["lifecycle"]["state"].clone();
    wire["lifecycle"] = serde_json::json!({ "state": state });
    wire
}

/// Grants on the wire with the authority that read them stripped off.
fn unsourced_grants(inputs: &Inputs) -> serde_json::Value {
    let mut wire = honest(inputs);
    let values = wire["grants"]["values"].clone();
    let qualifier = wire["grants"]["qualifier"].clone();
    wire["grants"] = serde_json::json!({ "values": values, "qualifier": qualifier });
    wire
}

/// The limit that lives only in a README.
fn silent_qualifier(inputs: &Inputs) -> serde_json::Value {
    let mut wire = honest(inputs);
    wire["grants"]["qualifier"] = serde_json::json!("");
    wire
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::{failures, CHECKS};

    #[test]
    fn the_honest_reading_passes_every_check() {
        // The precondition for the whole harness: if the honest answer
        // failed a check, every mutant below would go red for free and
        // the matrix would prove nothing.
        let inputs = a_failure_with_an_unrelated_refusal_in_its_window();
        assert_eq!(failures(&honest(&inputs)), Vec::<String>::new());
    }

    #[test]
    fn the_reproduction_really_does_hold_a_sentence_worth_stealing() {
        // Without this, the fabrication mutant would be caught for the
        // wrong reason: an empty window fabricates nothing.
        let inputs = a_failure_with_an_unrelated_refusal_in_its_window();
        assert_eq!(inputs.history.reason_bearing(), 1);
        assert_eq!(
            (MUTANTS[0].read)(&inputs)["lifecycle"]["reason"]["detail"],
            serde_json::json!("an earlier incarnation's refusal"),
            "the mutant must genuinely reproduce the round-1 answer"
        );
    }

    #[test]
    fn every_named_defect_goes_red_on_the_check_it_is_named_against() {
        let inputs = a_failure_with_an_unrelated_refusal_in_its_window();
        for mutant in &MUTANTS {
            let red = failures(&(mutant.read)(&inputs));
            assert!(
                red.iter().any(|name| name.starts_with(mutant.caught_by)),
                "MUTANT SURVIVED — `{}` ({}) was expected red on `{}`; it went red on {red:?}",
                mutant.name,
                mutant.provenance,
                mutant.caught_by
            );
        }
    }

    #[test]
    fn no_mutant_survives_and_no_check_is_dead_weight() {
        // A mutant nothing catches is a hole in the suite. A check no
        // mutant reaches is unproven law — the shape that shipped an
        // assertion which could not fail.
        let inputs = a_failure_with_an_unrelated_refusal_in_its_window();
        let mut exercised = std::collections::BTreeSet::new();
        for mutant in &MUTANTS {
            let red = failures(&(mutant.read)(&inputs));
            assert!(!red.is_empty(), "MUTANT SURVIVED: {}", mutant.name);
            for name in red {
                exercised.insert(name.split(':').next().unwrap_or_default().to_owned());
            }
        }
        for check in &CHECKS {
            assert!(
                exercised.contains(check.name),
                "no mutant reaches `{}` — the check is unproven law",
                check.name
            );
        }
    }
}
