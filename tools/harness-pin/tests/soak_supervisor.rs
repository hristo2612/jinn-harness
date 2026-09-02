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
        "record-build.sh",
        "run.jinn.harness-soak.plist.template",
        "detach.py",
        "provision-token.sh",
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
    for name in [
        "soak-run.sh",
        "install-launchd.sh",
        "record-build.sh",
        "provision-token.sh",
    ] {
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

/// The daemon body every scratch root installs unless a test replaces it.
/// Distinct bodies hash distinctly, which is the mechanism under test.
#[cfg(target_os = "macos")]
const STUB_DAEMON: &str = "#!/bin/sh\nexit 0\n";
/// The pin a coherent scratch root is running, and the one its harness ships.
#[cfg(target_os = "macos")]
const RUNNING_PIN: &str = "3a8e5c03fdbe2f21144faee8daba73beeb75d8b4";
#[cfg(target_os = "macos")]
const HARNESS_PIN: &str = "3a8e5c03fdbe2f21144faee8daba73beeb75d8b4";

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
        // The kernel it is running is an input too, so it is readable by
        // default and only the test under way makes it otherwise.
        scratch.install_daemon(STUB_DAEMON);
        scratch.build_record(&scratch.daemon_sha256(), RUNNING_PIN, HARNESS_PIN);
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
            // Whether a kernel checkout is reachable changes which reading the
            // wrapper can take, so it is stated here rather than inherited
            // from whatever ran the suite.
            .env_remove("JINND_DIR")
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
        let status = Command::new("/bin/sh")
            .arg(soak_dir().join("soak-run.sh"))
            .env("SOAK", &self.root)
            .env("PATH", &self.path)
            .status()
            .expect("/bin/sh");
        assert!(
            status.success(),
            "the wrapper failed on its real start path"
        );
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

// A wait status is not a claim about agency (PLA-297 round 5, 2026-08-30).
//
// Round 4 made the narrative and the field one decode. It left the clean
// branch wording that decode as `ended CLEANLY, on its own: exit 0` — and
// that is not merely unprovable, it is FALSE on the pinned daemon's own
// normal path: a planned stop sends SIGINT, the handler runs, and the
// process exits 0 (SOAK.md §Stop). The wrapper was asserting the daemon was
// not signalled at precisely the moment it was.
//
// `wait` rc 0 proves the EXIT STATUS. Whether a signal prompted the exit is
// a DIFFERENT reading, and the wrapper does not have it — nothing on this
// path retains a sender. So the clause is deleted rather than hedged: an
// unreadable fact gets no wording, not a softer one.

/// The premise, observed rather than cited: a process signalled EXTERNALLY
/// can still exit 0, so `LastExitStatus = 0` cannot carry "on its own".
/// This test signals a real process, watches it exit 0, and then requires the
/// wrapper to narrate that same status with no claim about agency.
#[cfg(target_os = "macos")]
#[test]
fn an_externally_signalled_exit_zero_is_never_narrated_as_agency() {
    let mut probe = Command::new("/bin/sh")
        .arg("-c")
        .arg("trap 'exit 0' TERM; while :; do sleep 0.05; done")
        .spawn()
        .expect("probe");
    // Let the shell install its trap before the signal arrives.
    std::thread::sleep(std::time::Duration::from_millis(300));
    let signalled = Command::new("kill")
        .arg("-TERM")
        .arg(probe.id().to_string())
        .status()
        .expect("kill");
    assert!(signalled.success(), "the probe could not be signalled");
    assert_eq!(
        probe.wait().expect("wait").code(),
        Some(0),
        "premise: an externally signalled process can still exit 0"
    );

    let scratch = Scratch::new("signalled-exit-zero");
    scratch.ancient_pid_record("file");
    scratch.launchctl_status("0");
    let line = scratch.death_line();
    assert!(
        line.contains("ended CLEANLY: exit 0"),
        "the clean status must be stated without a claim about agency: {line}"
    );
    assert!(
        !line.contains("on its own"),
        "exit 0 narrated as agency the wrapper never read: {line}"
    );
}

/// The same deletion, swept across every branch of the exit-status space: no
/// wording the wrapper can produce claims the daemon ended unprompted, because
/// no input it reads carries that.
#[cfg(target_os = "macos")]
#[test]
fn no_branch_of_the_exit_status_space_claims_agency() {
    for raw in [Some("15"), Some("0"), Some("768"), Some("2"), None] {
        let tag = raw.unwrap_or("none");
        let scratch = Scratch::new(&format!("no-agency-{tag}"));
        scratch.ancient_pid_record("file");
        match raw {
            Some(raw) => scratch.launchctl_status(raw),
            None => scratch.launchctl_retains_nothing(),
        }
        let line = scratch.death_line();
        for claim in ["on its own", "by itself", "unprompted", "was not signalled"] {
            assert!(
                !line.contains(claim),
                "status {tag} narrated as {claim:?}: {line}"
            );
        }
    }
}

// What it is RUNNING, derived (PLA-297 round 7, 2026-08-31).
//
// A COO drift audit found three sources disagreeing about which kernel the
// M2 duty soak was running: `meta.json` — the artifact the +7d audit reads —
// said `41cb2f47`; the installed binary and `ops.log` said `57360cc`;
// `KERNEL-PIN.md`, what M2 actually SHIPS, said `3a8e5c03`. A third pin bump
// happened and was never written to the file whose only job is to record
// exactly that, because a person writes that file by hand.
//
// No gate caught it and none could have: every one of those readings is
// internally consistent. The record was not damaged, it was STALE, and a
// stale record is indistinguishable from a current one when nothing binds it
// to the thing it describes.
//
// So it is bound. The wrapper hashes the binary it is about to exec and the
// pin is licensed by that hash matching an install record derived from the
// composition marker — never typed. A record that describes a DIFFERENT
// binary is not a weaker reading of the current one, it is a reading of
// something else, and the answer is `unknown` with the mismatch named. This
// is the same inversion the start reason took across six rounds, applied one
// level out: the wrapper told the truth about how it started while nothing
// told the truth about what it was running.
//
// And the two pins are two READINGS. `running_pin` is what is executing;
// `harness_pin` is what `KERNEL-PIN.md` says should be. They never share a
// field, because the drift the audit found is exactly the distance between
// them, and a field that can hold either cannot show it.

#[cfg(target_os = "macos")]
impl Scratch {
    /// The binary the wrapper is about to exec. Distinct bodies hash
    /// distinctly, which is the whole mechanism under test.
    fn install_daemon(&self, body: &str) {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::create_dir_all(self.root.join("bin")).expect("bin");
        let daemon = self.root.join("bin/jinnd");
        std::fs::write(&daemon, body).expect("stub daemon");
        std::fs::set_permissions(&daemon, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    /// The install record `tools/soak/record-build.sh` writes: the identity of
    /// the binary it was derived beside, and the two pins as two fields.
    fn build_record(&self, binary_sha256: &str, running_pin: &str, harness_pin: &str) {
        std::fs::create_dir_all(self.root.join("bin")).expect("bin");
        std::fs::write(
            self.root.join("bin/jinnd.build"),
            format!(
                "binary-sha256={binary_sha256}\n\
                 running-pin={running_pin}\n\
                 harness-pin={harness_pin}\n\
                 recorded-utc=2026-08-31T00:00:00Z\n"
            ),
        )
        .expect("build record");
    }

    /// The sha256 of the installed daemon, computed the way an auditor would.
    fn daemon_sha256(&self) -> String {
        let out = Command::new("shasum")
            .args(["-a", "256"])
            .arg(self.root.join("bin/jinnd"))
            .output()
            .expect("shasum");
        assert!(out.status.success(), "shasum failed");
        String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .next()
            .expect("digest")
            .to_owned()
    }
}

/// THE DRIFT FIXTURE. The record names a pin and a binary; the binary that
/// will actually run is a different one. Reporting the record's pin here is
/// precisely what `meta.json` did for two days, so the wrapper reports
/// nothing and names the mismatch.
#[cfg(target_os = "macos")]
#[test]
fn a_recorded_pin_that_does_not_describe_the_running_binary_is_never_reported() {
    let scratch = Scratch::new("pin-drift");
    scratch.ancient_pid_record("file");
    scratch.install_daemon("#!/bin/sh\n# the binary that is really here\nexit 0\n");
    // A record left behind by an earlier install: a real pin, a real digest,
    // and neither of them this binary's.
    scratch.build_record(
        &"a".repeat(64),
        "41cb2f47bdd18838e43096607f0b7c3e8800a61d",
        "3a8e5c03fdbe2f21144faee8daba73beeb75d8b4",
    );
    let out = scratch.dry_run();
    assert!(
        out.contains("running_pin=unknown"),
        "a record describing another binary was reported as this one's pin: {out}"
    );
    assert!(
        !out.contains("41cb2f47"),
        "the stale pin reached the record anyway: {out}"
    );
    assert!(
        out.contains("build-record-mismatch"),
        "the mismatch must be named, not merely absorbed: {out}"
    );
    // The identity that DID read is still reported: the auditor sees which
    // binary is running even when nothing can name its commit.
    assert!(
        out.contains(&format!("binary_sha256={}", scratch.daemon_sha256())),
        "the binary's own identity is a proven reading and is always reported: {out}"
    );
}

/// The licensed case: the record describes the binary that is about to run,
/// so the pin it names is this binary's pin.
#[cfg(target_os = "macos")]
#[test]
fn the_running_pin_is_derived_from_the_binary_that_is_about_to_run() {
    let scratch = Scratch::new("pin-derived");
    scratch.ancient_pid_record("file");
    scratch.install_daemon("#!/bin/sh\nexit 0\n");
    let sha = scratch.daemon_sha256();
    scratch.build_record(
        &sha,
        "3a8e5c03fdbe2f21144faee8daba73beeb75d8b4",
        "3a8e5c03fdbe2f21144faee8daba73beeb75d8b4",
    );
    let out = scratch.dry_run();
    assert!(
        out.contains("running_pin=3a8e5c03fdbe2f21144faee8daba73beeb75d8b4"),
        "{out}"
    );
    assert!(out.contains(&format!("binary_sha256={sha}")), "{out}");
    assert!(
        out.contains("unproven=none"),
        "nothing was unreadable here: {out}"
    );
}

/// No record at all. There is no last value to fall back to, and the harness
/// pin is not an answer to this question.
#[cfg(target_os = "macos")]
#[test]
fn an_absent_build_record_claims_no_pin() {
    let scratch = Scratch::new("pin-no-record");
    scratch.ancient_pid_record("file");
    std::fs::remove_file(scratch.root.join("bin/jinnd.build")).expect("rm record");
    let out = scratch.dry_run();
    assert!(out.contains("running_pin=unknown"), "{out}");
    assert!(out.contains("harness_pin=unknown"), "{out}");
    assert!(
        out.contains("build-record"),
        "the missing input is named: {out}"
    );
    // The binary is still there and still identifiable.
    assert!(
        out.contains(&format!("binary_sha256={}", scratch.daemon_sha256())),
        "{out}"
    );
}

/// The binary cannot be read — a directory in its place is a read failure for
/// every uid, root included. Nothing can be derived from an artifact that was
/// never read, so nothing is claimed about it.
#[cfg(target_os = "macos")]
#[test]
fn an_unreadable_running_binary_claims_no_pin() {
    let scratch = Scratch::new("pin-no-binary");
    scratch.ancient_pid_record("file");
    std::fs::remove_file(scratch.root.join("bin/jinnd")).expect("displace the daemon");
    std::fs::create_dir(scratch.root.join("bin/jinnd")).expect("daemon dir");
    scratch.build_record(
        &"b".repeat(64),
        "3a8e5c03fdbe2f21144faee8daba73beeb75d8b4",
        "3a8e5c03fdbe2f21144faee8daba73beeb75d8b4",
    );
    let out = scratch.dry_run();
    assert!(out.contains("binary_sha256=unknown"), "{out}");
    assert!(out.contains("running_pin=unknown"), "{out}");
    assert!(
        out.contains("running-binary"),
        "the unreadable artifact is named: {out}"
    );
    assert!(
        !out.contains("3a8e5c03fdbe2f21144faee8daba73beeb75d8b4\u{20}running_pin"),
        "{out}"
    );
}

/// Two readings, two fields, and the fallback direction that must not exist:
/// when the running pin is unprovable, the harness pin — which is READABLE
/// and looks like an answer — does not fill it. What it SHOULD be running is
/// a different question from what it IS running, and conflating them is the
/// drift itself.
#[cfg(target_os = "macos")]
#[test]
fn the_harness_pin_can_never_fill_the_running_pin_field() {
    let scratch = Scratch::new("pin-two-readings");
    scratch.ancient_pid_record("file");
    scratch.install_daemon("#!/bin/sh\nexit 0\n");
    // A soak two pins behind what the harness ships — the audit's finding,
    // as a fixture: the record is honest, the two pins simply differ.
    scratch.build_record(
        &scratch.daemon_sha256(),
        "57360ccd3e6493cc2d20e8e6e480daaa88486817",
        "3a8e5c03fdbe2f21144faee8daba73beeb75d8b4",
    );
    let out = scratch.dry_run();
    assert!(
        out.contains("running_pin=57360ccd3e6493cc2d20e8e6e480daaa88486817"),
        "{out}"
    );
    assert!(
        out.contains("harness_pin=3a8e5c03fdbe2f21144faee8daba73beeb75d8b4"),
        "the divergence is only visible if both readings are printed: {out}"
    );

    // Now break only the running side. The harness pin still reads.
    scratch.build_record(
        &"c".repeat(64),
        "57360ccd3e6493cc2d20e8e6e480daaa88486817",
        "3a8e5c03fdbe2f21144faee8daba73beeb75d8b4",
    );
    let out = scratch.dry_run();
    assert!(out.contains("running_pin=unknown"), "{out}");
    assert!(
        !out.contains("running_pin=3a8e5c03"),
        "the harness pin filled the running field: {out}"
    );
}

/// The +7d audit reports duty PER PIN, so the wrapper records which pin ran
/// from when. Each start opens a segment; the next start closes the previous
/// one at the last moment the daemon was seen alive — a BOUND, labelled as
/// one, because the wrapper `exec`s and nobody is standing beside the daemon
/// when it dies.
#[cfg(target_os = "macos")]
#[test]
fn each_start_opens_a_duty_segment_and_closes_the_previous_one() {
    let scratch = Scratch::new("pin-segments");
    scratch.ancient_pid_record("file");
    scratch.install_daemon("#!/bin/sh\nexit 0\n");
    scratch.build_record(
        &scratch.daemon_sha256(),
        "57360ccd3e6493cc2d20e8e6e480daaa88486817",
        "3a8e5c03fdbe2f21144faee8daba73beeb75d8b4",
    );
    let start = || {
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
    };
    start();
    let duty = scratch.root.join("logs/pin-duty.log");
    let first = std::fs::read_to_string(&duty).expect("pin-duty.log");
    assert_eq!(
        first
            .lines()
            .filter(|l| l.contains("segment-opened"))
            .count(),
        1,
        "the first start opens exactly one segment: {first}"
    );
    assert!(
        first.contains("pin=57360ccd3e6493cc2d20e8e6e480daaa88486817"),
        "{first}"
    );
    assert!(
        !first.contains("segment-closed"),
        "nothing was open to close: {first}"
    );

    // A pin bump: a different binary, a record that describes it.
    scratch.install_daemon("#!/bin/sh\n# bumped\nexit 0\n");
    scratch.build_record(
        &scratch.daemon_sha256(),
        "3a8e5c03fdbe2f21144faee8daba73beeb75d8b4",
        "3a8e5c03fdbe2f21144faee8daba73beeb75d8b4",
    );
    start();
    let after = std::fs::read_to_string(&duty).expect("pin-duty.log");
    let closed = after
        .lines()
        .find(|l| l.contains("segment-closed"))
        .unwrap_or_else(|| panic!("the previous segment was never closed: {after}"));
    assert!(
        closed.contains("pin=57360ccd3e6493cc2d20e8e6e480daaa88486817"),
        "the closed segment names the pin that ran, not the new one: {closed}"
    );
    assert!(
        closed.contains("to_bound=2026-08-29T14:18:19.162106Z"),
        "the end is the last-seen bound: {closed}"
    );
    assert!(
        closed.contains("bound=last-log-line"),
        "an end nobody observed is labelled a bound: {closed}"
    );
    assert_eq!(
        after
            .lines()
            .filter(|l| l.contains("segment-opened"))
            .count(),
        2,
        "the bump opened the new pin's segment: {after}"
    );
    assert!(
        after.contains("pin=3a8e5c03fdbe2f21144faee8daba73beeb75d8b4"),
        "{after}"
    );
}

/// `record-build.sh` writes every field by DERIVATION — the digest it
/// computes itself, the running pin from the composition harness's `.commit`
/// marker, the harness pin from `KERNEL-PIN.md`. Nothing is a parameter a
/// person types, which is why the record cannot go stale silently.
#[cfg(target_os = "macos")]
#[test]
fn record_build_derives_every_field_it_writes() {
    let scratch = Scratch::new("record-build");
    scratch.install_daemon("#!/bin/sh\nexit 0\n");
    // The layout the composition harness really produces: the archive is
    // extracted, the daemon is built inside its own target dir, and the
    // `.commit` marker sits at the archive root.
    let built = scratch.root.join("built");
    std::fs::create_dir_all(built.join("target/debug")).expect("built");
    std::fs::write(built.join("target/debug/jinnd"), STUB_DAEMON).expect("built daemon");
    std::fs::write(
        built.join(".commit"),
        "3a8e5c03fdbe2f21144faee8daba73beeb75d8b4\n",
    )
    .expect("marker");
    let repo = scratch.root.join("repo");
    std::fs::create_dir_all(&repo).expect("repo");
    std::fs::write(
        repo.join("KERNEL-PIN.md"),
        "```\nrepo: https://example.invalid/jinnd\n\
         commit: 3a8e5c03fdbe2f21144faee8daba73beeb75d8b4\n```\n",
    )
    .expect("KERNEL-PIN.md");

    let run = |args: &[&std::ffi::OsStr]| -> std::process::Output {
        Command::new("/bin/sh")
            .arg(soak_dir().join("record-build.sh"))
            .args(args)
            .env("SOAK", &scratch.root)
            .output()
            .expect("/bin/sh")
    };
    let ok = run(&[built.as_os_str(), repo.as_os_str()]);
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    let record = std::fs::read_to_string(scratch.root.join("bin/jinnd.build")).expect("record");
    assert!(
        record.contains(&format!("binary-sha256={}", scratch.daemon_sha256())),
        "the digest is of the binary that was installed: {record}"
    );
    assert!(
        record.contains("running-pin=3a8e5c03fdbe2f21144faee8daba73beeb75d8b4"),
        "{record}"
    );
    assert!(
        record.contains("harness-pin=3a8e5c03fdbe2f21144faee8daba73beeb75d8b4"),
        "{record}"
    );

    // A bare directory holding the daemon is the second accepted shape.
    let flat = scratch.root.join("flat");
    std::fs::create_dir_all(&flat).expect("flat");
    std::fs::write(flat.join("jinnd"), STUB_DAEMON).expect("flat daemon");
    std::fs::write(
        flat.join(".commit"),
        "3a8e5c03fdbe2f21144faee8daba73beeb75d8b4\n",
    )
    .expect("flat marker");
    let flat_ok = run(&[flat.as_os_str(), repo.as_os_str()]);
    assert!(
        flat_ok.status.success(),
        "{}",
        String::from_utf8_lossy(&flat_ok.stderr)
    );

    // A missing marker is a refusal, never a record with a hole in it: half a
    // record is what an auditor cannot tell from a whole one.
    std::fs::remove_file(built.join(".commit")).expect("rm marker");
    let refused = run(&[built.as_os_str(), repo.as_os_str()]);
    assert!(
        !refused.status.success(),
        "a derivation that could not read its source still wrote a record"
    );
}

// A SHAPE IS A READING, AND SO IS EXISTENCE (PLA-297 round 2, both Majors).
//
// The round-1 wrapper bound the pin to the binary's digest and called that a
// derivation. It was not one yet. Every hex reading went through one
// predicate — "a nonempty run of lowercase hex" — under which `a` is a
// commit, `a` is a sha256, and a record saying `running-pin=a harness-pin=b`
// is a licensed account of a 64-megabyte daemon. The verifier produced
// exactly that at rc 0. A validator that accepts `a` proves nothing about the
// artifact it claims to describe, so deriving from it is transcription with
// extra steps: this packet's own defect, one level in.
//
// Length is therefore part of the shape. A commit is exactly 40 lowercase hex
// characters and a digest exactly 64; anything else is `unknown` with the
// attempted reading named, never the malformed value dressed up as one.
//
// And "well-formed" is not "is a real commit". They are two readings, so they
// get two fields, for the same reason `running_pin` and `harness_pin` do: a
// field that can hold either cannot say which was performed. Where a kernel
// checkout is reachable the wrapper asks the repo and says so; where it is
// not — under launchd, which is every real start — it says `well-formed` and
// claims no more than it looked at.

/// Jinn Taste §1, enforced instead of reviewed. Four of these comments were
/// deleted from `soak_supervisor.rs` in the previous soak packet's round 6 and
/// four more arrived in these scripts one packet later, so the class is now a
/// test rather than something a reviewer has to catch a third time.
#[test]
fn no_soak_shell_asset_carries_a_section_divider_comment() {
    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(soak_dir()).expect("tools/soak") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("sh") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read shell asset");
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_owned();
        for (n, line) in text.lines().enumerate() {
            let Some(comment) = line.trim_start().strip_prefix('#') else {
                continue;
            };
            if comment.trim_start().starts_with("---") {
                offenders.push(format!("{name}:{}: {line}", n + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "Jinn Taste §1 — no section-divider ASCII. Say it in prose or start a \
         new file:\n  {}",
        offenders.join("\n  ")
    );
}

#[cfg(target_os = "macos")]
impl Scratch {
    /// Run `record-build.sh` over a build whose `.commit` marker and whose
    /// harness `KERNEL-PIN.md` say exactly what the caller asks. Both inputs
    /// are the ones the install path really reads.
    fn record_build(&self, marker: &str, kernel_pin_commit: &str) -> std::process::Output {
        let built = self.root.join("built");
        let _ = std::fs::remove_dir_all(&built);
        std::fs::create_dir_all(built.join("target/debug")).expect("built");
        std::fs::write(built.join("target/debug/jinnd"), STUB_DAEMON).expect("built daemon");
        std::fs::write(built.join(".commit"), format!("{marker}\n")).expect("marker");
        let repo = self.root.join("repo");
        std::fs::create_dir_all(&repo).expect("repo");
        std::fs::write(
            repo.join("KERNEL-PIN.md"),
            format!("```\nrepo: https://example.invalid/jinnd\ncommit: {kernel_pin_commit}\n```\n"),
        )
        .expect("KERNEL-PIN.md");
        Command::new("/bin/sh")
            .arg(soak_dir().join("record-build.sh"))
            .arg(&built)
            .arg(&repo)
            .env("SOAK", &self.root)
            .output()
            .expect("/bin/sh")
    }

    /// A real git repository holding one real commit, standing in for the
    /// kernel checkout the pin is looked up in. Returns its path and the sha
    /// it actually holds, so the resolving case is proven against an object
    /// that exists rather than against a mock that always says yes.
    fn kernel_repo(&self) -> (PathBuf, String) {
        let dir = self.root.join("kernel-repo");
        std::fs::create_dir_all(&dir).expect("kernel repo");
        let git = |args: &[&str]| -> std::process::Output {
            let out = Command::new("git")
                .args(args)
                .current_dir(&dir)
                .output()
                .expect("git");
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            out
        };
        git(&["init", "-q"]);
        git(&[
            "-c",
            "user.email=soak@example.invalid",
            "-c",
            "user.name=soak",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "the commit the pin names",
        ]);
        let sha = String::from_utf8_lossy(&git(&["rev-parse", "HEAD"]).stdout)
            .trim()
            .to_owned();
        (dir, sha)
    }

    /// The dry run with a kernel checkout reachable, which is the only
    /// condition under which existence is answerable at all.
    fn dry_run_against(&self, kernel_repo: &Path) -> String {
        let output = Command::new("/bin/sh")
            .arg(soak_dir().join("soak-run.sh"))
            .env("SOAK", &self.root)
            .env("SOAK_DRY_RUN", "1")
            .env("PATH", &self.path)
            .env("JINND_DIR", kernel_repo)
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

/// THE VERIFIER'S REPRODUCTION, verbatim: a `.commit` marker of `a` and a
/// `KERNEL-PIN.md` commit of `b`. Round 1 wrote the record at rc 0 and the
/// wrapper reported `running_pin=a harness_pin=b`. Neither is a commit.
#[cfg(target_os = "macos")]
#[test]
fn a_one_character_hex_string_is_never_a_pin() {
    let scratch = Scratch::new("pin-one-char");

    let refused = scratch.record_build("a", "b");
    assert!(
        !refused.status.success(),
        "record-build wrote a record from a one-character marker: {}",
        String::from_utf8_lossy(&refused.stdout)
    );
    let complaint = String::from_utf8_lossy(&refused.stderr);
    assert!(
        complaint.contains("40"),
        "the refusal must name the shape it wanted: {complaint}"
    );
    assert!(
        !scratch.root.join("bin/jinnd.build").exists()
            || !std::fs::read_to_string(scratch.root.join("bin/jinnd.build"))
                .expect("record")
                .contains("running-pin=a"),
        "the malformed marker reached the record anyway"
    );

    // And the wrapper, handed such a record with an otherwise perfect digest,
    // reports no pin rather than a one-character one.
    scratch.ancient_pid_record("file");
    scratch.build_record(&scratch.daemon_sha256(), "a", "b");
    let out = scratch.dry_run();
    assert!(
        out.contains("running_pin=unknown") && out.contains("harness_pin=unknown"),
        "a one-character hex string was reported as a pin: {out}"
    );
    assert!(
        !out.contains("running_pin=a ") && !out.contains("harness_pin=b "),
        "the malformed value was dressed up as a reading: {out}"
    );
    assert!(
        out.contains("build-record-unreadable"),
        "what could not be read must be named: {out}"
    );
}

/// The digest is the same class of reading and gets the same treatment: a
/// one-character `binary-sha256` is not a digest, and the join it would
/// otherwise license is between two strings that describe nothing.
#[cfg(target_os = "macos")]
#[test]
fn a_one_character_hex_string_is_never_a_digest() {
    let scratch = Scratch::new("digest-one-char");
    scratch.ancient_pid_record("file");
    scratch.build_record("a", RUNNING_PIN, HARNESS_PIN);
    let out = scratch.dry_run();
    assert!(
        out.contains("running_pin=unknown"),
        "a record whose digest is not a digest still licensed a pin: {out}"
    );
    assert!(
        out.contains("build-record-unreadable"),
        "the unreadable record must be named: {out}"
    );
}

/// Shape and existence are two readings and never share a field. With no
/// kernel checkout reachable — which is every launchd start — the wrapper
/// says which check it actually performed.
#[cfg(target_os = "macos")]
#[test]
fn an_unreachable_kernel_repo_yields_a_well_formed_reading_and_says_so() {
    let scratch = Scratch::new("pin-shape-only");
    scratch.ancient_pid_record("file");
    let out = scratch.dry_run();
    assert!(out.contains(&format!("running_pin={RUNNING_PIN}")), "{out}");
    assert!(
        out.contains("running_pin_checked=well-formed"),
        "the wrapper must say it checked the shape and nothing more: {out}"
    );
    assert!(
        out.contains("harness_pin_checked=well-formed"),
        "the harness pin is the same kind of reading: {out}"
    );
}

/// Where a checkout IS reachable, the stronger reading is taken and named.
#[cfg(target_os = "macos")]
#[test]
fn a_reachable_kernel_repo_proves_the_pin_resolves() {
    let scratch = Scratch::new("pin-resolves");
    scratch.ancient_pid_record("file");
    let (repo, sha) = scratch.kernel_repo();
    scratch.build_record(&scratch.daemon_sha256(), &sha, &sha);
    let out = scratch.dry_run_against(&repo);
    assert!(out.contains(&format!("running_pin={sha}")), "{out}");
    assert!(
        out.contains("running_pin_checked=resolves-in-kernel-repo"),
        "a pin the repo really holds must be reported as resolved: {out}"
    );
    assert!(
        out.contains("unproven=none"),
        "everything here was readable: {out}"
    );
}

/// A well-formed sha the kernel repo does not hold is not a commit, and the
/// difference is only visible because the two readings are separate.
#[cfg(target_os = "macos")]
#[test]
fn a_well_formed_pin_the_kernel_repo_does_not_hold_is_not_a_commit() {
    let scratch = Scratch::new("pin-absent");
    scratch.ancient_pid_record("file");
    let (repo, _real) = scratch.kernel_repo();
    let plausible = "0123456789abcdef0123456789abcdef01234567";
    scratch.build_record(&scratch.daemon_sha256(), plausible, plausible);
    let out = scratch.dry_run_against(&repo);
    assert!(
        !out.contains(&format!("running_pin={plausible}")),
        "a sha nothing holds was reported as the running pin: {out}"
    );
    assert!(out.contains("running_pin=unknown"), "{out}");
    assert!(
        out.contains("running_pin_checked=absent-from-kernel-repo"),
        "the reading that failed must be the one reported: {out}"
    );
    assert!(
        out.contains("pin-absent-from-kernel-repo"),
        "an unproven pin must be named among the unproven inputs: {out}"
    );
}

/// COULD NOT ASK is not an answer of no. A `JINND_DIR` that holds a `.git`
/// entry git cannot read makes no statement about the pin, so the pin keeps
/// the reading it did earn and the missing one is named. Sinking a good pin
/// on a reading nobody took would be this packet's own defect wearing the
/// fix's clothes.
#[cfg(target_os = "macos")]
#[test]
fn an_unreadable_kernel_repo_answers_nothing_rather_than_no() {
    let scratch = Scratch::new("pin-repo-unreadable");
    scratch.ancient_pid_record("file");
    // A `.git` that is a directory and not a repository: `-d` succeeds, git
    // refuses, and nothing about the pin has been established either way.
    let repo = scratch.root.join("not-a-repo");
    std::fs::create_dir_all(repo.join(".git")).expect("decoy");
    let out = scratch.dry_run_against(&repo);
    assert!(
        out.contains(&format!("running_pin={RUNNING_PIN}")),
        "a pin was sunk by a repo that never answered: {out}"
    );
    assert!(
        out.contains("running_pin_checked=well-formed-kernel-repo-unreadable"),
        "the reading that could not be taken must be named as untaken: {out}"
    );
    assert!(
        !out.contains("absent-from-kernel-repo"),
        "a repo that could not be read was made to say no: {out}"
    );
}

/// The launcher's half of the door (packet 2.8): the wrapper provisions
/// the `jinn:auth` credential of record before it execs the daemon, and
/// the installer ships the script that does it.
#[test]
fn the_wrapper_provisions_the_operator_token_before_the_daemon() {
    let wrapper = read("soak-run.sh");
    let provision = wrapper
        .find("provision-token.sh")
        .expect("the wrapper calls the provisioning script");
    let exec = wrapper
        .rfind("exec \"$SOAK/bin/jinnd\"")
        .expect("the wrapper execs the daemon");
    assert!(
        provision < exec,
        "the credential is provisioned BEFORE the daemon starts"
    );
    assert!(
        read("install-launchd.sh").contains("provision-token.sh"),
        "the installer ships the provisioning script beside the wrapper"
    );
}

/// Drives the provisioning script over a scratch root: the file appears
/// beside the data root as `data.operator-token`, holds 64 hex characters
/// (32 random bytes), is mode 0600, is NEVER printed, and a second run
/// keeps it byte for byte; a file with a wide mode is refused rather
/// than kept.
#[test]
fn provisioning_writes_a_private_random_token_once_and_refuses_a_wide_mode() {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!(
        "jinn-harness-provision-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("after the epoch")
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("scratch root");
    let run = || {
        Command::new("/bin/sh")
            .arg(soak_dir().join("provision-token.sh"))
            .env("SOAK", &root)
            .output()
            .expect("/bin/sh")
    };
    let first = run();
    assert!(first.status.success(), "{first:?}");
    let token = root.join("data.operator-token");
    assert!(token.is_file(), "the file is beside the data root");
    let value = std::fs::read_to_string(&token).expect("readable by the owner");
    let value = value.trim();
    assert_eq!(value.len(), 64, "32 random bytes, hex-encoded: {value:?}");
    assert!(value.chars().all(|c| c.is_ascii_hexdigit()), "{value:?}");
    let mode = std::fs::metadata(&token)
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600, "mode {mode:o}");
    let stdout = String::from_utf8_lossy(&first.stdout);
    assert!(
        !stdout.contains(value) && !String::from_utf8_lossy(&first.stderr).contains(value),
        "the value is never printed: {stdout}"
    );
    assert!(stdout.contains("provisioned"), "{stdout}");
    // A second run is a restart: the credential is kept as it is.
    let second = run();
    assert!(second.status.success(), "{second:?}");
    assert_eq!(
        std::fs::read_to_string(&token).expect("readable").trim(),
        value,
        "a restart keeps its credential"
    );
    assert!(
        String::from_utf8_lossy(&second.stdout).contains("kept"),
        "{second:?}"
    );
    // A wide mode is what the kernel refuses; the script says so at the
    // start instead of leaving a ledger of refusals to explain it.
    std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o644)).expect("chmod");
    let wide = run();
    assert!(
        !wide.status.success(),
        "a group-readable credential is refused"
    );
    assert!(
        String::from_utf8_lossy(&wide.stderr).contains("group- or world-accessible"),
        "{wide:?}"
    );
    let _ = std::fs::remove_dir_all(&root);
}
