//! The engines seam's real-composition gate (AGENTS.md standing order 3):
//! every proof boots the engines profile — the engine providers and the
//! probe beside the api trio, the settings pair and the cron seam —
//! through the REAL pinned `jinnd` daemon in the operator layout, and
//! drives the seam as an operator would: plain HTTP on loopback, evidence
//! from the ledger, the daemon log, the profile document of record, and
//! the probe's own written record.
//!
//! The three malleability proofs — SWITCH, COEXISTENCE, EXTENSION — are
//! profile edits against a LIVE daemon, never a rebuild. The vendor
//! providers are exercised where their CLI exists and are honestly
//! environment-gated where it does not; the echo provider carries every
//! proof that must hold everywhere, including CI.
//!
//! Self-skips LOUDLY when no jinnd checkout holding the pinned commit is
//! reachable (KERNEL-PIN.md Gate 2).

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use composition::api::{delete, get, post};
use composition::daemon::{jinnd_source, pinned_commit, pinned_daemon};
use composition::kit::{artifact_hash, entry_mut, fresh_engine_root, Daemon};

/// The switchable slot's entry id and the engine id it serves.
const DEFAULT_ID: &str = "jinn-engine-default";
/// See [`DEFAULT_ID`].
const DEFAULT_ENGINE: &str = "default";
/// The consumer's entry id.
const PROBE: &str = "jinn-engine-probe";
/// The extension proof's entry — NOT in the base document.
const ECHO_ID: &str = "jinn-engine-echo";
/// See [`ECHO_ID`].
const ECHO_ENGINE: &str = "echo";

/// How long a run may take to reach a terminal state before a proof
/// fails. Generous: a vendor CLI's cold start is seconds, and the suite
/// runs several daemons at once.
const RUN_DEADLINE: Duration = Duration::from_secs(90);

/// The pinned daemon binary, or a LOUD skip.
fn gate() -> Option<&'static PathBuf> {
    static BINARY: OnceLock<Option<PathBuf>> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            let commit = pinned_commit().expect("KERNEL-PIN.md parses");
            let Some(source) = jinnd_source(&commit) else {
                eprintln!(
                    "SKIPPED (loudly): real-composition gate found no jinnd checkout holding \
                     pinned commit {commit} — set JINND_DIR, add a sibling ../jinnd, or set \
                     JINND_CLONE_URL (KERNEL-PIN.md Gate 2 discipline)"
                );
                return None;
            };
            Some(pinned_daemon(&source, &commit).expect("the pinned daemon builds"))
        })
        .as_ref()
}

/// Boots a fresh engines root and waits for readiness AND the API's first
/// answer.
fn booted(name: &str) -> Option<(Daemon, u16, PathBuf)> {
    let binary = gate()?;
    let (root, port) = fresh_engine_root(name);
    let daemon = Daemon::boot_operator(binary, &root);
    daemon.await_ready();
    let health = get(port, "/v1/health");
    assert_eq!(health.status, 200, "{}", health.raw);
    Some((daemon, port, root))
}

/// Starts one run and answers its id.
fn start(port: u16, engine: &str, body: &serde_json::Value) -> String {
    let started = post(port, &format!("/v1/engines/{engine}/runs"), body);
    assert_eq!(started.status, 200, "{}", started.raw);
    started.body["run-id"]
        .as_str()
        .unwrap_or_else(|| panic!("a run id: {}", started.raw))
        .to_owned()
}

/// Polls a run until it reaches a terminal state; answers its record.
fn settled(daemon: &Daemon, port: u16, engine: &str, run: &str) -> serde_json::Value {
    let deadline = Instant::now() + RUN_DEADLINE;
    loop {
        let answer = get(port, &format!("/v1/engines/{engine}/runs/{run}"));
        assert_eq!(answer.status, 200, "{}", answer.raw);
        let state = answer.body["state"].as_str().unwrap_or_default().to_owned();
        if matches!(state.as_str(), "exited" | "cancelled" | "failed") {
            return answer.body;
        }
        assert!(
            Instant::now() < deadline,
            "run {run} on {engine} never settled (last state {state:?})\n\
             --- daemon log ---\n{}",
            daemon.log()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// The engines the API lists, by id.
fn listed(port: u16) -> Vec<String> {
    let answer = get(port, "/v1/engines");
    assert_eq!(answer.status, 200, "{}", answer.raw);
    answer.body["engines"]
        .as_array()
        .unwrap_or_else(|| panic!("an engines array: {}", answer.raw))
        .iter()
        .map(|engine| engine["engine"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// A minimal run body: the prompt, tools denied by default, a budget
/// small enough that a runaway is caught inside the deadline.
fn ask(prompt: &str) -> serde_json::Value {
    serde_json::json!({ "prompt": prompt,
                        "budget": { "wall-ms": 60_000, "output-bytes": 262_144 } })
}

#[test]
fn a_run_streams_its_events_in_sequence_and_exits_with_usage() {
    let Some((daemon, port, _root)) = booted("engines-run") else {
        return;
    };
    let run = start(port, DEFAULT_ENGINE, &ask("say ok"));
    let record = settled(&daemon, port, DEFAULT_ENGINE, &run);

    assert_eq!(record["state"], "exited", "{record}");
    assert_eq!(record["status"], 0, "{record}");
    assert!(
        !record["text"].as_str().unwrap_or_default().is_empty(),
        "the run assembled an answer: {record}"
    );
    // The events are the seam's, in the definition's order, and the
    // sequence is dense from 0 — a listener orders on `seq`, so a gap
    // would be a defect the record must not hide.
    let kinds: Vec<&str> = record["events"]
        .as_array()
        .expect("events")
        .iter()
        .map(|event| event["kind"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(kinds.first(), Some(&"started"), "{record}");
    assert_eq!(kinds.last(), Some(&"exited"), "{record}");
    assert!(
        kinds
            .iter()
            .any(|kind| *kind == "delta" || *kind == "turn-end"),
        "the answer arrived as deltas or as one turn: {record}"
    );
    // Usage is reported, not guessed: the exit carries it.
    let exited = record["events"]
        .as_array()
        .expect("events")
        .last()
        .expect("an exit");
    assert!(exited["usage"].is_object(), "{record}");
    assert_eq!(exited["truncated"], false, "{record}");

    // The crossing is on the ledger under the PROVIDER's entry, not the
    // API's: authority and attribution follow the fiber that acted.
    let rows = daemon.ledger_rows();
    assert!(
        rows.iter().any(
            |row| row.entry.as_deref() == Some(DEFAULT_ID) && row.kind.contains("ContractCall")
        ),
        "the provider's own call is attributed to it"
    );
    daemon.interrupt();
}

#[test]
fn the_switchable_slot_changes_package_by_profile_edit_and_no_consumer_moves() {
    let Some((daemon, port, root)) = booted("engines-switch") else {
        return;
    };
    // Before: the slot is served by the echo package.
    let before = get(port, &format!("/v1/engines/{DEFAULT_ENGINE}"));
    assert_eq!(before.status, 200, "{}", before.raw);
    let served_before = before.body["provider"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    assert!(
        served_before.contains("echo"),
        "the slot starts on the echo package: {}",
        before.raw
    );
    let probe_before = document(&daemon)["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|entry| entry["id"] == PROBE)
        .cloned()
        .expect("the probe entry");

    // What the slot is switched TO. A different IMPLEMENTATION brings its
    // own authority and its own machine knowledge with it — a CLI
    // provider needs the executable allowlist and the path its entry
    // carries — so the switch moves the package, the pin, the grants and
    // the data together. What it does NOT move is the entry's id or its
    // ENGINE id, which is the contract every consumer resolves.
    let Some(donor) = document(&daemon)["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|entry| entry["id"] == "jinn-engine-claude")
        .cloned()
    else {
        eprintln!(
            "NOTE: no vendor provider is mounted on this host, so there is no second \
             implementation to switch the slot to. The switch proof needs two packages that \
             can both serve one engine id; it is skipped rather than faked."
        );
        daemon.interrupt();
        return;
    };
    assert_eq!(
        donor["hash"],
        serde_json::json!(artifact_hash(&root, "jinn-engine-claude")),
        "the donor entry is pinned to the artifact the kit built"
    );
    daemon.edit_profile_restarting(DEFAULT_ID, |document| {
        let entry = entry_mut(document, DEFAULT_ID);
        entry["package"] = donor["package"].clone();
        entry["hash"] = donor["hash"].clone();
        entry["config"] = donor["config"].clone();
        // The engine id — and therefore the contract — is the one thing
        // held fixed across the switch.
        entry["config"]["data"]["engine"] = serde_json::json!(DEFAULT_ENGINE);
        for grant in entry["config"]["grants"].as_array_mut().expect("grants") {
            if grant.as_str() == Some("jinn:engine.claude") {
                *grant = serde_json::json!(format!("jinn:engine.{DEFAULT_ENGINE}"));
            }
        }
    });

    // After: the same contract, the same engine id, a different package
    // — and the consumers were never edited.
    daemon.eventually("the switched provider to describe itself", || {
        let after = get(port, &format!("/v1/engines/{DEFAULT_ENGINE}"));
        after.status == 200
            && after.body["provider"]
                .as_str()
                .is_some_and(|provider| provider.contains("claude"))
    });
    let probe_after = document(&daemon)["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .find(|entry| entry["id"] == PROBE)
        .cloned()
        .expect("the probe entry");
    assert_eq!(
        probe_before, probe_after,
        "the consumer's entry is byte-identical across the switch"
    );
    daemon.interrupt();
}

#[test]
fn a_third_provider_joins_a_live_composition_by_profile_edit_alone() {
    let Some((daemon, port, root)) = booted("engines-extend") else {
        return;
    };
    assert!(
        !listed(port).contains(&ECHO_ENGINE.to_owned()),
        "the echo engine is not mounted at boot"
    );
    let echo_hash = artifact_hash(&root, "jinn-engine-echo");

    // The extension: one new entry, against an artifact already on disk.
    // No definition change, no consumer change, no new contract — and
    // the API is granted the new engine in the same edit, because a
    // grant IS the authority the kernel enforces.
    daemon.edit_profile(|document| {
        let entry = serde_json::json!({
            "id": ECHO_ID, "package": "engines/jinn-engine-echo", "hash": echo_hash,
            "config": { "grants": [ format!("jinn:engine.{ECHO_ENGINE}"), "jinn:clock",
                                    { "contract": "jinn:keystore", "scope": ["engines/"],
                                      "ops": ["get"] } ],
                        "data": { "engine": ECHO_ENGINE, "models": ["echo-1"],
                                  "default-model": "echo-1", "delay-ms": 250,
                                  "keep-runs": 8 } } });
        document["entries"]
            .as_array_mut()
            .expect("entries")
            .push(entry);
        let api = entry_mut(document, "jinn-api-http");
        api["config"]["grants"]
            .as_array_mut()
            .expect("grants")
            .push(serde_json::json!(format!("jinn:engine.{ECHO_ENGINE}")));
        let engines = api["config"]["data"]["engines"]
            .as_array_mut()
            .expect("the engines setting");
        engines.push(serde_json::json!(ECHO_ENGINE));
    });

    daemon.eventually("the added engine to answer", || {
        listed(port).contains(&ECHO_ENGINE.to_owned())
    });
    // Both engines are live at once and a request routes by engine id —
    // which IS the contract name (plugins/engines/README.md).
    let engines = listed(port);
    assert!(engines.contains(&DEFAULT_ENGINE.to_owned()), "{engines:?}");
    assert!(engines.contains(&ECHO_ENGINE.to_owned()), "{engines:?}");
    let on_default = start(port, DEFAULT_ENGINE, &ask("say ok"));
    let on_echo = start(port, ECHO_ENGINE, &ask("say ok"));
    let default_record = settled(&daemon, port, DEFAULT_ENGINE, &on_default);
    let echo_record = settled(&daemon, port, ECHO_ENGINE, &on_echo);
    assert_eq!(default_record["engine"], DEFAULT_ENGINE, "{default_record}");
    assert_eq!(echo_record["engine"], ECHO_ENGINE, "{echo_record}");
    // Each run belongs to its own provider: a run id minted by one is not
    // a run of the other.
    let crossed = get(
        port,
        &format!("/v1/engines/{ECHO_ENGINE}/runs/{on_default}"),
    );
    assert_ne!(
        crossed.status, 200,
        "a run minted by one provider is not a run of the other: {}",
        crossed.raw
    );

    // Two providers, two provisions, one topic — the kernel's own view.
    let rows = daemon.ledger_rows();
    for entry in [DEFAULT_ID, ECHO_ID] {
        assert!(
            rows.iter()
                .any(|row| row.entry.as_deref() == Some(entry)
                    && row.kind.contains("ServiceProvided")),
            "{entry} provided its contract"
        );
    }
    daemon.interrupt();
}

#[test]
fn a_secret_outside_the_granted_prefix_is_refused_and_the_run_never_starts() {
    let Some((daemon, port, _root)) = booted("engines-secret") else {
        return;
    };
    // The provider's keystore grant is the `engines/` prefix, read-only.
    // A reference outside it is the kernel's refusal, surfaced typed —
    // and the run must NOT be reported as having run.
    let mut body = ask("say ok");
    body["secrets"] = serde_json::json!({ "SOME_KEY": { "$secret": "elsewhere/key" } });
    let refused = post(port, &format!("/v1/engines/{DEFAULT_ENGINE}/runs"), &body);
    assert_ne!(refused.status, 200, "{}", refused.raw);
    assert_eq!(
        refused.body["error"]["code"], "refused",
        "the kernel's grant refusal reaches the operator typed: {}",
        refused.raw
    );
    // On the record, attributed to the provider that tried.
    daemon.eventually("the refusal on the ledger", || {
        daemon.ledger_rows().iter().any(|row| {
            row.entry.as_deref() == Some(DEFAULT_ID) && row.kind.contains("GrantRefused")
        })
    });
    // A reference INSIDE the prefix but absent is a different, also
    // typed, answer — absence is not a refusal.
    let mut body = ask("say ok");
    body["secrets"] = serde_json::json!({ "SOME_KEY": { "$secret": "engines/not-set" } });
    let absent = post(port, &format!("/v1/engines/{DEFAULT_ENGINE}/runs"), &body);
    assert_ne!(absent.status, 200, "{}", absent.raw);
    assert_ne!(
        absent.body["error"]["code"], "refused",
        "an absent key is not a grant refusal: {}",
        absent.raw
    );
    daemon.interrupt();
}

#[test]
fn a_cancelled_run_is_terminal_and_the_provider_stops_polling() {
    let Some((daemon, port, _root)) = booted("engines-cancel") else {
        return;
    };
    // A cancel needs something LIVE to kill, so the slot's run has to
    // outlive the round trip. The delay is the echo provider's own knob
    // and this is a profile edit like any other.
    daemon.edit_profile_restarting(DEFAULT_ID, |document| {
        entry_mut(document, DEFAULT_ID)["config"]["data"]["delay-ms"] = serde_json::json!(20_000);
    });
    daemon.eventually("the slower provider to answer", || {
        listed(port).contains(&DEFAULT_ENGINE.to_owned())
    });
    let run = start(port, DEFAULT_ENGINE, &ask("say ok"));
    daemon.eventually("the run to be live", || {
        get(port, &format!("/v1/engines/{DEFAULT_ENGINE}/runs/{run}")).body["state"] == "running"
    });
    let cancelled = delete(port, &format!("/v1/engines/{DEFAULT_ENGINE}/runs/{run}"));
    assert_eq!(cancelled.status, 200, "{}", cancelled.raw);

    let record = settled(&daemon, port, DEFAULT_ENGINE, &run);
    assert_eq!(record["state"], "cancelled", "{record}");
    assert!(
        record["events"]
            .as_array()
            .expect("events")
            .iter()
            .any(|event| event["kind"] == "cancelled"),
        "the cancellation is on the run's own record: {record}"
    );
    daemon.interrupt();
}

#[test]
fn a_stop_ends_the_seam_cleanly_and_the_next_boot_re_declares() {
    let Some((daemon, port, root)) = booted("engines-suspend") else {
        return;
    };
    let run = start(port, DEFAULT_ENGINE, &ask("say ok"));
    settled(&daemon, port, DEFAULT_ENGINE, &run);
    daemon.interrupt();

    // The provision is withdrawn on the suspend and re-declared on the
    // next activate — a run does not survive an incarnation, and the
    // provider does not pretend it did.
    let rows = composition::kit::ledger_rows_at(&root);
    assert!(
        rows.iter()
            .any(|row| row.entry.as_deref() == Some(DEFAULT_ID)
                && row.kind.contains("ServiceWithdrawn")),
        "the provision withdrew on the clean stop"
    );

    let binary = gate().expect("the gate held once");
    let restarted = Daemon::boot_operator(binary, &root);
    restarted.await_ready();
    restarted.eventually("the engine to answer again", || {
        listed(port).contains(&DEFAULT_ENGINE.to_owned())
    });
    // The old run id is gone with its incarnation, answered honestly
    // rather than invented.
    let stale = get(port, &format!("/v1/engines/{DEFAULT_ENGINE}/runs/{run}"));
    assert_ne!(stale.status, 200, "{}", stale.raw);
    // And a fresh run works.
    let again = start(port, DEFAULT_ENGINE, &ask("say ok"));
    assert_eq!(
        settled(&restarted, port, DEFAULT_ENGINE, &again)["state"],
        "exited"
    );
    restarted.interrupt();
}

#[test]
fn the_probe_runs_on_its_schedule_and_records_what_it_saw() {
    let Some((daemon, _port, _root)) = booted("engines-probe") else {
        return;
    };
    daemon.eventually("the probe to write its first record", || {
        daemon.data_json("engine-probe/last.json").is_some()
    });
    let record = daemon
        .data_json("engine-probe/last.json")
        .expect("the probe's record");
    assert_eq!(record["engine"], DEFAULT_ENGINE, "{record}");
    // The probe is the seam's ordering witness: it folds the bus events
    // itself and records whether any arrived out of order.
    assert_eq!(record["order-ok"], true, "{record}");
    assert_eq!(record["order-faults"], 0, "{record}");
    assert_eq!(record["malformed-events"], 0, "{record}");
    assert_eq!(record["outcome"], "exited", "{record}");
    assert!(
        !record["text"].as_str().unwrap_or_default().is_empty(),
        "{record}"
    );
    // The record is a document another process reads WHILE the daemon
    // runs; it parses on every read because `jinn:fs` commits whole
    // (FINDINGS.md #22 closed at this pin).
    for _ in 0..20 {
        assert!(
            daemon.data_json("engine-probe/last.json").is_some(),
            "the record is never observed torn"
        );
    }
    daemon.interrupt();
}

#[test]
fn a_vendor_engine_answers_for_real_or_is_honestly_environment_gated() {
    let Some((daemon, port, _root)) = booted("engines-vendor") else {
        return;
    };
    let engines = listed(port);
    let mut exercised = Vec::new();
    for engine in ["claude", "codex"] {
        if !engines.contains(&engine.to_owned()) {
            eprintln!(
                "NOTE: the {engine} provider is not mounted — its CLI is not on this host, so \
                 the kit did not write its entry. The seam's proofs are carried by the echo \
                 providers; no run was faked."
            );
            continue;
        }
        let run = start(port, engine, &ask("Reply with exactly: OK"));
        let record = settled(&daemon, port, engine, &run);
        match record["state"].as_str().unwrap_or_default() {
            "exited" => {
                assert_eq!(record["status"], 0, "{record}");
                assert!(
                    record["text"]
                        .as_str()
                        .is_some_and(|text| text.contains("OK")),
                    "the real engine answered: {record}"
                );
                let usage = &record["usage"];
                assert!(
                    usage["input-tokens"].as_u64().unwrap_or(0) > 0,
                    "a real run reports real usage: {record}"
                );
                exercised.push(format!("{engine}: ran"));
            }
            other => {
                // The honest gate: mounted, correct, and unable to run
                // HERE. Never a faked success.
                eprintln!(
                    "NOTE: the {engine} provider is ENVIRONMENT-GATED on this host \
                     (state {other}): {record}"
                );
                exercised.push(format!("{engine}: environment-gated ({other})"));
            }
        }
    }
    eprintln!("vendor engines: {}", exercised.join("; "));
    daemon.interrupt();
}

/// The profile document of record, read off disk.
fn document(daemon: &Daemon) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(daemon.root.join("profile.json")).expect("profile"))
        .expect("profile parses")
}
