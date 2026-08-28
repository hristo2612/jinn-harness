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
