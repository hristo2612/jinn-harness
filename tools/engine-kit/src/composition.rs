//! The engine composition shared by every kit that runs work.
//! Vendor configuration and process authority are profile policy (R10),
//! owned here rather than copied by each higher-level seam.

use std::path::Path;

use crate::{
    probe_entry, provider_entry, resolve_cli, Provider, DEFAULT_ENGINE, DEFAULT_ID, SPAWN_ENGINE,
    SPAWN_ID, SPAWN_SECONDS,
};
use cron_kit::build;

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

/// Builds all engine artifacts and returns their mounted entries and engine ids.
///
/// The default echo and its probe are always mounted. The process witness
/// is mounted when its three executables exist, and vendor providers only
/// when the caller supplies their CLI paths. Unmounted artifacts remain
/// available for later profile edits. Entries and ids retain the same order.
///
/// # Panics
///
/// If a guest cannot be built or its artifact cannot be written.
#[must_use]
pub fn build_entries(
    artifacts: &Path,
    claude_bin: Option<&Path>,
    codex_bin: Option<&Path>,
    probe_every_ms: u64,
) -> (Vec<serde_json::Value>, Vec<&'static str>) {
    let echo = build(artifacts, "engines", "jinn-engine-echo");
    let claude = build(artifacts, "engines", "jinn-engine-claude");
    let codex = build(artifacts, "engines", "jinn-engine-codex");
    let probe = build(artifacts, "engines", "jinn-engine-probe");

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
    if let Some(command) = claude_bin {
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
    if let Some(command) = codex_bin {
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
        probe_every_ms,
        PROBE_PROMPT,
    ));

    (entries, engines)
}
