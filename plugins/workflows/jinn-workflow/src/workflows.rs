//! The run registry every run-store provider keeps: workflow ids and
//! their revisions, run ids, event sequencing, the node-state law
//! applied, and the graph walk that decides what runs next. Pure — no
//! host call, no clock of its own (the caller passes the kernel's `now`)
//! — so the seam's ledger semantics are ONE implementation with one set
//! of tests, and a provider adds only where the records live.
//!
//! # Nothing here advances until the record is durable
//!
//! Every mutation is TWO calls: a `plan_*` that computes what WOULD
//! happen and touches nothing, and a `commit_*` that folds the planned
//! record into the registry. A provider appends that record to its
//! journal BETWEEN the two, so the state this registry reports is the
//! state the log holds. A durable write that fails leaves the reported
//! state exactly where it was, and a restart replays what the live view
//! was already saying. There is no method here that advances state and
//! writes nothing — which is why the two views cannot disagree. That
//! discipline is the todos seam's, one layer down
//! (`plugins/todos/jinn-todo/src/todos.rs`), and it is the third layer in
//! a row to owe it.
//!
//! # The refusal is a recorded outcome, not an exception
//!
//! [`Workflows::plan_node_move`] answers [`Moved::Refused`] for an
//! illegal move, carrying the [`RefusedChange`] its provider must record.
//! That shape makes it impossible for a provider to refuse a move WITHOUT
//! a record in hand, because there is no code path that produces the
//! refusal and not the record. "Typed and ledgered" is then a property of
//! the type, not of a provider remembering to do both.
//!
//! # The pin is read from the run, never from the definition
//!
//! Every method that needs to know what a run is executing reads
//! [`RunRecord::spec`] — the revision the run pinned at `start`, carried
//! whole in its own `run-started` line. Nothing here consults the
//! workflow's current revision on behalf of a live run, which is what
//! makes a mid-flight edit unable to reach it (`crate::revision`).

use std::collections::BTreeMap;

use crate::journal::Replayed;
use crate::{
    run_ending, Definition, ErrorCode, Event, EventKind, EventsPage, Extensions, ListRunsRequest,
    NodeChange, NodeRun, NodeState, RefusedChange, RunEvent, RunList, RunRecord, RunStatus,
    RunSummary, StartRequest, WorkflowError, WorkflowList, WorkflowRecord, WorkflowSpec,
    WorkflowSummary, API_VERSION, INTERRUPTED_NODE_REASON, INTERRUPTED_RUN_REASON,
};

/// How many events one run's feed holds before the OLDEST are dropped. A
/// ring, because a store that kept every event of every run forever is a
/// memory leak with a schedule; the count of what was dropped is reported
/// with every page, so a reader is never told a gap is quiet.
pub const EVENT_RING: usize = 256;

/// What a node-state move WOULD do. Both arms are records the caller
/// makes DURABLE and only then commits — see the module doc.
#[derive(Clone, Debug, PartialEq)]
pub enum Moved {
    /// The move is legal. Record it, then [`Workflows::commit_node_change`].
    Changed(NodeChange),
    /// The move is refused. Record the attempt, then
    /// [`Workflows::commit_refusal`]; the error is what the caller
    /// answers with once the attempt is on the record.
    Refused(RefusedChange, WorkflowError),
}

/// What a `start` WOULD open: the run id it would carry and the
/// definition revision it would PIN, resolved once and never again.
#[derive(Clone, Debug, PartialEq)]
pub struct Started {
    pub run_id: String,
    pub definition: Definition,
    pub input: Extensions,
    pub actor: Option<String>,
}

/// What one run left open by a crash owes before its store may serve: an
/// ending for every node still declared `running`, and then an ending for
/// the run. Both are ordinary records, appended after the ones already
/// there — never an edit of one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recovery {
    pub node_changes: Vec<NodeChange>,
    /// `Some` when the run itself has no recorded ending.
    pub run_end: Option<(RunStatus, String)>,
}

impl Recovery {
    /// Whether this run owes nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.node_changes.is_empty() && self.run_end.is_none()
    }
}

struct LiveRun {
    record: RunRecord,
    seq: u64,
    events: Vec<RunEvent>,
    dropped: u64,
}

/// Every workflow and every run one store incarnation holds.
#[derive(Default)]
pub struct Workflows {
    store: String,
    minted_workflows: u64,
    minted_runs: u64,
    definitions: BTreeMap<String, Vec<Definition>>,
    runs: BTreeMap<String, LiveRun>,
}

fn no_workflow(workflow_id: &str) -> WorkflowError {
    WorkflowError::new(
        ErrorCode::NotFound,
        format!("{workflow_id:?} is not a workflow in this store"),
    )
}

fn no_run(run_id: &str) -> WorkflowError {
    WorkflowError::new(ErrorCode::NotFound, format!("{run_id:?} is not here"))
}

impl Workflows {
    /// A registry for the store `id` this provider serves.
    #[must_use]
    pub fn new(store: impl Into<String>) -> Self {
        Self {
            store: store.into(),
            minted_workflows: 0,
            minted_runs: 0,
            definitions: BTreeMap::new(),
            runs: BTreeMap::new(),
        }
    }

    /// The store id everything here belongs to.
    #[must_use]
    pub fn store(&self) -> &str {
        &self.store
    }

    /// The workflow ids this registry holds.
    pub fn workflow_ids(&self) -> impl Iterator<Item = &str> {
        self.definitions.keys().map(String::as_str)
    }

    /// The run ids this registry holds.
    pub fn run_ids(&self) -> impl Iterator<Item = &str> {
        self.runs.keys().map(String::as_str)
    }

    /// Moves the RUN counter past `run_id` without installing anything,
    /// so a later `start` cannot mint it.
    ///
    /// This is the half of the absence answer that is not about reading.
    /// A document holding no complete record is not adopted — correctly,
    /// there is no run in it — but the id it was NAMED for is then still
    /// free, and the next `start` hands it out; the store's next record
    /// would land in that document (`FINDINGS.md` #36).
    pub fn reserve_run(&mut self, run_id: &str) {
        Self::mint_past(&mut self.minted_runs, &format!("{}-r", self.store), run_id);
    }

    /// [`Workflows::reserve_run`] for the definitions lane.
    pub fn reserve_workflow(&mut self, workflow_id: &str) {
        Self::mint_past(
            &mut self.minted_workflows,
            &format!("{}-w", self.store),
            workflow_id,
        );
    }

    fn mint_past(counter: &mut u64, prefix: &str, id: &str) {
        if let Some(minted) = id
            .strip_prefix(prefix)
            .and_then(|tail| tail.parse::<u64>().ok())
        {
            *counter = (*counter).max(minted);
        }
    }

    // ---- definitions -------------------------------------------------

    /// The revision a `define` WOULD record. Touches nothing: the
    /// registry learns of it in [`Workflows::commit_define`], after its
    /// journal line is durable.
    ///
    /// A `workflow_id` that is already here appends revision `n + 1`; one
    /// that is absent mints a new workflow at revision 1. A revision is
    /// never replaced (`crate::revision`).
    ///
    /// # Errors
    ///
    /// A spec this seam will not record, or a named workflow that is not
    /// in this store.
    pub fn plan_define(
        &self,
        spec: &WorkflowSpec,
        workflow_id: Option<&str>,
        now_ms: u64,
    ) -> Result<Definition, WorkflowError> {
        spec.check()?;
        let (workflow_id, revision) = match workflow_id {
            Some(id) => {
                let revisions = self.definitions.get(id).ok_or_else(|| no_workflow(id))?;
                (id.to_owned(), revisions.len() as u64 + 1)
            }
            None => (format!("{}-w{}", self.store, self.minted_workflows + 1), 1),
        };
        Ok(Definition::new(workflow_id, revision, spec.clone(), now_ms))
    }

    /// Folds a revision whose `defined` line is already durable into the
    /// registry.
    pub fn commit_define(&mut self, definition: &Definition) {
        Self::mint_past(
            &mut self.minted_workflows,
            &format!("{}-w", self.store),
            &definition.workflow_id,
        );
        self.definitions
            .entry(definition.workflow_id.clone())
            .or_default()
            .push(definition.clone());
    }

    /// Installs a workflow's revisions read back from a durable journal.
    pub fn adopt_workflow(&mut self, workflow_id: &str, revisions: Vec<Definition>) {
        Self::mint_past(
            &mut self.minted_workflows,
            &format!("{}-w", self.store),
            workflow_id,
        );
        self.definitions.insert(workflow_id.to_owned(), revisions);
    }

    /// One workflow's whole recorded history.
    #[must_use]
    pub fn workflow(&self, workflow_id: &str) -> Option<WorkflowRecord> {
        let revisions = self.definitions.get(workflow_id)?;
        Some(WorkflowRecord {
            api_version: API_VERSION.to_owned(),
            workflow_id: workflow_id.to_owned(),
            store: self.store.clone(),
            latest_revision: revisions.iter().map(|rev| rev.revision).max().unwrap_or(0),
            revisions: revisions.clone(),
            extra: Extensions::new(),
        })
    }

    /// Every workflow this store holds.
    #[must_use]
    pub fn list_workflows(&self) -> WorkflowList {
        WorkflowList {
            api_version: API_VERSION.to_owned(),
            store: self.store.clone(),
            workflows: self
                .definitions
                .iter()
                .filter_map(|(workflow_id, revisions)| {
                    let latest = revisions.iter().max_by_key(|rev| rev.revision)?;
                    Some(WorkflowSummary {
                        workflow_id: workflow_id.clone(),
                        name: latest.spec.name.clone(),
                        latest_revision: latest.revision,
                        spec_digest: latest.spec_digest.clone(),
                        nodes: latest.spec.nodes.len(),
                        extra: Extensions::new(),
                    })
                })
                .collect(),
            extra: Extensions::new(),
        }
    }

    // ---- runs --------------------------------------------------------

    /// What a `start` WOULD open. Touches nothing.
    ///
    /// The revision is resolved HERE, once: an absent `revision` becomes
    /// the latest AT THIS MOMENT and is written into the run's own
    /// record. Nothing later re-resolves it, which is the whole of the
    /// pin.
    ///
    /// # Errors
    ///
    /// No such workflow, no such revision, a blank actor, or an input the
    /// pinned revision's schema does not admit.
    pub fn plan_start(&self, request: &StartRequest) -> Result<Started, WorkflowError> {
        let actor = request.attribution.check()?;
        let record = self
            .workflow(&request.workflow_id)
            .ok_or_else(|| no_workflow(&request.workflow_id))?;
        let definition = record.revision(request.revision).ok_or_else(|| {
            WorkflowError::new(
                ErrorCode::NotFound,
                match request.revision {
                    Some(revision) => format!(
                        "{:?} has no revision {revision}; its latest is {}",
                        request.workflow_id, record.latest_revision
                    ),
                    None => format!("{:?} has no revisions at all", request.workflow_id),
                },
            )
        })?;
        definition.spec.input.check(&request.input)?;
        Ok(Started {
            run_id: format!("{}-r{}", self.store, self.minted_runs + 1),
            definition: definition.clone(),
            input: request.input.clone(),
            actor,
        })
    }

    /// Folds a run whose `run-started` line is already durable into the
    /// registry. Its nodes are the PINNED revision's nodes, every one of
    /// them `pending`.
    pub fn commit_start(&mut self, started: &Started, now_ms: u64) {
        Self::mint_past(
            &mut self.minted_runs,
            &format!("{}-r", self.store),
            &started.run_id,
        );
        let spec = started.definition.spec.clone();
        let nodes = spec
            .nodes
            .iter()
            .map(|node| NodeRun {
                node_id: node.id.clone(),
                kind: node.kind,
                ..NodeRun::default()
            })
            .collect();
        self.install(RunRecord {
            api_version: API_VERSION.to_owned(),
            run_id: started.run_id.clone(),
            store: self.store.clone(),
            workflow_id: started.definition.workflow_id.clone(),
            definition_revision: started.definition.revision,
            spec_digest: started.definition.spec_digest.clone(),
            spec,
            status: RunStatus::Running,
            reason: None,
            input: started.input.clone(),
            nodes,
            history: Vec::new(),
            refused: Vec::new(),
            actor: started.actor.clone(),
            started_ms: now_ms,
            ended_ms: None,
            extra: Extensions::new(),
        });
    }

    /// Installs a run read back from a durable journal, exactly as the
    /// document says it stands — `running` included. What makes a
    /// `running` node unobservable is the ORDER a durable store activates
    /// in, not a repair here (`crate::journal`).
    pub fn adopt_run(&mut self, run_id: &str, replayed: Replayed) {
        Self::mint_past(&mut self.minted_runs, &format!("{}-r", self.store), run_id);
        self.install(RunRecord {
            api_version: API_VERSION.to_owned(),
            run_id: run_id.to_owned(),
            store: self.store.clone(),
            workflow_id: replayed.workflow_id,
            definition_revision: replayed.revision,
            spec_digest: replayed.spec_digest,
            spec: replayed.spec,
            status: replayed.status,
            reason: replayed.reason,
            input: replayed.input,
            nodes: replayed.nodes,
            history: replayed.history,
            refused: replayed.refused,
            actor: replayed.actor,
            started_ms: replayed.started_ms,
            ended_ms: replayed.ended_ms,
            extra: Extensions::new(),
        });
    }

    fn install(&mut self, record: RunRecord) {
        self.runs.insert(
            record.run_id.clone(),
            LiveRun {
                record,
                seq: 0,
                events: Vec::new(),
                dropped: 0,
            },
        );
    }

    /// One run, whole.
    #[must_use]
    pub fn run(&self, run_id: &str) -> Option<&RunRecord> {
        self.runs.get(run_id).map(|live| &live.record)
    }

    /// One node of one run, to bind a Todo onto.
    pub fn node_mut(&mut self, run_id: &str, node_id: &str) -> Option<&mut NodeRun> {
        self.runs
            .get_mut(run_id)?
            .record
            .nodes
            .iter_mut()
            .find(|node| node.node_id == node_id)
    }

    /// The runs this store holds, filtered.
    #[must_use]
    pub fn list_runs(&self, request: &ListRunsRequest) -> RunList {
        RunList {
            api_version: API_VERSION.to_owned(),
            store: self.store.clone(),
            runs: self
                .runs
                .values()
                .map(|live| &live.record)
                .filter(|record| {
                    request
                        .workflow_id
                        .as_ref()
                        .is_none_or(|id| &record.workflow_id == id)
                        && request.status.is_none_or(|status| record.status == status)
                })
                .map(|record| RunSummary {
                    run_id: record.run_id.clone(),
                    workflow_id: record.workflow_id.clone(),
                    definition_revision: record.definition_revision,
                    status: record.status,
                    reason: record.reason.clone(),
                    nodes_ended: record
                        .nodes
                        .iter()
                        .filter(|node| node.state.is_terminal())
                        .count(),
                    nodes_total: record.nodes.len(),
                    started_ms: record.started_ms,
                    ended_ms: record.ended_ms,
                    extra: Extensions::new(),
                })
                .collect(),
            extra: Extensions::new(),
        }
    }

    // ---- the graph walk ----------------------------------------------

    /// Whether every inbound edge of `node_id` is DECIDED, and whether
    /// any of them is followed. An edge is decided once its source node
    /// has ended; an undecided edge means this node's turn has not come.
    fn inbound(record: &RunRecord, node_id: &str) -> (bool, bool) {
        let mut decided = true;
        let mut followed = false;
        for edge in record.spec.edges.iter().filter(|edge| edge.to == node_id) {
            match record.node(&edge.from) {
                Some(source) if source.state.is_terminal() => {
                    followed |= edge.kind.follows(source.state);
                }
                // A source that has not ended — or, for a document this
                // seam did not write, one that is not there at all —
                // leaves the edge undecided rather than silently taken.
                _ => decided = false,
            }
        }
        (decided, followed)
    }

    /// The nodes that may START now: `pending`, every inbound edge
    /// decided, and at least one of them followed. A node with no inbound
    /// edge at all is an entry and is ready immediately.
    #[must_use]
    pub fn ready_nodes(&self, run_id: &str) -> Vec<String> {
        let Some(record) = self.run(run_id) else {
            return Vec::new();
        };
        if record.status.is_terminal() {
            return Vec::new();
        }
        record
            .nodes
            .iter()
            .filter(|node| node.state == NodeState::Pending)
            .filter(|node| {
                let has_inbound = record.spec.edges.iter().any(|edge| edge.to == node.node_id);
                if !has_inbound {
                    return true;
                }
                let (decided, followed) = Self::inbound(record, &node.node_id);
                decided && followed
            })
            .map(|node| node.node_id.clone())
            .collect()
    }

    /// The nodes that will never run in this run: `pending`, every
    /// inbound edge decided, and NONE of them followed. Skipping is a
    /// positive reading of a decided graph, never a timeout.
    #[must_use]
    pub fn skipped_nodes(&self, run_id: &str) -> Vec<String> {
        let Some(record) = self.run(run_id) else {
            return Vec::new();
        };
        if record.status.is_terminal() {
            return Vec::new();
        }
        record
            .nodes
            .iter()
            .filter(|node| node.state == NodeState::Pending)
            .filter(|node| {
                let has_inbound = record.spec.edges.iter().any(|edge| edge.to == node.node_id);
                if !has_inbound {
                    return false;
                }
                let (decided, followed) = Self::inbound(record, &node.node_id);
                decided && !followed
            })
            .map(|node| node.node_id.clone())
            .collect()
    }

    /// How this run ENDS, given the nodes it holds now — or `None` while
    /// any node can still move. Derived from what is recorded and nothing
    /// else (`crate::record::run_ending`).
    #[must_use]
    pub fn run_would_end(&self, run_id: &str) -> Option<(RunStatus, Option<String>)> {
        let record = self.run(run_id)?;
        if record.status.is_terminal() {
            return None;
        }
        run_ending(&record.nodes)
    }

    // ---- moves -------------------------------------------------------

    /// What one node-state move WOULD do — the table applied, and the
    /// record of either outcome. Touches nothing. See the module doc for
    /// why a refusal is an `Ok(Moved)`.
    ///
    /// # Errors
    ///
    /// The run is not here, the node is not in the revision this run
    /// pinned, or the run has already ended.
    pub fn plan_node_move(
        &self,
        run_id: &str,
        node_id: &str,
        to: NodeState,
        actor: Option<String>,
        note: Option<String>,
        now_ms: u64,
    ) -> Result<Moved, WorkflowError> {
        let record = self.run(run_id).ok_or_else(|| no_run(run_id))?;
        if record.status.is_terminal() {
            return Err(WorkflowError::new(
                ErrorCode::Refused,
                format!(
                    "run {run_id:?} ended {} and its nodes do not move again",
                    record.status.tag()
                ),
            ));
        }
        let node = record.node(node_id).ok_or_else(|| {
            WorkflowError::new(
                ErrorCode::NotFound,
                format!(
                    "node {node_id:?} is not in revision {} of {:?}, which is what this run \
                     executes",
                    record.definition_revision, record.workflow_id
                ),
            )
        })?;
        let from = node.state;
        match from.transition(to) {
            Ok(to) => Ok(Moved::Changed(NodeChange {
                seq: record.history.len() as u64,
                node_id: node_id.to_owned(),
                from,
                to,
                actor,
                note,
                at_ms: now_ms,
                extra: Extensions::new(),
            })),
            Err(refusal) => Ok(Moved::Refused(
                RefusedChange {
                    seq: record.refused.len() as u64,
                    node_id: node_id.to_owned(),
                    from,
                    to,
                    actor,
                    at_ms: now_ms,
                    extra: Extensions::new(),
                },
                WorkflowError::refused_transition(node_id, refusal),
            )),
        }
    }

    /// Folds a node-state move that is already durable into the registry.
    /// A run or node that is gone is a no-op rather than a panic: the
    /// record is on the log either way, and a replay is what decides.
    pub fn commit_node_change(&mut self, run_id: &str, change: &NodeChange) {
        let Some(live) = self.runs.get_mut(run_id) else {
            return;
        };
        if let Some(node) = live
            .record
            .nodes
            .iter_mut()
            .find(|node| node.node_id == change.node_id)
        {
            node.state = change.to;
            if change.to == NodeState::Running {
                node.started_ms = Some(change.at_ms);
            }
            if change.to.is_terminal() {
                node.ended_ms = Some(change.at_ms);
            }
            node.reason = match (change.to.needs_reason(), change.note.clone()) {
                (false, note) => note,
                (true, Some(note)) => Some(note),
                (true, None) => Some(format!(
                    "this node ended {} and recorded no reason",
                    change.to.tag()
                )),
            };
        }
        live.record.history.push(change.clone());
    }

    /// Folds a refused attempt that is already durable into the registry.
    /// No node moves — a refusal never was a move — but the attempt joins
    /// the record.
    pub fn commit_refusal(&mut self, run_id: &str, refused: &RefusedChange) {
        if let Some(live) = self.runs.get_mut(run_id) {
            live.record.refused.push(refused.clone());
        }
    }

    /// The ending a `run-ended` line WOULD record.
    ///
    /// # Errors
    ///
    /// The run is not here, it has already ended, the status is not
    /// terminal, or an ending that needs a reason carries none.
    pub fn plan_run_end(
        &self,
        run_id: &str,
        status: RunStatus,
        reason: Option<String>,
        _now_ms: u64,
    ) -> Result<(RunStatus, Option<String>), WorkflowError> {
        let record = self.run(run_id).ok_or_else(|| no_run(run_id))?;
        if record.status.is_terminal() {
            return Err(WorkflowError::new(
                ErrorCode::Refused,
                format!(
                    "run {run_id:?} already ended {}, and an ending is not rewritten",
                    record.status.tag()
                ),
            ));
        }
        if !status.is_terminal() {
            return Err(WorkflowError::new(
                ErrorCode::Invalid,
                format!("{} is not an ending", status.tag()),
            ));
        }
        if status.needs_reason()
            && reason
                .as_deref()
                .is_none_or(|reason| reason.trim().is_empty())
        {
            return Err(WorkflowError::new(
                ErrorCode::Invalid,
                format!(
                    "a run that ends {} carries a reason, so no reader has to invent one",
                    status.tag()
                ),
            ));
        }
        Ok((status, reason))
    }

    /// Folds a run ending that is already durable into the registry.
    pub fn commit_run_end(
        &mut self,
        run_id: &str,
        status: RunStatus,
        reason: Option<String>,
        now_ms: u64,
    ) {
        if let Some(live) = self.runs.get_mut(run_id) {
            live.record.status = status;
            live.record.reason = reason;
            live.record.ended_ms = Some(now_ms);
        }
    }

    // ---- recovery ----------------------------------------------------

    /// What one adopted run owes before its store may serve it.
    ///
    /// A node the document left declared `running` gets a real
    /// `running -> interrupted` move carrying
    /// [`INTERRUPTED_NODE_REASON`], and a run with no recorded ending
    /// gets a `run-ended` at [`RunStatus::Interrupted`] carrying
    /// [`INTERRUPTED_RUN_REASON`]. Both are NEW records appended after
    /// the ones already there — never an edit — so the whole history
    /// stays readable and a reader can see both that the work was started
    /// and that the daemon died on it.
    ///
    /// A run that already ended, or whose nodes all ended, owes an empty
    /// [`Recovery`]; a store that appends nothing for it is correct.
    #[must_use]
    pub fn plan_recovery(&self, run_id: &str, now_ms: u64) -> Recovery {
        let Some(record) = self.run(run_id) else {
            return Recovery::default();
        };
        if record.status.is_terminal() {
            return Recovery::default();
        }
        let mut recovery = Recovery::default();
        let mut nodes = record.nodes.clone();
        let open = nodes
            .iter_mut()
            .filter(|node| node.state == NodeState::Running);
        for (seq, node) in (record.history.len() as u64..).zip(open) {
            recovery.node_changes.push(NodeChange {
                seq,
                node_id: node.node_id.clone(),
                from: NodeState::Running,
                to: NodeState::Interrupted,
                actor: None,
                note: Some(INTERRUPTED_NODE_REASON.to_owned()),
                at_ms: now_ms,
                extra: Extensions::new(),
            });
            node.state = NodeState::Interrupted;
            node.reason = Some(INTERRUPTED_NODE_REASON.to_owned());
        }
        // The one run that ends anything but INTERRUPTED is the one that
        // had already finished every node cleanly and only lacked its
        // closing line — there, `done` is a claim every node's own
        // terminal record justifies.
        //
        // Every other run ends `interrupted`, INCLUDING one whose nodes
        // all ended with a failure among them. `run_ending` would derive
        // `failed` there, and this deliberately does not use it: what the
        // journal proves is that the nodes ended, not that the daemon
        // would have closed the run on that verdict, and a status is only
        // as strong as the line behind it. The nodes' own reason is
        // carried through, so nothing about WHY is lost — only the claim
        // that the run reached its own conclusion.
        recovery.run_end = Some(match run_ending(&nodes) {
            Some((RunStatus::Done, _)) if recovery.node_changes.is_empty() => {
                (RunStatus::Done, String::new())
            }
            Some((_, Some(reason))) => (RunStatus::Interrupted, reason),
            _ => (RunStatus::Interrupted, INTERRUPTED_RUN_REASON.to_owned()),
        });
        recovery
    }

    // ---- events ------------------------------------------------------

    /// The next event sequence number for a run.
    pub fn next_seq(&mut self, run_id: &str) -> u64 {
        match self.runs.get_mut(run_id) {
            Some(live) => {
                let seq = live.seq;
                live.seq += 1;
                seq
            }
            None => 0,
        }
    }

    /// Records one event against a run and answers the record to put on
    /// the bus. The sequence is minted HERE, once, so the feed a reader
    /// polls and the records a listener receives carry the same numbers.
    pub fn record_event(&mut self, run_id: &str, kind: EventKind) -> RunEvent {
        let seq = self.next_seq(run_id);
        let record = RunEvent::new(&self.store, run_id, seq, kind);
        if let Some(live) = self.runs.get_mut(run_id) {
            live.events.push(record.clone());
            if live.events.len() > EVENT_RING {
                let over = live.events.len() - EVENT_RING;
                live.events.drain(..over);
                live.dropped += over as u64;
            }
        }
        record
    }

    /// One page of a run's event feed, with the count of what the ring
    /// dropped — so a reader is never told a gap is quiet.
    #[must_use]
    pub fn events_since(
        &self,
        run_id: &str,
        after: Option<u64>,
        limit: Option<usize>,
    ) -> Option<EventsPage> {
        let live = self.runs.get(run_id)?;
        let events: Vec<RunEvent> = live
            .events
            .iter()
            .filter(|event| after.is_none_or(|after| event.seq > after))
            .take(limit.unwrap_or(EVENT_RING))
            .cloned()
            .collect();
        Some(EventsPage {
            api_version: API_VERSION.to_owned(),
            store: self.store.clone(),
            run_id: run_id.to_owned(),
            events,
            dropped: live.dropped,
            extra: Extensions::new(),
        })
    }

    /// The events one node move puts on the bus. Answered from the
    /// definition rather than assembled by a provider, so a provider
    /// cannot record a move without emitting what it means.
    #[must_use]
    pub fn move_events(change: &NodeChange) -> Vec<EventKind> {
        let mut events = Vec::new();
        if change.to == NodeState::Running {
            events.push(EventKind::NodeStarted {
                node_id: change.node_id.clone(),
            });
        }
        if change.to.is_terminal() {
            events.push(EventKind::NodeEnded {
                node_id: change.node_id.clone(),
                outcome: change.to,
                reason: change.note.clone(),
            });
        }
        events
    }
}

/// The topic-side view of one event, for a consumer that holds an
/// [`Event`] and wants its tag.
#[must_use]
pub fn event_kind(event: &Event) -> &str {
    event.kind_tag()
}
