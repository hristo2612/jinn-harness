//! The DURABLE journal: one append-only JSONL document per session, and
//! the replay that reads it back honestly.
//!
//! # Why a claim needs proof here
//!
//! A journal is what a store has after a crash, and a crash is exactly the
//! moment a system is tempted to lie. So the reader is built the other way
//! round: [`TurnStatus::Done`] — the answer that says "the engine
//! finished and this text is whole" — is produced ONLY by a terminal
//! record actually on disk. There is no sentinel that can pass for one,
//! and no code path where the absence of a contradiction becomes a claim.
//!
//! - A [`Kind::TurnStarted`] with no terminal record of its own replays as
//!   [`TurnStatus::Interrupted`] with [`INTERRUPTED_REASON`]. That is the
//!   conservative answer, and it is reached by CONSTRUCTION: [`replay`]
//!   opens every turn as interrupted and only a terminal record can move
//!   it. `Running` is unreachable from here — a replayed session can never
//!   claim to be working.
//! - A line that does not decode is not a record. The reader admits a
//!   torn TAIL — the last line, written short — as ABSENCE, because a
//!   half-written turn must read as "absent or complete" and never as a
//!   damaged one. A hole anywhere EARLIER is corruption, not a tear, and
//!   is REFUSED: answering the two the same way would let real damage
//!   masquerade as a clean stop.
//!
//! The kernel's `jinn:fs` `append` commits whole-document atomically
//! (stage + fsync + rename — `FINDINGS.md` #22, closed at pin `3fd7b05`),
//! so a tear should be unreachable through that path. The reader does not
//! rely on that: the guarantee belongs to a contract this seam does not
//! own, and a reader that trusts it has no answer the day it changes.

use serde::{Deserialize, Serialize};

use crate::{Extensions, SessionSpec, SessionStatus, Turn, TurnStatus, API_VERSION};

/// The reason a turn open at replay carries. One string, one home: the
/// store answers it, the API relays it, and a test asserts on it.
pub const INTERRUPTED_REASON: &str =
    "the daemon stopped while this turn was in flight; how far it got is not recorded";

/// What one journal line records. A CLOSED value space: a kind this
/// version cannot name is a REFUSAL, because a journal whose unknown lines
/// were skipped would replay a different session than it holds.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// The session opened; the line carries its spec.
    Created,
    /// A turn began; the line carries the caller's message.
    TurnStarted,
    /// A turn ended; the line carries its status, answer and usage.
    TurnEnded,
    /// The session closed for good.
    Closed,
}

/// One line of a session's journal.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Record {
    #[serde(default)]
    pub api_version: String,
    pub kind: Kind,
    pub at_ms: u64,
    /// Present on `created`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spec: Option<SessionSpec>,
    /// Present on every turn line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Present on `turn-started`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Present on `turn-ended` — the PROOF a terminal status rests on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<TurnStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<jinn_engine::Usage>,
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
            turn_id: None,
            message: None,
            status: None,
            answer: None,
            reason: None,
            run_id: None,
            usage: None,
            extra: Extensions::new(),
        }
    }

    /// The `created` line.
    #[must_use]
    pub fn created(spec: SessionSpec, at_ms: u64) -> Self {
        Self {
            spec: Some(spec),
            ..Self::new(Kind::Created, at_ms)
        }
    }

    /// The `turn-started` line — never a claim that the turn finished.
    #[must_use]
    pub fn turn_started(turn_id: &str, message: &str, at_ms: u64) -> Self {
        Self {
            turn_id: Some(turn_id.to_owned()),
            message: Some(message.to_owned()),
            ..Self::new(Kind::TurnStarted, at_ms)
        }
    }

    /// The `turn-ended` line: the ONLY proof a terminal status exists.
    /// `Running` is not a terminal status and is refused here, so a
    /// caller cannot write a line that would replay as a live turn.
    ///
    /// # Errors
    ///
    /// `status` is [`TurnStatus::Running`].
    pub fn turn_ended(turn: &Turn, at_ms: u64) -> Result<Self, String> {
        if !turn.status.is_terminal() {
            return Err(format!(
                "a journal records a turn's END, and {:?} is not an ending",
                turn.status
            ));
        }
        Ok(Self {
            turn_id: Some(turn.turn_id.clone()),
            status: Some(turn.status),
            answer: Some(turn.answer.clone()),
            reason: turn.reason.clone(),
            run_id: turn.run_id.clone(),
            usage: Some(turn.usage.clone()),
            ..Self::new(Kind::TurnEnded, at_ms)
        })
    }

    /// The `closed` line.
    #[must_use]
    pub fn closed(at_ms: u64) -> Self {
        Self::new(Kind::Closed, at_ms)
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

/// What a journal replayed back into: the session as it stands, with every
/// open turn already conservative.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Replayed {
    pub spec: SessionSpec,
    pub created_ms: u64,
    pub turns: Vec<Turn>,
    pub closed: bool,
    /// How many trailing bytes were an unterminated tail and read as
    /// absence. Reported rather than swallowed: a store that discards
    /// bytes says so.
    pub torn_tail_bytes: usize,
}

impl Replayed {
    /// The session's status, derived — never stored, so it cannot drift
    /// from the turns it describes. `Running` is impossible here by
    /// construction (see the module doc).
    #[must_use]
    pub fn status(&self) -> SessionStatus {
        if self.closed {
            return SessionStatus::Closed;
        }
        match self.turns.last().map(|turn| turn.status) {
            Some(TurnStatus::Failed | TurnStatus::Interrupted) => SessionStatus::Failed,
            _ => SessionStatus::Idle,
        }
    }
}

/// Replays a journal document.
///
/// # Errors
///
/// A line that does not decode anywhere but at the very end (a hole, not
/// a tear), a `turn-ended` for a turn that never started, or a document
/// whose first record is not `created`.
pub fn replay(document: &[u8]) -> Result<Replayed, String> {
    let (body, torn_tail_bytes) = match document.iter().rposition(|byte| *byte == b'\n') {
        Some(last) => (&document[..=last], document.len() - last - 1),
        None => (&document[..0], document.len()),
    };
    let mut replayed = Replayed {
        torn_tail_bytes,
        ..Replayed::default()
    };
    for (index, line) in body.split(|byte| *byte == b'\n').enumerate() {
        if line.is_empty() {
            continue;
        }
        let record: Record = serde_json::from_slice(line)
            .map_err(|error| format!("journal line {}: {error}", index + 1))?;
        apply(&mut replayed, record, index + 1)?;
    }
    Ok(replayed)
}

fn apply(replayed: &mut Replayed, record: Record, line: usize) -> Result<(), String> {
    match record.kind {
        Kind::Created => {
            replayed.spec = record.spec.unwrap_or_default();
            replayed.created_ms = record.at_ms;
        }
        Kind::TurnStarted => {
            let turn_id = record
                .turn_id
                .ok_or_else(|| format!("journal line {line}: a started turn carries a turn-id"))?;
            let seq = replayed.turns.len() as u64;
            // Opened INTERRUPTED, not running: only a terminal record can
            // move it, so an unfinished turn needs no special case.
            replayed.turns.push(Turn {
                turn_id,
                seq,
                status: TurnStatus::Interrupted,
                message: record.message.unwrap_or_default(),
                reason: Some(INTERRUPTED_REASON.to_owned()),
                started_ms: record.at_ms,
                ..Turn::default()
            });
        }
        Kind::TurnEnded => {
            let turn_id = record
                .turn_id
                .ok_or_else(|| format!("journal line {line}: an ended turn carries a turn-id"))?;
            let status = record
                .status
                .ok_or_else(|| format!("journal line {line}: an ended turn carries a status"))?;
            let turn = replayed
                .turns
                .iter_mut()
                .rev()
                .find(|turn| turn.turn_id == turn_id)
                .ok_or_else(|| format!("journal line {line}: turn {turn_id:?} never started"))?;
            turn.status = status;
            turn.answer = record.answer.unwrap_or_default();
            turn.reason = record.reason;
            turn.run_id = record.run_id;
            turn.usage = record.usage.unwrap_or_default();
            turn.ended_ms = Some(record.at_ms);
        }
        Kind::Closed => replayed.closed = true,
    }
    Ok(())
}

jinn_settings::closed_value_space!(Kind, "a journal record's `kind`", {
    "created" => Self::Created,
    "turn-started" => Self::TurnStarted,
    "turn-ended" => Self::TurnEnded,
    "closed" => Self::Closed,
});

jinn_settings::additive!(Record);
