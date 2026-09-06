//! Metered UI-4a route proof using this head's real ui-kit composition.
use composition::api::{delete, get, post};
use composition::daemon::{jinnd_source, pinned_commit, pinned_daemon};
use composition::kit::Daemon;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn read(port: u16, path: &str) -> Value {
    let response = get(port, path);
    assert_eq!(response.status, 200, "{}", response.raw);
    response.body
}

// Track the worker from this entry's kernel ledger, then include its actual
// descendants. Launcher exit alone previously produced a false Stop pass.
fn owned_tree(daemon: &Daemon) -> Vec<u32> {
    let pid = daemon
        .ledger_rows()
        .iter()
        .rev()
        .find_map(|row| {
            let (kind, fields) = row.kind_of();
            (row.entry.as_deref() == Some("chat-codex") && kind == "ProcessSpawned")
                .then(|| fields["pid"].as_u64().unwrap() as u32)
        })
        .expect("the native worker was spawned");
    let rows = process_parents();
    assert!(
        rows.iter().any(|(child, _)| *child == pid),
        "worker must be live before Stop"
    );
    let mut owned = vec![pid];
    loop {
        let before = owned.len();
        for (child, parent) in &rows {
            if owned.contains(parent) && !owned.contains(child) {
                owned.push(*child);
            }
        }
        if owned.len() == before {
            break;
        }
    }
    eprintln!("actual owned worker tree: {owned:?}");
    owned
}

fn process_parents() -> Vec<(u32, u32)> {
    let output = std::process::Command::new("ps")
        .args(["-axo", "pid=,ppid="])
        .output()
        .expect("process table");
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| {
            let mut fields = line.split_whitespace();
            (
                fields.next().unwrap().parse().unwrap(),
                fields.next().unwrap().parse().unwrap(),
            )
        })
        .collect()
}

fn assert_reaped(owned: &[u32]) {
    let rows = process_parents();
    assert!(
        !rows.iter().any(|(pid, _)| owned.contains(pid)),
        "confirmed Stop must leave no native worker/descendant: {owned:?}"
    );
    eprintln!("confirmed absent from process table: {owned:?}");
}

#[test]
fn actual_todo_context_to_astra_result_and_conditional_operator_review() {
    let Ok(root) = std::env::var("JINN_TODO_JOURNEY_ROOT") else {
        eprintln!("SKIPPED (loudly): JINN_TODO_JOURNEY_ROOT must name this head's rebuilt ui-kit root for the actual Todo vendor proof");
        return;
    };
    let root = PathBuf::from(root);
    let profile: Value =
        serde_json::from_slice(&std::fs::read(root.join("profile.json")).unwrap()).unwrap();
    let entries = profile["entries"].as_array().unwrap();
    let port = entries.iter().find(|e| e["id"] == "jinn-api-http").unwrap()["config"]["data"]
        ["port"]
        .as_u64()
        .unwrap() as u16;
    assert!(
        !composition::api::listening(port),
        "owned port must be unused"
    );
    assert!(entries
        .iter()
        .any(|e| e["id"] == "work-todos" && e["package"] == "todos/jinn-todo-fs"));
    assert!(entries
        .iter()
        .any(|e| e["id"] == "task-sessions" && e["package"] == "sessions/jinn-session-fs"));
    let pin = pinned_commit().unwrap();
    let binary = pinned_daemon(&jinnd_source(&pin).expect("exact pin required"), &pin).unwrap();
    let daemon = Daemon::boot_operator(&binary, &root);
    daemon.await_ready();
    assert_eq!(get(port, "/").status, 200, "actual embedded UI artifact");
    let created = post(
        port,
        "/v1/todos/work",
        &json!({"title":"Draft a project update", "body":"Write one short sentence using all the acceptance and context facts.", "acceptance":"Include the exact word SUNFLOWER."}),
    );
    assert_eq!(created.status, 200, "{}", created.raw);
    let id = created.body["todo-id"].as_str().unwrap();
    let path = format!("/v1/todos/work/{id}");
    let before = read(port, &path);
    let comment = post(
        port,
        &format!("{path}/comments"),
        &json!({"body":"The deadline is Friday. Include Friday in the sentence.", "expected-revision":before["revision"],"actor":"operator"}),
    );
    assert_eq!(comment.status, 200, "{}", comment.raw);
    let stale = post(
        port,
        &format!("{path}/comments"),
        &json!({"body":"A stale duplicate", "expected-revision":before["revision"]}),
    );
    assert_eq!(stale.status, 502, "{}", stale.raw);
    assert_eq!(read(port, &path)["comments"].as_array().unwrap().len(), 1);
    let dispatch = json!({"dispatch":{"store":"tasks","engine":{"engine":"codex","model":"gpt-6-astra","effort":"high"}}, "expected-revision":comment.body["revision"],"actor":"operator"});
    let opened = post(port, &format!("{path}/dispatch"), &dispatch);
    assert_eq!(opened.status, 200, "{}", opened.raw);
    let duplicate = post(port, &format!("{path}/dispatch"), &dispatch);
    assert_eq!(
        duplicate.status, 502,
        "duplicate/stale must not execute: {}",
        duplicate.raw
    );
    let start = Instant::now();
    let done = loop {
        let record = read(port, &path);
        if record["dispatches"][0]["status"] != "running" {
            break record;
        }
        assert!(
            start.elapsed() < Duration::from_secs(130),
            "bounded task did not settle: {record}"
        );
        std::thread::sleep(Duration::from_millis(250));
    };
    let result = &done["dispatches"][0];
    assert_eq!(result["status"], "done", "{done}");
    assert_eq!(
        done["status"], "executing",
        "model completion is not acceptance"
    );
    let answer = result["answer"].as_str().unwrap();
    assert!(
        answer.contains("SUNFLOWER") && answer.contains("Friday"),
        "answer must use captured acceptance and added context: {answer}"
    );
    let session_id = result["session-id"].as_str().unwrap();
    let session = read(port, &format!("/v1/sessions/tasks/{session_id}"));
    assert_eq!(session["model"], "gpt-6-astra", "{session}");
    assert_eq!(session["engine"], "codex");
    let journal = std::fs::read_to_string(
        root.join("task-history")
            .join(format!("{session_id}.jsonl")),
    )
    .unwrap();
    let opened_session: Value = serde_json::from_str(journal.lines().next().unwrap()).unwrap();
    assert_eq!(opened_session["spec"]["engine"]["effort"], "high");
    assert_eq!(opened_session["spec"]["tools"]["mode"], "denied");
    let prompt = session["log"][0]["message"].as_str().unwrap();
    assert!(prompt.contains("SUNFLOWER") && prompt.contains("Friday"));
    assert!(prompt.len() <= jinn_todo::dispatch::PROMPT_BYTES);
    eprintln!(
        "ACTUAL submitted prompt ({} UTF-8 bytes):\n{prompt}\nACTUAL Astra/high answer: {answer}",
        prompt.len()
    );
    let missing_review = post(
        port,
        &format!("{path}/status"),
        &json!({"status":"in-review","expected-revision":done["revision"]}),
    );
    assert_eq!(missing_review.status, 502);
    let reviewed = post(
        port,
        &format!("{path}/status"),
        &json!({"status":"in-review","expected-revision":done["revision"],"reviewed-dispatch":result["dispatch-id"],"actor":"operator"}),
    );
    assert_eq!(reviewed.status, 200, "{}", reviewed.raw);
    let closed = post(
        port,
        &format!("{path}/status"),
        &json!({"status":"done","expected-revision":reviewed.body["revision"],"reviewed-dispatch":result["dispatch-id"],"actor":"operator"}),
    );
    assert_eq!(closed.status, 200, "{}", closed.raw);
    assert_eq!(
        closed.body["history"].as_array().unwrap().last().unwrap()["reviewed-dispatch"],
        result["dispatch-id"]
    );
    daemon.interrupt();
    let restarted = Daemon::boot_operator(&binary, &root);
    restarted.await_ready();
    assert_eq!(
        read(port, &path),
        closed.body,
        "completed result/review/revision survive restart"
    );
    task_stop_and_busy(&restarted, port);
    oversized_prompt(port);
    let active = open_long_task(port);
    let active_path = format!("/v1/todos/work/{}", active["todo-id"].as_str().unwrap());
    let active_revision = active["revision"].as_u64().unwrap();
    let owned = owned_tree(&restarted);
    restarted.interrupt();
    assert_reaped(&owned);
    let recovered = Daemon::boot_operator(&binary, &root);
    recovered.await_ready();
    let interrupted = read(port, &active_path);
    let last = interrupted["dispatches"]
        .as_array()
        .unwrap()
        .last()
        .unwrap();
    assert_eq!(interrupted["status"], "blocked");
    assert_eq!(last["status"], "interrupted");
    assert!(
        last.get("session-id").is_none(),
        "active link recovery is not provided at this scope"
    );
    assert_eq!(last["answer"], "", "never invent partial answer recovery");
    assert!(
        interrupted["revision"].as_u64().unwrap() > active_revision,
        "recovery advances durable revision"
    );
    recovered.interrupt();
    eprintln!("PASS actual Todo → durable task session → codex/gpt-6-astra/high denied-tool result → conditional operator submit/accept; exact pin {pin}");
}

fn open_long_task(port: u16) -> Value {
    let created = post(
        port,
        "/v1/todos/work",
        &json!({"title":"Write a long story", "body":"Write an original story of at least 6000 words. Start with the story, and keep writing until the ending."}),
    );
    assert_eq!(created.status, 200, "{}", created.raw);
    let path = format!(
        "/v1/todos/work/{}",
        created.body["todo-id"].as_str().unwrap()
    );
    let before = read(port, &path);
    let opened = post(
        port,
        &format!("{path}/dispatch"),
        &json!({"dispatch":{"store":"tasks","engine":{"engine":"codex","model":"gpt-6-astra","effort":"high"}}, "expected-revision":before["revision"]}),
    );
    assert_eq!(opened.status, 200, "{}", opened.raw);
    opened.body
}

fn task_stop_and_busy(daemon: &Daemon, port: u16) {
    let active = open_long_task(port);
    let latest = active["dispatches"].as_array().unwrap().last().unwrap();
    let session = latest["session-id"].as_str().unwrap();
    let path = format!("/v1/todos/work/{}", active["todo-id"].as_str().unwrap());
    let owned = owned_tree(daemon);
    let busy = post(
        port,
        "/v1/todos/work",
        &json!({"title":"A concurrent task"}),
    );
    let busy_path = format!("/v1/todos/work/{}", busy.body["todo-id"].as_str().unwrap());
    let busy_record = read(port, &busy_path);
    let refused = post(
        port,
        &format!("{busy_path}/dispatch"),
        &json!({"dispatch":{"store":"tasks","engine":{"engine":"codex","model":"gpt-6-astra","effort":"high"}},"expected-revision":busy_record["revision"]}),
    );
    assert_eq!(refused.status, 502, "{}", refused.raw);
    let failed = read(port, &busy_path);
    assert_eq!(failed["dispatches"][0]["status"], "failed");
    assert!(failed["dispatches"][0]["reason"]
        .as_str()
        .unwrap()
        .contains("already running"));
    let stopped = delete(port, &format!("/v1/sessions/tasks/{session}/turns"));
    assert_eq!(stopped.status, 200, "{}", stopped.raw);
    // DELETE acknowledges the request; the saved terminal outcome confirms it.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let record = read(port, &path);
        if record["dispatches"][0]["status"] != "running" {
            assert_eq!(record["dispatches"][0]["status"], "cancelled");
            assert_eq!(
                record["status"], "executing",
                "Stop does not cancel the Todo"
            );
            break;
        }
        assert!(
            Instant::now() < deadline,
            "cancelled session must reach durable dispatch ending"
        );
        std::thread::sleep(Duration::from_millis(250));
    }
    assert_reaped(&owned);
    eprintln!("PASS actual Todo Stop: native worker/descendants absent; busy second task refused with saved failure; no automatic rerun");
}

fn oversized_prompt(port: u16) {
    let created = post(
        port,
        "/v1/todos/work",
        &json!({"title":"Oversized", "body":"界".repeat(11000)}),
    );
    assert_eq!(created.status, 200, "{}", created.raw);
    let path = format!(
        "/v1/todos/work/{}",
        created.body["todo-id"].as_str().unwrap()
    );
    let before = read(port, &path);
    let refused = post(
        port,
        &format!("{path}/dispatch"),
        &json!({"dispatch":{"store":"tasks","engine":{"engine":"codex","model":"gpt-6-astra","effort":"high"}},"expected-revision":before["revision"]}),
    );
    assert_eq!(refused.status, 502, "{}", refused.raw);
    assert!(refused.raw.contains("32 KiB"));
    assert_eq!(
        read(port, &path),
        before,
        "oversize cannot change status, revision or dispatch history"
    );
}
