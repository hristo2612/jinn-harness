//! The READ side of the registry: one session's record, one page of its
//! turns, and the store's listing. A status is DERIVED here from the turns
//! it describes and never stored, so the two cannot drift apart.

use super::{Live, Sessions, DEFAULT_PAGE};
use crate::{
    Extensions, ListRequest, Page, SessionRecord, SessionStatus, SessionSummary, TurnStatus,
    API_VERSION,
};

impl Sessions {
    /// One session's record.
    #[must_use]
    pub fn record(&self, session_id: &str) -> Option<SessionRecord> {
        let live = self.live.get(session_id)?;
        Some(SessionRecord {
            api_version: API_VERSION.to_owned(),
            session_id: session_id.to_owned(),
            store: self.store.clone(),
            engine: live.spec.engine.engine.clone(),
            model: live.spec.engine.model.clone(),
            owner: live.spec.attribution.owner.clone(),
            status: status_of(live),
            turns: live.turns.len() as u64,
            log: live.turns.clone(),
            created_ms: live.created_ms,
            metadata: live.spec.metadata.clone(),
            extra: Extensions::new(),
        })
    }

    /// One page of a session's turns.
    #[must_use]
    pub fn page(&self, session_id: &str, offset: u64, limit: Option<u64>) -> Option<Page> {
        let live = self.live.get(session_id)?;
        Some(Page::of(
            session_id,
            &live.turns,
            offset,
            limit.unwrap_or(DEFAULT_PAGE).max(1),
        ))
    }

    /// The sessions this store holds, filtered.
    #[must_use]
    pub fn list(&self, filter: &ListRequest) -> Vec<SessionSummary> {
        self.live
            .iter()
            .filter(|(_, live)| {
                filter
                    .owner
                    .as_ref()
                    .is_none_or(|owner| live.spec.attribution.owner.as_ref() == Some(owner))
                    && filter
                        .engine
                        .as_ref()
                        .is_none_or(|engine| &live.spec.engine.engine == engine)
            })
            .map(|(session_id, live)| SessionSummary {
                session_id: session_id.clone(),
                store: self.store.clone(),
                engine: live.spec.engine.engine.clone(),
                status: status_of(live),
                turns: live.turns.len() as u64,
                owner: live.spec.attribution.owner.clone(),
                created_ms: live.created_ms,
                extra: Extensions::new(),
            })
            .collect()
    }
}

/// Derived, never stored: a status cannot drift from the turns it
/// describes. `Running` requires a turn actually in flight in THIS
/// incarnation — a replayed session has none, so it can never read back
/// as working (see `journal`'s honesty law).
pub(super) fn status_of(live: &Live) -> SessionStatus {
    if live.closed {
        return SessionStatus::Closed;
    }
    if live.turns.iter().any(|turn| !turn.status.is_terminal()) {
        return SessionStatus::Running;
    }
    match live.turns.last().map(|turn| turn.status) {
        Some(TurnStatus::Failed | TurnStatus::Interrupted) => SessionStatus::Failed,
        _ => SessionStatus::Idle,
    }
}
