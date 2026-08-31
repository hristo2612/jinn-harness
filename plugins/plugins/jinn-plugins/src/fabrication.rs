//! The FABRICATION proofs: the verifier's round-1 reproduction, kept as
//! law in its own file because it is about one defect class rather than
//! about the reading law as a whole.
//!
//! `Catalog::entry` used to hand a failed activation the LAST
//! reason-bearing line in the window as its reason, with no link of any
//! kind between the two. An unrelated refusal from an EARLIER
//! incarnation therefore surfaced as this activation's cause: a real,
//! plausible, FALSE sentence, which is worse than an absence because it
//! looks like evidence.

use crate::catalog::{Catalog, Declared};
use crate::entry::GrantSource;
use crate::history::{History, Line};
use crate::lifecycle::{Reason, Snapshot, Unserved, Window};

fn window() -> Window {
    Window {
        from: 1,
        to: 40,
        scanned: 40,
        truncated: false,
    }
}

fn line(seq: u64, entry: &str, kind: &str, payload: serde_json::Value) -> Line {
    Line {
        seq,
        wall_ms: 1_000 + seq,
        entry: entry.to_owned(),
        kind: kind.to_owned(),
        payload,
        sensitivity: "public".to_owned(),
    }
}

fn snapshot(state: Option<&str>, incarnation: Option<u64>, unserved: Option<Unserved>) -> Snapshot {
    Snapshot {
        state: state.map(ToOwned::to_owned),
        incarnation,
        unserved,
        provisions: Vec::new(),
    }
}
//
// The verifier's round-1 reproduction, kept as law. `Catalog::entry` used
// to hand a failed activation the LAST reason-bearing line in the window
// as its reason, with no link of any kind between the two. An unrelated
// refusal from an EARLIER incarnation therefore surfaced as this
// activation's cause: a real, plausible, FALSE sentence, which is worse
// than an absence because it looks like evidence.

/// The verifier's reproduction, verbatim in shape: one refusal, written
/// by an earlier incarnation of this same entry, and a failure that has
/// nothing to do with it.
fn an_unrelated_earlier_refusal() -> History {
    History::of(
        "a",
        vec![
            line(
                1,
                "a",
                "GrantRefused",
                serde_json::json!({"detail": "unrelated refusal from an earlier incarnation"}),
            ),
            line(
                2,
                "a",
                "FiberTransition",
                serde_json::json!({"to": "Pending", "cause": "ConfigChanged"}),
            ),
            line(
                3,
                "a",
                "FiberTransition",
                serde_json::json!({"to": "Failed", "cause": "InitialLoad"}),
            ),
        ],
        window(),
    )
}

#[test]
fn an_unrelated_earlier_refusal_is_never_this_failures_reason() {
    let history = an_unrelated_earlier_refusal();
    // Precondition: the window really does hold that refusal, so what is
    // ruled out below is the seam CITING it and not an empty search.
    assert!(
        history.lines.iter().any(|line| line.kind == "GrantRefused"),
        "the reproduction needs the refusal it is about"
    );
    let reading = Catalog::entry(
        &Declared {
            id: "a".to_owned(),
            ..Declared::default()
        },
        GrantSource::ProfileDocument,
        Some(&snapshot(Some("failed"), None, None)),
        &history,
        window(),
    );
    let reason = reading
        .lifecycle
        .reason()
        .expect("a failure names a reason");
    assert_eq!(
        reason,
        &Reason::NoRecordedCause {
            window: window(),
            candidates: 1,
            qualifier: crate::lifecycle::NO_CAUSE_QUALIFIER.to_owned(),
        },
        "a neighbouring refusal presented as a cause is a fabrication: {reason:?}"
    );
}

#[test]
fn an_unrelated_earlier_refusal_is_never_a_dark_entrys_reason_either() {
    // The same defect one arm over: an entry the kernel reports with NO
    // live fiber took its reason from the same unlinked line.
    let reading = Catalog::entry(
        &Declared {
            id: "a".to_owned(),
            ..Declared::default()
        },
        GrantSource::ProfileDocument,
        Some(&snapshot(None, None, None)),
        &an_unrelated_earlier_refusal(),
        window(),
    );
    let reason = reading
        .lifecycle
        .reason()
        .expect("a reading names a reason");
    assert!(
        matches!(reason, Reason::NoRecordedCause { candidates: 1, .. }),
        "{reason:?}"
    );
}

#[test]
fn the_document_is_still_allowed_to_say_why_a_dark_entry_is_dark() {
    // The precondition that keeps the two tests above from passing for
    // the wrong reason: a POSITIVE reading of the document still wins,
    // so the fix removed a fabrication and not the seam's whole answer.
    let reading = Catalog::entry(
        &Declared {
            id: "a".to_owned(),
            disabled: true,
            ..Declared::default()
        },
        GrantSource::ProfileDocument,
        Some(&snapshot(None, None, None)),
        &an_unrelated_earlier_refusal(),
        window(),
    );
    assert_eq!(reading.lifecycle.reason(), Some(&Reason::Disabled));
}
