//! The workflows seam's real-composition gate (AGENTS.md standing order
//! 3): every proof boots the workflows profile — the two run stores OVER
//! the two Todo stores over the two session stores over the engine
//! providers, with the api trio, the settings pair and the cron seam —
//! through the REAL pinned `jinnd` daemon in the operator layout, and
//! drives it as an operator would: plain HTTP on loopback, with evidence
//! from the journals on disk.
//!
//! This is the FOUR-LAYER seam, and the composition is what most of the
//! proofs are about:
//!
//! - **A workflow node dispatches a Todo to a session that runs on an
//!   engine**, each hop reached by DEFINITION: `jinn:workflow.<store>`
//!   -> `jinn:todo.<store>` -> `jinn:session.<store>` -> `jinn:engine.<id>`.
//! - **The engine swaps** by one field of a node's binding, with all
//!   three stores untouched and none of them aware of which provider
//!   answered.
//! - **The run store swaps** by a profile edit, with the API and every
//!   layer below untouched.
//! - **Both run stores are live at once**, routed per run.
//! - **A third run store joins** a live daemon by profile edit alone.
//! - **The grant graph the four layers compose through is ACYCLIC**, and
//!   the profile is what proves it — which is why this seam needed no
//!   kernel-pin bump for M2-K10's cycle refusal.
//!
//! And the ones that are not about composition at all:
//!
//! - **THE PIN.** A definition edited mid-run does not change the run in
//!   flight, and the run REPORTS the revision it is executing.
//! - **An illegal node-state transition REFUSES**, typed, naming the node
//!   and the attempted `from -> to`, and the attempt is on the record.
//! - **A node in flight when the daemon is KILLED** comes back RECORDED
//!   interrupted with a reason, never eternally `running`.
//! - **History is append-only**, and a torn TAIL is absence rather than
//!   corruption.
//! - **A document holding NO complete record is the absence of the run**,
//!   answered `404` and never as a status — and the heal that clears it
//!   DROPS bytes without ever writing a record (`FINDINGS.md` #36).
//! - **What the fourth layer COSTS**, measured rather than derived —
//!   `FINDINGS.md` #35.
//!
//! Self-skips LOUDLY when no jinnd checkout holding the pinned commit is
//! reachable (KERNEL-PIN.md Gate 2).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use composition::api::{get, post};
use composition::daemon::{jinnd_source, pinned_commit, pinned_daemon};
use composition::kit::{artifact_hash, entry_mut, fresh_workflow_root, Daemon, SESSION_POLL_MS};

/// The switchable run-store slot's entry id and the store id it serves.
const DEFAULT_ID: &str = "jinn-workflow-default";
/// See [`DEFAULT_ID`].
const DEFAULT_STORE: &str = "default";
/// The coexistence half's store id. Its ENTRY id is not named here: no
/// proof edits that entry — the swap proof moves the switchable slot ONTO
/// its package instead, which is the edit an operator actually makes.
const MEMORY_STORE: &str = "memory";
/// The extension proof's entry — NOT in the base document.
const SCRATCH_ID: &str = "jinn-workflow-scratch";
/// See [`SCRATCH_ID`].
const SCRATCH_STORE: &str = "scratch";
/// The API entry, whose grants and settings the extension proof edits.
const API_ID: &str = "jinn-api-http";

/// The TODO store every node dispatches through — the second layer.
const TODO_STORE: &str = "default";
/// The SESSION store every Todo is dispatched to — the third layer.
const SESSION_STORE: &str = "default";
/// The engine every proof that is not about engines runs on.
const DEFAULT_ENGINE: &str = "default";
/// The SECOND engine, and a genuinely different provider shape — the echo
/// package driving a real child through `jinn:process`. A run on it stays
/// live for tens of seconds, which is what makes it both the "another
/// engine" proof and the mid-flight one.
const SPAWN_ENGINE: &str = "spawn";

/// The env var that turns the VENDOR leg on, and its only home. A vendor
/// CLI spends real inference under the operator's own authentication, so
/// that leg runs where a person asked for it by name and self-skips
/// everywhere else — CI included, exactly as the pinned-daemon gate
/// self-skips without a jinnd checkout. A skip is announced and proves
/// NOTHING; it never stands in for a run.
const VENDOR_GATE: &str = "JINN_HARNESS_WORKFLOW_VENDOR_ENGINE";
/// What every node in the vendor proof asks for: one line, from an engine
/// that is metered. The echo leg is asked the same thing, so the two legs
/// differ in the binding and in nothing else.
const VENDOR_PROMPT: &str = "Reply with exactly: OK";

/// The id of a run that never existed: the name on a document a daemon
/// was killed inside the first append of. Nothing ever minted it.
const ABSENT_RUN: &str = "default-r999";

/// The reason an interrupted node carries, from its one home.
const INTERRUPTED_NODE_REASON: &str = jinn_workflow::INTERRUPTED_NODE_REASON;

/// How long a run may take to settle before a proof fails. Generous: the
/// suite runs several daemons at once, and this seam polls THROUGH a seam
/// that polls THROUGH a seam that polls.
const RUN_DEADLINE: Duration = Duration::from_secs(180);

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

/// Boots a fresh workflows root and waits for readiness AND the API's
/// first answer.
fn booted(name: &str) -> Option<(Daemon, u16, PathBuf)> {
    let binary = gate()?;
    let (root, port) = fresh_workflow_root(name);
    let daemon = Daemon::boot_operator(binary, &root);
    daemon.await_ready();
    let health = get(port, "/v1/health");
    assert_eq!(health.status, 200, "{}", health.raw);
    Some((daemon, port, root))
}

/// Re-boots over an EXISTING root — the restart lane. Same profile, same
/// port, same journals.
fn reboot(root: &Path) -> Daemon {
    let binary = gate().expect("the gate held on the first boot");
    let daemon = Daemon::boot_operator(binary, root);
    daemon.await_ready();
    daemon
}

/// One `dispatch` node's spec: the whole of what makes this the fourth
/// layer. It names a TODO store and, through it, a session store and an
/// engine — every one of them a definition, none of them a provider.
fn dispatch_node(id: &str, engine: &str, message: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "kind": "dispatch",
        "title": id,
        "todo": {
            "store": TODO_STORE,
            "todo": { "title": id, "acceptance": "the node ends done" },
            "dispatch": {
                "store": SESSION_STORE,
                "engine": { "engine": engine },
                "message": message
            }
        }
    })
}

/// The one-node workflow every composition proof uses: a single dispatch
/// node, no edges. One node is deliberate — it makes a run's cost the
/// cost of ONE pass through all four layers, with no graph walk mixed in.
fn one_node_spec(name: &str, engine: &str) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "nodes": [dispatch_node("work", engine, "do the work")],
        "edges": [],
        "actor": "planner"
    })
}

/// Records a workflow (or a new revision of one) and answers
/// `(workflow-id, revision)`.
fn define(port: u16, store: &str, spec: &serde_json::Value) -> (String, u64) {
    let body = serde_json::json!({ "spec": spec });
    let defined = post(port, &format!("/v1/workflows/{store}"), &body);
    assert_eq!(defined.status, 200, "{}", defined.raw);
    (
        defined.body["workflow-id"]
            .as_str()
            .unwrap_or_else(|| panic!("a workflow id: {}", defined.raw))
            .to_owned(),
        defined.body["revision"]
            .as_u64()
            .unwrap_or_else(|| panic!("a revision: {}", defined.raw)),
    )
}

/// Records a NEW REVISION of an existing workflow.
fn redefine(port: u16, store: &str, workflow: &str, spec: &serde_json::Value) -> u64 {
    let body = serde_json::json!({ "spec": spec, "workflow-id": workflow });
    let defined = post(port, &format!("/v1/workflows/{store}"), &body);
    assert_eq!(defined.status, 200, "{}", defined.raw);
    defined.body["revision"]
        .as_u64()
        .unwrap_or_else(|| panic!("a revision: {}", defined.raw))
}

/// Opens a run and answers its id.
fn start(port: u16, store: &str, workflow: &str) -> String {
    let started = post(
        port,
        &format!("/v1/workflows/{store}/{workflow}/runs"),
        &serde_json::json!({ "actor": "planner" }),
    );
    assert_eq!(started.status, 200, "{}", started.raw);
    started.body["run-id"]
        .as_str()
        .unwrap_or_else(|| panic!("a run id: {}", started.raw))
        .to_owned()
}

/// One run's record.
fn run(port: u16, store: &str, run_id: &str) -> serde_json::Value {
    let read = get(port, &format!("/v1/workflows/{store}/runs/{run_id}"));
    assert_eq!(read.status, 200, "{}", read.raw);
    read.body
}

/// One node of a run's record.
fn node<'doc>(record: &'doc serde_json::Value, node_id: &str) -> &'doc serde_json::Value {
    record["nodes"]
        .as_array()
        .unwrap_or_else(|| panic!("nodes: {record}"))
        .iter()
        .find(|node| node["node-id"] == node_id)
        .unwrap_or_else(|| panic!("node {node_id:?}: {record}"))
}

/// Polls until a run reaches a terminal status, and answers the whole
/// record.
fn settled(daemon: &Daemon, port: u16, store: &str, run_id: &str) -> serde_json::Value {
    let deadline = Instant::now() + RUN_DEADLINE;
    loop {
        let read = run(port, store, run_id);
        match read["status"].as_str() {
            Some("running") | None => {}
            Some(_) => return read,
        }
        assert!(
            Instant::now() < deadline,
            "run {run_id} never settled\n--- daemon log ---\n{}",
            daemon.log()
        );
        std::thread::sleep(Duration::from_millis(120));
    }
}

/// One store's `describe`, from the store list.
fn described(port: u16, store: &str) -> serde_json::Value {
    let list = get(port, "/v1/workflows");
    assert_eq!(list.status, 200, "{}", list.raw);
    list.body["stores"]
        .as_array()
        .unwrap_or_else(|| panic!("a store list: {}", list.raw))
        .iter()
        .find(|entry| entry["store"] == store)
        .unwrap_or_else(|| panic!("store {store:?} in the list: {}", list.raw))
        .clone()
}

/// The durable store's journal for one run, as raw bytes.
fn run_journal(daemon: &Daemon, run_id: &str) -> Option<Vec<u8>> {
    std::fs::read(daemon.data(&format!("workflows/runs/{run_id}.jsonl"))).ok()
}

/// The `kind`s a journal document holds, in order.
fn kinds(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_slice::<serde_json::Value>(line).ok())
        .filter_map(|record| record["kind"].as_str().map(str::to_owned))
        .collect()
}

/// Asserts a journal document is WHOLE: every line decodes, and the last
/// byte is the terminator that makes a short write detectable.
fn assert_untorn(bytes: &[u8], what: &str) {
    assert!(!bytes.is_empty(), "{what}: an empty journal");
    assert_eq!(
        bytes.last().copied(),
        Some(b'\n'),
        "{what}: the document does not end on a line terminator, so its tail is torn"
    );
    for (index, line) in bytes.split(|byte| *byte == b'\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        serde_json::from_slice::<serde_json::Value>(line).unwrap_or_else(|error| {
            panic!(
                "{what}: journal line {} does not decode ({error}): {:?}",
                index + 1,
                String::from_utf8_lossy(line)
            )
        });
    }
}

// ---- the four-layer composition --------------------------------------

#[test]
fn a_workflow_node_dispatches_a_todo_to_a_session_that_runs_on_an_engine() {
    let Some((daemon, port, _)) = booted("workflows-four-layers") else {
        return;
    };
    let (workflow, revision) = define(
        port,
        DEFAULT_STORE,
        &one_node_spec("the four-layer lane", DEFAULT_ENGINE),
    );
    assert_eq!(revision, 1);
    let run_id = start(port, DEFAULT_STORE, &workflow);
    let settled = settled(&daemon, port, DEFAULT_STORE, &run_id);

    assert_eq!(settled["status"], "done", "{settled}");
    let work = node(&settled, "work");
    assert_eq!(work["state"], "done", "{settled}");
    // Each hop is VISIBLE in the record, and each is a definition: the
    // node names a Todo store, that Todo names a session store, and the
    // session names an engine. No layer named the next one's provider.
    assert_eq!(work["todo-store"], TODO_STORE, "{settled}");
    let todo_id = work["todo-id"]
        .as_str()
        .unwrap_or_else(|| panic!("the node bound a Todo: {settled}"));

    // The TODO the node dispatched is a real Todo in the layer below,
    // readable on its own surface.
    let todo = get(port, &format!("/v1/todos/{TODO_STORE}/{todo_id}"));
    assert_eq!(todo.status, 200, "{}", todo.raw);
    let dispatch = todo.body["dispatches"]
        .as_array()
        .and_then(|dispatches| dispatches.last())
        .unwrap_or_else(|| panic!("a dispatch: {}", todo.raw));
    assert_eq!(dispatch["status"], "done", "{}", todo.raw);
    assert_eq!(dispatch["session-store"], SESSION_STORE, "{}", todo.raw);
    assert_eq!(dispatch["engine"], DEFAULT_ENGINE, "{}", todo.raw);
    // And the SESSION that Todo drove is a real session in the layer
    // below that.
    let session_id = dispatch["session-id"]
        .as_str()
        .unwrap_or_else(|| panic!("a session: {}", todo.raw));
    let session = get(port, &format!("/v1/sessions/{SESSION_STORE}/{session_id}"));
    assert_eq!(session.status, 200, "{}", session.raw);

    // The run's journal holds the whole life, append-only and untorn.
    let document = run_journal(&daemon, &run_id).expect("a durable run journal");
    assert_untorn(&document, "the run");
    assert_eq!(
        kinds(&document),
        vec![
            "run-started",
            "node-state-changed",
            "node-state-changed",
            "run-ended"
        ],
        "the run's life, in lines"
    );
    daemon.interrupt();
}

#[test]
fn the_same_workflow_runs_over_another_engine_by_the_binding_alone() {
    let Some((daemon, port, _)) = booted("workflows-engine-swap") else {
        return;
    };
    // Two revisions of one workflow that differ in the ENGINE FIELD and
    // in nothing else — the same nodes, the same edges, the same message.
    let (workflow, _) = define(
        port,
        DEFAULT_STORE,
        &one_node_spec("the lane", DEFAULT_ENGINE),
    );
    let echo_run = start(port, DEFAULT_STORE, &workflow);
    let echo = settled(&daemon, port, DEFAULT_STORE, &echo_run);
    assert_eq!(echo["status"], "done", "{echo}");

    let second = redefine(
        port,
        DEFAULT_STORE,
        &workflow,
        &one_node_spec("the lane", SPAWN_ENGINE),
    );
    assert_eq!(second, 2);
    let spawn_run = start(port, DEFAULT_STORE, &workflow);
    let spawned = settled(&daemon, port, DEFAULT_STORE, &spawn_run);
    assert_eq!(spawned["status"], "done", "{spawned}");
    assert_eq!(spawned["definition-revision"], 2, "{spawned}");

    // Both ran the same procedure. What differed is one field, four
    // layers down — and neither run store, Todo store nor session store
    // named a provider to make it happen.
    for (record, engine) in [(&echo, DEFAULT_ENGINE), (&spawned, SPAWN_ENGINE)] {
        let todo_id = node(record, "work")["todo-id"]
            .as_str()
            .unwrap_or_else(|| panic!("a Todo: {record}"));
        let todo = get(port, &format!("/v1/todos/{TODO_STORE}/{todo_id}"));
        let dispatch = todo.body["dispatches"]
            .as_array()
            .and_then(|dispatches| dispatches.last())
            .unwrap_or_else(|| panic!("a dispatch: {}", todo.raw));
        assert_eq!(dispatch["engine"], engine, "{}", todo.raw);
        assert_eq!(dispatch["status"], "done", "{}", todo.raw);
    }
    daemon.interrupt();
}

// ---- the pin ---------------------------------------------------------

#[test]
fn a_definition_edited_mid_run_does_not_change_the_run_and_the_run_reports_its_revision() {
    let Some((daemon, port, _)) = booted("workflows-pin") else {
        return;
    };
    // A run whose node stays live for tens of seconds, so the edit lands
    // while the run is genuinely in flight rather than racing its answer.
    let (workflow, first) = define(
        port,
        DEFAULT_STORE,
        &one_node_spec("the pinned lane", SPAWN_ENGINE),
    );
    assert_eq!(first, 1);
    let run_id = start(port, DEFAULT_STORE, &workflow);
    daemon.eventually("the run's node to be in flight", || {
        node(&run(port, DEFAULT_STORE, &run_id), "work")["state"] == "running"
    });

    // THE EDIT, mid-flight: a second node, a different name, a different
    // engine. A new revision — never a replacement of the one the run is
    // executing.
    let edited = serde_json::json!({
        "name": "the pinned lane, revised",
        "nodes": [
            dispatch_node("work", DEFAULT_ENGINE, "do it differently"),
            serde_json::json!({ "id": "audit", "kind": "checkpoint", "title": "audit" })
        ],
        "edges": [serde_json::json!({ "from": "work", "to": "audit", "kind": "always" })],
        "actor": "planner"
    });
    let second = redefine(port, DEFAULT_STORE, &workflow, &edited);
    assert_eq!(second, 2);

    // The run in flight never learns. It reports revision 1, carries
    // revision 1's nodes, and finishes revision 1's procedure.
    let live = run(port, DEFAULT_STORE, &run_id);
    assert_eq!(live["definition-revision"], 1, "{live}");
    assert_eq!(live["spec"]["name"], "the pinned lane", "{live}");
    assert_eq!(live["nodes"].as_array().expect("nodes").len(), 1, "{live}");
    let finished = settled(&daemon, port, DEFAULT_STORE, &run_id);
    assert_eq!(finished["definition-revision"], 1, "{finished}");
    assert_eq!(
        finished["nodes"].as_array().expect("nodes").len(),
        1,
        "{finished}"
    );
    assert_eq!(finished["status"], "done", "{finished}");
    let todo_id = node(&finished, "work")["todo-id"].as_str().expect("a Todo");
    let todo = get(port, &format!("/v1/todos/{TODO_STORE}/{todo_id}"));
    let dispatch = todo.body["dispatches"]
        .as_array()
        .and_then(|dispatches| dispatches.last())
        .expect("a dispatch");
    assert_eq!(
        dispatch["engine"], SPAWN_ENGINE,
        "the edit reached a run that had already pinned its revision: {}",
        todo.raw
    );

    // A run started NOW executes the edit, because "latest" is resolved
    // once, at start — and it says which one it resolved to.
    let after = start(port, DEFAULT_STORE, &workflow);
    let after = settled(&daemon, port, DEFAULT_STORE, &after);
    assert_eq!(after["definition-revision"], 2, "{after}");
    assert_eq!(
        after["nodes"].as_array().expect("nodes").len(),
        2,
        "{after}"
    );
    assert_eq!(after["status"], "done", "{after}");

    // And revision 1 is still exactly what it was: a revision is never
    // replaced.
    let read = get(
        port,
        &format!("/v1/workflows/{DEFAULT_STORE}/{workflow}?revision=1"),
    );
    assert_eq!(read.status, 200, "{}", read.raw);
    assert_eq!(
        read.body["revisions"]
            .as_array()
            .and_then(|revisions| revisions.iter().find(|rev| rev["revision"] == 1))
            .map(|rev| rev["spec"]["name"].clone()),
        Some(serde_json::json!("the pinned lane")),
        "{}",
        read.raw
    );
    daemon.interrupt();
}

// ---- ledger honesty --------------------------------------------------

#[test]
fn an_illegal_node_transition_is_refused_typed_and_ledgered() {
    let Some((daemon, port, _)) = booted("workflows-refusal") else {
        return;
    };
    // A workflow whose entry node is a checkpoint gated behind nothing,
    // and a second node that will not start until the first ends — so
    // there is a node genuinely standing at `pending` to aim at.
    let spec = serde_json::json!({
        "name": "the gated lane",
        "nodes": [
            serde_json::json!({ "id": "open", "kind": "checkpoint" }),
            dispatch_node("work", SPAWN_ENGINE, "stay live")
        ],
        "edges": [serde_json::json!({ "from": "open", "to": "work", "kind": "on-done" })],
        "actor": "planner"
    });
    let (workflow, _) = define(port, DEFAULT_STORE, &spec);
    let run_id = start(port, DEFAULT_STORE, &workflow);
    daemon.eventually("the second node to be in flight", || {
        node(&run(port, DEFAULT_STORE, &run_id), "work")["state"] == "running"
    });

    // `running -> pending` is not in the table: a node does not un-start.
    let refused = post(
        port,
        &format!("/v1/workflows/{DEFAULT_STORE}/runs/{run_id}/nodes/work/state"),
        &serde_json::json!({ "state": "pending", "actor": "planner" }),
    );
    assert_ne!(
        refused.status, 200,
        "an illegal move is refused: {}",
        refused.raw
    );
    assert_eq!(
        refused.body["error"]["store-code"], "refused",
        "{}",
        refused.raw
    );
    // The refusal names the attempt as DATA, not only as prose — a caller
    // classifies on `from`/`to` rather than parsing a message.
    assert_eq!(refused.body["error"]["node"], "work", "{}", refused.raw);
    assert_eq!(refused.body["error"]["from"], "running", "{}", refused.raw);
    assert_eq!(refused.body["error"]["to"], "pending", "{}", refused.raw);
    // The API's error document names the prose `detail`; the DATA above
    // is what a caller classifies on, and the prose is what an operator
    // reads.
    assert!(
        refused.body["error"]["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("running -> pending")),
        "{}",
        refused.raw
    );

    // And the attempt is ON THE RECORD, in the run and in the journal —
    // an operator reading the ledger sees the refused move even if the
    // caller dropped the answer on the floor.
    daemon.eventually("the refusal to reach the record", || {
        run(port, DEFAULT_STORE, &run_id)["refused"]
            .as_array()
            .is_some_and(|refused| !refused.is_empty())
    });
    let record = run(port, DEFAULT_STORE, &run_id);
    let attempt = &record["refused"][0];
    assert_eq!(attempt["node-id"], "work", "{record}");
    assert_eq!(attempt["from"], "running", "{record}");
    assert_eq!(attempt["to"], "pending", "{record}");
    // Nothing moved.
    assert_eq!(node(&record, "work")["state"], "running", "{record}");
    let document = run_journal(&daemon, &run_id).expect("a journal");
    assert!(
        kinds(&document).contains(&"node-transition-refused".to_owned()),
        "the refusal is a line: {:?}",
        kinds(&document)
    );
    daemon.interrupt();
}

#[test]
fn a_node_in_flight_when_the_daemon_dies_comes_back_interrupted_with_a_reason() {
    let Some((daemon, port, root)) = booted("workflows-restart-honesty") else {
        return;
    };
    // A finished run first, so the restart has to tell the two apart: the
    // honest answer is not "everything is interrupted".
    let (finished_workflow, _) = define(
        port,
        DEFAULT_STORE,
        &one_node_spec("work that completed", DEFAULT_ENGINE),
    );
    let finished = start(port, DEFAULT_STORE, &finished_workflow);
    settled(&daemon, port, DEFAULT_STORE, &finished);

    // Now one genuinely in flight: the child-backed engine's run lives
    // for tens of seconds, so the kill lands mid-node.
    let (live_workflow, _) = define(
        port,
        DEFAULT_STORE,
        &one_node_spec("work the daemon died on", SPAWN_ENGINE),
    );
    let live = start(port, DEFAULT_STORE, &live_workflow);
    daemon.eventually("the node to be in flight", || {
        node(&run(port, DEFAULT_STORE, &live), "work")["state"] == "running"
    });
    // The journal already holds the started node — that ordering is what
    // the whole proof rests on, and it is checked BEFORE the kill so a
    // failure here is not confused with one after it.
    let document = run_journal(&daemon, &live).expect("a journal for a started node");
    assert_untorn(&document, "the live run, before the kill");
    assert_eq!(
        kinds(&document),
        vec!["run-started", "node-state-changed"],
        "started, not ended"
    );

    // The CRASH path: SIGKILL, no chance to write anything on the way out.
    daemon.kill();
    let daemon = reboot(&root);

    let recovered = run(port, DEFAULT_STORE, &live);
    // The NODE is recorded interrupted, with a reason — never eternally
    // running.
    let work = node(&recovered, "work");
    assert_eq!(
        work["state"], "interrupted",
        "a node the daemon died on is interrupted, never eternally running: {recovered}"
    );
    assert_eq!(work["reason"], INTERRUPTED_NODE_REASON, "{recovered}");
    // And the RUN itself ended, with a reason of its own.
    assert_eq!(recovered["status"], "interrupted", "{recovered}");
    assert!(
        recovered["reason"]
            .as_str()
            .is_some_and(|reason| !reason.trim().is_empty()),
        "an ending nobody can explain: {recovered}"
    );

    // The recovery is RECORDED, not merely derived, and it is a NEW line
    // appended after the ones already there: the move that started the
    // work is still readable exactly as it was written.
    let history = recovered["history"].as_array().expect("a history");
    assert_eq!(history.len(), 2, "{recovered}");
    assert_eq!(history[0]["to"], "running", "{recovered}");
    assert_eq!(history[1]["from"], "running", "{recovered}");
    assert_eq!(history[1]["to"], "interrupted", "{recovered}");
    assert_eq!(history[1]["note"], INTERRUPTED_NODE_REASON, "{recovered}");
    // Nobody asked for the recovery, and the record says so rather than
    // naming a principal that did not act.
    assert!(history[1].get("actor").is_none(), "{recovered}");

    let after_crash = run_journal(&daemon, &live).expect("the journal survived");
    assert_untorn(&after_crash, "the live run, after the crash");
    assert!(
        after_crash.len() >= document.len(),
        "an append-only log does not shorten"
    );
    assert_eq!(
        kinds(&after_crash),
        vec![
            "run-started",
            "node-state-changed",
            "node-state-changed",
            "run-ended"
        ],
        "the recovery is two lines, appended after the node they explain"
    );

    // A run that DID finish is still done — the restart is not a blanket
    // verdict.
    let survived = run(port, DEFAULT_STORE, &finished);
    assert_eq!(survived["status"], "done", "{survived}");
    assert_eq!(node(&survived, "work")["state"], "done", "{survived}");

    // And the store serves normally afterwards: a fresh run of the same
    // workflow runs to done, so the recovery left a usable ledger rather
    // than a wedged one.
    let again = start(port, DEFAULT_STORE, &finished_workflow);
    let again = settled(&daemon, port, DEFAULT_STORE, &again);
    assert_eq!(again["status"], "done", "{again}");
    daemon.interrupt();
}

#[test]
fn a_torn_tail_is_absence_and_the_run_before_it_survives() {
    let Some((daemon, port, root)) = booted("workflows-torn-tail") else {
        return;
    };
    let (workflow, _) = define(
        port,
        DEFAULT_STORE,
        &one_node_spec("a run whose tail is torn", DEFAULT_ENGINE),
    );
    let run_id = start(port, DEFAULT_STORE, &workflow);
    settled(&daemon, port, DEFAULT_STORE, &run_id);
    let path = daemon.data(&format!("workflows/runs/{run_id}.jsonl"));
    let whole = std::fs::read(&path).expect("a journal");
    assert_untorn(&whole, "before the tear");

    // The tear is MANUFACTURED, behind the daemon's back: `jinn:fs`'s
    // append is whole-document atomic (`FINDINGS.md` #22), so this proves
    // the READER's behaviour on a torn document, not that the kernel
    // tears. The honest limit is stated rather than implied.
    daemon.kill();
    let mut torn = whole.clone();
    torn.extend_from_slice(br#"{"kind":"node-state-changed","at-ms":9,"from":"runn"#);
    std::fs::write(&path, &torn).expect("write the torn journal");
    let daemon = reboot(&root);

    // The run before the torn line survives, and the store SAYS it healed
    // rather than discarding bytes in silence.
    let recovered = run(port, DEFAULT_STORE, &run_id);
    assert_eq!(recovered["workflow-id"], workflow, "{recovered}");
    assert_eq!(recovered["definition-revision"], 1, "{recovered}");
    assert!(
        described(port, DEFAULT_STORE)["describe"]["extra"]["healed-tails"]
            .as_u64()
            .is_some_and(|healed| healed > 0),
        "a store that drops bytes says so: {}",
        described(port, DEFAULT_STORE)
    );
    // And the healed document is appendable again: the next move lands as
    // a record rather than fusing with the tear (`FINDINGS.md` #34).
    let next = start(port, DEFAULT_STORE, &workflow);
    let next = settled(&daemon, port, DEFAULT_STORE, &next);
    assert_eq!(next["status"], "done", "{next}");
    let after = std::fs::read(&path).expect("the healed journal");
    assert_untorn(&after, "after the heal");
    daemon.interrupt();
}

/// Lays a record-less run document into a live store's directory and
/// re-boots over it.
///
/// What a daemon killed INSIDE its very first append leaves behind: bytes
/// that were never a record. Manufactured behind the daemon's back for
/// the same reason the torn-tail proof is — `jinn:fs` writes whole
/// documents (`FINDINGS.md` #22) — so what these proofs test is the
/// READER, not that the kernel tears.
///
/// Answers the re-booted daemon, the document's path, and the id of a run
/// that really happened, so each proof can show the real run is untouched.
fn booted_over_a_record_less_document(name: &str) -> Option<(Daemon, u16, String, String)> {
    let (daemon, port, root) = booted(name)?;
    let (workflow, _) = define(
        port,
        DEFAULT_STORE,
        &one_node_spec("a run that really happened", DEFAULT_ENGINE),
    );
    let run_id = start(port, DEFAULT_STORE, &workflow);
    settled(&daemon, port, DEFAULT_STORE, &run_id);

    let path = daemon.data(&format!("workflows/runs/{ABSENT_RUN}.jsonl"));
    daemon.kill();
    std::fs::write(&path, b"{").expect("write the record-less journal");
    let daemon = reboot(&root);
    Some((daemon, port, path.to_string_lossy().into_owned(), run_id))
}

#[test]
fn a_run_document_holding_no_record_reads_as_absence_and_never_as_a_run() {
    let Some((daemon, port, _, run_id)) = booted_over_a_record_less_document("workflows-no-record")
    else {
        return;
    };

    // A run is a POSITIVE reading. Without one complete `run-started`
    // record there is nothing to report, and the answer that must never
    // come back is a status: this route once returned 200 with
    // `status: "done"`, `workflow-id: ""` and revision 0 — a completed
    // run fabricated out of one byte (`FINDINGS.md` #36).
    let read = get(
        port,
        &format!("/v1/workflows/{DEFAULT_STORE}/runs/{ABSENT_RUN}"),
    );
    assert_eq!(
        read.status, 404,
        "one byte that was never a record answered as a run: {}",
        read.raw
    );

    // It is absent from the LIST for the same reason, so no reader meets
    // it by another door.
    let list = get(port, &format!("/v1/workflows/{DEFAULT_STORE}/runs"));
    assert_eq!(list.status, 200, "{}", list.raw);
    assert!(
        !list.raw.contains(ABSENT_RUN),
        "the run that never was, listed: {}",
        list.raw
    );

    // The store SAW the document and declined to make a run of it, and
    // says so — evidence of absence, not absence of evidence.
    let described = described(port, DEFAULT_STORE);
    assert!(
        described["describe"]["extra"]["documents-without-a-record"]
            .as_u64()
            .is_some_and(|seen| seen >= 1),
        "a store that discards a whole document says so: {described}"
    );

    // The run that really happened is untouched by any of it.
    assert_eq!(run(port, DEFAULT_STORE, &run_id)["status"], "done");
    daemon.interrupt();
}

#[test]
fn a_heal_drops_incomplete_bytes_and_never_writes_a_record() {
    // The SECOND fault, proven on its own terms and without asking the
    // API anything: boot turned one torn byte into a lone `run-ended`
    // line. A heal may DROP bytes that were never a record; it may not
    // create, complete or infer one.
    let Some((daemon, _port, path, run_id)) =
        booted_over_a_record_less_document("workflows-no-heal")
    else {
        return;
    };

    // No bytes at all: the document that held no record is DROPPED whole
    // rather than trimmed to nothing, so there is no name left for a
    // later writer to append onto. Either reading — gone, or present and
    // empty — is "no bytes"; only bytes would be a failure.
    let after = std::fs::read(&path).unwrap_or_default();
    assert_eq!(
        kinds(&after),
        Vec::<String>::new(),
        "boot wrote a record into a document that held none: {:?}",
        String::from_utf8_lossy(&after)
    );
    assert!(
        after.is_empty(),
        "the incomplete bytes survived the heal: {:?}",
        String::from_utf8_lossy(&after)
    );

    // And the real run's document is whole and unedited — the heal
    // touched the document that needed it and nothing else.
    let real = run_journal(&daemon, &run_id).expect("the real run's journal");
    assert_untorn(&real, "the run that really happened");
    assert!(
        kinds(&real).contains(&"run-started".to_owned()),
        "the real run lost its own opening line"
    );
    daemon.interrupt();
}

// ---- swap, coexistence, extension ------------------------------------

#[test]
fn both_run_stores_are_live_at_once_and_a_run_is_routed_by_its_store() {
    let Some((daemon, port, _)) = booted("workflows-coexist") else {
        return;
    };
    let spec = one_node_spec("the same procedure, two ledgers", DEFAULT_ENGINE);
    let (durable_workflow, _) = define(port, DEFAULT_STORE, &spec);
    let (ephemeral_workflow, _) = define(port, MEMORY_STORE, &spec);
    let durable = start(port, DEFAULT_STORE, &durable_workflow);
    let ephemeral = start(port, MEMORY_STORE, &ephemeral_workflow);
    assert_eq!(
        settled(&daemon, port, DEFAULT_STORE, &durable)["status"],
        "done"
    );
    assert_eq!(
        settled(&daemon, port, MEMORY_STORE, &ephemeral)["status"],
        "done"
    );

    // Two stores, two ledgers, routed apart: a run of one is not in the
    // other, and only the durable one is on disk.
    let crossed = get(
        port,
        &format!("/v1/workflows/{MEMORY_STORE}/runs/{durable}"),
    );
    assert_eq!(crossed.status, 404, "{}", crossed.raw);
    assert_eq!(described(port, DEFAULT_STORE)["describe"]["durable"], true);
    assert_eq!(described(port, MEMORY_STORE)["describe"]["durable"], false);
    assert!(run_journal(&daemon, &durable).is_some());
    assert!(run_journal(&daemon, &ephemeral).is_none());
    daemon.interrupt();
}

#[test]
fn the_run_store_swaps_by_a_profile_edit_with_every_layer_below_untouched() {
    let Some((daemon, port, root)) = booted("workflows-store-swap") else {
        return;
    };
    let spec = one_node_spec("the lane", DEFAULT_ENGINE);
    let (before_workflow, _) = define(port, DEFAULT_STORE, &spec);
    let before = start(port, DEFAULT_STORE, &before_workflow);
    settled(&daemon, port, DEFAULT_STORE, &before);
    assert_eq!(described(port, DEFAULT_STORE)["describe"]["durable"], true);
    assert!(run_journal(&daemon, &before).is_some());

    // The swap: ONE entry's package and hash. The API entry, the TODO
    // stores, the SESSION stores, the engine entries and the store id are
    // not touched — so the contract name stays `jinn:workflow.default`
    // and every consumer keeps its grant.
    let ephemeral = artifact_hash(&root, "jinn-workflow-memory");
    daemon.edit_profile_restarting(DEFAULT_ID, |document| {
        let entry = entry_mut(document, DEFAULT_ID);
        entry["package"] = serde_json::json!("workflows/jinn-workflow-memory");
        entry["hash"] = serde_json::json!(ephemeral);
        let grants = entry["config"]["grants"].as_array_mut().expect("grants");
        grants.retain(|grant| grant["contract"] != "jinn:fs");
        entry["config"]["data"]
            .as_object_mut()
            .expect("data")
            .remove("dir");
    });

    daemon.eventually("the swapped store to declare itself ephemeral", || {
        described(port, DEFAULT_STORE)["describe"]["durable"] == serde_json::json!(false)
    });
    let (after_workflow, _) = define(port, DEFAULT_STORE, &spec);
    let after = start(port, DEFAULT_STORE, &after_workflow);
    let settled = settled(&daemon, port, DEFAULT_STORE, &after);
    assert_eq!(settled["status"], "done", "{settled}");
    assert!(
        run_journal(&daemon, &after).is_none(),
        "the swapped-in store writes nothing"
    );
    // Every layer BELOW is untouched: the same Todo store, the same
    // session store and the same engine answered before and after,
    // through a run ledger that changed underneath them.
    assert_eq!(
        node(&settled, "work")["todo-store"],
        TODO_STORE,
        "{settled}"
    );
    assert_eq!(get(port, "/v1/todos").status, 200);
    assert_eq!(get(port, "/v1/sessions").status, 200);
    assert_eq!(get(port, "/v1/engines").status, 200);
    daemon.interrupt();
}

#[test]
fn a_third_run_store_joins_a_live_daemon_by_a_profile_edit_alone() {
    let Some((daemon, port, root)) = booted("workflows-extension") else {
        return;
    };
    // Not here yet, and refused by the API without a kernel call.
    let missing = get(port, &format!("/v1/workflows/{SCRATCH_STORE}"));
    assert_eq!(missing.status, 404, "{}", missing.raw);

    let ephemeral = artifact_hash(&root, "jinn-workflow-memory");
    let todo_grant = serde_json::json!(format!("jinn:todo.{TODO_STORE}"));
    daemon.edit_profile(|document| {
        // The new store: its own contract name, its own entry, no change
        // to the definition and no new artifact.
        document["entries"]
            .as_array_mut()
            .expect("entries")
            .push(serde_json::json!({
                "id": SCRATCH_ID,
                "package": "workflows/jinn-workflow-memory",
                "hash": ephemeral,
                "config": {
                    "grants": [
                        format!("jinn:workflow.{SCRATCH_STORE}"),
                        jinn_workflow::EVENT_TOPIC,
                        "jinn:clock",
                        todo_grant
                    ],
                    "data": { "store": SCRATCH_STORE, "poll-ms": SESSION_POLL_MS }
                }
            }));
        // The API may route to it only because the profile SAYS so.
        let api = entry_mut(document, API_ID);
        api["config"]["grants"]
            .as_array_mut()
            .expect("grants")
            .push(serde_json::json!(format!("jinn:workflow.{SCRATCH_STORE}")));
        api["config"]["data"]["workflow-stores"]
            .as_array_mut()
            .expect("workflow-stores")
            .push(serde_json::json!(SCRATCH_STORE));
    });

    daemon.eventually("the third store to answer", || {
        get(port, &format!("/v1/workflows/{SCRATCH_STORE}")).status == 200
    });
    let (workflow, _) = define(
        port,
        SCRATCH_STORE,
        &one_node_spec("work in the scratch ledger", DEFAULT_ENGINE),
    );
    let run_id = start(port, SCRATCH_STORE, &workflow);
    let settled = settled(&daemon, port, SCRATCH_STORE, &run_id);
    assert_eq!(settled["status"], "done", "{settled}");
    assert_eq!(settled["store"], SCRATCH_STORE, "{settled}");
    // The stores it joined are still there and still routed apart.
    assert_eq!(described(port, DEFAULT_STORE)["describe"]["durable"], true);
    assert_eq!(described(port, MEMORY_STORE)["describe"]["durable"], false);
    daemon.interrupt();
}

#[test]
fn an_ephemeral_store_keeps_nothing_across_a_restart_and_says_so() {
    let Some((daemon, port, root)) = booted("workflows-ephemeral") else {
        return;
    };
    let (workflow, _) = define(
        port,
        MEMORY_STORE,
        &one_node_spec("a throwaway lane", DEFAULT_ENGINE),
    );
    let run_id = start(port, MEMORY_STORE, &workflow);
    assert_eq!(
        settled(&daemon, port, MEMORY_STORE, &run_id)["status"],
        "done"
    );
    assert_eq!(described(port, MEMORY_STORE)["describe"]["durable"], false);

    daemon.interrupt();
    let daemon = reboot(&root);
    // `durable: false` is a promise, and this is it kept: a successor
    // starts empty rather than carrying state that would make the swap
    // proof a lie.
    let gone = get(port, &format!("/v1/workflows/{MEMORY_STORE}/runs/{run_id}"));
    assert_eq!(gone.status, 404, "{}", gone.raw);
    let gone = get(port, &format!("/v1/workflows/{MEMORY_STORE}/{workflow}"));
    assert_eq!(gone.status, 404, "{}", gone.raw);
    daemon.interrupt();
}

// ---- why this seam needed no pin bump --------------------------------

#[test]
fn the_grant_graph_the_four_layers_compose_through_is_acyclic() {
    // The kernel-pin decision for this packet, as EVIDENCE rather than as
    // an assurance. jinnd M2-K10 makes a reply-expecting dispatch that
    // would close a cycle refuse, typed and ledgered. This seam does not
    // need it, and the reason is checkable in the profile the kit
    // generates: a call is only possible where a GRANT allows it, so the
    // grant graph bounds the dispatch graph. If the grant graph is
    // acyclic, no dispatch in this composition can close a cycle.
    //
    // The proof reads the profile rather than the daemon, so it holds
    // without the pinned-daemon gate and cannot be skipped quietly.
    let (root, _) = fresh_workflow_root("workflows-acyclic");
    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("profile.json")).expect("profile"))
            .expect("profile parses");
    let entries = document["entries"].as_array().expect("entries");

    // Which entry PROVIDES each contract name. A bare-string grant on a
    // seam contract is BOTH the authority to provide it and the authority
    // to call it, so the grant alone does not say which — the entry's
    // PACKAGE does. A `workflows/...` package serves `jinn:workflow.<id>`
    // and calls `jinn:todo.<id>`; reading the grant without the package
    // would make every store look like the provider of everything it may
    // reach, and would report a cycle that is not there.
    let seam_of = |package: &str| -> Option<&'static str> {
        match package.split('/').next()? {
            "workflows" => Some("jinn:workflow."),
            "todos" => Some("jinn:todo."),
            "sessions" => Some("jinn:session."),
            "engines" => Some("jinn:engine."),
            _ => None,
        }
    };
    let mut provider: BTreeMap<String, String> = BTreeMap::new();
    for entry in entries {
        let id = entry["id"].as_str().expect("an id").to_owned();
        let Some(prefix) = entry["package"].as_str().and_then(seam_of) else {
            continue;
        };
        for grant in entry["config"]["grants"].as_array().into_iter().flatten() {
            let Some(name) = grant.as_str() else { continue };
            if name.starts_with(prefix) {
                provider.insert(name.to_owned(), id.clone());
            }
        }
    }
    // Every layer of the stack is represented, so the walk below is over
    // the whole composition and not over a fragment of it.
    for prefix in [
        "jinn:workflow.",
        "jinn:todo.",
        "jinn:session.",
        "jinn:engine.",
    ] {
        assert!(
            provider.keys().any(|name| name.starts_with(prefix)),
            "the profile mounts no {prefix}<id> provider: {provider:?}"
        );
    }

    // The call graph: entry -> the entries it may call, through the
    // contracts it was granted.
    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for entry in entries {
        let id = entry["id"].as_str().expect("an id").to_owned();
        let outbound = edges.entry(id.clone()).or_default();
        for grant in entry["config"]["grants"].as_array().into_iter().flatten() {
            let Some(name) = grant.as_str() else { continue };
            if let Some(target) = provider.get(name) {
                if target != &id {
                    outbound.insert(target.clone());
                }
            }
        }
    }

    // Kahn's algorithm over the call graph. A cycle would leave nodes
    // that can never be removed, and the failure NAMES them.
    let mut remaining: BTreeSet<String> = edges.keys().cloned().collect();
    loop {
        let ready: Vec<String> = remaining
            .iter()
            .filter(|id| edges[*id].iter().all(|target| !remaining.contains(target)))
            .cloned()
            .collect();
        if ready.is_empty() {
            break;
        }
        for id in ready {
            remaining.remove(&id);
        }
        if remaining.is_empty() {
            break;
        }
    }
    assert!(
        remaining.is_empty(),
        "these entries can reach each other in a cycle, so a dispatch in this composition \
         COULD close one and the pin owes M2-K10: {remaining:?}"
    );

    // And the specific layering claim, positively: a run store holds no
    // authority to reach a session or an engine at all, so the four
    // layers cannot be short-circuited even by a store that tried.
    let store = entries
        .iter()
        .find(|entry| entry["id"] == DEFAULT_ID)
        .expect("the switchable run store");
    let grants: Vec<&str> = store["config"]["grants"]
        .as_array()
        .expect("grants")
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    assert!(
        !grants.iter().any(|name| name.starts_with("jinn:session.")),
        "the session is the Todo's business: {grants:?}"
    );
    assert!(
        !grants.iter().any(|name| name.starts_with("jinn:engine.")),
        "the engine is the session's business: {grants:?}"
    );
    assert!(
        grants.iter().any(|name| name.starts_with("jinn:todo.")),
        "{grants:?}"
    );
}

// ---- what the fourth layer costs (FINDINGS #35) ----------------------

/// How many times each depth is measured. Small: every repetition is a
/// real end-to-end run through the whole stack, and the entry is about a
/// TERM that is either there or is not, not about a distribution.
const LATENCY_SAMPLES: usize = 5;

/// How often the measurement polls the API. Well under the store poll
/// period, so the observer is not the dominant term — and stated rather
/// than left for a reader to assume.
const OBSERVE_MS: u64 = 15;

/// Polls `read` until it answers `Some`, and answers how long that took.
fn time_until(daemon: &Daemon, what: &str, mut read: impl FnMut() -> bool) -> Duration {
    let started = Instant::now();
    let deadline = started + RUN_DEADLINE;
    loop {
        if read() {
            return started.elapsed();
        }
        assert!(
            Instant::now() < deadline,
            "{what} never settled\n--- daemon log ---\n{}",
            daemon.log()
        );
        std::thread::sleep(Duration::from_millis(OBSERVE_MS));
    }
}

fn median(mut samples: Vec<u128>) -> u128 {
    samples.sort_unstable();
    samples[samples.len() / 2]
}

#[test]
fn dispatch_latency_at_two_three_and_four_layers() {
    // `FINDINGS.md` #35 said the additive term is structural and graded
    // itself *derived, not measured*. This is the measurement it asked
    // for, taken from ONE daemon so the three depths differ in the number
    // of layers and in nothing else: the same engine, the same poll
    // period, the same machine, the same moment.
    //
    // Two layers:  jinn:session.<s> -> jinn:engine.<e>
    // Three:       jinn:todo.<t>    -> the above
    // Four:        jinn:workflow.<w> -> the above
    //
    // Each depth's workflow is ONE dispatch node with no edges, so a run
    // is one pass through the stack and no graph walk is mixed in.
    let Some((daemon, port, _)) = booted("workflows-latency") else {
        return;
    };
    let (workflow, _) = define(
        port,
        DEFAULT_STORE,
        &one_node_spec("the measured lane", DEFAULT_ENGINE),
    );

    let mut two = Vec::new();
    let mut three = Vec::new();
    let mut four = Vec::new();
    for _ in 0..LATENCY_SAMPLES {
        // TWO layers: a session driving an engine.
        let created = post(
            port,
            &format!("/v1/sessions/{SESSION_STORE}"),
            &serde_json::json!({ "engine": { "engine": DEFAULT_ENGINE } }),
        );
        assert_eq!(created.status, 200, "{}", created.raw);
        let session = created.body["session-id"]
            .as_str()
            .expect("a session")
            .to_owned();
        let accepted = post(
            port,
            &format!("/v1/sessions/{SESSION_STORE}/{session}/turns"),
            &serde_json::json!({ "message": "do the work" }),
        );
        assert_eq!(accepted.status, 200, "{}", accepted.raw);
        let turn = accepted.body["turn-id"]
            .as_str()
            .expect("a turn")
            .to_owned();
        two.push(
            time_until(&daemon, "the turn", || {
                get(port, &format!("/v1/sessions/{SESSION_STORE}/{session}")).body["log"]
                    .as_array()
                    .and_then(|log| log.iter().find(|entry| entry["turn-id"] == turn).cloned())
                    .and_then(|entry| entry["status"].as_str().map(str::to_owned))
                    .is_some_and(|status| status != "running")
            })
            .as_millis(),
        );

        // THREE layers: a Todo driving that session.
        let todo = post(
            port,
            &format!("/v1/todos/{TODO_STORE}"),
            &serde_json::json!({ "title": "the measured Todo", "actor": "planner" }),
        );
        assert_eq!(todo.status, 200, "{}", todo.raw);
        let todo_id = todo.body["todo-id"].as_str().expect("a Todo").to_owned();
        let sent = post(
            port,
            &format!("/v1/todos/{TODO_STORE}/{todo_id}/dispatch"),
            &serde_json::json!({
                "store": SESSION_STORE,
                "engine": { "engine": DEFAULT_ENGINE },
                "message": "do the work",
                "actor": "planner"
            }),
        );
        assert_eq!(sent.status, 200, "{}", sent.raw);
        three.push(
            time_until(&daemon, "the dispatch", || {
                get(port, &format!("/v1/todos/{TODO_STORE}/{todo_id}")).body["dispatches"]
                    .as_array()
                    .and_then(|dispatches| dispatches.last().cloned())
                    .and_then(|dispatch| dispatch["status"].as_str().map(str::to_owned))
                    .is_some_and(|status| status != "running")
            })
            .as_millis(),
        );

        // FOUR layers: a workflow node driving that Todo.
        let run_id = start(port, DEFAULT_STORE, &workflow);
        four.push(
            time_until(&daemon, "the run", || {
                get(
                    port,
                    &format!("/v1/workflows/{DEFAULT_STORE}/runs/{run_id}"),
                )
                .body["status"]
                    .as_str()
                    .is_some_and(|status| status != "running")
            })
            .as_millis(),
        );
    }

    let (m2, m3, m4) = (
        median(two.clone()),
        median(three.clone()),
        median(four.clone()),
    );
    println!(
        "FINDINGS #35, measured at poll-ms={SESSION_POLL_MS} per store layer \
         (observer polls every {OBSERVE_MS} ms, {LATENCY_SAMPLES} samples, one daemon):\n\
         \x20 2 layers (session -> engine)              median {m2} ms  samples {two:?}\n\
         \x20 3 layers (todo -> session -> engine)      median {m3} ms  samples {three:?}\n\
         \x20 4 layers (workflow -> todo -> ...)        median {m4} ms  samples {four:?}\n\
         \x20 per-layer term: 3-2 = {} ms, 4-3 = {} ms (the additive model predicts \
         one poll period, {SESSION_POLL_MS} ms, for each)",
        m3 as i128 - m2 as i128,
        m4 as i128 - m3 as i128,
    );

    // The assertion is deliberately WEAK and structural. This test exists
    // to produce numbers for a findings entry, and a threshold on wall
    // time across a loaded machine would be a flaky red that says nothing
    // about the seam. What IS asserted is the direction the entry claims:
    // each layer costs something, and never nothing.
    assert!(
        m3 > m2,
        "three layers did not cost more than two: {m3} vs {m2}"
    );
    assert!(
        m4 > m3,
        "four layers did not cost more than three: {m4} vs {m3}"
    );
    daemon.interrupt();
}

// ---- the vendor leg --------------------------------------------------

#[test]
fn the_same_run_runs_over_a_vendor_engine_when_the_operator_names_one() {
    let Ok(engine) = std::env::var(VENDOR_GATE) else {
        eprintln!(
            "SKIPPED (loudly): the vendor leg spends metered inference under the operator's \
             own authentication, so it runs only where a person names an engine in \
             {VENDOR_GATE}. A skip proves nothing and is never a pass."
        );
        return;
    };
    let Some((daemon, port, _)) = booted("workflows-vendor") else {
        return;
    };
    // An engine that is NAMED and not mounted FAILS the proof rather than
    // skipping it — a gate that quietly passes on a typo proves nothing.
    let engines = get(port, "/v1/engines");
    assert_eq!(engines.status, 200, "{}", engines.raw);
    assert!(
        engines.body["engines"]
            .as_array()
            .is_some_and(|mounted| mounted.iter().any(|entry| entry["engine"] == engine)),
        "{VENDOR_GATE}={engine} names an engine this composition does not mount: {}",
        engines.raw
    );

    // The two legs differ in the ENGINE BINDING and in nothing else.
    let (workflow, _) = define(
        port,
        DEFAULT_STORE,
        &serde_json::json!({
            "name": "the vendor lane",
            "nodes": [dispatch_node("work", DEFAULT_ENGINE, VENDOR_PROMPT)],
            "edges": [],
            "actor": "planner"
        }),
    );
    let echo = start(port, DEFAULT_STORE, &workflow);
    assert_eq!(
        settled(&daemon, port, DEFAULT_STORE, &echo)["status"],
        "done"
    );

    redefine(
        port,
        DEFAULT_STORE,
        &workflow,
        &serde_json::json!({
            "name": "the vendor lane",
            "nodes": [dispatch_node("work", &engine, VENDOR_PROMPT)],
            "edges": [],
            "actor": "planner"
        }),
    );
    let vendor = start(port, DEFAULT_STORE, &workflow);
    let settled = settled(&daemon, port, DEFAULT_STORE, &vendor);
    assert_eq!(settled["status"], "done", "{settled}");
    let work = node(&settled, "work");
    assert_eq!(work["state"], "done", "{settled}");
    assert!(
        work["answer"]
            .as_str()
            .is_some_and(|answer| !answer.trim().is_empty()),
        "a vendor engine that answered nothing is not a proof: {settled}"
    );
    daemon.interrupt();
}

/// The id this store mints after `minted` — the name a daemon killed
/// inside its very next `start` would leave a document under.
fn next_id(minted: &str) -> String {
    let (prefix, number) = minted.rsplit_once('r').expect("a run id ends in r<number>");
    let number: u64 = number.parse().expect("a numeric run id");
    format!("{prefix}r{}", number + 1)
}

#[test]
fn the_id_of_a_record_less_document_is_never_handed_to_a_new_run() {
    // `a_heal_drops_incomplete_bytes_and_never_writes_a_record` proves the
    // BYTES half at an id this store would not mint for a very long time.
    // This is the ID half, at the id it mints NEXT — the one a daemon
    // killed inside `start`'s first append actually leaves behind, and the
    // one whose reuse turned an accepted absence into corruption one seam
    // down (`FINDINGS.md` #36).
    let Some((daemon, port, root)) = booted("workflows-record-less-id") else {
        return;
    };
    let (workflow, _) = define(
        port,
        DEFAULT_STORE,
        &one_node_spec("a run that really happened", DEFAULT_ENGINE),
    );
    let real = start(port, DEFAULT_STORE, &workflow);
    settled(&daemon, port, DEFAULT_STORE, &real);

    let absent = next_id(&real);
    let path = daemon.data(&format!("workflows/runs/{absent}.jsonl"));
    daemon.kill();
    std::fs::write(&path, b"{").expect("write the record-less journal");
    let daemon = reboot(&root);

    let after = std::fs::read(&path).unwrap_or_default();
    assert!(
        after.is_empty(),
        "the incomplete bytes survived the boot: {:?}",
        String::from_utf8_lossy(&after)
    );
    let fresh = start(port, DEFAULT_STORE, &workflow);
    assert_ne!(
        fresh, absent,
        "a new run was handed the record-less document's id"
    );
    settled(&daemon, port, DEFAULT_STORE, &fresh);
    let document = run_journal(&daemon, &fresh).expect("the new run's journal");
    assert_untorn(&document, "the run started after the absence");

    // The boot after is where a fused line would show up as a replay that
    // refuses and a store that fails to activate.
    daemon.kill();
    let daemon = reboot(&root);
    assert_eq!(run(port, DEFAULT_STORE, &fresh)["status"], "done");
    assert_eq!(run(port, DEFAULT_STORE, &real)["status"], "done");
    daemon.interrupt();
}
