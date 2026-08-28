//! Locating a `jinnd` repo and building the PINNED daemon binary. The pin
//! (KERNEL-PIN.md) names the one kernel commit the harness runs on; the
//! composition gate boots exactly that daemon — never a working tree.
//!
//! Discovery mirrors the pin gate's Gate-2 lanes: `JINND_DIR`, a sibling
//! `../jinnd` checkout, or a fresh clone from `JINND_CLONE_URL`. When none
//! is reachable the gate self-skips LOUDLY (jinnd is private; CI without
//! the read token still holds every fail-closed gate).

use std::path::{Path, PathBuf};
use std::process::Command;

/// The workspace root (this crate lives at `tests/composition`).
#[must_use]
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("tests/composition sits two levels under the root")
        .to_path_buf()
}

/// The pinned kernel commit, from `KERNEL-PIN.md` (the one home of that
/// fact — parsed by the same code the pin gate trusts).
///
/// # Errors
///
/// The pin file is unreadable or malformed.
pub fn pinned_commit() -> Result<String, String> {
    let path = workspace_root().join("KERNEL-PIN.md");
    let text =
        std::fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    harness_pin::parse_pin(&text).map(|pin| pin.commit)
}

fn run(command: &mut Command) -> Result<Vec<u8>, String> {
    let rendered = format!("{command:?}");
    let output = command
        .output()
        .map_err(|error| format!("{rendered}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{rendered}: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output.stdout)
}

fn has_commit(repo: &Path, commit: &str) -> bool {
    Command::new("git")
        .args(["-C", &repo.display().to_string(), "cat-file", "-e"])
        .arg(format!("{commit}^{{commit}}"))
        .status()
        .is_ok_and(|status| status.success())
}

/// A jinnd repo holding the pinned commit: `JINND_DIR`, the sibling
/// checkout, or a cached clone from `JINND_CLONE_URL`. `None` = unreachable
/// (the caller skips loudly).
#[must_use]
pub fn jinnd_source(commit: &str) -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("JINND_DIR") {
        let dir = PathBuf::from(dir);
        if has_commit(&dir, commit) {
            return Some(dir);
        }
        eprintln!("JINND_DIR does not hold pinned commit {commit}");
    }
    let sibling = workspace_root().join("../jinnd");
    if sibling.is_dir() && has_commit(&sibling, commit) {
        return Some(sibling);
    }
    if let Ok(url) = std::env::var("JINND_CLONE_URL") {
        let clone = workspace_root().join("target/composition/jinnd-clone");
        if has_commit(&clone, commit) {
            return Some(clone);
        }
        let _ = std::fs::remove_dir_all(&clone);
        let parent = clone.parent().expect("cache parent");
        if let Err(error) = std::fs::create_dir_all(parent) {
            eprintln!("clone cache dir: {error}");
            return None;
        }
        match run(Command::new("git")
            .arg("clone")
            .arg(&url)
            .arg(&clone)
            .current_dir(parent))
        {
            Ok(_) if has_commit(&clone, commit) => return Some(clone),
            Ok(_) => eprintln!("clone of jinnd does not hold pinned commit {commit}"),
            Err(error) => eprintln!("clone of jinnd failed: {error}"),
        }
    }
    None
}

/// Builds the daemon binary from the PINNED commit's tree (via `git
/// archive` into a cache — no worktree metadata, no working-tree reads) and
/// returns the binary path. Cached per commit; a pin bump rebuilds.
///
/// # Errors
///
/// Git or cargo failures while materializing or building the pinned tree.
pub fn pinned_daemon(source: &Path, commit: &str) -> Result<PathBuf, String> {
    let cache = workspace_root().join("target/composition/pinned-jinnd");
    let marker = cache.join(".commit");
    let binary = cache.join("target/debug/jinnd");
    if binary.is_file() && std::fs::read_to_string(&marker).is_ok_and(|held| held.trim() == commit)
    {
        return Ok(binary);
    }
    let _ = std::fs::remove_dir_all(&cache);
    std::fs::create_dir_all(&cache).map_err(|error| format!("daemon cache: {error}"))?;
    let archive = run(Command::new("git")
        .args(["-C", &source.display().to_string(), "archive"])
        .arg(commit))?;
    let mut untar = Command::new("tar")
        .args(["-x", "-C", &cache.display().to_string()])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("tar: {error}"))?;
    use std::io::Write as _;
    untar
        .stdin
        .take()
        .expect("piped stdin")
        .write_all(&archive)
        .map_err(|error| format!("tar stdin: {error}"))?;
    let status = untar.wait().map_err(|error| format!("tar: {error}"))?;
    if !status.success() {
        return Err("tar of the pinned tree failed".into());
    }
    eprintln!("building the pinned jinnd daemon at {commit} (one-time per pin)…");
    run(Command::new("cargo")
        .args(["build", "-p", "jinnd-daemon"])
        .current_dir(&cache)
        .env("CARGO_TARGET_DIR", cache.join("target")))?;
    std::fs::write(&marker, commit).map_err(|error| format!("marker: {error}"))?;
    Ok(binary)
}
