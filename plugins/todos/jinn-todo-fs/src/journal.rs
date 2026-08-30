//! Where this store's records live: one append-only JSONL document per
//! Todo under the entry's `jinn:fs` scope, and the replay that reads them
//! back on activate.
//!
//! The record LAW — what a line is, what a replay may conclude, what a
//! torn tail means — is the definition's (`jinn_todo::journal`), with its
//! one home there. This file is only the host calls: where the document
//! sits, and how its bytes move.

use jinn_todo::journal::{replay, Record};
use jinn_todo::{
    Comment, Dispatch, ErrorCode, RefusedChange, StatusChange, TodoError, TodoSpec,
};

use crate::jinn::plugin::fs;
use crate::store::{StoreConfig, TODOS};

/// The directory a store falls back to when its entry names none. Named
/// after the seam, never a bare root: a store writes inside a place it
/// owns.
const DEFAULT_DIR: &str = "todos";

/// One Todo's document. The id is minted by the registry (`<store>-<n>`)
/// and carries no separator that could climb out of the directory; the
/// check is here anyway, because a path this store builds from an id must
/// be refusable rather than trusted.
fn document_of(todo_id: &str) -> Result<String, TodoError> {
    if todo_id.contains('/') || todo_id.contains("..") || todo_id.is_empty() {
        return Err(TodoError::new(
            ErrorCode::Invalid,
            format!("{todo_id:?} is not a Todo id this store can name a document for"),
        ));
    }
    Ok(format!("{}/{todo_id}.jsonl", dir()))
}

fn dir() -> String {
    crate::store::config()
        .dir
        .unwrap_or_else(|| DEFAULT_DIR.to_owned())
}

/// Appends one record's line. The idempotency key is EMPTY on purpose:
/// every line of an append-only log is a distinct fact, and a key would
/// make a repeat of one silently answer the recorded effect instead of
/// writing the new line (`jinn:fs`'s keyed exactly-once semantics).
fn append(todo_id: &str, record: &Record) -> Result<(), TodoError> {
    let path = document_of(todo_id)?;
    fs::append(&path, &record.line(), "").map_err(|error| {
        TodoError::new(
            ErrorCode::Failed,
            format!("the journal of {todo_id:?} could not be appended to: {error:?}"),
        )
    })
}

/// The Todo's first line: its spec.
pub fn created(todo_id: &str, spec: &TodoSpec, at_ms: u64) -> Result<(), TodoError> {
    append(todo_id, &Record::created(spec.clone(), at_ms))
}

/// A status moved. `Record::status_changed` checks the table, so a line
/// the reader would refuse cannot be written here at all.
pub fn status_changed(
    todo_id: &str,
    change: &StatusChange,
    at_ms: u64,
) -> Result<(), TodoError> {
    let record = Record::status_changed(change, at_ms)
        .map_err(|error| TodoError::new(ErrorCode::Invalid, error))?;
    append(todo_id, &record)
}

/// A move the ledger refused. Written BEFORE the caller is told, so the
/// attempt is on the record even if the answer is dropped.
pub fn transition_refused(
    todo_id: &str,
    refused: &RefusedChange,
    at_ms: u64,
) -> Result<(), TodoError> {
    append(todo_id, &Record::transition_refused(refused, at_ms))
}

/// A comment.
pub fn commented(todo_id: &str, comment: &Comment, at_ms: u64) -> Result<(), TodoError> {
    append(todo_id, &Record::commented(comment, at_ms))
}

/// A dispatch began. Written BEFORE any session is asked for anything —
/// see the crate doc: this line is what makes a crash read as
/// `interrupted` rather than as nothing at all.
pub fn dispatch_started(
    todo_id: &str,
    dispatch: &Dispatch,
    at_ms: u64,
) -> Result<(), TodoError> {
    append(todo_id, &Record::dispatch_started(dispatch, at_ms))
}

/// A dispatch ended. `Record::dispatch_ended` refuses a non-terminal
/// status, so a line that would replay as a live dispatch cannot be
/// written here at all.
pub fn dispatch_ended(todo_id: &str, dispatch: &Dispatch, at_ms: u64) -> Result<(), TodoError> {
    let record = Record::dispatch_ended(dispatch, at_ms)
        .map_err(|error| TodoError::new(ErrorCode::Invalid, error))?;
    append(todo_id, &record)
}

/// Reads every journal in the store's directory back into the registry.
///
/// An absent directory is an EMPTY store, not a failure: a first boot has
/// written nothing. A document that does not replay is a REFUSAL — the
/// activation fails and the entry stops, on the record — because a store
/// that quietly skipped a corrupt Todo would answer `list` short and no
/// one would know a piece of work was missing.
///
/// # Errors
///
/// The directory is unreadable for any reason but absence, or a document
/// in it does not replay.
pub fn adopt_all(config: &StoreConfig) -> Result<(), TodoError> {
    let dir = config.dir.clone().unwrap_or_else(|| DEFAULT_DIR.to_owned());
    let entries = match fs::list(&dir) {
        Ok(entries) => entries,
        // The TYPED absence answer of the contract, and the only one that
        // means "nothing here yet". Every other error is a store that
        // cannot see its own records and must say so.
        Err(fs::FsError::NotFound) => return Ok(()),
        Err(error) => {
            return Err(TodoError::new(
                ErrorCode::Unavailable,
                format!("this store cannot read {dir:?}: {error:?}"),
            ))
        }
    };
    for entry in entries {
        if entry.is_dir {
            continue;
        }
        let Some(todo_id) = document_id(&entry.path) else {
            continue;
        };
        let path = format!("{dir}/{}", file_name(&entry.path));
        let bytes = fs::read(&path).map_err(|error| {
            TodoError::new(
                ErrorCode::Unavailable,
                format!("the journal {path:?} could not be read: {error:?}"),
            )
        })?;
        let replayed = replay(&bytes).map_err(|error| {
            TodoError::new(
                ErrorCode::Failed,
                format!("the journal {path:?} does not replay: {error}"),
            )
        })?;
        let mut held = TODOS.lock().unwrap();
        held.as_mut()
            .expect("activate holds the registry")
            .adopt(&todo_id, replayed);
    }
    Ok(())
}

/// The last segment of a listed path.
fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The Todo id a document name carries, or `None` when the file is not
/// one of this store's journals. A stray file in the directory is not a
/// Todo and is not an error either — it is simply not ours.
fn document_id(path: &str) -> Option<String> {
    file_name(path)
        .strip_suffix(".jsonl")
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}
