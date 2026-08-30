//! Where this store's records live: two append-only JSONL families under
//! the entry's `jinn:fs` scope — one document per WORKFLOW, holding its
//! revisions, and one per RUN, holding that run's whole life — and the
//! replay that reads them back on activate.
//!
//! The record LAW — what a line is, what a replay may conclude, what a
//! torn tail means — is the definition's (`jinn_workflow::journal`), with
//! its one home there. This file is only the host calls: where the
//! documents sit, and how their bytes move.
//!
//! # Why two families and not one
//!
//! A revision is immutable and outlives every run of it; a run is a life
//! with a beginning and an ending. Keeping them apart is what lets the
//! reader be strict in both directions: a workflow's document holds
//! `defined` lines and nothing else, a run's document opens with the
//! `run-started` line that PINS what it executes and holds only that
//! run's own moves, and either reader refuses a line from the other
//! family by name (`jinn_workflow::journal::replay_workflow` and
//! `replay`).

use jinn_workflow::journal::{replay, replay_workflow, Record};
use jinn_workflow::{
    Definition, ErrorCode, NodeChange, NodeRun, RefusedChange, RunStatus, Started, WorkflowError,
};

use crate::jinn::plugin::fs;
use crate::store::{StoreConfig, WORKFLOWS};

/// The directory a store falls back to when its entry names none. Named
/// after the seam, never a bare root: a store writes inside a place it
/// owns.
const DEFAULT_DIR: &str = "workflows";

/// Where the workflow documents sit, under the store's own directory.
const WORKFLOW_DIR: &str = "workflows";
/// Where the run documents sit, under the store's own directory.
const RUN_DIR: &str = "runs";

fn dir() -> String {
    crate::store::config()
        .dir
        .unwrap_or_else(|| DEFAULT_DIR.to_owned())
}

/// One document's path. The ids are minted by the registry
/// (`<store>-w<n>` and `<store>-r<n>`) and carry no separator that could
/// climb out of the directory; the check is here anyway, because a path
/// this store builds from an id must be refusable rather than trusted.
fn document_of(family: &str, id: &str) -> Result<String, WorkflowError> {
    if id.contains('/') || id.contains("..") || id.is_empty() {
        return Err(WorkflowError::new(
            ErrorCode::Invalid,
            format!("{id:?} is not an id this store can name a document for"),
        ));
    }
    Ok(format!("{}/{family}/{id}.jsonl", dir()))
}

/// Appends one record's line. The idempotency key is EMPTY on purpose:
/// every line of an append-only log is a distinct fact, and a key would
/// make a repeat of one silently answer the recorded effect instead of
/// writing the new line (`jinn:fs`'s keyed exactly-once semantics).
fn append(family: &str, id: &str, record: &Record) -> Result<(), WorkflowError> {
    let path = document_of(family, id)?;
    fs::append(&path, &record.line(), "").map_err(|error| {
        WorkflowError::new(
            ErrorCode::Failed,
            format!("the journal of {id:?} could not be appended to: {error:?}"),
        )
    })
}

/// A workflow revision. Appended to the WORKFLOW's document, where a
/// revision is never replaced and the reader checks that the numbers run
/// consecutively from 1.
pub fn defined(definition: &Definition) -> Result<(), WorkflowError> {
    append(
        WORKFLOW_DIR,
        &definition.workflow_id,
        &Record::defined(definition),
    )
}

/// A run's first line — the PIN, written whole. A run that could not
/// write this line is not a run: nothing is opened, and a replay has
/// nothing to disagree with.
pub fn run_started(started: &Started, at_ms: u64) -> Result<(), WorkflowError> {
    let definition = &started.definition;
    append(
        RUN_DIR,
        &started.run_id,
        &Record::run_started(
            &definition.workflow_id,
            definition.revision,
            &definition.spec_digest,
            &definition.spec,
            &started.input,
            started.actor.as_deref(),
            at_ms,
        ),
    )
}

/// A node moved. `Record::node_state_changed` checks the table, so a line
/// the reader would refuse cannot be written here at all. The node is
/// passed whole because the line carries what it bound — the Todo store,
/// the Todo, the dispatch and the answer — so a replay reads those back
/// with the move that recorded them.
pub fn node_state_changed(
    run_id: &str,
    change: &NodeChange,
    node: &NodeRun,
) -> Result<(), WorkflowError> {
    let record = Record::node_state_changed(change, node)
        .map_err(|error| WorkflowError::new(ErrorCode::Invalid, error))?;
    append(RUN_DIR, run_id, &record)
}

/// A move the ledger refused. Written BEFORE the caller is told, so the
/// attempt is on the record even if the answer is dropped.
pub fn node_transition_refused(run_id: &str, refused: &RefusedChange) -> Result<(), WorkflowError> {
    append(RUN_DIR, run_id, &Record::node_transition_refused(refused))
}

/// A run ended. `Record::run_ended` refuses a non-terminal status and an
/// unexplained ending, so a line that would replay as a live run — or as
/// an ending nobody can account for — cannot be written here at all.
pub fn run_ended(
    run_id: &str,
    status: RunStatus,
    reason: Option<&str>,
    at_ms: u64,
) -> Result<(), WorkflowError> {
    let record = Record::run_ended(status, reason, at_ms)
        .map_err(|error| WorkflowError::new(ErrorCode::Invalid, error))?;
    append(RUN_DIR, run_id, &record)
}

/// Reads every journal in the store's two directories back into the
/// registry: the workflows first, then the runs.
///
/// An absent directory is an EMPTY store, not a failure: a first boot has
/// written nothing. A document that does not replay is a REFUSAL — the
/// activation fails and the entry stops, on the record — because a store
/// that quietly skipped a corrupt run would answer `list-runs` short and
/// no one would know a piece of work was missing.
///
/// # What this does BESIDES reading
///
/// **A torn tail is healed.** The reader admits an unterminated last line
/// as absence, but leaving those bytes in place would make the NEXT
/// append land on the end of the partial line — turning a tolerable tear
/// into an unreadable hole, and a run that came back fine into one that
/// refuses to replay at the boot after. `jinn:fs` can append and it can
/// rewrite, but it cannot drop a suffix (`FINDINGS.md` #34), so a
/// document that replayed with a torn tail is rewritten to its whole
/// prefix. No RECORD is lost: by the reader's own law those bytes were
/// never a record. The count is reported by `describe`, so a store that
/// discarded bytes says so.
///
/// **What it does NOT do is recover.** The recovery every adopted run
/// owes is the shared store's, immediately after this answers and before
/// the contract is provided — one home for that ordering, and the same
/// one for both providers (`store-core/store.rs`, `recover_all`).
///
/// # Errors
///
/// A directory is unreadable for any reason but absence, a document in it
/// does not replay, or a healed document could not be written back.
pub fn adopt_all(config: &StoreConfig) -> Result<(), WorkflowError> {
    let root = config.dir.clone().unwrap_or_else(|| DEFAULT_DIR.to_owned());
    for (id, path, bytes) in documents(&format!("{root}/{WORKFLOW_DIR}"))? {
        let (revisions, torn) = replay_workflow(&bytes).map_err(|error| {
            WorkflowError::new(
                ErrorCode::Failed,
                format!("the journal {path:?} does not replay: {error}"),
            )
        })?;
        if torn > 0 {
            heal(&path, &bytes, torn)?;
        }
        with_registry(|workflows| workflows.adopt_workflow(&id, revisions));
    }
    for (id, path, bytes) in documents(&format!("{root}/{RUN_DIR}"))? {
        let replayed = replay(&bytes).map_err(|error| {
            WorkflowError::new(
                ErrorCode::Failed,
                format!("the journal {path:?} does not replay: {error}"),
            )
        })?;
        if replayed.torn_tail_bytes > 0 {
            heal(&path, &bytes, replayed.torn_tail_bytes)?;
        }
        with_registry(|workflows| workflows.adopt_run(&id, replayed));
    }
    Ok(())
}

/// Every journal in one directory, as `(id, path, bytes)`. An absent
/// directory answers none.
type Document = (String, String, Vec<u8>);

fn documents(dir: &str) -> Result<Vec<Document>, WorkflowError> {
    let entries = match fs::list(dir) {
        Ok(entries) => entries,
        // The TYPED absence answer of the contract, and the only one that
        // means "nothing here yet". Every other error is a store that
        // cannot see its own records and must say so.
        Err(fs::FsError::NotFound) => return Ok(Vec::new()),
        Err(error) => {
            return Err(WorkflowError::new(
                ErrorCode::Unavailable,
                format!("this store cannot read {dir:?}: {error:?}"),
            ))
        }
    };
    let mut documents = Vec::new();
    for entry in entries {
        if entry.is_dir {
            continue;
        }
        let Some(id) = document_id(&entry.path) else {
            continue;
        };
        let path = format!("{dir}/{}", file_name(&entry.path));
        let bytes = fs::read(&path).map_err(|error| {
            WorkflowError::new(
                ErrorCode::Unavailable,
                format!("the journal {path:?} could not be read: {error:?}"),
            )
        })?;
        documents.push((id, path, bytes));
    }
    Ok(documents)
}

fn with_registry<T>(act: impl FnOnce(&mut jinn_workflow::Workflows) -> T) -> T {
    let mut held = WORKFLOWS.lock().unwrap();
    act(held.as_mut().expect("activate holds the registry"))
}

/// Rewrites a document to its whole prefix, dropping a torn tail. See
/// `adopt_all`'s doc for why the bytes are droppable and the write is not
/// an edit of history.
fn heal(path: &str, bytes: &[u8], torn: usize) -> Result<(), WorkflowError> {
    let whole = &bytes[..bytes.len().saturating_sub(torn)];
    fs::write(path, whole, "").map_err(|error| {
        WorkflowError::new(
            ErrorCode::Failed,
            format!("the torn tail of {path:?} could not be healed: {error:?}"),
        )
    })?;
    *crate::store::HEALED_TAILS.lock().unwrap() += 1;
    Ok(())
}

/// The last segment of a listed path.
fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The id a document name carries, or `None` when the file is not one of
/// this store's journals. A stray file in the directory is not a record
/// and is not an error either — it is simply not ours.
fn document_id(path: &str) -> Option<String> {
    file_name(path)
        .strip_suffix(".jsonl")
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}
