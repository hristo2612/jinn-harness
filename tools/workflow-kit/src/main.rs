//! The workflows seam's kit builder (see Cargo.toml for usage): the two
//! run stores mounted above the two TODO stores, which sit above the two
//! SESSION stores, the engine providers, the api trio, the settings pair
//! and the cron seam, pinned by content hash.
//!
//! The base profile mounts the SWITCHABLE slot (`jinn-workflow-default`,
//! served by the DURABLE store) and a second, ephemeral store live at the
//! same time — the coexistence half. A third store is deliberately NOT
//! mounted: it is the extension proof, added to a LIVE composition by a
//! profile edit alone, against an artifact this kit already built.
//!
//! The FOUR-layer stack is what this profile exists to compose:
//! `jinn:workflow.<store>` -> `jinn:todo.<store>` ->
//! `jinn:session.<store>` -> `jinn:engine.<id>`, each hop a grant the
//! kernel enforces.

use std::path::PathBuf;

use api_kit::{api_entries, settings_entries, PROVIDER_ID};
use cron_kit::{build, cron_entries, flag, write_profile};
use engine_kit::{api_engine_grants, build_entries, resolve_cli};
use session_kit::api_store_grants;
use todo_kit::api_todo_grants;
use workflow_kit::{
    api_workflow_grants, store_entry as workflow_store_entry, Store as WorkflowStore,
    DEFAULT_ID as WORKFLOW_ID, DEFAULT_STORE as WORKFLOW_STORE, FS_PACKAGE as WORKFLOW_FS,
    JOURNAL_DIR as WORKFLOW_DIR, MEMORY_ID as WORKFLOW_MEMORY_ID,
    MEMORY_PACKAGE as WORKFLOW_MEMORY, MEMORY_STORE as WORKFLOW_MEMORY_STORE,
};

/// Where the durable SESSION store's journals live, under the data root.
const SESSION_DIR: &str = "sessions";

struct Cli {
    root: PathBuf,
    port: u16,
    claude: Option<PathBuf>,
    codex: Option<PathBuf>,
    poll_ms: u64,
    probe_every_ms: u64,
    every_ms: u64,
    tick_ms: u64,
}

fn kit(cli: &Cli) {
    let artifacts = cli.root.join("artifacts");
    let scheduler = build(&artifacts, "cron", "cron-scheduler");
    let snapshot = build(&artifacts, "cron", "health-snapshot");
    let http = build(&artifacts, "api", "jinn-api-http");
    let status = build(&artifacts, "api", "jinn-status");
    let edit = build(&artifacts, "api", "jinn-profile-edit");
    let settings = build(&artifacts, "settings", "jinn-settings-profile");
    let settings_store = build(&artifacts, "settings", "jinn-settings-store");
    let session_fs = build(&artifacts, "sessions", "jinn-session-fs");
    let session_memory = build(&artifacts, "sessions", "jinn-session-memory");
    let todo_fs = build(&artifacts, "todos", "jinn-todo-fs");
    let todo_memory = build(&artifacts, "todos", "jinn-todo-memory");
    let workflow_fs = build(&artifacts, "workflows", "jinn-workflow-fs");
    let workflow_memory = build(&artifacts, "workflows", "jinn-workflow-memory");

    let (mut entries, engines) = build_entries(
        &artifacts,
        cli.claude.as_deref(),
        cli.codex.as_deref(),
        cli.probe_every_ms,
    );
    // The two SESSION stores, live at once — mounted exactly as the
    // sessions kit mounts them (one home per fact).
    entries.push(session_kit::store_entry(&session_kit::Store {
        id: session_kit::DEFAULT_ID,
        package: session_kit::FS_PACKAGE,
        hash: &session_fs,
        store: session_kit::DEFAULT_STORE,
        dir: Some(SESSION_DIR),
        engines: &engines,
        poll_ms: cli.poll_ms,
    }));
    entries.push(session_kit::store_entry(&session_kit::Store {
        id: session_kit::MEMORY_ID,
        package: session_kit::MEMORY_PACKAGE,
        hash: &session_memory,
        store: session_kit::MEMORY_STORE,
        dir: None,
        engines: &engines,
        poll_ms: cli.poll_ms,
    }));
    let sessions = [session_kit::DEFAULT_STORE, session_kit::MEMORY_STORE];
    // The two TODO stores above them, mounted exactly as the todos kit
    // mounts them.
    entries.push(todo_kit::store_entry(&todo_kit::Store {
        id: todo_kit::DEFAULT_ID,
        package: todo_kit::FS_PACKAGE,
        hash: &todo_fs,
        store: todo_kit::DEFAULT_STORE,
        dir: Some(todo_kit::JOURNAL_DIR),
        sessions: &sessions,
        poll_ms: cli.poll_ms,
    }));
    entries.push(todo_kit::store_entry(&todo_kit::Store {
        id: todo_kit::MEMORY_ID,
        package: todo_kit::MEMORY_PACKAGE,
        hash: &todo_memory,
        store: todo_kit::MEMORY_STORE,
        dir: None,
        sessions: &sessions,
        poll_ms: cli.poll_ms,
    }));
    let todo_stores = [todo_kit::DEFAULT_STORE, todo_kit::MEMORY_STORE];
    // And the two WORKFLOW run stores above those. The switchable slot is
    // DURABLE, so the swap proof moves it toward the ephemeral one and
    // the difference it makes is visible on disk.
    entries.push(workflow_store_entry(&WorkflowStore {
        id: WORKFLOW_ID,
        package: WORKFLOW_FS,
        hash: &workflow_fs,
        store: WORKFLOW_STORE,
        dir: Some(WORKFLOW_DIR),
        todos: &todo_stores,
        poll_ms: cli.poll_ms,
    }));
    entries.push(workflow_store_entry(&WorkflowStore {
        id: WORKFLOW_MEMORY_ID,
        package: WORKFLOW_MEMORY,
        hash: &workflow_memory,
        store: WORKFLOW_MEMORY_STORE,
        dir: None,
        todos: &todo_stores,
        poll_ms: cli.poll_ms,
    }));
    let workflow_stores = [WORKFLOW_STORE, WORKFLOW_MEMORY_STORE];

    let mut document = cron_entries(&scheduler, &snapshot, cli.every_ms, cli.tick_ms);
    document.extend(api_entries(&http, &status, &edit, cli.port));
    document.extend(settings_entries(
        &settings,
        &settings_store,
        &["cron-scheduler"],
    ));
    document.extend(entries);
    // The API routes to exactly the engines and stores the profile grants
    // it. The grant list IS the authority the kernel enforces; the
    // settings are the same facts told to the provider, written from the
    // same source.
    for entry in &mut document {
        if entry["id"] == PROVIDER_ID {
            let grants = entry["config"]["grants"].as_array_mut().expect("grants");
            grants.extend(api_engine_grants(&engines));
            grants.extend(api_store_grants(&sessions));
            grants.extend(api_todo_grants(&todo_stores));
            grants.extend(api_workflow_grants(&workflow_stores));
            entry["config"]["data"]["engines"] = serde_json::json!(engines);
            entry["config"]["data"]["stores"] = serde_json::json!(sessions);
            entry["config"]["data"]["todo-stores"] = serde_json::json!(todo_stores);
            entry["config"]["data"]["workflow-stores"] = serde_json::json!(workflow_stores);
        }
    }
    write_profile(&cli.root, document);
    println!(
        "workflow stores mounted: {} (over todos: {}; sessions: {}; engines: {})",
        workflow_stores.join(", "),
        todo_stores.join(", "),
        sessions.join(", "),
        engines.join(", ")
    );
}

fn usage() -> ! {
    eprintln!(
        "usage: workflow-kit kit <root> --port N [--claude-bin PATH] [--codex-bin PATH] \
         [--poll-ms N] [--probe-every-ms N] [--every-ms N] [--tick-ms N]"
    );
    std::process::exit(2);
}

fn path_flag(args: &[String], name: &str) -> Option<String> {
    let position = args.iter().position(|arg| arg == name)?;
    args.get(position + 1).cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("kit") {
        usage();
    }
    let root = args.get(1).map(PathBuf::from).unwrap_or_else(|| usage());
    let cli = Cli {
        port: flag(&args, "--port", usage)
            .and_then(|port| u16::try_from(port).ok())
            .unwrap_or_else(|| usage()),
        claude: resolve_cli(path_flag(&args, "--claude-bin").as_deref(), "claude"),
        codex: resolve_cli(path_flag(&args, "--codex-bin").as_deref(), "codex"),
        poll_ms: flag(&args, "--poll-ms", usage).unwrap_or(250),
        probe_every_ms: flag(&args, "--probe-every-ms", usage).unwrap_or(900_000),
        every_ms: flag(&args, "--every-ms", usage).unwrap_or(900_000),
        tick_ms: flag(&args, "--tick-ms", usage).unwrap_or(jinn_cron::DEFAULT_TICK_MS),
        root,
    };
    kit(&cli);
}
