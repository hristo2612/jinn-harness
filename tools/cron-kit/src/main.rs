//! The cron seam's kit builder (see Cargo.toml for usage). The machinery
//! and the cron entries live in this crate's library, shared with the
//! other seam kits.

use std::path::{Path, PathBuf};

use cron_kit::{build, cron_entries, flag, write_profile};

fn kit(root: &Path, every_ms: u64, tick_ms: u64) {
    let artifacts = root.join("artifacts");
    let scheduler = build(&artifacts, "cron", "cron-scheduler");
    let snapshot = build(&artifacts, "cron", "health-snapshot");
    write_profile(root, cron_entries(&scheduler, &snapshot, every_ms, tick_ms));
}

fn usage() -> ! {
    eprintln!("usage: cron-kit kit <root> [--every-ms N] [--tick-ms N]");
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("kit") => {
            let root = args.get(1).map(PathBuf::from).unwrap_or_else(|| usage());
            kit(
                &root,
                flag(&args, "--every-ms", usage).unwrap_or(900_000),
                flag(&args, "--tick-ms", usage).unwrap_or(jinn_cron::DEFAULT_TICK_MS),
            );
        }
        _ => usage(),
    }
}
