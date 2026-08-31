//! The DEFECT CATALOGUE: each named defect as a defective reading, and
//! the input shapes they are measured on.
//!
//! Separate from [`crate::mutants`], which owns the harness that
//! measures them, because the two answer different questions: this file
//! says WHAT went wrong somewhere, once; that one says how we prove a
//! check would catch it. Every reading here computes from the same
//! [`Inputs`] the honest one reads — nothing is a literal, because a
//! defect handed its own answer proves only that the checker can read
//! JSON.

use crate::catalog::Declared;
use crate::history::{History, Line, REASON_BEARING};
use crate::lifecycle::{Snapshot, Unserved, Window};
use crate::mutants::{honest, Inputs};

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

/// A LOADING fiber whose live incarnation already owes a change — the
/// shape both round-3 defects were found on. Its honest reading is
/// `restarting`: a rest the operator can be handed, so a mutant's
/// `active` or `activating` is the injected defect and not the fixture's
/// own shape.
#[must_use]
pub fn a_loading_incarnation_that_already_owes_a_change() -> Inputs {
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
            state: Some("loading".to_owned()),
            incarnation: Some(7),
            unserved: Some(Unserved::Restarting),
            provisions: vec!["jinn:plugins.main".to_owned()],
        }),
        history: History::of("a", Vec::new(), window),
        window,
    }
}

/// The round-1 code, restored: the newest reason-bearing line in the
/// window, presented as this activation's cause. It reads the real
/// ledger page — nothing here is a literal.
#[must_use]
pub fn fabricated_reason(inputs: &Inputs) -> serde_json::Value {
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
#[must_use]
pub fn active_without_proof(inputs: &Inputs) -> serde_json::Value {
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

/// The reading law's THIRD fact dropped: an installed incarnation taken
/// as proof on its own, so an incarnation that already owes a change is
/// still reported as serving. This is the half
/// `active-needs-positive-proof` documented and did not enforce until
/// round 3.
#[must_use]
pub fn active_while_the_incarnation_owes_a_change(inputs: &Inputs) -> serde_json::Value {
    let mut wire = honest(inputs);
    if inputs
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.incarnation)
        .is_some()
    {
        wire["lifecycle"] = serde_json::json!({ "state": "active" });
    }
    wire
}

/// The `unserved` arm dropped from the `loading` case: a fiber that owes
/// a change nothing will schedule keeps being reported as on its way up.
/// At this pin it is doubly wrong — `activating` is a reading no consumer
/// can legitimately be handed at all (`crate::pin`).
#[must_use]
pub fn eternally_activating(inputs: &Inputs) -> serde_json::Value {
    let mut wire = honest(inputs);
    if inputs
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.state.as_deref())
        == Some("loading")
    {
        wire["lifecycle"] = serde_json::json!({ "state": "activating" });
    }
    wire
}

/// A state this build does not know, folded into a sentinel instead of
/// carried verbatim.
#[must_use]
pub fn unknown_sentinel(inputs: &Inputs) -> serde_json::Value {
    let mut wire = honest(inputs);
    wire["lifecycle"] = serde_json::json!({ "state": "unknown" });
    wire
}

/// A failure that reports the state and drops the reason.
#[must_use]
pub fn reasonless_failure(inputs: &Inputs) -> serde_json::Value {
    let mut wire = honest(inputs);
    let state = wire["lifecycle"]["state"].clone();
    wire["lifecycle"] = serde_json::json!({ "state": state });
    wire
}

/// Grants on the wire with the authority that read them stripped off.
#[must_use]
pub fn unsourced_grants(inputs: &Inputs) -> serde_json::Value {
    let mut wire = honest(inputs);
    let values = wire["grants"]["values"].clone();
    let qualifier = wire["grants"]["qualifier"].clone();
    wire["grants"] = serde_json::json!({ "values": values, "qualifier": qualifier });
    wire
}

/// The limit that lives only in a README.
#[must_use]
pub fn silent_qualifier(inputs: &Inputs) -> serde_json::Value {
    let mut wire = honest(inputs);
    wire["grants"]["qualifier"] = serde_json::json!("");
    wire
}
