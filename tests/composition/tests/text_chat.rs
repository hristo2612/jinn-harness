//! Opt-in, metered vendor proof. The root MUST come from this head's ui-kit
//! with --codex-bin/--codex-home; no echo provider or hand-mounted substitute.
use std::path::PathBuf;
use std::time::{Duration, Instant};

use composition::api::{delete, get, post};
use composition::daemon::{jinnd_source, pinned_commit, pinned_daemon};
use composition::kit::Daemon;
use serde_json::{json, Value};

fn record(port: u16, id: &str) -> Value {
    let response = get(port, &format!("/v1/sessions/chat/{id}"));
    assert_eq!(response.status, 200);
    response.body
}

fn until(port: u16, id: &str, mut condition: impl FnMut(&Value) -> bool) -> Value {
    let started = Instant::now();
    loop {
        let saved = record(port, id);
        if condition(&saved) {
            return saved;
        }
        assert!(
            started.elapsed() < Duration::from_secs(100),
            "vendor turn did not satisfy its bounded condition: {saved}"
        );
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn send(port: u16, id: &str, message: &str) {
    let response = post(
        port,
        &format!("/v1/sessions/chat/{id}/turns"),
        &json!({"message":message}),
    );
    assert_eq!(response.status, 200, "{}", response.raw);
}

#[test]
fn real_astra_partials_context_cancel_and_restart_at_the_archived_pin() {
    let Ok(root) = std::env::var("JINN_TEXT_CHAT_ROOT") else {
        eprintln!("SKIPPED (loudly): set JINN_TEXT_CHAT_ROOT to a rebuilt ui-kit Chat profile for the metered vendor proof");
        return;
    };
    let root = PathBuf::from(root);
    let document: Value =
        serde_json::from_slice(&std::fs::read(root.join("profile.json")).unwrap()).unwrap();
    let entries = document["entries"].as_array().expect("kit entries");
    let api = entries
        .iter()
        .find(|entry| entry["id"] == "jinn-api-http")
        .unwrap();
    let port = u16::try_from(api["config"]["data"]["port"].as_u64().unwrap()).unwrap();
    let provider = entries
        .iter()
        .find(|entry| entry["id"] == "chat-codex")
        .unwrap();
    assert_eq!(provider["package"], "engines/jinn-engine-codex");
    assert!(provider["config"]["data"]["text-chat"].is_object());
    let pin = pinned_commit().unwrap();
    let source = jinnd_source(&pin).expect("the exact pin is mandatory for this vendor proof");
    let binary = pinned_daemon(&source, &pin).unwrap();
    let daemon = Daemon::boot_operator(&binary, &root);
    daemon.await_ready();
    assert_eq!(
        get(port, "/").status,
        200,
        "the rebuilt UI is served by this composition"
    );
    let spec = json!({"engine":{"engine":"codex","model":"gpt-6-astra","effort":"high"},"tools":{"mode":"denied","allow":[]},"transcript-context":true});
    let created = post(port, "/v1/sessions/chat", &spec);
    assert_eq!(created.status, 200, "{}", created.raw);
    let id = created.body["session-id"].as_str().unwrap();
    let fold = post(
        port,
        "/v1/moments/ui/before-send",
        &json!({"text":"Write a 180-word story about a lighthouse named Copper Finch. No tools.","attachments":[],"session-id":id}),
    );
    assert_eq!(fold.status, 200);
    let message = fold.body["text"].as_str().unwrap();
    assert!(
        message.ends_with('🟢'),
        "real profile before-send transformation"
    );
    let started = Instant::now();
    send(port, id, message);
    let mut prefixes = Vec::<String>::new();
    let completed = until(port, id, |saved| {
        let turn = &saved["log"][0];
        let text = turn["answer"].as_str().unwrap_or_default();
        if turn["status"] == "running"
            && !text.is_empty()
            && prefixes.last().is_none_or(|last| last != text)
        {
            eprintln!(
                "visible partial {} at {:?}, {} bytes",
                prefixes.len() + 1,
                started.elapsed(),
                text.len()
            );
            prefixes.push(text.to_owned());
        }
        turn["status"] != "running"
    });
    assert_eq!(completed["log"][0]["status"], "done", "{completed}");
    assert!(
        prefixes.len() >= 2,
        "two actual prefixes before the terminal snapshot"
    );
    assert_eq!(completed["log"][0]["message"], message);
    assert!(completed["event-after"].is_u64());
    eprintln!(
        "terminal at {:?}; {} distinct visible prefixes",
        started.elapsed(),
        prefixes.len()
    );
    send(
        port,
        id,
        "What was the lighthouse's name in your previous story? Reply with just the name.",
    );
    let followup = until(port, id, |saved| saved["log"][1]["status"] != "running");
    assert_eq!(followup["log"][1]["status"], "done", "{followup}");
    assert!(
        followup["log"][1]["answer"]
            .as_str()
            .unwrap()
            .contains("Copper Finch"),
        "{followup}"
    );
    send(port, id, "Write a long 1500-word continuation, no tools.");
    let partial = until(port, id, |saved| {
        saved["log"][2]["answer"]
            .as_str()
            .is_some_and(|text| !text.is_empty())
    });
    assert_eq!(partial["log"][2]["status"], "running");
    let stopped = delete(port, &format!("/v1/sessions/chat/{id}/turns"));
    assert_eq!(stopped.status, 200);
    let cancelled = until(port, id, |saved| saved["log"][2]["status"] != "running");
    assert_eq!(cancelled["log"][2]["status"], "cancelled", "{cancelled}");
    assert!(cancelled["log"][2]["answer"]
        .as_str()
        .unwrap()
        .starts_with(partial["log"][2]["answer"].as_str().unwrap()));
    send(
        port,
        id,
        "Write another long 1500-word continuation, no tools.",
    );
    until(port, id, |saved| {
        saved["log"][3]["answer"]
            .as_str()
            .is_some_and(|text| !text.is_empty())
    });
    daemon.interrupt();
    let daemon = Daemon::boot_operator(&binary, &root);
    daemon.await_ready();
    let recovered = record(port, id);
    assert_eq!(recovered["log"][3]["status"], "interrupted", "{recovered}");
    assert_eq!(recovered["log"][0], completed["log"][0]);
    assert_eq!(recovered["log"][1], followup["log"][1]);
    assert_eq!(recovered["log"][2], cancelled["log"][2]);
    assert_eq!(recovered["turns"], 4);
    eprintln!("PASS real-loader vendor: actual partials, contextual follow-up, transformed saved message, confirmed cancel with prefix, restart interruption; pin {pin}");
    daemon.interrupt();
}
