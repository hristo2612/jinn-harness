//! THE INTROSPECT MIRROR CHECK (harness pin-bump 6, `jinn:introspect`
//! 0.4.0 → 0.5.0; pin-bump 7 widened the entry's `extra` set to 0.6.0). `kernel.rs` spells three of the bundle's records a
//! second time as `serde` structs — `Readiness`, `Registrations`,
//! `IntrospectEntry` — and nothing but a reader's eye ever compared the
//! copies. 0.5.0 is the FIRST edition of the bundle a parser accepts, so
//! this is the first pin at which the comparison can be mechanical: the
//! vendored `contract.wit` is parsed (`harness_pin::ContractWit`) and each
//! mirror's wire keys are asserted against the record it claims to be.
//!
//! What is enforced, exactly: the set of keys a mirror WRITES equals (or,
//! for the entry, is a named subset of) the record's field names as the
//! parser reads them. Types are not compared, and the answer the daemon
//! actually sends is proven by the real-composition suite, not here.

use std::collections::BTreeSet;

use harness_pin::ContractWit;
use jinn_api::kernel::{
    IntrospectEntry, Readiness, Registrations, OP_INTROSPECT_ENTRIES, OP_INTROSPECT_READINESS,
};

fn keys<T: serde::Serialize>(value: &T) -> BTreeSet<String> {
    serde_json::to_value(value)
        .expect("a mirror serializes")
        .as_object()
        .expect("a mirror is an object")
        .keys()
        .cloned()
        .collect()
}

fn contract() -> ContractWit {
    ContractWit::vendored("jinn-introspect").expect(
        "the vendored jinn:introspect bundle parses as WIT (0.5.0 is its first parseable edition)",
    )
}

#[test]
fn readiness_mirrors_the_readiness_report_record_the_readiness_operation_answers() {
    let wit = contract();
    // 0.5.0 renamed the RECORD `readiness` → `readiness-report`; the wire
    // OPERATION is still `readiness`, and this is the check that both
    // halves of that sentence hold in the vendored file.
    assert_eq!(
        wit.function_result(OP_INTROSPECT_READINESS)
            .expect("the readiness operation is declared")
            .as_deref(),
        Some("readiness-report"),
        "the `readiness` operation answers the `readiness-report` record"
    );
    assert_eq!(
        keys(&Readiness::default()),
        wit.record_fields("readiness-report").expect("the record"),
        "`Readiness` writes exactly the `readiness-report` fields"
    );
}

#[test]
fn registrations_mirrors_the_registrations_record() {
    assert_eq!(
        keys(&Registrations::default()),
        contract()
            .record_fields("registrations")
            .expect("the record"),
    );
}

#[test]
fn the_entry_mirror_names_only_entry_fields_and_leaves_exactly_unserved_to_extra() {
    let wit = contract();
    assert!(
        wit.function_result(OP_INTROSPECT_ENTRIES).is_ok(),
        "the entries operation is declared"
    );
    let record = wit.record_fields("entry").expect("the record");
    // Every optional field populated, so its key is written.
    let populated = IntrospectEntry {
        id: "a".into(),
        fiber: Some(1),
        state: Some("active".into()),
        incarnation: Some(1),
        ..IntrospectEntry::default()
    };
    let named = keys(&populated);
    let foreign: Vec<_> = named.difference(&record).collect();
    assert!(
        foreign.is_empty(),
        "keys no `entry` field spells: {foreign:?}"
    );
    // What the mirror does NOT name lands in `extra` additively; the
    // plugins seam reads `unserved` from there by key (its own mirror
    // check covers that key). Name the gap so a widening of it is a
    // decision, not drift. Pin-bump 7 (`jinn:introspect` 0.6.0, M2-K24)
    // widened it by decision: `injects` (the entry's string-lane
    // declaration) and `unmet` (which declared providers its gate finds
    // missing) are carried in `extra` and read by nothing here yet — the
    // operator surface that shows WHY an entry is `pending` is a later
    // card's, not a side effect of a pin bump.
    let unnamed: Vec<_> = record.difference(&named).cloned().collect();
    assert_eq!(
        unnamed,
        ["injects", "unmet", "unserved"],
        "entry fields carried only in `extra`"
    );
}
