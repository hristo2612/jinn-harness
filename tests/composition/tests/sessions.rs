//! The sessions seam's real-composition gate (AGENTS.md standing order
//! 3): every proof boots the sessions profile — the two store providers
//! beside the engine providers, the api trio, the settings pair and the
//! cron seam — through the REAL pinned `jinnd` daemon in the operator
//! layout, and drives it as an operator would: plain HTTP on loopback,
//! with evidence from the journals on disk and the daemon's own log.
//!
//! This is the first seam that COMPOSES another, and the composition is
//! what the proofs are about:
//!
//! - **The store swaps** by a profile edit, with the API and the engine
//!   providers untouched.
//! - **The engine swaps** by ONE FIELD of a session spec, with the store
//!   untouched and unaware of which provider answered.
//! - **Both stores are live at once**, routed per session.
//! - **A third store joins** a live daemon by profile edit alone.
//!
//! And the one that is not about composition at all:
//!
//! - **Restart honesty.** A turn in flight when the daemon is KILLED
//!   comes back recorded `interrupted` WITH A REASON — never eternally
//!   `running`, never silently `done` — and the journal is never torn.
//!
//! Self-skips LOUDLY when no jinnd checkout holding the pinned commit is
//! reachable (KERNEL-PIN.md Gate 2).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use composition::api::{delete, get, post};
use composition::daemon::{jinnd_source, pinned_commit, pinned_daemon};
use composition::kit::{artifact_hash, entry_mut, fresh_session_root, Daemon};

/// The switchable store slot's entry id and the store id it serves.
const DEFAULT_ID: &str = "jinn-session-default";
/// See [`DEFAULT_ID`].
const DEFAULT_STORE: &str = "default";
/// The coexistence half's store id. Its ENTRY id is not named here: no
/// proof edits that entry — the swap proof moves the switchable slot ONTO
/// its package instead, which is the edit an operator actually makes.
const MEMORY_STORE: &str = "memory";
/// The extension proof's entry — NOT in the base document.
const SCRATCH_ID: &str = "jinn-session-scratch";
/// See [`SCRATCH_ID`].
const SCRATCH_STORE: &str = "scratch";
/// The API entry, whose grants and settings the extension proof edits.
const API_ID: &str = "jinn-api-http";

/// The engine every proof that is not about engines runs on: the echo
/// package, which answers on any box.
const DEFAULT_ENGINE: &str = "default";
/// The SECOND engine, and a genuinely different provider shape — the
/// echo package driving a real child through `jinn:process`. A run on it
/// stays live for tens of seconds, which is what makes it both the
/// "another engine" proof and the mid-flight one.
const SPAWN_ENGINE: &str = "spawn";

/// The reason a turn interrupted by a daemon stop carries. Asserted
/// verbatim, from its one home in the definition.
const INTERRUPTED_REASON: &str = jinn_session::journal::INTERRUPTED_REASON;

/// How long a turn may take to settle before a proof fails. Generous: the
/// suite runs several daemons at once and a store polls its engine.
const TURN_DEADLINE: Duration = Duration::from_secs(90);

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

/// Boots a fresh sessions root and waits for readiness AND the API's
/// first answer.
fn booted(name: &str) -> Option<(Daemon, u16, PathBuf)> {
    let binary = gate()?;
    let (root, port) = fresh_session_root(name);
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

/// Opens one session and answers its id.
fn create(port: u16, store: &str, engine: &str) -> String {
    let created = post(
        port,
        &format!("/v1/sessions/{store}"),
        &serde_json::json!({ "engine": { "engine": engine } }),
    );
    assert_eq!(created.status, 200, "{}", created.raw);
    created.body["session-id"]
        .as_str()
        .unwrap_or_else(|| panic!("a session id: {}", created.raw))
        .to_owned()
}

/// Sends one message and answers its turn id.
fn send(port: u16, store: &str, session: &str, message: &str) -> String {
    let accepted = post(
        port,
        &format!("/v1/sessions/{store}/{session}/turns"),
        &serde_json::json!({ "message": message }),
    );
    assert_eq!(accepted.status, 200, "{}", accepted.raw);
    accepted.body["turn-id"]
        .as_str()
        .unwrap_or_else(|| panic!("a turn id: {}", accepted.raw))
        .to_owned()
}

/// One session's record.
fn record(port: u16, store: &str, session: &str) -> serde_json::Value {
    let read = get(port, &format!("/v1/sessions/{store}/{session}"));
    assert_eq!(read.status, 200, "{}", read.raw);
    read.body
}

/// The status of one turn of a session, by turn id.
fn turn_status(record: &serde_json::Value, turn_id: &str) -> Option<String> {
    record["log"]
        .as_array()?
        .iter()
        .find(|turn| turn["turn-id"] == turn_id)
        .and_then(|turn| turn["status"].as_str())
        .map(str::to_owned)
}

/// Polls until a turn reaches a terminal status, and answers the whole
/// session record. A turn that never settles fails the proof with the
/// daemon's log.
fn settled(
    daemon: &Daemon,
    port: u16,
    store: &str,
    session: &str,
    turn: &str,
) -> serde_json::Value {
    let deadline = Instant::now() + TURN_DEADLINE;
    loop {
        let read = record(port, store, session);
        match turn_status(&read, turn).as_deref() {
            Some("running") | None => {}
            Some(_) => return read,
        }
        assert!(
            Instant::now() < deadline,
            "turn {turn} never settled\n--- daemon log ---\n{}",
            daemon.log()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// One store's `describe`, from the store list.
fn described(port: u16, store: &str) -> serde_json::Value {
    let list = get(port, "/v1/sessions");
    assert_eq!(list.status, 200, "{}", list.raw);
    list.body["stores"]
        .as_array()
        .unwrap_or_else(|| panic!("a store list: {}", list.raw))
        .iter()
        .find(|entry| entry["store"] == store)
        .unwrap_or_else(|| panic!("store {store:?} in the list: {}", list.raw))
        .clone()
}

/// The durable store's journal for one session, as raw bytes.
fn journal(daemon: &Daemon, session: &str) -> Option<Vec<u8>> {
    std::fs::read(daemon.data(&format!("sessions/{session}.jsonl"))).ok()
}

/// Asserts a journal document is WHOLE: every line decodes, and the last
/// byte is the terminator that makes a short write detectable. A torn
/// tail would be admitted by the reader as absence — that is the reader's
/// contract — but the WRITER is not supposed to produce one, and this is
/// where that is checked rather than assumed.
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
fn a_session_runs_a_turn_over_the_engine_it_is_bound_to() {
    let Some((daemon, port, _root)) = booted("sessions-turn") else {
        return;
    };
    let session = create(port, DEFAULT_STORE, DEFAULT_ENGINE);
    let turn = send(port, DEFAULT_STORE, &session, "hello");
    let settled = settled(&daemon, port, DEFAULT_STORE, &session, &turn);
    assert_eq!(
        turn_status(&settled, &turn).as_deref(),
        Some("done"),
        "the echo engine finished the turn: {settled}"
    );
    // `done` claims the answer is whole, and here it is: the echo engine
    // answers with the prompt, so the text is checkable and not merely
    // non-empty.
    let answer = settled["log"][0]["answer"].as_str().expect("an answer");
    assert!(
        answer.contains("hello"),
        "the engine's answer reached the turn: {settled}"
    );
    // The paginated read is the same log through the other route.
    let page = get(
        port,
        &format!("/v1/sessions/{DEFAULT_STORE}/{session}/messages?offset=0&limit=10"),
    );
    assert_eq!(page.status, 200, "{}", page.raw);
    assert_eq!(page.body["total"], 1);
    assert_eq!(page.body["messages"][0]["turn-id"], turn.as_str());
    assert!(
        page.body.get("next-offset").is_none(),
        "absence is the end of the log: {}",
        page.raw
    );
    // The feed carries the turn's life, in sequence, from a cursor.
    let feed = get(
        port,
        &format!("/v1/sessions/{DEFAULT_STORE}/{session}/events?limit=100"),
    );
    assert_eq!(feed.status, 200, "{}", feed.raw);
    let kinds: Vec<&str> = feed.body["events"]
        .as_array()
        .expect("events")
        .iter()
        .filter_map(|event| event["kind"].as_str())
        .collect();
    assert!(
        kinds.contains(&"created")
            && kinds.contains(&"turn-started")
            && kinds.contains(&"turn-ended"),
        "the feed carries the turn's life: {kinds:?}"
    );
    // And the DURABLE store wrote it down, whole.
    let document = journal(&daemon, &session).expect("the durable store wrote a journal");
    assert_untorn(&document, "the settled session");
    daemon.interrupt();
}

#[test]
fn the_same_spec_runs_over_another_engine_by_the_binding_alone() {
    let Some((daemon, port, _root)) = booted("sessions-engine-swap") else {
        return;
    };
    // Two sessions in the SAME store, differing in exactly one field of
    // the spec. Neither the store's package nor its config moves.
    let echoed = create(port, DEFAULT_STORE, DEFAULT_ENGINE);
    let spawned = create(port, DEFAULT_STORE, SPAWN_ENGINE);
    let echo_turn = send(port, DEFAULT_STORE, &echoed, "hello");
    let echo_settled = settled(&daemon, port, DEFAULT_STORE, &echoed, &echo_turn);
    assert_eq!(
        turn_status(&echo_settled, &echo_turn).as_deref(),
        Some("done")
    );
    // The second engine is a genuinely different provider shape — it
    // spawns a real child through `jinn:process`. The store does not know
    // that, and needs no change to drive it.
    let spawn_turn = send(port, DEFAULT_STORE, &spawned, "hello");
    let running = record(port, DEFAULT_STORE, &spawned);
    assert_eq!(
        turn_status(&running, &spawn_turn).as_deref(),
        Some("running"),
        "a child-backed run is live: {running}"
    );
    // Cancelling it ends the turn honestly — with a reason, and never
    // `done`.
    let cancelled = delete(
        port,
        &format!("/v1/sessions/{DEFAULT_STORE}/{spawned}/turns"),
    );
    assert_eq!(cancelled.status, 200, "{}", cancelled.raw);
    let settled = settled(&daemon, port, DEFAULT_STORE, &spawned, &spawn_turn);
    assert_eq!(
        turn_status(&settled, &spawn_turn).as_deref(),
        Some("cancelled"),
        "{settled}"
    );
    let reason = settled["log"][0]["reason"].as_str().unwrap_or_default();
    assert!(
        !reason.is_empty(),
        "a non-done ending explains itself: {settled}"
    );
    // The engine each session ran on is recorded per SESSION, so the two
    // are told apart by their binding and not by their store.
    assert_eq!(
        record(port, DEFAULT_STORE, &echoed)["engine"],
        DEFAULT_ENGINE
    );
    assert_eq!(
        record(port, DEFAULT_STORE, &spawned)["engine"],
        SPAWN_ENGINE
    );
    daemon.interrupt();
}

#[test]
fn both_stores_are_live_at_once_and_a_session_is_routed_by_its_store() {
    let Some((daemon, port, _root)) = booted("sessions-coexist") else {
        return;
    };
    let durable = create(port, DEFAULT_STORE, DEFAULT_ENGINE);
    let ephemeral = create(port, MEMORY_STORE, DEFAULT_ENGINE);
    // Each store answers for its OWN sessions and knows nothing of the
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
    let crossed = get(port, &format!("/v1/sessions/{MEMORY_STORE}/{durable}"));
    assert_eq!(crossed.status, 404, "{}", crossed.raw);
    // Their durability declarations differ, and so does what is on disk.
    assert_eq!(described(port, DEFAULT_STORE)["describe"]["durable"], true);
    assert_eq!(described(port, MEMORY_STORE)["describe"]["durable"], false);
    let turn = send(port, MEMORY_STORE, &ephemeral, "hello");
    settled(&daemon, port, MEMORY_STORE, &ephemeral, &turn);
    assert!(
        journal(&daemon, &ephemeral).is_none(),
        "the ephemeral store wrote nothing, which is its whole contract"
    );
    assert!(journal(&daemon, &durable).is_some());
    daemon.interrupt();
}

#[test]
fn the_store_swaps_by_a_profile_edit_with_the_api_and_the_engines_untouched() {
    let Some((daemon, port, root)) = booted("sessions-store-swap") else {
        return;
    };
    let before = create(port, DEFAULT_STORE, DEFAULT_ENGINE);
    let turn = send(port, DEFAULT_STORE, &before, "hello");
    settled(&daemon, port, DEFAULT_STORE, &before, &turn);
    assert_eq!(described(port, DEFAULT_STORE)["describe"]["durable"], true);
    assert!(journal(&daemon, &before).is_some());

    // The swap: ONE entry's package and hash. The API entry, the engine
    // entries, and the store id are not touched — so the contract name
    // stays `jinn:session.default` and every consumer keeps its grant.
    let ephemeral = artifact_hash(&root, "jinn-session-memory");
    daemon.edit_profile_restarting(DEFAULT_ID, |document| {
        let entry = entry_mut(document, DEFAULT_ID);
        entry["package"] = serde_json::json!("sessions/jinn-session-memory");
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

    // The same route, the same store id, a different implementation — and
    // the store says so itself.
    daemon.eventually("the swapped store to declare itself ephemeral", || {
        described(port, DEFAULT_STORE)["describe"]["durable"] == serde_json::json!(false)
    });
    let after = create(port, DEFAULT_STORE, DEFAULT_ENGINE);
    let turn = send(port, DEFAULT_STORE, &after, "hello");
    let record = settled(&daemon, port, DEFAULT_STORE, &after, &turn);
    assert_eq!(
        turn_status(&record, &turn).as_deref(),
        Some("done"),
        "{record}"
    );
    assert!(
        journal(&daemon, &after).is_none(),
        "the swapped-in store writes nothing"
    );
    // The engines are untouched: the same engine answered before and
    // after, through a store that changed underneath it.
    assert_eq!(record["engine"], DEFAULT_ENGINE);
    let engines = get(port, "/v1/engines");
    assert_eq!(engines.status, 200, "{}", engines.raw);
    daemon.interrupt();
}

#[test]
fn a_third_store_joins_a_live_daemon_by_a_profile_edit_alone() {
    let Some((daemon, port, root)) = booted("sessions-extension") else {
        return;
    };
    // Not here yet, and refused by the API without a kernel call.
    let missing = get(port, &format!("/v1/sessions/{SCRATCH_STORE}"));
    assert_eq!(missing.status, 404, "{}", missing.raw);

    let ephemeral = artifact_hash(&root, "jinn-session-memory");
    let engine_grant = serde_json::json!(format!("jinn:engine.{DEFAULT_ENGINE}"));
    daemon.edit_profile(|document| {
        // The new store: its own contract name, its own entry, no change
        // to the definition and no new artifact.
        document["entries"]
            .as_array_mut()
            .expect("entries")
            .push(serde_json::json!({
                "id": SCRATCH_ID,
                "package": "sessions/jinn-session-memory",
                "hash": ephemeral,
                "config": {
                    "grants": [format!("jinn:session.{SCRATCH_STORE}"), "jinn:clock", engine_grant],
                    "data": { "store": SCRATCH_STORE, "poll-ms": 250 }
                }
            }));
        // The API may route to it only because the profile SAYS so: the
        // grant is the authority the kernel enforces, and the setting is
        // the same fact told to the provider.
        let api = entry_mut(document, API_ID);
        api["config"]["grants"]
            .as_array_mut()
            .expect("grants")
            .push(serde_json::json!(format!("jinn:session.{SCRATCH_STORE}")));
        let stores = api["config"]["data"]["stores"]
            .as_array_mut()
            .expect("stores");
        stores.push(serde_json::json!(SCRATCH_STORE));
    });

    daemon.eventually("the third store to answer", || {
        get(port, &format!("/v1/sessions/{SCRATCH_STORE}")).status == 200
    });
    let session = create(port, SCRATCH_STORE, DEFAULT_ENGINE);
    let turn = send(port, SCRATCH_STORE, &session, "hello");
    let record = settled(&daemon, port, SCRATCH_STORE, &session, &turn);
    assert_eq!(
        turn_status(&record, &turn).as_deref(),
        Some("done"),
        "{record}"
    );
    assert_eq!(record["store"], SCRATCH_STORE);
    // The stores it joined are still there and still routed apart.
    assert_eq!(described(port, DEFAULT_STORE)["describe"]["durable"], true);
    assert_eq!(described(port, MEMORY_STORE)["describe"]["durable"], false);
    daemon.interrupt();
}

#[test]
fn a_turn_in_flight_when_the_daemon_dies_comes_back_interrupted_with_a_reason() {
    let Some((daemon, port, root)) = booted("sessions-restart-honesty") else {
        return;
    };
    // A finished turn first, so the restart has to tell the two apart:
    // the honest answer is not "everything is interrupted".
    let session = create(port, DEFAULT_STORE, DEFAULT_ENGINE);
    let finished = send(port, DEFAULT_STORE, &session, "hello");
    settled(&daemon, port, DEFAULT_STORE, &session, &finished);

    // Now one that is genuinely in flight: the child-backed engine's run
    // lives for tens of seconds, so the kill lands mid-turn rather than
    // in a race with the answer.
    let live_session = create(port, DEFAULT_STORE, SPAWN_ENGINE);
    let live_turn = send(port, DEFAULT_STORE, &live_session, "hello");
    daemon.eventually("the turn to be in flight", || {
        turn_status(&record(port, DEFAULT_STORE, &live_session), &live_turn).as_deref()
            == Some("running")
    });
    // The journal already holds the started turn — that ordering is what
    // the whole proof rests on, and it is checked BEFORE the kill so a
    // failure here is not confused with one after it.
    let document = journal(&daemon, &live_session).expect("a journal for a started turn");
    assert_untorn(&document, "the live session, before the kill");

    // The CRASH path: SIGKILL, no chance to write anything on the way out.
    daemon.kill();
    let daemon = reboot(&root);

    // The finished turn is still finished. The live one is INTERRUPTED,
    // with the reason, and it is not `running` and not `done`.
    let recovered = record(port, DEFAULT_STORE, &live_session);
    assert_eq!(
        turn_status(&recovered, &live_turn).as_deref(),
        Some("interrupted"),
        "a turn the daemon died on is interrupted, never eternally running: {recovered}"
    );
    let reason = recovered["log"]
        .as_array()
        .expect("a log")
        .iter()
        .find(|turn| turn["turn-id"] == live_turn.as_str())
        .and_then(|turn| turn["reason"].as_str())
        .unwrap_or_default();
    assert_eq!(
        reason, INTERRUPTED_REASON,
        "the interruption explains itself: {recovered}"
    );
    let survived = record(port, DEFAULT_STORE, &session);
    assert_eq!(
        turn_status(&survived, &finished).as_deref(),
        Some("done"),
        "a turn that DID finish is still done — the restart is not a blanket verdict: {survived}"
    );
    // The message survived too: an interrupted turn keeps what was asked,
    // it only refuses to claim what came back.
    let asked = recovered["log"]
        .as_array()
        .expect("a log")
        .iter()
        .find(|turn| turn["turn-id"] == live_turn.as_str())
        .and_then(|turn| turn["message"].as_str())
        .unwrap_or_default();
    assert_eq!(asked, "hello", "{recovered}");

    // The log is not torn: the crash left a document every line of which
    // decodes and which ends on its terminator.
    let after_crash = journal(&daemon, &live_session).expect("the journal survived");
    assert_untorn(&after_crash, "the live session, after the crash");
    assert!(
        after_crash.len() >= document.len(),
        "a crash does not shorten an append-only log"
    );

    // And the recovered session is usable: a new turn runs on it, which
    // is what makes `interrupted` a state and not a tombstone.
    let again = send(port, DEFAULT_STORE, &live_session, "hello again");
    let settled = settled(&daemon, port, DEFAULT_STORE, &live_session, &again);
    assert_eq!(
        turn_status(&settled, &again).as_deref(),
        Some("done"),
        "{settled}"
    );
    daemon.interrupt();
}

#[test]
fn an_ephemeral_store_keeps_nothing_across_a_restart_and_says_so() {
    let Some((daemon, port, root)) = booted("sessions-ephemeral-restart") else {
        return;
    };
    let session = create(port, MEMORY_STORE, DEFAULT_ENGINE);
    let turn = send(port, MEMORY_STORE, &session, "hello");
    settled(&daemon, port, MEMORY_STORE, &session, &turn);
    assert_eq!(described(port, MEMORY_STORE)["describe"]["sessions"], 1);

    daemon.kill();
    let daemon = reboot(&root);

    // Gone, and answered as gone — `not-found`, never an empty session
    // that would read as one which merely has no turns.
    let after = get(port, &format!("/v1/sessions/{MEMORY_STORE}/{session}"));
    assert_eq!(after.status, 404, "{}", after.raw);
    assert_eq!(described(port, MEMORY_STORE)["describe"]["sessions"], 0);
    // The DURABLE store in the same composition kept its own, so this is
    // a property of the store and not of the restart.
    assert_eq!(described(port, DEFAULT_STORE)["describe"]["durable"], true);
    daemon.interrupt();
}
