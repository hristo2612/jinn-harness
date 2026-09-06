//! The UI seam's kit builder (see Cargo.toml for usage). `variant` builds
//! a second provider from the same archive (a marked document, or a
//! corrupted blob) for the swap and fail-closed proofs.

use std::path::{Path, PathBuf};

use api_kit::{api_entries, settings_entries, PROVIDER_ID};
use cron_kit::{build, component, cron_entries, flag, write_artifact, write_profile};
use ext_kit::{ext_entry, GREEN_BUDGET, GREEN_ID, GREEN_SOURCE};
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

fn kit(root: &Path, port: u16, every_ms: u64, tick_ms: u64, chat: Option<(&str, &str)>) {
    let chat = chat.map(|(command, home)| {
        let command = ui_kit::codex::executable(Path::new(command))
            .unwrap_or_else(|error| panic!("Text Chat executable: {error}"));
        let home = std::fs::canonicalize(home).expect("dedicated Codex home exists");
        (command, home)
    });
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
    // The operator's example from §6: ONE extension, origin `human`,
    // under the budget the kernel honors since pin `b1dbe8f` (M2-K25).
    entries.push(ext_entry(
        GREEN_ID,
        &ext,
        &[TOPIC_BEFORE_SEND],
        GREEN_SOURCE,
        Origin::Human,
        Some(GREEN_BUDGET),
    ));

    let tasks = build(&artifacts, "sessions", "jinn-session-fs");
    let todos = build(&artifacts, "todos", "jinn-todo-fs");
    entries.push(session_kit::store_entry(&session_kit::Store {
        id: "task-sessions",
        package: session_kit::FS_PACKAGE,
        hash: &tasks,
        store: "tasks",
        dir: Some("task-history"),
        engines: &["codex"],
        poll_ms: 250,
    }));
    entries.push(todo_kit::store_entry(&todo_kit::Store {
        id: "work-todos",
        package: todo_kit::FS_PACKAGE,
        hash: &todos,
        store: "work",
        dir: Some("todo-history"),
        sessions: &["tasks"],
        poll_ms: 250,
    }));
    if let Some((command, codex_home)) = &chat {
        let home = root.join("chat-home");
        std::fs::create_dir_all(home.join("workspace")).expect("isolated chat workspace");
        let home = std::fs::canonicalize(home).expect("chat home exists");
        let engine = build(&artifacts, "engines", "jinn-engine-codex");
        let sessions = build(&artifacts, "sessions", "jinn-session-fs");
        entries.push(engine_kit::provider_entry(&engine_kit::Provider {
            id: "chat-codex", package: "engines/jinn-engine-codex", hash: &engine,
            engine: "codex", command: Some(&command.to_string_lossy()), also_exec: &[],
            env: &["HOME", "CODEX_HOME", "PATH"], models: &["gpt-6-astra"],
            data: serde_json::json!({"text-chat":{"home":home,"codex-home":codex_home,"cwd":home.join("workspace")}}),
        }));
        entries.push(session_kit::store_entry(&session_kit::Store {
            id: "chat-sessions",
            package: session_kit::FS_PACKAGE,
            hash: &sessions,
            store: "chat",
            dir: Some("chat-history"),
            engines: &["codex"],
            poll_ms: 250,
        }));
    }
    let catalogs = [MAIN_CATALOG, PARKED_CATALOG];
    for entry in &mut entries {
        if entry["id"] == PROVIDER_ID {
            let grants = entry["config"]["grants"].as_array_mut().expect("grants");
            grants.extend(api_catalog_grants(&catalogs));
            entry["config"]["data"]["catalogs"] = serde_json::json!(catalogs);
            entry["config"]["grants"]
                .as_array_mut()
                .expect("grants")
                .extend(todo_kit::api_todo_grants(&["work"]));
            entry["config"]["grants"]
                .as_array_mut()
                .expect("grants")
                .extend(session_kit::api_store_grants(&["tasks"]));
            entry["config"]["data"]["todo-stores"] = serde_json::json!(["work"]);
            entry["config"]["data"]["stores"] = serde_json::json!(["tasks"]);
            if chat.is_some() {
                entry["config"]["grants"]
                    .as_array_mut()
                    .expect("grants")
                    .extend(session_kit::api_store_grants(&["chat"]));
                entry["config"]["data"]["stores"] = serde_json::json!(["chat", "tasks"]);
            }
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
        "usage: ui-kit kit <root> --port N [--every-ms N] [--tick-ms N] [--codex-bin PATH --codex-home PATH]\n       ui-kit variant <root> --name NAME [--marker TEXT] [--corrupt]"
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
            let chat = match (
                text_flag(&args, "--codex-bin"),
                text_flag(&args, "--codex-home"),
            ) {
                (Some(command), Some(home)) => Some((command, home)),
                (None, None) => None,
                _ => usage(),
            };
            kit(
                &root,
                port,
                flag(&args, "--every-ms", usage).unwrap_or(900_000),
                flag(&args, "--tick-ms", usage).unwrap_or(jinn_cron::DEFAULT_TICK_MS),
                chat,
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
