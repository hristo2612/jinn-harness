//! The todos seam's kit builder (see Cargo.toml for usage): the two Todo
//! stores mounted beside the two SESSION stores, the engine providers,
//! the api trio, the settings pair and the cron seam, pinned by content
//! hash.
//!
//! The base profile mounts the SWITCHABLE slot (`jinn-todo-default`,
//! served by the DURABLE store) and a second, ephemeral store live at the
//! same time — the coexistence half. A third store is deliberately NOT
//! mounted: it is the extension proof, added to a LIVE composition by a
//! profile edit alone, against an artifact this kit already built.
//!
//! The three-layer stack is what this profile exists to compose:
//! `jinn:todo.<store>` -> `jinn:session.<store>` -> `jinn:engine.<id>`,
//! each hop a grant the kernel enforces.

use std::path::PathBuf;

use api_kit::{api_entries, settings_entries, PROVIDER_ID};
use cron_kit::{build, cron_entries, flag, write_profile};
use engine_kit::{
    api_engine_grants, probe_entry, provider_entry, resolve_cli, Provider, DEFAULT_ENGINE,
    SPAWN_ENGINE, SPAWN_ID, SPAWN_SECONDS,
};
use session_kit::api_store_grants;
use todo_kit::{
    api_todo_grants, store_entry as todo_store_entry, Store as TodoStore, DEFAULT_ID as TODO_ID,
    DEFAULT_STORE as TODO_STORE, FS_PACKAGE as TODO_FS, JOURNAL_DIR as TODO_DIR,
    MEMORY_ID as TODO_MEMORY_ID, MEMORY_PACKAGE as TODO_MEMORY, MEMORY_STORE as TODO_MEMORY_STORE,
};

/// See `engine-kit`'s own constants — the vendor model lists and the env
/// allowlist have one home there, and this kit borrows them by mounting
/// the same entries rather than restating them.
const CLI_ENV: [&str; 2] = ["HOME", "PATH"];
const CLAUDE_MODELS: [&str; 2] = ["claude-haiku-4-5-20251001", "claude-sonnet-5"];
const CODEX_MODELS: [&str; 1] = ["gpt-5.6-sol"];
const PROBE_PROMPT: &str = "Reply with exactly: OK";

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
    let echo = build(&artifacts, "engines", "jinn-engine-echo");
    let claude = build(&artifacts, "engines", "jinn-engine-claude");
    let codex = build(&artifacts, "engines", "jinn-engine-codex");
    let probe = build(&artifacts, "engines", "jinn-engine-probe");
    let session_fs = build(&artifacts, "sessions", "jinn-session-fs");
    let session_memory = build(&artifacts, "sessions", "jinn-session-memory");
    let todo_fs = build(&artifacts, "todos", "jinn-todo-fs");
    let todo_memory = build(&artifacts, "todos", "jinn-todo-memory");

    let mut entries = vec![provider_entry(&Provider {
        id: "jinn-engine-default",
        package: "engines/jinn-engine-echo",
        hash: &echo,
        engine: DEFAULT_ENGINE,
        command: None,
        also_exec: &[],
        env: &[],
        models: &["echo-1"],
        // A POSITIVE delay for the reason the engines kit gives: a
        // synchronous echo emits its whole run from inside its CALLER's
        // dispatch, and here that caller is a session store driven in
        // turn by a Todo store. Deferring the finish to a clock wake puts
        // the emit on the engine's own fiber and gives both polls a
        // genuinely live run to observe.
        data: serde_json::json!({ "delay-ms": 250 }),
    })];
    let mut engines = vec![DEFAULT_ENGINE];
    // The process-lifecycle witness: a second, genuinely different engine
    // provider that runs on any POSIX host. It is what "the SAME Todo
    // over another engine" is proven against when no vendor CLI is
    // authenticated here.
    let sleep = resolve_cli(None, "sleep");
    let printenv = resolve_cli(None, "env");
    let denied = resolve_cli(None, "sh");
    if let (Some(sleep), Some(printenv), Some(denied)) = (&sleep, &printenv, &denied) {
        let (sleep, printenv) = (sleep.display().to_string(), printenv.display().to_string());
        entries.push(provider_entry(&Provider {
            id: SPAWN_ID,
            package: "engines/jinn-engine-echo",
            hash: &echo,
            engine: SPAWN_ENGINE,
            command: Some(&sleep),
            also_exec: &[&printenv],
            env: &CLI_ENV,
            models: &["witness-1"],
            data: serde_json::json!({
                "args": [SPAWN_SECONDS],
                "env-command": printenv,
                "denied-command": denied.display().to_string(),
            }),
        }));
        engines.push(SPAWN_ENGINE);
    }
    if let Some(command) = &cli.claude {
        entries.push(provider_entry(&Provider {
            id: "jinn-engine-claude",
            package: "engines/jinn-engine-claude",
            hash: &claude,
            engine: "claude",
            command: Some(&command.display().to_string()),
            also_exec: &[],
            env: &CLI_ENV,
            models: &CLAUDE_MODELS,
            data: serde_json::Value::Null,
        }));
        engines.push("claude");
    }
    if let Some(command) = &cli.codex {
        entries.push(provider_entry(&Provider {
            id: "jinn-engine-codex",
            package: "engines/jinn-engine-codex",
            hash: &codex,
            engine: "codex",
            command: Some(&command.display().to_string()),
            also_exec: &[],
            env: &CLI_ENV,
            models: &CODEX_MODELS,
            data: serde_json::Value::Null,
        }));
        engines.push("codex");
    }
    entries.push(probe_entry(
        &probe,
        DEFAULT_ENGINE,
        cli.probe_every_ms,
        PROBE_PROMPT,
    ));
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
    // And the two TODO stores above them. The switchable slot is
    // DURABLE, so the swap proof moves it toward the ephemeral one and
    // the difference it makes is visible on disk.
    entries.push(todo_store_entry(&TodoStore {
        id: TODO_ID,
        package: TODO_FS,
        hash: &todo_fs,
        store: TODO_STORE,
        dir: Some(TODO_DIR),
        sessions: &sessions,
        poll_ms: cli.poll_ms,
    }));
    entries.push(todo_store_entry(&TodoStore {
        id: TODO_MEMORY_ID,
        package: TODO_MEMORY,
        hash: &todo_memory,
        store: TODO_MEMORY_STORE,
        dir: None,
        sessions: &sessions,
        poll_ms: cli.poll_ms,
    }));
    let todo_stores = [TODO_STORE, TODO_MEMORY_STORE];

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
            entry["config"]["data"]["engines"] = serde_json::json!(engines);
            entry["config"]["data"]["stores"] = serde_json::json!(sessions);
            entry["config"]["data"]["todo-stores"] = serde_json::json!(todo_stores);
        }
    }
    write_profile(&cli.root, document);
    println!(
        "todo stores mounted: {} (over sessions: {}; engines: {})",
        todo_stores.join(", "),
        sessions.join(", "),
        engines.join(", ")
    );
}

fn usage() -> ! {
    eprintln!(
        "usage: todo-kit kit <root> --port N [--claude-bin PATH] [--codex-bin PATH] \
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
