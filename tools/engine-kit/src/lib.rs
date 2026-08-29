//! The engines seam's profile entries (one home per fact: every profile
//! that mounts the seam mounts these exact entries) and the host-side
//! resolution of a vendor CLI's absolute path.
//!
//! Authority is the profile side's, as always. A provider is granted:
//! its own `jinn:engine.<id>` contract (providing IS authority — the
//! kernel checks the grant on `provide` exactly as on a call), `jinn:clock`
//! for its one-shot poll wakes, a `jinn:process` scope naming the ONE
//! executable it may spawn and the environment allowlist it may pass
//! through, and a `jinn:keystore` prefix attenuated to `["get"]` — a
//! provider reads secret VALUES and may never write, delete, or enumerate
//! them (the operation-class attenuation M2-K8 landed, FINDINGS.md #24).

use std::path::{Path, PathBuf};

/// The switchable slot: the entry whose `engine` is `default`. The switch
/// proof swaps its PACKAGE and leaves this id, this engine id, and every
/// consumer alone.
pub const DEFAULT_ID: &str = "jinn-engine-default";
/// The engine id [`DEFAULT_ID`] serves.
pub const DEFAULT_ENGINE: &str = "default";
/// The probe consumer's entry id.
pub const PROBE_ID: &str = "jinn-engine-probe";
/// The extension proof's entry: NOT in the base profile — the composition
/// suite adds it by profile edit alone, against an artifact the kit
/// already built.
pub const ECHO_ID: &str = "jinn-engine-echo";
/// See [`ECHO_ID`].
pub const ECHO_ENGINE: &str = "echo";

/// The PROCESS-lifecycle witness: the echo package in its spawning shape,
/// driving a real child through `jinn:process`. It exists so the seam's
/// lifecycle and grant-refusal proofs — a cancel that kills a pid, a
/// suspend that kills one in flight, an executable outside the exec
/// allowlist, an environment bounded by the env policy — hold on ANY box,
/// including CI and an independent verification that declines to spend a
/// metered vendor fixture. A vendor CLI would prove the same thing and be
/// absent exactly when the proof matters.
pub const SPAWN_ID: &str = "jinn-engine-spawn";
/// See [`SPAWN_ID`].
pub const SPAWN_ENGINE: &str = "spawn";
/// How long the witness child lives when nothing kills it: long enough
/// that a cancel and a suspend always land while it is genuinely
/// running, short enough that a leaked one is gone before the next run.
pub const SPAWN_SECONDS: &str = "30";

/// The `jinn:keystore` prefix engine providers read under. Key NAMES only
/// ever appear in a profile; values live in the kernel's sealed store.
pub const KEYSTORE_PREFIX: &str = "engines/";

/// Every engine guest the kit builds. The package path is the artifact
/// name and the profile's `package`, as elsewhere in this repo.
pub const GUESTS: [&str; 4] = [
    "jinn-engine-echo",
    "jinn-engine-claude",
    "jinn-engine-codex",
    "jinn-engine-probe",
];

/// A provider entry's authority and its machine-local knowledge.
pub struct Provider<'a> {
    /// The profile entry id.
    pub id: &'a str,
    /// The package (and artifact) serving it.
    pub package: &'a str,
    /// Its content hash — the profile's pin (kernel Law 5).
    pub hash: &'a str,
    /// The engine id, which names the contract it provides.
    pub engine: &'a str,
    /// The absolute path of the CLI it spawns, when it spawns one. The
    /// ONLY place a machine path is written.
    pub command: Option<&'a str>,
    /// Further absolute paths the entry's `jinn:process` exec allowlist
    /// admits beyond [`Provider::command`]. Empty for every provider that
    /// drives exactly one CLI; the lifecycle witness needs two, because a
    /// proof about the ENVIRONMENT and a proof about a LIVE child are
    /// different children.
    pub also_exec: &'a [&'a str],
    /// Variables the child may inherit from the host, by name. `HOME`
    /// because every one of these CLIs opens its OWN credential file
    /// under it (the harness never reads those files); `PATH` because a
    /// node-hosted CLI needs its interpreter.
    pub env: &'a [&'a str],
    /// Models it advertises through `describe`.
    pub models: &'a [&'a str],
    /// Extra `config.data` fields (a provider's own knobs).
    pub data: serde_json::Value,
}

/// One provider entry: grants on the left, its own knowledge on the right.
#[must_use]
pub fn provider_entry(provider: &Provider<'_>) -> serde_json::Value {
    let mut grants = vec![
        serde_json::json!(jinn_engine::engine_contract(provider.engine)),
        serde_json::json!(jinn_cron::CLOCK_CONTRACT),
        serde_json::json!({ "contract": "jinn:keystore",
                            "scope": [KEYSTORE_PREFIX], "ops": ["get"] }),
    ];
    if let Some(command) = provider.command {
        let mut exec = vec![command];
        exec.extend_from_slice(provider.also_exec);
        grants.push(serde_json::json!({ "contract": "jinn:process",
            "scope": { "exec": exec, "env": provider.env } }));
    }
    let mut data = serde_json::json!({
        "engine": provider.engine,
        "models": provider.models,
        "default-model": provider.models.first(),
        "poll-ms": 250,
        "keep-runs": 8,
    });
    if let Some(command) = provider.command {
        data["command"] = serde_json::json!(command);
    }
    if let Some(extra) = provider.data.as_object() {
        for (key, value) in extra {
            data[key] = value.clone();
        }
    }
    serde_json::json!({ "id": provider.id, "package": provider.package,
                        "hash": provider.hash,
                        "config": { "grants": grants, "data": data } })
}

/// The probe consumer's entry: it may call exactly the engine it is
/// pointed at, listen on the seam's topic, keep a schedule, and write its
/// record. Nothing else.
#[must_use]
pub fn probe_entry(hash: &str, engine: &str, every_ms: u64, prompt: &str) -> serde_json::Value {
    serde_json::json!({ "id": PROBE_ID, "package": "engines/jinn-engine-probe", "hash": hash,
      "config": { "grants": [jinn_engine::engine_contract(engine),
                             jinn_engine::EVENT_TOPIC,
                             jinn_cron::CLOCK_CONTRACT,
                             { "contract": "jinn:fs", "scope": "engine-probe" }],
                  "data": { "engine": engine, "prompt": prompt, "every-ms": every_ms,
                            "dir": "engine-probe" } } })
}

/// The engine contracts the operator API may route to. A grant per
/// ENGINE is per-engine authority the KERNEL enforces: an API that may
/// run the echo engine and not a paid one is this list, not a code path.
#[must_use]
pub fn api_engine_grants(engines: &[&str]) -> Vec<serde_json::Value> {
    engines
        .iter()
        .map(|engine| serde_json::json!(jinn_engine::engine_contract(engine)))
        .collect()
}

/// Where a vendor CLI is on THIS host: the flag if given, else a `PATH`
/// lookup, else nowhere — in which case the provider is not mounted and
/// the seam is carried by the providers that can run here. Resolving at
/// kit time (rather than letting the sandbox look up `PATH`) is what the
/// `jinn:process` exec allowlist needs: it authorizes an absolute,
/// post-symlink path.
#[must_use]
pub fn resolve_cli(flag: Option<&str>, name: &str) -> Option<PathBuf> {
    if let Some(path) = flag {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::metadata(path)
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_provider_entry_grants_exactly_what_it_needs() {
        let entry = provider_entry(&Provider {
            id: DEFAULT_ID,
            package: "engines/jinn-engine-echo",
            hash: "abc",
            engine: DEFAULT_ENGINE,
            command: None,
            also_exec: &[],
            env: &[],
            models: &["echo-1"],
            data: serde_json::json!({ "delay-ms": 0 }),
        });
        let grants = entry["config"]["grants"].as_array().expect("grants");
        assert_eq!(grants[0], "jinn:engine.default");
        // A provider with no CLI holds NO process authority at all.
        assert!(!grants
            .iter()
            .any(|grant| grant["contract"] == "jinn:process"));
        // The keystore grant is read-only: values in, never out (#24).
        let keystore = grants
            .iter()
            .find(|grant| grant["contract"] == "jinn:keystore")
            .expect("a keystore grant");
        assert_eq!(keystore["ops"], serde_json::json!(["get"]));
        assert_eq!(keystore["scope"], serde_json::json!(["engines/"]));
        // Its own knobs merge over the shared defaults; the shared ones stay.
        assert_eq!(entry["config"]["data"]["delay-ms"], 0);
        assert_eq!(entry["config"]["data"]["poll-ms"], 250);
        assert_eq!(entry["config"]["data"]["default-model"], "echo-1");
        assert!(entry["config"]["data"].get("command").is_none());
    }

    #[test]
    fn a_cli_provider_may_spawn_one_executable_with_a_named_environment() {
        let entry = provider_entry(&Provider {
            id: "jinn-engine-codex",
            package: "engines/jinn-engine-codex",
            hash: "def",
            engine: "codex",
            command: Some("/opt/example/bin/codex"),
            also_exec: &[],
            env: &["HOME", "PATH"],
            models: &[],
            data: serde_json::Value::Null,
        });
        let process = entry["config"]["grants"]
            .as_array()
            .expect("grants")
            .iter()
            .find(|grant| grant["contract"] == "jinn:process")
            .expect("a process grant");
        assert_eq!(
            process["scope"],
            serde_json::json!({ "exec": ["/opt/example/bin/codex"], "env": ["HOME", "PATH"] })
        );
        // The path is the ENTRY's knowledge, and the provider is told it.
        assert_eq!(entry["config"]["data"]["command"], "/opt/example/bin/codex");
        // No model advertised is an empty list, never a guess.
        assert_eq!(entry["config"]["data"]["models"], serde_json::json!([]));
        assert!(entry["config"]["data"]["default-model"].is_null());
    }

    #[test]
    fn the_probe_may_reach_exactly_the_engine_it_is_pointed_at() {
        let entry = probe_entry("ghi", "default", 2_000, "say ok");
        let grants = entry["config"]["grants"].as_array().expect("grants");
        assert!(grants.contains(&serde_json::json!("jinn:engine.default")));
        assert!(grants.contains(&serde_json::json!("jinn:engine/event")));
        assert!(!grants.contains(&serde_json::json!("jinn:engine.codex")));
        assert_eq!(entry["config"]["data"]["every-ms"], 2_000);
    }

    #[test]
    fn a_cli_that_is_not_on_this_host_resolves_to_nothing() {
        assert!(resolve_cli(Some("/nonexistent/definitely-not-here"), "x").is_none());
        assert!(resolve_cli(None, "definitely-not-a-real-binary-name-9f3a").is_none());
        // A real one on every POSIX host, found through PATH.
        assert!(resolve_cli(None, "sh").is_some_and(|path| path.is_absolute()));
    }
}
