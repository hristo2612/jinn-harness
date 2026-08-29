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
    assert!(out.contains("reason=keepalive-restart"), "{out}");
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
    assert!(dry_run(&root).contains("reason=boot"));

    // An operator's reason file wins, and dry-run does not consume it.
    std::fs::write(root.join("run/launchd.reason"), "planned-start").expect("reason");
    assert!(dry_run(&root).contains("reason=planned-start"));
    assert!(
        root.join("run/launchd.reason").is_file(),
        "dry-run consumed the reason"
    );
    let _ = std::fs::remove_dir_all(&root);
}
