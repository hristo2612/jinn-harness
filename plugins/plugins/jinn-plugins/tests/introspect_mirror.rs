//! THE INTROSPECT MIRROR CHECK for this seam (harness pin-bump 6,
//! `jinn:introspect` 0.4.0 → 0.5.0). This crate spells the bundle's
//! `transition` record as `witness::Transition`, its `unserved` enum as
//! `lifecycle::Unserved`, and reads `entry` by string key in
//! `Snapshot::parse_entries`. Each is checked here against the vendored
//! `contract.wit` PARSED (`harness_pin::ContractWit`), not read by eye.
//! Enforced exactly: key sets and case names; never types, never the
//! daemon's live answer (the real-composition suite owns that).

use std::collections::BTreeSet;

use harness_pin::ContractWit;
use jinn_plugins::{Snapshot, Transition, Unserved};

fn contract() -> ContractWit {
    ContractWit::vendored("jinn-introspect").expect(
        "the vendored jinn:introspect bundle parses as WIT (0.5.0 is its first parseable edition)",
    )
}

#[test]
fn transition_mirrors_the_transition_record_including_the_escaped_from_field() {
    let populated = Transition {
        entry: "a".into(),
        fiber: 1,
        incarnation: Some(1),
        from: "loading".into(),
        to: "active".into(),
        ordinal: 1,
        committed_by: 1,
    };
    let written: BTreeSet<String> = serde_json::to_value(&populated)
        .expect("serializes")
        .as_object()
        .expect("an object")
        .keys()
        .cloned()
        .collect();
    // `%from` in the contract is the wire field `from`: the parser strips
    // the escape, and the mirror must write the unescaped name.
    assert_eq!(
        written,
        contract().record_fields("transition").expect("the record")
    );
}

#[test]
fn unserved_mirrors_the_unserved_enum_case_for_case() {
    let mirrored: Vec<String> = [
        Unserved::Restarting,
        Unserved::Gone,
        Unserved::Suspended,
        Unserved::Stalled,
    ]
    .iter()
    .map(|case| {
        serde_json::to_value(case)
            .expect("serializes")
            .as_str()
            .expect("a string")
            .to_owned()
    })
    .collect();
    let declared = contract().enum_cases("unserved").expect("the enum");
    assert_eq!(mirrored, declared, "same cases, same order, same spelling");
    for case in &declared {
        assert!(Unserved::parse(case).is_some(), "`{case}` parses");
    }
}

#[test]
fn the_snapshot_reads_entry_fields_by_their_contract_names() {
    // An entry whose keys are EXACTLY the record's fields, with a
    // representative value under each; a snapshot that read any key the
    // contract does not spell would come back empty here.
    let fields = contract().record_fields("entry").expect("the record");
    let mut entry = serde_json::Map::new();
    for field in &fields {
        let value = match field.as_str() {
            "id" => serde_json::json!("a"),
            "fiber" | "incarnation" => serde_json::json!(3),
            "state" => serde_json::json!("active"),
            "unserved" => serde_json::json!("restarting"),
            "provisions" => serde_json::json!(["jinn:plugins.main"]),
            "registrations" => serde_json::json!({}),
            other => panic!("the entry record grew a field this seam has not read: `{other}`"),
        };
        entry.insert(field.clone(), value);
    }
    let parsed = Snapshot::parse_entries(&serde_json::json!([entry]));
    let snapshot = parsed.get("a").expect("keyed by `id`");
    assert_eq!(snapshot.state.as_deref(), Some("active"));
    assert_eq!(snapshot.incarnation, Some(3));
    assert_eq!(snapshot.unserved, Some(Unserved::Restarting));
    assert_eq!(snapshot.provisions, ["jinn:plugins.main"]);
}
