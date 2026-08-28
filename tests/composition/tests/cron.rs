//! The cron seam's real-composition gate (AGENTS.md standing order 3):
//! every proof boots the generated profile through the REAL pinned `jinnd`
//! daemon binary — reconcile through its file watcher, time through its
//! `jinn:clock` alarms, evidence from its data root and its append-only
//! ledger. Nothing is hand-mounted, and no instant is injected: the suite
//! runs on real time over a fast kit (2 s boundaries, 500 ms wakes) and
//! asserts invariants that hold for any interleaving (grid membership,
//! gap arithmetic, counts that only grow).
//!
//! Self-skips LOUDLY when no jinnd checkout holding the pinned commit is
//! reachable (jinnd is private; see KERNEL-PIN.md Gate 2). Locally the
//! sibling checkout makes this run everywhere the verify gate runs.

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

use composition::daemon::{jinnd_source, pinned_commit, pinned_daemon};
use composition::kit::{entry_config, fresh_root, Daemon, JOB_PERIOD_MS, TICK_MS};

/// The ledger label of the scheduler's alarm request (jinn:clock's effect
/// label for `alarm-every`).
fn alarm_label() -> String {
    format!("alarm every {TICK_MS}ms")
}

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

/// Boots a fresh root and waits for the pair's boot evidence (the
/// consumer's boot report write — the last activation effect of the tree).
fn booted(name: &str) -> Option<Daemon> {
    let binary = gate()?;
    let root = fresh_root(name);
    let daemon = Daemon::boot(binary, &root);
    daemon.eventually("the pair to boot (consumer boot report)", || {
        daemon.data("health/boot.json").is_file()
    });
    Some(daemon)
}

/// The scheduler's history log: one record per line, grown by `append`.
fn history(daemon: &Daemon) -> Vec<serde_json::Value> {
    std::fs::read_to_string(daemon.data("cron/history.jsonl"))
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

fn fires(daemon: &Daemon) -> u64 {
    daemon
        .data_json("health/report.json")
        .and_then(|report| report["fires"].as_u64())
        .unwrap_or(0)
}

fn fired_records(daemon: &Daemon) -> Vec<serde_json::Value> {
    history(daemon)
        .into_iter()
        .filter(|record| record["outcome"]["fired"].is_object())
        .collect()
}

#[test]
fn boots_with_a_started_schedule_and_a_live_alarm() {
    let Some(daemon) = booted("boot") else { return };
    // Time enters at activation (the clock's `now` read): a fresh schedule
    // STARTS (firing law #4) — recorded, never fired — before any wake.
    daemon.eventually("the schedule to start at boot", || {
        !history(&daemon).is_empty()
    });
    let first = &history(&daemon)[0];
    assert_eq!(first["outcome"], "schedule-started", "{first}");
    assert_eq!(
        first["tick-seq"], 0,
        "the activate plan is edition 0: {first}"
    );
    assert_eq!(
        first["scheduled-ms"].as_u64().unwrap_or(1) % JOB_PERIOD_MS,
        0,
        "anchored on the grid: {first}"
    );
    // The boot is ledger-visible: two artifact admissions, the cron
    // provision, the clock read, the alarm request as an EFFECT (R5), and
    // the consumer's boot-report write (Law 2).
    let kinds = daemon.ledger_kinds().join("\n");
    assert_eq!(kinds.matches("ArtifactLoaded").count(), 2, "{kinds}");
    assert!(kinds.contains("jinn:cron"), "provision recorded: {kinds}");
    assert!(
        kinds.contains(r#""contract":"jinn:clock","operation":"now""#),
        "the activate-time clock read is a contract call: {kinds}"
    );
    assert!(
        kinds.contains(&format!(
            r#"EffectRegistered":{{"label":"{}""#,
            alarm_label()
        )),
        "the alarm request is a registered effect: {kinds}"
    );
    assert!(kinds.contains("health/boot.json"), "boot write: {kinds}");
    daemon.interrupt();
}

#[test]
fn fires_on_schedule_from_kernel_wakes_and_records_the_run() {
    let Some(daemon) = booted("fire") else { return };
    daemon.eventually("the first fire to land in the consumer report", || {
        fires(&daemon) >= 1
    });
    let report = daemon.data_json("health/report.json").expect("report");
    assert_eq!(report["probe-ok"], true, "{report}");
    let scheduled = report["last"]["scheduled-ms"]
        .as_u64()
        .expect("scheduled-ms");
    assert_eq!(scheduled % JOB_PERIOD_MS, 0, "on the grid: {report}");
    let now = report["last"]["now-ms"].as_u64().expect("now-ms");
    assert!(
        now >= scheduled && now < scheduled + JOB_PERIOD_MS + TICK_MS,
        "fired within one wake of its boundary: {report}"
    );
    assert!(
        report["last"]["tick-seq"].as_u64().is_some(),
        "wakes carry an edition: {report}"
    );
    // The scheduler's run record agrees with the consumer's report.
    daemon.eventually("the run record to settle", || {
        fired_records(&daemon)
            .iter()
            .any(|record| record["scheduled-ms"] == scheduled)
    });
    let fired = fired_records(&daemon)
        .into_iter()
        .find(|record| record["scheduled-ms"] == scheduled)
        .expect("the fired run record");
    assert_eq!(fired["outcome"]["fired"]["answers"], 1, "{fired}");
    let run_record_path = format!("cron/runs/health/{scheduled}.json");
    let run_record = daemon
        .data_json(&run_record_path)
        .expect("the per-fire record exists");
    assert_eq!(run_record["outcome"]["fired"]["answers"], 1, "{run_record}");
    // The fire is ledger-visible three ways (Law 2): the wake that carried
    // time in (`AlarmWake`), the fire emit itself (`DispatchTrace` on the
    // job topic — the first-class audit line), and the per-fire record
    // write whose label names the job and boundary (the outcome document).
    let kinds = daemon.ledger_kinds().join("\n");
    assert!(
        kinds.contains("AlarmWake"),
        "wakes are ledger events: {kinds}"
    );
    assert!(
        kinds.contains(r#"DispatchTrace":{"topic":"cron:health""#),
        "the fire emit lands a DispatchTrace on the job topic: {kinds}"
    );
    for path in [
        run_record_path.as_str(),
        "cron/state.json",
        "cron/history.jsonl",
        "health/report.json",
    ] {
        assert!(kinds.contains(path), "{path} write is ledgered:\n{kinds}");
    }
    daemon.interrupt();
}

#[test]
fn run_history_is_append_backed_and_the_consumer_sees_the_wider_surface() {
    let Some(daemon) = booted("append") else {
        return;
    };
    daemon.eventually("two fires", || fires(&daemon) >= 2);
    // The history lane is append-only on the record (FINDINGS.md #3
    // retired by the pin): every history effect is an `append` on the
    // log, sized by the tick — never a `write` of the whole document.
    let kinds = daemon.ledger_kinds().join("\n");
    let appends = daemon.ledger_count("fs append cron/history.jsonl");
    assert!(appends >= 3, "one append per recording tick: {kinds}");
    assert_eq!(
        daemon.ledger_count("fs write cron/history.jsonl"),
        0,
        "the log is never rewritten: {kinds}"
    );
    assert!(
        !daemon.data("cron/history.json").exists(),
        "no legacy array document is written"
    );
    // The log decodes line by line and holds every settled record —
    // the schedule start and each fire.
    let records = history(&daemon);
    assert!(records.iter().any(|r| r["outcome"] == "schedule-started"));
    assert!(fired_records(&daemon).len() >= 2, "{records:?}");
    // The consumer's report is built from `list` and `meta`, not
    // inferred: its own directory, the fired job's run records (one file
    // per fire on disk), and the history log's size (Law 2: each is a
    // ledgered contract call).
    let report = daemon.data_json("health/report.json").expect("report");
    let entries = report["dir"]["entries"].as_array().expect("dir listed");
    assert!(entries.iter().any(|entry| entry == "probe.txt"), "{report}");
    let runs_on_disk = std::fs::read_dir(daemon.data("cron/runs/health"))
        .expect("run records dir")
        .count() as u64;
    let listed = report["run-records"]["count"]
        .as_u64()
        .expect("runs listed");
    assert!(
        listed >= 1 && listed <= runs_on_disk,
        "the listed run records are the files on disk: {report}"
    );
    let log_size = report["history-log"]["size"].as_u64().expect("log stat");
    assert!(log_size > 0, "{report}");
    assert!(
        log_size
            <= std::fs::metadata(daemon.data("cron/history.jsonl"))
                .expect("log")
                .len(),
        "meta reports the log as it was at the fire: {report}"
    );
    for operation in ["list", "meta"] {
        assert!(
            kinds.contains(&format!(
                r#""contract":"jinn:fs","operation":"{operation}""#
            )),
            "{operation} is a ledgered contract call: {kinds}"
        );
    }
    daemon.interrupt();
}

#[test]
fn corrupt_persisted_state_is_quarantined_and_recorded() {
    let Some(daemon) = booted("quarantine") else {
        return;
    };
    // Build real state, then stop the daemon — by the crash path: a clean
    // shutdown would withdraw the persisted state this test corrupts
    // (FINDINGS.md #14).
    daemon.eventually("the first fire", || fires(&daemon) >= 1);
    let root = daemon.root.clone();
    daemon.kill();
    let fires_before = fires_at(&root);

    // Corrupt the persisted state on disk.
    let garbage = b"not json {{{".to_vec();
    std::fs::write(root.join("data/cron/state.json"), &garbage).expect("corrupt state");

    // Reboot: the corrupt document is preserved under quarantine, the loss
    // is a recorded state-fault run record, and the schedule starts fresh
    // — never a silent default (contract §Persistence honesty).
    let binary = gate().expect("gate already passed");
    let daemon = Daemon::boot(binary, &root);
    daemon.eventually("the quarantined original to be preserved", || {
        std::fs::read(daemon.data("cron/quarantine/state.json"))
            .is_ok_and(|preserved| preserved == garbage)
    });
    daemon.eventually("the state fault to be recorded", || {
        history(&daemon)
            .iter()
            .any(|record| record["outcome"]["state-fault"]["path"] == "cron/state.json")
    });
    // The fresh schedule follows law #4: it starts (no fire) at the boot
    // instant, and only the NEXT boundary fires.
    let started = history(&daemon)
        .into_iter()
        .rfind(|record| record["outcome"] == "schedule-started")
        .expect("the fresh schedule started");
    assert_eq!(fires(&daemon), fires_before, "no fire from corrupt state");
    daemon.eventually("one fire on the restarted schedule", || {
        fires(&daemon) == fires_before + 1
    });
    let report = daemon.data_json("health/report.json").expect("report");
    assert!(
        report["last"]["scheduled-ms"].as_u64() > started["scheduled-ms"].as_u64(),
        "the fire is a boundary AFTER the fresh start: {report} vs {started}"
    );
    daemon.interrupt();
}

/// The consumer's fire count on disk, for a daemon that is stopped.
fn fires_at(root: &std::path::Path) -> u64 {
    std::fs::read(root.join("data/health/report.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|report| report["fires"].as_u64())
        .unwrap_or(0)
}

#[test]
fn reschedules_on_config_edit_through_reconcile() {
    let Some(daemon) = booted("reschedule") else {
        return;
    };
    daemon.eventually("the schedule to start", || !history(&daemon).is_empty());
    // Operator lane: halve the period in the profile document. Only the
    // scheduler entry restarts (reconcile-by-id); its alarm is withdrawn
    // with the old fiber and re-requested by the new one.
    let halved = JOB_PERIOD_MS / 2;
    daemon.edit_profile(|document| {
        entry_config(document, "cron-scheduler")["data"]["jobs"][0]["every-ms"] =
            serde_json::json!(halved);
    });
    daemon.eventually("the scheduler to restart on its config edit", || {
        daemon
            .log()
            .contains(r#"restarted=[EntryId("cron-scheduler")]"#)
    });
    assert!(
        !daemon
            .log()
            .contains(r#"restarted=[EntryId("health-snapshot")]"#),
        "the consumer keeps its fiber"
    );
    // A fire on an odd boundary of the halved grid proves the new schedule
    // is live (such an instant is never a boundary of the old grid).
    daemon.eventually("a fire on the halved schedule", || {
        fired_records(&daemon).iter().any(|record| {
            record["scheduled-ms"]
                .as_u64()
                .is_some_and(|ms| ms % JOB_PERIOD_MS == halved)
        })
    });
    let kinds = daemon.ledger_kinds().join("\n");
    assert!(
        kinds.contains(&format!(
            r#"EffectWithdrawn":{{"label":"{}""#,
            alarm_label()
        )),
        "the old fiber's alarm was withdrawn (R5 undo): {kinds}"
    );
    assert_eq!(
        daemon.ledger_count(&format!(
            r#"EffectRegistered":{{"label":"{}""#,
            alarm_label()
        )),
        2,
        "the restarted scheduler re-requested its alarm: {kinds}"
    );
    daemon.interrupt();
}

#[test]
fn restart_rerequests_the_alarm_fires_once_and_records_the_gap() {
    let Some(daemon) = booted("restart") else {
        return;
    };
    daemon.eventually("the first fire", || fires(&daemon) >= 1);
    let root = daemon.root.clone();
    // The process dies hard: firing law #3 (state persists across daemon
    // restarts) is proven through the crash path, because at this pin a
    // clean SIGINT withdraws the fibers' fs contributions — the persisted
    // schedule with them (FINDINGS.md #14; pinned by
    // `a_clean_shutdown_withdraws_the_fibers_persisted_contribution`).
    daemon.kill();
    let fires_before = fires_at(&root);
    // Sleep across several boundaries: alarms do not survive a restart
    // (the contract says so), and the daemon is down anyway.
    let gap_periods = 3;
    std::thread::sleep(Duration::from_millis(JOB_PERIOD_MS * gap_periods + TICK_MS));

    // Reboot over the same root: state and history persist (firing law
    // #3), the scheduler re-requests its alarm in `activate`, and its
    // activate-time plan lands the catch-up at once: the newest elapsed
    // boundary fires (one catch-up, `missed-before` > 0), the earlier ones
    // are one skipped record — no backfill.
    let binary = gate().expect("gate already passed");
    let daemon = Daemon::boot(binary, &root);
    daemon.eventually("the catch-up fire", || fires(&daemon) > fires_before);
    // The gap is one skipped record; the catch-up is the fire on the
    // boundary right after it (history is the stable evidence — the
    // consumer's report may already show a later, ordinary fire).
    daemon.eventually("the gap to be recorded", || {
        history(&daemon)
            .iter()
            .any(|record| record["outcome"]["skipped"].is_object())
    });
    let skipped = history(&daemon)
        .into_iter()
        .find(|record| record["outcome"]["skipped"].is_object())
        .expect("the gap is recorded");
    let boundaries = skipped["outcome"]["skipped"]["boundaries"]
        .as_u64()
        .expect("boundaries");
    assert!(
        boundaries >= gap_periods - 1,
        "the downtime spans boundaries: {skipped}"
    );
    let first = skipped["outcome"]["skipped"]["first-ms"]
        .as_u64()
        .expect("first-ms");
    let last = skipped["outcome"]["skipped"]["last-ms"]
        .as_u64()
        .expect("last-ms");
    assert_eq!(last, first + (boundaries - 1) * JOB_PERIOD_MS, "{skipped}");
    let caught_up = last + JOB_PERIOD_MS;
    let catch_up = fired_records(&daemon)
        .into_iter()
        .find(|record| record["scheduled-ms"] == caught_up)
        .expect("exactly the boundary after the gap fired");
    assert_eq!(
        catch_up["tick-seq"], 0,
        "the catch-up is the activate plan: {catch_up}"
    );
    assert!(
        fired_records(&daemon)
            .iter()
            .all(|record| record["scheduled-ms"].as_u64() != Some(first)),
        "no backfill of a skipped boundary"
    );
    let run_record = daemon
        .data_json(&format!("cron/runs/health/{caught_up}.json"))
        .expect("the catch-up's per-fire record");
    assert_eq!(run_record["outcome"]["fired"]["answers"], 1, "{run_record}");
    // The re-request is on the record: the reboot's ledger carries a second
    // alarm registration (the first died with the process, unwithdrawn —
    // restarts drop alarms; the contract's honest bound).
    assert_eq!(
        daemon.ledger_count(&format!(
            r#"EffectRegistered":{{"label":"{}""#,
            alarm_label()
        )),
        2,
        "alarm re-requested on activate"
    );
    daemon.interrupt();
}

#[test]
fn a_clean_shutdown_withdraws_the_fibers_persisted_contribution() {
    // FINDINGS.md #14, pinned as a reproducible transcript: at this pin
    // every `jinn:fs` mutation joins its fiber's journal, and the daemon's
    // graceful shutdown disposes every fiber — so a clean SIGINT withdraws
    // the scheduler's persisted schedule state and history append, and the
    // consumer's report, LIFO, on the record. The seam has no durable-state
    // lane. When the kernel retires the finding this test goes red, and
    // the restart tests above return to the clean path deliberately.
    let Some(daemon) = booted("clean-stop") else {
        return;
    };
    daemon.eventually("the first fire", || fires(&daemon) >= 1);
    daemon.interrupt();
    let kinds = daemon_kinds_at(&daemon_root("clean-stop")).join("\n");
    for label in [
        "fs write cron/state.json",
        "fs append cron/history.jsonl",
        "fs write health/report.json",
    ] {
        assert!(
            kinds.contains(&format!(r#"EffectWithdrawn":{{"label":"{label}"#)),
            "{label} is withdrawn by the clean shutdown: {kinds}"
        );
    }
}

/// The run root of a booted-then-stopped daemon named by `booted`.
fn daemon_root(name: &str) -> PathBuf {
    composition::daemon::workspace_root()
        .join("target/composition/runs")
        .join(format!("{name}-{}", std::process::id()))
}

/// Every ledger `kind` of a stopped daemon's root.
fn daemon_kinds_at(root: &std::path::Path) -> Vec<String> {
    let connection = rusqlite::Connection::open(root.join("ledger.sqlite")).expect("ledger");
    let mut select = connection
        .prepare("SELECT kind FROM events ORDER BY seq")
        .expect("schema");
    let kinds = select
        .query_map([], |row| row.get::<_, String>(0))
        .expect("reads")
        .collect::<Result<Vec<_>, _>>()
        .expect("rows");
    kinds
}

#[test]
fn disposing_the_scheduler_cancels_its_alarm_and_leaves_a_clean_trail() {
    let Some(daemon) = booted("dispose") else {
        return;
    };
    daemon.eventually("the first wake", || daemon.ledger_count("AlarmWake") >= 1);
    daemon.edit_profile(|document| {
        let entries = document["entries"].as_array_mut().expect("entries");
        entries.retain(|entry| entry["id"] != "cron-scheduler");
    });
    daemon.eventually("the scheduler to dispose", || {
        daemon
            .log()
            .contains(r#"disposed=[EntryId("cron-scheduler")]"#)
    });
    let kinds = daemon.ledger_kinds().join("\n");
    assert!(
        kinds.contains("cron-scheduler on duty") && kinds.contains("EffectWithdrawn"),
        "the guest's effect is withdrawn on the ledger: {kinds}"
    );
    assert!(
        kinds.contains(&format!(
            r#"EffectWithdrawn":{{"label":"{}""#,
            alarm_label()
        )),
        "the alarm effect is withdrawn (its undo cancels): {kinds}"
    );
    assert!(
        kinds.contains("ServiceWithdrawn"),
        "the cron provision is withdrawn: {kinds}"
    );
    // Cancelled means cancelled: no wake lands after the withdrawal.
    let wakes = daemon.ledger_count("AlarmWake");
    std::thread::sleep(Duration::from_millis(TICK_MS * 4));
    assert_eq!(
        daemon.ledger_count("AlarmWake"),
        wakes,
        "no wake after dispose"
    );
    daemon.interrupt();
}

#[test]
fn the_clock_grant_gates_the_scheduler() {
    let Some(daemon) = booted("clock-grant") else {
        return;
    };
    // Withdraw jinn:clock from the scheduler (the profile side's authority
    // decision): its activation is refused at the alarm request, the
    // refusal is ledgered, and the fiber fails loudly — contained (R11),
    // never a silently timeless scheduler.
    daemon.edit_profile(|document| {
        entry_config(document, "cron-scheduler")["grants"] =
            serde_json::json!([jinn_cron::CRON_CONTRACT, "jinn:fs"]);
    });
    daemon.eventually(
        "the ungranted clock call to be refused on the ledger",
        || {
            daemon
                .ledger_kinds()
                .iter()
                .any(|kind| kind.contains("GrantRefused") && kind.contains("jinn:clock"))
        },
    );
    daemon.eventually("the scheduler to fail its activation", || {
        daemon
            .ledger_kinds()
            .iter()
            .any(|kind| kind.contains("FiberTransition") && kind.contains(r#""to":"Failed""#))
    });
    daemon.interrupt();
}

#[test]
fn the_cron_grant_gates_the_consumer_peek() {
    let Some(daemon) = booted("grants") else {
        return;
    };
    // Positive control: re-activate the consumer alone (nonce bump) — the
    // peek deterministically sees the job table.
    daemon.edit_profile_until_restart("health-snapshot", |document| {
        entry_config(document, "health-snapshot")["data"]["nonce"] = serde_json::json!(1);
    });
    daemon.eventually("the granted peek to see the job table", || {
        daemon
            .data_json("health/boot.json")
            .is_some_and(|boot| boot["cron"]["jobs"][0]["id"] == "health")
    });
    // Withdraw the jinn:cron grant (the profile side's authority decision):
    // the peek is refused, the refusal is ledgered, and the consumer stays
    // honest about it.
    daemon.edit_profile_until_restart("health-snapshot", |document| {
        let config = entry_config(document, "health-snapshot");
        config["grants"] = serde_json::json!(["cron:health", "jinn:fs"]);
        config["data"]["nonce"] = serde_json::json!(2);
    });
    daemon.eventually("the ungranted peek to record unavailable", || {
        daemon
            .data_json("health/boot.json")
            .is_some_and(|boot| boot["cron"]["unavailable"].is_string())
    });
    let kinds = daemon.ledger_kinds().join("\n");
    assert!(
        kinds.contains("GrantRefused"),
        "the refusal is a ledger event (Law 1/2): {kinds}"
    );
    daemon.interrupt();
}
