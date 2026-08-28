//! The cron seam's kit builder (see Cargo.toml for usage).
//! The build pattern is the kernel demo builder's: compile each guest for
//! wasm32-unknown-unknown, encode the core module to a component in
//! process, pin it by content hash (kernel Law 5), and write the profile
//! with the honest pins.

use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

fn plugin_dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../plugins/cron/{name}"))
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

fn build_core(name: &str) -> Vec<u8> {
    let dir = plugin_dir(name);
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
            .current_dir(&dir)
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

fn component(name: &str) -> (Vec<u8>, String) {
    let core = build_core(name);
    let bytes = wit_component::ComponentEncoder::default()
        .module(&core)
        .unwrap_or_else(|error| panic!("core module rejected: {error:#}"))
        .validate(true)
        .encode()
        .unwrap_or_else(|error| panic!("component encoding failed: {error:#}"));
    let hash = format!("{:x}", Sha256::digest(&bytes));
    (bytes, hash)
}

fn write_artifact(dir: &Path, name: &str, bytes: &[u8], hash: &str) {
    std::fs::create_dir_all(dir).expect("artifacts dir");
    let file = dir.join(format!("{name}.wasm"));
    std::fs::write(&file, bytes).expect("artifact write");
    std::fs::write(dir.join(format!("{name}.wasm.sha256")), hash).expect("sidecar write");
    println!("{} {}", hash, file.display());
}

/// The cron profile: the seam's two guests with their grants. The listen
/// grant (the job topic) and contract grants (`jinn:cron`, `jinn:fs`,
/// `jinn:clock` — a bare clock grant holds the kernel's default 250 ms
/// resolution floor) are the profile side's authority decisions — requests
/// are not grants.
fn profile(scheduler: &str, snapshot: &str, every_ms: u64, tick_ms: u64) -> String {
    let document = serde_json::json!({ "entries": [
        { "id": "cron-scheduler", "package": "cron/cron-scheduler", "hash": scheduler,
          "config": { "grants": [jinn_cron::CRON_CONTRACT, "jinn:fs", jinn_cron::CLOCK_CONTRACT],
                      "data": { "tick-ms": tick_ms, "jobs": [
                          { "id": "health", "every-ms": every_ms, "topic": "cron:health" }
                      ] } } },
        { "id": "health-snapshot", "package": "cron/health-snapshot", "hash": snapshot,
          "config": { "grants": ["cron:health", jinn_cron::CRON_CONTRACT, "jinn:fs"],
                      "data": { "topic": "cron:health", "dir": "health", "nonce": 0 } } },
    ]});
    serde_json::to_string_pretty(&document).expect("profile encoding")
}

fn kit(root: &Path, every_ms: u64, tick_ms: u64) {
    let artifacts = root.join("artifacts");
    let (scheduler, scheduler_hash) = component("cron-scheduler");
    let (snapshot, snapshot_hash) = component("health-snapshot");
    write_artifact(&artifacts, "cron-scheduler", &scheduler, &scheduler_hash);
    write_artifact(&artifacts, "health-snapshot", &snapshot, &snapshot_hash);
    std::fs::create_dir_all(root).expect("kit root");
    std::fs::write(
        root.join("profile.json"),
        profile(&scheduler_hash, &snapshot_hash, every_ms, tick_ms),
    )
    .expect("profile write");
    println!("profile {}", root.join("profile.json").display());
}

fn usage() -> ! {
    eprintln!("usage: cron-kit kit <root> [--every-ms N] [--tick-ms N]");
    std::process::exit(2);
}

fn flag(args: &[String], name: &str) -> Option<u64> {
    let position = args.iter().position(|arg| arg == name)?;
    let value = args.get(position + 1).unwrap_or_else(|| usage());
    Some(value.parse().unwrap_or_else(|_| usage()))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("kit") => {
            let root = args.get(1).map(PathBuf::from).unwrap_or_else(|| usage());
            kit(
                &root,
                flag(&args, "--every-ms").unwrap_or(900_000),
                flag(&args, "--tick-ms").unwrap_or(jinn_cron::DEFAULT_TICK_MS),
            );
        }
        _ => usage(),
    }
}
