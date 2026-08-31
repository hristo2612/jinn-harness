//! The todos seam's real-composition gate (AGENTS.md standing order 3):
//! every proof boots the todos profile — the two Todo stores OVER the two
//! session stores over the engine providers, with the api trio, the
//! settings pair and the cron seam — through the REAL pinned `jinnd`
//! daemon in the operator layout, and drives it as an operator would:
//! plain HTTP on loopback, with evidence from the journals on disk.
//!
//! This is the THREE-LAYER seam, and the composition is what most of the
//! proofs are about:
//!
//! - **A Todo dispatched to a session runs on an engine**, with each hop
//!   reached by DEFINITION: `jinn:todo.<store>` -> `jinn:session.<store>`
//!   -> `jinn:engine.<id>`.
//! - **The engine swaps** by one field of a dispatch, with both stores
//!   untouched and neither aware of which provider answered.
//! - **The Todo store swaps** by a profile edit, with the API, the
//!   sessions seam and the engines untouched.
//! - **Both Todo stores are live at once**, routed per Todo.
//! - **A third store joins** a live daemon by profile edit alone.
//!
//! And the ones that are not about composition at all — the LEDGER
//! HONESTY this seam owes:
//!
//! - **An illegal status transition REFUSES**, typed, naming the
//!   attempted `from -> to`, and the attempt is on the record.
//! - **A dispatch in flight when the daemon is KILLED** comes back
//!   recorded `interrupted` WITH A REASON, and the Todo reads `blocked`
//!   rather than eternally `executing` — while its declared history is
//!   not rewritten.
//! - **History is append-only**, and a torn TAIL is absence rather than
//!   corruption.
//!
//! Self-skips LOUDLY when no jinnd checkout holding the pinned commit is
//! reachable (KERNEL-PIN.md Gate 2).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use composition::api::{get, post};
use composition::daemon::{jinnd_source, pinned_commit, pinned_daemon};
use composition::kit::{artifact_hash, entry_mut, fresh_todo_root, Daemon, ExtraDaemonLoad};

/// The switchable Todo store slot's entry id and the store id it serves.
const DEFAULT_ID: &str = "jinn-todo-default";
/// See [`DEFAULT_ID`].
const DEFAULT_STORE: &str = "default";
/// The coexistence half's store id. Its ENTRY id is not named here: no
/// proof edits that entry — the swap proof moves the switchable slot ONTO
/// its package instead, which is the edit an operator actually makes.
const MEMORY_STORE: &str = "memory";
/// The extension proof's entry — NOT in the base document.
const SCRATCH_ID: &str = "jinn-todo-scratch";
/// See [`SCRATCH_ID`].
const SCRATCH_STORE: &str = "scratch";
/// The API entry, whose grants and settings the extension proof edits.
const API_ID: &str = "jinn-api-http";

/// The SESSION store every dispatch is sent to — the middle layer.
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
const VENDOR_GATE: &str = "JINN_HARNESS_TODO_VENDOR_ENGINE";
/// What every dispatch in the vendor proof asks for: one line, from an
/// engine that is metered. The echo leg is asked the same thing, so the
/// two legs differ in the binding and in nothing else.
const VENDOR_PROMPT: &str = "Reply with exactly: OK";

/// The reason an interrupted dispatch carries, from its one home.
const INTERRUPTED_REASON: &str = jinn_todo::journal::INTERRUPTED_REASON;
/// The reason a Todo blocked by one carries, from its one home.
const BLOCKED_REASON: &str = jinn_todo::INTERRUPTED_STATUS_REASON;

/// How long a dispatch may take to settle before a proof fails. Generous:
/// the suite runs several daemons at once, and this seam polls THROUGH a
/// seam that polls.
const DISPATCH_DEADLINE: Duration = Duration::from_secs(120);

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

/// Boots a fresh todos root and waits for readiness AND the API's first
/// answer.
fn booted(name: &str) -> Option<(Daemon, u16, PathBuf)> {
    let binary = gate()?;
    let (root, port) = fresh_todo_root(name);
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

/// Records one Todo and answers its id.
fn create(port: u16, store: &str, title: &str) -> String {
    let created = post(
        port,
        &format!("/v1/todos/{store}"),
        &serde_json::json!({ "title": title, "acceptance": "it is done", "actor": "planner" }),
    );
    assert_eq!(created.status, 200, "{}", created.raw);
    created.body["todo-id"]
        .as_str()
        .unwrap_or_else(|| panic!("a Todo id: {}", created.raw))
        .to_owned()
}

/// One Todo's record.
fn record(port: u16, store: &str, todo: &str) -> serde_json::Value {
    let read = get(port, &format!("/v1/todos/{store}/{todo}"));
    assert_eq!(read.status, 200, "{}", read.raw);
    read.body
}

/// Moves one Todo's status; answers the raw response so a proof can read
/// a REFUSAL as well as a move.
fn update(
    port: u16,
    store: &str,
    todo: &str,
    status: &str,
    actor: &str,
) -> composition::api::Response {
    post(
        port,
        &format!("/v1/todos/{store}/{todo}/status"),
        &serde_json::json!({ "status": status, "actor": actor }),
    )
}

/// Dispatches one Todo to a session on `engine`; answers the record.
fn dispatch(port: u16, store: &str, todo: &str, engine: &str) -> serde_json::Value {
    let sent = post(
        port,
        &format!("/v1/todos/{store}/{todo}/dispatch"),
        &serde_json::json!({
            "store": SESSION_STORE,
            "engine": { "engine": engine },
            "actor": "planner"
        }),
    );
    assert_eq!(sent.status, 200, "{}", sent.raw);
    sent.body
}

/// Dispatches one Todo with an explicit message, so two legs can differ
/// in the engine binding and in nothing else.
fn dispatch_saying(
    port: u16,
    store: &str,
    todo: &str,
    engine: &str,
    message: &str,
) -> composition::api::Response {
    post(
        port,
        &format!("/v1/todos/{store}/{todo}/dispatch"),
        &serde_json::json!({
            "store": SESSION_STORE,
            "engine": { "engine": engine },
            "message": message,
            "actor": "planner"
        }),
    )
}

/// The last dispatch of a Todo record.
fn last_dispatch(record: &serde_json::Value) -> serde_json::Value {
    record["dispatches"]
        .as_array()
        .and_then(|dispatches| dispatches.last())
        .cloned()
        .unwrap_or_else(|| panic!("a dispatch: {record}"))
}

/// Polls until a Todo's last dispatch reaches a terminal status, and
/// answers the whole record.
fn settled(daemon: &Daemon, port: u16, store: &str, todo: &str) -> serde_json::Value {
    let deadline = Instant::now() + DISPATCH_DEADLINE;
    loop {
        let read = record(port, store, todo);
        match last_dispatch(&read)["status"].as_str() {
            Some("running") | None => {}
            Some(_) => return read,
        }
        assert!(
            Instant::now() < deadline,
            "the dispatch of {todo} never settled\n--- daemon log ---\n{}",
            daemon.log()
        );
        std::thread::sleep(Duration::from_millis(150));
    }
}

/// One store's `describe`, from the store list.
fn described(port: u16, store: &str) -> serde_json::Value {
    let list = get(port, "/v1/todos");
    assert_eq!(list.status, 200, "{}", list.raw);
    list.body["stores"]
        .as_array()
        .unwrap_or_else(|| panic!("a store list: {}", list.raw))
        .iter()
        .find(|entry| entry["store"] == store)
        .unwrap_or_else(|| panic!("store {store:?} in the list: {}", list.raw))
        .clone()
}

/// The durable store's journal for one Todo, as raw bytes.
fn journal(daemon: &Daemon, todo: &str) -> Option<Vec<u8>> {
    std::fs::read(daemon.data(&format!("todos/{todo}.jsonl"))).ok()
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

#[test]
fn a_todo_dispatched_to_a_session_runs_over_the_engine_it_named() {
    let Some((daemon, port, _root)) = booted("todos-three-layer") else {
        return;
    };
    let todo = create(port, DEFAULT_STORE, "port the ledger");
    // Recorded, and it opens where every Todo opens.
    let opened = record(port, DEFAULT_STORE, &todo);
    assert_eq!(opened["status"], "backlog");
    assert_eq!(opened["declared-status"], "backlog");

    // The dispatch: three layers, each hop reached by DEFINITION.
    let sent = dispatch(port, DEFAULT_STORE, &todo, DEFAULT_ENGINE);
    assert_eq!(
        sent["status"], "executing",
        "a dispatch IS the work starting: {sent}"
    );
    let settled = settled(&daemon, port, DEFAULT_STORE, &todo);
    let landed = last_dispatch(&settled);
    assert_eq!(landed["status"], "done", "{settled}");
    assert_eq!(landed["session-store"], SESSION_STORE);
    assert_eq!(landed["engine"], DEFAULT_ENGINE);
    // `done` claims the work was carried out and the answer is whole:
    // the echo engine answers with the prompt, so the Todo's own title
    // came back through the session and the engine both.
    let answer = landed["answer"].as_str().unwrap_or_default();
    assert!(
        answer.contains("port the ledger"),
        "the engine's answer reached the Todo through the session: {settled}"
    );

    // The MIDDLE layer is really there: the session the dispatch opened
    // is readable on the sessions surface, and knows the Todo it serves.
    let session_id = landed["session-id"].as_str().expect("a session id");
    let session = get(port, &format!("/v1/sessions/{SESSION_STORE}/{session_id}"));
    assert_eq!(session.status, 200, "{}", session.raw);
    assert_eq!(session.body["engine"], DEFAULT_ENGINE);
    assert_eq!(session.body["metadata"]["todo-id"], todo.as_str());

    // And the durable store wrote it down, whole and in order.
    let document = journal(&daemon, &todo).expect("the durable store wrote a journal");
    assert_untorn(&document, "the settled Todo");
    assert_eq!(
        kinds(&document),
        vec![
            "created",
            "status-changed",
            "dispatch-started",
            "dispatch-ended"
        ],
        "the dispatch is recorded STARTED before it is recorded ended"
    );
    daemon.interrupt();
}

#[test]
fn the_same_todo_store_dispatches_over_another_engine_by_the_binding_alone() {
    let Some((daemon, port, _root)) = booted("todos-engine-swap") else {
        return;
    };
    // Two Todos in the SAME store, dispatched to the SAME session store,
    // differing in exactly one field: the engine binding. Neither store's
    // package nor its config moves.
    let echoed = create(port, DEFAULT_STORE, "over the echo engine");
    let spawned = create(port, DEFAULT_STORE, "over the child-backed engine");
    dispatch(port, DEFAULT_STORE, &echoed, DEFAULT_ENGINE);
    let settled = settled(&daemon, port, DEFAULT_STORE, &echoed);
    assert_eq!(last_dispatch(&settled)["status"], "done", "{settled}");

    // The second engine is a genuinely different provider shape — it
    // spawns a real child through `jinn:process`. Neither the Todo store
    // nor the session store knows that, and neither needs a change.
    dispatch(port, DEFAULT_STORE, &spawned, SPAWN_ENGINE);
    daemon.eventually("the child-backed dispatch to be in flight", || {
        last_dispatch(&record(port, DEFAULT_STORE, &spawned))["status"] == "running"
    });
    let live = record(port, DEFAULT_STORE, &spawned);
    assert_eq!(last_dispatch(&live)["engine"], SPAWN_ENGINE);
    // The engine each Todo ran on is recorded PER DISPATCH, so the two
    // are told apart by their binding and not by their store.
    assert_eq!(last_dispatch(&settled)["engine"], DEFAULT_ENGINE);
    // A second dispatch while one is in flight is refused, not queued.
    let again = post(
        port,
        &format!("/v1/todos/{DEFAULT_STORE}/{spawned}/dispatch"),
        &serde_json::json!({ "store": SESSION_STORE, "engine": { "engine": DEFAULT_ENGINE } }),
    );
    assert_ne!(again.status, 200, "{}", again.raw);
    assert_eq!(again.body["error"]["code"], "refused", "{}", again.raw);
    assert!(
        again.body["error"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("in flight"),
        "{}",
        again.raw
    );
    daemon.interrupt();
}

#[test]
fn an_illegal_status_transition_is_refused_typed_and_ledgered() {
    let Some((daemon, port, _root)) = booted("todos-illegal-transition") else {
        return;
    };
    let todo = create(port, DEFAULT_STORE, "port the ledger");
    let moved = update(port, DEFAULT_STORE, &todo, "executing", "the producer");
    assert_eq!(moved.status, 200, "{}", moved.raw);

    // A producer closing their own work: `executing -> done` is not a
    // move this ledger makes.
    let refused = update(port, DEFAULT_STORE, &todo, "done", "the producer");
    assert_ne!(
        refused.status, 200,
        "an illegal move is refused: {}",
        refused.raw
    );
    // TYPED, and naming the attempt as DATA — not only in the prose.
    assert_eq!(
        refused.body["error"]["from"], "executing",
        "{}",
        refused.raw
    );
    assert_eq!(refused.body["error"]["to"], "done", "{}", refused.raw);
    assert_eq!(refused.body["error"]["code"], "refused", "{}", refused.raw);
    let detail = refused.body["error"]["detail"].as_str().unwrap_or_default();
    assert!(detail.contains("executing -> done"), "{}", refused.raw);

    // NOT silently accepted, and NOT coerced to a neighbouring status.
    let after = record(port, DEFAULT_STORE, &todo);
    assert_eq!(after["status"], "executing", "{after}");
    assert_eq!(after["declared-status"], "executing", "{after}");

    // LEDGERED: the attempt is on the Todo's record, and in its journal,
    // and on its event feed.
    assert_eq!(after["refused"][0]["from"], "executing", "{after}");
    assert_eq!(after["refused"][0]["to"], "done", "{after}");
    assert_eq!(after["refused"][0]["actor"], "the producer", "{after}");
    let document = journal(&daemon, &todo).expect("a journal");
    assert_untorn(&document, "the Todo whose move was refused");
    assert!(
        kinds(&document).contains(&"transition-refused".to_owned()),
        "the refusal is durable: {:?}",
        kinds(&document)
    );
    daemon.eventually("the refusal to reach the feed", || {
        let feed = get(
            port,
            &format!("/v1/todos/{DEFAULT_STORE}/{todo}/events?limit=100"),
        );
        feed.body["events"]
            .as_array()
            .is_some_and(|events| events.iter().any(|e| e["kind"] == "transition-refused"))
    });

    // And the LEGAL route to done still works, so the table is a route
    // and not a wall.
    assert_eq!(
        update(port, DEFAULT_STORE, &todo, "in-review", "the producer").status,
        200
    );
    let closed = update(port, DEFAULT_STORE, &todo, "done", "the reviewer");
    assert_eq!(closed.status, 200, "{}", closed.raw);
    let done = record(port, DEFAULT_STORE, &todo);
    assert_eq!(done["status"], "done");
    // History is APPEND-ONLY: three moves, in order, none rewritten.
    let history = done["history"].as_array().expect("a history");
    assert_eq!(history.len(), 3, "{done}");
    assert_eq!(history[0]["to"], "executing");
    assert_eq!(history[2]["actor"], "the reviewer");
    // A terminal Todo is terminal.
    let reopened = update(port, DEFAULT_STORE, &todo, "executing", "the producer");
    assert_ne!(reopened.status, 200, "{}", reopened.raw);
    daemon.interrupt();
}

#[test]
fn both_stores_are_live_at_once_and_a_todo_is_routed_by_its_store() {
    let Some((daemon, port, _root)) = booted("todos-coexist") else {
        return;
    };
    let durable = create(port, DEFAULT_STORE, "durable work");
    let ephemeral = create(port, MEMORY_STORE, "throwaway work");
    // Each store answers for its OWN Todos and knows nothing of the
    // other's — the routing is the store id in the path, and the kernel's
    // one-provider-per-contract-name slot behind it.
    assert_eq!(
        record(port, DEFAULT_STORE, &durable)["store"],
        DEFAULT_STORE
    );
    assert_eq!(
        record(port, MEMORY_STORE, &ephemeral)["store"],
        MEMORY_STORE
    );
    let crossed = get(port, &format!("/v1/todos/{MEMORY_STORE}/{durable}"));
    assert_eq!(crossed.status, 404, "{}", crossed.raw);
    // Their durability declarations differ, and so does what is on disk.
    assert_eq!(described(port, DEFAULT_STORE)["describe"]["durable"], true);
    assert_eq!(described(port, MEMORY_STORE)["describe"]["durable"], false);
    dispatch(port, MEMORY_STORE, &ephemeral, DEFAULT_ENGINE);
    settled(&daemon, port, MEMORY_STORE, &ephemeral);
    assert!(
        journal(&daemon, &ephemeral).is_none(),
        "the ephemeral store wrote nothing, which is its whole contract"
    );
    assert!(journal(&daemon, &durable).is_some());
    daemon.interrupt();
}

#[test]
fn the_store_swaps_by_a_profile_edit_with_every_layer_below_untouched() {
    let Some((daemon, port, root)) = booted("todos-store-swap") else {
        return;
    };
    let before = create(port, DEFAULT_STORE, "before the swap");
    dispatch(port, DEFAULT_STORE, &before, DEFAULT_ENGINE);
    settled(&daemon, port, DEFAULT_STORE, &before);
    assert_eq!(described(port, DEFAULT_STORE)["describe"]["durable"], true);
    assert!(journal(&daemon, &before).is_some());

    // The swap: ONE entry's package and hash. The API entry, the SESSION
    // stores, the engine entries and the store id are not touched — so
    // the contract name stays `jinn:todo.default` and every consumer
    // keeps its grant.
    let ephemeral = artifact_hash(&root, "jinn-todo-memory");
    daemon.edit_profile_restarting(DEFAULT_ID, |document| {
        let entry = entry_mut(document, DEFAULT_ID);
        entry["package"] = serde_json::json!("todos/jinn-todo-memory");
        entry["hash"] = serde_json::json!(ephemeral);
        // The ephemeral store reads no `dir`; leaving the grant would be
        // authority it has no use for.
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
    let after = create(port, DEFAULT_STORE, "after the swap");
    dispatch(port, DEFAULT_STORE, &after, DEFAULT_ENGINE);
    let settled = settled(&daemon, port, DEFAULT_STORE, &after);
    assert_eq!(last_dispatch(&settled)["status"], "done", "{settled}");
    assert!(
        journal(&daemon, &after).is_none(),
        "the swapped-in store writes nothing"
    );
    // Every layer BELOW is untouched: the same session store and the same
    // engine answered before and after, through a ledger that changed
    // underneath them.
    assert_eq!(last_dispatch(&settled)["engine"], DEFAULT_ENGINE);
    assert_eq!(get(port, "/v1/sessions").status, 200);
    assert_eq!(get(port, "/v1/engines").status, 200);
    daemon.interrupt();
}

#[test]
fn a_third_store_joins_a_live_daemon_by_a_profile_edit_alone() {
    let Some((daemon, port, root)) = booted("todos-extension") else {
        return;
    };
    // Not here yet, and refused by the API without a kernel call.
    let missing = get(port, &format!("/v1/todos/{SCRATCH_STORE}"));
    assert_eq!(missing.status, 404, "{}", missing.raw);

    let ephemeral = artifact_hash(&root, "jinn-todo-memory");
    let session_grant = serde_json::json!(format!("jinn:session.{SESSION_STORE}"));
    daemon.edit_profile(|document| {
        // The new store: its own contract name, its own entry, no change
        // to the definition and no new artifact.
        document["entries"]
            .as_array_mut()
            .expect("entries")
            .push(serde_json::json!({
                "id": SCRATCH_ID,
                "package": "todos/jinn-todo-memory",
                "hash": ephemeral,
                "config": {
                    "grants": [format!("jinn:todo.{SCRATCH_STORE}"), "jinn:clock", session_grant],
                    "data": { "store": SCRATCH_STORE, "poll-ms": 250 }
                }
            }));
        // The API may route to it only because the profile SAYS so.
        let api = entry_mut(document, API_ID);
        api["config"]["grants"]
            .as_array_mut()
            .expect("grants")
            .push(serde_json::json!(format!("jinn:todo.{SCRATCH_STORE}")));
        api["config"]["data"]["todo-stores"]
            .as_array_mut()
            .expect("todo-stores")
            .push(serde_json::json!(SCRATCH_STORE));
    });

    daemon.eventually("the third store to answer", || {
        get(port, &format!("/v1/todos/{SCRATCH_STORE}")).status == 200
    });
    let todo = create(port, SCRATCH_STORE, "work in the scratch ledger");
    dispatch(port, SCRATCH_STORE, &todo, DEFAULT_ENGINE);
    let settled = settled(&daemon, port, SCRATCH_STORE, &todo);
    assert_eq!(last_dispatch(&settled)["status"], "done", "{settled}");
    assert_eq!(settled["store"], SCRATCH_STORE);
    // The stores it joined are still there and still routed apart.
    assert_eq!(described(port, DEFAULT_STORE)["describe"]["durable"], true);
    assert_eq!(described(port, MEMORY_STORE)["describe"]["durable"], false);
    daemon.interrupt();
}

#[test]
fn a_dispatch_in_flight_when_the_daemon_dies_comes_back_interrupted_with_a_reason() {
    let Some((daemon, port, root)) = booted("todos-restart-honesty") else {
        return;
    };
    // A finished dispatch first, so the restart has to tell the two
    // apart: the honest answer is not "everything is interrupted".
    let finished = create(port, DEFAULT_STORE, "work that completed");
    dispatch(port, DEFAULT_STORE, &finished, DEFAULT_ENGINE);
    settled(&daemon, port, DEFAULT_STORE, &finished);

    // Now one that is genuinely in flight: the child-backed engine's run
    // lives for tens of seconds, so the kill lands mid-dispatch rather
    // than in a race with the answer.
    let live = create(port, DEFAULT_STORE, "work the daemon died on");
    dispatch(port, DEFAULT_STORE, &live, SPAWN_ENGINE);
    daemon.eventually("the dispatch to be in flight", || {
        last_dispatch(&record(port, DEFAULT_STORE, &live))["status"] == "running"
    });
    // The journal already holds the started dispatch — that ordering is
    // what the whole proof rests on, and it is checked BEFORE the kill so
    // a failure here is not confused with one after it.
    let document = journal(&daemon, &live).expect("a journal for a started dispatch");
    assert_untorn(&document, "the live Todo, before the kill");
    assert!(
        kinds(&document).contains(&"dispatch-started".to_owned())
            && !kinds(&document).contains(&"dispatch-ended".to_owned()),
        "started, not ended: {:?}",
        kinds(&document)
    );

    // The CRASH path: SIGKILL, no chance to write anything on the way out.
    daemon.kill();
    let daemon = reboot(&root);

    let recovered = record(port, DEFAULT_STORE, &live);
    // The DISPATCH is recorded interrupted, with a reason.
    let interrupted = last_dispatch(&recovered);
    assert_eq!(
        interrupted["status"], "interrupted",
        "a dispatch the daemon died on is interrupted, never eternally running: {recovered}"
    );
    assert_eq!(interrupted["reason"], INTERRUPTED_REASON, "{recovered}");
    // And the TODO does not read as still executing.
    assert_eq!(
        recovered["status"], "blocked",
        "never eternally executing: {recovered}"
    );
    assert!(
        recovered.get("status-reason").is_none(),
        "once the recovery is recorded the two statuses AGREE, so there is \
         nothing to explain away: {recovered}"
    );
    // The recovery is RECORDED, not merely derived: the declared status
    // moved too, so the ledger a caller can act on and the status a
    // reader is shown are the same status.
    assert_eq!(recovered["declared-status"], "blocked", "{recovered}");
    // And it is a NEW event appended after the ones already there —
    // history is append-only, and the move that started the work is
    // still readable exactly as it was written.
    let history = recovered["history"].as_array().expect("a history");
    assert_eq!(history.len(), 2, "{recovered}");
    assert_eq!(history[0]["from"], "backlog", "{recovered}");
    assert_eq!(history[0]["to"], "executing", "{recovered}");
    assert_eq!(history[1]["from"], "executing", "{recovered}");
    assert_eq!(history[1]["to"], "blocked", "{recovered}");
    assert_eq!(history[1]["note"], BLOCKED_REASON, "{recovered}");
    // Nobody asked for the recovery, and the record says so rather than
    // naming a principal that did not act.
    assert!(history[1].get("actor").is_none(), "{recovered}");

    // A dispatch that DID finish is still done — the restart is not a
    // blanket verdict.
    let survived = record(port, DEFAULT_STORE, &finished);
    assert_eq!(last_dispatch(&survived)["status"], "done", "{survived}");
    assert_eq!(survived["status"], "executing", "{survived}");

    // The log is not torn, and a crash does not shorten an append-only
    // log.
    let after_crash = journal(&daemon, &live).expect("the journal survived");
    assert_untorn(&after_crash, "the live Todo, after the crash");
    assert!(after_crash.len() >= document.len());
    assert_eq!(
        kinds(&after_crash),
        vec![
            "created",
            "status-changed",
            "dispatch-started",
            "status-changed"
        ],
        "the recovery is a line, appended after the dispatch it explains"
    );

    // And the recovered Todo is USABLE: `blocked -> executing` is a legal
    // move FROM WHERE THE RECORD SAYS IT IS, and a new dispatch runs on
    // it — which is what makes the interruption a state and not a
    // tombstone. A fold alone would have failed here, with the ledger
    // refusing a move an operator was shown as available.
    let resumed = update(port, DEFAULT_STORE, &live, "executing", "planner");
    assert_eq!(resumed.status, 200, "{}", resumed.raw);
    dispatch(port, DEFAULT_STORE, &live, DEFAULT_ENGINE);
    let again = settled(&daemon, port, DEFAULT_STORE, &live);
    assert_eq!(last_dispatch(&again)["status"], "done", "{again}");
    daemon.interrupt();
}

#[test]
fn a_torn_tail_is_absence_and_the_todo_before_it_survives() {
    let Some((daemon, port, root)) = booted("todos-torn-tail") else {
        return;
    };
    let todo = create(port, DEFAULT_STORE, "work with a torn tail");
    assert_eq!(
        update(port, DEFAULT_STORE, &todo, "executing", "planner").status,
        200
    );
    let path = daemon.data(&format!("todos/{todo}.jsonl"));
    let whole = std::fs::read(&path).expect("a journal");
    assert_untorn(&whole, "before the tear");
    daemon.kill();

    // A short write: the last line, unterminated, exactly what a torn
    // append would leave. The reader must admit it as ABSENCE — the Todo
    // reads back at its last WHOLE line, and the store comes up.
    let mut torn = whole.clone();
    torn.extend_from_slice(br#"{"kind":"status-changed","at-ms":9,"from":"exec"#);
    std::fs::write(&path, &torn).expect("write the torn journal");

    let daemon = reboot(&root);
    let recovered = record(port, DEFAULT_STORE, &todo);
    assert_eq!(
        recovered["declared-status"], "executing",
        "the tear is absence, not damage: {recovered}"
    );
    assert_eq!(recovered["history"].as_array().map(Vec::len), Some(1));
    // The store says what it discarded rather than dropping bytes in
    // silence.
    assert_eq!(
        described(port, DEFAULT_STORE)["describe"]["extra"]["healed-tails"],
        1
    );
    // And the store is fully usable: the next move appends onto a HEALED
    // document, so a tolerable tear never becomes an unreadable hole at
    // the boot after.
    assert_eq!(
        update(port, DEFAULT_STORE, &todo, "in-review", "planner").status,
        200
    );
    daemon.eventually("the appended move to be durable", || {
        std::fs::read(&path).is_ok_and(|bytes| {
            kinds(&bytes)
                .iter()
                .filter(|kind| *kind == "status-changed")
                .count()
                >= 2
        })
    });
    daemon.interrupt();
}

#[test]
fn an_ephemeral_store_keeps_nothing_across_a_restart_and_says_so() {
    let Some((daemon, port, root)) = booted("todos-ephemeral-restart") else {
        return;
    };
    let todo = create(port, MEMORY_STORE, "throwaway work");
    assert_eq!(described(port, MEMORY_STORE)["describe"]["todos"], 1);

    daemon.kill();
    let daemon = reboot(&root);

    // Gone, and answered as gone — `not-found`, never an empty Todo that
    // would read as one which merely has no history.
    let after = get(port, &format!("/v1/todos/{MEMORY_STORE}/{todo}"));
    assert_eq!(after.status, 404, "{}", after.raw);
    assert_eq!(described(port, MEMORY_STORE)["describe"]["todos"], 0);
    // The DURABLE store in the same composition kept its own, so this is
    // a property of the store and not of the restart.
    assert_eq!(described(port, DEFAULT_STORE)["describe"]["durable"], true);
    daemon.interrupt();
}

#[test]
fn a_tree_and_a_list_answer_the_company_view_of_the_ledger() {
    let Some((daemon, port, _root)) = booted("todos-tree") else {
        return;
    };
    let root_todo = create(port, DEFAULT_STORE, "the objective");
    let child = post(
        port,
        &format!("/v1/todos/{DEFAULT_STORE}"),
        &serde_json::json!({ "title": "a deliverable", "parent": root_todo,
                             "department": "platform" }),
    );
    assert_eq!(child.status, 200, "{}", child.raw);
    let child_id = child.body["todo-id"].as_str().expect("an id").to_owned();
    // A parent that is not here is a typed refusal, not a dangling edge.
    let orphan = post(
        port,
        &format!("/v1/todos/{DEFAULT_STORE}"),
        &serde_json::json!({ "title": "an orphan", "parent": "default-999" }),
    );
    assert_eq!(orphan.status, 404, "{}", orphan.raw);

    let tree = get(port, &format!("/v1/todos/{DEFAULT_STORE}/{root_todo}/tree"));
    assert_eq!(tree.status, 200, "{}", tree.raw);
    assert_eq!(tree.body["root"]["todo-id"], root_todo.as_str());
    assert_eq!(
        tree.body["root"]["children"][0]["todo-id"],
        child_id.as_str()
    );

    // `roots-only` is the objective view, and `total` still says how many
    // Todos the store holds — a filtered answer is never read as a short
    // store.
    let roots = get(port, &format!("/v1/todos/{DEFAULT_STORE}?roots-only=true"));
    assert_eq!(roots.status, 200, "{}", roots.raw);
    assert_eq!(
        roots.body["todos"].as_array().map(Vec::len),
        Some(1),
        "{}",
        roots.raw
    );
    assert_eq!(roots.body["total"], 2, "{}", roots.raw);

    // A comment is recorded with its actor, and an ANONYMOUS one records
    // that nobody was declared rather than inventing a principal.
    let commented = post(
        port,
        &format!("/v1/todos/{DEFAULT_STORE}/{child_id}/comments"),
        &serde_json::json!({ "body": "started", "actor": "planner" }),
    );
    assert_eq!(commented.status, 200, "{}", commented.raw);
    let anonymous = post(
        port,
        &format!("/v1/todos/{DEFAULT_STORE}/{child_id}/comments"),
        &serde_json::json!({ "body": "and continued" }),
    );
    assert_eq!(anonymous.status, 200, "{}", anonymous.raw);
    let read = record(port, DEFAULT_STORE, &child_id);
    assert_eq!(read["comments"][0]["actor"], "planner", "{read}");
    assert!(
        read["comments"][1].get("actor").is_none(),
        "absence is not a principal: {read}"
    );
    // A BLANK actor is refused rather than recorded as one.
    let blank = post(
        port,
        &format!("/v1/todos/{DEFAULT_STORE}/{child_id}/comments"),
        &serde_json::json!({ "body": "anon", "actor": "  " }),
    );
    assert_ne!(blank.status, 200, "{}", blank.raw);
    daemon.interrupt();
}

#[test]
fn a_status_no_durable_line_justifies_is_never_a_status_this_store_reports() {
    let Some((daemon, port, root)) = booted("todos-append-refused") else {
        return;
    };
    let todo = create(port, DEFAULT_STORE, "work whose journal stops accepting");
    assert_eq!(
        update(port, DEFAULT_STORE, &todo, "executing", "planner").status,
        200
    );
    let path = daemon.data(&format!("todos/{todo}.jsonl"));

    // Withdraw exactly ONE authority and nothing else: the durable store
    // may still read, list and rewrite its own directory, and may no
    // longer APPEND to it. Everything above and below is untouched, so
    // what follows is a failure of the durable write and of nothing else.
    daemon.edit_profile_restarting(DEFAULT_ID, |document| {
        let entry = entry_mut(document, DEFAULT_ID);
        for grant in entry["config"]["grants"].as_array_mut().expect("grants") {
            if grant["contract"] == "jinn:fs" {
                grant["ops"] = serde_json::json!(["read", "list", "meta", "write"]);
            }
        }
    });
    daemon.eventually("the re-granted store to answer", || {
        get(port, &format!("/v1/todos/{DEFAULT_STORE}/{todo}")).status == 200
    });
    // What the store reports with its journal intact, and the bytes that
    // justify it. Both are read AFTER the restart, so the comparison is
    // between two readings of the same document.
    let before = record(port, DEFAULT_STORE, &todo);
    let bytes_before = std::fs::read(&path).expect("a journal");
    assert_eq!(before["declared-status"], "executing", "{before}");

    // (a) The move FAILS, typed, naming the durable write that refused.
    let refused = update(port, DEFAULT_STORE, &todo, "in-review", "planner");
    assert_ne!(
        refused.status, 200,
        "a move whose record could not be written is not a move: {}",
        refused.raw
    );
    assert!(
        refused.body["error"]["detail"]
            .as_str()
            .unwrap_or_default()
            .contains("could not be appended to"),
        "the refusal names the write that failed: {}",
        refused.raw
    );

    // (b) The status the store REPORTS did not move, and its history did
    // not grow — the reported state is still exactly what the journal
    // holds, byte for byte.
    let after = record(port, DEFAULT_STORE, &todo);
    assert_eq!(after["status"], before["status"], "{after}");
    assert_eq!(
        after["declared-status"], before["declared-status"],
        "{after}"
    );
    assert_eq!(after["history"], before["history"], "{after}");
    assert_eq!(
        std::fs::read(&path).expect("a journal"),
        bytes_before,
        "the journal is byte-identical, so nothing may have advanced above it"
    );

    // The same holds for a COMMENT: refused, and not reported either.
    let commented = post(
        port,
        &format!("/v1/todos/{DEFAULT_STORE}/{todo}/comments"),
        &serde_json::json!({ "body": "started", "actor": "planner" }),
    );
    assert_ne!(commented.status, 200, "{}", commented.raw);
    assert_eq!(
        record(port, DEFAULT_STORE, &todo)["comments"],
        before["comments"],
        "a comment whose line did not land is not a comment the store holds"
    );

    // (c) The binding assertion: a RESTART replays exactly what the live
    // view was already saying. The two views of one Todo cannot disagree,
    // because the live one is folded from the log rather than kept beside
    // it.
    daemon.kill();
    let daemon = reboot(&root);
    let replayed = record(port, DEFAULT_STORE, &todo);
    assert_eq!(replayed["status"], after["status"], "{replayed}");
    assert_eq!(
        replayed["declared-status"], after["declared-status"],
        "{replayed}"
    );
    assert_eq!(replayed["history"], after["history"], "{replayed}");
    assert_eq!(replayed["comments"], after["comments"], "{replayed}");
    daemon.interrupt();
}

#[test]
fn the_same_dispatch_runs_over_a_vendor_engine_when_the_operator_names_one() {
    // The three-layer composition against a REAL vendor CLI: the same
    // Todo store, the same session store, the same message, and one
    // field different. Gated by name because it spends metered inference
    // under the operator's own authentication.
    let Ok(named) = std::env::var(VENDOR_GATE) else {
        eprintln!(
            "SKIPPED (loudly): the vendor leg of the three-layer composition did NOT run in \
             this pass and nothing here reports one. Set {VENDOR_GATE}=claude (or codex) on a \
             host where that CLI is authenticated to run it; the echo and child-backed legs \
             prove the binding swap between two in-repo providers and cannot stand in for a \
             real vendor CLI."
        );
        return;
    };
    let engine = named.trim().to_owned();
    assert!(
        !engine.is_empty(),
        "{VENDOR_GATE} names the engine to bind (claude or codex); an empty value is not a \
         request and is not a skip"
    );
    // A vendor CLI's load is not in the daemon budget's model.
    let _load = ExtraDaemonLoad::all_but_one();
    let Some((daemon, port, _root)) = booted("todos-vendor-engine") else {
        return;
    };
    // Asked for and ABSENT is a failure, never a quiet pass: the operator
    // named an engine, so its absence is the answer they need.
    let engines = get(port, "/v1/engines");
    assert_eq!(engines.status, 200, "{}", engines.raw);
    assert!(
        engines.body["engines"]
            .as_array()
            .is_some_and(|mounted| mounted
                .iter()
                .any(|entry| entry["engine"] == engine.as_str())),
        "{VENDOR_GATE} named {engine:?}, which this profile does not mount — the kit writes a \
         vendor entry only where that CLI is on the host: {}",
        engines.raw
    );

    // Leg one: the echo engine, asked exactly what the vendor will be.
    let echoed = create(port, DEFAULT_STORE, "the same brief, over echo");
    let sent = dispatch_saying(port, DEFAULT_STORE, &echoed, DEFAULT_ENGINE, VENDOR_PROMPT);
    assert_eq!(sent.status, 200, "{}", sent.raw);
    let first = settled(&daemon, port, DEFAULT_STORE, &echoed);
    assert_eq!(last_dispatch(&first)["status"], "done", "{first}");
    assert_eq!(last_dispatch(&first)["engine"], DEFAULT_ENGINE);

    // Leg two: ONE field different — the engine the dispatch names. The
    // Todo store, the session store, the API and every entry in the
    // profile are the same ones leg one ran through.
    let vended = create(port, DEFAULT_STORE, "the same brief, over a vendor CLI");
    let sent = dispatch_saying(port, DEFAULT_STORE, &vended, &engine, VENDOR_PROMPT);
    assert_eq!(sent.status, 200, "{}", sent.raw);
    let second = settled(&daemon, port, DEFAULT_STORE, &vended);
    let landed = last_dispatch(&second);
    assert_eq!(
        landed["status"], "done",
        "the vendor CLI answered and the Todo store recorded it: {second}"
    );
    assert_eq!(landed["engine"], engine.as_str(), "{second}");
    assert_eq!(landed["session-store"], SESSION_STORE, "{second}");
    let answer = landed["answer"].as_str().unwrap_or_default();
    assert!(
        answer.contains("OK"),
        "a real vendor answer reached the Todo through the session: {second}"
    );
    // The MIDDLE layer carried it: the session the dispatch opened is on
    // the sessions surface, bound to the vendor engine and to this Todo.
    let session_id = landed["session-id"].as_str().expect("a session id");
    let session = get(port, &format!("/v1/sessions/{SESSION_STORE}/{session_id}"));
    assert_eq!(session.status, 200, "{}", session.raw);
    assert_eq!(session.body["engine"], engine.as_str());
    assert_eq!(session.body["metadata"]["todo-id"], vended.as_str());
    // Said out loud, so a reader of the run's output can tell a leg that
    // RAN from one that was skipped without reading the assertions.
    eprintln!(
        "VENDOR LEG RAN: engine {engine:?} answered a Todo dispatched through session store \
         {SESSION_STORE:?}; answer {:?}",
        answer.trim()
    );
    daemon.interrupt();
}

/// The id this store mints after `minted` — the name a daemon killed
/// inside its very next `create` would leave a document under.
fn next_id(minted: &str) -> String {
    let (prefix, number) = minted.rsplit_once('-').expect("an id ends in its number");
    let number: u64 = number.parse().expect("a numeric id");
    format!("{prefix}-{}", number + 1)
}

#[test]
fn the_id_of_a_record_less_document_is_never_handed_to_a_new_todo() {
    // Round 2 taught this store to read a record-less document as
    // absence, which is right and is half an answer. The other half is
    // the BYTES and the ID: the document is still named for an id, and a
    // `create` that minted it again would write the new Todo's first
    // record into it. That half was missing one layer down and turned an
    // accepted absence into a journal that refuses to replay, so it is
    // proven here rather than assumed to be inherited (`FINDINGS.md` #36).
    let Some((daemon, port, root)) = booted("todos-record-less-id") else {
        return;
    };
    let real = create(port, DEFAULT_STORE, "work that really happened");
    let absent = next_id(&real);
    let path = daemon.data(&format!("todos/{absent}.jsonl"));
    daemon.kill();
    std::fs::write(&path, b"{").expect("write the record-less journal");
    let daemon = reboot(&root);

    // The bytes are gone, and nothing was written where a record never
    // was: a drop is the only repair.
    let after = std::fs::read(&path).unwrap_or_default();
    assert!(
        after.is_empty(),
        "the incomplete bytes survived the boot: {:?}",
        String::from_utf8_lossy(&after)
    );
    let read = get(port, &format!("/v1/todos/{DEFAULT_STORE}/{absent}"));
    assert_eq!(
        read.status, 404,
        "one byte that was never a record answered as a Todo: {}",
        read.raw
    );
    let described = described(port, DEFAULT_STORE);
    assert!(
        described["describe"]["extra"]["documents-without-a-record"]
            .as_u64()
            .is_some_and(|seen| seen >= 1),
        "a store that discards a whole document says so: {described}"
    );

    // The id is spoken for, and the Todo created next lands in a document
    // of its own that is whole from its first byte.
    let fresh = create(port, DEFAULT_STORE, "work after an absence");
    assert_ne!(
        fresh, absent,
        "a new Todo was handed the record-less document's id"
    );
    let document = journal(&daemon, &fresh).expect("the new Todo's journal");
    assert_untorn(&document, "the Todo created after the absence");

    // And the store comes back: the boot after is where the fusion would
    // have shown up as a replay that refuses.
    daemon.kill();
    let daemon = reboot(&root);
    record(port, DEFAULT_STORE, &fresh);
    record(port, DEFAULT_STORE, &real);
    daemon.interrupt();
}
