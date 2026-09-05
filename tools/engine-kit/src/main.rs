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
use engine_kit::{api_engine_grants, build_entries, resolve_cli, DEFAULT_ENGINE, PROBE_ID};

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

    let (entries, engines) = build_entries(
        &artifacts,
        cli.claude.as_deref(),
        cli.codex.as_deref(),
        cli.probe_every_ms,
    );

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
