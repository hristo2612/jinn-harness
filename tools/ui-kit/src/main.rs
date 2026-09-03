//! The UI seam's kit builder (see Cargo.toml for usage). `variant` builds
//! a second provider from the same archive (a marked document, or a
//! corrupted blob) for the swap and fail-closed proofs.

use std::path::{Path, PathBuf};

use api_kit::{api_entries, settings_entries, PROVIDER_ID};
use cron_kit::{build, component, cron_entries, flag, write_artifact, write_profile};
use ext_kit::{ext_entry, GREEN_ID, GREEN_SOURCE};
use jinn_ext::Origin;
use jinn_ui::TOPIC_BEFORE_SEND;
use plugin_kit::{
    api_catalog_grants, fixed_entry, live_entry, misbound_entry, FIXED_ID, MAIN_CATALOG,
    PARKED_CATALOG, SHELVED_ID,
};
use ui_kit::{
    archive, build_web, bundle_entry, marked, mount_bundle_on, mount_moments_on, write_bundle,
    BUNDLE_DIR, BUNDLE_DIR_VAR, BUNDLE_PACKAGE,
};

/// Builds the embedded provider with `$JINN_UI_BUNDLE_DIR` pointed at
/// `bundle_dir` and writes it under `name`; answers its pin.
fn build_provider(artifacts: &Path, bundle_dir: &Path, name: &str) -> String {
    let absolute = std::fs::canonicalize(bundle_dir).expect("bundle dir exists");
    // The provider's build reads the variable at compile time (its build
    // script declares the dependency); the kit's own process is what the
    // guest build inherits.
    std::env::set_var(BUNDLE_DIR_VAR, &absolute);
    let (bytes, hash) = component("ui", "jinn-ui-bundle-embedded");
    write_artifact(artifacts, name, &bytes, &hash);
    hash
}

fn kit(root: &Path, port: u16, every_ms: u64, tick_ms: u64) {
    let artifacts = root.join("artifacts");
    let out = build_web();
    let files = archive(&out);
    let bundle_dir = root.join(BUNDLE_DIR);
    write_bundle(&bundle_dir, &files);
    let bundle = build_provider(&artifacts, &bundle_dir, "jinn-ui-bundle-embedded");

    let scheduler = build(&artifacts, "cron", "cron-scheduler");
    let snapshot = build(&artifacts, "cron", "health-snapshot");
    let http = build(&artifacts, "api", "jinn-api-http");
    let status = build(&artifacts, "api", "jinn-status");
    let edit = build(&artifacts, "api", "jinn-profile-edit");
    let settings = build(&artifacts, "settings", "jinn-settings-profile");
    let store = build(&artifacts, "settings", "jinn-settings-store");
    let live = build(&artifacts, "plugins", "jinn-plugins-profile");
    let fixed = build(&artifacts, "plugins", "jinn-plugins-static");
    let (ext, ext_size) = ext_kit::build(&artifacts);
    println!("{} {ext_size} bytes sha256 {ext}", ext_kit::BOA_GUEST);

    let mut entries = cron_entries(&scheduler, &snapshot, every_ms, tick_ms);
    entries.extend(api_entries(&http, &status, &edit, port));
    entries.extend(settings_entries(&settings, &store, &["cron-scheduler"]));
    // The plugins seam exactly as plugin-kit mounts it, the failing and
    // the shelved entries included: the plugins page is ported to show
    // them, and a page that claims to is proven on a tree that has them.
    entries.push(live_entry(&live, MAIN_CATALOG));
    entries.push(fixed_entry(FIXED_ID, &fixed, PARKED_CATALOG, false));
    entries.push(fixed_entry(SHELVED_ID, &fixed, "shelf", true));
    entries.push(misbound_entry(
        &http,
        port.wrapping_add(1),
        port.wrapping_add(2),
    ));
    entries.push(bundle_entry(BUNDLE_PACKAGE, &bundle));
    // The operator's example from §6: ONE extension, origin `human`.
    entries.push(ext_entry(
        GREEN_ID,
        &ext,
        &[TOPIC_BEFORE_SEND],
        GREEN_SOURCE,
        Origin::Human,
    ));

    let catalogs = [MAIN_CATALOG, PARKED_CATALOG];
    for entry in &mut entries {
        if entry["id"] == PROVIDER_ID {
            let grants = entry["config"]["grants"].as_array_mut().expect("grants");
            grants.extend(api_catalog_grants(&catalogs));
            entry["config"]["data"]["catalogs"] = serde_json::json!(catalogs);
            mount_bundle_on(entry);
            mount_moments_on(entry);
        }
    }
    write_profile(root, entries);
}

/// A second provider from the kit's archive: `--marker TEXT` stamps the
/// document; `--corrupt` flips one byte of the first asset INSIDE the
/// blob after the manifest was written, so the bytes no longer match it.
fn variant(root: &Path, name: &str, marker: Option<&str>, corrupt: bool) {
    let source = root.join(BUNDLE_DIR);
    let blob = std::fs::read(source.join("bundle.bin")).expect("the kit's bundle");
    let mut files = jinn_ui::decode_bundle(&blob).expect("the kit's bundle decodes");
    if let Some(marker) = marker {
        files = marked(&files, marker);
    }
    let out = root.join(format!("{BUNDLE_DIR}-{name}"));
    write_bundle(&out, &files);
    if corrupt {
        let path = out.join("bundle.bin");
        let mut blob = std::fs::read(&path).expect("variant bundle");
        let asset = files
            .iter()
            .find(|(path, _)| path.starts_with(jinn_ui::ASSETS_PREFIX))
            .expect("an asset to corrupt");
        let at = blob
            .windows(asset.1.len())
            .position(|window| window == asset.1.as_slice())
            .expect("the asset's bytes in the blob");
        blob[at] ^= 0xff;
        std::fs::write(&path, blob).expect("corrupted bundle write");
        println!("corrupted one byte of {} in {}", asset.0, path.display());
    }
    build_provider(&root.join("artifacts"), &out, name);
}

fn usage() -> ! {
    eprintln!(
        "usage: ui-kit kit <root> --port N [--every-ms N] [--tick-ms N]\n       ui-kit variant <root> --name NAME [--marker TEXT] [--corrupt]"
    );
    std::process::exit(2);
}

fn text_flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    let position = args.iter().position(|arg| arg == name)?;
    Some(args.get(position + 1).unwrap_or_else(|| usage()))
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = args.get(1).map(PathBuf::from).unwrap_or_else(|| usage());
    match args.first().map(String::as_str) {
        Some("kit") => {
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
        Some("variant") => variant(
            &root,
            text_flag(&args, "--name").unwrap_or_else(|| usage()),
            text_flag(&args, "--marker"),
            args.iter().any(|arg| arg == "--corrupt"),
        ),
        _ => usage(),
    }
}
