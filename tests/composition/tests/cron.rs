//! The cron seam's real-composition gate (AGENTS.md standing order 3):
//! every proof boots the generated profile through the REAL pinned `jinnd`
//! daemon binary — reconcile through its file watcher, evidence from its
//! data root and its append-only ledger. Nothing is hand-mounted.
//!
//! Self-skips LOUDLY when no jinnd checkout holding the pinned commit is
//! reachable (jinnd is private; see KERNEL-PIN.md Gate 2). Locally the
//! sibling checkout makes this run everywhere the verify gate runs.

use std::path::PathBuf;
use std::sync::OnceLock;

use composition::daemon::{jinnd_source, pinned_commit, pinned_daemon};
use composition::kit::{entry_config, fresh_root, Daemon};

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

/// Boots a fresh root and waits for the trio's boot evidence (the
/// consumer's boot report write — the last activation effect of the tree).
fn booted(name: &str) -> Option<Daemon> {
    let binary = gate()?;
    let root = fresh_root(name);
    let daemon = Daemon::boot(binary, &root);
    daemon.eventually("the trio to boot (consumer boot report)", || {
        daemon.data("health/boot.json").is_file()
    });
    Some(daemon)
}

fn history(daemon: &Daemon) -> Vec<serde_json::Value> {
    daemon
        .data_json("cron/history.json")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
}

fn fires(daemon: &Daemon) -> u64 {
    daemon
        .data_json("health/report.json")
        .and_then(|report| report["fires"].as_u64())
        .unwrap_or(0)
}

#[test]
fn boots_the_trio_quiet_with_ledgered_admissions() {
    let Some(daemon) = booted("boot") else { return };
    // Quiet: time has not entered the seam (seed tick 0 never dispatches),
    // so nothing fired and no schedule started.
    assert!(!daemon.data("health/report.json").exists());
    assert!(!daemon.data("cron/history.json").exists());
    // The boot is ledger-visible: three artifact admissions, the cron
    // provision, and the consumer's boot-report write effect (Law 2).
    let kinds = daemon.ledger_kinds().join("\n");
    assert_eq!(kinds.matches("ArtifactLoaded").count(), 3, "{kinds}");
    assert!(kinds.contains("jinn:cron"), "provision recorded: {kinds}");
    assert!(kinds.contains("health/boot.json"), "boot write: {kinds}");
    daemon.interrupt();
}

#[test]
fn fires_on_schedule_and_records_the_run() {
    let Some(daemon) = booted("fire") else { return };
    // Tick 1 at the first boundary: the schedule STARTS (firing law #4) —
    // recorded, not fired.
    daemon.tick(1, 60_000);
    daemon.eventually("the schedule to start", || {
        history(&daemon)
            .iter()
            .any(|record| record["outcome"] == "schedule-started")
    });
    assert_eq!(fires(&daemon), 0, "law #4: no fire on a fresh schedule");
    // Tick 2 past the next boundary: exactly one fire; the consumer's
    // report and the scheduler's run record agree.
    daemon.tick(2, 121_000);
    daemon.eventually("the fire to land in the consumer report", || {
        fires(&daemon) == 1
    });
    let report = daemon.data_json("health/report.json").expect("report");
    assert_eq!(report["probe-ok"], true, "{report}");
    assert_eq!(report["last"]["scheduled-ms"], 120_000, "{report}");
    assert_eq!(report["last"]["missed-before"], 0, "{report}");
    let fired = history(&daemon)
        .into_iter()
        .find(|record| record["outcome"]["fired"].is_object())
        .expect("a fired run record");
    assert_eq!(fired["outcome"]["fired"]["answers"], 1, "{fired}");
    assert_eq!(fired["scheduled-ms"], 120_000, "{fired}");
    // Every crossing is in the ledger: the fs writes for state, history,
    // probe, and report are registered revertible effects (Law 2/3).
    let kinds = daemon.ledger_kinds().join("\n");
    for path in ["cron/state.json", "cron/history.json", "health/report.json"] {
        assert!(kinds.contains(path), "{path} write is ledgered");
    }
    daemon.interrupt();
}

#[test]
fn reschedules_on_config_edit_through_reconcile() {
    let Some(daemon) = booted("reschedule") else {
        return;
    };
    daemon.tick(1, 60_000);
    daemon.eventually("the schedule to start", || !history(&daemon).is_empty());
    // Operator lane: halve the period in the profile document. Only the
    // scheduler entry restarts (reconcile-by-id).
    daemon.edit_profile(|document| {
        entry_config(document, "cron-scheduler")["data"]["jobs"][0]["every-ms"] =
            serde_json::json!(30_000);
    });
    daemon.eventually("the scheduler to restart on its config edit", || {
        daemon
            .log()
            .contains(r#"restarted=[EntryId("cron-scheduler")]"#)
    });
    // 91s is past a 30s boundary (90s) but NOT past the next 60s boundary
    // (120s): a fire here proves the new schedule is live and the state
    // (last=60s) survived the restart.
    daemon.tick(2, 91_000);
    daemon.eventually("a fire on the halved schedule", || fires(&daemon) == 1);
    let report = daemon.data_json("health/report.json").expect("report");
    assert_eq!(report["last"]["scheduled-ms"], 90_000, "{report}");
    daemon.interrupt();
}

#[test]
fn restart_fires_once_and_records_the_gap_without_backfill() {
    let Some(daemon) = booted("restart") else {
        return;
    };
    daemon.tick(1, 60_000);
    daemon.eventually("the schedule to start", || !history(&daemon).is_empty());
    daemon.tick(2, 121_000);
    daemon.eventually("the first fire", || fires(&daemon) == 1);
    let root = daemon.root.clone();
    daemon.interrupt();

    // Reboot over the same root: state and history persist (firing law
    // #3). The re-activated tick entry replays tick 2; the firing law
    // absorbs the replay (no new boundary).
    let binary = gate().expect("gate already passed");
    let daemon = Daemon::boot(binary, &root);
    daemon.eventually("the trio to boot again", || {
        daemon.log().contains("reconciled")
    });
    assert_eq!(fires(&daemon), 1, "a boot replays no fire");
    // Five boundaries (180k..420k) elapsed while down: the newest fires
    // (one catch-up), the other four are one skipped record — no backfill.
    daemon.tick(3, 421_000);
    daemon.eventually("exactly one catch-up fire", || fires(&daemon) == 2);
    let report = daemon.data_json("health/report.json").expect("report");
    assert_eq!(report["last"]["scheduled-ms"], 420_000, "{report}");
    assert_eq!(report["last"]["missed-before"], 4, "{report}");
    let skipped = history(&daemon)
        .into_iter()
        .find(|record| record["outcome"]["skipped"].is_object())
        .expect("the gap is recorded");
    assert_eq!(skipped["outcome"]["skipped"]["boundaries"], 4, "{skipped}");
    assert_eq!(
        skipped["outcome"]["skipped"]["first-ms"], 180_000,
        "{skipped}"
    );
    assert_eq!(
        skipped["outcome"]["skipped"]["last-ms"], 360_000,
        "{skipped}"
    );
    daemon.interrupt();
}

#[test]
fn disposing_the_scheduler_leaves_a_clean_ledger_trail() {
    let Some(daemon) = booted("dispose") else {
        return;
    };
    daemon.tick(1, 60_000);
    daemon.eventually("the schedule to start", || !history(&daemon).is_empty());
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
        kinds.contains("ServiceWithdrawn"),
        "the cron provision is withdrawn: {kinds}"
    );
    daemon.interrupt();
}

#[test]
fn the_cron_grant_gates_the_consumer_peek() {
    let Some(daemon) = booted("grants") else {
        return;
    };
    // Positive control: re-activate the consumer alone (nonce bump) while
    // the scheduler is idle — the peek deterministically sees the job
    // table.
    daemon.edit_profile(|document| {
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
    daemon.edit_profile(|document| {
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
