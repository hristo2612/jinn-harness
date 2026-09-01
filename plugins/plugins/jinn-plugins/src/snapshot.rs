//! What a SNAPSHOT PULL can and cannot deliver, as a fact the definition
//! holds rather than a sentence in a README.
//!
//! # Three of the eleven readings are unreachable from a snapshot
//!
//! The reading law names eleven readings. Three of them — `mounted`
//! (a fiber resting in `pending`), `activating` (`loading`) and
//! `interrupted` (`unloading`) — describe a fiber BETWEEN TWO RESTS.
//! `jinn:introspect`'s `entries` is a PULL answered from a snapshot, and
//! a WASM unload-and-reload completes well inside the time one read
//! takes, so no poller reaches one. That is measured, not assumed:
//! `FINDINGS.md` #41 records 189 catalog reads across a real restart,
//! the kernel's own ledger recording `Active → Unloading → Pending →
//! Loading → Active`, and every single read returning `active`.
//!
//! # This used to be a claim about the PIN, and that claim is retired
//!
//! Until kernel pin `901d207` the kernel had no publish path at all, so
//! the three words were unreachable by ANY consumer and this module
//! marked them so (`UNREACHABLE_AT_PIN`), guarded by a canary built to
//! go red the day that stopped holding. `jinn:introspect@0.4.0` is that
//! day: the kernel publishes every committed transition, a subscriber
//! witnesses the transients, and the canary went red exactly as designed
//! (`docs/notes/witnessed-transitions.md` carries the transcript).
//!
//! What survives is narrower and still true: the three readings are
//! unreachable FROM A SNAPSHOT. An entry's `lifecycle` is snapshot-
//! derived, so one carrying a transient reading is still reporting
//! something it cannot have seen — and [`crate::checks`] holds the guard
//! that says so. The transients are delivered by [`crate::witness`],
//! which does not infer them from a snapshot; it is handed them.

use crate::lifecycle::Lifecycle;

/// The readings a SNAPSHOT PULL can never deliver, named in the
/// vocabulary a consumer reads. Each one describes a fiber between two
/// rests; the kernel passes through all three and answers `entries` only
/// at rest. They are reachable through [`crate::witness`], and there
/// alone.
pub const NOT_FROM_A_SNAPSHOT: [&str; 3] = ["mounted", "activating", "interrupted"];

/// What being on [`NOT_FROM_A_SNAPSHOT`] MEANS, travelling with the
/// definition rather than only in a README. Its one home.
pub const SNAPSHOT_QUALIFIER: &str =
    "not reachable from a snapshot: this reading names a fiber between two rests, and \
     `jinn:introspect`'s `entries` is a pull answered from a snapshot taken at rest. A real \
     restart, measured through this seam, completed inside one HTTP read while 189 \
     consecutive reads all returned `active` and the kernel's own ledger recorded the \
     whole path (FINDINGS.md #41). An entry's `lifecycle` is snapshot-derived, so one \
     CARRYING this reading is reporting what it cannot have seen. Since kernel pin 901d207 \
     the reading itself is reachable — on `witness`, which is handed the kernel's own \
     published transition rather than inferring it (FINDINGS.md #40)";

/// Whether a SNAPSHOT-DERIVED answer can legitimately deliver this
/// reading. It asks EXACTLY ONE question — is this word on
/// [`NOT_FROM_A_SNAPSHOT`] — so the guard built on it has one meaning. A
/// word that is no reading at all is a different defect with its own
/// check (`no-sentinel-in-the-vocabulary`), and folding the two together
/// would let either pass for the other.
#[must_use]
pub fn deliverable_from_a_snapshot(reading: &str) -> bool {
    !NOT_FROM_A_SNAPSHOT.contains(&reading)
}

impl Lifecycle {
    /// Whether THIS reading is one a snapshot pull can actually produce.
    /// `false` is not a statement about the plugin — it is a statement
    /// about which surface may carry the reading.
    #[must_use]
    pub fn deliverable_from_a_snapshot(&self) -> bool {
        deliverable_from_a_snapshot(self.name())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::catalog::{Catalog, Declared};
    use crate::checks::failures;
    use crate::entry::GrantSource;
    use crate::history::History;
    use crate::lifecycle::{Snapshot, Unserved, Window};
    use crate::transition::NAMES;

    const GUARD: &str = "no-transient-reading-from-a-snapshot";

    fn window() -> Window {
        Window {
            from: 1,
            to: 40,
            scanned: 40,
            truncated: false,
        }
    }

    fn wire(snapshot: &Snapshot) -> serde_json::Value {
        serde_json::to_value(Catalog::entry(
            &Declared {
                id: "a".to_owned(),
                ..Declared::default()
            },
            GrantSource::ProfileDocument,
            Some(snapshot),
            &History::of("a", Vec::new(), window()),
            window(),
        ))
        .expect("an entry encodes")
    }

    fn snapshot(state: &str, incarnation: Option<u64>, unserved: Option<Unserved>) -> Snapshot {
        Snapshot {
            state: Some(state.to_owned()),
            incarnation,
            unserved,
            provisions: Vec::new(),
        }
    }

    #[test]
    fn every_reading_marked_snapshot_unreachable_is_a_reading_this_seam_can_name() {
        // A marking that named a word outside the vocabulary would mark
        // nothing, and the guard built on it would be unreachable law.
        for unreachable in crate::snapshot::NOT_FROM_A_SNAPSHOT {
            assert!(
                NAMES.contains(&unreachable),
                "`{unreachable}` is marked snapshot-unreachable and is not a reading"
            );
            assert!(!crate::snapshot::deliverable_from_a_snapshot(unreachable));
        }
        // Precondition: the marking is a RESTRICTION and not a blanket —
        // the rest of the vocabulary is deliverable.
        let deliverable = NAMES
            .iter()
            .filter(|name| crate::snapshot::deliverable_from_a_snapshot(name))
            .count();
        assert_eq!(
            deliverable,
            NAMES.len() - crate::snapshot::NOT_FROM_A_SNAPSHOT.len()
        );
        assert!(crate::snapshot::SNAPSHOT_QUALIFIER.contains("FINDINGS.md #41"));
        // And the marking is about a SURFACE, not about the pin: the
        // same three words ARE deliverable by the witness, which is
        // handed them rather than inferring them.
        for unreachable in crate::snapshot::NOT_FROM_A_SNAPSHOT {
            assert!(
                crate::witness::WITNESS_QUALIFIER.contains("witnessed history"),
                "`{unreachable}` has nowhere honest to be delivered"
            );
        }
    }

    #[test]
    fn the_third_fact_behind_active_rides_on_the_wire() {
        // The reading law reaches `active` only with an incarnation
        // installed AND the live incarnation owing nothing. Two of those
        // three facts ride on the wire; without the third, a consumer
        // must TRUST that the reader applied it.
        let owing = wire(&snapshot("loading", Some(7), Some(Unserved::Restarting)));
        assert_eq!(
            owing["owes"],
            json!("restarting"),
            "what the live incarnation owes is the third fact, and it belongs beside the claim"
        );
        // Precondition: an incarnation owing NOTHING says so by absence,
        // so the field is a positive reading and not a default.
        let serving = wire(&snapshot("active", Some(7), None));
        assert!(
            serving.get("owes").is_none(),
            "an incarnation that owes nothing carries no owed change: {serving}"
        );
    }

    #[test]
    fn a_wire_answer_reading_active_while_its_incarnation_owes_a_change_is_caught() {
        // The honest reader cannot produce this. A defective one can,
        // and until the evidence rode on the wire nothing downstream
        // could tell the two apart.
        let mut defective = wire(&snapshot("active", Some(7), None));
        defective["lifecycle"] = json!({ "state": "active" });
        defective["owes"] = json!("restarting");
        let red = failures(&defective);
        assert!(
            red.iter()
                .any(|name| name.starts_with("active-needs-positive-proof")),
            "`active` beside an owed change went uncaught: {red:?}"
        );
    }

    #[test]
    fn the_guard_rejects_exactly_the_readings_a_snapshot_cannot_deliver() {
        // A transient reading carried by a SNAPSHOT-DERIVED entry is a
        // defect (FINDINGS #41): the entry's lifecycle is a join over a
        // pull, and a pull is answered at rest.
        for name in NAMES {
            let mut answer = wire(&snapshot("active", Some(7), None));
            answer["lifecycle"] = json!({ "state": name, "reason": { "from": "disabled" } });
            let caught = failures(&answer)
                .iter()
                .any(|failure| failure.starts_with(GUARD));
            assert_eq!(
                caught,
                ["mounted", "activating", "interrupted"].contains(&name),
                "the guard answered {caught} for `{name}`"
            );
        }
    }
}
