//! The DURABLE journal: one append-only JSONL document per workflow (its
//! revisions) and one per run (its whole life), and the replay that reads
//! them back honestly.
//!
//! # History is append-only, and the reader enforces it
//!
//! A node-state change is a NEW line. Nothing in this seam edits a line
//! that was written, and nothing removes one — which is why a run's
//! `history` is the document's own state lines in order, not a field
//! somebody kept up to date. That makes the READER the place where an
//! illegal history is caught:
//!
//! - A [`Kind::NodeStateChanged`] whose `from` is not where the node
//!   actually stood is REFUSED. A line that begins from a state the node
//!   was never in is not a move this seam made.
//! - A move the TABLE does not admit is REFUSED
//!   ([`NodeState::transition`]). The writer refuses illegal moves, so a
//!   document holding one did not come from this seam, and believing it
//!   would let `done` — the claim that a step was carried out — be
//!   reached by a path the law forbids.
//! - A [`Kind::RunEnded`] whose status is not terminal is REFUSED, for
//!   the same reason: a run that reads as live after a restart is a run
//!   nothing will ever finish.
//! - A run whose `run-started` names a node that its own PINNED spec does
//!   not contain is REFUSED. The pin is the authority on what the run is
//!   executing, so a state line for a node outside it is damage.
//!
//! # `running` is declared, and a store that cannot end it does not serve
//!
//! A replay reports what the document SAYS, including a node still
//! declared `running` and a run still declared `running` — anything else
//! would be the reader inventing a line nobody wrote. What makes "never
//! eternally `running`" true is not the reader: it is the ORDER a durable
//! store activates in. It replays, plans the recovery
//! ([`crate::Workflows::plan_recovery`]) for every run left open, appends
//! those `node-state-changed` and `run-ended` lines, and only THEN
//! provides its contract. A store whose recovery append is refused fails
//! to activate rather than serving a `running` no durable line justifies.
//! So `running` exists in this crate's memory for the length of one
//! `activate`, before anything can call it, and never afterwards.
//!
//! [`Replayed::open_nodes`] and [`Replayed::run_open`] are what that
//! ordering keys on, so the obligation is a value the caller holds rather
//! than a rule it must remember.
//!
//! # Absence and corruption are not answered the same way
//!
//! A line that does not decode is not a record. The reader admits a torn
//! TAIL — the last line, written short — as ABSENCE, because a
//! half-written record must read as "absent or complete" and never as a
//! damaged one. A hole anywhere EARLIER is corruption and is REFUSED:
//! answering the two the same way would let real damage masquerade as a
//! clean stop. (`FINDINGS.md` #34 is the kernel side: `jinn:fs` cannot
//! drop a suffix, so a store that tolerates a tail has to rewrite the
//! document to stay readable.)

use serde::{Deserialize, Serialize};

use crate::{
    Definition, Extensions, NodeChange, NodeKind, NodeRun, NodeState, RefusedChange, RunStatus,
    WorkflowSpec, API_VERSION, INTERRUPTED_NODE_REASON, INTERRUPTED_RUN_REASON,
};

/// What one journal line records. A CLOSED value space: a kind this
/// version cannot name is a REFUSAL, because a journal whose unknown
/// lines were skipped would replay a different run than it holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// A workflow revision was recorded. Only in a workflow document.
    Defined,
    /// A run opened; the line carries the PINNED revision and its whole
    /// spec. Only in a run document, and always its first line.
    RunStarted,
    /// A node moved; the line carries `from`, `to` and the actor.
    NodeStateChanged,
    /// A node-state move was REFUSED. Recorded because an attempt is a
    /// fact.
    NodeTransitionRefused,
    /// The run ended; the line carries its terminal status.
    RunEnded,
}

/// One line of a journal.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Record {
    #[serde(default)]
    pub api_version: String,
    pub kind: Kind,
    pub at_ms: u64,
    /// Present on `defined` and `run-started`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<WorkflowSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec_digest: Option<String>,
    /// Present on `run-started`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<Extensions>,
    /// Present on both node lines.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<NodeState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<NodeState>,
    /// The principal who asked, where one was declared. Absence is
    /// absence — never a principal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// What a `dispatch` node bound, carried on the move that bound it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub todo_store: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub todo_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    /// Present on `run-ended` — the PROOF a terminal run status rests on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<RunStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl Record {
    fn new(kind: Kind, at_ms: u64) -> Self {
        Self {
            api_version: API_VERSION.to_owned(),
            kind,
            at_ms,
            workflow_id: None,
            revision: None,
            spec: None,
            spec_digest: None,
            input: None,
            node_id: None,
            from: None,
            to: None,
            actor: None,
            note: None,
            todo_store: None,
            todo_id: None,
            dispatch_id: None,
            answer: None,
            status: None,
            reason: None,
            extra: Extensions::new(),
        }
    }

    /// The `defined` line: one immutable revision.
    #[must_use]
    pub fn defined(definition: &Definition) -> Self {
        Self {
            workflow_id: Some(definition.workflow_id.clone()),
            revision: Some(definition.revision),
            spec: Some(definition.spec.clone()),
            spec_digest: Some(definition.spec_digest.clone()),
            actor: definition.actor.clone(),
            ..Self::new(Kind::Defined, definition.defined_ms)
        }
    }

    /// The `run-started` line — the PIN, written whole. A run that could
    /// not write this line is not a run: nothing is opened, and a replay
    /// has nothing to disagree with.
    #[must_use]
    pub fn run_started(
        workflow_id: &str,
        revision: u64,
        spec_digest: &str,
        spec: &WorkflowSpec,
        input: &Extensions,
        actor: Option<&str>,
        at_ms: u64,
    ) -> Self {
        Self {
            workflow_id: Some(workflow_id.to_owned()),
            revision: Some(revision),
            spec: Some(spec.clone()),
            spec_digest: Some(spec_digest.to_owned()),
            input: Some(input.clone()),
            actor: actor.map(str::to_owned),
            ..Self::new(Kind::RunStarted, at_ms)
        }
    }

    /// The `node-state-changed` line. The move is checked HERE too, so an
    /// illegal transition cannot be written even by a caller that skipped
    /// the registry.
    ///
    /// # Errors
    ///
    /// `from -> to` is not in the table.
    pub fn node_state_changed(change: &NodeChange, node: &NodeRun) -> Result<Self, String> {
        change
            .from
            .transition(change.to)
            .map_err(|refusal| refusal.message())?;
        Ok(Self {
            node_id: Some(change.node_id.clone()),
            from: Some(change.from),
            to: Some(change.to),
            actor: change.actor.clone(),
            note: change.note.clone(),
            todo_store: node.todo_store.clone(),
            todo_id: node.todo_id.clone(),
            dispatch_id: node.dispatch_id.clone(),
            answer: (!node.answer.is_empty()).then(|| node.answer.clone()),
            ..Self::new(Kind::NodeStateChanged, change.at_ms)
        })
    }

    /// The `node-transition-refused` line: an attempt, recorded.
    #[must_use]
    pub fn node_transition_refused(refused: &RefusedChange) -> Self {
        Self {
            node_id: Some(refused.node_id.clone()),
            from: Some(refused.from),
            to: Some(refused.to),
            actor: refused.actor.clone(),
            ..Self::new(Kind::NodeTransitionRefused, refused.at_ms)
        }
    }

    /// The `run-ended` line: the ONLY proof a terminal run status exists.
    ///
    /// # Errors
    ///
    /// The status is [`RunStatus::Running`], or an ending that needs a
    /// reason carries none.
    pub fn run_ended(status: RunStatus, reason: Option<&str>, at_ms: u64) -> Result<Self, String> {
        if !status.is_terminal() {
            return Err(format!(
                "a journal records a run's END, and {} is not an ending",
                status.tag()
            ));
        }
        if status.needs_reason() && reason.is_none_or(|reason| reason.trim().is_empty()) {
            return Err(format!(
                "a run that ended {} carries a reason, so no reader has to invent one",
                status.tag()
            ));
        }
        Ok(Self {
            status: Some(status),
            reason: reason.map(str::to_owned),
            ..Self::new(Kind::RunEnded, at_ms)
        })
    }

    /// The line as it goes into the document, newline-terminated. The
    /// TERMINATOR is what makes a short write detectable as a tail.
    ///
    /// # Panics
    ///
    /// Never in practice: the seam's own types all encode.
    #[must_use]
    pub fn line(&self) -> Vec<u8> {
        let mut bytes = serde_json::to_vec(self).expect("a journal record encodes");
        bytes.push(b'\n');
        bytes
    }
}

/// What a RUN's journal replayed back into.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Replayed {
    pub workflow_id: String,
    pub revision: u64,
    pub spec_digest: String,
    /// The PINNED spec, from the run's own first line. The authority on
    /// what this run executes.
    pub spec: WorkflowSpec,
    pub input: Extensions,
    pub actor: Option<String>,
    pub started_ms: u64,
    pub nodes: Vec<NodeRun>,
    pub history: Vec<NodeChange>,
    pub refused: Vec<RefusedChange>,
    /// What the document SAYS the run's status is. `running` here is a
    /// run the recovery owes an ending — see [`Self::run_open`].
    pub status: RunStatus,
    pub reason: Option<String>,
    pub ended_ms: Option<u64>,
    /// How many trailing bytes were an unterminated tail and read as
    /// absence. Reported rather than swallowed: a store that discards
    /// bytes says so.
    pub torn_tail_bytes: usize,
}

impl Replayed {
    /// The nodes this replay left declared `running` — exactly the ones a
    /// store must record ENDED before it may serve. See the module doc.
    #[must_use]
    pub fn open_nodes(&self) -> Vec<&NodeRun> {
        self.nodes
            .iter()
            .filter(|node| node.state == NodeState::Running)
            .collect()
    }

    /// Whether this run has no recorded ending.
    #[must_use]
    pub fn run_open(&self) -> bool {
        self.status == RunStatus::Running
    }
}

/// Replays a WORKFLOW document into its revisions, oldest first.
///
/// # Errors
///
/// A line that does not decode anywhere but at the very end, a line that
/// is not `defined`, a revision that is not one more than the last, or a
/// revision whose carried spec does not match its own digest.
pub fn replay_workflow(document: &[u8]) -> Result<(Vec<Definition>, usize), String> {
    let (body, torn_tail_bytes) = split_tail(document);
    let mut revisions: Vec<Definition> = Vec::new();
    for (index, line) in body.split(|byte| *byte == b'\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        let line_no = index + 1;
        let record: Record = serde_json::from_slice(line)
            .map_err(|error| format!("journal line {line_no}: {error}"))?;
        if record.kind != Kind::Defined {
            return Err(format!(
                "journal line {line_no}: a workflow's journal holds revisions, and this line \
                 is {:?}",
                record.kind
            ));
        }
        let revision = record
            .revision
            .ok_or_else(|| format!("journal line {line_no}: a revision carries its number"))?;
        let expected = revisions.len() as u64 + 1;
        // A revision is never reused and never replaced (`crate::revision`),
        // so a document whose numbers skip or repeat did not come from
        // this seam.
        if revision != expected {
            return Err(format!(
                "journal line {line_no}: revisions are consecutive from 1 and this one is \
                 {revision} where {expected} was due"
            ));
        }
        let definition = Definition {
            api_version: record.api_version,
            workflow_id: record.workflow_id.unwrap_or_default(),
            revision,
            spec: record.spec.unwrap_or_default(),
            spec_digest: record.spec_digest.unwrap_or_default(),
            defined_ms: record.at_ms,
            actor: record.actor,
            extra: Extensions::new(),
        };
        if !definition.digest_matches() {
            return Err(format!(
                "journal line {line_no}: revision {revision}'s digest does not match the spec \
                 it carries, so one of the two was not written by this seam"
            ));
        }
        revisions.push(definition);
    }
    Ok((revisions, torn_tail_bytes))
}

/// Replays a RUN document.
///
/// # Errors
///
/// A line that does not decode anywhere but at the very end (a hole, not
/// a tear), a document whose first record is not `run-started`, a state
/// move that is illegal or begins from the wrong state, a line naming a
/// node the pinned spec does not contain, a `run-ended` claiming a
/// non-terminal status, or a second ending after one already landed.
pub fn replay(document: &[u8]) -> Result<Replayed, String> {
    let (body, torn_tail_bytes) = split_tail(document);
    let mut replayed = Replayed {
        torn_tail_bytes,
        ..Replayed::default()
    };
    let mut opened = false;
    for (index, line) in body.split(|byte| *byte == b'\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        let line_no = index + 1;
        let record: Record = serde_json::from_slice(line)
            .map_err(|error| format!("journal line {line_no}: {error}"))?;
        if !opened && record.kind != Kind::RunStarted {
            return Err(format!(
                "journal line {line_no}: a run's journal opens with the run it records, and \
                 this one opens with {:?}",
                record.kind
            ));
        }
        opened = true;
        apply(&mut replayed, record, line_no)?;
    }
    Ok(replayed)
}

fn split_tail(document: &[u8]) -> (&[u8], usize) {
    match document.iter().rposition(|byte| *byte == b'\n') {
        Some(last) => (&document[..=last], document.len() - last - 1),
        None => (&document[..0], document.len()),
    }
}

fn apply(replayed: &mut Replayed, record: Record, line: usize) -> Result<(), String> {
    let at_ms = record.at_ms;
    match record.kind {
        Kind::Defined => {
            return Err(format!(
                "journal line {line}: a `defined` revision belongs in the workflow's own \
                 document, not in a run's"
            ))
        }
        Kind::RunStarted => {
            if replayed.started_ms != 0 || !replayed.nodes.is_empty() {
                return Err(format!("journal line {line}: this run already started"));
            }
            let spec = record.spec.ok_or_else(|| {
                format!(
                    "journal line {line}: a run pins the revision it executes, and this line \
                     carries no spec"
                )
            })?;
            replayed.workflow_id = record.workflow_id.unwrap_or_default();
            replayed.revision = record.revision.unwrap_or_default();
            replayed.spec_digest = record.spec_digest.unwrap_or_default();
            replayed.input = record.input.unwrap_or_default();
            replayed.actor = record.actor;
            replayed.started_ms = at_ms;
            // The run's nodes are the PINNED revision's nodes. Nothing
            // else could be: the definition may have been edited since,
            // and this run does not execute the edit.
            replayed.nodes = spec
                .nodes
                .iter()
                .map(|node| NodeRun {
                    node_id: node.id.clone(),
                    kind: node.kind,
                    ..NodeRun::default()
                })
                .collect();
            replayed.spec = spec;
            replayed.status = RunStatus::Running;
        }
        Kind::NodeStateChanged => {
            let node_id = required_node(record.node_id, line)?;
            let from = required_state(record.from, line, "from")?;
            let to = required_state(record.to, line, "to")?;
            if replayed.status.is_terminal() {
                return Err(format!(
                    "journal line {line}: this run already ended, and a node cannot move \
                     after it"
                ));
            }
            let node = replayed
                .nodes
                .iter_mut()
                .find(|node| node.node_id == node_id)
                .ok_or_else(|| {
                    format!(
                        "journal line {line}: node {node_id:?} is not in the revision this run \
                         pinned"
                    )
                })?;
            // A line that starts from somewhere this node has never been
            // is not a move this seam made — a second writer, or damage.
            if from != node.state {
                return Err(format!(
                    "journal line {line}: a move from {} but node {node_id:?} stood at {}",
                    from.tag(),
                    node.state.tag()
                ));
            }
            // The table, on the way back in. See the module doc.
            from.transition(to)
                .map_err(|refusal| format!("journal line {line}: {}", refusal.message()))?;
            node.state = to;
            if to == NodeState::Running {
                node.started_ms = Some(at_ms);
            }
            if to.is_terminal() {
                node.ended_ms = Some(at_ms);
            }
            // An ending that needs a reason and carries none keeps the
            // note the line does carry rather than reading as an ending
            // nobody can explain.
            node.reason = match (to.needs_reason(), record.note.clone()) {
                (false, note) => note,
                (true, Some(note)) => Some(note),
                (true, None) => Some(format!(
                    "this node ended {} and recorded no reason",
                    to.tag()
                )),
            };
            if record.todo_store.is_some() {
                node.todo_store = record.todo_store.clone();
            }
            if record.todo_id.is_some() {
                node.todo_id = record.todo_id.clone();
            }
            if record.dispatch_id.is_some() {
                node.dispatch_id = record.dispatch_id.clone();
            }
            if let Some(answer) = record.answer.clone() {
                node.answer = answer;
            }
            replayed.history.push(NodeChange {
                seq: replayed.history.len() as u64,
                node_id,
                from,
                to,
                actor: record.actor,
                note: record.note,
                at_ms,
                extra: Extensions::new(),
            });
        }
        Kind::NodeTransitionRefused => {
            let node_id = required_node(record.node_id, line)?;
            replayed.refused.push(RefusedChange {
                seq: replayed.refused.len() as u64,
                node_id,
                from: required_state(record.from, line, "from")?,
                to: required_state(record.to, line, "to")?,
                actor: record.actor,
                at_ms,
                extra: Extensions::new(),
            });
        }
        Kind::RunEnded => {
            let status = record
                .status
                .ok_or_else(|| format!("journal line {line}: an ended run carries a status"))?;
            // The WRITER refuses a non-terminal ending, so the READER
            // refuses one too: a line claiming a run ended `running`
            // would hand back a run nothing will ever finish.
            if !status.is_terminal() {
                return Err(format!(
                    "journal line {line}: a run's END cannot be {}",
                    status.tag()
                ));
            }
            if replayed.status.is_terminal() {
                return Err(format!(
                    "journal line {line}: this run already ended {}, and an ending is not \
                     rewritten",
                    replayed.status.tag()
                ));
            }
            replayed.status = status;
            replayed.reason = match (status.needs_reason(), record.reason) {
                (false, _) => None,
                (true, Some(reason)) => Some(reason),
                (true, None) => Some(format!(
                    "this run ended {} and recorded no reason",
                    status.tag()
                )),
            };
            replayed.ended_ms = Some(at_ms);
        }
    }
    Ok(())
}

fn required_state(state: Option<NodeState>, line: usize, which: &str) -> Result<NodeState, String> {
    state.ok_or_else(|| format!("journal line {line}: a node move carries `{which}`"))
}

fn required_node(node_id: Option<String>, line: usize) -> Result<String, String> {
    node_id.ok_or_else(|| format!("journal line {line}: a node line carries a node-id"))
}

/// The reason a recovery records for a node left running. One string, one
/// home — see [`INTERRUPTED_NODE_REASON`].
#[must_use]
pub fn interrupted_node_reason() -> String {
    INTERRUPTED_NODE_REASON.to_owned()
}

/// The reason a recovery records for a run left open.
#[must_use]
pub fn interrupted_run_reason() -> String {
    INTERRUPTED_RUN_REASON.to_owned()
}

/// The node kind of one node of a pinned spec, for a caller that holds a
/// [`NodeRun`] and needs the spec's own word.
#[must_use]
pub fn kind_of(spec: &WorkflowSpec, node_id: &str) -> NodeKind {
    spec.node(node_id)
        .map_or(NodeKind::default(), |node| node.kind)
}

jinn_settings::closed_value_space!(Kind, "a journal record's `kind`", {
    "defined" => Self::Defined,
    "run-started" => Self::RunStarted,
    "node-state-changed" => Self::NodeStateChanged,
    "node-transition-refused" => Self::NodeTransitionRefused,
    "run-ended" => Self::RunEnded,
});

jinn_settings::additive!(Record);
