//! The operator-API seam's real-composition gate (AGENTS.md standing
//! order 3): every proof boots the operator profile — the api trio beside
//! the cron seam — through the REAL pinned `jinnd` daemon in the operator
//! layout (`--data <root>`, so the profile document sits inside the
//! `jinn:fs` surface the consumers are scoped to), and drives the API as
//! an operator would: plain HTTP on loopback, evidence from the ledger,
//! the daemon log, and the profile document of record.
//!
//! Self-skips LOUDLY when no jinnd checkout holding the pinned commit is
//! reachable (KERNEL-PIN.md Gate 2).

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use composition::api::{get, listening, patch};
use composition::daemon::{jinnd_source, pinned_commit, pinned_daemon};
use composition::kit::{
    entry_config, free_port, fresh_api_root, ledger_rows_at, set_provider_port, Daemon, LedgerRow,
    JOB_PERIOD_MS,
};

const PROVIDER: &str = "jinn-api-http";
const STATUS: &str = "jinn-status";
const EDITOR: &str = "jinn-profile-edit";
/// Every entry of the operator profile: the cron seam, the api trio and
/// the settings pair (`profiles/operator-api/README.md`).
const ALL: [&str; 7] = [
    "cron-scheduler",
    "health-snapshot",
    PROVIDER,
    STATUS,
    EDITOR,
    "jinn-settings-profile",
    "jinn-settings-store",
];

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

/// Boots a fresh operator root and waits for readiness AND the API's
/// first answer (served from the listener's readiness wake).
fn booted(name: &str) -> Option<(Daemon, u16)> {
    let binary = gate()?;
    let (root, port) = fresh_api_root(name);
    let daemon = Daemon::boot_operator(binary, &root);
    daemon.await_ready();
    let health = get(port, "/v1/health");
    assert_eq!(health.status, 200, "{}", health.raw);
    Some((daemon, port))
}

/// The fiber ids that ever became Active, in first-activation order, as
/// the ledger records them (`FiberTransition … to Active`).
fn active_fibers(rows: &[LedgerRow]) -> Vec<u64> {
    let mut seen = Vec::new();
    for row in rows {
        if row.kind.contains(r#""to":"Active""#) {
            if let Some(fiber) = row.fiber {
                if !seen.contains(&fiber) {
                    seen.push(fiber);
                }
            }
        }
    }
    seen
}

/// The fiber that provided `service` (the attribution of its
/// `ServiceProvided` row).
fn provider_fiber(rows: &[LedgerRow], service: &str) -> u64 {
    rows.iter()
        .find(|row| row.kind == format!(r#"{{"ServiceProvided":{{"service":"{service}"}}}}"#))
        .and_then(|row| row.fiber)
        .unwrap_or_else(|| panic!("{service} was provided by a fiber"))
}

#[test]
fn status_health_and_ledger_tail_answer_through_the_api() {
    let Some((daemon, port)) = booted("api-status") else {
        return;
    };
    // `status`: the entries of record (all five, with their authority
    // fields from the document AND the kernel's own view of each through
    // `jinn:introspect` — fiber, state, incarnation, provisions,
    // registrations; FINDINGS.md #19 closed), the cron probe LIVE
    // through a granted `jinn:cron` call, the daemon's readiness, and
    // the ledger's high-water mark through `jinn:ledger` (#20 closed).
    // Nothing is left in `kernel.unavailable`.
    let status = get(port, "/v1/status");
    assert_eq!(status.status, 200, "{}", status.raw);
    let report = &status.body;
    assert_eq!(report["api-version"], jinn_api::API_VERSION);
    let entries = report["entries"].as_array().expect("entries");
    let ids: BTreeSet<&str> = entries
        .iter()
        .filter_map(|entry| entry["id"].as_str())
        .collect();
    assert_eq!(ids, ALL.iter().copied().collect(), "{report}");
    let scheduler = entries
        .iter()
        .find(|entry| entry["id"] == "cron-scheduler")
        .expect("the scheduler entry");
    assert_eq!(scheduler["package"], "cron/cron-scheduler", "{scheduler}");
    assert!(
        scheduler["hash"]
            .as_str()
            .is_some_and(|hash| hash.len() == 64),
        "{scheduler}"
    );
    assert!(
        scheduler["grants"]
            .as_array()
            .is_some_and(|grants| grants.contains(&serde_json::json!("jinn:clock"))),
        "{scheduler}"
    );
    assert_eq!(scheduler["state"], "active", "{scheduler}");
    assert!(scheduler["fiber"].is_u64(), "{scheduler}");
    assert!(scheduler["incarnation"].is_u64(), "{scheduler}");
    assert_eq!(
        scheduler["provisions"],
        serde_json::json!(["jinn:cron"]),
        "{scheduler}"
    );
    // The periodic alarm, plus the one-shot settings alarm while the
    // kernel still counts it (a fired `alarm-at` stays a registration of
    // the seat until the fiber releases it).
    assert!(
        scheduler["registrations"]["alarms"]
            .as_u64()
            .is_some_and(|alarms| (1..=2).contains(&alarms)),
        "{scheduler}"
    );
    let http = entries
        .iter()
        .find(|entry| entry["id"] == PROVIDER)
        .expect("the provider entry");
    assert_eq!(http["state"], "active", "{http}");
    assert_eq!(
        http["registrations"]["alarms"], 0,
        "the HTTP provider holds NO alarm (FINDINGS.md #23 closed): {http}"
    );
    assert!(
        http["registrations"]["sockets"]
            .as_u64()
            .is_some_and(|n| n >= 1),
        "the listener is a kernel registration: {http}"
    );
    assert!(
        entries.iter().all(|entry| entry["state"] == "active"),
        "every entry active: {report}"
    );
    let probe = &report["probes"][0];
    assert_eq!(probe["contract"], "jinn:cron", "{report}");
    assert_eq!(probe["live"], true, "{report}");
    assert_eq!(probe["answer"]["jobs"][0]["id"], "health", "{report}");
    assert!(
        probe["answer"]["jobs"][0]["next-ms"]
            .as_u64()
            .is_some_and(|next| next % JOB_PERIOD_MS == 0),
        "the schedule as held: {report}"
    );
    assert_eq!(
        report["kernel"]["unavailable"],
        serde_json::json!([]),
        "nothing is unavailable at this pin: {report}"
    );
    assert_eq!(
        report["readiness"],
        serde_json::json!({ "boot-reconciled": true, "watcher-armed": true }),
        "{report}"
    );
    assert!(
        report["last-ledger-seq"]
            .as_u64()
            .is_some_and(|seq| seq > 0),
        "{report}"
    );
    assert_eq!(report["document"]["readable"], true, "{report}");

    // `health`: every entry Active by the kernel's word, the probe live.
    let health = get(port, "/v1/health");
    assert_eq!(health.body["ok"], true, "{}", health.raw);
    assert_eq!(health.body["entries"], ALL.len(), "{}", health.raw);
    assert_eq!(health.body["probes-live"], 1, "{}", health.raw);

    // `ledger-tail`: a REAL page through the granted reader — events
    // with id > after, at most limit, typed records with a sensitivity
    // tag, `next-after` set for a further page; the read is receipted
    // on the ledger under the status entry (`LedgerConsumed`).
    let tail = get(port, "/v1/ledger/tail?after=7&limit=3");
    assert_eq!(tail.status, 200, "{}", tail.raw);
    assert_eq!(tail.body["after"], 7, "{}", tail.raw);
    assert_eq!(tail.body["limit"], 3, "{}", tail.raw);
    let events = tail.body["events"].as_array().expect("events");
    assert_eq!(events.len(), 3, "{}", tail.raw);
    assert_eq!(events[0]["id"], 8, "{}", tail.raw);
    assert!(events[0]["kind"].is_string(), "{}", tail.raw);
    assert!(
        matches!(
            events[0]["sensitivity"].as_str(),
            Some("public" | "personal")
        ),
        "{}",
        tail.raw
    );
    assert_eq!(tail.body["next-after"], 10, "{}", tail.raw);
    assert!(tail.body["unavailable"].is_null(), "{}", tail.raw);
    let clamped = get(port, "/v1/ledger/tail?limit=99999");
    assert_eq!(
        clamped.body["limit"],
        jinn_api::LEDGER_TAIL_MAX_LIMIT,
        "{}",
        clamped.raw
    );
    let last = report["last-ledger-seq"].as_u64().expect("seq");
    let beyond = get(port, &format!("/v1/ledger/tail?after={}", last + 1000));
    assert_eq!(
        beyond.body["events"],
        serde_json::json!([]),
        "{}",
        beyond.raw
    );
    assert!(beyond.body["next-after"].is_null(), "{}", beyond.raw);

    // The transport's own refusals are typed too.
    let missing = get(port, "/nope");
    assert_eq!(missing.status, 404, "{}", missing.raw);
    assert_eq!(
        missing.body["error"]["code"], "not-found",
        "{}",
        missing.raw
    );
    assert_eq!(
        missing.body["api-version"],
        jinn_api::API_VERSION,
        "an error answer is versioned: {}",
        missing.raw
    );
    let wrong_method = get(port, "/v1/profile/entries/cron-scheduler");
    assert_eq!(wrong_method.status, 405, "{}", wrong_method.raw);

    // Law 2: the listen, every accept, every readiness wake, and every
    // request's contract crossing are ledger events attributed to the
    // provider's ENTRY and fiber; the kernel reads are the status
    // entry's crossings with their receipts; and the provider's fiber
    // never woke on an alarm. (Eight requests so far; the ledger writer
    // is asynchronous, so wait for the last close to land.)
    daemon.eventually("the last request's close to land on the ledger", || {
        daemon.ledger_count("NetClosed") >= 8
    });
    let rows = daemon.ledger_rows();
    let kinds: Vec<&str> = rows.iter().map(|row| row.kind.as_str()).collect();
    let joined = kinds.join("\n");
    assert!(
        joined.contains(&format!(r#"NetListening":{{"handle":1,"port":{port}}}"#)),
        "{joined}"
    );
    assert!(
        joined.matches("NetAccepted").count() >= 8,
        "one accept per request: {joined}"
    );
    assert!(
        joined.matches("NetReadable").count() >= 8,
        "one wake per readiness transition: {joined}"
    );
    for operation in ["status", "health", "ledger-tail"] {
        assert!(
            joined.contains(&format!(
                r#"ContractCall":{{"contract":"jinn:api-status","operation":"{operation}"}}"#
            )),
            "{operation} is a ledgered contract call: {joined}"
        );
    }
    for (contract, operation) in [
        ("jinn:cron", "jobs"),
        ("jinn:introspect", "entries"),
        ("jinn:introspect", "readiness"),
        ("jinn:ledger", "last-seq"),
        ("jinn:ledger", "read-range"),
    ] {
        // (The cron probe is also the health consumer's boot peek: the
        // status entry's own crossing is the one attributed to it.)
        assert!(
            rows.iter().any(|row| {
                row.entry.as_deref() == Some(STATUS)
                    && row.kind
                        == format!(
                            r#"{{"ContractCall":{{"contract":"{contract}","operation":"{operation}"}}}}"#
                        )
            }),
            "{contract}/{operation} is a granted call attributed to the status ENTRY: {joined}"
        );
    }
    assert!(
        rows.iter()
            .any(|row| row.entry.as_deref() == Some(STATUS) && row.kind.contains("LedgerConsumed")),
        "the ledger read is receipted under the reader: {joined}"
    );
    let listener = rows
        .iter()
        .find(|row| row.kind.contains("NetListening"))
        .expect("the listen is attributed");
    assert_eq!(listener.entry.as_deref(), Some(PROVIDER), "{listener:?}");
    let listener_fiber = listener.fiber.expect("attributed");
    assert!(
        rows.iter()
            .filter(|row| row.kind.contains(r#""contract":"jinn:api-status""#))
            .all(|row| row.fiber == Some(listener_fiber)),
        "every api call is the HTTP provider's crossing: {joined}"
    );
    assert!(
        !rows
            .iter()
            .any(|row| row.fiber == Some(listener_fiber) && row.kind.contains("AlarmWake")),
        "the provider polls nothing: {joined}"
    );
    daemon.interrupt();
}

#[test]
fn patching_one_entry_through_the_api_restarts_exactly_that_fiber() {
    let Some((daemon, port)) = booted("api-patch") else {
        return;
    };
    let before = daemon.ledger_rows();
    let fibers_before = active_fibers(&before);
    assert_eq!(fibers_before.len(), ALL.len(), "every entry active at boot");
    let editor = provider_fiber(&before, "jinn:api-profile");
    let last_seq = before.last().map_or(0, |row| row.seq);

    // The operator patch: halve the health job's period on ONE entry.
    // The request is one granted `jinn:api-profile` call; the editor
    // hands the patch to the kernel's `jinn:profile`, whose LOADER
    // validates it, writes the document back and restarts exactly the
    // patched fiber (pin `57360cc`).
    let halved = JOB_PERIOD_MS / 2;
    let answer = patch(
        port,
        "/v1/profile/entries/cron-scheduler",
        &serde_json::json!({ "config": { "data": { "jobs": [
            { "id": "health", "every-ms": halved, "topic": "cron:health" } ] } } }),
    );
    assert_eq!(answer.status, 200, "{}", answer.raw);
    assert_eq!(answer.body["changed"], true, "{}", answer.raw);
    assert_eq!(
        answer.body["entry"]["config"]["data"]["jobs"][0]["every-ms"], halved,
        "{}",
        answer.raw
    );
    assert_eq!(
        answer.body["entry"]["config"]["data"]["tick-ms"],
        composition::kit::TICK_MS,
        "siblings kept: {}",
        answer.raw
    );

    let scheduler = provider_fiber(&before, "jinn:cron");
    daemon.eventually("the scheduler to restart on the patched config", || {
        daemon.config_restarts(scheduler) == 1
    });
    for other in fibers_before.iter().filter(|fiber| **fiber != scheduler) {
        assert_eq!(
            daemon.config_restarts(*other),
            0,
            "fiber {other} kept its incarnation"
        );
    }
    // The document of record carries the patch (the API never bypassed it).
    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(daemon.root.join("profile.json")).expect("profile"))
            .expect("parses");
    let mut document = document;
    assert_eq!(
        entry_config(&mut document, "cron-scheduler")["data"]["jobs"][0]["every-ms"],
        halved
    );
    // A fire on an odd boundary of the halved grid proves the patched
    // schedule is live (never a boundary of the old grid).
    daemon.eventually("a fire on the halved schedule", || {
        std::fs::read_to_string(daemon.data("cron/history.jsonl"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .any(|record| {
                record["outcome"]["fired"].is_object()
                    && record["scheduled-ms"]
                        .as_u64()
                        .is_some_and(|ms| ms % JOB_PERIOD_MS == halved)
            })
    });

    // Uid evidence on the ledger: the patch is the editor's granted
    // `jinn:profile` call, recorded as `ProfilePatched` under the editor
    // with NO fs effect (operator intent, not a fiber's contribution —
    // FINDINGS.md #21 closed), and after it exactly ONE fiber cycled —
    // the scheduler's successor incarnation — while every other fiber
    // appears in no transition since.
    let rows = daemon.ledger_rows();
    let after: Vec<&LedgerRow> = rows.iter().filter(|row| row.seq > last_seq).collect();
    let patched = after
        .iter()
        .find(|row| {
            row.kind
                .contains(r#"ProfilePatched":{"entry":"cron-scheduler","by":"jinn-profile-edit""#)
        })
        .expect("the patch is operator intent on the record");
    assert_eq!(
        (patched.entry.as_deref(), patched.fiber),
        (Some(EDITOR), Some(editor)),
        "attributed to the editor: {patched:?}"
    );
    assert!(
        after.iter().any(|row| row.fiber == Some(editor)
            && row.kind
                == r#"{"ContractCall":{"contract":"jinn:profile","operation":"patch-entry"}}"#),
        "the kernel patch is a granted contract call"
    );
    assert!(
        !after.iter().any(|row| row
            .kind
            .contains(r#"EffectRegistered":{"label":"fs write profile"#)),
        "no fs effect carries the edit: {after:?}"
    );
    assert!(
        after.iter().any(|row| row.kind.contains(
            r#"ContractCall":{"contract":"jinn:api-profile","operation":"patch-entry"}"#
        )),
        "the patch itself is a ledgered contract call"
    );
    // A reconcile restart is an INCARNATION replacement inside the
    // entry's one fiber (the id is the entry's, kept across incarnations):
    // the only fiber id in any transition after the patch is the
    // scheduler's — suspended (its persisted contribution retained) and
    // re-activated with `cause: ConfigChanged` — and no new id appears.
    // The other fibers have no transition at all: the patch went
    // through the profile, not around the trio.
    let transitioned: BTreeSet<u64> = after
        .iter()
        .filter(|row| row.kind.contains("FiberTransition") || row.kind.contains("FiberSuspended"))
        .filter_map(|row| row.fiber)
        .collect();
    assert_eq!(
        transitioned,
        BTreeSet::from([scheduler]),
        "exactly the scheduler's fiber cycled: {after:?}"
    );
    assert_eq!(
        active_fibers(&rows),
        fibers_before,
        "no new fiber id: the entry keeps its fiber"
    );
    assert!(
        after.iter().any(|row| row.fiber == Some(scheduler)
            && row
                .kind
                .contains(r#""to":"Active","cause":"ConfigChanged""#)),
        "the successor incarnation activated on the config change: {after:?}"
    );
    assert!(
        after
            .iter()
            .any(|row| row.fiber == Some(scheduler) && row.kind.contains("FiberSuspended")),
        "the first incarnation was suspended, not withdrawn: {after:?}"
    );
    // The reconcile ran once for the patch and was never mistaken for the
    // daemon's own echo.
    assert_eq!(daemon.swallowed_reconciles(), 0, "{}", daemon.log());
    daemon.interrupt();
}

#[test]
fn a_bind_outside_the_grant_and_a_non_loopback_bind_are_refused_on_the_record() {
    let Some((daemon, port)) = booted("api-refused") else {
        return;
    };
    // A port outside the granted range: the provider entry restarts on
    // its config edit, the bind is refused at the broker (ledgered
    // `GrantRefused jinn:net`), the fiber fails its activation —
    // contained to the entry — and the consumers keep their fibers.
    let outside = free_port();
    daemon.edit_profile_restarting(PROVIDER, |document| {
        entry_config(document, PROVIDER)["data"]["port"] = serde_json::json!(outside);
    });
    daemon.eventually("the out-of-range bind to be refused on the ledger", || {
        daemon.ledger_count(r#"GrantRefused":{"contract":"jinn:net""#) >= 1
    });
    daemon.eventually("the provider fiber to fail, contained", || {
        daemon
            .ledger_kinds()
            .iter()
            .any(|kind| kind.contains("FiberTransition") && kind.contains(r#""to":"Failed""#))
    });
    assert!(!listening(outside), "nothing listens on the refused port");
    assert!(
        !listening(port),
        "the old listener was released with the incarnation"
    );
    for id in [STATUS, EDITOR, "cron-scheduler", "health-snapshot"] {
        assert_eq!(daemon.restart_count(id), 0, "{id} kept its fiber");
    }

    // A non-loopback host at an in-range port: refused the same way
    // (the bundle binds loopback only in v0.1).
    daemon.edit_profile_restarting(PROVIDER, |document| {
        let config = entry_config(document, PROVIDER);
        config["data"]["port"] = serde_json::json!(port);
        config["data"]["host"] = serde_json::json!("0.0.0.0");
    });
    daemon.eventually("the non-loopback bind to be refused on the ledger", || {
        daemon.ledger_count(r#"GrantRefused":{"contract":"jinn:net""#) >= 2
    });
    std::thread::sleep(Duration::from_millis(600));
    assert!(!listening(port), "nothing listens after a refused bind");
    // Both refusals are the broker's, attributed to the provider's fiber,
    // and each ends in a contained `Failed` transition of that fiber
    // alone (`cause: ConfigChanged`); the daemon's own status line names
    // the entry as Failed, and each refusal carries its typed reason on
    // the record (pin `57360cc`).
    let rows = daemon.ledger_rows();
    let http_fiber = rows
        .iter()
        .find(|row| row.kind.contains("NetListening"))
        .and_then(|row| row.fiber)
        .expect("the boot listen is attributed");
    let refusals: Vec<&LedgerRow> = rows
        .iter()
        .filter(|row| row.kind.contains(r#"GrantRefused":{"contract":"jinn:net""#))
        .collect();
    assert_eq!(refusals.len(), 2, "{refusals:?}");
    assert!(
        refusals.iter().all(|row| row.fiber == Some(http_fiber)),
        "attributed to the provider: {refusals:?}"
    );
    assert!(
        refusals
            .iter()
            .all(|row| row.entry.as_deref() == Some(PROVIDER) && row.kind.contains(r#""reason":"#)),
        "each refusal names its entry and typed reason: {refusals:?}"
    );
    let failed: Vec<&LedgerRow> = rows
        .iter()
        .filter(|row| row.kind.contains(r#""to":"Failed""#))
        .collect();
    assert_eq!(failed.len(), 2, "{failed:?}");
    assert!(
        failed.iter().all(|row| row.fiber == Some(http_fiber)),
        "contained to the provider's fiber: {failed:?}"
    );
    assert!(
        daemon.log().contains(r#"entry="jinn-api-http" fiber="#),
        "the status line names the entry"
    );

    // Restoring the authorized shape brings the API back — the entry was
    // only ever contained, never the daemon.
    daemon.edit_profile_restarting(PROVIDER, |document| {
        entry_config(document, PROVIDER)["data"]["host"] = serde_json::json!("127.0.0.1");
    });
    let health = get(port, "/v1/health");
    assert_eq!(health.status, 200, "{}", health.raw);
    daemon.interrupt();
}

#[test]
fn swapping_the_provider_entry_by_profile_edit_leaves_the_consumers_untouched() {
    let Some((daemon, port_a)) = booted("api-swap") else {
        return;
    };
    // The composition proof of the seam split: the provider is replaced
    // by a profile edit alone — the `jinn-api-http` entry leaves, a
    // second provider entry (same artifact, its own id and port) arrives
    // — and the consumers, which own the schema, are not restarted. An
    // operator switching transports later does exactly this edit.
    let port_b = free_port();
    daemon.edit_profile(|document| {
        let entries = document["entries"].as_array_mut().expect("entries");
        let mut replacement = entries
            .iter()
            .find(|entry| entry["id"] == PROVIDER)
            .cloned()
            .expect("the provider entry");
        entries.retain(|entry| entry["id"] != PROVIDER);
        replacement["id"] = serde_json::json!("jinn-api-http-b");
        entries.push(replacement);
        set_provider_port(document, "jinn-api-http-b", port_b);
    });
    daemon.eventually("the provider swap to reconcile", || {
        let log = daemon.log();
        log.contains(r#"created=[EntryId("jinn-api-http-b")]"#)
            && log.contains(r#"disposed=[EntryId("jinn-api-http")]"#)
    });
    let status = get(port_b, "/v1/status");
    assert_eq!(status.status, 200, "{}", status.raw);
    assert_eq!(status.body["probes"][0]["live"], true, "{}", status.raw);
    let health = get(port_b, "/v1/health");
    assert_eq!(health.body["ok"], true, "{}", health.raw);
    assert!(
        !listening(port_a),
        "the disposed provider's listener is gone"
    );
    for id in [STATUS, EDITOR, "cron-scheduler", "health-snapshot"] {
        assert_eq!(daemon.restart_count(id), 0, "{id} untouched by the swap");
    }
    let kinds = daemon.ledger_kinds().join("\n");
    assert!(
        kinds.contains(&format!(r#"NetListening":{{"handle":1,"port":{port_a}}}"#)),
        "{kinds}"
    );
    assert!(
        kinds.contains(&format!(r#""port":{port_b}}}"#)),
        "the successor listens: {kinds}"
    );
    assert!(
        kinds.contains(r#"EffectWithdrawn":{"label":"jinn:net listen [handle 1]","clean":true}"#),
        "the old listener withdrawn on the record: {kinds}"
    );
    assert!(
        kinds.contains("ServiceWithdrawn") || kinds.contains(r#"NetClosed":{"handle":1}"#),
        "{kinds}"
    );
    daemon.interrupt();
}

#[test]
fn suspend_releases_the_listener_and_a_restart_relistens() {
    let Some((daemon, port)) = booted("api-suspend") else {
        return;
    };
    let root = daemon.root.clone();
    daemon.interrupt();
    assert!(!listening(port), "the clean stop released the listener");
    let rows = ledger_rows_at(&root);
    let kinds: Vec<&str> = rows.iter().map(|row| row.kind.as_str()).collect();
    let joined = kinds.join("\n");
    assert_eq!(
        joined.matches("FiberSuspended").count(),
        ALL.len(),
        "one suspension per fiber: {joined}"
    );
    assert!(
        joined.contains(r#"EffectWithdrawn":{"label":"jinn:net listen [handle 1]","clean":true}"#),
        "the listener is a kernel registration, released on the record: {joined}"
    );
    assert!(joined.contains(r#"NetClosed":{"handle":1}"#), "{joined}");
    assert!(
        !joined.contains(r#"EffectWithdrawn":{"label":"fs "#),
        "no world mutation withdrawn by a suspend: {joined}"
    );
    // Restart over the same root: `activate` re-listens (a second
    // `NetListening` on the record) and the API answers again.
    let binary = gate().expect("gate already passed");
    let daemon = Daemon::boot_operator(binary, &root);
    daemon.await_ready();
    let health = get(port, "/v1/health");
    assert_eq!(health.status, 200, "{}", health.raw);
    assert_eq!(health.body["ok"], true, "{}", health.raw);
    assert_eq!(
        daemon.ledger_count("NetListening"),
        2,
        "re-listened on activate"
    );
    daemon.interrupt();
}

/// FINDINGS.md #21, CLOSED at pin `57360cc`: the operator's patch is
/// operator intent applied by the loader — no fs inverse, no journal
/// entry — so removing the editor from the profile withdraws exactly
/// the editor's own contribution and the edit STAYS: the document keeps
/// the patch, the patched entry keeps its new config, and the editor is
/// gone for good (never resurrected by a restored document). This is the
/// transcript that went red on adoption, replaced by the new law's
/// proof (the cron seam's #14 precedent).
#[test]
fn disposing_the_editor_leaves_the_operators_edit_in_place_finding_21_closed() {
    let Some((daemon, port)) = booted("api-revert") else {
        return;
    };
    let scheduler = provider_fiber(&daemon.ledger_rows(), "jinn:cron");
    let answer = patch(
        port,
        "/v1/profile/entries/cron-scheduler",
        &serde_json::json!({ "config": { "data": { "tick-ms": composition::kit::TICK_MS + 50 } } }),
    );
    assert_eq!(answer.status, 200, "{}", answer.raw);
    daemon.eventually("the scheduler to restart on the patch", || {
        daemon.config_restarts(scheduler) == 1
    });
    // The operator now removes the editor entry (a direct profile edit,
    // the way an operator retires a plugin).
    daemon.edit_profile(|document| {
        let entries = document["entries"].as_array_mut().expect("entries");
        entries.retain(|entry| entry["id"] != EDITOR);
    });
    daemon.eventually("the editor to dispose", || {
        daemon
            .log()
            .contains(&format!(r#"disposed=[EntryId("{EDITOR}")]"#))
    });
    daemon.eventually("the editor's own trail to be withdrawn", || {
        daemon
            .ledger_kinds()
            .iter()
            .any(|kind| kind.contains(r#"EffectWithdrawn":{"label":"jinn-profile-edit on duty""#))
    });
    // Give a reversal every chance to show: nothing does.
    std::thread::sleep(Duration::from_millis(1_500));
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(daemon.root.join("profile.json")).expect("profile"))
            .expect("parses");
    assert_eq!(
        document["entries"].as_array().map_or(0, Vec::len),
        ALL.len() - 1,
        "the editor stays gone: {document}"
    );
    let tick_ms = entry_config(&mut document, "cron-scheduler")["data"]["tick-ms"].clone();
    assert_eq!(
        tick_ms,
        composition::kit::TICK_MS + 50,
        "the edit survives the editor: {document}"
    );
    let kinds = daemon.ledger_kinds().join("\n");
    assert!(
        !kinds.contains(r#"EffectWithdrawn":{"label":"fs write profile.json"#),
        "no document withdrawal: {kinds}"
    );
    assert_eq!(
        daemon.config_restarts(scheduler),
        1,
        "the scheduler never restarted on an old config"
    );
    assert!(
        !daemon
            .log()
            .contains(&format!(r#"created=[EntryId("{EDITOR}")]"#)),
        "the editor was not resurrected"
    );
    let health = get(port, "/v1/health");
    assert_eq!(health.body["entries"], ALL.len() - 1, "{}", health.raw);
    daemon.interrupt();
}

/// FINDINGS.md #25 CLOSED (pin `3fd7b05`, jinnd M2-K8): the same api trio
/// booted in the CRON layout — the profile beside the data root,
/// `<root>/data` as `jinn:fs`'s surface, the soak's layout — now reads the
/// document of record IN FULL. The read is the kernel's own `jinn:profile`
/// `document`, not a file read, so where the document sits stopped
/// mattering: every entry carries its authority fields (`package`,
/// `hash`, `grants`) and `get` answers the document. This test is the
/// inverse of what it asserted at the previous pin, where the same layout
/// could only answer the typed `unavailable`.
#[test]
fn the_operator_api_reads_the_document_beside_the_data_root_finding_25_closed() {
    let Some(binary) = gate() else {
        return;
    };
    let (root, port) = fresh_api_root("api-beside");
    let daemon = Daemon::boot(binary, &root);
    daemon.await_ready();
    let health = get(port, "/v1/health");
    assert_eq!(health.status, 200, "{}", health.raw);
    assert_eq!(health.body["ok"], true, "{}", health.raw);
    assert_eq!(
        health.body["profile-readable"], true,
        "the document is readable beside the data root: {}",
        health.raw
    );
    assert_eq!(health.body["entries"], ALL.len(), "{}", health.raw);

    // `status`: the document was read, nothing is typed unavailable, and
    // every entry carries the authority fields only the document holds.
    let status = get(port, "/v1/status");
    let report = &status.body;
    assert_eq!(report["document"]["readable"], true, "{report}");
    assert!(
        report["document"]["unavailable"].is_null(),
        "no typed unavailable is left: {report}"
    );
    let entries = report["entries"].as_array().expect("entries");
    assert_eq!(entries.len(), ALL.len(), "{report}");
    for entry in entries {
        assert!(
            entry["package"].as_str().is_some_and(|p| !p.is_empty()),
            "the pinned package is the document's, never guessed: {entry}"
        );
        assert!(
            entry["hash"].as_str().is_some_and(|hash| hash.len() == 64),
            "the content hash is the document's (kernel Law 5): {entry}"
        );
        assert!(
            entry["grants"].as_array().is_some_and(|g| !g.is_empty()),
            "the grants as written: {entry}"
        );
        assert_eq!(entry["state"], "active", "{entry}");
    }
    let scheduler = entries
        .iter()
        .find(|entry| entry["id"] == "cron-scheduler")
        .expect("the scheduler entry");
    assert_eq!(scheduler["package"], "cron/cron-scheduler", "{scheduler}");
    assert!(
        scheduler["grants"]
            .as_array()
            .is_some_and(|grants| grants.contains(&serde_json::json!("jinn:clock"))),
        "{scheduler}"
    );
    assert_eq!(report["readiness"]["boot-reconciled"], true, "{report}");

    // `get` answers the document of record in the same layout.
    let document = get(port, "/v1/profile");
    assert_eq!(document.status, 200, "{}", document.raw);
    let listed = document.body["profile"]["entries"]
        .as_array()
        .expect("entries");
    assert_eq!(listed.len(), ALL.len(), "{}", document.raw);
    let ids: BTreeSet<&str> = listed
        .iter()
        .filter_map(|entry| entry["id"].as_str())
        .collect();
    assert_eq!(ids, ALL.iter().copied().collect(), "{}", document.raw);

    // Law 2: the read is a granted `jinn:profile` crossing attributed to
    // each reader's ENTRY — and no `jinn:fs` read of the document exists
    // any more, because neither consumer holds one.
    // Wait per READER, not on a total: two rows could both be one
    // reader's, and then the assertions below race the other's first
    // read rather than proving it.
    let read_by = |reader: &str| {
        daemon.ledger_rows().iter().any(|row| {
            row.entry.as_deref() == Some(reader)
                && row.kind
                    == r#"{"ContractCall":{"contract":"jinn:profile","operation":"document"}}"#
        })
    };
    for reader in [STATUS, EDITOR] {
        daemon.eventually(
            &format!("{reader}'s document read to land on the ledger"),
            || read_by(reader),
        );
    }
    let rows = daemon.ledger_rows();
    for reader in [STATUS, EDITOR] {
        assert!(
            rows.iter().any(|row| {
                row.entry.as_deref() == Some(reader)
                    && row.kind
                        == r#"{"ContractCall":{"contract":"jinn:profile","operation":"document"}}"#
            }),
            "{reader} reads the document through jinn:profile: {rows:?}"
        );
    }
    // Neither reader holds any `jinn:fs` authority over the document any
    // more — the coupling #25 named is gone from the authority side too.
    for reader in [STATUS, EDITOR] {
        let grants = listed
            .iter()
            .find(|entry| entry["id"] == reader)
            .expect("the reader entry")["grants"]
            .to_string();
        assert!(
            !grants.contains("jinn:fs"),
            "{reader} reads the document with no fs authority: {grants}"
        );
    }
    daemon.interrupt();
}

/// FINDINGS.md #24 CLOSED (pin `3fd7b05`, jinnd M2-K8): a grant carries an
/// operation class, so authority can be exactly as wide as its use. The
/// status viewer ships with `jinn:profile` attenuated to
/// `ops: ["entry", "document"]` — it reads the document of record and
/// holds NO patch authority — while the editor holds the reads AND
/// `patch-entry`.
///
/// What this proves: the shipped classes are what the document of record
/// says they are; the kernel enforces the class PER OPERATION at dispatch,
/// on the record — with the editor's own grant narrowed to the viewer's
/// read-only class, its `patch-entry` is refused with a ledgered
/// `GrantRefused` naming the operation, the document is left unwritten,
/// and its `document` read keeps working (a class, not a contract, was
/// withdrawn). What it does NOT prove: that `jinn-status`'s own fiber is
/// refused a patch — that plugin never calls `patch-entry`, and driving
/// one from it would take a plugin whose only purpose is to misuse its
/// grant. The enforcement point is the same broker check for both.
#[test]
fn a_read_only_profile_grant_holds_no_patch_authority_finding_24_closed() {
    let Some((daemon, port)) = booted("api-attenuated") else {
        return;
    };
    // The shipped authority, from the document of record itself.
    let document = get(port, "/v1/profile");
    assert_eq!(document.status, 200, "{}", document.raw);
    let profile_ops = |id: &str| -> Vec<String> {
        document.body["profile"]["entries"]
            .as_array()
            .expect("entries")
            .iter()
            .find(|entry| entry["id"] == id)
            .unwrap_or_else(|| panic!("the {id} entry"))["grants"]
            .as_array()
            .expect("grants")
            .iter()
            .find(|grant| grant["contract"] == jinn_api::KERNEL_PROFILE_CONTRACT)
            .unwrap_or_else(|| panic!("{id} holds a jinn:profile grant"))["ops"]
            .as_array()
            .expect("an operation class is written out")
            .iter()
            .filter_map(|op| op.as_str().map(str::to_owned))
            .collect()
    };
    assert_eq!(
        profile_ops(STATUS),
        jinn_api::KERNEL_PROFILE_READ_OPS,
        "the status viewer is read-only: {}",
        document.raw
    );
    assert!(
        !profile_ops(STATUS).contains(&jinn_api::OP_KERNEL_PATCH_ENTRY.to_owned()),
        "a viewer holds no write authority over the document it reads"
    );
    assert_eq!(
        profile_ops(EDITOR),
        jinn_api::KERNEL_PROFILE_EDIT_OPS,
        "the editor holds the reads AND the write: {}",
        document.raw
    );

    // The editor's class admits `patch-entry`: a PATCH goes end to end.
    let editor = provider_fiber(&daemon.ledger_rows(), "jinn:api-profile");
    let scheduler = provider_fiber(&daemon.ledger_rows(), "jinn:cron");
    let bumped = composition::kit::TICK_MS + 30;
    let accepted = patch(
        port,
        "/v1/profile/entries/cron-scheduler",
        &serde_json::json!({ "config": { "data": { "tick-ms": bumped } } }),
    );
    assert_eq!(accepted.status, 200, "{}", accepted.raw);
    assert_eq!(accepted.body["changed"], true, "{}", accepted.raw);
    daemon.eventually("the patch to land as operator intent", || {
        daemon.ledger_count(r#"ProfilePatched":{"entry":"cron-scheduler""#) == 1
    });
    daemon.eventually("the scheduler to restart on the patch", || {
        daemon.config_restarts(scheduler) == 1
    });

    // The operator now narrows the EDITOR's own grant to the viewer's
    // read-only class — a direct edit of the document, the way authority
    // is withdrawn — and the entry restarts under it.
    let refusals_before = daemon.ledger_count(r#"GrantRefused":{"contract":"jinn:profile""#);
    daemon.edit_profile_restarting(EDITOR, |document| {
        let grants = entry_config(document, EDITOR)["grants"]
            .as_array_mut()
            .expect("the editor's grants");
        for grant in grants.iter_mut() {
            if grant["contract"] == jinn_api::KERNEL_PROFILE_CONTRACT {
                grant["ops"] = serde_json::json!(jinn_api::KERNEL_PROFILE_READ_OPS);
            }
        }
    });

    // The same patch is now REFUSED at the broker, on the record.
    let refused = patch(
        port,
        "/v1/profile/entries/cron-scheduler",
        &serde_json::json!({ "config": { "data": { "tick-ms": bumped + 30 } } }),
    );
    assert_eq!(refused.status, 502, "{}", refused.raw);
    assert_eq!(refused.body["error"]["code"], "refused", "{}", refused.raw);
    daemon.eventually("the operation refusal to land on the ledger", || {
        daemon.ledger_count(r#"GrantRefused":{"contract":"jinn:profile""#) > refusals_before
    });
    let rows = daemon.ledger_rows();
    let refusal = rows
        .iter()
        .find(|row| {
            row.kind
                .contains(r#""GrantRefused":{"contract":"jinn:profile""#)
                && row
                    .kind
                    .contains("patch-entry is outside the granted operation class")
        })
        .expect("the refused operation is named on the ledger");
    assert_eq!(
        refusal.fiber,
        Some(editor),
        "attributed to the attenuated caller: {refusal:?}"
    );
    // The class was narrowed, not the contract withdrawn: the reads live.
    let still_readable = get(port, "/v1/profile");
    assert_eq!(still_readable.status, 200, "{}", still_readable.raw);
    // And nothing was written: exactly one `ProfilePatched` ever, the
    // accepted one, and the document still carries only that value.
    assert_eq!(
        daemon.ledger_count("ProfilePatched"),
        1,
        "the refused patch wrote nothing"
    );
    let mut on_disk: serde_json::Value =
        serde_json::from_slice(&std::fs::read(daemon.root.join("profile.json")).expect("profile"))
            .expect("parses");
    assert_eq!(
        entry_config(&mut on_disk, "cron-scheduler")["data"]["tick-ms"],
        bumped,
        "the document of record is unchanged by the refusal"
    );
    daemon.interrupt();
}
