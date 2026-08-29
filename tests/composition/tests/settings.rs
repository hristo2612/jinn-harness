//! The settings seam's real-composition gate (AGENTS.md standing order
//! 3): the operator profile — cron seam + api trio + settings pair —
//! booted through the REAL pinned `jinnd` daemon in the operator layout,
//! driven over the operator API. Every proof the seam claims lands here:
//! declare → resolve with defaults → patch (hot and restart) → a
//! validation refusal typed AND on the record → the `changed` event
//! delivered → the scheduler rescheduling from a settings patch → the
//! provider swapped by a profile edit with the consumers untouched. The
//! C5/C6 evidence (what a patch costs on each path) is printed as a
//! transcript from the ledger of this very run.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use composition::api::{get, patch};
use composition::daemon::{jinnd_source, pinned_commit, pinned_daemon};
use composition::kit::{entry_config, fresh_api_root, Daemon, LedgerRow, JOB_PERIOD_MS, TICK_MS};

const SCHEDULER: &str = "cron-scheduler";
const PROVIDER: &str = "jinn-settings-profile";
const STORE: &str = "jinn-settings-store";

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

/// Boots the operator root and waits until the scheduler has declared
/// its namespace (its one-shot settings alarm, one clock floor after
/// activation) — `GET /v1/settings/cron` answers 200.
fn booted(name: &str) -> Option<(Daemon, u16)> {
    let binary = gate()?;
    let (root, port) = fresh_api_root(name);
    let daemon = Daemon::boot_operator(binary, &root);
    daemon.await_ready();
    daemon.eventually("the cron namespace to be declared", || {
        get(port, "/v1/settings/cron").status == 200
    });
    Some((daemon, port))
}

fn fiber_of(rows: &[LedgerRow], entry: &str) -> u64 {
    rows.iter()
        .find(|row| row.entry.as_deref() == Some(entry) && row.fiber.is_some())
        .and_then(|row| row.fiber)
        .unwrap_or_else(|| panic!("{entry} has a fiber on the record"))
}

fn document(daemon: &Daemon) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(daemon.root.join("profile.json")).expect("profile"))
        .expect("parses")
}

/// A fire recorded on the `period` grid but not on the kit's 2 s grid —
/// the proof a patched schedule is live.
fn fired_on_grid(daemon: &Daemon, period: u64) -> bool {
    std::fs::read_to_string(daemon.data("cron/history.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .any(|record| {
            record["outcome"]["fired"].is_object()
                && record["scheduled-ms"]
                    .as_u64()
                    .is_some_and(|ms| ms % period == 0 && ms % JOB_PERIOD_MS != 0)
        })
}

#[test]
fn declare_resolve_and_patch_on_both_paths_with_the_c5_c6_transcript() {
    let Some((daemon, port)) = booted("settings-paths") else {
        return;
    };
    // Declared and resolved: defaults under the entry layer, an empty
    // overlay, the kit's schedule as the scheduler activated on it.
    let resolved = get(port, "/v1/settings/cron");
    assert_eq!(resolved.status, 200, "{}", resolved.raw);
    let body = &resolved.body;
    assert_eq!(body["namespace"], "cron", "{body}");
    assert_eq!(body["entry"], SCHEDULER, "{body}");
    assert_eq!(body["settings"]["tick-ms"], TICK_MS, "{body}");
    assert_eq!(body["settings"]["jobs"][0]["id"], "health", "{body}");
    assert_eq!(
        body["layers"]["defaults"]["tick-ms"],
        jinn_cron::DEFAULT_TICK_MS,
        "{body}"
    );
    assert_eq!(body["layers"]["entry"]["tick-ms"], TICK_MS, "{body}");
    assert_eq!(body["layers"]["overlay"], serde_json::json!({}), "{body}");
    assert_eq!(
        body["hot-keys"],
        serde_json::json!(["jobs", "notify-token"]),
        "{body}"
    );
    let namespaces = get(port, "/v1/settings");
    assert_eq!(
        namespaces.body["namespaces"]["cron"]["entry"], SCHEDULER,
        "{}",
        namespaces.raw
    );
    let rows = daemon.ledger_rows();
    let scheduler = fiber_of(&rows, SCHEDULER);
    let store = fiber_of(&rows, STORE);
    assert!(
        rows.iter()
            .any(|row| row.entry.as_deref() == Some(SCHEDULER)
                && row.kind
                    == r#"{"ContractCall":{"contract":"jinn:settings","operation":"declare"}}"#),
        "the declaration is the scheduler's granted call"
    );

    // HOT PATH: a `jobs` patch lands in the overlay — the STORE entry is
    // patched through `jinn:profile` (its trivial fiber restarts), the
    // scheduler absorbs the `changed` event in place (its fiber never
    // cycles), and the halved schedule is live.
    let halved = JOB_PERIOD_MS / 2;
    let hot_from = daemon.ledger_rows().last().map_or(0, |row| row.seq);
    let hot_started = Instant::now();
    let hot = patch(
        port,
        "/v1/settings/cron",
        &serde_json::json!({ "patch": { "jobs": [
            { "id": "health", "every-ms": halved, "topic": "cron:health" } ] } }),
    );
    let hot_answered = hot_started.elapsed();
    assert_eq!(hot.status, 200, "{}", hot.raw);
    assert_eq!(hot.body["applied"], "hot", "{}", hot.raw);
    assert_eq!(
        hot.body["settings"]["jobs"][0]["every-ms"], halved,
        "{}",
        hot.raw
    );
    assert_eq!(hot.body["revision"], 1, "{}", hot.raw);
    let probe = get(port, "/v1/status");
    assert_eq!(
        probe.body["probes"][0]["answer"]["jobs"][0]["every-ms"], halved,
        "the scheduler holds the patched table: {}",
        probe.raw
    );
    assert_eq!(
        probe.body["probes"][0]["answer"]["settings-revision"], 1,
        "{}",
        probe.raw
    );
    daemon.eventually("a fire on the halved schedule", || {
        fired_on_grid(&daemon, halved)
    });
    let mut doc = document(&daemon);
    assert_eq!(
        entry_config(&mut doc, STORE)["data"]["overlays"]["cron"]["jobs"][0]["every-ms"],
        halved,
        "the overlay is in the document of record"
    );
    assert_eq!(
        entry_config(&mut doc, SCHEDULER)["data"]["jobs"][0]["every-ms"],
        JOB_PERIOD_MS,
        "the owner entry is untouched by a hot patch"
    );
    let rows = daemon.ledger_rows();
    let hot_rows: Vec<&LedgerRow> = rows.iter().filter(|row| row.seq > hot_from).collect();
    assert!(
        hot_rows.iter().any(|row| row.kind.contains(&format!(
            r#"ProfilePatched":{{"entry":"{STORE}","by":"{PROVIDER}""#
        ))),
        "the hot patch is a store patch on the record: {hot_rows:?}"
    );
    assert!(
        hot_rows.iter().any(
            |row| row.kind.contains(r#""topic":"jinn:settings/changed""#)
                && row.kind.contains(r#""listeners":1"#)
        ),
        "the changed event reached exactly the scheduler: {hot_rows:?}"
    );
    assert_eq!(
        daemon.config_restarts(scheduler),
        0,
        "the scheduler never cycled"
    );
    assert_eq!(daemon.config_restarts(store), 1, "the store cycled once");
    let hot_settle = hot_rows
        .iter()
        .find(|row| row.kind.contains(r#""topic":"jinn:settings/changed""#))
        .map_or(0, |row| row.seq);
    let hot_cost = hot_rows.iter().filter(|row| row.seq <= hot_settle).count();

    // RESTART PATH: a `tick-ms` patch lands in the entry — the OWNER is
    // patched through `jinn:profile`, the loader restarts exactly its
    // fiber (suspended with its state retained, re-activated on the new
    // config), and the scheduler re-declares on its next wake with the
    // new entry layer under the same overlay.
    let restart_from = daemon.ledger_rows().last().map_or(0, |row| row.seq);
    let restart_started = Instant::now();
    let restart = patch(
        port,
        "/v1/settings/cron",
        &serde_json::json!({ "patch": { "tick-ms": TICK_MS / 2 } }),
    );
    let restart_answered = restart_started.elapsed();
    assert_eq!(restart.status, 200, "{}", restart.raw);
    assert_eq!(restart.body["applied"], "restart", "{}", restart.raw);
    assert_eq!(
        restart.body["settings"]["tick-ms"],
        TICK_MS / 2,
        "{}",
        restart.raw
    );
    daemon.eventually("the scheduler to restart on the patched entry", || {
        daemon.config_restarts(scheduler) == 1
    });
    let mut doc = document(&daemon);
    assert_eq!(
        entry_config(&mut doc, SCHEDULER)["data"]["tick-ms"],
        TICK_MS / 2,
        "the owner entry carries the restart-path patch"
    );
    daemon.eventually("the restarted scheduler to re-declare", || {
        let body = get(port, "/v1/settings/cron").body;
        body["layers"]["entry"]["tick-ms"] == TICK_MS / 2
            && body["settings"]["jobs"][0]["every-ms"] == halved
    });
    let rows = daemon.ledger_rows();
    let restart_rows: Vec<&LedgerRow> = rows.iter().filter(|row| row.seq > restart_from).collect();
    assert!(
        restart_rows.iter().any(|row| row.kind.contains(&format!(
            r#"ProfilePatched":{{"entry":"{SCHEDULER}","by":"{PROVIDER}""#
        ))),
        "the restart patch is an owner patch on the record: {restart_rows:?}"
    );
    let suspended = restart_rows
        .iter()
        .find(|row| row.fiber == Some(scheduler) && row.kind.contains("FiberSuspended"))
        .expect("the owner was suspended, state retained");
    let reactivated = restart_rows
        .iter()
        .find(|row| {
            row.fiber == Some(scheduler)
                && row
                    .kind
                    .contains(r#""to":"Active","cause":"ConfigChanged""#)
        })
        .expect("the owner re-activated on the new config");
    assert!(
        restart_rows.iter().any(|row| row.fiber == Some(scheduler)
            && row.kind == r#"{"ContractCall":{"contract":"jinn:fs","operation":"read"}}"#
            && row.seq > suspended.seq),
        "the successor read its retained state (during its activation)"
    );
    let state = daemon.data_json("cron/state.json").expect("state");
    assert!(
        state["last"]["health"].is_u64(),
        "the schedule resumed: {state}"
    );
    let restart_cost = restart_rows
        .iter()
        .filter(|row| row.seq <= reactivated.seq)
        .count();
    assert_eq!(
        daemon.config_restarts(store),
        1,
        "the store did not cycle for a restart patch"
    );

    // The C5/C6 transcript for FINDINGS.md: rows and latency per path.
    eprintln!(
        "C5/C6 TRANSCRIPT (settings-paths, root {}):\n  hot path: {hot_cost} ledger rows from the \
         patch call to the changed event's dispatch (store fiber cycled, scheduler untouched), \
         answered in {} ms\n  restart path: {restart_cost} ledger rows from the patch call to the \
         owner's re-activation (FiberSuspended seq {} → Active seq {}), answered in {} ms, \
         state retained (last=health {})",
        daemon
            .root
            .file_name()
            .unwrap_or_default()
            .to_string_lossy(),
        hot_answered.as_millis(),
        suspended.seq,
        reactivated.seq,
        restart_answered.as_millis(),
        state["last"]["health"]
    );
    daemon.interrupt();
}

#[test]
fn a_patch_the_schema_refuses_is_typed_and_on_the_record() {
    let Some((daemon, port)) = booted("settings-refused") else {
        return;
    };
    let before = daemon.ledger_rows().last().map_or(0, |row| row.seq);
    // A wrong kind, a bare secret where a reference is declared, a key
    // the schema does not declare, a non-object patch: each refused
    // BEFORE anything applies — typed, and nothing patched.
    for (patch_body, needle) in [
        (serde_json::json!({ "patch": { "jobs": "nope" } }), "Array"),
        (
            serde_json::json!({ "patch": { "notify-token": "hunter2" } }),
            "holds no secret",
        ),
        (
            serde_json::json!({ "patch": { "stray": 1 } }),
            "not a declared setting",
        ),
        (serde_json::json!({ "patch": [1] }), "JSON object"),
    ] {
        let refused = patch(port, "/v1/settings/cron", &patch_body);
        assert_eq!(refused.status, 422, "{}", refused.raw);
        assert_eq!(refused.body["error"]["code"], "invalid", "{}", refused.raw);
        assert!(
            refused.body["error"]["detail"]
                .as_str()
                .is_some_and(|detail| detail.contains(needle)),
            "{}",
            refused.raw
        );
    }
    let unknown = patch(
        port,
        "/v1/settings/nope",
        &serde_json::json!({ "patch": {} }),
    );
    assert_eq!(unknown.status, 404, "{}", unknown.raw);
    daemon.eventually("the refusals to land on the record", || {
        daemon
            .ledger_rows()
            .iter()
            .filter(|row| row.seq > before)
            .filter(|row| row.kind.contains(r#""topic":"jinn:settings/refused""#))
            .count()
            >= 4
    });
    let rows = daemon.ledger_rows();
    assert!(
        !rows
            .iter()
            .filter(|row| row.seq > before)
            .any(|row| row.kind.contains("ProfilePatched")),
        "nothing applied: {rows:?}"
    );
    let untouched = get(port, "/v1/settings/cron");
    assert_eq!(untouched.body["revision"], 0, "{}", untouched.raw);
    assert!(untouched.body["settings"]["notify-token"].is_null());

    // A typed secret REFERENCE is admitted (hot: it is a hot key) and
    // shown verbatim — the document holds the name, never the secret.
    let reference = patch(
        port,
        "/v1/settings/cron",
        &serde_json::json!({ "patch": { "notify-token": { "$secret": "cron/notify" } } }),
    );
    assert_eq!(reference.status, 200, "{}", reference.raw);
    assert_eq!(reference.body["applied"], "hot", "{}", reference.raw);
    let shown = get(port, "/v1/settings/cron");
    assert_eq!(
        shown.body["settings"]["notify-token"],
        serde_json::json!({ "$secret": "cron/notify" }),
        "{}",
        shown.raw
    );
    let mut doc = document(&daemon);
    assert_eq!(
        entry_config(&mut doc, STORE)["data"]["overlays"]["cron"]["notify-token"]["$secret"],
        "cron/notify"
    );
    daemon.interrupt();
}

#[test]
fn swapping_the_settings_provider_by_profile_edit_leaves_the_consumers_untouched() {
    let Some((daemon, port)) = booted("settings-swap") else {
        return;
    };
    let rows = daemon.ledger_rows();
    let scheduler = fiber_of(&rows, SCHEDULER);
    // The provider entry leaves and a successor (same artifact, its own
    // id) arrives by ONE profile edit: the scheduler's fiber does not
    // cycle, it re-declares on its next wake, and a patch applies
    // through the successor — attributed to it.
    daemon.edit_profile(|document| {
        let entries = document["entries"].as_array_mut().expect("entries");
        let mut successor = entries
            .iter()
            .find(|entry| entry["id"] == PROVIDER)
            .cloned()
            .expect("the provider entry");
        entries.retain(|entry| entry["id"] != PROVIDER);
        successor["id"] = serde_json::json!("jinn-settings-profile-b");
        entries.push(successor);
    });
    daemon.eventually("the provider swap to reconcile", || {
        let log = daemon.log();
        log.contains(r#"created=[EntryId("jinn-settings-profile-b")]"#)
            && log.contains(&format!(r#"disposed=[EntryId("{PROVIDER}")]"#))
    });
    daemon.eventually("the scheduler to re-declare on the successor", || {
        get(port, "/v1/settings/cron").status == 200
    });
    let after = patch(
        port,
        "/v1/settings/cron",
        &serde_json::json!({ "patch": { "jobs": [
            { "id": "health", "every-ms": JOB_PERIOD_MS, "topic": "cron:health" },
            { "id": "second", "every-ms": JOB_PERIOD_MS, "topic": "cron:health" } ] } }),
    );
    assert_eq!(after.status, 200, "{}", after.raw);
    assert_eq!(after.body["applied"], "hot", "{}", after.raw);
    let probe = get(port, "/v1/status");
    assert_eq!(
        probe.body["probes"][0]["answer"]["jobs"]
            .as_array()
            .map(Vec::len),
        Some(2),
        "the scheduler holds both jobs: {}",
        probe.raw
    );
    assert_eq!(
        daemon.config_restarts(scheduler),
        0,
        "the consumer was untouched"
    );
    std::thread::sleep(Duration::from_millis(200));
    let kinds = daemon.ledger_kinds().join("\n");
    assert!(
        kinds.contains(&format!(
            r#"ProfilePatched":{{"entry":"{STORE}","by":"jinn-settings-profile-b""#
        )),
        "the successor applied it: {kinds}"
    );
    assert!(
        kinds.contains(&format!(
            r#"EffectWithdrawn":{{"label":"{PROVIDER} on duty","clean":true}}"#
        )),
        "the old provider withdrawn on the record: {kinds}"
    );
    daemon.interrupt();
}

/// The consistency law (PLA-314 round 2): the settings a `patch` reports
/// and emits are the settings the next `get` resolves — in BOTH orders.
/// Inverse order first: a mixed hot+cold patch with no overlay lands
/// whole in the entry; a hot patch after it lands in the overlay; each
/// report equals the next GET. Then the verifier's probe: with that
/// overlay in place, a mixed patch is refused WHOLE, typed `shadowed
/// { key, layer }`, nothing written, the revision unmoved, the refusal
/// on the record — and the next GET still equals the last report.
#[test]
fn a_patch_reports_exactly_what_the_next_get_resolves_in_both_orders() {
    let Some((daemon, port)) = booted("settings-consistency") else {
        return;
    };
    let rows = daemon.ledger_rows();
    let scheduler = fiber_of(&rows, SCHEDULER);
    let requested = serde_json::json!([
        { "id": "health", "every-ms": JOB_PERIOD_MS, "topic": "cron:health" },
        { "id": "requested", "every-ms": JOB_PERIOD_MS, "topic": "cron:health" } ]);

    // Inverse order, step 1: mixed hot+cold with an empty overlay.
    let mixed = patch(
        port,
        "/v1/settings/cron",
        &serde_json::json!({ "patch": { "jobs": requested, "tick-ms": TICK_MS / 2 } }),
    );
    assert_eq!(mixed.status, 200, "{}", mixed.raw);
    assert_eq!(mixed.body["applied"], "restart", "{}", mixed.raw);
    assert_eq!(mixed.body["settings"]["jobs"], requested, "{}", mixed.raw);
    daemon.eventually("the restarted scheduler to re-declare", || {
        get(port, "/v1/settings/cron").body["layers"]["entry"]["tick-ms"] == TICK_MS / 2
    });
    let next = get(port, "/v1/settings/cron");
    assert_eq!(
        next.body["settings"], mixed.body["settings"],
        "PATCH must report the state the configured layers resolve to: {}",
        next.raw
    );
    assert_eq!(daemon.config_restarts(scheduler), 1);

    // Inverse order, step 2: a hot patch over it lands in the overlay.
    let overlay = serde_json::json!([
        { "id": "overlay", "every-ms": JOB_PERIOD_MS, "topic": "cron:health" } ]);
    let hot = patch(
        port,
        "/v1/settings/cron",
        &serde_json::json!({ "patch": { "jobs": overlay } }),
    );
    assert_eq!(hot.status, 200, "{}", hot.raw);
    assert_eq!(hot.body["applied"], "hot", "{}", hot.raw);
    let next = get(port, "/v1/settings/cron");
    assert_eq!(next.body["settings"], hot.body["settings"], "{}", next.raw);
    assert_eq!(next.body["settings"]["jobs"], overlay, "{}", next.raw);
    assert_eq!(
        next.body["layers"]["overlay"]["jobs"], overlay,
        "{}",
        next.raw
    );
    let revision = next.body["revision"].clone();

    // The verifier's probe: existing hot overlay → mixed hot+cold patch.
    let before = daemon.ledger_rows().last().map_or(0, |row| row.seq);
    let shadowed = patch(
        port,
        "/v1/settings/cron",
        &serde_json::json!({ "patch": { "jobs": requested, "tick-ms": TICK_MS / 4 } }),
    );
    assert_eq!(
        shadowed.status, 422,
        "a mixed patch under an overlay is refused whole: {}",
        shadowed.raw
    );
    assert_eq!(
        shadowed.body["error"]["code"], "invalid",
        "{}",
        shadowed.raw
    );
    assert_eq!(
        shadowed.body["error"]["shadowed"],
        serde_json::json!({ "key": "jobs", "layer": "overlay" }),
        "typed: which key, which layer: {}",
        shadowed.raw
    );
    let next = get(port, "/v1/settings/cron");
    assert_eq!(
        next.body["settings"], hot.body["settings"],
        "nothing moved: the next GET still resolves the last applied report: {}",
        next.raw
    );
    assert_eq!(
        next.body["settings"]["tick-ms"],
        TICK_MS / 2,
        "{}",
        next.raw
    );
    assert_eq!(next.body["revision"], revision, "{}", next.raw);
    daemon.eventually("the refusal to land on the record", || {
        daemon
            .ledger_rows()
            .iter()
            .any(|row| row.seq > before && row.kind.contains(r#""topic":"jinn:settings/refused""#))
    });
    let rows = daemon.ledger_rows();
    assert!(
        !rows
            .iter()
            .any(|row| row.seq > before && row.kind.contains("ProfilePatched")),
        "nothing written for a refused patch: {rows:?}"
    );
    assert_eq!(
        daemon.config_restarts(scheduler),
        1,
        "the owner did not cycle"
    );
    let mut doc = document(&daemon);
    assert_eq!(
        entry_config(&mut doc, SCHEDULER)["data"]["tick-ms"],
        TICK_MS / 2,
        "the owner entry is as the last applied patch left it"
    );
    assert_eq!(
        entry_config(&mut doc, STORE)["data"]["overlays"]["cron"]["jobs"],
        overlay,
        "the overlay is as the last applied patch left it"
    );
    daemon.interrupt();
}
