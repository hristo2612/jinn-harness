//! The plugins seam through the REAL pinned daemon (AGENTS.md standing
//! order 3). Every proof here boots a profile through the loader; a
//! hand-mounted catalog would prove nothing about the machine.
//!
//! # What "non-vacuous" means in this file
//!
//! Every honesty assertion is preceded by an assertion of the
//! PRECONDITION that makes it meaningful — most often that the reader
//! CAN produce the answer being ruled out. A test that says "this entry
//! does not read `active`" over a reader that can never say `active` is
//! indistinguishable from one that passes.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::OnceLock;

use composition::api::{get, patch, Response};
use composition::daemon::{jinnd_source, pinned_commit, pinned_daemon};
use composition::kit::{fresh_plugins_root, Daemon};

const LIVE_ID: &str = "jinn-plugins-live";
const FIXED_ID: &str = "jinn-plugins-appliance";
const SHELVED_ID: &str = "jinn-plugins-shelf";
const FAILING_ID: &str = "jinn-api-http-misbound";
const API_ID: &str = "jinn-api-http";
const MAIN: &str = "main";
const PARKED: &str = "parked";

const LIVE_PACKAGE: &str = "plugins/jinn-plugins-profile";
const FIXED_PACKAGE: &str = "plugins/jinn-plugins-static";

/// The pinned daemon binary, or a LOUD skip.
fn gate() -> Option<&'static PathBuf> {
    static BINARY: OnceLock<Option<PathBuf>> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            let commit = pinned_commit().expect("KERNEL-PIN.md parses");
            let Some(source) = jinnd_source(&commit) else {
                eprintln!(
                    "SKIPPED (loudly): real-composition gate found no jinnd checkout holding \
                     pinned commit {commit} — set JINND_DIR, add a sibling ../jinnd, or set \
                     JINND_CLONE_URL (KERNEL-PIN.md Gate 2 discipline)"
                );
                return None;
            };
            Some(pinned_daemon(&source, &commit).expect("the pinned daemon builds"))
        })
        .as_ref()
}

fn booted(name: &str) -> Option<(Daemon, u16)> {
    let binary = gate()?;
    let (root, port) = fresh_plugins_root(name);
    let daemon = Daemon::boot_operator(binary, &root);
    daemon.await_ready();
    let health = get(port, "/v1/health");
    assert_eq!(health.status, 200, "{}", health.raw);
    Some((daemon, port))
}

/// One catalog's listing.
fn listing(port: u16, catalog: &str) -> serde_json::Value {
    let read = get(port, &format!("/v1/plugins/{catalog}"));
    assert_eq!(read.status, 200, "{}", read.raw);
    read.body
}

/// One entry out of a listing.
fn entry(listing: &serde_json::Value, id: &str) -> serde_json::Value {
    listing["entries"]
        .as_array()
        .unwrap_or_else(|| panic!("a listing: {listing}"))
        .iter()
        .find(|entry| entry["id"] == id)
        .unwrap_or_else(|| panic!("entry {id:?} in the listing: {listing}"))
        .clone()
}

fn described(port: u16, catalog: &str, id: &str) -> Response {
    get(port, &format!("/v1/plugins/{catalog}/{id}"))
}

fn history(port: u16, catalog: &str, id: &str) -> serde_json::Value {
    let read = get(port, &format!("/v1/plugins/{catalog}/{id}/history"));
    assert_eq!(read.status, 200, "{}", read.raw);
    read.body
}

/// The `state` an entry reads as.
fn state(entry: &serde_json::Value) -> String {
    entry["lifecycle"]["state"]
        .as_str()
        .unwrap_or_else(|| panic!("a lifecycle: {entry}"))
        .to_owned()
}

// ------------------------------------------------------------ legibility

#[test]
fn the_catalog_reports_the_plugin_tree_with_every_entrys_grants_and_life() {
    let Some((daemon, port)) = booted("plugins-legible") else {
        return;
    };
    let listed = listing(port, MAIN);
    assert_eq!(listed["catalog"], MAIN, "{listed}");
    assert_eq!(listed["served-by"], LIVE_PACKAGE, "{listed}");

    // Precondition: the tree is not empty and holds the entries this
    // profile mounts — otherwise every claim below is vacuous.
    let ids: BTreeSet<String> = listed["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .map(|entry| entry["id"].as_str().expect("an id").to_owned())
        .collect();
    for expected in [API_ID, LIVE_ID, FIXED_ID, SHELVED_ID, FAILING_ID] {
        assert!(ids.contains(expected), "{expected} missing from {ids:?}");
    }

    // The API is serving, and the catalog says so with its provisions —
    // the binding, read from the kernel rather than from a package name.
    let api = entry(&listed, API_ID);
    assert_eq!(state(&api), "active", "{api}");
    assert_eq!(api["package"], "api/jinn-api-http", "{api}");

    // A plugin's authority is readable without reading the profile by
    // hand, and it says it IS the authority.
    let grants = api["grants"].clone();
    assert_eq!(grants["source"], "profile-document", "{api}");
    assert!(
        grants["qualifier"]
            .as_str()
            .is_some_and(|text| text.contains("the authority the kernel enforces")),
        "{api}"
    );
    let contracts: BTreeSet<String> = grants["values"]
        .as_array()
        .expect("values")
        .iter()
        .map(|grant| grant["contract"].as_str().expect("contract").to_owned())
        .collect();
    assert!(contracts.contains("jinn:net"), "{api}");
    assert!(contracts.contains("jinn:plugins.main"), "{api}");
    daemon.interrupt();
}

#[test]
fn a_plugin_granted_nothing_reports_nothing_and_the_reader_can_report_grants() {
    let Some((daemon, port)) = booted("plugins-grants") else {
        return;
    };
    // The appliance catalog DECLARES a grant-less entry. Its own entry in
    // the live catalog holds grants, which is the precondition: a reader
    // that answered `[]` for everything would otherwise pass this.
    let live = listing(port, MAIN);
    let fixed_entry = entry(&live, FIXED_ID);
    assert!(
        !fixed_entry["grants"]["values"]
            .as_array()
            .expect("values")
            .is_empty(),
        "the reader must be able to report grants at all: {fixed_entry}"
    );

    let declared = listing(port, PARKED);
    assert_eq!(declared["served-by"], FIXED_PACKAGE, "{declared}");
    let bare = entry(&declared, "jinn-status");
    assert!(
        bare["grants"]["values"]
            .as_array()
            .expect("values")
            .is_empty(),
        "a plugin declared with no grants reports none: {bare}"
    );
    // And it says its word is a CLAIM, not the authority — the narrower
    // guarantee travels in the answer the consumer reads.
    assert_eq!(bare["grants"]["source"], "catalog-declaration", "{bare}");
    assert!(
        bare["grants"]["qualifier"]
            .as_str()
            .is_some_and(|text| text.contains("NOT read from the document of record")),
        "{bare}"
    );
    daemon.interrupt();
}

#[test]
fn every_answer_carries_the_window_and_the_qualifier_that_bounds_it() {
    let Some((daemon, port)) = booted("plugins-qualifier") else {
        return;
    };
    let listed = listing(port, MAIN);
    // Precondition: the ledger really was read — a window over an empty
    // scan would make the qualifier decorative.
    let window = listed["read"]["ledger"].clone();
    assert!(
        window["scanned"]
            .as_u64()
            .is_some_and(|scanned| scanned > 0),
        "the window records a read that happened: {listed}"
    );
    assert!(window["to"].as_u64().is_some_and(|to| to > 0), "{listed}");
    assert_eq!(
        listed["read"]["qualifier"],
        jinn_plugins::JOIN_QUALIFIER,
        "the join's narrowness travels in the answer, from its one home"
    );

    let told = history(port, MAIN, API_ID);
    assert_eq!(told["qualifier"], jinn_plugins::history::HISTORY_QUALIFIER);
    assert!(told["window"]["scanned"].as_u64().is_some_and(|n| n > 0));
    daemon.interrupt();
}

// ------------------------------------------------------- lifecycle honesty

#[test]
fn an_entry_with_no_incarnation_never_reads_active_and_says_why() {
    let Some((daemon, port)) = booted("plugins-no-incarnation") else {
        return;
    };
    let listed = listing(port, MAIN);
    // Precondition: this catalog CAN answer `active` — proven on an entry
    // that is genuinely serving — so the ruling-out below is about the
    // shelved entry and not about a reader that never says `active`.
    assert_eq!(state(&entry(&listed, API_ID)), "active");

    let shelved = entry(&listed, SHELVED_ID);
    assert_ne!(state(&shelved), "active", "{shelved}");
    assert_ne!(state(&shelved), "activating", "{shelved}");
    assert_eq!(state(&shelved), "no-incarnation", "{shelved}");
    // And the reason is a POSITIVE reading of the document, not a guess
    // and not a sentinel.
    assert_eq!(
        shelved["lifecycle"]["reason"]["from"], "disabled",
        "{shelved}"
    );
    daemon.interrupt();
}

#[test]
fn a_failed_activation_reports_failed_with_a_reason_and_never_unknown() {
    let Some((daemon, port)) = booted("plugins-failed") else {
        return;
    };
    let listed = listing(port, MAIN);
    // Precondition, again: `active` is reachable, so `failed` below is a
    // reading and not this catalog's only answer.
    assert_eq!(state(&entry(&listed, API_ID)), "active");

    let failed = entry(&listed, FAILING_ID);
    assert_eq!(
        state(&failed),
        "failed",
        "an entry whose bind the broker refused is failed: {failed}"
    );
    let reason = failed["lifecycle"]["reason"].clone();
    // There is no `unknown` in this vocabulary at all, and no default:
    // the reason names WHERE it came from.
    assert!(
        ["ledgered", "not-found-in-window", "composition", "disabled"]
            .contains(&reason["from"].as_str().unwrap_or_default()),
        "a failure's reason names its source: {failed}"
    );
    assert_ne!(reason["from"], "unknown", "{failed}");
    // The kernel records the bind refusal, so the reason IS ledgered and
    // carries the kernel's own prose. If a future pin stops recording it,
    // this assertion fails LOUDLY rather than the seam quietly inventing
    // a sentence (FINDINGS.md #37).
    assert_eq!(
        reason["from"], "ledgered",
        "the refusal the kernel recorded is the reason this seam reports: {failed}"
    );
    assert!(
        reason["detail"]
            .as_str()
            .is_some_and(|detail| !detail.trim().is_empty()),
        "a failure nobody can explain: {failed}"
    );
    // And a reason that had NOT been found would still carry the window
    // it searched — the narrower answer names its own bound.
    assert!(failed["lifecycle"]["reason"].is_object(), "{failed}");
    daemon.interrupt();
}

#[test]
fn a_plugin_the_catalog_names_but_the_machine_does_not_run_is_not_mounted() {
    let Some((daemon, port)) = booted("plugins-not-mounted") else {
        return;
    };
    let declared = listing(port, PARKED);
    // Precondition: this catalog reports a MOUNTED entry as active, so
    // `not-mounted` below is a reading of the machine and not a blanket
    // answer from a catalog that cannot see the composition.
    assert_eq!(state(&entry(&declared, API_ID)), "active", "{declared}");

    let absent = entry(&declared, "a-plugin-this-appliance-was-built-with");
    assert_eq!(state(&absent), "not-mounted", "{absent}");
    // And the catalog names what the MACHINE runs that it does not
    // declare, rather than quietly omitting the difference.
    let unlisted = declared["unlisted"].as_array().expect("unlisted");
    assert!(
        unlisted.iter().any(|id| id == LIVE_ID),
        "the difference between the appliance and the machine is named: {declared}"
    );
    daemon.interrupt();
}

// ---------------------------------------------------------- attribution

#[test]
fn a_history_holds_that_plugins_lines_and_only_its_own() {
    let Some((daemon, port)) = booted("plugins-attribution") else {
        return;
    };
    let mine = history(port, MAIN, API_ID);
    let theirs = history(port, MAIN, LIVE_ID);
    // Precondition: BOTH plugins have lines in the window, so "only its
    // own" is not vacuously true of a single-tenant ledger.
    let mine_lines = mine["lines"].as_array().expect("lines");
    let their_lines = theirs["lines"].as_array().expect("lines");
    assert!(!mine_lines.is_empty(), "{mine}");
    assert!(!their_lines.is_empty(), "{theirs}");

    assert!(
        mine_lines.iter().all(|line| line["entry"] == API_ID),
        "a history holds only its own: {mine}"
    );
    assert!(
        their_lines.iter().all(|line| line["entry"] == LIVE_ID),
        "{theirs}"
    );
    daemon.interrupt();
}

#[test]
fn a_disposed_plugins_history_survives_its_disposal() {
    let Some((daemon, port)) = booted("plugins-disposal") else {
        return;
    };
    // The subject is an entry that RAN: a plugin that never activated has
    // no lines to lose, so disposing it would prove nothing. (The first
    // draft of this proof used the DISABLED entry and its precondition
    // caught exactly that — its history is empty, because a disabled
    // entry is never charged a single ledger line.)
    let before = history(port, MAIN, FIXED_ID);
    let had = before["lines"].as_array().expect("lines").len();
    assert!(had > 0, "nothing to lose: {before}");
    assert_eq!(described(port, MAIN, FIXED_ID).status, 200);
    // And a disabled entry's history really is empty — stated here so the
    // subject swap above is a recorded fact rather than a quiet edit.
    assert!(
        history(port, MAIN, SHELVED_ID)["lines"]
            .as_array()
            .expect("lines")
            .is_empty(),
        "a plugin that never activated is charged no ledger line"
    );

    // Remove it from the document of record entirely.
    daemon.edit_profile(|document| {
        let entries = document["entries"].as_array_mut().expect("entries");
        entries.retain(|entry| entry["id"] != FIXED_ID);
    });
    daemon.eventually("the entry to leave the catalog", || {
        described(port, MAIN, FIXED_ID).status == 404
    });

    // The ledger is append-only, so its lines are still there — because
    // a history asks the LEDGER and not the fiber.
    let after = history(port, MAIN, FIXED_ID);
    let kept = after["lines"].as_array().expect("lines").len();
    assert!(
        kept >= had,
        "a disposed plugin's history must survive its disposal: {had} -> {kept}\n{after}"
    );
    assert!(after["lines"]
        .as_array()
        .expect("lines")
        .iter()
        .all(|line| line["entry"] == FIXED_ID));
    daemon.interrupt();
}

// -------------------------------------------------- the malleability contract

#[test]
fn the_catalog_provider_swaps_through_the_api_with_the_layer_above_untouched() {
    let Some((daemon, port)) = booted("plugins-api-swap") else {
        return;
    };
    // BEFORE: the live catalog answers `main`, and the fixed one waits on
    // `parked`.
    assert_eq!(listing(port, MAIN)["served-by"], LIVE_PACKAGE);
    assert_eq!(listing(port, PARKED)["served-by"], FIXED_PACKAGE);
    // The API's own incarnation, so "the layer above is untouched" is a
    // measured fact and not an impression.
    let api_before = entry(&listing(port, MAIN), API_ID)["lifecycle"].clone();
    let incarnation_before = incarnation(port, API_ID);
    assert!(
        incarnation_before.is_some(),
        "the API is running: {api_before}"
    );

    // THE SWAP, THROUGH THE API — two profile patches, no file edit.
    // Park the incumbent first: the kernel holds one provider slot per
    // contract name, so claiming an occupied one refuses at `provide`.
    let parked = patch(
        port,
        &format!("/v1/profile/entries/{LIVE_ID}"),
        &serde_json::json!({ "config": { "data": { "catalog": "unbound" } } }),
    );
    assert_eq!(parked.status, 200, "{}", parked.raw);
    let claimed = patch(
        port,
        &format!("/v1/profile/entries/{FIXED_ID}"),
        &serde_json::json!({ "config": { "data": { "catalog": MAIN } } }),
    );
    assert_eq!(claimed.status, 200, "{}", claimed.raw);

    // AFTER: the same catalog id is answered by the other package.
    daemon.eventually("the fixed catalog to take the switchable name", || {
        get(port, &format!("/v1/plugins/{MAIN}")).body["served-by"] == FIXED_PACKAGE
    });
    let after = listing(port, MAIN);
    assert_eq!(after["served-by"], FIXED_PACKAGE, "{after}");
    assert_eq!(after["catalog"], MAIN, "{after}");
    // The new binding brings its own entry set AND its own qualifier: an
    // operator reading the swapped catalog is told its grants are now a
    // claim rather than the authority.
    assert_eq!(
        entry(&after, "jinn-status")["grants"]["source"],
        "catalog-declaration",
        "{after}"
    );

    // THE LAYER ABOVE IS UNTOUCHED. Not "still answers" — the SAME
    // incarnation, so the API demonstrably did not restart across the
    // swap.
    assert_eq!(
        incarnation(port, API_ID),
        incarnation_before,
        "the API restarted across the swap, so it was not untouched"
    );
    // And it stayed up throughout: every other surface still answers.
    for surface in ["/v1/status", "/v1/health", "/v1/profile"] {
        assert_eq!(get(port, surface).status, 200, "{surface} went down");
    }
    daemon.interrupt();
}

/// One entry's live incarnation, read through the catalog itself.
fn incarnation(port: u16, id: &str) -> Option<u64> {
    let described = get(port, &format!("/v1/plugins/{MAIN}/{id}"));
    if described.status != 200 {
        return None;
    }
    described.body["provides"].as_array()?;
    // The catalog reports provisions, not the raw incarnation; the
    // kernel's own number comes from the status surface, which reads
    // `jinn:introspect` directly.
    let status = get(port, "/v1/status");
    status.body["entries"]
        .as_array()?
        .iter()
        .find(|entry| entry["id"] == id)?
        .get("incarnation")?
        .as_u64()
}

#[test]
fn a_catalog_this_api_was_not_granted_is_a_404_and_an_unreadable_read_names_its_contract() {
    let Some((daemon, port)) = booted("plugins-refusals") else {
        return;
    };
    let missing = get(port, "/v1/plugins/not-a-catalog");
    assert_eq!(missing.status, 404, "{}", missing.raw);

    // Precondition: a catalog that IS granted answers, so the 404 above
    // is about the grant and not about the surface being broken.
    assert_eq!(get(port, &format!("/v1/plugins/{MAIN}")).status, 200);

    // The catalog list names every catalog this API may route to, with
    // each one's own word about itself.
    let catalogs = get(port, "/v1/plugins");
    assert_eq!(catalogs.status, 200, "{}", catalogs.raw);
    let listed = catalogs.body["catalogs"].as_array().expect("catalogs");
    assert_eq!(listed.len(), 2, "{}", catalogs.raw);
    assert!(listed
        .iter()
        .any(|entry| entry["catalog"] == MAIN && entry["contract"] == "jinn:plugins.main"));

    // A plugin that is not in the catalog is a typed 404, never an empty
    // answer that could be read as "it exists and has nothing".
    let absent = described(port, MAIN, "no-such-plugin");
    assert_eq!(absent.status, 404, "{}", absent.raw);
    assert_eq!(
        absent.body["error"]["catalog-code"], "not-found",
        "{}",
        absent.raw
    );
    daemon.interrupt();
}

#[test]
fn describe_says_what_a_plugin_may_do_what_it_has_done_and_what_may_happen_next() {
    let Some((daemon, port)) = booted("plugins-describe") else {
        return;
    };
    let described = described(port, MAIN, API_ID);
    assert_eq!(described.status, 200, "{}", described.raw);
    let body = described.body;
    // Precondition: this really is the serving API entry.
    assert_eq!(body["lifecycle"]["state"], "active", "{body}");
    // What it MAY do.
    let effects = body["declared-effects"].as_array().expect("effects");
    assert!(
        effects.iter().any(|effect| effect
            .as_str()
            .is_some_and(|text| text.starts_with("may call jinn:net"))),
        "{body}"
    );
    // What it HAS done, within the window, and only its own.
    let done = body["done"].as_object().expect("done");
    assert!(!done.is_empty(), "{body}");
    // What may happen NEXT, from the seam's transition table.
    let next = body["legal-next"].as_array().expect("legal-next");
    assert!(next.iter().any(|state| state == "restarting"), "{body}");
    assert!(
        !next.iter().any(|state| state == "not-mounted"),
        "an active entry does not become not-mounted in one step: {body}"
    );
    daemon.interrupt();
}
