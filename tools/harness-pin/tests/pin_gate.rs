//! The kernel pin gate (`cargo test -p harness-pin`).
//!
//! Gate 1 (always on, fail-closed): the vendored contract surface in
//! `kernel-pin/` must hash to exactly what `KERNEL-PIN.md` records. Editing
//! either alone fails CI.
//!
//! Gate 2 (self-skipping, per the distribution playbook's real-API pattern):
//! when a jinnd checkout is reachable — `JINND_DIR`, the default sibling
//! `../jinnd`, or a clone from `JINND_CLONE_URL` — the pinned commit's
//! `wit/` and `contracts/` trees must hash to the pinned values. Skipped
//! loudly when the kernel repo is unreachable (it is private; CI provides
//! `JINND_CLONE_URL` via a read token when configured).

use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn pin() -> harness_pin::KernelPin {
    let text = std::fs::read_to_string(repo_root().join("KERNEL-PIN.md")).expect("KERNEL-PIN.md");
    harness_pin::parse_pin(&text).expect("KERNEL-PIN.md must carry all pin fields")
}

#[test]
fn vendored_surface_matches_pin() {
    let pin = pin();
    let root = repo_root();
    assert_eq!(
        harness_pin::contract_hash(&root.join("kernel-pin/wit")).expect("hash kernel-pin/wit"),
        pin.wit_hash,
        "kernel-pin/wit diverges from KERNEL-PIN.md wit-hash — bump the pin properly (one commit, hashes + surface together)"
    );
    assert_eq!(
        harness_pin::contract_hash(&root.join("kernel-pin/contracts"))
            .expect("hash kernel-pin/contracts"),
        pin.contracts_hash,
        "kernel-pin/contracts diverges from KERNEL-PIN.md contracts-hash — bump the pin properly (one commit, hashes + surface together)"
    );
}

#[test]
fn pinned_kernel_checkout_matches_pin() {
    let pin = pin();
    let Some(jinnd) = locate_jinnd(&pin) else {
        eprintln!(
            "SKIP pinned_kernel_checkout_matches_pin: no jinnd checkout reachable \
             (set JINND_DIR or JINND_CLONE_URL); the fail-closed vendored-surface gate still ran"
        );
        return;
    };
    assert_eq!(
        harness_pin::contract_hash_of_git_tree(&jinnd, &pin.commit, "wit")
            .expect("hash wit/ at pinned commit"),
        pin.wit_hash,
        "jinnd wit/ at the pinned commit diverges from KERNEL-PIN.md"
    );
    assert_eq!(
        harness_pin::contract_hash_of_git_tree(&jinnd, &pin.commit, "contracts")
            .expect("hash contracts/ at pinned commit"),
        pin.contracts_hash,
        "jinnd contracts/ at the pinned commit diverges from KERNEL-PIN.md"
    );
}

/// Find a usable jinnd repo: env override, sibling checkout, or a fresh clone.
/// Returns None (→ loud skip) when none is available.
fn locate_jinnd(pin: &harness_pin::KernelPin) -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("JINND_DIR") {
        let dir = PathBuf::from(dir);
        assert!(dir.join(".git").exists(), "JINND_DIR is not a git checkout");
        return Some(dir);
    }
    let sibling = repo_root().join("../jinnd");
    if sibling.join(".git").exists() {
        return Some(sibling);
    }
    if let Ok(url) = std::env::var("JINND_CLONE_URL") {
        let dest = std::env::temp_dir().join(format!("jinnd-pin-gate-{}", std::process::id()));
        let out = std::process::Command::new("git")
            .args(["clone", "--quiet", "--no-checkout", &url])
            .arg(&dest)
            .output()
            .expect("run git clone");
        assert!(
            out.status.success(),
            "JINND_CLONE_URL was set but the clone failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let _ = pin; // commit existence is checked by the tree hash itself
        return Some(dest);
    }
    None
}
