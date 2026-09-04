//! The composition's SHAPE through the operator API, at pin `f8b285b`
//! (jinnd M2-K23, `jinn:profile-admin`; FINDINGS #37 closed by harness
//! pin-bump 10). Every proof boots the plugins profile through the REAL
//! pinned daemon (AGENTS.md standing order 3), issues ONE write through
//! the transport, and reads the two things the card names for it: the
//! `ProfileAdministered { write }` row under the CALLING entry, and the
//! live effect on the runtime and the document of record.
//!
//! The swap (the fifth write, the one FINDINGS #37 was filed on) lives
//! in `plugins.rs` under the proof that flipped and was renamed.

use composition::api::{delete, get, patch, post};
use composition::kit::{Daemon, LedgerRow};
use composition::plugins::{booted, booted_with, listing, PARKED};

const TRANSPORT: &str = "jinn-api-http";
const ADMIN_CONTRACT: &str = "jinn:profile-admin";
const FIXED_ID: &str = "jinn-plugins-appliance";
const FIXED_PACKAGE: &str = "plugins/jinn-plugins-static";
const ADDED_ID: &str = "jinn-plugins-added";
const ADDED_CATALOG: &str = "added";

// ------------------------------------------------------------ readings

/// One entry of the status report (`jinn:introspect` laid over the
/// document): id, package, hash, grants, fiber, state, incarnation.
fn status_entry(port: u16, id: &str) -> Option<serde_json::Value> {
    get(port, "/v1/status").body["entries"]
        .as_array()?
        .iter()
        .find(|entry| entry["id"] == id)
        .cloned()
}

/// The document of record's entry, through `GET /v1/profile`.
fn document_entry(port: u16, id: &str) -> Option<serde_json::Value> {
    get(port, "/v1/profile").body["profile"]["entries"]
        .as_array()?
        .iter()
        .find(|entry| entry["id"] == id)
        .cloned()
}

fn last_seq(daemon: &Daemon) -> u64 {
    daemon.ledger_rows().last().map_or(0, |row| row.seq)
}

/// The `ProfileAdministered` row an accepted answer names by sequence.
fn administered(daemon: &Daemon, seq: u64) -> (LedgerRow, serde_json::Value) {
    let row = daemon
        .ledger_rows()
        .into_iter()
        .find(|row| row.seq == seq)
        .unwrap_or_else(|| panic!("ledger row {seq}"));
    let (name, fields) = row.kind_of();
    assert_eq!(name, "ProfileAdministered", "{}", row.kind);
    (row, fields)
}

/// Whether `fiber` reached `to` after `after`, and with which cause.
fn transition_cause(daemon: &Daemon, after: u64, fiber: u64, to: &str) -> Option<String> {
    daemon.ledger_rows().iter().find_map(|row| {
        let (name, fields) = row.kind_of();
        (row.seq > after
            && name == "FiberTransition"
            && fields["fiber"] == fiber
            && fields["to"] == to)
            .then(|| fields["cause"].to_string())
    })
}

fn fiber_of(entry: &serde_json::Value) -> u64 {
    entry["fiber"]
        .as_u64()
        .unwrap_or_else(|| panic!("a fiber: {entry}"))
}

/// One write's transcript line and the assertions every accepted write
/// shares: 200, the write named, the row under the transport with the
/// write class and the administered entry, `by` the transport.
fn accepted(
    daemon: &Daemon,
    answer: &composition::api::Response,
    label: &str,
    write: &str,
    class: &str,
    id: &str,
) -> (u64, serde_json::Value) {
    println!("FINDINGS #37 transcript ({label})\n{}", answer.raw.trim());
    assert_eq!(answer.status, 200, "{}", answer.raw);
    assert_eq!(answer.body["write"], write, "{}", answer.raw);
    let seq = answer.body["administered-seq"]
        .as_u64()
        .unwrap_or_else(|| panic!("the row's sequence: {}", answer.raw));
    let (row, fields) = administered(daemon, seq);
    assert_eq!(
        row.entry.as_deref(),
        Some(TRANSPORT),
        "under the caller: {row:?}"
    );
    assert_eq!(fields["write"], class, "{fields}");
    assert_eq!(fields["entry"], id, "{fields}");
    assert_eq!(fields["by"], TRANSPORT, "{fields}");
    assert_ne!(
        fields["before"], fields["after"],
        "the document moved: {fields}"
    );
    (seq, fields)
}

// ------------------------------------------------------------ the writes

#[test]
fn add_entry_through_the_api_lands_the_row_and_the_entry_live() {
    let Some((daemon, port)) = booted("plugins-admin-add") else {
        return;
    };
    // A package a document-led reconcile already admitted (the bundle's
    // 0.1.0 limit): the appliance's own, under a NEW catalog name.
    let appliance = status_entry(port, FIXED_ID).expect("the appliance");
    let mut grants = appliance["grants"].as_array().cloned().expect("grants");
    grants.push(serde_json::json!(jinn_plugins::catalog_contract(
        ADDED_CATALOG
    )));
    let record = serde_json::json!({
        "id": ADDED_ID, "package": FIXED_PACKAGE, "version": "", "hash": appliance["hash"],
        "grants": grants,
        "config": { "data": { "catalog": ADDED_CATALOG, "ledger-limit": 16, "entries": [] } },
        "disabled": false, "parent": null
    });
    assert!(status_entry(port, ADDED_ID).is_none(), "not there before");

    let answer = post(port, "/v1/profile/entries", &record);
    let (_, fields) = accepted(&daemon, &answer, "add", "add-entry", "Add", ADDED_ID);
    assert!(fields["prior"].is_null(), "an add has no prior: {fields}");

    // LIVE: a new incarnation activates under the added id, and the
    // document of record holds the entry as written.
    daemon.eventually("the added entry to activate", || {
        status_entry(port, ADDED_ID).is_some_and(|entry| entry["state"] == "active")
    });
    let added = document_entry(port, ADDED_ID).expect("in the document");
    assert_eq!(added["package"], FIXED_PACKAGE, "{added}");
    assert_eq!(added["config"]["data"]["catalog"], ADDED_CATALOG, "{added}");
    daemon.interrupt();
}

#[test]
fn remove_entry_through_the_api_withdraws_it_on_the_record() {
    let Some((daemon, port)) = booted("plugins-admin-remove") else {
        return;
    };
    let before = status_entry(port, FIXED_ID).expect("the appliance");
    let old = fiber_of(&before);
    assert_eq!(listing(port, PARKED)["served-by"], FIXED_PACKAGE);
    let baseline = last_seq(&daemon);

    let answer = delete(port, &format!("/v1/profile/entries/{FIXED_ID}"));
    let (_, fields) = accepted(
        &daemon,
        &answer,
        "remove",
        "remove-entry",
        "Remove",
        FIXED_ID,
    );
    assert!(
        fields["prior"]
            .as_str()
            .is_some_and(|prior| prior.contains(FIXED_PACKAGE)),
        "the inverse write's payload rides on the row: {fields}"
    );

    // WITHDRAWN, on the record: the fiber rests Disposed, its provision
    // is withdrawn, and the document no longer holds the entry.
    daemon.eventually("the removed fiber to rest", || {
        transition_cause(&daemon, baseline, old, "Disposed").is_some()
    });
    let withdrawn = daemon.ledger_rows().iter().any(|row| {
        let (name, fields) = row.kind_of();
        row.seq > baseline
            && name == "ServiceWithdrawn"
            && fields["service"] == jinn_plugins::catalog_contract(PARKED)
    });
    assert!(
        withdrawn,
        "the catalog's provision was withdrawn on the record"
    );
    daemon.eventually("the document to drop the entry", || {
        document_entry(port, FIXED_ID).is_none() && status_entry(port, FIXED_ID).is_none()
    });
    let parked = get(port, &format!("/v1/plugins/{PARKED}"));
    assert_ne!(
        parked.status, 200,
        "nothing serves the parked catalog: {}",
        parked.raw
    );
    daemon.interrupt();
}

#[test]
fn set_disabled_through_the_api_disposes_then_spawns_and_self_administration_is_refused() {
    let Some((daemon, port)) = booted("plugins-admin-disable") else {
        return;
    };
    // The confinement, first: no plugin reshapes ITSELF — the transport
    // asking to disable its own entry is refused typed `unauthorized`.
    let own = patch(
        port,
        &format!("/v1/profile/entries/{TRANSPORT}"),
        &serde_json::json!({ "disabled": true }),
    );
    println!("FINDINGS #37 transcript (self)\n{}", own.raw.trim());
    assert_eq!(own.status, 502, "{}", own.raw);
    assert_eq!(own.body["error"]["code"], "refused", "{}", own.raw);
    assert_eq!(own.body["error"]["class"], "unauthorized", "{}", own.raw);

    let old = fiber_of(&status_entry(port, FIXED_ID).expect("the appliance"));
    let baseline = last_seq(&daemon);
    let off = patch(
        port,
        &format!("/v1/profile/entries/{FIXED_ID}"),
        &serde_json::json!({ "disabled": true }),
    );
    accepted(
        &daemon,
        &off,
        "disable",
        "set-disabled",
        "SetDisabled",
        FIXED_ID,
    );
    // A DISPOSAL: the fiber rests, the entry is kept, the flag persisted.
    daemon.eventually("the disabled fiber to rest", || {
        transition_cause(&daemon, baseline, old, "Disposed").is_some()
    });
    daemon.eventually("the document to carry disabled", || {
        document_entry(port, FIXED_ID).is_some_and(|entry| entry["disabled"] == true)
    });
    assert_ne!(
        status_entry(port, FIXED_ID).expect("kept")["state"],
        "active",
        "disabled is not active"
    );

    let on = patch(
        port,
        &format!("/v1/profile/entries/{FIXED_ID}"),
        &serde_json::json!({ "disabled": false }),
    );
    accepted(
        &daemon,
        &on,
        "enable",
        "set-disabled",
        "SetDisabled",
        FIXED_ID,
    );
    // A SPAWN: a fresh incarnation, a new fiber, the catalog served again.
    daemon.eventually("a fresh incarnation to activate", || {
        status_entry(port, FIXED_ID)
            .is_some_and(|entry| entry["state"] == "active" && entry["fiber"] != old)
    });
    daemon.eventually("the parked catalog to be served again", || {
        get(port, &format!("/v1/plugins/{PARKED}")).body["served-by"] == FIXED_PACKAGE
    });
    assert_eq!(
        document_entry(port, FIXED_ID).expect("kept")["disabled"],
        false
    );
    daemon.interrupt();
}

#[test]
fn set_grants_through_the_api_lands_only_via_the_restart() {
    let Some((daemon, port)) = booted("plugins-admin-grants") else {
        return;
    };
    let before = status_entry(port, FIXED_ID).expect("the appliance");
    let fiber = fiber_of(&before);
    let incarnation = before["incarnation"].as_u64().expect("an incarnation");
    let mut widened = before["grants"].as_array().cloned().expect("grants");
    assert!(!widened.contains(&serde_json::json!("jinn:clock")));
    widened.push(serde_json::json!("jinn:clock"));
    let restarts = daemon.config_restarts(fiber);

    let answer = patch(
        port,
        &format!("/v1/profile/entries/{FIXED_ID}"),
        &serde_json::json!({ "grants": widened }),
    );
    let (seq, _) = accepted(
        &daemon,
        &answer,
        "grants",
        "set-grants",
        "SetGrants",
        FIXED_ID,
    );

    // AN EPOCH INPUT: the grant lands through the entry's restart
    // (`ConfigChanged`), never live — the incarnation moves, and only
    // then does the runtime hold the widened list.
    daemon.eventually("the restart to land the grants", || {
        daemon.config_restarts(fiber) > restarts
    });
    daemon.eventually("the new incarnation to read active", || {
        status_entry(port, FIXED_ID).is_some_and(|entry| {
            entry["state"] == "active"
                && entry["incarnation"]
                    .as_u64()
                    .is_some_and(|now| now > incarnation)
        })
    });
    let after = status_entry(port, FIXED_ID).expect("the appliance");
    assert_eq!(
        after["grants"],
        serde_json::Value::Array(widened.clone()),
        "{after}"
    );
    assert_eq!(
        document_entry(port, FIXED_ID).expect("kept")["config"]["grants"],
        serde_json::Value::Array(widened)
    );
    let restart_after_row = daemon.ledger_rows().iter().any(|row| {
        row.seq > seq && row.fiber == Some(fiber) && row.kind.contains(r#""cause":"ConfigChanged""#)
    });
    assert!(
        restart_after_row,
        "the restart follows the intent row (Law 2)"
    );
    daemon.interrupt();
}

// ------------------------------------------------------------ the refusals

/// M2-K23 (d) and the grant: a transport WITHOUT `jinn:profile-admin`
/// cannot reshape the composition — every admin route answers a typed
/// `refused` — and a `grants` widening sent through the config route
/// (`jinn:profile.patch-entry`, 0.3.0) is refused by the kernel with
/// nothing written, an `AmendmentRefused` row on the record.
#[test]
fn an_entry_without_the_grant_is_refused_and_a_grants_widening_through_patch_entry_is_refused() {
    let Some((daemon, port)) = booted_with("plugins-admin-ungranted", |document| {
        let transport = composition::kit::entry_mut(document, TRANSPORT);
        let grants = transport["config"]["grants"]
            .as_array_mut()
            .expect("grants");
        let before = grants.len();
        grants.retain(|grant| grant["contract"] != ADMIN_CONTRACT);
        assert_eq!(
            grants.len(),
            before - 1,
            "the admin grant was there to strip"
        );
    }) else {
        return;
    };
    let before = status_entry(port, FIXED_ID).expect("the appliance");
    let baseline = last_seq(&daemon);

    let ungranted = patch(
        port,
        &format!("/v1/profile/entries/{FIXED_ID}"),
        &serde_json::json!({ "disabled": true }),
    );
    println!(
        "FINDINGS #37 transcript (ungranted)\n{}",
        ungranted.raw.trim()
    );
    assert_eq!(ungranted.status, 502, "{}", ungranted.raw);
    assert_eq!(
        ungranted.body["error"]["code"], "refused",
        "{}",
        ungranted.raw
    );
    assert!(
        ungranted.body["error"]["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains(ADMIN_CONTRACT)),
        "the refusal names the contract the caller does not hold: {}",
        ungranted.raw
    );
    // Nothing written anywhere: the entry is as it was, no admin row.
    assert_eq!(
        status_entry(port, FIXED_ID).expect("kept")["state"],
        "active"
    );
    assert_eq!(
        daemon
            .ledger_rows()
            .iter()
            .filter(|row| row.seq > baseline && row.kind.contains("ProfileAdministered"))
            .count(),
        0
    );

    // (d): grants through the CONFIG route — the harness's entry-patch
    // law forwards them as a merge on `config`, and the 0.3.0 kernel
    // refuses a patch whose grants differ from the committed ones.
    let mut widened = before["grants"].as_array().cloned().expect("grants");
    widened.push(serde_json::json!("jinn:clock"));
    let through_config = patch(
        port,
        &format!("/v1/profile/entries/{FIXED_ID}"),
        &serde_json::json!({ "config": { "grants": widened } }),
    );
    println!("FINDINGS #37 transcript (d)\n{}", through_config.raw.trim());
    assert_eq!(through_config.status, 502, "{}", through_config.raw);
    assert_eq!(
        through_config.body["error"]["code"], "refused",
        "{}",
        through_config.raw
    );
    assert!(
        through_config.body["error"]["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("grants")),
        "the kernel's reason names grants: {}",
        through_config.raw
    );
    daemon.eventually("the refusal on the ledger", || {
        daemon
            .ledger_rows()
            .iter()
            .any(|row| row.seq > baseline && row.kind.contains("AmendmentRefused"))
    });
    let after = status_entry(port, FIXED_ID).expect("kept");
    assert_eq!(
        after["grants"], before["grants"],
        "nothing written: {after}"
    );
    assert_eq!(
        after["incarnation"], before["incarnation"],
        "no restart: {after}"
    );
    daemon.interrupt();
}
