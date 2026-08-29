//! The operator-API seam's kit builder (see Cargo.toml for usage): the
//! api trio mounted beside the cron seam, pinned by content hash.

use std::path::{Path, PathBuf};

use cron_kit::{build, cron_entries, flag, write_profile};

/// The HTTP provider's id in the operator profile.
pub const PROVIDER_ID: &str = "jinn-api-http";
/// The settings provider's and overlay store's ids.
pub const SETTINGS_ID: &str = "jinn-settings-profile";
/// See [`SETTINGS_ID`].
pub const STORE_ID: &str = "jinn-settings-store";

/// The api trio's profile entries. Authority is the profile side's: the
/// provider's `jinn:net` grant is scoped to exactly its port (loopback is
/// the bundle's own v0.1 bound) and it holds NO clock (served from the
/// kernel's readiness wakes); the status consumer holds the read-only
/// kernel contracts (`jinn:introspect`, `jinn:ledger`), the editor holds
/// `jinn:profile` over every entry (the scope written out — a bare grant
/// patches nothing); both consumers' `jinn:fs` grants are scoped to the
/// profile document alone, and each is granted the contract it provides;
/// the status consumer probes `jinn:cron`.
#[must_use]
pub fn api_entries(http: &str, status: &str, edit: &str, port: u16) -> Vec<serde_json::Value> {
    let profile_scope = serde_json::json!({ "contract": "jinn:fs", "scope": "profile.json" });
    vec![
        serde_json::json!({ "id": PROVIDER_ID, "package": "api/jinn-api-http", "hash": http,
          "config": { "grants": [
                          { "contract": "jinn:net", "scope": { "bind": [port, port] } },
                          jinn_api::STATUS_CONTRACT, jinn_api::PROFILE_CONTRACT,
                          jinn_settings::SETTINGS_CONTRACT ],
                      "data": { "port": port } } }),
        serde_json::json!({ "id": "jinn-status", "package": "api/jinn-status", "hash": status,
          "config": { "grants": [jinn_api::STATUS_CONTRACT, profile_scope, jinn_cron::CRON_CONTRACT,
                                 jinn_api::INTROSPECT_CONTRACT, jinn_api::LEDGER_CONTRACT],
                      "data": { "profile-path": "profile.json",
                                "probes": [ { "contract": jinn_cron::CRON_CONTRACT, "operation": jinn_cron::OP_JOBS } ] } } }),
        serde_json::json!({ "id": "jinn-profile-edit", "package": "api/jinn-profile-edit", "hash": edit,
          "config": { "grants": [jinn_api::PROFILE_CONTRACT, profile_scope,
                                 { "contract": jinn_api::KERNEL_PROFILE_CONTRACT,
                                   "scope": [jinn_api::KERNEL_PROFILE_SCOPE_ALL] }],
                      "data": { "profile-path": "profile.json" } } }),
    ]
}

/// The settings seam's entries: the provider (granted `jinn:settings` to
/// provide, `jinn:settings-store` to read the overlay, and `jinn:profile`
/// scoped to exactly the entries it may patch — every namespace owner
/// and the store) and the store (granted only what it provides). The
/// store's `overlays` is the hot layer's home in the document; the kit
/// writes it empty.
#[must_use]
pub fn settings_entries(provider: &str, store: &str, owners: &[&str]) -> Vec<serde_json::Value> {
    let mut scope: Vec<&str> = owners.to_vec();
    scope.push(STORE_ID);
    vec![
        serde_json::json!({ "id": SETTINGS_ID, "package": "settings/jinn-settings-profile", "hash": provider,
          "config": { "grants": [jinn_settings::SETTINGS_CONTRACT, jinn_settings::STORE_CONTRACT,
                                 { "contract": jinn_api::KERNEL_PROFILE_CONTRACT, "scope": scope }],
                      "data": { "store": STORE_ID } } }),
        serde_json::json!({ "id": STORE_ID, "package": "settings/jinn-settings-store", "hash": store,
          "config": { "grants": [jinn_settings::STORE_CONTRACT],
                      "data": { "overlays": {} } } }),
    ]
}

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
