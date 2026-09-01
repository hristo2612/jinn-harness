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
//! The three transient ones — `mounted` (a fiber resting in `pending`),
//! `activating` (`loading`) and `interrupted` (`unloading`) — are now
//! proven TWICE OVER, and the two proofs say opposite-looking things
//! that are both true:
//!
//! - No POLL reaches one. A real restart is driven through the operator
//!   API and the catalog is read as fast as it will answer for the whole
//!   window; every read is a rest. That is `FINDINGS.md` #41's
//!   measurement, and it still holds — it was never a claim about the
//!   kernel, it was a claim about a pull answered from a snapshot.
//! - The SUBSCRIPTION reaches all three. At kernel pin `901d207` the
//!   kernel publishes every transition it commits, the catalog listens
//!   on the reserved topic under its own `jinn:introspect` grant, and
//!   `/v1/plugins/{catalog}/{id}/transitions` hands back what it
//!   witnessed. That is `FINDINGS.md` #40's answer.
//!
//! Which is why the pin-wide marking is gone. `UNREACHABLE_AT_PIN` said
//! no consumer at pin `3a8e5c0` could ever be handed one of the three,
//! guarded by a canary built to go red the day that stopped being true.
//! It stopped being true here, the canary's predicate refuses the very
//! readings this daemon delivered (printed below), and what replaced it
//! is the narrower law that survives: an ENTRY's lifecycle is
//! snapshot-derived and still may not carry one.

use composition::api::patch;
use composition::plugins::{booted, entry, listing, state, transitions, MAIN};
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
fn no_poll_reaches_a_transient_and_the_subscription_witnesses_every_one() {
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
    println!("READINGS THE POLL OBSERVED: {seen:?}");

    // And not one of those states was ever visible TO THE POLL. That is
    // not a defect in the catalog and never was: it answers what
    // `jinn:introspect` holds when it is asked, and a pull is answered
    // at rest. FINDINGS #41's measurement, unchanged.
    for observed in &seen {
        assert!(
            jinn_plugins::deliverable_from_a_snapshot(observed),
            "a snapshot-derived reading delivered `{observed}`: {}",
            jinn_plugins::SNAPSHOT_QUALIFIER
        );
    }

    // THE SUBSCRIPTION. The same restart, as the catalog WITNESSED it:
    // the kernel's own published transitions, not a diff of two reads.
    let witnessed = transitions(port, MAIN, RESTARTED_ID);
    println!(
        "WITNESSED BY {} ({}): {}",
        witnessed["served-by"], witnessed["stream"], witnessed["witnessed"]
    );
    let sightings = witnessed["witnessed"]
        .as_array()
        .unwrap_or_else(|| panic!("a witnessed list: {witnessed}"));
    assert!(
        !sightings.is_empty(),
        "the catalog subscribed and witnessed nothing across a real restart: {witnessed}"
    );
    assert!(
        witnessed["qualifier"]
            .as_str()
            .is_some_and(|stated| stated.contains("witnessed history")),
        "the bound has to travel in the answer: {witnessed}"
    );

    // Every sighting is the KERNEL'S record, and the ordering barrier is
    // checkable rather than merely asserted: a delivery never precedes
    // its own ledger row, so the row's sequence sits at or before the
    // `committed-by` mark the kernel published with it.
    let rows = daemon.ledger_rows();
    let high_water = rows.iter().map(|row| row.seq).max().unwrap_or_default();
    let mut witnessed_readings: std::collections::BTreeSet<String> =
        std::collections::BTreeSet::new();
    for sighting in sightings {
        assert_eq!(sighting["entry"], RESTARTED_ID, "{sighting}");
        let committed_by = sighting["committed-by"]
            .as_u64()
            .unwrap_or_else(|| panic!("a committed-by mark: {sighting}"));
        assert!(
            committed_by <= high_water,
            "a delivery claimed a ledger mark the ledger never reached: {sighting}"
        );
        witnessed_readings.insert(
            sighting["lifecycle"]["state"]
                .as_str()
                .unwrap_or_else(|| panic!("a reading: {sighting}"))
                .to_owned(),
        );
    }
    println!("READINGS THE SUBSCRIPTION WITNESSED: {witnessed_readings:?}");

    // The claim this whole packet exists to settle: the three readings
    // no poll can reach ARE reached here.
    for transient in jinn_plugins::NOT_FROM_A_SNAPSHOT {
        assert!(
            witnessed_readings.contains(transient),
            "`{transient}` was not witnessed across a real restart: {witnessed_readings:?}"
        );
        assert!(
            !seen.contains(transient),
            "the poll saw `{transient}`, which would make FINDINGS #41 stale: {seen:?}"
        );
    }

    // THE RETIRED CANARY, RUN ON WHAT THIS DAEMON ACTUALLY DELIVERED.
    // `no-transient-reading-at-this-pin` claimed no consumer at pin
    // `3a8e5c0` could be handed one of the three; its predicate is
    // `deliverable_from_a_snapshot` under its old name. Fed the readings
    // above it refuses every one — which is the red the marking was
    // built to produce, and the evidence on which it was retired. What
    // survives is the narrower law: an ENTRY may still not carry one.
    for reading in &witnessed_readings {
        let as_an_entry = serde_json::json!({
            "lifecycle": { "state": reading, "reason": { "from": "cause-not-delivered" } },
            "incarnation": 1,
            "grants": { "source": "profile-document", "values": [], "qualifier": "q" },
        });
        let red = jinn_plugins::failures(&as_an_entry);
        if jinn_plugins::deliverable_from_a_snapshot(reading) {
            continue;
        }
        println!("CANARY RED on the daemon's own witnessed `{reading}`: {red:?}");
        assert!(
            red.iter()
                .any(|name| name.starts_with("no-transient-reading-from-a-snapshot")),
            "the surviving guard did not refuse a witnessed transient carried by an entry: \
             {red:?}"
        );
    }

    // And the reading law that answers the witness is the SAME law the
    // snapshot answers use — exercised here on the kernel's own recorded
    // state words from this very run, not on invented ones.
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
