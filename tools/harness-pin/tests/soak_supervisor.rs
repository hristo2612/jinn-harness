//! The soak supervisor gate (`cargo test -p harness-pin`).
//!
//! PLA-306 puts the soak daemon under a user LaunchAgent so a host reboot is
//! a counted soak event rather than a silent outage. Two of the packet's
//! bounds are mechanical, so they are a test rather than a promise:
//!
//! - the tracked assets carry NO machine paths (the repo goes public; the
//!   plist is a template whose only absolute path is a placeholder the
//!   installer fills from `$HOME`), and
//! - the plist declares the supervision contract SOAK.md documents:
//!   `RunAtLoad`, a `KeepAlive` that does NOT fight a clean planned stop,
//!   and a `ThrottleInterval`.

use std::path::{Path, PathBuf};
use std::process::Command;

fn soak_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/soak")
        .canonicalize()
        .expect("tools/soak")
}

fn read(name: &str) -> String {
    let path = soak_dir().join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Every tracked soak asset is machine-neutral: the runtime root is derived
/// from `$HOME` at install time, never written down here.
#[test]
fn tracked_assets_carry_no_machine_paths() {
    for name in [
        "soak-run.sh",
        "install-launchd.sh",
        "run.jinn.harness-soak.plist.template",
        "detach.py",
    ] {
        let text = read(name);
        for (n, line) in text.lines().enumerate() {
            assert!(
                !line.contains("/Users/") && !line.contains("/home/"),
                "{name}:{} carries a machine path: {line}",
                n + 1
            );
        }
    }
}

/// The plist is a template: its one absolute path is the placeholder the
/// installer substitutes, and it declares the supervision contract.
#[test]
fn plist_template_declares_the_supervision_contract() {
    let plist = read("run.jinn.harness-soak.plist.template");
    assert!(
        plist.contains("__SOAK__/bin/soak-run.sh"),
        "the template must run the wrapper from the substituted runtime root"
    );
    assert!(
        plist.contains("<key>Label</key>"),
        "a LaunchAgent needs its label"
    );
    assert!(
        plist.contains("run.jinn.harness-soak"),
        "the label is the one SOAK.md and the Todo name"
    );
    assert!(
        plist.contains("<key>RunAtLoad</key>"),
        "RunAtLoad is what makes a host reboot restart the soak"
    );
    assert!(
        plist.contains("<key>KeepAlive</key>"),
        "KeepAlive is what makes a crash a counted restart"
    );
    // A bare `KeepAlive=true` would relaunch after the SIGINT planned stop
    // and fight SOAK.md §Stop. `SuccessfulExit=false` restarts only an
    // UNCLEAN exit: the daemon exits 0 on a clean suspend-and-flush.
    assert!(
        plist.contains("<key>SuccessfulExit</key>"),
        "KeepAlive must be conditioned so a clean planned stop stays stopped"
    );
    assert!(
        plist.contains("<key>ThrottleInterval</key>"),
        "a crash loop must be bounded"
    );
}

/// The wrapper and the installer are the operator's hands: they must at
/// least parse, and the wrapper must log its start reason to `ops.log`.
#[test]
fn shell_assets_parse_and_log_their_reason() {
    for name in ["soak-run.sh", "install-launchd.sh"] {
        let status = Command::new("/bin/sh")
            .arg("-n")
            .arg(soak_dir().join(name))
            .status()
            .expect("/bin/sh");
        assert!(status.success(), "{name} is not valid POSIX shell");
    }
    let wrapper = read("soak-run.sh");
    assert!(
        wrapper.contains("started (launchd; reason="),
        "the wrapper must append the supervisor's start line to ops.log"
    );
    assert!(
        wrapper.contains("keepalive-restart"),
        "a supervisor restart must be distinguishable from a boot in the audit"
    );
    assert!(
        wrapper.contains("ops.log"),
        "ops.log is the soak's evidence surface"
    );
}

/// The wrapper's account of an unplanned restart must be checkable, not
/// asserted (PLA-297, 2026-08-29: a KeepAlive relaunch after a SIGTERM was
/// written as `reason=boot` on a host that had not rebooted). The decision
/// keys on the host's boot time against the previous start — never on a
/// scratch file that can go missing — and the previous instance's end is
/// recorded from what launchd retained, before the start line.
#[test]
fn wrapper_decides_boot_from_uptime_and_records_the_previous_end() {
    let wrapper = read("soak-run.sh");
    for needle in [
        "kern.boottime",
        "LastExitStatus",
        "first-supervised-start",
        "SOAK_DRY_RUN",
        "killed by signal",
    ] {
        assert!(wrapper.contains(needle), "wrapper lacks {needle:?}");
    }
    assert!(
        !wrapper.contains("launchd.hostboot"),
        "the boot stamp file is retired: uptime is the evidence"
    );
}

/// Drives the wrapper's decision in dry-run mode over a scratch root with
/// a stub `launchctl`: each start reason and the decoded previous end.
#[cfg(target_os = "macos")]
#[test]
fn wrapper_dry_run_classifies_each_start() {
    let root = std::env::temp_dir().join(format!("soak-dry-run-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for dir in ["logs", "run", "stub"] {
        std::fs::create_dir_all(root.join(dir)).expect("scratch dir");
    }
    // A colored daemon log whose last timestamp is the last-seen-alive bound.
    std::fs::write(
        root.join("logs/jinnd.log"),
        "\u{1b}[2m2026-08-29T14:18:19.162106Z\u{1b}[0m INFO fs effect\n",
    )
    .expect("log");
    let stub = root.join("stub/launchctl");
    std::fs::write(
        &stub,
        "#!/bin/sh\nprintf '{\\n\\t\"LastExitStatus\" = 15;\\n};\\n'\n",
    )
    .expect("stub");
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(&stub, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let path = format!(
        "{}:{}",
        root.join("stub").display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let dry_run = |root: &Path| -> String {
        let output = Command::new("/bin/sh")
            .arg(soak_dir().join("soak-run.sh"))
            .env("SOAK", root)
            .env("SOAK_DRY_RUN", "1")
            .env("PATH", &path)
            .output()
            .expect("/bin/sh");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    };

    // No previous pid: provenance unknown, never asserted as a boot.
    assert!(dry_run(&root).contains("reason=first-supervised-start"));

    // Previous start AFTER this host booted, previous end SIGTERM: a
    // KeepAlive restart, with the death decoded and the last-seen bound.
    std::fs::write(root.join("run/jinnd.pid"), "75738\n").expect("pid");
    let out = dry_run(&root);
    assert!(out.contains("reason=keepalive-restart-consistent"), "{out}");
    assert!(out.contains("killed by signal 15"), "{out}");
    assert!(
        out.contains("last_seen=2026-08-29T14:18:19.162106Z"),
        "{out}"
    );

    // Previous start BEFORE this host booted: the daemon died with the host.
    let touch = Command::new("touch")
        .args(["-t", "200001010000"])
        .arg(root.join("run/jinnd.pid"))
        .status()
        .expect("touch");
    assert!(touch.success());
    assert!(dry_run(&root).contains("reason=boot-consistent"));

    // An operator's reason file wins, and dry-run does not consume it.
    std::fs::write(root.join("run/launchd.reason"), "planned-start").expect("reason");
    assert!(dry_run(&root).contains("reason=planned-start"));
    assert!(
        root.join("run/launchd.reason").is_file(),
        "dry-run consumed the reason"
    );
    let _ = std::fs::remove_dir_all(&root);
}

// ---------------------------------------------------------------------------
// The inverted default (PLA-297, 2026-08-30).
//
// Three degradation paths in a row produced a confident `reason=boot` out of
// an input nobody managed to read: a vanished stamp file; an unreadable
// `kern.boottime` falling back to a zero epoch; and a torn previous-start
// record whose missing mtime defaulted to `0`, making `boottime > prev_start`
// trivially true. Patching each path in turn failed three times, so the
// default inverts, as jinnd M2-K9 inverted serial dispatch:
//
//   `boot` requires POSITIVE PROOF FROM BOTH SIDES — a host boot time the
//   wrapper can prove it read AND a coherent previous-start record, with the
//   boot strictly after that start. Every other outcome, imagined or not,
//   resolves to `unknown` by construction.
//
// A claim is derived from proof, never from the absence of a contradiction.
//
// The tests below are one per input: make that read fail or tear, and assert
// the wrapper claims nothing.

/// A scratch runtime root with a stub directory first on `PATH`.
#[cfg(target_os = "macos")]
struct Scratch {
    root: PathBuf,
    path: String,
}

#[cfg(target_os = "macos")]
impl Scratch {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!("soak-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["logs", "run", "stub"] {
            std::fs::create_dir_all(root.join(dir)).expect("scratch dir");
        }
        std::fs::write(
            root.join("logs/jinnd.log"),
            "2026-08-29T14:18:19.162106Z fire\n",
        )
        .expect("log");
        let path = format!(
            "{}:{}",
            root.join("stub").display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let scratch = Self { root, path };
        // launchd retained the previous instance's SIGTERM, so the death is
        // readable in every case below. What is not readable is the one input
        // under test — and that alone must sink the claim.
        scratch.stub(
            "launchctl",
            "#!/bin/sh\nprintf '{\\n\\t\"LastExitStatus\" = 15;\\n};\\n'\n",
        );
        scratch
    }

    fn stub(&self, name: &str, body: &str) {
        use std::os::unix::fs::PermissionsExt as _;
        let path = self.root.join("stub").join(name);
        std::fs::write(&path, body).expect("stub");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    /// A previous-start record whose mtime predates every plausible host
    /// boot: were the pair trusted, the wrapper would answer `boot`. That is
    /// what makes "not boot" a real assertion rather than an accident of
    /// timing.
    fn ancient_pid_record(&self, kind: &str) {
        let target = self.root.join("run/jinnd.pid");
        match kind {
            "file" => std::fs::write(&target, "75738\n").expect("pid"),
            // A directory is a read failure for every uid, root included:
            // `stat` still answers, `cat` cannot.
            "dir" => std::fs::create_dir(&target).expect("pid dir"),
            other => panic!("unknown record kind {other}"),
        }
        let touched = Command::new("touch")
            .args(["-t", "200001010000"])
            .arg(&target)
            .status()
            .expect("touch");
        assert!(touched.success());
    }

    fn dry_run(&self) -> String {
        let output = Command::new("/bin/sh")
            .arg(soak_dir().join("soak-run.sh"))
            .env("SOAK", &self.root)
            .env("SOAK_DRY_RUN", "1")
            .env("PATH", &self.path)
            .output()
            .expect("/bin/sh");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
}

#[cfg(target_os = "macos")]
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// INPUT 1 — the host boot time. `sysctl` answers nothing (absent, or its
/// `{ sec = N, usec = M }` shape reshaped under a future macOS). Half the
/// evidence for `boot` is missing, so no boot is claimed and no boot time is
/// printed: `unknown`, never the zero epoch that `date -r 0` prints as 1970.
#[cfg(target_os = "macos")]
#[test]
fn an_unreadable_host_boot_time_claims_nothing() {
    let scratch = Scratch::new("no-boottime");
    scratch.ancient_pid_record("file");
    scratch.stub("sysctl", "#!/bin/sh\nexit 1\n");
    let out = scratch.dry_run();
    assert!(out.contains("host_boot=unknown"), "{out}");
    assert!(
        !out.contains("1970"),
        "a zero epoch is not a boot time: {out}"
    );
    assert!(out.contains("reason=unknown"), "{out}");
}

/// INPUT 2 — the previous pid. The record is there and `stat` answers, but
/// the contents cannot be read. Half the pair is missing, so the pair is not
/// a previous start: nothing is claimed, and `first-supervised-start` is NOT
/// inferred from a read failure — only a proven ABSENCE earns that.
#[cfg(target_os = "macos")]
#[test]
fn an_unreadable_previous_pid_claims_nothing() {
    let scratch = Scratch::new("unreadable-pid");
    scratch.ancient_pid_record("dir");
    let out = scratch.dry_run();
    assert!(out.contains("reason=unknown"), "{out}");
    assert!(!out.contains("first-supervised-start"), "{out}");
}

/// INPUT 3 — the previous start's mtime, TORN. The verifier's FIFO probe in
/// deterministic form: the record answers once and is gone by the second
/// look, so the pid and the mtime cannot be proven to describe one record.
/// The old wrapper defaulted the missing mtime to `0`, which made
/// `boottime > prev_start` trivially true and returned `reason=boot` at rc 0.
/// A torn record is not a previous start.
#[cfg(target_os = "macos")]
#[test]
fn a_torn_previous_start_record_claims_nothing() {
    let scratch = Scratch::new("torn-record");
    scratch.ancient_pid_record("file");
    scratch.stub(
        "stat",
        // Answers the first look with an ancient mtime, then vanishes.
        "#!/bin/sh\nc=$0.calls\nn=$(cat \"$c\" 2>/dev/null || echo 0)\n\
         n=$((n+1)); printf '%s' \"$n\" >\"$c\"\n\
         [ \"$n\" = 1 ] || exit 1\nprintf '946684800\\n'\n",
    );
    let out = scratch.dry_run();
    assert!(out.contains("reason=unknown"), "{out}");
    assert!(!out.contains("reason=boot"), "{out}");
}

/// The same tear the other way round: both looks answer, but with different
/// mtimes — the record was replaced between them. Two reads of two records
/// are not one previous start either.
#[cfg(target_os = "macos")]
#[test]
fn a_previous_start_record_replaced_mid_read_claims_nothing() {
    let scratch = Scratch::new("replaced-record");
    scratch.ancient_pid_record("file");
    scratch.stub(
        "stat",
        "#!/bin/sh\nc=$0.calls\nn=$(cat \"$c\" 2>/dev/null || echo 0)\n\
         n=$((n+1)); printf '%s' \"$n\" >\"$c\"\n\
         if [ \"$n\" = 1 ]; then printf '946684800\\n'; else printf '946684801\\n'; fi\n",
    );
    let out = scratch.dry_run();
    assert!(out.contains("reason=unknown"), "{out}");
    assert!(!out.contains("reason=boot"), "{out}");
}

/// The proven lane still answers. With a readable boot time and a coherent
/// record, `boot` and `keepalive-restart` are both still reachable — the
/// inversion must not have made the wrapper useless, only honest.
#[cfg(target_os = "macos")]
#[test]
fn both_sides_proven_still_decides() {
    let scratch = Scratch::new("proven");
    // Previous start BEFORE this host booted: the daemon died with the host.
    scratch.ancient_pid_record("file");
    let out = scratch.dry_run();
    assert!(out.contains("reason=boot-consistent"), "{out}");
    assert!(out.contains("prev_pid=75738"), "{out}");
    assert!(out.contains("killed by signal 15"), "{out}");
    // Previous start AFTER it: launchd replaced a daemon that ended uncleanly.
    std::fs::write(scratch.root.join("run/jinnd.pid"), "75738\n").expect("pid");
    let out = scratch.dry_run();
    assert!(out.contains("reason=keepalive-restart-consistent"), "{out}");
}

/// The absence of a previous record is evidence only where the wrapper could
/// actually look. An unenumerable run directory is a read failure, not proof
/// of a first start — and certainly not of a boot.
#[cfg(target_os = "macos")]
#[test]
fn an_unenumerable_run_directory_is_not_a_first_supervised_start() {
    let root = std::env::temp_dir().join(format!("soak-no-run-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let output = Command::new("/bin/sh")
        .arg(soak_dir().join("soak-run.sh"))
        .env("SOAK", &root)
        .env("SOAK_DRY_RUN", "1")
        .output()
        .expect("/bin/sh");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let out = String::from_utf8_lossy(&output.stdout);
    assert!(out.contains("reason=unknown"), "{out}");
    assert!(!out.contains("first-supervised-start"), "{out}");
    let _ = std::fs::remove_dir_all(&root);
}

/// A run directory the wrapper CAN enumerate, with no record in it: that
/// absence is the evidence, and `first-supervised-start` is earned.
#[cfg(target_os = "macos")]
#[test]
fn a_proven_absent_record_is_a_first_supervised_start() {
    let scratch = Scratch::new("first-start");
    let out = scratch.dry_run();
    assert!(out.contains("reason=first-supervised-start"), "{out}");
}

/// The dry run's documented promise is that it touches nothing. It was
/// creating `logs/` and `run/` under the supplied root before printing its
/// decision — a write; and on a root that did not exist, it was the only
/// thing that made the root exist at all.
#[cfg(target_os = "macos")]
#[test]
fn dry_run_touches_nothing() {
    let root = std::env::temp_dir().join(format!("soak-untouched-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let output = Command::new("/bin/sh")
        .arg(soak_dir().join("soak-run.sh"))
        .env("SOAK", &root)
        .env("SOAK_DRY_RUN", "1")
        .output()
        .expect("/bin/sh");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!root.exists(), "dry run created {}", root.display());
}

/// The construction, not the guard: no read may fall back to a value that
/// reads like a measurement. `0`, an empty string and a zero epoch are all
/// values a comparison happily believes.
#[test]
fn no_read_falls_back_to_a_value_that_looks_measured() {
    let wrapper = read("soak-run.sh");
    for banned in ["|| echo 0", "||echo 0", ":-0}"] {
        assert!(
            !wrapper.contains(banned),
            "a read falls back to a measured-looking default: {banned:?}"
        );
    }
    assert!(
        wrapper.contains("reason=unknown"),
        "every unproven path must resolve to reason=unknown"
    );
}

// ---------------------------------------------------------------------------
// Evidence on the line, and the conclusion labelled a derivation
// (PLA-297 round 3, 2026-08-30).
//
// Round 2 inverted the READING: every input yields a value the wrapper can
// prove it read, or `unknown`. What no care at the read site can invert is the
// record's IDENTITY — `stat`-after-read proves "I read a pid and an mtime
// together", never "this mtime belongs to that pid". A replacement preserving
// the mtime defeats it, and a fifth construction would defeat the fifth guard.
//
// So the claim changes rather than the guard. `boot` and `keepalive-restart`
// are causal statements about the host that a boot time and a file mtime
// cannot establish; the wrapper says only that its readings are CONSISTENT
// with them, and prints those readings verbatim beside the inference so a
// human auditor can see through a wrong input.

#[cfg(target_os = "macos")]
impl Scratch {
    /// A `stat` that answers every look with one fixed epoch — the mtime is
    /// then the test's, not the local timezone's, so the rendered ISO is
    /// deterministic wherever the gate runs.
    fn fixed_mtime(&self, secs: &str) {
        self.stub("stat", &format!("#!/bin/sh\nprintf '{secs}\\n'\n"));
    }
}

/// The line carries its inputs, not only its conclusion: the host boot time as
/// read, the previous record's status, its pid and mtime as read, launchd's
/// status raw AND decoded, the last-seen bound, and — explicitly — that
/// nothing was unread. An auditor who distrusts the inference can redo it.
#[cfg(target_os = "macos")]
#[test]
fn the_decision_carries_its_evidence_verbatim() {
    let scratch = Scratch::new("evidence");
    scratch.ancient_pid_record("file");
    scratch.fixed_mtime("946684800");
    let out = scratch.dry_run();
    for field in [
        "prev_record=present",
        "prev_pid=75738",
        "prev_start_sec=946684800",
        "prev_start=2000-01-01T00:00:00Z",
        "prev_end_raw=15",
        "prev_end=\"killed by signal 15 (SIGTERM)\"",
        "last_seen=2026-08-29T14:18:19.162106Z",
        "unproven=none",
    ] {
        assert!(out.contains(field), "evidence lacks {field:?}: {out}");
    }
    // The host boot time is the live host's, so only its shape is asserted —
    // but it must be BOTH forms: the raw reading and its rendering.
    assert!(out.contains("host_boot_sec="), "{out}");
    assert!(out.contains("host_boot="), "{out}");
    assert!(!out.contains("host_boot_sec=unknown"), "{out}");
}

/// `unproven=` is an observation, not an artifact of the unknown branch: a
/// decision that rests on complete readings says so, in the same field the
/// unknown lane uses to name what it could not read.
#[cfg(target_os = "macos")]
#[test]
fn an_unread_input_is_named_on_every_line_including_the_proven_one() {
    let proven = Scratch::new("unproven-none");
    proven.ancient_pid_record("file");
    proven.fixed_mtime("946684800");
    assert!(proven.dry_run().contains("unproven=none"));

    let torn = Scratch::new("unproven-named");
    torn.ancient_pid_record("dir");
    let out = torn.dry_run();
    assert!(out.contains("unproven=previous-start-record"), "{out}");
    assert!(!out.contains("unproven=none"), "{out}");
}

/// The conclusion is labelled a derivation. A bare `boot` reads as an
/// established fact about the host; `boot-consistent` reads as what it is —
/// an inference from the two readings printed beside it. The same holds for
/// its twin, which claims the host did NOT reboot from exactly those readings.
#[cfg(target_os = "macos")]
#[test]
fn the_conclusion_is_labelled_a_derivation_not_an_assertion() {
    let scratch = Scratch::new("derivation");
    scratch.ancient_pid_record("file");
    scratch.fixed_mtime("946684800");
    let out = scratch.dry_run();
    assert!(out.contains("reason=boot-consistent"), "{out}");

    // The previous start is later than any plausible host boot: the readings
    // say this host never rebooted under it.
    scratch.fixed_mtime("4102444800");
    let out = scratch.dry_run();
    assert!(out.contains("reason=keepalive-restart-consistent"), "{out}");
}

/// The vocabulary is the wrapper's, not only the printer's: the branches that
/// write `ops.log` dispatch on the derived names too, so no line can be
/// reached that words the same inference as a fact.
#[test]
fn no_branch_words_a_derivation_as_a_fact() {
    let wrapper = read("soak-run.sh");
    assert!(
        wrapper.contains("boot-consistent)"),
        "the boot branch is a derivation"
    );
    assert!(
        wrapper.contains("keepalive-restart-consistent)"),
        "the restart branch is a derivation"
    );
    for bare in ["    boot)", "    keepalive-restart)"] {
        assert!(
            !wrapper.contains(bare),
            "a branch still asserts {bare:?} as established fact"
        );
    }
}

/// The evidence has to reach the file the audit actually reads. This drives
/// the wrapper's REAL start path over a scratch root with a stub daemon: both
/// the death line and the start line carry the same record, built once.
#[cfg(target_os = "macos")]
#[test]
fn the_ops_log_lines_carry_the_evidence_and_the_derivation() {
    let scratch = Scratch::new("ops-log");
    scratch.ancient_pid_record("file");
    scratch.fixed_mtime("946684800");
    std::fs::create_dir_all(scratch.root.join("bin")).expect("bin");
    std::fs::write(scratch.root.join("bin/jinnd"), "#!/bin/sh\nexit 0\n").expect("stub daemon");
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(
        scratch.root.join("bin/jinnd"),
        std::fs::Permissions::from_mode(0o755),
    )
    .expect("chmod");
    std::fs::write(scratch.root.join("bin/jinnd.commit"), "3fd7b05\n").expect("commit");

    let status = Command::new("/bin/sh")
        .arg(soak_dir().join("soak-run.sh"))
        .env("SOAK", &scratch.root)
        .env("PATH", &scratch.path)
        .status()
        .expect("/bin/sh");
    assert!(
        status.success(),
        "the wrapper failed on its real start path"
    );

    let ops = std::fs::read_to_string(scratch.root.join("logs/ops.log")).expect("ops.log");
    let death = ops
        .lines()
        .find(|l| l.contains("previous jinnd"))
        .unwrap_or_else(|| panic!("no death line: {ops}"));
    let start = ops
        .lines()
        .find(|l| l.contains("started (launchd"))
        .unwrap_or_else(|| panic!("no start line: {ops}"));
    assert!(death.contains("boot-consistent"), "{death}");
    assert!(
        !death.contains("died with the host:"),
        "the death line still asserts the cause: {death}"
    );
    assert!(start.contains("reason=boot-consistent"), "{start}");
    for line in [death, start] {
        for field in [
            "prev_pid=75738",
            "prev_start_sec=946684800",
            "prev_end_raw=15",
            "unproven=none",
        ] {
            assert!(line.contains(field), "{field:?} missing from: {line}");
        }
    }
}

// ---------------------------------------------------------------------------
// One decode, one value (PLA-297 round 4, 2026-08-30).
//
// Round 3 decoded launchd's wait status into the `prev_end=` field and worded
// the ops.log narrative separately, as a literal. On a real start path with
// `LastExitStatus = 15` the two met on one line, at rc 0:
//
//   previous jinnd 75738 ended UNCLEAN; DERIVED keepalive-restart-consistent:
//   … launchd is relaunching a daemon that ended on its own …
//   prev_end="killed by signal 15 (SIGTERM)"
//
// "ended on its own" and "killed by signal 15" are the same line disagreeing
// with itself. An auditor reading it on 2026-09-04 learns only that the
// wrapper does not know what it is saying — which is the whole defect class
// this packet exists to close, arrived at from the printing side instead of
// the reading side.
//
// The fix is not a check that the two agree. It is that there is only one of
// them: the status is decoded ONCE into a kind, and the field, the narrative
// phrase and the clean/unclean token are all rendered from that single
// dispatch — the phrase EMBEDDING the field, so disagreement is not guarded
// against but unrepresentable.

#[cfg(target_os = "macos")]
impl Scratch {
    /// launchd retained this `LastExitStatus` for the label.
    fn launchctl_status(&self, raw: &str) {
        self.stub(
            "launchctl",
            &format!("#!/bin/sh\nprintf '{{\\n\\t\"LastExitStatus\" = {raw};\\n}};\\n'\n"),
        );
    }

    /// launchd retained nothing at all — a label it has never run, or a
    /// status it has since dropped.
    fn launchctl_retains_nothing(&self) {
        self.stub("launchctl", "#!/bin/sh\nexit 1\n");
    }

    /// The REAL start path over a stub daemon; returns the death line the
    /// audit reads. The dry run prints only the evidence record, so the
    /// narrative can only be caught here — which is where it hid.
    fn death_line(&self) -> String {
        std::fs::create_dir_all(self.root.join("bin")).expect("bin");
        let daemon = self.root.join("bin/jinnd");
        std::fs::write(&daemon, "#!/bin/sh\nexit 0\n").expect("stub daemon");
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&daemon, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let status = Command::new("/bin/sh")
            .arg(soak_dir().join("soak-run.sh"))
            .env("SOAK", &self.root)
            .env("PATH", &self.path)
            .status()
            .expect("/bin/sh");
        assert!(status.success(), "the wrapper failed on its real start path");
        let ops = std::fs::read_to_string(self.root.join("logs/ops.log")).expect("ops.log");
        ops.lines()
            .find(|l| l.contains("previous jinnd"))
            .unwrap_or_else(|| panic!("no death line: {ops}"))
            .to_owned()
    }
}

/// Across the whole exit-status space — a signal death, a clean exit, a dirty
/// exit, and no retained status at all — the prose the line opens with and the
/// `prev_end=` field it carries are the same decode. The narrative is asserted
/// to END with the decoded field verbatim: it does not merely agree with it,
/// it is rendered from it.
#[cfg(target_os = "macos")]
#[test]
fn the_narrative_and_the_decoded_status_cannot_disagree() {
    // (retained status, the decode it must produce, wordings that would
    // contradict that decode if the prose were written independently)
    let cases: [(Option<&str>, &str, &[&str]); 4] = [
        // The blocker, verbatim: this line said "ended on its own".
        (
            Some("15"),
            "killed by signal 15 (SIGTERM)",
            &["on its own", "CLEANLY"],
        ),
        // A clean exit. Calling it UNCLEAN is the same defect mirrored.
        (Some("0"), "exit 0", &["UNCLEAN", "killed by signal"]),
        // The daemon exited by itself, badly: 3 in the high byte.
        (Some("768"), "exit 3", &["killed by signal", "CLEANLY"]),
        // Nothing retained: neither clean nor killed may be claimed.
        (
            None,
            "end status unknown (launchd retained none)",
            &["UNCLEAN", "CLEANLY", "killed by signal"],
        ),
    ];
    for (raw, field, contradictions) in cases {
        let tag = raw.unwrap_or("none");
        let scratch = Scratch::new(&format!("agree-{tag}"));
        scratch.ancient_pid_record("file");
        match raw {
            Some(raw) => scratch.launchctl_status(raw),
            None => scratch.launchctl_retains_nothing(),
        }
        let line = scratch.death_line();
        // The narrative is everything before the derivation clause.
        let head = line
            .split("; DERIVED")
            .next()
            .expect("split")
            .split(", PROVENANCE")
            .next()
            .expect("split");
        assert!(
            head.ends_with(field),
            "the narrative does not render the decoded status {field:?}: {head}"
        );
        assert!(
            line.contains(&format!("prev_end=\"{field}\"")),
            "the field disagrees with the narrative: {line}"
        );
        for wrong in contradictions {
            assert!(
                !head.contains(wrong),
                "status {tag} narrated as {wrong:?}: {head}"
            );
        }
    }
}

/// One decode, in one place. Each `ops.log` line renders the phrase that
/// decode produced; none words the previous end for itself. There is nothing
/// left for a second statement to disagree with.
#[test]
fn the_previous_end_is_decoded_in_exactly_one_place() {
    let wrapper = read("soak-run.sh");
    assert_eq!(
        wrapper.matches("killed by signal $").count(),
        1,
        "the wait status is decoded in more than one place"
    );
    assert_eq!(
        wrapper.matches("\"$prev_end_phrase\"").count(),
        3,
        "every ops.log line must render the one decoded phrase"
    );
}
