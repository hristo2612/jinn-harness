//! The definition's own proofs. Every honesty assertion here states and
//! asserts the PRECONDITION that makes it meaningful: an assertion over
//! an empty set, or over a reader that could not have answered otherwise,
//! is indistinguishable from one that cannot fail.

use std::collections::BTreeMap;

use crate::catalog::{Catalog, Declared};
use crate::entry::{Grant, GrantSource};
use crate::history::{History, Line};
use crate::lifecycle::{Lifecycle, Reason, Snapshot, Unserved, Window};
use crate::transition::{legal_next, may_follow, NAMES};

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

fn searched(candidates: u32) -> Reason {
    Reason::NoRecordedCause {
        window: window(),
        candidates,
        qualifier: crate::lifecycle::NO_CAUSE_QUALIFIER.to_owned(),
    }
}

fn read(snapshot: Option<&Snapshot>) -> Lifecycle {
    Lifecycle::read(snapshot, searched(0), searched(0))
}

// ---------------------------------------------------------------- reading

#[test]
fn active_is_the_only_reading_that_claims_the_plugin_is_serving() {
    // The precondition that makes this non-vacuous: the reader CAN
    // answer `active`, proven on the one input that licenses it.
    let serving = snapshot(Some("active"), Some(7), None);
    assert!(
        read(Some(&serving)).is_serving(),
        "the reader cannot answer `active` at all, so the sweep below proves nothing"
    );

    // Now the sweep: every other input, and none of them may claim it.
    let mut checked = 0;
    for state in [
        None,
        Some("pending"),
        Some("loading"),
        Some("active"),
        Some("failed"),
        Some("unloading"),
        Some("disposed"),
        Some("a-state-from-a-newer-kernel"),
    ] {
        for owed in [
            None,
            Some(Unserved::Restarting),
            Some(Unserved::Gone),
            Some(Unserved::Suspended),
            Some(Unserved::Stalled),
        ] {
            for incarnation in [None, Some(3)] {
                let reading = read(Some(&snapshot(state, incarnation, owed)));
                let licensed = state == Some("active") && owed.is_none() && incarnation.is_some();
                assert_eq!(
                    reading.is_serving(),
                    licensed,
                    "state {state:?} / owed {owed:?} / incarnation {incarnation:?} read {reading:?}"
                );
                checked += 1;
            }
        }
    }
    assert_eq!(checked, 80, "the sweep covered every combination");
}

#[test]
fn a_mounted_entry_that_never_activated_never_reads_active() {
    let mounted = snapshot(Some("pending"), None, None);
    // Precondition: this IS the mounted-but-never-activated shape — a
    // fiber exists and no incarnation was ever installed.
    assert!(mounted.state.is_some() && mounted.incarnation.is_none());
    assert_eq!(read(Some(&mounted)), Lifecycle::Mounted);
}

#[test]
fn an_entry_with_no_live_fiber_reads_neither_active_nor_activating() {
    let dark = snapshot(None, None, None);
    let reading = read(Some(&dark));
    assert!(
        matches!(reading, Lifecycle::NoIncarnation { .. }),
        "{reading:?}"
    );
    assert!(!reading.is_serving());
    assert_ne!(reading.name(), "activating");
}

#[test]
fn a_loading_fiber_that_already_owes_a_change_is_never_eternally_activating() {
    // Precondition: loading with NOTHING owed is genuinely `activating`,
    // so the assertions below are about the OWED half and not about the
    // reader being unable to say `activating` at all.
    assert_eq!(
        read(Some(&snapshot(Some("loading"), Some(1), None))),
        Lifecycle::Activating
    );
    for owed in [Unserved::Gone, Unserved::Stalled] {
        let reading = read(Some(&snapshot(Some("loading"), Some(1), Some(owed))));
        assert!(
            matches!(reading, Lifecycle::Interrupted { .. }),
            "loading + {owed:?} read {reading:?}"
        );
        assert_eq!(
            reading.reason(),
            Some(&Reason::Composition { unserved: owed }),
            "an interruption names the word the kernel used"
        );
    }
    // The two that promise a future are their own answers, not
    // interruptions: they are different next moves for a caller.
    assert_eq!(
        read(Some(&snapshot(
            Some("loading"),
            Some(1),
            Some(Unserved::Restarting)
        ))),
        Lifecycle::Restarting
    );
    assert_eq!(
        read(Some(&snapshot(
            Some("loading"),
            Some(1),
            Some(Unserved::Suspended)
        ))),
        Lifecycle::Suspended
    );
}

#[test]
fn a_failed_activation_reports_failed_with_a_reason_and_never_a_default() {
    let failed = snapshot(Some("failed"), None, None);
    // A window holding a refusal — the shape that USED to be cited as a
    // cause. The reason a failure carries is the searched statement, and
    // the refusal is COUNTED so an operator knows to go and read it.
    let history = History::of(
        "a",
        vec![
            line(
                11,
                "a",
                "FiberTransition",
                serde_json::json!({"to": "Failed"}),
            ),
            line(
                12,
                "a",
                "GrantRefused",
                serde_json::json!({"contract": "jinn:net", "detail": "bind 1 is outside the grant"}),
            ),
        ],
        window(),
    );
    // Precondition: the window really does hold a reason-bearing line,
    // so the count below is not vacuously zero over an empty scan.
    assert_eq!(history.reason_bearing(), 1);
    let reading = Catalog::entry(
        &Declared {
            id: "a".to_owned(),
            ..Declared::default()
        },
        GrantSource::ProfileDocument,
        Some(&failed),
        &history,
        window(),
    )
    .lifecycle;
    match reading {
        Lifecycle::Failed {
            reason:
                Reason::NoRecordedCause {
                    window: searched,
                    candidates,
                    qualifier,
                },
        } => {
            assert_eq!(searched, window(), "the reason carries the span it read");
            assert_eq!(candidates, 1, "the lines it declines to cite are counted");
            assert!(
                qualifier.contains("no causal parent"),
                "the limit travels in the answer: {qualifier}"
            );
        }
        other => panic!("a failed activation must name its reason: {other:?}"),
    }
}

#[test]
fn a_failure_with_no_reason_in_the_window_says_so_and_carries_the_window() {
    // The kernel does not put a guest activation's prose on the ledger at
    // this pin (FINDINGS.md #38), so this is the COMMON case and it must
    // never read as `unknown` or as a made-up sentence.
    let empty = History::of("a", Vec::new(), window());
    // Precondition: the scan happened and found nothing — not that no
    // scan was attempted.
    assert!(empty.lines.is_empty() && empty.window.scanned > 0);
    let reading = Catalog::entry(
        &Declared {
            id: "a".to_owned(),
            ..Declared::default()
        },
        GrantSource::ProfileDocument,
        Some(&snapshot(Some("failed"), None, None)),
        &empty,
        window(),
    )
    .lifecycle;
    assert_eq!(
        reading,
        Lifecycle::Failed {
            reason: Reason::NoRecordedCause {
                window: window(),
                candidates: 0,
                qualifier: crate::lifecycle::NO_CAUSE_QUALIFIER.to_owned(),
            }
        },
        "a searched-and-no-cause reason carries the window it searched"
    );
}

#[test]
fn a_state_this_table_does_not_know_is_carried_verbatim_and_never_folded() {
    let reading = read(Some(&snapshot(Some("quiescing"), Some(2), None)));
    assert_eq!(
        reading,
        Lifecycle::Unrecognised {
            kernel_state: "quiescing".to_owned()
        }
    );
    assert!(!reading.is_serving());
}

#[test]
fn an_entry_the_composition_does_not_report_is_not_mounted() {
    assert_eq!(read(None), Lifecycle::NotMounted);
}

// ------------------------------------------------------------ transitions

#[test]
fn the_table_is_total_and_names_only_readings_that_exist() {
    for name in NAMES {
        let next = legal_next(name);
        assert!(!next.is_empty(), "{name} has no row");
        for successor in next {
            assert!(
                NAMES.contains(successor),
                "{name} -> {successor} is not a reading"
            );
        }
    }
}

#[test]
fn a_failed_reading_is_never_followed_directly_by_active() {
    // R9: a failure is not retried against an unchanged environment, so
    // the next reading after `failed` is never `active`.
    assert!(!may_follow("failed", "active"));
    // Precondition: the table CAN admit `active`, so this is about
    // `failed` and not about `active` being unreachable everywhere.
    assert!(may_follow("activating", "active"));
}

#[test]
fn disposal_is_terminal_for_the_incarnation() {
    assert!(!may_follow("disposed", "active"));
    assert!(may_follow("disposed", "disposed"));
}

// ------------------------------------------------------------ attribution

#[test]
fn a_history_holds_that_plugins_lines_and_only_its_own() {
    let page = vec![
        line(1, "a", "ContractCall", serde_json::json!({})),
        line(2, "b", "ContractCall", serde_json::json!({})),
        line(
            3,
            "a",
            "EffectRegistered",
            serde_json::json!({"label": "x"}),
        ),
    ];
    // Precondition: the page really does carry another plugin's lines, so
    // "only its own" is not vacuously true of a single-tenant page.
    assert!(page.iter().any(|row| row.entry == "b"));
    let history = History::of("a", page, window());
    assert_eq!(history.lines.len(), 2);
    assert!(history.lines.iter().all(|row| row.entry == "a"));
}

#[test]
fn a_history_carries_the_window_and_the_qualifier_that_bounds_it() {
    let history = History::of("a", Vec::new(), window());
    assert_eq!(history.window, window());
    assert!(
        history.qualifier.contains("WITHIN `window`"),
        "the bound travels with the answer: {}",
        history.qualifier
    );
}

// ----------------------------------------------------------------- grants

#[test]
fn a_plugin_granted_nothing_reports_nothing_and_a_granted_one_reports_its_grants() {
    let declared = vec![
        Declared {
            id: "granted".to_owned(),
            grants: vec![Grant {
                contract: "jinn:ledger".to_owned(),
                ..Grant::default()
            }],
            ..Declared::default()
        },
        Declared {
            id: "bare".to_owned(),
            ..Declared::default()
        },
    ];
    let listing = Catalog::list(
        "main",
        "plugins/jinn-plugins-profile",
        &declared,
        GrantSource::ProfileDocument,
        &BTreeMap::new(),
        &[],
        window(),
    );
    // Precondition: this reader DOES report grants when there are any —
    // without it, a reader that answered `[]` for everything would pass.
    let granted = &listing.entries[0];
    assert_eq!(granted.grants.values.len(), 1);
    assert_eq!(granted.grants.values[0].contract, "jinn:ledger");
    // And so the empty one is a reading, not a failure to read.
    assert!(listing.entries[1].grants.values.is_empty());
    assert_eq!(
        listing.entries[1].grants.source,
        GrantSource::ProfileDocument
    );
}

#[test]
fn a_declared_grant_list_says_it_is_a_claim_and_a_read_one_says_it_is_the_authority() {
    for (source, needle) in [
        (
            GrantSource::ProfileDocument,
            "the authority the kernel enforces",
        ),
        (
            GrantSource::CatalogDeclaration,
            "NOT read from the document of record",
        ),
    ] {
        let listing = Catalog::list(
            "main",
            "plugins/x",
            &[Declared {
                id: "a".to_owned(),
                ..Declared::default()
            }],
            source,
            &BTreeMap::new(),
            &[],
            window(),
        );
        assert!(
            listing.entries[0].grants.qualifier.contains(needle),
            "{source:?} must say how far its word goes: {}",
            listing.entries[0].grants.qualifier
        );
    }
}

#[test]
fn an_unreadable_document_is_never_an_empty_catalog() {
    let refused = Catalog::parse_document(&serde_json::json!({ "nope": 1 }));
    assert!(
        refused.is_err(),
        "a document with no entries array is not an empty one"
    );
    let empty = Catalog::parse_document(&serde_json::json!({ "entries": [] })).expect("readable");
    assert!(
        empty.is_empty(),
        "a document with an empty array IS an empty one"
    );
}

// ------------------------------------------------------------- the answer

#[test]
fn every_listing_carries_the_qualifier_that_the_join_is_not_atomic() {
    let listing = Catalog::list(
        "main",
        "plugins/x",
        &[],
        GrantSource::ProfileDocument,
        &BTreeMap::new(),
        &[],
        window(),
    );
    assert!(
        listing.read.qualifier.contains("not one atomic view"),
        "{}",
        listing.read.qualifier
    );
    assert_eq!(listing.read.ledger, window());
}

#[test]
fn a_listing_names_what_the_kernel_reports_that_the_catalog_does_not_declare() {
    let mut snapshots = BTreeMap::new();
    snapshots.insert(
        "only-in-the-machine".to_owned(),
        snapshot(Some("active"), Some(1), None),
    );
    let listing = Catalog::list(
        "appliance",
        "plugins/jinn-plugins-static",
        &[Declared {
            id: "declared-only".to_owned(),
            ..Declared::default()
        }],
        GrantSource::CatalogDeclaration,
        &snapshots,
        &[],
        window(),
    );
    assert_eq!(
        listing.extra["unlisted"],
        serde_json::json!(["only-in-the-machine"])
    );
    // And the declared-but-absent one is NOT MOUNTED, not active.
    assert_eq!(listing.entries[0].lifecycle, Lifecycle::NotMounted);
}

#[test]
fn describe_says_what_a_plugin_may_do_and_what_it_has_done() {
    let history = History::of(
        "a",
        vec![
            line(1, "a", "ContractCall", serde_json::json!({})),
            line(2, "a", "ContractCall", serde_json::json!({})),
            line(3, "b", "ContractCall", serde_json::json!({})),
        ],
        window(),
    );
    let description = Catalog::describe(
        "main",
        "plugins/jinn-plugins-profile",
        &Declared {
            id: "a".to_owned(),
            grants: vec![Grant {
                contract: "jinn:fs".to_owned(),
                scope: Some(serde_json::json!("workflows")),
                ops: None,
            }],
            ..Declared::default()
        },
        GrantSource::ProfileDocument,
        Some(&snapshot(Some("active"), Some(4), None)),
        &history,
        window(),
    );
    assert_eq!(
        description.declared_effects,
        vec![r#"may call jinn:fs scoped to "workflows""#]
    );
    assert_eq!(
        description.done.get("ContractCall"),
        Some(&2),
        "only its own"
    );
    assert!(description.legal_next.contains(&"restarting".to_owned()));
}

#[test]
fn an_active_entry_carries_the_incarnation_that_proves_it() {
    let live = Catalog::entry(
        &Declared {
            id: "a".to_owned(),
            ..Declared::default()
        },
        GrantSource::ProfileDocument,
        Some(&snapshot(Some("active"), Some(7), None)),
        &History::of("a", Vec::new(), window()),
        window(),
    );
    // Precondition: this really is the `active` reading, so the number
    // below is the evidence for a claim and not decoration on a dark
    // entry.
    assert!(live.lifecycle.is_serving());
    assert_eq!(live.incarnation, Some(7));
    // And an entry with no installed incarnation carries none — the
    // absence is a reading, and it is what makes the claim checkable.
    let dark = Catalog::entry(
        &Declared {
            id: "a".to_owned(),
            ..Declared::default()
        },
        GrantSource::ProfileDocument,
        Some(&snapshot(None, None, None)),
        &History::of("a", Vec::new(), window()),
        window(),
    );
    assert_eq!(dark.incarnation, None);
}

// UI-2 (§9.7 amendment 8(d)): the attestation is the catalog's STABLE
// reading of an extension entry — its declared origin and its source's
// digest — so the page renders the breadcrumb from the entry, never from
// a sliding history window. An entry declaring no origin attests nothing.
#[test]
fn an_extension_entry_attests_its_origin_and_its_source_digest_and_the_rest_attest_nothing() {
    let declared = Catalog::parse_document(&serde_json::json!({ "entries": [
        { "id": "ext-green", "package": "ext/jinn-ext-js-boa", "hash": "0",
          "config": { "grants": [], "data": { "topics": [], "source": "(p) => p", "origin": "human" } } },
        { "id": "jinn-api-http", "package": "api/jinn-api-http", "hash": "0" }
    ] }))
    .expect("readable");
    let listing = Catalog::list(
        "main",
        "plugins/x",
        &declared,
        GrantSource::ProfileDocument,
        &BTreeMap::new(),
        &[],
        window(),
    );
    let answer = serde_json::to_value(&listing).expect("encodes");
    let entries = answer["entries"].as_array().expect("entries");
    assert_eq!(
        entries[0]["attestation"],
        serde_json::json!({
            "origin": "human",
            "source": "sha256:6f6a800fb5cdf3bfe422dbaf81e4022968c2e91b1ee2e6553fb4c61e92dade42"
        }),
        "{}",
        entries[0]
    );
    assert!(
        entries[1].get("attestation").is_none(),
        "no origin declared, no attestation field: {}",
        entries[1]
    );
}
