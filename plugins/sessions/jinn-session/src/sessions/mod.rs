//! The session registry every store provider keeps: session ids, turn
//! ids, event sequencing, status derivation, and the state machine that
//! decides what a session will accept. Pure — no host call, no clock of
//! its own (the caller passes the kernel's `now`) — so the seam's session
//! semantics are ONE implementation with one set of tests, and a provider
//! adds only where the records live.

use std::collections::BTreeMap;

mod views;

use crate::journal::Replayed;
use crate::{
    ErrorCode, Extensions, SessionCreated, SessionError, SessionSpec, Turn, TurnAccepted,
    TurnStatus, API_VERSION,
};

/// The page size a `messages` read falls back to when the caller names
/// none. A bound, never "everything": an unbounded default is how a large
/// log becomes an outage.
pub const DEFAULT_PAGE: u64 = 50;

pub(super) struct Live {
    pub(super) spec: SessionSpec,
    pub(super) turns: Vec<Turn>,
    pub(super) closed: bool,
    pub(super) created_ms: u64,
    seq: u64,
    minted_turns: u64,
}

/// Every session one store incarnation holds.
#[derive(Default)]
pub struct Sessions {
    pub(super) store: String,
    minted: u64,
    pub(super) live: BTreeMap<String, Live>,
}

impl Sessions {
    /// A registry for the store `id` this provider serves.
    #[must_use]
    pub fn new(store: impl Into<String>) -> Self {
        Self {
            store: store.into(),
            minted: 0,
            live: BTreeMap::new(),
        }
    }

    /// The store id every session here belongs to.
    #[must_use]
    pub fn store(&self) -> &str {
        &self.store
    }

    /// The ids this registry holds, oldest id first.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.live.keys().map(String::as_str)
    }

    /// Opens a session and mints its id (`<store>-<n>`, monotone within
    /// this incarnation).
    pub fn create(&mut self, spec: SessionSpec, now_ms: u64) -> SessionCreated {
        self.minted += 1;
        let session_id = format!("{}-{}", self.store, self.minted);
        let engine = spec.engine.engine.clone();
        self.install(session_id.clone(), spec, now_ms, Vec::new(), false);
        SessionCreated {
            api_version: API_VERSION.to_owned(),
            session_id,
            store: self.store.clone(),
            engine,
            extra: Extensions::new(),
        }
    }

    /// Installs a session read back from a durable journal under the id it
    /// was stored as. The replay decides its turns; nothing here can
    /// promote one to `running` (see `journal`'s honesty law). Mints
    /// forward past any adopted numeric id so a later `create` cannot
    /// collide with one.
    pub fn adopt(&mut self, session_id: &str, replayed: Replayed) {
        if let Some(minted) = session_id
            .strip_prefix(&format!("{}-", self.store))
            .and_then(|tail| tail.parse::<u64>().ok())
        {
            self.minted = self.minted.max(minted);
        }
        let turns = replayed.turns.len() as u64;
        let created = replayed.created_ms;
        self.install(
            session_id.to_owned(),
            replayed.spec,
            created,
            replayed.turns,
            replayed.closed,
        );
        if let Some(live) = self.live.get_mut(session_id) {
            live.minted_turns = turns;
        }
    }

    fn install(
        &mut self,
        session_id: String,
        spec: SessionSpec,
        created_ms: u64,
        turns: Vec<Turn>,
        closed: bool,
    ) {
        let minted_turns = turns.len() as u64;
        self.live.insert(
            session_id,
            Live {
                spec,
                turns,
                closed,
                created_ms,
                seq: 0,
                minted_turns,
            },
        );
    }

    /// The next event sequence number for a session.
    pub fn next_seq(&mut self, session_id: &str) -> u64 {
        match self.live.get_mut(session_id) {
            Some(live) => {
                let seq = live.seq;
                live.seq += 1;
                seq
            }
            None => 0,
        }
    }

    /// The session's engine binding, for a provider about to drive it.
    #[must_use]
    pub fn spec(&self, session_id: &str) -> Option<&SessionSpec> {
        self.live.get(session_id).map(|live| &live.spec)
    }

    /// Accepts a message: mints the turn id and records it RUNNING — the
    /// one place that status is ever minted, and only for a turn this
    /// incarnation is about to drive.
    ///
    /// # Errors
    ///
    /// The session is unknown, closed, or already has a turn in flight.
    pub fn send(
        &mut self,
        session_id: &str,
        message: &str,
        now_ms: u64,
    ) -> Result<TurnAccepted, SessionError> {
        let store = self.store.clone();
        let live = self
            .live
            .get_mut(session_id)
            .ok_or_else(|| not_found(session_id))?;
        if live.closed {
            return Err(SessionError::new(
                ErrorCode::Refused,
                format!("session {session_id:?} is closed"),
            ));
        }
        if live.turns.iter().any(|turn| !turn.status.is_terminal()) {
            return Err(SessionError::new(
                ErrorCode::Refused,
                format!("session {session_id:?} already has a turn in flight"),
            ));
        }
        live.minted_turns += 1;
        let turn_id = format!("{store}-{}-t{}", session_id, live.minted_turns);
        let seq = live.turns.len() as u64;
        live.turns.push(Turn {
            turn_id: turn_id.clone(),
            seq,
            status: TurnStatus::Running,
            message: message.to_owned(),
            started_ms: now_ms,
            ..Turn::default()
        });
        Ok(TurnAccepted {
            api_version: API_VERSION.to_owned(),
            session_id: session_id.to_owned(),
            turn_id,
            extra: Extensions::new(),
        })
    }

    /// The turn in flight, if the session has one.
    #[must_use]
    pub fn in_flight(&self, session_id: &str) -> Option<&Turn> {
        self.live
            .get(session_id)?
            .turns
            .iter()
            .find(|turn| !turn.status.is_terminal())
    }

    /// Mutates one turn in place; `None` when the session or turn is gone.
    pub fn turn_mut(&mut self, session_id: &str, turn_id: &str) -> Option<&mut Turn> {
        self.live
            .get_mut(session_id)?
            .turns
            .iter_mut()
            .find(|turn| turn.turn_id == turn_id)
    }

    /// Ends a turn. A terminal status other than `done` MUST carry a
    /// reason, so no reader ever has to invent one.
    ///
    /// # Errors
    ///
    /// The turn is unknown, the status is not terminal, or a non-`done`
    /// ending carries no reason.
    pub fn end_turn(
        &mut self,
        session_id: &str,
        turn_id: &str,
        status: TurnStatus,
        reason: Option<String>,
        now_ms: u64,
    ) -> Result<Turn, SessionError> {
        if !status.is_terminal() {
            return Err(SessionError::new(
                ErrorCode::Invalid,
                format!("{status:?} does not end a turn"),
            ));
        }
        if status != TurnStatus::Done && reason.is_none() {
            return Err(SessionError::new(
                ErrorCode::Invalid,
                format!("a turn ending {status:?} carries a reason"),
            ));
        }
        let turn = self
            .turn_mut(session_id, turn_id)
            .ok_or_else(|| not_found(turn_id))?;
        turn.status = status;
        turn.reason = reason;
        turn.ended_ms = Some(now_ms);
        Ok(turn.clone())
    }

    /// Closes a session for good.
    ///
    /// # Errors
    ///
    /// The session is unknown.
    pub fn close(&mut self, session_id: &str) -> Result<(), SessionError> {
        let live = self
            .live
            .get_mut(session_id)
            .ok_or_else(|| not_found(session_id))?;
        live.closed = true;
        Ok(())
    }
}

fn not_found(what: &str) -> SessionError {
    SessionError::new(ErrorCode::NotFound, format!("{what:?} is not here"))
}
