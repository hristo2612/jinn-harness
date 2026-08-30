//! The guest-side run store, shared by both providers (see this
//! directory's README for why it is source and not a crate). Everything
//! here is a host call or the sequencing around one; the semantics are
//! the definition's (`jinn_workflow::Workflows`, `jinn_workflow::journal`,
//! `jinn_workflow::dispatch`).
//!
//! # The four disciplines this file exists to hold in one place
//!
//! **A run store drives a TODO store and never records a Todo itself.**
//! Every node dispatch is two calls on the TODOS seam's DEFINITION: the
//! node's binding names a Todo store, that store id becomes a contract
//! name through `jinn_workflow::dispatch::todo_contract`, and whatever
//! provider holds that slot answers `jinn_todo::OP_CREATE` and
//! `jinn_todo::OP_DISPATCH`. There is no session knowledge here and no
//! engine knowledge here at all — the Todo store resolves the session,
//! and the session resolves the engine — which is what makes the
//! four-layer stack compose. Nothing in this file names a Todo provider,
//! a session provider or an engine.
//!
//! **The record is durable BEFORE the store's answer moves.** Every
//! mutation here is the same three steps in the same order: PLAN what
//! would happen (the definition's `plan_*`, which touches nothing),
//! APPEND the record to the journal, and only then COMMIT it into the
//! registry (`commit_*`). So a refused append leaves the state this store
//! reports exactly where it was, and a restart replays what the live view
//! was already saying — the two views cannot disagree, because the live
//! one is folded from the log by construction. Writing first and folding
//! after is not a courtesy; it is the only order in which a reported
//! state is a state something durable justifies.
//!
//! **A refusal is recorded before it is answered.** An illegal node-state
//! move goes to the journal and the bus as a `node-transition-refused`,
//! and only then comes back to the caller as a typed error. The registry
//! makes that hard to get wrong (`Moved::Refused` carries the record),
//! and this file is where the durable half lands.
//!
//! **Nothing is emitted from inside a caller's dispatch.** The todos seam
//! publishes a Todo's progress on its own topic; a store that LISTENED
//! there would emit its own events from inside that fiber's delivery —
//! the nested-dispatch class this repo keeps finding (`FINDINGS.md` #4,
//! and #32 at this pin). So the store POLLS the Todo store's `get` on its
//! own clock wake (`jinn_todo::OP_GET`), and every bus record minted
//! while a caller is in this guest is held in [`DEFERRED`] until that
//! wake.

use std::collections::BTreeSet;
use std::sync::Mutex;

use jinn_workflow::{
    dispatch as translate, Answer, Attribution, CancelRequest, DefineRequest, ErrorCode, EventKind,
    EventsRequest, Extensions, ListRunsRequest, Moved, NodeChange, NodeKind, NodeRun, NodeSpec,
    NodeState, RefusedChange, RunEvent, RunRequest, RunStatus, StartRequest, TodoBinding,
    WorkflowDefined, WorkflowError, WorkflowRequest, Workflows, API_VERSION, EVENT_TOPIC,
    OP_CANCEL, OP_DEFINE, OP_EVENTS, OP_GET, OP_GET_RUN, OP_LIST, OP_LIST_RUNS, OP_NODE_STATE,
    OP_START,
};
use serde::Deserialize;

use crate::jinn::plugin::types::{DispatchMode, Selector};
use crate::jinn::plugin::{clock, events, services};
use crate::journal;

/// The wake token every poll is scheduled under.
pub const ALARM_TOKEN: u64 = 2;
/// The kernel's alarm wake topic, bound from `kernel-pin/wit/plugin.wit`.
pub const WAKE_TOPIC: &str = "jinn:clock/alarm";

/// The reason a `dispatch` node whose pinned spec carries no binding ends
/// with. The definition refuses such a spec at `define`
/// (`WorkflowSpec::check`), so this is reachable only from a document
/// this seam did not write — and a node that cannot be dispatched is
/// ENDED naming that, never left `running` forever.
const UNBOUND_NODE_REASON: &str =
    "this node is a `dispatch` and the revision this run pinned carries no Todo binding for it";

/// One store entry's config.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct StoreConfig {
    /// The store id served — the second half of the contract name, and
    /// the only place the id lives (the definition's rule).
    pub store: String,
    /// Where a durable store's journals go, relative to the `jinn:fs`
    /// grant's scope. Read by `jinn-workflow-fs`'s journal and by nothing
    /// in the ephemeral store, which is why it is dead code in exactly
    /// one of the two crates that include this file.
    #[serde(default)]
    #[allow(dead_code)]
    pub dir: Option<String>,
    /// How often a run with live work is polled.
    #[serde(default = "default_poll_ms")]
    pub poll_ms: u64,
}

fn default_poll_ms() -> u64 {
    250
}

/// `node-state`: the OPERATOR's lane into the transition table. Not a
/// definition type, because it is a request shape and not a record: the
/// definition owns what a move MEANS, and this owns what one call
/// carries.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct NodeStateRequest {
    run_id: String,
    node_id: String,
    /// The state asked for. A CLOSED value space: a state this version
    /// cannot name is refused at the decode, never folded onto a
    /// neighbour.
    state: NodeState,
    #[serde(default)]
    note: Option<String>,
    #[serde(default, flatten)]
    attribution: Attribution,
}

pub static CONFIG: Mutex<Option<StoreConfig>> = Mutex::new(None);
pub static WORKFLOWS: Mutex<Option<Workflows>> = Mutex::new(None);
/// The runs this incarnation is driving. Per incarnation, and restored by
/// nothing: a node's Todo is dispatched by the incarnation that started
/// it, so a fresh one drives nothing it did not open — which is exactly
/// why an adopted run is RECOVERED before this store serves anything. A
/// run id is the whole of what a driver needs, because everything else it
/// reads is in that run's own record.
pub static DRIVING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());
/// Bus records minted inside a call, held until this provider's own fiber
/// wakes (see the module doc).
pub static DEFERRED: Mutex<Vec<RunEvent>> = Mutex::new(Vec::new());
/// Refused emits since activation. Never fatal — the feed and the record
/// still hold the event — and never silent: `describe` reports it.
pub static EMIT_FAILURES: Mutex<u64> = Mutex::new(0);
/// Journals whose torn TAIL was dropped on adoption. Bytes that were
/// never a record, discarded out loud rather than in silence — a durable
/// store's `describe` reports it, and an ephemeral store's is always 0.
pub static HEALED_TAILS: Mutex<u64> = Mutex::new(0);

/// The config this incarnation activated with.
///
/// # Panics
///
/// Called before `activate`.
pub fn config() -> StoreConfig {
    CONFIG
        .lock()
        .unwrap()
        .clone()
        .expect("activate holds the config")
}

fn with_workflows<T>(act: impl FnOnce(&mut Workflows) -> T) -> T {
    let mut held = WORKFLOWS.lock().unwrap();
    act(held.as_mut().expect("activate holds the registry"))
}

/// Puts one bus record on the wire. A refusal is counted, never fatal.
fn emit(record: &RunEvent) {
    let payload = serde_json::to_vec(record).expect("a run event encodes");
    if events::emit(EVENT_TOPIC, DispatchMode::Emit, &Selector::All, &payload).is_err() {
        *EMIT_FAILURES.lock().unwrap() += 1;
    }
}

/// Puts everything held for this fiber on the wire, in sequence order.
pub fn flush_deferred() {
    let records = std::mem::take(&mut *DEFERRED.lock().unwrap());
    for record in &records {
        emit(record);
    }
}

/// Records one event against a run and HOLDS it for the next wake.
///
/// A `defined` event belongs to no run, and its `run_id` is empty rather
/// than a run id this store would have to invent: a listener routes it on
/// the workflow it names, which is the only thing a definition has.
fn record_event(run_id: &str, kind: EventKind) {
    let record = with_workflows(|workflows| workflows.record_event(run_id, kind));
    DEFERRED.lock().unwrap().push(record);
}

/// This incarnation's `now`.
///
/// # Errors
///
/// The clock refused.
pub fn now_ms() -> Result<u64, WorkflowError> {
    clock::now().map_err(|error| {
        WorkflowError::new(ErrorCode::Failed, format!("clock now: {error:?}"))
    })
}

/// Schedules this store's next poll.
fn wake_at(at_ms: u64) -> Result<(), WorkflowError> {
    clock::alarm_at(at_ms, ALARM_TOKEN).map_err(|error| {
        WorkflowError::new(
            ErrorCode::Failed,
            format!("this store could not schedule its own poll: {error:?}"),
        )
    })?;
    Ok(())
}

/// The workflows error class a todos error maps onto. `unavailable` stays
/// `unavailable` — this store is fine and its Todo store cannot be
/// reached — so it never reads as a refusal of the run.
fn todo_code(code: jinn_todo::ErrorCode) -> ErrorCode {
    match code {
        jinn_todo::ErrorCode::Invalid => ErrorCode::Invalid,
        jinn_todo::ErrorCode::NotFound => ErrorCode::NotFound,
        jinn_todo::ErrorCode::Refused => ErrorCode::Refused,
        jinn_todo::ErrorCode::Unavailable => ErrorCode::Unavailable,
        jinn_todo::ErrorCode::Failed => ErrorCode::Failed,
    }
}

/// One call on the todos seam's DEFINITION. A composition that holds no
/// such Todo store is an ordinary typed answer naming it — never a fault.
///
/// # Errors
///
/// The contract is unresolvable, the call was refused, or the Todo store
/// answered a typed error (which rides along as `todo-code`).
pub fn todo_call(
    contract: &str,
    operation: &str,
    payload: &[u8],
) -> Result<serde_json::Value, WorkflowError> {
    let handle = services::resolve(contract).map_err(|error| {
        WorkflowError::new(
            ErrorCode::Unavailable,
            format!("{contract} is not resolvable: {error:?}"),
        )
    })?;
    let bytes = services::call(handle, operation, payload).map_err(|error| {
        WorkflowError::new(
            ErrorCode::Refused,
            format!("{contract}/{operation} refused: {error:?}"),
        )
    })?;
    let answer: jinn_todo::Answer = serde_json::from_slice(&bytes).map_err(|error| {
        WorkflowError::new(ErrorCode::Failed, format!("malformed Todo answer: {error}"))
    })?;
    answer.into_result().map_err(|error| {
        let mut mapped = WorkflowError::new(todo_code(error.code), error.message);
        if let Ok(code) = serde_json::to_value(error.code) {
            mapped.extra.insert("todo-code".to_owned(), code);
        }
        mapped
    })
}

// ---- the durable half of every move ----------------------------------

/// The durable half of one node move: the journal, then the registry, and
/// nothing on the bus. The line carries the node as it stands NOW, which
/// is why the node is read before it is written — an ending's line carries
/// the Todo binding and the answer the node collected, so a replay reads
/// both back.
fn record_change(run_id: &str, change: &NodeChange) -> Result<(), WorkflowError> {
    let node = node_now(run_id, &change.node_id)?;
    journal::node_state_changed(run_id, change, &node)?;
    with_workflows(|workflows| workflows.commit_node_change(run_id, change));
    Ok(())
}

/// Lands one node move in the ONE order this store is allowed to use: the
/// journal first, the registry second, the bus last. A move whose line did
/// not land changes nothing a reader can see. What the move MEANS on the
/// bus rides along from the definition's own `move_events`, so a provider
/// cannot record a move without emitting what it means.
fn land_change(run_id: &str, change: &NodeChange) -> Result<(), WorkflowError> {
    record_change(run_id, change)?;
    for kind in Workflows::move_events(change) {
        record_event(run_id, kind);
    }
    Ok(())
}

/// Lands one REFUSED attempt in the same order: the journal, then the
/// record, then the bus. A refusal whose line did not land is answered as
/// the append's own failure — never as a refusal the record does not hold.
fn land_refusal(run_id: &str, refused: &RefusedChange) -> Result<(), WorkflowError> {
    journal::node_transition_refused(run_id, refused)?;
    with_workflows(|workflows| workflows.commit_refusal(run_id, refused));
    record_event(
        run_id,
        EventKind::NodeTransitionRefused {
            node_id: refused.node_id.clone(),
            from: refused.from,
            to: refused.to,
            actor: refused.actor.clone(),
        },
    );
    Ok(())
}

/// The durable half of a run's ending: the journal, then the registry.
/// Answers the ending as the definition PLANNED it, so whatever is
/// announced afterwards is what was recorded rather than what was asked
/// for.
fn record_run_end(
    run_id: &str,
    status: RunStatus,
    reason: Option<String>,
    now: u64,
) -> Result<(RunStatus, Option<String>), WorkflowError> {
    let (status, reason) =
        with_workflows(|workflows| workflows.plan_run_end(run_id, status, reason, now))?;
    journal::run_ended(run_id, status, reason.as_deref(), now)?;
    with_workflows(|workflows| workflows.commit_run_end(run_id, status, reason.clone(), now));
    Ok((status, reason))
}

/// Ends one run: the journal, the registry, the bus, and then the driver
/// lets it go. Nothing is REPORTED that is not recorded.
fn end_run(
    run_id: &str,
    status: RunStatus,
    reason: Option<String>,
    now: u64,
) -> Result<(), WorkflowError> {
    let (status, reason) = record_run_end(run_id, status, reason, now)?;
    record_event(run_id, EventKind::RunEnded { status, reason });
    DRIVING.lock().unwrap().remove(run_id);
    Ok(())
}

/// Moves one node through the table and lands whichever outcome the
/// definition planned. A refusal is RECORDED and only then answered.
fn move_node(
    run_id: &str,
    node_id: &str,
    to: NodeState,
    actor: Option<String>,
    note: Option<String>,
    now: u64,
) -> Result<(), WorkflowError> {
    let moved = with_workflows(|workflows| {
        workflows.plan_node_move(run_id, node_id, to, actor, note, now)
    })?;
    match moved {
        Moved::Changed(change) => land_change(run_id, &change),
        Moved::Refused(refused, error) => {
            land_refusal(run_id, &refused)?;
            Err(error)
        }
    }
}

// ---- operations ------------------------------------------------------

/// `define`: the revision's journal line, and only then the revision. A
/// caller that holds a revision number holds one that survives a crash,
/// and a `defined` line that could not be written leaves NO revision
/// behind — not one a later replay would have to disagree with.
fn on_define(payload: &[u8]) -> Result<serde_json::Value, WorkflowError> {
    let request: DefineRequest = decode(payload, "define")?;
    let now = now_ms()?;
    let definition = with_workflows(|workflows| {
        workflows.plan_define(&request.spec, request.workflow_id.as_deref(), now)
    })?;
    journal::defined(&definition)?;
    with_workflows(|workflows| workflows.commit_define(&definition));
    record_event(
        "",
        EventKind::Defined {
            workflow_id: definition.workflow_id.clone(),
            revision: definition.revision,
        },
    );
    // The `defined` event is on this fiber's deferred list; a wake has to
    // come for it to reach the bus.
    wake_at(now)?;
    Ok(serde_json::to_value(WorkflowDefined {
        api_version: API_VERSION.to_owned(),
        workflow_id: definition.workflow_id.clone(),
        store: config().store,
        revision: definition.revision,
        spec_digest: definition.spec_digest.clone(),
        extra: Extensions::new(),
    })
    .expect("encodes"))
}

/// `start`: the run's first line — the PIN, written whole — and only then
/// the run.
///
/// The run is opened and nothing else: not one node is started here. The
/// graph is walked on this store's OWN wake, so every call into the todos
/// seam happens on a fiber this store woke rather than inside a caller's
/// dispatch, and a run has exactly one place where it advances
/// ([`poll_once`]).
fn on_start(payload: &[u8]) -> Result<serde_json::Value, WorkflowError> {
    let request: StartRequest = decode(payload, "start")?;
    let now = now_ms()?;
    let started = with_workflows(|workflows| workflows.plan_start(&request))?;
    journal::run_started(&started, now)?;
    with_workflows(|workflows| workflows.commit_start(&started, now));
    record_event(
        &started.run_id,
        EventKind::RunStarted {
            workflow_id: started.definition.workflow_id.clone(),
            revision: started.definition.revision,
        },
    );
    DRIVING.lock().unwrap().insert(started.run_id.clone());
    wake_at(now)?;
    run_of(&started.run_id)
}

/// `node-state`: the OPERATOR's move through the table.
///
/// A REFUSAL is recorded and only then answered — the journal line and the
/// bus event land before the caller hears `refused`, so an operator
/// reading the ledger sees the attempt even if the caller drops the answer
/// on the floor.
///
/// What FOLLOWS the move — a node that is now ready, a run that can now
/// end — is the driver's, not this call's: the wake armed here walks the
/// graph, so a run advances in one place whether the move came from an
/// operator or from a Todo that finished.
fn on_node_state(payload: &[u8]) -> Result<serde_json::Value, WorkflowError> {
    let request: NodeStateRequest = decode(payload, "node-state")?;
    let actor = request.attribution.check()?;
    let now = now_ms()?;
    let outcome = move_node(
        &request.run_id,
        &request.node_id,
        request.state,
        actor,
        request.note.clone(),
        now,
    );
    wake_at(now)?;
    outcome?;
    run_of(&request.run_id)
}

/// `cancel`: an ending, on the record.
///
/// A blank reason is REFUSED before anything is written. An ending nobody
/// can explain is worse than no ending at all: a reader would be left
/// inventing why the company stopped this work.
///
/// Every node that has not ended is moved to `cancelled` carrying that
/// reason, and only then does the run end. A Todo a node already
/// dispatched is not reached into from here — this seam records what its
/// own ledger can prove, and the Todo's own store owns that Todo's
/// ending.
fn on_cancel(payload: &[u8]) -> Result<serde_json::Value, WorkflowError> {
    let request: CancelRequest = decode(payload, "cancel")?;
    let actor = request.attribution.check()?;
    let reason = request.reason.trim().to_owned();
    if reason.is_empty() {
        return Err(WorkflowError::new(
            ErrorCode::Invalid,
            "a cancelled run carries a reason, so no reader has to invent one",
        ));
    }
    let now = now_ms()?;
    let open = with_workflows(|workflows| {
        workflows.run(&request.run_id).map(|record| {
            record
                .nodes
                .iter()
                .filter(|node| !node.state.is_terminal())
                .map(|node| node.node_id.clone())
                .collect::<Vec<String>>()
        })
    })
    .ok_or_else(|| no_run(&request.run_id))?;
    for node_id in open {
        move_node(
            &request.run_id,
            &node_id,
            NodeState::Cancelled,
            actor.clone(),
            Some(reason.clone()),
            now,
        )?;
    }
    end_run(
        &request.run_id,
        RunStatus::Cancelled,
        Some(reason),
        now,
    )?;
    wake_at(now)?;
    run_of(&request.run_id)
}

// ---- the driver ------------------------------------------------------

/// One poll of every run this incarnation is driving. Answers whether
/// anything is still being driven.
///
/// # Errors
///
/// Only what the store cannot recover from; a Todo that cannot be read
/// ENDS its node (failed, with the reason) rather than leaving it running
/// forever.
pub fn poll_once(now: u64) -> Result<bool, WorkflowError> {
    let driving: Vec<String> = DRIVING.lock().unwrap().iter().cloned().collect();
    for run_id in driving {
        end_finished_nodes(&run_id, now)?;
        start_ready_nodes(&run_id, now)?;
        skip_decided_nodes(&run_id, now)?;
        if let Some((status, reason)) = with_workflows(|workflows| workflows.run_would_end(&run_id))
        {
            end_run(&run_id, status, reason, now)?;
        }
    }
    Ok(!DRIVING.lock().unwrap().is_empty())
}

/// Reads every Todo a `running` node bound, and ends the nodes whose Todo
/// has ended. An UNREADABLE Todo — its store swapped, its provider gone —
/// ends that node `failed` with what the store saw, because a Todo nobody
/// can read is not a reason to keep claiming the node is running.
fn end_finished_nodes(run_id: &str, now: u64) -> Result<(), WorkflowError> {
    let bound: Vec<(String, String, String, String)> = with_workflows(|workflows| {
        workflows.run(run_id).map_or_else(Vec::new, |record| {
            record
                .nodes
                .iter()
                .filter(|node| node.state == NodeState::Running)
                .filter_map(|node| {
                    Some((
                        node.node_id.clone(),
                        node.todo_store.clone()?,
                        node.todo_id.clone()?,
                        node.dispatch_id.clone()?,
                    ))
                })
                .collect()
        })
    });
    for (node_id, store, todo_id, dispatch_id) in bound {
        let read = read_todo(&store, &todo_id);
        let record = match read {
            Ok(record) => record,
            Err(error) => {
                end_node(
                    run_id,
                    &node_id,
                    NodeState::Failed,
                    Some(translate::lost_todo_reason(&error.message)),
                    String::new(),
                    now,
                )?;
                continue;
            }
        };
        // Not an ending: the dispatch is still in flight, and nothing is
        // claimed about a node whose work has not come back.
        let Some((state, reason, answer)) = translate::ended(&record, &dispatch_id) else {
            continue;
        };
        end_node(run_id, &node_id, state, reason, answer, now)?;
    }
    Ok(())
}

/// One `get` on the todos seam's definition, decoded to that seam's own
/// record.
fn read_todo(store: &str, todo_id: &str) -> Result<jinn_todo::TodoRecord, WorkflowError> {
    let value = todo_call(
        &jinn_todo::store_contract(store),
        jinn_todo::OP_GET,
        &serde_json::to_vec(&serde_json::json!({ "todo-id": todo_id })).expect("encodes"),
    )?;
    serde_json::from_value(value).map_err(|error| {
        WorkflowError::new(ErrorCode::Failed, format!("malformed Todo record: {error}"))
    })
}

/// Ends one node: the ANSWER its Todo carried joins the node's record
/// before the line that ends it is written, so the line carries the answer
/// and a replay reads it back. The state move itself is the ordinary
/// journal-registry-bus landing.
fn end_node(
    run_id: &str,
    node_id: &str,
    state: NodeState,
    reason: Option<String>,
    answer: String,
    now: u64,
) -> Result<(), WorkflowError> {
    if !answer.is_empty() {
        with_workflows(|workflows| {
            if let Some(node) = workflows.node_mut(run_id, node_id) {
                node.answer = answer;
            }
        });
    }
    move_node(run_id, node_id, state, None, reason, now)
}

/// Starts every node whose turn has come.
fn start_ready_nodes(run_id: &str, now: u64) -> Result<(), WorkflowError> {
    for node_id in with_workflows(|workflows| workflows.ready_nodes(run_id)) {
        start_node(run_id, &node_id, now)?;
    }
    Ok(())
}

/// Starts one node, in the order restart honesty requires.
///
/// The `pending -> running` line — carrying nothing yet, because nothing
/// has been opened — lands on disk BEFORE the Todo store is asked for
/// anything. So a daemon that dies at ANY point after this comes back to a
/// node declared `running` with no ending, which the recovery records as
/// interrupted with a reason. Opening the Todo first and recording after
/// would leave a window where a crash loses the node entirely, and an
/// absent node is a worse answer than an interrupted one: it says nothing
/// happened.
///
/// A [`NodeKind::Checkpoint`] has no work of its own, so it moves
/// `pending -> running` and `running -> done` in this same wake — an
/// entry, a join or an exit is expressible without pretending it
/// dispatched anything.
fn start_node(run_id: &str, node_id: &str, now: u64) -> Result<(), WorkflowError> {
    move_node(run_id, node_id, NodeState::Running, None, None, now)?;
    let node = node_spec(run_id, node_id)?;
    match node.kind {
        NodeKind::Checkpoint => end_node(run_id, node_id, NodeState::Done, None, String::new(), now),
        NodeKind::Dispatch => match &node.todo {
            Some(binding) => open_todo(run_id, &node, binding, now),
            None => end_node(
                run_id,
                node_id,
                NodeState::Failed,
                Some(UNBOUND_NODE_REASON.to_owned()),
                String::new(),
                now,
            ),
        },
    }
}

/// Opens this node's Todo through the todos seam's DEFINITION and binds
/// what came back onto the node.
///
/// A create or a dispatch the Todo store REFUSED ends the node `failed`
/// with that store's own reason — never left running, and never silently
/// dropped: the run's ledger carries the refusal in the words the store
/// used.
fn open_todo(
    run_id: &str,
    node: &NodeSpec,
    binding: &TodoBinding,
    now: u64,
) -> Result<(), WorkflowError> {
    let contract = translate::todo_contract(binding);
    match create_and_dispatch(&contract, binding, node, run_id) {
        Ok((todo_id, dispatch_id)) => {
            with_workflows(|workflows| {
                if let Some(bound) = workflows.node_mut(run_id, &node.id) {
                    bound.todo_store = Some(binding.store.clone());
                    bound.todo_id = Some(todo_id);
                    bound.dispatch_id = Some(dispatch_id);
                }
            });
            Ok(())
        }
        Err(error) => end_node(
            run_id,
            &node.id,
            NodeState::Failed,
            Some(error.message),
            String::new(),
            now,
        ),
    }
}

/// The two calls on the todos seam that a dispatch node is: `create`, then
/// `dispatch`. Both payloads are the definition's own translation — this
/// store builds no Todo of its own.
fn create_and_dispatch(
    contract: &str,
    binding: &TodoBinding,
    node: &NodeSpec,
    run_id: &str,
) -> Result<(String, String), WorkflowError> {
    let created = todo_call(
        contract,
        jinn_todo::OP_CREATE,
        &serde_json::to_vec(&translate::create_request(binding, node, run_id)).expect("encodes"),
    )?;
    let todo_id = created
        .get("todo-id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            WorkflowError::new(
                ErrorCode::Failed,
                "the Todo store created a Todo and named no todo-id",
            )
        })?
        .to_owned();
    let dispatched = todo_call(
        contract,
        jinn_todo::OP_DISPATCH,
        &serde_json::to_vec(&translate::dispatch_request(binding, &todo_id)).expect("encodes"),
    )?;
    let record: jinn_todo::TodoRecord = serde_json::from_value(dispatched).map_err(|error| {
        WorkflowError::new(
            ErrorCode::Failed,
            format!("malformed Todo record after dispatch: {error}"),
        )
    })?;
    // The dispatch this node owns is the LAST one the store recorded — a
    // Todo's dispatches are oldest first — and it is FOUND rather than
    // assumed: a record holding none is a typed failure, because a node
    // that cannot name its dispatch could never read its ending.
    let dispatch_id = record
        .dispatches
        .last()
        .map(|dispatch| dispatch.dispatch_id.clone())
        .ok_or_else(|| {
            WorkflowError::new(
                ErrorCode::Failed,
                "the Todo store accepted a dispatch and its record holds none",
            )
        })?;
    Ok((todo_id, dispatch_id))
}

/// Skips every node the graph has DECIDED will never run. Skipping is a
/// positive reading of a decided graph — every inbound edge settled and
/// none of them followed — and the note says exactly that, so a reader is
/// never left wondering whether the node was forgotten.
fn skip_decided_nodes(run_id: &str, now: u64) -> Result<(), WorkflowError> {
    for node_id in with_workflows(|workflows| workflows.skipped_nodes(run_id)) {
        move_node(
            run_id,
            &node_id,
            NodeState::Skipped,
            None,
            Some(
                "every edge into this node was decided and none of them was followed, so this \
                 run never reaches it"
                    .to_owned(),
            ),
            now,
        )?;
    }
    Ok(())
}

// ---- reads -----------------------------------------------------------

fn no_run(run_id: &str) -> WorkflowError {
    WorkflowError::new(ErrorCode::NotFound, format!("{run_id:?} is not here"))
}

fn run_of(run_id: &str) -> Result<serde_json::Value, WorkflowError> {
    with_workflows(|workflows| workflows.run(run_id).cloned())
        .map(|record| serde_json::to_value(record).expect("encodes"))
        .ok_or_else(|| no_run(run_id))
}

/// One node as the registry holds it now — what a journal line is written
/// from.
fn node_now(run_id: &str, node_id: &str) -> Result<NodeRun, WorkflowError> {
    with_workflows(|workflows| {
        workflows
            .run(run_id)
            .and_then(|record| record.node(node_id))
            .cloned()
    })
    .ok_or_else(|| {
        WorkflowError::new(
            ErrorCode::NotFound,
            format!("node {node_id:?} is not in run {run_id:?}"),
        )
    })
}

/// One node as the revision this run PINNED declares it. The authority on
/// what a node does is the run's own spec, never the workflow's current
/// definition.
fn node_spec(run_id: &str, node_id: &str) -> Result<NodeSpec, WorkflowError> {
    with_workflows(|workflows| {
        workflows
            .run(run_id)
            .and_then(|record| record.spec.node(node_id))
            .cloned()
    })
    .ok_or_else(|| {
        WorkflowError::new(
            ErrorCode::NotFound,
            format!(
                "node {node_id:?} is not in the revision run {run_id:?} pinned, which is what \
                 it executes"
            ),
        )
    })
}

fn decode<T: serde::de::DeserializeOwned>(payload: &[u8], what: &str) -> Result<T, WorkflowError> {
    serde_json::from_slice(payload).map_err(|error| {
        WorkflowError::new(
            ErrorCode::Invalid,
            format!("malformed {what} request: {error}"),
        )
    })
}

/// Every operation but `describe`, which each provider answers with its
/// own declaration.
pub fn dispatch(operation: &str, payload: &[u8]) -> Answer {
    let outcome = match operation {
        OP_DEFINE => on_define(payload),
        OP_START => on_start(payload),
        OP_NODE_STATE => on_node_state(payload),
        OP_CANCEL => on_cancel(payload),
        OP_GET => decode::<WorkflowRequest>(payload, "get").and_then(|request| {
            with_workflows(|workflows| workflows.workflow(&request.workflow_id))
                .map(|record| serde_json::to_value(record).expect("encodes"))
                .ok_or_else(|| {
                    WorkflowError::new(
                        ErrorCode::NotFound,
                        format!(
                            "{:?} is not a workflow in this store",
                            request.workflow_id
                        ),
                    )
                })
        }),
        OP_LIST => Ok(serde_json::to_value(with_workflows(|workflows| {
            workflows.list_workflows()
        }))
        .expect("encodes")),
        OP_GET_RUN => {
            decode::<RunRequest>(payload, "get-run").and_then(|request| run_of(&request.run_id))
        }
        OP_LIST_RUNS => decode::<ListRunsRequest>(payload, "list-runs").map(|request| {
            serde_json::to_value(with_workflows(|workflows| workflows.list_runs(&request)))
                .expect("encodes")
        }),
        OP_EVENTS => decode::<EventsRequest>(payload, "events").and_then(|request| {
            with_workflows(|workflows| {
                workflows.events_since(&request.run_id, request.after, request.limit)
            })
            .map(|page| serde_json::to_value(page).expect("encodes"))
            .ok_or_else(|| no_run(&request.run_id))
        }),
        other => Err(WorkflowError::new(
            ErrorCode::Invalid,
            format!("unknown operation {other:?}"),
        )),
    };
    match outcome {
        Ok(value) => Answer::ok(value),
        Err(error) => Answer::error(error),
    }
}

/// The store's own `describe`: what it is, and what it PROMISES about
/// where its records live.
pub fn describe(provider: &str, durable: bool) -> Answer {
    let config = config();
    let mut extra = Extensions::new();
    extra.insert(
        "emit-failures".to_owned(),
        serde_json::json!(*EMIT_FAILURES.lock().unwrap()),
    );
    extra.insert("poll-ms".to_owned(), serde_json::json!(config.poll_ms));
    extra.insert(
        "healed-tails".to_owned(),
        serde_json::json!(*HEALED_TAILS.lock().unwrap()),
    );
    extra.insert(
        "driving".to_owned(),
        serde_json::json!(DRIVING.lock().unwrap().len()),
    );
    Answer::ok(serde_json::json!({
        "api-version": API_VERSION,
        "store": config.store,
        "provider": provider,
        // The store's own word, so a consumer gates on a declaration
        // rather than inferring durability from a package name.
        "durable": durable,
        "workflows": with_workflows(|workflows| workflows.workflow_ids().count()),
        "runs": with_workflows(|workflows| workflows.run_ids().count()),
        "extra": extra,
    }))
}

/// Records what every adopted run owes before this store may serve.
///
/// A run read back declaring nodes `running`, or declaring itself
/// `running`, is a run the last incarnation was driving when the daemon
/// stopped. Nothing here is driving it now — [`DRIVING`] is restored by
/// nothing — so the honest record is that it was interrupted, and the
/// definition says exactly what that costs
/// ([`Workflows::plan_recovery`]): a real `node-state-changed` line per
/// open node and a `run-ended` line for the run, appended AFTER the lines
/// already there. Neither replaces anything: a reader still sees that the
/// work was started and now also sees that the daemon died on it.
///
/// A recovery whose append FAILS fails the activation. A store that cannot
/// record an ending must not serve a `running` no durable line justifies:
/// the alternative is a run that reads as live to every caller and can
/// never finish, which is the one answer this seam refuses to give.
///
/// The recovery is recorded and not announced. It is what this store found
/// on disk, not something that happened while anyone was listening; the
/// record carries it, and `get-run` reports it to whoever asks.
fn recover_all() -> Result<(), WorkflowError> {
    let now = now_ms()?;
    let run_ids: Vec<String> = with_workflows(|workflows| {
        workflows.run_ids().map(str::to_owned).collect()
    });
    for run_id in run_ids {
        let recovery = with_workflows(|workflows| workflows.plan_recovery(&run_id, now));
        for change in &recovery.node_changes {
            record_change(&run_id, change)?;
        }
        if let Some((status, reason)) = recovery.run_end {
            // A run whose nodes all reached `done` before the daemon
            // stopped ends `done` and carries no reason, because `done`
            // is the one ending that explains itself.
            let reason = (!reason.trim().is_empty()).then_some(reason);
            record_run_end(&run_id, status, reason, now)?;
        }
    }
    Ok(())
}

/// The shared half of `activate`, IN THE ONE ORDER an honest store may use:
///
/// 1. read the config;
/// 2. build the registry;
/// 3. adopt what the last incarnation left behind (`journal::adopt_all` —
///    replay, heal a torn tail, install);
/// 4. record every adopted run's [`recover_all`];
/// 5. and only THEN may the caller `services::provide`.
///
/// The order is the whole of the guarantee. `running` exists in this
/// store's memory for the length of one `activate`, before a single
/// caller can reach it, and never afterwards — so no reader is ever shown
/// a state the journal does not justify. A provision made before step 4
/// would open exactly that window.
///
/// The ephemeral store's `journal::adopt_all` writes and reads nothing, so
/// its registry is empty here and its recovery is empty too. That is the
/// honest state of a store that holds nothing across incarnations, not a
/// step it skipped.
///
/// # Errors
///
/// A malformed config, an empty store id, a journal this store could not
/// read, or a recovery line it could not append.
pub fn activate(config_bytes: &[u8]) -> Result<StoreConfig, String> {
    let config: StoreConfig = serde_json::from_slice(config_bytes)
        .map_err(|error| format!("malformed config: {error}"))?;
    if config.store.is_empty() {
        return Err("config.store is the store id this provider serves; it cannot be empty".into());
    }
    *WORKFLOWS.lock().unwrap() = Some(Workflows::new(config.store.clone()));
    // A node's Todo belongs to the incarnation that dispatched it, so a
    // fresh incarnation drives nothing and adopts no run into flight.
    *DRIVING.lock().unwrap() = BTreeSet::new();
    *DEFERRED.lock().unwrap() = Vec::new();
    *EMIT_FAILURES.lock().unwrap() = 0;
    *HEALED_TAILS.lock().unwrap() = 0;
    *CONFIG.lock().unwrap() = Some(config.clone());
    journal::adopt_all(&config).map_err(|error| error.message)?;
    recover_all().map_err(|error| error.message)?;
    Ok(config)
}

/// One wake: everything held goes on the wire, every run being driven is
/// polled, and the poll re-arms while anything is still being driven.
///
/// # Errors
///
/// The store could not re-arm its own poll.
pub fn on_wake(now: u64) -> Result<bool, String> {
    flush_deferred();
    let driving = poll_once(now).map_err(|error| error.message)?;
    flush_deferred();
    if driving {
        wake_at(now.saturating_add(config().poll_ms)).map_err(|error| error.message)?;
    }
    Ok(driving)
}
