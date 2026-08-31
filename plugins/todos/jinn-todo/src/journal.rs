//! The DURABLE journal: one append-only JSONL document per Todo, and the
//! replay that reads it back honestly.
//!
//! # History is append-only, and the reader enforces it
//!
//! A status change is a NEW line. Nothing in this seam edits a line that
//! was written, and nothing removes one — which is why a Todo's `history`
//! is the document's own status lines in order, not a field somebody kept
//! up to date.
//!
//! That makes the READER the place where an illegal history is caught:
//!
//! - A [`Kind::StatusChanged`] whose `from` is not where the Todo
//!   actually stood is REFUSED. A line that begins from a status the Todo
//!   was never in is not a move this seam made.
//! - A `status-changed` the TABLE does not admit is REFUSED
//!   ([`Status::transition`]). The writer refuses illegal moves, so a
//!   document holding one did not come from this seam, and believing it
//!   would let `done` — the claim that work is finished — be reached by a
//!   path the company's law forbids.
//! - A [`Kind::DispatchEnded`] whose status is not terminal is REFUSED,
//!   for the same reason [`crate::DispatchStatus::Running`] cannot be
//!   minted from a document at all: a dispatch that reads as live after a
//!   restart is a Todo nothing will ever finish.
//!
//! # Absence and corruption are not answered the same way
//!
//! A line that does not decode is not a record. The reader admits a torn
//! TAIL — the last line, written short — as ABSENCE, because a
//! half-written record must read as "absent or complete" and never as a
//! damaged one. A hole anywhere EARLIER is corruption and is REFUSED:
//! answering the two the same way would let real damage masquerade as a
//! clean stop.
//!
//! The kernel's `jinn:fs` `append` commits whole-document atomically
//! (stage + fsync + rename — `FINDINGS.md` #22, closed at pin `3fd7b05`),
//! so a tear should be unreachable through that path. The reader does not
//! rely on that: the guarantee belongs to a contract this seam does not
//! own, and a reader that trusts it has no answer the day it changes.

use serde::{Deserialize, Serialize};

use crate::{
    Comment, Dispatch, DispatchStatus, Extensions, RefusedChange, Status, StatusChange, TodoSpec,
    API_VERSION,
};

/// The reason a dispatch open at replay carries. One string, one home.
pub const INTERRUPTED_REASON: &str =
    "the daemon stopped while this dispatch was in flight; how far it got is not recorded";

/// What one journal line records. A CLOSED value space: a kind this
/// version cannot name is a REFUSAL, because a journal whose unknown
/// lines were skipped would replay a different Todo than it holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// The Todo was recorded; the line carries its spec.
    Created,
    /// A status moved; the line carries `from`, `to` and the actor.
    StatusChanged,
    /// A comment was added.
    Commented,
    /// A dispatch to a session began.
    DispatchStarted,
    /// A dispatch ended; the line carries its terminal status.
    DispatchEnded,
    /// A status move was REFUSED. Recorded because an attempt is a fact.
    TransitionRefused,
}

/// One line of a Todo's journal.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Record {
    #[serde(default)]
    pub api_version: String,
    pub kind: Kind,
    pub at_ms: u64,
    /// Present on `created`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<TodoSpec>,
    /// Present on `status-changed` and `transition-refused`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Status>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<Status>,
    /// The principal who asked, where one was declared. Absence is
    /// absence — never a principal (`crate::spec`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Present on `commented`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Present on every dispatch line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_store: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Present on `dispatch-ended` — the PROOF a terminal status rests on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch_status: Option<DispatchStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl Record {
    fn new(kind: Kind, at_ms: u64) -> Self {
        Self {
            api_version: API_VERSION.to_owned(),
            kind,
            at_ms,
            spec: None,
            from: None,
            to: None,
            actor: None,
            note: None,
            comment_id: None,
            body: None,
            dispatch_id: None,
            session_store: None,
            engine: None,
            session_id: None,
            turn_id: None,
            dispatch_status: None,
            reason: None,
            answer: None,
            extra: Extensions::new(),
        }
    }

    /// The `created` line.
    #[must_use]
    pub fn created(spec: TodoSpec, at_ms: u64) -> Self {
        Self {
            actor: spec.attribution.actor.clone(),
            spec: Some(spec),
            ..Self::new(Kind::Created, at_ms)
        }
    }

    /// The `status-changed` line. The move is checked HERE too, so an
    /// illegal transition cannot be written even by a caller that skipped
    /// the registry.
    ///
    /// # Errors
    ///
    /// `from -> to` is not in the table.
    pub fn status_changed(change: &StatusChange, at_ms: u64) -> Result<Self, String> {
        change
            .from
            .transition(change.to)
            .map_err(|refusal| refusal.message())?;
        Ok(Self {
            from: Some(change.from),
            to: Some(change.to),
            actor: change.actor.clone(),
            note: change.note.clone(),
            ..Self::new(Kind::StatusChanged, at_ms)
        })
    }

    /// The `transition-refused` line: an attempt, recorded.
    #[must_use]
    pub fn transition_refused(refused: &RefusedChange, at_ms: u64) -> Self {
        Self {
            from: Some(refused.from),
            to: Some(refused.to),
            actor: refused.actor.clone(),
            ..Self::new(Kind::TransitionRefused, at_ms)
        }
    }

    /// The `commented` line.
    #[must_use]
    pub fn commented(comment: &Comment, at_ms: u64) -> Self {
        Self {
            comment_id: Some(comment.comment_id.clone()),
            body: Some(comment.body.clone()),
            actor: comment.actor.clone(),
            ..Self::new(Kind::Commented, at_ms)
        }
    }

    /// The `dispatch-started` line — never a claim that the work
    /// finished, and written BEFORE any session is asked for anything.
    #[must_use]
    pub fn dispatch_started(dispatch: &Dispatch, at_ms: u64) -> Self {
        Self {
            dispatch_id: Some(dispatch.dispatch_id.clone()),
            session_store: Some(dispatch.session_store.clone()),
            engine: Some(dispatch.engine.clone()),
            ..Self::new(Kind::DispatchStarted, at_ms)
        }
    }

    /// The `dispatch-ended` line: the ONLY proof a terminal dispatch
    /// status exists.
    ///
    /// # Errors
    ///
    /// The dispatch's status is [`DispatchStatus::Running`].
    pub fn dispatch_ended(dispatch: &Dispatch, at_ms: u64) -> Result<Self, String> {
        if !dispatch.status.is_terminal() {
            return Err(format!(
                "a journal records a dispatch's END, and {:?} is not an ending",
                dispatch.status
            ));
        }
        Ok(Self {
            dispatch_id: Some(dispatch.dispatch_id.clone()),
            session_id: dispatch.session_id.clone(),
            turn_id: dispatch.turn_id.clone(),
            dispatch_status: Some(dispatch.status),
            reason: dispatch.reason.clone(),
            answer: Some(dispatch.answer.clone()),
            ..Self::new(Kind::DispatchEnded, at_ms)
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

/// What a journal replayed back into: the Todo as it stands, with every
/// open dispatch already conservative.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Replayed {
    pub spec: TodoSpec,
    pub created_ms: u64,
    pub declared_status: Status,
    pub history: Vec<StatusChange>,
    pub refused: Vec<RefusedChange>,
    pub comments: Vec<Comment>,
    pub dispatches: Vec<Dispatch>,
    /// How many trailing bytes were an unterminated tail and read as
    /// absence. Reported rather than swallowed: a store that discards
    /// bytes says so.
    pub torn_tail_bytes: usize,
}

/// Replays a journal document.
///
/// `None` where the document holds NO complete record — a daemon killed
/// inside its very first append leaves bytes that were never one, and
/// that is the absence of the TODO rather than a Todo with an empty id
/// and a default status. A default [`Replayed`] is a sentinel that passes
/// for a real reading, so it is not returned; `FINDINGS.md` #36 is what
/// returning it costs one layer up.
///
/// # Errors
///
/// A line that does not decode anywhere but at the very end (a hole, not
/// a tear), a document whose first record is not `created`, a status move
/// that is illegal or begins from the wrong status, a `dispatch-ended`
/// for a dispatch that never started, or a `dispatch-ended` claiming a
/// non-terminal status.
pub fn replay(document: &[u8]) -> Result<Option<Replayed>, String> {
    let (body, torn_tail_bytes) = match document.iter().rposition(|byte| *byte == b'\n') {
        Some(last) => (&document[..=last], document.len() - last - 1),
        None => (&document[..0], document.len()),
    };
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
        if !opened && record.kind != Kind::Created {
            return Err(format!(
                "journal line {line_no}: a Todo's journal opens with the Todo it records, \
                 and this one opens with {:?}",
                record.kind
            ));
        }
        opened = true;
        apply(&mut replayed, record, line_no)?;
    }
    if !opened {
        return Ok(None);
    }
    Ok(Some(replayed))
}

fn apply(replayed: &mut Replayed, record: Record, line: usize) -> Result<(), String> {
    let at_ms = record.at_ms;
    match record.kind {
        Kind::Created => {
            replayed.spec = record.spec.unwrap_or_default();
            replayed.created_ms = at_ms;
            replayed.declared_status = Status::default();
        }
        Kind::StatusChanged => {
            let from = required_status(record.from, line, "from")?;
            let to = required_status(record.to, line, "to")?;
            // A line that starts from somewhere this Todo has never been
            // is not a move this seam made — a second writer, or damage.
            if from != replayed.declared_status {
                return Err(format!(
                    "journal line {line}: a move from {} but the Todo stood at {}",
                    from.tag(),
                    replayed.declared_status.tag()
                ));
            }
            // The table, on the way back in. See the module doc.
            from.transition(to)
                .map_err(|refusal| format!("journal line {line}: {}", refusal.message()))?;
            replayed.declared_status = to;
            replayed.history.push(StatusChange {
                seq: replayed.history.len() as u64,
                from,
                to,
                actor: record.actor,
                note: record.note,
                at_ms,
                extra: Extensions::new(),
            });
        }
        Kind::TransitionRefused => {
            let from = required_status(record.from, line, "from")?;
            let to = required_status(record.to, line, "to")?;
            replayed.refused.push(RefusedChange {
                seq: replayed.refused.len() as u64,
                from,
                to,
                actor: record.actor,
                at_ms,
                extra: Extensions::new(),
            });
        }
        Kind::Commented => {
            let comment_id = record
                .comment_id
                .ok_or_else(|| format!("journal line {line}: a comment carries a comment-id"))?;
            replayed.comments.push(Comment {
                comment_id,
                seq: replayed.comments.len() as u64,
                body: record.body.unwrap_or_default(),
                actor: record.actor,
                at_ms,
                extra: Extensions::new(),
            });
        }
        Kind::DispatchStarted => {
            let dispatch_id = record
                .dispatch_id
                .ok_or_else(|| format!("journal line {line}: a dispatch carries a dispatch-id"))?;
            // Opened INTERRUPTED, not running: only a terminal record can
            // move it, so an unfinished dispatch needs no special case
            // and `running` is unreachable from a document.
            replayed.dispatches.push(Dispatch {
                dispatch_id,
                session_store: record.session_store.unwrap_or_default(),
                engine: record.engine.unwrap_or_default(),
                status: DispatchStatus::Interrupted,
                reason: Some(INTERRUPTED_REASON.to_owned()),
                started_ms: at_ms,
                ..Dispatch::default()
            });
        }
        Kind::DispatchEnded => {
            let dispatch_id = record
                .dispatch_id
                .ok_or_else(|| format!("journal line {line}: a dispatch carries a dispatch-id"))?;
            let status = record.dispatch_status.ok_or_else(|| {
                format!("journal line {line}: an ended dispatch carries a status")
            })?;
            // The WRITER refuses a non-terminal ending, so the READER
            // refuses one too: a line claiming a dispatch ended `running`
            // would hand back a Todo eternally in flight.
            if !status.is_terminal() {
                return Err(format!(
                    "journal line {line}: a dispatch's END cannot be {status:?}"
                ));
            }
            let dispatch = replayed
                .dispatches
                .iter_mut()
                .rev()
                .find(|dispatch| dispatch.dispatch_id == dispatch_id)
                .ok_or_else(|| {
                    format!("journal line {line}: dispatch {dispatch_id:?} never started")
                })?;
            dispatch.status = status;
            dispatch.session_id = record.session_id;
            dispatch.turn_id = record.turn_id;
            dispatch.answer = record.answer.unwrap_or_default();
            // A non-`done` ending carries a reason by the registry's
            // rule. A line that carries none is not proof there was
            // none: the conservative reason the started dispatch already
            // holds stands rather than an ending nobody can explain.
            dispatch.reason = match (status, record.reason) {
                (DispatchStatus::Done, _) => None,
                (_, Some(reason)) => Some(reason),
                (_, None) => dispatch.reason.take(),
            };
            dispatch.ended_ms = Some(at_ms);
        }
    }
    Ok(())
}

fn required_status(status: Option<Status>, line: usize, which: &str) -> Result<Status, String> {
    status.ok_or_else(|| format!("journal line {line}: a status move carries `{which}`"))
}

jinn_settings::closed_value_space!(Kind, "a journal record's `kind`", {
    "created" => Self::Created,
    "status-changed" => Self::StatusChanged,
    "commented" => Self::Commented,
    "dispatch-started" => Self::DispatchStarted,
    "dispatch-ended" => Self::DispatchEnded,
    "transition-refused" => Self::TransitionRefused,
});

jinn_settings::additive!(Record);
