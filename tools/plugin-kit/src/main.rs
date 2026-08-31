//! The plugins seam's kit builder (see Cargo.toml for usage): the two
//! catalog providers mounted beside the api trio, the settings pair and
//! the cron seam, pinned by content hash. The entries themselves live in
//! this crate's library (one home per fact).

use std::path::{Path, PathBuf};

use api_kit::{api_entries, settings_entries, PROVIDER_ID};
use cron_kit::{build, cron_entries, flag, write_profile};
use plugin_kit::{
    api_catalog_grants, fixed_entry, live_entry, misbound_entry, FIXED_ID, MAIN_CATALOG,
    PARKED_CATALOG, SHELVED_ID,
};

fn kit(root: &Path, port: u16, every_ms: u64, tick_ms: u64) {
    let artifacts = root.join("artifacts");
    let scheduler = build(&artifacts, "cron", "cron-scheduler");
    let snapshot = build(&artifacts, "cron", "health-snapshot");
    let http = build(&artifacts, "api", "jinn-api-http");
    let status = build(&artifacts, "api", "jinn-status");
    let edit = build(&artifacts, "api", "jinn-profile-edit");
    let settings = build(&artifacts, "settings", "jinn-settings-profile");
    let store = build(&artifacts, "settings", "jinn-settings-store");
    let live = build(&artifacts, "plugins", "jinn-plugins-profile");
    let fixed = build(&artifacts, "plugins", "jinn-plugins-static");

    let mut entries = cron_entries(&scheduler, &snapshot, every_ms, tick_ms);
    entries.extend(api_entries(&http, &status, &edit, port));
    entries.extend(settings_entries(&settings, &store, &["cron-scheduler"]));
    // The live catalog takes the switchable name; the fixed one waits on
    // the parked one. A swap moves the name between them, and the order
    // it happens in is the library's law.
    entries.push(live_entry(&live, MAIN_CATALOG));
    entries.push(fixed_entry(FIXED_ID, &fixed, PARKED_CATALOG, false));
    // A disabled entry and a failing one, mounted on purpose: a seam that
    // claims to report these honestly has to have them to report.
    entries.push(fixed_entry(SHELVED_ID, &fixed, "shelf", true));
    entries.push(misbound_entry(
        &http,
        port.wrapping_add(1),
        port.wrapping_add(2),
    ));

    let catalogs = [MAIN_CATALOG, PARKED_CATALOG];
    for entry in &mut entries {
        if entry["id"] == PROVIDER_ID {
            let grants = entry["config"]["grants"].as_array_mut().expect("grants");
            grants.extend(api_catalog_grants(&catalogs));
            entry["config"]["data"]["catalogs"] = serde_json::json!(catalogs);
        }
    }
    write_profile(root, entries);
    println!("catalogs mounted: {}", catalogs.join(", "));
}

fn usage() -> ! {
    eprintln!("usage: plugin-kit kit <root> --port N [--every-ms N] [--tick-ms N]");
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
