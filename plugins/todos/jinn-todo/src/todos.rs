//! The Todo registry every store provider keeps: Todo ids, comment and
//! dispatch ids, event sequencing, the status law applied, and the fold
//! that decides what a store REPORTS. Pure — no host call, no clock of
//! its own (the caller passes the kernel's `now`) — so the seam's ledger
//! semantics are ONE implementation with one set of tests, and a provider
//! adds only where the records live.
//!
//! # The refusal is a recorded outcome, not an exception
//!
//! [`Todos::update`] answers [`Moved::Refused`] for an illegal move, with
//! the [`RefusedChange`] already appended to the Todo. That shape is
//! deliberate: it makes it impossible for a provider to refuse a move
//! WITHOUT recording it, because there is no code path that produces the
//! refusal and not the record. "Typed and ledgered" is then a property of
//! the type, not of a provider remembering to do both.

use std::collections::BTreeMap;

use crate::journal::Replayed;
use crate::{
    reported_status, Comment, Dispatch, DispatchSpec, DispatchStatus, ErrorCode, Event, EventKind,
    EventPage, Extensions, ListRequest, RefusedChange, Status, StatusChange, TodoCreated,
    TodoError, TodoEvent, TodoList, TodoRecord, TodoSpec, TodoSummary, Tree, TreeNode, API_VERSION,
};

/// How many events one Todo's feed holds before the OLDEST are dropped. A
/// ring, because a store that kept every event of every Todo forever is a
/// memory leak with a schedule; the count of what was dropped is reported
/// with every page, so a reader is never told a gap is quiet.
pub const EVENT_RING: usize = 256;

/// What an `update` did. Both arms are outcomes the caller RECORDS — see
/// the module doc.
#[derive(Clone, Debug, PartialEq)]
pub enum Moved {
    /// The move was legal and is now history.
    Changed(StatusChange),
    /// The move was refused. The attempt is already on the Todo's
    /// `refused` list; the error is what the caller answers with.
    Refused(RefusedChange, TodoError),
}

struct Live {
    spec: TodoSpec,
    declared: Status,
    history: Vec<StatusChange>,
    refused: Vec<RefusedChange>,
    comments: Vec<Comment>,
    dispatches: Vec<Dispatch>,
    created_ms: u64,
    seq: u64,
    minted_comments: u64,
    minted_dispatches: u64,
    events: Vec<TodoEvent>,
    dropped: u64,
}

/// Every Todo one store incarnation holds.
#[derive(Default)]
pub struct Todos {
    store: String,
    minted: u64,
    live: BTreeMap<String, Live>,
}

fn not_found(todo_id: &str) -> TodoError {
    TodoError::new(ErrorCode::NotFound, format!("{todo_id:?} is not here"))
}

impl Todos {
    /// A registry for the store `id` this provider serves.
    #[must_use]
    pub fn new(store: impl Into<String>) -> Self {
        Self {
            store: store.into(),
            minted: 0,
            live: BTreeMap::new(),
        }
    }

    /// The store id every Todo here belongs to.
    #[must_use]
    pub fn store(&self) -> &str {
        &self.store
    }

    /// The ids this registry holds, oldest id first.
    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.live.keys().map(String::as_str)
    }

    /// Records a Todo and mints its id (`<store>-<n>`, monotone within
    /// this incarnation).
    ///
    /// # Errors
    ///
    /// A spec the seam will not record, or a parent that is not here. A
    /// parent must ALREADY exist, which is what makes the tree acyclic by
    /// construction rather than by a cycle check.
    pub fn create(&mut self, spec: TodoSpec, now_ms: u64) -> Result<TodoCreated, TodoError> {
        spec.check()?;
        if let Some(parent) = &spec.parent {
            if !self.live.contains_key(parent) {
                return Err(TodoError::new(
                    ErrorCode::NotFound,
                    format!("this Todo's parent {parent:?} is not in this store"),
                ));
            }
        }
        self.minted += 1;
        let todo_id = format!("{}-{}", self.store, self.minted);
        self.install(todo_id.clone(), spec, now_ms);
        Ok(TodoCreated {
            api_version: API_VERSION.to_owned(),
            todo_id,
            store: self.store.clone(),
            status: Status::default(),
            extra: Extensions::new(),
        })
    }

    /// Installs a Todo read back from a durable journal under the id it
    /// was stored as. The replay decides its history; nothing here can
    /// promote a dispatch to `running` (see `journal`'s honesty law).
    /// Mints forward past any adopted numeric id so a later `create`
    /// cannot collide with one.
    pub fn adopt(&mut self, todo_id: &str, replayed: Replayed) {
        if let Some(minted) = todo_id
            .strip_prefix(&format!("{}-", self.store))
            .and_then(|tail| tail.parse::<u64>().ok())
        {
            self.minted = self.minted.max(minted);
        }
        let created = replayed.created_ms;
        self.install(todo_id.to_owned(), replayed.spec, created);
        if let Some(live) = self.live.get_mut(todo_id) {
            live.declared = replayed.declared_status;
            live.minted_comments = replayed.comments.len() as u64;
            live.minted_dispatches = replayed.dispatches.len() as u64;
            live.history = replayed.history;
            live.refused = replayed.refused;
            live.comments = replayed.comments;
            live.dispatches = replayed.dispatches;
        }
    }

    fn install(&mut self, todo_id: String, spec: TodoSpec, created_ms: u64) {
        self.live.insert(
            todo_id,
            Live {
                spec,
                declared: Status::default(),
                history: Vec::new(),
                refused: Vec::new(),
                comments: Vec::new(),
                dispatches: Vec::new(),
                created_ms,
                seq: 0,
                minted_comments: 0,
                minted_dispatches: 0,
                events: Vec::new(),
                dropped: 0,
            },
        );
    }

    /// Drops a Todo whose durable record did not land, so nothing
    /// half-created is left for a later replay to disagree with.
    pub fn forget(&mut self, todo_id: &str) {
        self.live.remove(todo_id);
    }

    /// The spec of one Todo.
    #[must_use]
    pub fn spec(&self, todo_id: &str) -> Option<&TodoSpec> {
        self.live.get(todo_id).map(|live| &live.spec)
    }

    /// The next event sequence number for a Todo.
    pub fn next_seq(&mut self, todo_id: &str) -> u64 {
        match self.live.get_mut(todo_id) {
            Some(live) => {
                let seq = live.seq;
                live.seq += 1;
                seq
            }
            None => 0,
        }
    }

    /// Records one event against a Todo and answers the record to put on
    /// the bus. The sequence is minted HERE, once, so the feed a reader
    /// polls and the records a listener receives carry the same numbers.
    pub fn record_event(&mut self, todo_id: &str, kind: EventKind) -> TodoEvent {
        let seq = self.next_seq(todo_id);
        let record = TodoEvent::new(&self.store, todo_id, seq, kind);
        if let Some(live) = self.live.get_mut(todo_id) {
            live.events.push(record.clone());
            if live.events.len() > EVENT_RING {
                let over = live.events.len() - EVENT_RING;
                live.events.drain(..over);
                live.dropped += over as u64;
            }
        }
        record
    }

    /// Applies one status move — the table, and the record of what it
    /// did. See the module doc for why a refusal is an `Ok(Moved)`.
    ///
    /// # Errors
    ///
    /// The Todo is not here, or the actor is a blank.
    pub fn update(
        &mut self,
        todo_id: &str,
        to: Status,
        actor: Option<String>,
        note: Option<String>,
        now_ms: u64,
    ) -> Result<Moved, TodoError> {
        let live = self
            .live
            .get_mut(todo_id)
            .ok_or_else(|| not_found(todo_id))?;
        let from = live.declared;
        match from.transition(to) {
            Ok(to) => {
                let change = StatusChange {
                    seq: live.history.len() as u64,
                    from,
                    to,
                    actor,
                    note,
                    at_ms: now_ms,
                    extra: Extensions::new(),
                };
                live.declared = to;
                live.history.push(change.clone());
                Ok(Moved::Changed(change))
            }
            Err(refusal) => {
                let refused = RefusedChange {
                    seq: live.refused.len() as u64,
                    from,
                    to,
                    actor,
                    at_ms: now_ms,
                    extra: Extensions::new(),
                };
                live.refused.push(refused.clone());
                Ok(Moved::Refused(
                    refused,
                    TodoError::refused_transition(refusal),
                ))
            }
        }
    }

    /// Adds one comment and mints its id.
    ///
    /// # Errors
    ///
    /// The Todo is not here, or the comment is blank — a comment that
    /// says nothing is a line in the history that means nothing.
    pub fn comment(
        &mut self,
        todo_id: &str,
        body: &str,
        actor: Option<String>,
        now_ms: u64,
    ) -> Result<Comment, TodoError> {
        if body.trim().is_empty() {
            return Err(TodoError::new(
                ErrorCode::Invalid,
                "a comment's `body` cannot be blank",
            ));
        }
        let live = self
            .live
            .get_mut(todo_id)
            .ok_or_else(|| not_found(todo_id))?;
        live.minted_comments += 1;
        let comment = Comment {
            comment_id: format!("{todo_id}-c{}", live.minted_comments),
            seq: live.comments.len() as u64,
            body: body.to_owned(),
            actor,
            at_ms: now_ms,
            extra: Extensions::new(),
        };
        live.comments.push(comment.clone());
        Ok(comment)
    }

    /// The dispatch in flight for a Todo, if it has one.
    #[must_use]
    pub fn in_flight(&self, todo_id: &str) -> Option<&Dispatch> {
        self.live
            .get(todo_id)?
            .dispatches
            .iter()
            .find(|dispatch| !dispatch.status.is_terminal())
    }

    /// Opens a dispatch: the status move it implies, then the dispatch
    /// itself, recorded RUNNING. This is the one place that status is
    /// ever minted, and only for a dispatch this incarnation is about to
    /// drive.
    ///
    /// # Errors
    ///
    /// The Todo is not here, it already has a dispatch in flight, or its
    /// status cannot legally reach `executing` (a terminal Todo is not
    /// dispatched — the table says so, and the refusal names the move).
    pub fn begin_dispatch(
        &mut self,
        todo_id: &str,
        spec: &DispatchSpec,
        actor: Option<String>,
        now_ms: u64,
    ) -> Result<(Dispatch, Option<StatusChange>), TodoError> {
        if let Some(dispatch) = self.in_flight(todo_id) {
            return Err(TodoError::new(
                ErrorCode::Refused,
                format!(
                    "{todo_id:?} already has dispatch {:?} in flight",
                    dispatch.dispatch_id
                ),
            ));
        }
        let declared = self
            .live
            .get(todo_id)
            .ok_or_else(|| not_found(todo_id))?
            .declared;
        // A dispatch IS the work starting, so it moves the Todo through
        // the same table every other move goes through. Already
        // `executing` is not a move.
        let change = if declared == Status::Executing {
            None
        } else {
            match self.update(todo_id, Status::Executing, actor, None, now_ms)? {
                Moved::Changed(change) => Some(change),
                // The refusal is already recorded by `update`; the
                // dispatch does not happen and the caller answers typed.
                Moved::Refused(_, error) => return Err(error),
            }
        };
        let live = self
            .live
            .get_mut(todo_id)
            .ok_or_else(|| not_found(todo_id))?;
        live.minted_dispatches += 1;
        let dispatch = Dispatch {
            dispatch_id: format!("{todo_id}-d{}", live.minted_dispatches),
            session_store: spec.store.clone(),
            engine: spec.engine.engine.clone(),
            status: DispatchStatus::Running,
            started_ms: now_ms,
            ..Dispatch::default()
        };
        live.dispatches.push(dispatch.clone());
        Ok((dispatch, change))
    }

    /// Mutates one dispatch in place; `None` when the Todo or dispatch is
    /// gone.
    pub fn dispatch_mut(&mut self, todo_id: &str, dispatch_id: &str) -> Option<&mut Dispatch> {
        self.live
            .get_mut(todo_id)?
            .dispatches
            .iter_mut()
            .find(|dispatch| dispatch.dispatch_id == dispatch_id)
    }

    /// Ends a dispatch. A terminal status other than `done` MUST carry a
    /// reason, so no reader ever has to invent one.
    ///
    /// # Errors
    ///
    /// The dispatch is unknown, the status is not terminal, or a
    /// non-`done` ending carries no reason.
    pub fn end_dispatch(
        &mut self,
        todo_id: &str,
        dispatch_id: &str,
        status: DispatchStatus,
        reason: Option<String>,
        answer: String,
        now_ms: u64,
    ) -> Result<Dispatch, TodoError> {
        if !status.is_terminal() {
            return Err(TodoError::new(
                ErrorCode::Invalid,
                format!("{status:?} is not an ending"),
            ));
        }
        if status != DispatchStatus::Done
            && reason.as_deref().is_none_or(|reason| reason.is_empty())
        {
            return Err(TodoError::new(
                ErrorCode::Invalid,
                format!("a {status:?} dispatch carries the reason it ended"),
            ));
        }
        let dispatch = self
            .dispatch_mut(todo_id, dispatch_id)
            .ok_or_else(|| not_found(dispatch_id))?;
        dispatch.status = status;
        dispatch.reason = reason;
        dispatch.answer = answer;
        dispatch.ended_ms = Some(now_ms);
        Ok(dispatch.clone())
    }

    /// One Todo's record, with the reported status FOLDED from its
    /// history and its dispatches (`crate::record::reported_status`).
    #[must_use]
    pub fn record(&self, todo_id: &str) -> Option<TodoRecord> {
        let live = self.live.get(todo_id)?;
        let (status, status_reason) = reported_status(live.declared, live.dispatches.last());
        Some(TodoRecord {
            api_version: API_VERSION.to_owned(),
            todo_id: todo_id.to_owned(),
            store: self.store.clone(),
            title: live.spec.title.clone(),
            body: live.spec.body.clone(),
            acceptance: live.spec.acceptance.clone(),
            parent: live.spec.parent.clone(),
            department: live.spec.department.clone(),
            priority: live.spec.priority,
            status,
            declared_status: live.declared,
            status_reason,
            history: live.history.clone(),
            refused: live.refused.clone(),
            comments: live.comments.clone(),
            dispatches: live.dispatches.clone(),
            actor: live.spec.attribution.actor.clone(),
            created_ms: live.created_ms,
            metadata: live.spec.metadata.clone(),
            extra: Extensions::new(),
        })
    }

    fn summary(&self, todo_id: &str) -> Option<TodoSummary> {
        let live = self.live.get(todo_id)?;
        let (status, _) = reported_status(live.declared, live.dispatches.last());
        Some(TodoSummary {
            todo_id: todo_id.to_owned(),
            store: self.store.clone(),
            title: live.spec.title.clone(),
            status,
            declared_status: live.declared,
            parent: live.spec.parent.clone(),
            department: live.spec.department.clone(),
            priority: live.spec.priority,
            comments: live.comments.len() as u64,
            created_ms: live.created_ms,
            extra: Extensions::new(),
        })
    }

    /// The Todos this store holds, filtered. `total` is every Todo BEFORE
    /// the filter, so a short answer is never read as a short store.
    #[must_use]
    pub fn list(&self, request: &ListRequest) -> TodoList {
        let todos: Vec<TodoSummary> =
            self.live
                .keys()
                .filter_map(|todo_id| self.summary(todo_id))
                .filter(|summary| {
                    request.status.is_none_or(|status| summary.status == status)
                        && request.department.as_deref().is_none_or(|department| {
                            summary.department.as_deref() == Some(department)
                        })
                        && request
                            .parent
                            .as_deref()
                            .is_none_or(|parent| summary.parent.as_deref() == Some(parent))
                        && (!request.roots_only || summary.parent.is_none())
                })
                .collect();
        TodoList {
            api_version: API_VERSION.to_owned(),
            store: self.store.clone(),
            todos,
            total: self.live.len() as u64,
            extra: Extensions::new(),
        }
    }

    /// One Todo and everything parented beneath it. The tree terminates
    /// because a parent must already exist when a child is created, so no
    /// Todo can be its own ancestor.
    #[must_use]
    pub fn tree(&self, todo_id: &str) -> Option<Tree> {
        let root = self.node(todo_id)?;
        Some(Tree::of(&self.store, root))
    }

    fn node(&self, todo_id: &str) -> Option<TreeNode> {
        let todo = self.summary(todo_id)?;
        let children = self
            .live
            .iter()
            .filter(|(_, live)| live.spec.parent.as_deref() == Some(todo_id))
            .filter_map(|(child, _)| self.node(child))
            .collect();
        Some(TreeNode { todo, children })
    }

    /// One page of a Todo's event feed, from `after` onward.
    #[must_use]
    pub fn events_since(
        &self,
        todo_id: &str,
        after: Option<u64>,
        limit: Option<u64>,
    ) -> Option<EventPage> {
        let live = self.live.get(todo_id)?;
        let limit = usize::try_from(limit.unwrap_or(EVENT_RING as u64)).unwrap_or(EVENT_RING);
        let events: Vec<TodoEvent> = live
            .events
            .iter()
            .filter(|event| after.is_none_or(|after| event.seq > after))
            .take(limit)
            .cloned()
            .collect();
        let next_after = events
            .last()
            .map(|event| event.seq)
            .or(after)
            .unwrap_or_default();
        Some(EventPage {
            api_version: API_VERSION.to_owned(),
            todo_id: todo_id.to_owned(),
            events,
            next_after,
            dropped: live.dropped,
            extra: Extensions::new(),
        })
    }

    /// The event a status move puts on the bus, including the `closed`
    /// that a terminal move implies. One place, so a provider cannot emit
    /// a move without the close that follows it.
    #[must_use]
    pub fn move_events(change: &StatusChange) -> Vec<EventKind> {
        let mut events = vec![EventKind::StatusChanged {
            from: change.from,
            to: change.to,
            actor: change.actor.clone(),
        }];
        if change.to.is_terminal() {
            events.push(EventKind::Closed { status: change.to });
        }
        events
    }
}

/// The `kind` an [`Event`] carries, for a consumer matching on one.
#[must_use]
pub fn event_kind(event: &Event) -> &EventKind {
    &event.kind
}
