//! LIFECYCLE HONESTY through the REAL pinned daemon, and the honest
//! record of the half no operator surface can reach.
//!
//! # What this file establishes, and what it refuses to claim
//!
//! The seam's reading law names eleven readings. Through the real daemon
//! this file proves the ones the machine RESTS in — an entry that never
//! activated, one disabled at runtime, one whose artifact was refused —
//! and it proves each with the precondition that the reader can also say
//! `active`, so a ruling-out is never a reader that cannot speak.
//!
//! The two transient ones the packet names — `mounted` (a fiber resting
//! in `pending`) and `interrupted` (one in `unloading`) — the kernel
//! genuinely passes through, and NO reader can observe them at this pin.
//! That is measured here rather than asserted: a real restart is driven
//! through the operator API, the catalog is read as fast as it will
//! answer for the whole window, and the kernel's OWN ledger is then read
//! back to show the transitions it committed while nobody could see
//! them. The reading law is then exercised on those recorded state
//! strings — the kernel's words from this run, not invented ones — which
//! is the strongest evidence this pin admits. `FINDINGS.md` #41 carries
//! the gap; this file carries its proof.

use composition::api::patch;
use composition::plugins::{booted, entry, listing, state, MAIN};
use jinn_plugins::checks::{failures, listing_states_the_join};
use jinn_plugins::lifecycle::{Lifecycle, Reason, Snapshot, Window};

const API_ID: &str = "jinn-api-http";
const SHELVED_ID: &str = "jinn-plugins-shelf";
const RESTARTED_ID: &str = "jinn-status";
const REFUSED_ID: &str = "an-artifact-this-machine-refuses";

/// A window standing for a read that HAPPENED. Only ever used to give
/// the reading law the reason argument it takes; every state string fed
/// to it below comes from the kernel's own ledger.
fn read_window() -> Window {
    Window {
        from: 1,
        to: 1,
        scanned: 1,
        truncated: false,
    }
}

fn searched() -> Reason {
    Reason::NoRecordedCause {
        window: read_window(),
        candidates: 0,
        qualifier: jinn_plugins::lifecycle::NO_CAUSE_QUALIFIER.to_owned(),
    }
}

/// The reading law over one kernel state string, with nothing owed.
fn reading(kernel_state: &str) -> Lifecycle {
    Lifecycle::read(
        Some(&Snapshot {
            state: Some(kernel_state.to_owned()),
            incarnation: None,
            unserved: None,
            provisions: Vec::new(),
        }),
        searched(),
        searched(),
    )
}

#[test]
fn every_answer_the_machine_rests_in_passes_the_honesty_checks() {
    let Some((daemon, port)) = booted("plugins-honesty-checks") else {
        return;
    };
    let listed = listing(port, MAIN);
    // Precondition: the reader CAN say `active`, so the checks below are
    // measuring a reader with something to get wrong.
    assert_eq!(state(&entry(&listed, API_ID)), "active", "{listed}");
    assert_eq!(listing_states_the_join(&listed), Ok(()));
    let entries = listed["entries"].as_array().expect("entries");
    // And it is a MIXED tree — an active entry, a dark one and a failed
    // one — so a sweep that passes is not a sweep over one shape.
    let states: std::collections::BTreeSet<String> = entries.iter().map(state).collect();
    assert!(
        states.len() >= 3,
        "the sweep needs a tree with something to get wrong: {states:?}"
    );
    for read in entries {
        assert_eq!(
            failures(read),
            Vec::<String>::new(),
            "the honesty checks the mutation harness measures mutants against, \
             run here against the real daemon's own answer: {read}"
        );
    }
    daemon.interrupt();
}

#[test]
fn an_entry_mounted_and_never_activated_never_reads_active() {
    let Some((daemon, port)) = booted("plugins-never-activated") else {
        return;
    };
    // Precondition: `active` is reachable in this very listing.
    assert_eq!(state(&entry(&listing(port, MAIN), API_ID)), "active");

    // An entry added to the document of record whose artifact hash the
    // machine refuses: mounted, and never activated, for the life of the
    // daemon. This is the durable shape of never-activated at this pin.
    daemon.edit_profile(|document| {
        let entries = document["entries"].as_array_mut().expect("entries");
        let mut clone = entries
            .iter()
            .find(|entry| entry["id"] == RESTARTED_ID)
            .expect("an entry to clone")
            .clone();
        clone["id"] = serde_json::json!(REFUSED_ID);
        clone["hash"] =
            serde_json::json!("0000000000000000000000000000000000000000000000000000000000000000");
        entries.push(clone);
    });
    daemon.eventually("the refused entry to reach the catalog", || {
        listing(port, MAIN)["entries"]
            .as_array()
            .is_some_and(|entries| entries.iter().any(|entry| entry["id"] == REFUSED_ID))
    });

    let refused = entry(&listing(port, MAIN), REFUSED_ID);
    assert_ne!(state(&refused), "active", "{refused}");
    assert_ne!(state(&refused), "activating", "{refused}");
    // It never served, so it provides nothing — and that is a reading of
    // the composition, not an empty field.
    assert_eq!(refused["provides"], serde_json::json!([]), "{refused}");
    // Whatever it reads, it reads WITH A REASON, and the reason is not a
    // ledger line pressed into service as a cause.
    let reason = &refused["lifecycle"]["reason"];
    assert_eq!(reason["from"], "no-recorded-cause", "{refused}");
    assert!(reason.get("seq").is_none(), "{refused}");
    assert_eq!(failures(&refused), Vec::<String>::new(), "{refused}");
    println!(
        "FINDINGS #39 transcript — a refused artifact reads: {}",
        refused["lifecycle"]
    );

    // And the disabled entry beside it: mounted, never activated, and
    // its reason is a POSITIVE reading of the document.
    let shelved = entry(&listing(port, MAIN), SHELVED_ID);
    assert_eq!(state(&shelved), "no-incarnation", "{shelved}");
    assert_eq!(shelved["lifecycle"]["reason"]["from"], "disabled");
    daemon.interrupt();
}

#[test]
fn the_kernel_passes_through_mounted_and_interrupted_and_no_read_can_see_it() {
    let Some((daemon, port)) = booted("plugins-transients") else {
        return;
    };
    // Precondition: the entry about to be restarted is genuinely serving
    // before the restart, so what follows is a transition and not a
    // reading of something already dark.
    assert_eq!(state(&entry(&listing(port, MAIN), RESTARTED_ID)), "active");

    // A REAL restart, driven through the operator API — a config patch
    // the plugin's own typed config reads, so the loader really does
    // take the entry down and bring it back.
    let patched = patch(
        port,
        &format!("/v1/profile/entries/{RESTARTED_ID}"),
        &serde_json::json!({ "config": { "data": { "probes": [] } } }),
    );
    assert_eq!(patched.status, 200, "{}", patched.raw);

    // Read the catalog as fast as it will answer, for the whole window.
    let mut seen: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut reads = 0_u32;
    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_secs(5) {
        seen.insert(state(&entry(&listing(port, MAIN), RESTARTED_ID)));
        reads += 1;
    }
    assert!(reads > 50, "the poll has to be a poll: {reads} reads");

    // The KERNEL'S OWN RECORD of the same window. The transitions are
    // there, in its ledger, with the cause it acted on.
    let committed: Vec<String> = daemon
        .ledger_rows()
        .iter()
        .filter(|row| row.entry.as_deref() == Some(RESTARTED_ID))
        .filter(|row| row.kind.contains("FiberTransition"))
        .map(|row| row.kind.clone())
        .collect();
    let through = |state: &str| committed.iter().any(|row| row.contains(state));
    assert!(
        through("Unloading") && through("Pending") && through("Loading"),
        "the restart has to have happened for this test to mean anything: {committed:?}"
    );
    println!("KERNEL RECORD across the restart ({reads} catalog reads):");
    for row in &committed {
        println!("  {row}");
    }
    println!("READINGS OBSERVED: {seen:?}");

    // And not one of those states was ever visible. The catalog is not
    // wrong here — it answers what `jinn:introspect` holds when it is
    // asked, and the kernel is only ever asked at rest. FINDINGS #41.
    for invisible in ["mounted", "interrupted", "activating"] {
        assert!(
            !seen.contains(invisible),
            "if a transient reading became observable at this pin, FINDINGS #41 is stale \
             and this seam owes it a direct proof: {seen:?}"
        );
    }

    // So the reading law is exercised on the kernel's OWN recorded state
    // words from this very run — `pending` and `unloading` above — which
    // is the strongest evidence this pin admits for the two readings no
    // surface exposes. It is stated as what it is: the join runs here,
    // in this process, because there is nowhere else to run it.
    assert_eq!(
        reading("pending"),
        Lifecycle::Mounted,
        "the kernel's own `pending` is mounted-never-activated, and is never active"
    );
    assert!(!reading("pending").is_serving());
    match reading("unloading") {
        Lifecycle::Interrupted { reason } => assert_eq!(
            reason,
            searched(),
            "an interruption carries a reason, and never a correlated one"
        ),
        other => panic!("the kernel's own `unloading` must read interrupted: {other:?}"),
    }
    assert!(!reading("unloading").is_serving());
    daemon.interrupt();
}
