//! The engines seam's kit builder (see Cargo.toml for usage): the engine
//! providers and the probe mounted beside the api trio, the settings pair
//! and the cron seam, pinned by content hash.
//!
//! The base profile mounts the SWITCHABLE slot (`jinn-engine-default`,
//! served by the echo package) plus whichever vendor providers this host
//! can actually run. The echo provider's own entry is deliberately NOT
//! mounted: it is the extension proof, added to a LIVE composition by a
//! profile edit alone, against an artifact this kit already built.

use std::path::PathBuf;

use api_kit::{api_entries, settings_entries, PROVIDER_ID};
use cron_kit::{build, cron_entries, flag, write_profile};
use engine_kit::{
    api_engine_grants, probe_entry, provider_entry, resolve_cli, Provider, DEFAULT_ENGINE,
    DEFAULT_ID, PROBE_ID, SPAWN_ENGINE, SPAWN_ID, SPAWN_SECONDS,
};

/// The models each vendor provider advertises through `describe`. A model
/// list is knowledge about a vendor, not about this host, so it belongs in
/// the generated profile rather than in a provider's source.
const CLAUDE_MODELS: [&str; 2] = ["claude-haiku-4-5-20251001", "claude-sonnet-5"];
/// See [`CLAUDE_MODELS`].
const CODEX_MODELS: [&str; 1] = ["gpt-5.6-sol"];

/// A vendor CLI opens its OWN credential file under `$HOME`, so `HOME` is
/// the whole reason this allowlist is not empty. `PATH` rides along
/// because a node-hosted CLI needs its interpreter. Nothing else is
/// inherited: an allowlist, never inherit-all.
const CLI_ENV: [&str; 2] = ["HOME", "PATH"];

/// The one-line prompt the probe sends. Neutral, and short enough that a
/// real run against a metered engine costs almost nothing.
const PROBE_PROMPT: &str = "Reply with exactly: OK";

struct Cli {
    root: PathBuf,
    port: u16,
    claude: Option<PathBuf>,
    codex: Option<PathBuf>,
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
    let store = build(&artifacts, "settings", "jinn-settings-store");
    let echo = build(&artifacts, "engines", "jinn-engine-echo");
    let claude = build(&artifacts, "engines", "jinn-engine-claude");
    let codex = build(&artifacts, "engines", "jinn-engine-codex");
    let probe = build(&artifacts, "engines", "jinn-engine-probe");

    // The switchable slot starts on the echo package: a composition boots
    // and answers with no vendor CLI anywhere, and the switch proof moves
    // it to a real engine by editing this one entry.
    let mut entries = vec![provider_entry(&Provider {
        id: DEFAULT_ID,
        package: "engines/jinn-engine-echo",
        hash: &echo,
        engine: DEFAULT_ENGINE,
        command: None,
        also_exec: &[],
        env: &[],
        models: &["echo-1"],
        // A POSITIVE delay is required here, not cosmetic: the probe
        // LISTENS on the seam's topic, and a synchronous echo would emit
        // its whole run from inside the caller's own `services::call` —
        // the delivery would park on the caller's busy supervisor until
        // the guest deadline (FINDINGS.md #4, nested dispatch). Deferring
        // the finish to a clock wake puts the emit on the provider's own
        // fiber, and it also gives `cancel` and `run-get` a genuinely
        // live run to act on. `delay-ms: 0` stays right for a driver that
        // does not listen.
        data: serde_json::json!({ "delay-ms": 250 }),
    })];
    let mut engines = vec![DEFAULT_ENGINE];
    // The process-lifecycle witness. `sleep` is a child that is reliably
    // ALIVE when a cancel or a suspend lands; `env` is a child that says
    // what it can see, which is how the entry's env policy is checked
    // rather than asserted. Both are POSIX, so the proofs hold wherever
    // the suite runs; a host missing them simply does not mount the
    // witness and the lifecycle proofs skip LOUDLY rather than lying.
    let sleep = resolve_cli(None, "sleep");
    let printenv = resolve_cli(None, "env");
    // NOT in the exec allowlist, and that is its whole job: the refusal
    // probe needs an executable that certainly exists and is certainly
    // unauthorized, so the refusal is the kernel's and not a typo's.
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
                // Read by the proofs out of the document, never hardcoded
                // in a test: a machine path lives in the profile only.
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

    let mut document = cron_entries(&scheduler, &snapshot, cli.every_ms, cli.tick_ms);
    document.extend(api_entries(&http, &status, &edit, cli.port));
    document.extend(settings_entries(&settings, &store, &["cron-scheduler"]));
    document.extend(entries);
    // The API routes to exactly the engines the profile grants it. The
    // grant list IS the authority the kernel enforces; the setting is the
    // same fact told to the provider, written from the same source.
    for entry in &mut document {
        if entry["id"] == PROVIDER_ID {
            let grants = entry["config"]["grants"].as_array_mut().expect("grants");
            grants.extend(api_engine_grants(&engines));
            entry["config"]["data"]["engines"] = serde_json::json!(engines);
        }
    }
    write_profile(&cli.root, document);
    println!(
        "engines mounted: {} (probe -> {DEFAULT_ENGINE}, entry {PROBE_ID})",
        engines.join(", ")
    );
}

fn usage() -> ! {
    eprintln!(
        "usage: engine-kit kit <root> --port N [--claude-bin PATH] [--codex-bin PATH] \
         [--probe-every-ms N] [--every-ms N] [--tick-ms N]"
    );
    std::process::exit(2);
}

/// A `--name <value>` string flag.
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
        probe_every_ms: flag(&args, "--probe-every-ms", usage).unwrap_or(900_000),
        every_ms: flag(&args, "--every-ms", usage).unwrap_or(900_000),
        tick_ms: flag(&args, "--tick-ms", usage).unwrap_or(jinn_cron::DEFAULT_TICK_MS),
        root,
    };
    kit(&cli);
}
