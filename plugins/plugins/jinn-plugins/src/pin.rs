//! What THIS PIN can and cannot deliver, as a fact the definition holds
//! rather than a sentence in a README.
//!
//! # Three of the eleven readings are unreachable here
//!
//! The reading law names eleven readings. Three of them — `mounted`
//! (a fiber resting in `pending`), `activating` (`loading`) and
//! `interrupted` (`unloading`) — describe a fiber BETWEEN TWO RESTS. The
//! kernel genuinely passes through all three; no consumer at pin
//! `3a8e5c0` can ever see one, because `jinn:introspect@0.2.0` is a pull
//! answered from a snapshot and a WASM unload-and-reload completes well
//! inside the time one read takes. That is measured, not assumed:
//! `FINDINGS.md` #41 records 189 catalog reads across a real restart, the
//! kernel's own ledger recording `Active → Unloading → Pending → Loading
//! → Active`, and every single read returning `active`.
//!
//! So the vocabulary a consumer reads has three words nothing can
//! produce. The limit therefore travels with the DEFINITION, exactly as
//! M2-K12 made a limit travel with the response — and
//! [`crate::checks`] carries the canary that goes red the day it stops
//! being true.

use crate::lifecycle::Lifecycle;

/// The readings this pin's `jinn:introspect` can never deliver, named in
/// the vocabulary a consumer reads. Each one describes a fiber between
/// two rests; the kernel passes through all three and answers only at
/// rest.
pub const UNREACHABLE_AT_PIN: [&str; 3] = ["mounted", "activating", "interrupted"];

/// What being on [`UNREACHABLE_AT_PIN`] MEANS, travelling with the
/// definition rather than only in a README. Its one home.
pub const UNREACHABLE_QUALIFIER: &str =
    "unreachable at kernel pin 3a8e5c0: this reading names a fiber between two rests, and \
     `jinn:introspect@0.2.0` is a pull answered from a snapshot taken at rest. A real \
     restart, measured through this seam, completed inside one HTTP read while 189 \
     consecutive reads all returned `active` and the kernel's own ledger recorded the \
     whole path (FINDINGS.md #41, and #40 for the missing publish). The reading law keeps \
     the word because the kernel really does pass through it; a catalog answer that \
     CARRIES it at this pin is a defect, and `checks::CHECKS` holds the canary that says so";

/// Whether a catalog at this pin can legitimately deliver this reading.
/// It asks EXACTLY ONE question — is this word on [`UNREACHABLE_AT_PIN`]
/// — so the canary built on it has one meaning. A word that is no
/// reading at all is a different defect with its own check
/// (`no-sentinel-in-the-vocabulary`), and folding the two together would
/// let either pass for the other.
#[must_use]
pub fn deliverable_at_pin(reading: &str) -> bool {
    !UNREACHABLE_AT_PIN.contains(&reading)
}

impl Lifecycle {
    /// Whether THIS reading is one a consumer can actually be handed at
    /// this pin. `false` is not a statement about the plugin — it is a
    /// statement about the kernel's read surface.
    #[must_use]
    pub fn deliverable_at_pin(&self) -> bool {
        deliverable_at_pin(self.name())
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

    const CANARY: &str = "no-transient-reading-at-this-pin";

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
    fn every_reading_marked_unreachable_is_a_reading_this_seam_can_name() {
        // A marking that named a word outside the vocabulary would mark
        // nothing, and the canary built on it would be unreachable law.
        for unreachable in crate::pin::UNREACHABLE_AT_PIN {
            assert!(
                NAMES.contains(&unreachable),
                "`{unreachable}` is marked unreachable and is not a reading"
            );
            assert!(!crate::pin::deliverable_at_pin(unreachable));
        }
        // Precondition: the marking is a RESTRICTION and not a blanket —
        // the rest of the vocabulary is deliverable.
        let deliverable = NAMES
            .iter()
            .filter(|name| crate::pin::deliverable_at_pin(name))
            .count();
        assert_eq!(
            deliverable,
            NAMES.len() - crate::pin::UNREACHABLE_AT_PIN.len()
        );
        assert!(crate::pin::UNREACHABLE_QUALIFIER.contains("FINDINGS.md #41"));
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
    fn the_canary_rejects_exactly_the_readings_this_pin_cannot_deliver() {
        // A transient reading DELIVERED at this pin is itself a defect
        // (FINDINGS #41). The day the kernel gains a publish path this
        // goes red and forces the reading law to be re-read.
        for name in NAMES {
            let mut answer = wire(&snapshot("active", Some(7), None));
            answer["lifecycle"] = json!({ "state": name, "reason": { "from": "disabled" } });
            let caught = failures(&answer)
                .iter()
                .any(|failure| failure.starts_with(CANARY));
            assert_eq!(
                caught,
                ["mounted", "activating", "interrupted"].contains(&name),
                "the canary answered {caught} for `{name}`"
            );
        }
    }
}
