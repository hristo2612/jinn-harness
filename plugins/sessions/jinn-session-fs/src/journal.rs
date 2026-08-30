//! Where this store's records live: one append-only JSONL document per
//! session under the entry's `jinn:fs` scope, and the replay that reads
//! them back on activate.
//!
//! The record LAW — what a line is, what a replay may conclude, what a
//! torn tail means — is the definition's
//! (`jinn_session::journal`), with its one home there. This file is only
//! the host calls: where the document sits, and how its bytes move.

use jinn_session::journal::{replay, Record};
use jinn_session::{ErrorCode, SessionError, SessionSpec, Turn};

use crate::jinn::plugin::fs;
use crate::store::{StoreConfig, SESSIONS};

/// The directory a store falls back to when its entry names none. Named
/// after the seam, never a bare root: a store writes inside a place it
/// owns.
const DEFAULT_DIR: &str = "sessions";

/// One session's document. The id is minted by the registry
/// (`<store>-<n>`) and carries no separator that could climb out of the
/// directory; the check is here anyway, because a path this store builds
/// from an id must be refusable rather than trusted.
fn document_of(session_id: &str) -> Result<String, SessionError> {
    if session_id.contains('/') || session_id.contains("..") || session_id.is_empty() {
        return Err(SessionError::new(
            ErrorCode::Invalid,
            format!("{session_id:?} is not a session id this store can name a document for"),
        ));
    }
    Ok(format!("{}/{session_id}.jsonl", dir()))
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
fn append(session_id: &str, record: &Record) -> Result<(), SessionError> {
    let path = document_of(session_id)?;
    fs::append(&path, &record.line(), "").map_err(|error| {
        SessionError::new(
            ErrorCode::Failed,
            format!("the journal of {session_id:?} could not be appended to: {error:?}"),
        )
    })
}

/// The session's first line: its spec.
pub fn created(session_id: &str, spec: &SessionSpec, at_ms: u64) -> Result<(), SessionError> {
    append(session_id, &Record::created(spec.clone(), at_ms))
}

/// A turn began. Written BEFORE any engine is asked for anything — see
/// the crate doc: this line is what makes a crash read as `interrupted`
/// rather than as nothing at all.
pub fn turn_started(
    session_id: &str,
    turn_id: &str,
    message: &str,
    at_ms: u64,
) -> Result<(), SessionError> {
    append(session_id, &Record::turn_started(turn_id, message, at_ms))
}

/// A turn ended. `Record::turn_ended` refuses a non-terminal status, so a
/// line that would replay as a live turn cannot be written here at all.
pub fn turn_ended(session_id: &str, turn: &Turn, at_ms: u64) -> Result<(), SessionError> {
    let record = Record::turn_ended(turn, at_ms)
        .map_err(|error| SessionError::new(ErrorCode::Invalid, error))?;
    append(session_id, &record)
}

/// The session closed for good.
pub fn closed(session_id: &str, at_ms: u64) -> Result<(), SessionError> {
    append(session_id, &Record::closed(at_ms))
}

/// Reads every journal in the store's directory back into the registry.
///
/// An absent directory is an EMPTY store, not a failure: a first boot has
/// written nothing. A document that does not replay is a REFUSAL — the
/// activation fails and the entry stops, on the record — because a store
/// that quietly skipped a corrupt session would answer `list` with a
/// short answer and no one would know a session was missing.
///
/// **A torn tail is healed.** The reader admits an unterminated last line
/// as absence, but leaving those bytes in place would make the NEXT
/// append land on the end of the partial line — turning a tolerable tear
/// into an unreadable hole, and a session that came back fine into one
/// that refuses to replay at the boot after. `jinn:fs` can append and it
/// can rewrite, but it cannot drop a suffix (`FINDINGS.md` #34), so a
/// document that replayed with a torn tail is rewritten to its whole
/// prefix. No RECORD is lost: by the reader's own law those bytes were
/// never a record. The count is reported by `describe`.
///
/// **A document with no record is absence — and absence is three things,
/// not one.** Reading it as absence is only the first
/// (`jinn_session::journal::replay` answers `None`, so no session is
/// installed out of bytes that were never a record). The second is the
/// BYTES: they are dropped, so nothing can ever be appended onto them.
/// The third is the ID: the document is named for one, and a `create`
/// that minted it again would write the new session's first record into
/// that same document. So the id is RESERVED — the registry moves past it
/// without installing anything. Each is counted by `describe`; together
/// they are why an accepted absence cannot become corruption
/// (`FINDINGS.md` #36).
///
/// A drop is the ONLY repair here. Nothing in this path completes,
/// synthesizes or infers a record: a document that held none still holds
/// none after it runs.
///
/// # Errors
///
/// The directory is unreadable for any reason but absence, a document in
/// it does not replay, or a document this store had to repair could not
/// be written.
pub fn adopt_all(config: &StoreConfig) -> Result<(), SessionError> {
    let dir = config.dir.clone().unwrap_or_else(|| DEFAULT_DIR.to_owned());
    let entries = match fs::list(&dir) {
        Ok(entries) => entries,
        // The TYPED absence answer of the contract, and the only one
        // that means "nothing here yet". Every other error is a store
        // that cannot see its own records and must say so.
        Err(fs::FsError::NotFound) => return Ok(()),
        Err(error) => {
            return Err(SessionError::new(
                ErrorCode::Unavailable,
                format!("this store cannot read {dir:?}: {error:?}"),
            ))
        }
    };
    for entry in entries {
        if entry.is_dir {
            continue;
        }
        let Some(session_id) = document_id(&entry.path) else {
            continue;
        };
        let path = format!("{dir}/{}", file_name(&entry.path));
        let bytes = fs::read(&path).map_err(|error| {
            SessionError::new(
                ErrorCode::Unavailable,
                format!("the journal {path:?} could not be read: {error:?}"),
            )
        })?;
        let replayed = replay(&bytes).map_err(|error| {
            SessionError::new(
                ErrorCode::Failed,
                format!("the journal {path:?} does not replay: {error}"),
            )
        })?;
        // No complete record is the absence of the SESSION. Adopting a
        // default `Replayed` would install a session nobody created —
        // empty spec, sitting `idle` — out of bytes that were never a
        // record. See `FINDINGS.md` #36.
        let Some(replayed) = replayed else {
            record_less(&path, &bytes, &session_id)?;
            continue;
        };
        if replayed.torn_tail_bytes > 0 {
            heal(&path, &bytes, replayed.torn_tail_bytes)?;
        }
        let mut held = SESSIONS.lock().unwrap();
        held.as_mut()
            .expect("activate holds the registry")
            .adopt(&session_id, replayed);
    }
    Ok(())
}

/// Rewrites a document to its whole prefix, dropping a torn tail. See
/// `adopt_all`'s doc for why the bytes are droppable and the write is not
/// an edit of history.
fn heal(path: &str, bytes: &[u8], torn: usize) -> Result<(), SessionError> {
    let whole = &bytes[..bytes.len().saturating_sub(torn)];
    fs::write(path, whole, "").map_err(|error| {
        SessionError::new(
            ErrorCode::Failed,
            format!("the torn tail of {path:?} could not be healed: {error:?}"),
        )
    })?;
    *crate::store::HEALED_TAILS.lock().unwrap() += 1;
    Ok(())
}

/// Answers one document this store read and found no record in: the bytes
/// go, the id is spoken for, and the count says both happened.
///
/// The document is REMOVED rather than emptied. Every byte in it is a
/// byte the reader's own law says was never a record, so nothing that is
/// a record is lost — and a name that is gone cannot be appended onto by
/// any later writer, which an empty file left in place still can. The id
/// is reserved in the same breath, so the registry never mints it even
/// though its document no longer exists: two independent reasons the next
/// `create` cannot land in an absent session's place, and neither leaning
/// on the other.
///
/// # Errors
///
/// The document could not be removed. That fails the activation rather
/// than leaving a store running over bytes it has decided to append past.
fn record_less(path: &str, bytes: &[u8], session_id: &str) -> Result<(), SessionError> {
    if !bytes.is_empty() {
        fs::remove(path, "").map_err(|error| {
            SessionError::new(
                ErrorCode::Failed,
                format!("the record-less document {path:?} could not be dropped: {error:?}"),
            )
        })?;
    }
    SESSIONS
        .lock()
        .unwrap()
        .as_mut()
        .expect("activate holds the registry")
        .reserve(session_id);
    *crate::store::RECORD_LESS_DOCUMENTS.lock().unwrap() += 1;
    Ok(())
}

/// The last segment of a listed path. `jinn:fs` lists names relative to
/// the directory, but a provider that answered a fuller path would still
/// be readable here rather than producing a path with the directory in it
/// twice.
fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The session id a document name carries, or `None` when the file is not
/// one of this store's journals. A stray file in the directory is not a
/// session and is not an error either — it is simply not ours.
fn document_id(path: &str) -> Option<String> {
    file_name(path)
        .strip_suffix(".jsonl")
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}
