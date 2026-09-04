//! Kit-building machinery shared by the seam kit builders (`cron-kit`,
//! `api-kit`): compile one guest crate for wasm32-unknown-unknown, encode
//! the core module to a component in process, pin it by content hash
//! (kernel Law 5), write the artifact + sidecar. The build pattern is the
//! kernel demo builder's. The cron seam's profile ENTRIES also live here
//! (one home per fact): every profile that mounts the cron seam mounts
//! these exact entries.

use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

/// The repo root (this crate lives at `tools/cron-kit`).
#[must_use]
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A guest crate's directory: `plugins/<seam>/<name>`.
#[must_use]
pub fn plugin_dir(seam: &str, name: &str) -> PathBuf {
    repo_root().join(format!("plugins/{seam}/{name}"))
}

/// The build must use a rustc whose wasm32-unknown-unknown std is
/// installed; fall back to the rustup toolchain's own binaries when PATH
/// shadows it (the kernel demo builder's discipline).
fn candidates() -> Vec<(PathBuf, Option<PathBuf>)> {
    let mut found = vec![(PathBuf::from("cargo"), None)];
    if let Ok(output) = Command::new("rustup").args(["which", "rustc"]).output() {
        if output.status.success() {
            if let Ok(path) = String::from_utf8(output.stdout) {
                let rustc = PathBuf::from(path.trim());
                let cargo = rustc.with_file_name("cargo");
                if cargo.exists() {
                    found.push((cargo, Some(rustc)));
                }
            }
        }
    }
    found
}

fn build_core(dir: &Path, name: &str) -> Vec<u8> {
    let module = format!(
        "target/wasm32-unknown-unknown/release/{}.wasm",
        name.replace('-', "_")
    );
    let artifact = dir.join(module);
    let mut failures = Vec::new();
    for (cargo, rustc) in candidates() {
        let mut command = Command::new(&cargo);
        command
            .args(["build", "--release", "--target", "wasm32-unknown-unknown"])
            .current_dir(dir)
            // The workspace's flags and target dir must not leak into the
            // guest build (guests are not workspace members by design).
            .env_remove("RUSTFLAGS")
            .env_remove("CARGO_TARGET_DIR");
        if let Some(rustc) = rustc {
            command.env("RUSTC", rustc);
        }
        match command.output() {
            Ok(output) if output.status.success() => {
                return std::fs::read(&artifact).unwrap_or_else(|error| {
                    panic!("built but {} is unreadable: {error}", artifact.display())
                });
            }
            Ok(output) => failures.push(format!(
                "{}: {}",
                cargo.display(),
                String::from_utf8_lossy(&output.stderr)
            )),
            Err(error) => failures.push(format!("{}: {error}", cargo.display())),
        }
    }
    panic!(
        "no toolchain could build {name}:\n{}",
        failures.join("\n---\n")
    );
}

/// Builds guest `name` of `seam` to a validated component; answers the
/// bytes and their content hash (the profile's pin).
#[must_use]
pub fn component(seam: &str, name: &str) -> (Vec<u8>, String) {
    let core = build_core(&plugin_dir(seam, name), name);
    let bytes = wit_component::ComponentEncoder::default()
        .module(&core)
        .unwrap_or_else(|error| panic!("core module rejected: {error:#}"))
        .validate(true)
        .encode()
        .unwrap_or_else(|error| panic!("component encoding failed: {error:#}"));
    let hash = format!("{:x}", Sha256::digest(&bytes));
    (bytes, hash)
}

/// Writes one artifact and its `.sha256` sidecar under `dir`.
pub fn write_artifact(dir: &Path, name: &str, bytes: &[u8], hash: &str) {
    std::fs::create_dir_all(dir).expect("artifacts dir");
    let file = dir.join(format!("{name}.wasm"));
    std::fs::write(&file, bytes).expect("artifact write");
    std::fs::write(dir.join(format!("{name}.wasm.sha256")), hash).expect("sidecar write");
    println!("{} {}", hash, file.display());
}

/// Builds and writes one guest; answers its pin.
#[must_use]
pub fn build(artifacts: &Path, seam: &str, name: &str) -> String {
    let (bytes, hash) = component(seam, name);
    write_artifact(artifacts, name, &bytes, &hash);
    hash
}

/// The cron seam's two profile entries with their grants. The listen
/// grant (the job topic) and contract grants (`jinn:cron`, `jinn:fs`,
/// `jinn:clock` — a bare clock grant holds the kernel's default 250 ms
/// resolution floor) are the profile side's authority decisions —
/// requests are not grants.
#[must_use]
pub fn cron_entries(
    scheduler: &str,
    snapshot: &str,
    every_ms: u64,
    tick_ms: u64,
) -> Vec<serde_json::Value> {
    vec![
        // `jinn:settings` and the changed-topic listen: the scheduler
        // consumes its job table through the settings seam where one is
        // mounted (the operator profile); in the cron-only profile the
        // resolve answers missing-dependency and the entry layer is the
        // whole truth. `entry-id` names this entry to the seam.
        serde_json::json!({ "id": "cron-scheduler", "package": "cron/cron-scheduler", "hash": scheduler,
          "config": { "grants": [jinn_cron::CRON_CONTRACT, "jinn:fs", jinn_cron::CLOCK_CONTRACT,
                                 jinn_settings::SETTINGS_CONTRACT, jinn_settings::CHANGED_TOPIC],
                      "data": { "entry-id": "cron-scheduler", "tick-ms": tick_ms, "jobs": [
                          { "id": "health", "every-ms": every_ms, "topic": "cron:health" }
                      ] } } }),
        serde_json::json!({ "id": "health-snapshot", "package": "cron/health-snapshot", "hash": snapshot,
          "config": { "grants": ["cron:health", jinn_cron::CRON_CONTRACT, "jinn:fs"],
                      "data": { "topic": "cron:health", "dir": "health", "nonce": 0 } } }),
    ]
}

/// Writes `<root>/profile.json` from its entries.
pub fn write_profile(root: &Path, entries: Vec<serde_json::Value>) {
    std::fs::create_dir_all(root).expect("kit root");
    let document = serde_json::json!({ "entries": entries });
    std::fs::write(
        root.join("profile.json"),
        serde_json::to_string_pretty(&document).expect("profile encoding"),
    )
    .expect("profile write");
    println!("profile {}", root.join("profile.json").display());
}

/// A `--name N` flag's value; a malformed one calls `usage`.
#[must_use]
pub fn flag(args: &[String], name: &str, usage: fn() -> !) -> Option<u64> {
    let position = args.iter().position(|arg| arg == name)?;
    let value = args.get(position + 1).unwrap_or_else(|| usage());
    Some(value.parse().unwrap_or_else(|_| usage()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The scheduler EMITS on every job's topic (the job table is operator
    /// data, `cron:<job-id>` by default); at pin `138fdce` (jinnd M2-K26
    /// (e); FINDINGS #49) an emit is covered by the topic's own grant, so
    /// the scheduler entry carries every topic its table fires — derived
    /// from the table, never a second list to drift.
    #[test]
    fn the_scheduler_is_granted_every_job_topic_it_fires() {
        let entries = cron_entries("abc", "def", 60_000, 1_000);
        let scheduler = &entries[0];
        assert_eq!(scheduler["id"], "cron-scheduler");
        let grants = scheduler["config"]["grants"].as_array().expect("grants");
        let topics: Vec<&str> = scheduler["config"]["data"]["jobs"]
            .as_array()
            .expect("jobs")
            .iter()
            .map(|job| job["topic"].as_str().expect("a topic"))
            .collect();
        assert!(!topics.is_empty(), "the shipped table has a job");
        for topic in topics {
            assert!(
                grants.contains(&serde_json::json!(topic)),
                "the scheduler is granted {topic}, the topic it fires: {grants:?}"
            );
        }
    }
}
