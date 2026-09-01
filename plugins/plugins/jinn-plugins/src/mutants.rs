//! The MUTATION HARNESS: inject each named defect, show exactly which
//! honesty checks go red, and for the reason the defect is named after.
//!
//! # Why this exists, and what it stands in for
//!
//! Red-first ordering shows that a test COULD fail. It was missed on this
//! seam's round 1 and cannot be un-missed, so what stands in its place is
//! strictly stronger evidence — and it is an ARTIFACT that runs, not a
//! sentence in a note. Each defect in [`crate::defects`] is a defective
//! IMPLEMENTATION of the reading, computed from the same [`Inputs`] the
//! honest one reads. Each is then measured with the same
//! [`crate::checks`] predicates the real composition proof runs against
//! the daemon's own answers.
//!
//! A mutant no check catches is a hole in the suite; a check no mutant
//! reaches is unproven law. The sweep at the bottom fails on either.
//!
//! # A mutant must go red for its OWN reason
//!
//! Round 3 found `active-needs-positive-proof` documented for two
//! exclusions and enforcing one. The sweep had not noticed, because the
//! check WAS reached — by the other half. So every mutant now names the
//! [`Mutant::evidence`] its red message must carry, and a mutant caught
//! by a neighbouring reason no longer counts as caught.

use crate::catalog::Declared;
use crate::defects;
use crate::entry::GrantSource;
use crate::history::History;
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
    /// A fragment the red message must carry, so a mutant caught for a
    /// NEIGHBOURING reason does not pass for one caught by its own.
    pub evidence: &'static str,
    /// The input shape this defect is reachable on.
    pub on: fn() -> Inputs,
    /// The defective reading, over the same inputs the honest one reads.
    pub read: fn(&Inputs) -> serde_json::Value,
}

/// Every defect this seam is built to exclude.
pub const MUTANTS: [Mutant; 8] = [
    Mutant {
        name: "the reason is the last reason-bearing line in the window",
        provenance: "round 1, catalog.rs:134 — the verifier's reproduction",
        caught_by: "no-reason-is-correlated",
        evidence: "carried `seq`",
        on: defects::a_failure_with_an_unrelated_refusal_in_its_window,
        read: defects::fabricated_reason,
    },
    Mutant {
        name: "`active` is read from the kernel's state string alone",
        provenance: "the class the reading law exists to exclude",
        caught_by: "active-needs-positive-proof",
        evidence: "with no incarnation to prove it",
        on: defects::a_failure_with_an_unrelated_refusal_in_its_window,
        read: defects::active_without_proof,
    },
    Mutant {
        name: "`active` survives an incarnation that already owes a change",
        provenance: "round 3, checks.rs — the half the doc claimed and the code did not enforce",
        caught_by: "active-needs-positive-proof",
        evidence: "its live incarnation owes",
        on: defects::a_loading_incarnation_that_already_owes_a_change,
        read: defects::active_while_the_incarnation_owes_a_change,
    },
    Mutant {
        name: "a loading fiber that already owes a change remains eternally activating",
        provenance: "round 2 verify — the named defect no mutant reached",
        caught_by: "no-transient-reading-from-a-snapshot",
        evidence: "which a snapshot cannot produce",
        on: defects::a_loading_incarnation_that_already_owes_a_change,
        read: defects::eternally_activating,
    },
    Mutant {
        name: "an unrecognised kernel state folds to `unknown`",
        provenance: "the sentinel class — sessions, todos, workflows",
        caught_by: "no-sentinel-in-the-vocabulary",
        evidence: "a sentinel state",
        on: defects::a_failure_with_an_unrelated_refusal_in_its_window,
        read: defects::unknown_sentinel,
    },
    Mutant {
        name: "a failure whose window held nothing reports no reason",
        provenance: "the absence class, FINDINGS #36",
        caught_by: "every-reading-that-owes-one-has-a-reason",
        evidence: "with no reason at all",
        on: defects::a_failure_with_an_unrelated_refusal_in_its_window,
        read: defects::reasonless_failure,
    },
    Mutant {
        name: "a grant list is reported without its authority",
        provenance: "an appliance's declaration passing for enforcement",
        caught_by: "grants-name-their-authority",
        evidence: "unnamed authority",
        on: defects::a_failure_with_an_unrelated_refusal_in_its_window,
        read: defects::unsourced_grants,
    },
    Mutant {
        name: "the qualifier is dropped from the answer",
        provenance: "M2-K12: a limit that lives only in a README",
        caught_by: "the-limit-travels-in-the-answer",
        evidence: "grants with no qualifier",
        on: defects::a_failure_with_an_unrelated_refusal_in_its_window,
        read: defects::silent_qualifier,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checks::{failures, CHECKS};

    #[test]
    fn the_honest_reading_passes_every_check_on_every_fixture() {
        // The precondition for the whole harness: if an honest answer
        // failed a check, every mutant on that fixture would go red for
        // free and the matrix would prove nothing. Checked on EVERY
        // input shape the table uses, not only the first.
        for mutant in &MUTANTS {
            let inputs = (mutant.on)();
            assert_eq!(
                failures(&honest(&inputs)),
                Vec::<String>::new(),
                "the honest reading behind `{}` is not clean",
                mutant.name
            );
        }
    }

    #[test]
    fn each_fixture_really_holds_what_its_defects_need_to_reach() {
        // Without this, a mutant would be caught for the wrong reason:
        // an empty window fabricates nothing, and an incarnation owing
        // nothing cannot be reported as serving while it owes.
        let stealable = defects::a_failure_with_an_unrelated_refusal_in_its_window();
        assert_eq!(stealable.history.reason_bearing(), 1);
        assert_eq!(
            defects::fabricated_reason(&stealable)["lifecycle"]["reason"]["detail"],
            serde_json::json!("an earlier incarnation's refusal"),
            "the mutant must genuinely reproduce the round-1 answer"
        );

        let owing = defects::a_loading_incarnation_that_already_owes_a_change();
        let snapshot = owing.snapshot.as_ref().expect("a live fiber");
        assert!(
            snapshot.incarnation.is_some() && snapshot.unserved.is_some(),
            "the owed-change fixture must genuinely owe a change"
        );
        assert_eq!(
            honest(&owing)["lifecycle"]["state"],
            serde_json::json!("restarting"),
            "its honest reading has to be a rest, or both defects on it are the fixture's \
             own shape rather than an injected one"
        );
    }

    #[test]
    fn every_named_defect_goes_red_on_the_check_it_is_named_against() {
        for mutant in &MUTANTS {
            let red = failures(&(mutant.read)(&(mutant.on)()));
            let own = red
                .iter()
                .find(|name| name.starts_with(mutant.caught_by))
                .unwrap_or_else(|| {
                    panic!(
                        "MUTANT SURVIVED — `{}` ({}) was expected red on `{}`; it went red on \
                         {red:?}",
                        mutant.name, mutant.provenance, mutant.caught_by
                    )
                });
            assert!(
                own.contains(mutant.evidence),
                "`{}` was caught by `{}` for the WRONG reason: expected {:?} in {own:?}",
                mutant.name,
                mutant.caught_by,
                mutant.evidence
            );
        }
    }

    #[test]
    fn no_mutant_survives_and_no_check_is_dead_weight() {
        // A mutant nothing catches is a hole in the suite. A check no
        // mutant reaches is unproven law — the shape that shipped an
        // assertion which could not fail.
        let mut exercised = std::collections::BTreeSet::new();
        for mutant in &MUTANTS {
            let red = failures(&(mutant.read)(&(mutant.on)()));
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
