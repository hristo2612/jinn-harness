//! The operator-API seam's kit builder (see Cargo.toml for usage): the
//! api trio mounted beside the cron seam, pinned by content hash. The
//! entries themselves live in this crate's library (one home per fact).

use std::path::{Path, PathBuf};

use api_kit::{api_entries, settings_entries};
use cron_kit::{build, cron_entries, flag, write_profile};

fn kit(root: &Path, port: u16, every_ms: u64, tick_ms: u64) {
    let artifacts = root.join("artifacts");
    let scheduler = build(&artifacts, "cron", "cron-scheduler");
    let snapshot = build(&artifacts, "cron", "health-snapshot");
    let http = build(&artifacts, "api", "jinn-api-http");
    let status = build(&artifacts, "api", "jinn-status");
    let edit = build(&artifacts, "api", "jinn-profile-edit");
    let settings = build(&artifacts, "settings", "jinn-settings-profile");
    let store = build(&artifacts, "settings", "jinn-settings-store");
    let mut entries = cron_entries(&scheduler, &snapshot, every_ms, tick_ms);
    entries.extend(api_entries(&http, &status, &edit, port));
    entries.extend(settings_entries(&settings, &store, &["cron-scheduler"]));
    write_profile(root, entries);
}

fn usage() -> ! {
    eprintln!("usage: api-kit kit <root> --port N [--every-ms N] [--tick-ms N]");
    std::process::exit(2);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("kit") => {
            let root = args.get(1).map(PathBuf::from).unwrap_or_else(|| usage());
            let port = flag(&args, "--port", usage)
                .and_then(|port| u16::try_from(port).ok())
                .unwrap_or_else(|| usage());
            kit(
                &root,
                port,
                flag(&args, "--every-ms", usage).unwrap_or(900_000),
                flag(&args, "--tick-ms", usage).unwrap_or(jinn_cron::DEFAULT_TICK_MS),
            );
        }
        _ => usage(),
    }
}
