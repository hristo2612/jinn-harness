//! The seam's ledger law, proven where it lives: the registry, the
//! journal's replay, and the fold between them.

use super::*;
use crate::journal::{replay, Kind, Record, INTERRUPTED_REASON};

fn spec(title: &str) -> TodoSpec {
    TodoSpec {
        title: title.to_owned(),
        ..TodoSpec::default()
    }
}

fn store() -> Todos {
    Todos::new("default")
}

/// Plan, record, commit — the three steps every provider takes, with a
/// journal that accepted the line. Every mutation in these tests goes
/// through one of these, because that is the only order the seam has.
fn landed(
    todos: &mut Todos,
    todo: &str,
    to: Status,
    actor: Option<String>,
    note: Option<String>,
    at_ms: u64,
) -> Result<Moved, TodoError> {
    let planned = todos.plan_update(todo, to, actor, note, at_ms)?;
    match &planned {
        Moved::Changed(change) => todos.commit_change(todo, change),
        Moved::Refused(refused, _) => todos.commit_refusal(todo, refused),
    }
    Ok(planned)
}

/// See [`landed`].
fn landed_create(todos: &mut Todos, spec: TodoSpec, at_ms: u64) -> Result<TodoCreated, TodoError> {
    let created = todos.plan_create(&spec, at_ms)?;
    todos.commit_create(&created, spec, at_ms);
    Ok(created)
}

/// See [`landed`].
fn landed_comment(
    todos: &mut Todos,
    todo: &str,
    body: &str,
    actor: Option<String>,
    at_ms: u64,
) -> Result<Comment, TodoError> {
    let comment = todos.plan_comment(todo, body, actor, at_ms)?;
    todos.commit_comment(todo, &comment);
    Ok(comment)
}

/// See [`landed`]. Answers the dispatch and the move it implied.
fn landed_dispatch(
    todos: &mut Todos,
    todo: &str,
    spec: &DispatchSpec,
    actor: Option<String>,
    at_ms: u64,
) -> Result<(Dispatch, Option<StatusChange>), TodoError> {
    match todos.plan_dispatch(todo, spec, actor, at_ms)? {
        Dispatching::Opens { change, dispatch } => {
            if let Some(change) = &change {
                todos.commit_change(todo, change);
            }
            todos.commit_dispatch(todo, &dispatch);
            Ok((dispatch, change))
        }
        Dispatching::Refused(refused, error) => {
            todos.commit_refusal(todo, &refused);
            Err(error)
        }
    }
}

/// See [`landed`].
fn landed_end(
    todos: &mut Todos,
    todo: &str,
    dispatch_id: &str,
    status: DispatchStatus,
    reason: Option<String>,
    answer: &str,
    at_ms: u64,
) -> Result<Dispatch, TodoError> {
    let ended =
        todos.plan_end_dispatch(todo, dispatch_id, status, reason, answer.to_owned(), at_ms)?;
    todos.commit_end_dispatch(todo, &ended);
    Ok(ended)
}

/// A registry holding one Todo, with its id.
fn one(title: &str) -> (Todos, String) {
    let mut todos = store();
    let created = landed_create(&mut todos, spec(title), 10).expect("a Todo");
    (todos, created.todo_id)
}

/// A journal document from lines, as the durable store writes it.
fn document(records: &[Record]) -> Vec<u8> {
    records.iter().flat_map(Record::line).collect()
}

fn created_line(title: &str) -> Record {
    Record::created(spec(title), 10)
}

fn moved(from: Status, to: Status, at_ms: u64) -> Record {
    Record::status_changed(
        &StatusChange {
            seq: 0,
            from,
            to,
            actor: Some("planner".to_owned()),
            note: None,
            at_ms,
            extra: Extensions::new(),
        },
        at_ms,
    )
    .expect("a legal move")
}

#[test]
fn an_illegal_move_is_refused_and_recorded_in_the_same_breath() {
    let (mut todos, todo) = one("port the ledger");
    // Get it to `executing` the legal way.
    assert!(matches!(
        landed(&mut todos, &todo, Status::Executing, None, None, 20),
        Ok(Moved::Changed(_))
    ));
    // A producer closing their own work.
    let Ok(Moved::Refused(recorded, error)) = landed(
        &mut todos,
        &todo,
        Status::Done,
        Some("the producer".to_owned()),
        None,
        30,
    ) else {
        panic!("executing -> done is not legal");
    };
    assert_eq!(error.code, ErrorCode::Refused);
    assert_eq!(error.extra["from"], "executing");
    assert_eq!(error.extra["to"], "done");
    assert_eq!(
        (recorded.from, recorded.to),
        (Status::Executing, Status::Done)
    );
    assert_eq!(recorded.actor.as_deref(), Some("the producer"));
    // LEDGERED: the attempt is on the record, and the status did not move.
    let record = todos.record(&todo).expect("a record");
    assert_eq!(record.refused.len(), 1);
    assert_eq!(record.refused[0].to, Status::Done);
    assert_eq!(record.declared_status, Status::Executing);
    assert_eq!(record.status, Status::Executing);
    // And nothing was coerced: the history holds ONE move, the legal one.
    assert_eq!(record.history.len(), 1);
    assert_eq!(record.history[0].to, Status::Executing);
}

#[test]
fn history_is_append_only_and_a_move_is_a_new_line() {
    let (mut todos, todo) = one("port the ledger");
    for to in [Status::Executing, Status::InReview, Status::Done] {
        assert!(matches!(
            landed(&mut todos, &todo, to, None, None, 20),
            Ok(Moved::Changed(_))
        ));
    }
    let record = todos.record(&todo).expect("a record");
    assert_eq!(record.history.len(), 3);
    assert_eq!(
        record
            .history
            .iter()
            .map(|change| (change.from, change.to))
            .collect::<Vec<_>>(),
        vec![
            (Status::Backlog, Status::Executing),
            (Status::Executing, Status::InReview),
            (Status::InReview, Status::Done),
        ]
    );
    // The sequence numbers are the order, and nothing was rewritten.
    assert_eq!(
        record
            .history
            .iter()
            .map(|change| change.seq)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    // A terminal Todo is terminal: the next move is refused, not applied.
    assert!(matches!(
        landed(&mut todos, &todo, Status::Executing, None, None, 40),
        Ok(Moved::Refused(..))
    ));
    assert_eq!(
        todos.record(&todo).expect("a record").declared_status,
        Status::Done
    );
}

#[test]
fn a_todo_whose_dispatch_was_interrupted_never_reads_as_still_executing() {
    // The journal a daemon killed mid-dispatch leaves behind: a Todo, the
    // move to executing, a dispatch that started, and nothing more.
    let document = document(&[
        created_line("port the ledger"),
        moved(Status::Backlog, Status::Executing, 20),
        Record::dispatch_started(
            &Dispatch {
                dispatch_id: "default-1-d1".to_owned(),
                session_store: "default".to_owned(),
                engine: "echo".to_owned(),
                ..Dispatch::default()
            },
            30,
        ),
    ]);
    let replayed = replay(&document)
        .expect("it replays")
        .expect("a record this document proves");
    assert_eq!(replayed.declared_status, Status::Executing);
    let dispatch = replayed.dispatches.last().expect("a dispatch");
    // RECORDED interrupted, with a reason, by construction.
    assert_eq!(dispatch.status, DispatchStatus::Interrupted);
    assert_eq!(dispatch.reason.as_deref(), Some(INTERRUPTED_REASON));

    let mut todos = store();
    todos.adopt("default-1", replayed);
    let record = todos.record("default-1").expect("a record");
    // The fold: never eternally executing, and the reason is given.
    assert_eq!(record.status, Status::Blocked);
    assert_eq!(
        record.status_reason.as_deref(),
        Some(INTERRUPTED_STATUS_REASON)
    );
    // History is NOT rewritten — what was declared is still readable.
    assert_eq!(record.declared_status, Status::Executing);
    assert_eq!(record.history.len(), 1);
}

#[test]
fn the_recovery_is_a_new_event_and_makes_the_ledger_usable_again() {
    // The fold alone leaves the ledger unusable: a reader is shown
    // `blocked` while the RECORD still stands at `executing`, so the move
    // the reader is offered is refused by the table. `recover` closes
    // that by making the fold a real, recorded move.
    let document = document(&[
        created_line("port the ledger"),
        moved(Status::Backlog, Status::Executing, 20),
        Record::dispatch_started(
            &Dispatch {
                dispatch_id: "default-1-d1".to_owned(),
                ..Dispatch::default()
            },
            30,
        ),
    ]);
    let mut todos = store();
    todos.adopt(
        "default-1",
        replay(&document)
            .expect("replays")
            .expect("a record this document proves"),
    );
    // Before recovery: the fold reports blocked and explains itself.
    let folded = todos.record("default-1").expect("a record");
    assert_eq!(folded.status, Status::Blocked);
    assert_eq!(folded.declared_status, Status::Executing);

    let change = todos.plan_recovery("default-1", 40).expect("a recovery");
    todos.commit_change("default-1", &change);
    assert_eq!(
        (change.from, change.to),
        (Status::Executing, Status::Blocked)
    );
    assert_eq!(change.note.as_deref(), Some(INTERRUPTED_STATUS_REASON));
    // Nobody asked for it, and the record says so rather than naming a
    // principal that did not act.
    assert_eq!(change.actor, None);

    let after = todos.record("default-1").expect("a record");
    assert_eq!(after.status, Status::Blocked);
    assert_eq!(after.declared_status, Status::Blocked);
    // The two agree, so there is nothing left to explain away.
    assert_eq!(after.status_reason, None);
    // APPEND-ONLY: the move that started the work is still exactly as it
    // was written, and the recovery is a new line after it.
    assert_eq!(after.history.len(), 2);
    assert_eq!(
        (after.history[0].from, after.history[0].to),
        (Status::Backlog, Status::Executing)
    );
    // And the ledger is usable: the move a reader is now offered is one
    // the table admits from where the record actually stands.
    assert!(matches!(
        landed(&mut todos, "default-1", Status::Executing, None, None, 50),
        Ok(Moved::Changed(_))
    ));
    // A Todo that owes nothing recovers nothing — no empty line is
    // appended to a history that did not change.
    let (clean, todo) = one("nothing to recover");
    assert!(clean.plan_recovery(&todo, 60).is_none());
}

#[test]
fn running_is_unreachable_from_a_document() {
    // Every dispatch line a document can hold, replayed: none produces a
    // dispatch this incarnation claims to be driving.
    let document = document(&[
        created_line("port the ledger"),
        moved(Status::Backlog, Status::Executing, 20),
        Record::dispatch_started(
            &Dispatch {
                dispatch_id: "d1".to_owned(),
                ..Dispatch::default()
            },
            30,
        ),
        Record::dispatch_ended(
            &Dispatch {
                dispatch_id: "d1".to_owned(),
                status: DispatchStatus::Done,
                answer: "did it".to_owned(),
                ..Dispatch::default()
            },
            40,
        )
        .expect("a terminal ending"),
    ]);
    let replayed = replay(&document)
        .expect("it replays")
        .expect("a record this document proves");
    assert!(replayed
        .dispatches
        .iter()
        .all(|dispatch| dispatch.status.is_terminal()));
    assert_eq!(replayed.dispatches[0].answer, "did it");
    // And the writer will not even produce the line that would say so.
    let refused = Record::dispatch_ended(
        &Dispatch {
            dispatch_id: "d1".to_owned(),
            status: DispatchStatus::Running,
            ..Dispatch::default()
        },
        50,
    )
    .expect_err("running is not an ending");
    assert!(refused.contains("not an ending"), "{refused}");
}

#[test]
fn a_dispatch_that_claims_to_have_ended_running_is_refused_by_the_reader_too() {
    let mut line = Record::dispatch_started(
        &Dispatch {
            dispatch_id: "d1".to_owned(),
            ..Dispatch::default()
        },
        30,
    );
    line.kind = Kind::DispatchEnded;
    line.dispatch_status = Some(DispatchStatus::Running);
    let document = document(&[created_line("t"), line]);
    let refused = replay(&document).expect_err("a hole, not a tear");
    assert!(refused.contains("cannot be Running"), "{refused}");
}

#[test]
fn a_document_holding_an_illegal_move_did_not_come_from_this_seam() {
    // Hand-built, because `Record::status_changed` refuses to write it.
    let mut illegal = moved(Status::Backlog, Status::Executing, 20);
    illegal.to = Some(Status::Done);
    let refused = replay(&document(&[created_line("t"), illegal]))
        .expect_err("the table holds on the way back in");
    assert!(refused.contains("backlog -> done"), "{refused}");

    // And a move that starts from somewhere the Todo has never been.
    let refused = replay(&document(&[
        created_line("t"),
        moved(Status::InReview, Status::Done, 20),
    ]))
    .expect_err("a move from the wrong place");
    assert!(refused.contains("stood at backlog"), "{refused}");
}

#[test]
fn a_torn_tail_is_absence_and_a_hole_is_corruption() {
    let whole = document(&[
        created_line("port the ledger"),
        moved(Status::Backlog, Status::Executing, 20),
    ]);
    // A tail written short: the last line, unterminated.
    let mut torn = whole.clone();
    torn.extend_from_slice(br#"{"kind":"status-cha"#);
    let replayed = replay(&torn)
        .expect("a tear reads as absence")
        .expect("a record this document proves");
    assert_eq!(replayed.declared_status, Status::Executing);
    assert_eq!(replayed.torn_tail_bytes, 19);
    // A hole EARLIER is not a tear.
    let mut holed = Vec::new();
    holed.extend_from_slice(&document(&[created_line("t")]));
    holed.extend_from_slice(b"{\"kind\":\"stat\n");
    holed.extend_from_slice(&document(&[moved(Status::Backlog, Status::Executing, 20)]));
    assert!(replay(&holed).is_err(), "a hole is corruption");
}

#[test]
fn a_journal_that_does_not_open_with_the_todo_is_refused() {
    let document = document(&[moved(Status::Backlog, Status::Executing, 20)]);
    let refused = replay(&document).expect_err("a journal opens with a `created`");
    assert!(refused.contains("opens with"), "{refused}");
    // An empty document is an empty Todo, not a corrupt one.
    assert!(replay(b"").is_ok());
}

#[test]
fn a_dispatch_moves_the_todo_through_the_same_table_as_everything_else() {
    let (mut todos, todo) = one("port the ledger");
    let dispatch_spec = DispatchSpec {
        store: "default".to_owned(),
        engine: jinn_session::EngineBinding {
            engine: "echo".to_owned(),
            ..jinn_session::EngineBinding::default()
        },
        ..DispatchSpec::default()
    };
    let (dispatch, change) = landed_dispatch(
        &mut todos,
        &todo,
        &dispatch_spec,
        Some("planner".to_owned()),
        20,
    )
    .expect("a dispatch");
    assert_eq!(dispatch.status, DispatchStatus::Running);
    let change = change.expect("backlog -> executing is a move");
    assert_eq!(
        (change.from, change.to),
        (Status::Backlog, Status::Executing)
    );
    // A second dispatch while one is in flight is refused.
    let refused =
        landed_dispatch(&mut todos, &todo, &dispatch_spec, None, 21).expect_err("one at a time");
    assert_eq!(refused.code, ErrorCode::Refused);
    // A terminal Todo is not dispatched: the table refuses the move.
    let (mut done, other) = one("already closed");
    for to in [Status::Executing, Status::InReview, Status::Done] {
        landed(&mut done, &other, to, None, None, 20).expect("legal");
    }
    let refused = landed_dispatch(&mut done, &other, &dispatch_spec, None, 30)
        .expect_err("a closed Todo is not dispatched");
    assert_eq!(refused.code, ErrorCode::Refused);
    assert_eq!(refused.extra["from"], "done");
}

#[test]
fn no_dispatch_ending_but_done_is_ever_recorded_without_a_reason() {
    let (mut todos, todo) = one("port the ledger");
    let (dispatch, _) =
        landed_dispatch(&mut todos, &todo, &DispatchSpec::default(), None, 20).expect("a dispatch");
    for status in [
        DispatchStatus::Failed,
        DispatchStatus::Cancelled,
        DispatchStatus::Interrupted,
    ] {
        let refused = landed_end(
            &mut todos,
            &todo,
            &dispatch.dispatch_id,
            status,
            None,
            "",
            30,
        )
        .expect_err("an ending explains itself");
        assert_eq!(refused.code, ErrorCode::Invalid);
    }
    // And `running` is not an ending at all.
    assert!(landed_end(
        &mut todos,
        &todo,
        &dispatch.dispatch_id,
        DispatchStatus::Running,
        Some("x".to_owned()),
        "",
        30
    )
    .is_err());
    let ended = landed_end(
        &mut todos,
        &todo,
        &dispatch.dispatch_id,
        DispatchStatus::Done,
        None,
        "did it",
        30,
    )
    .expect("done needs no reason, only proof");
    assert_eq!(ended.status, DispatchStatus::Done);
    assert_eq!(ended.answer, "did it");
    // A Todo whose dispatch ENDED is exactly where its history says.
    let record = todos.record(&todo).expect("a record");
    assert_eq!(record.status, Status::Executing);
    assert!(record.status_reason.is_none());
}

#[test]
fn a_tree_is_acyclic_because_a_parent_must_already_exist() {
    let mut todos = store();
    let root = landed_create(&mut todos, spec("root"), 10)
        .expect("a Todo")
        .todo_id;
    let child = landed_create(
        &mut todos,
        TodoSpec {
            parent: Some(root.clone()),
            ..spec("child")
        },
        11,
    )
    .expect("a child")
    .todo_id;
    landed_create(
        &mut todos,
        TodoSpec {
            parent: Some(child.clone()),
            ..spec("grandchild")
        },
        12,
    )
    .expect("a grandchild");
    // A parent that is not here is a typed refusal, not a dangling edge.
    let refused = landed_create(
        &mut todos,
        TodoSpec {
            parent: Some("default-99".to_owned()),
            ..spec("orphan")
        },
        13,
    )
    .expect_err("no such parent");
    assert_eq!(refused.code, ErrorCode::NotFound);

    let tree = todos.tree(&root).expect("a tree");
    assert_eq!(tree.root.todo.todo_id, root);
    assert_eq!(tree.root.children.len(), 1);
    assert_eq!(tree.root.children[0].children.len(), 1);
    // `roots-only` is the objective view: the root, and nothing beneath.
    let roots = todos.list(&ListRequest {
        roots_only: true,
        ..ListRequest::default()
    });
    assert_eq!(roots.todos.len(), 1);
    // …and `total` still says how many Todos the store holds.
    assert_eq!(roots.total, 3);
}

#[test]
fn a_list_filters_on_the_reported_status_not_the_declared_one() {
    let document = document(&[
        created_line("interrupted work"),
        moved(Status::Backlog, Status::Executing, 20),
        Record::dispatch_started(
            &Dispatch {
                dispatch_id: "d1".to_owned(),
                ..Dispatch::default()
            },
            30,
        ),
    ]);
    let mut todos = store();
    todos.adopt(
        "default-1",
        replay(&document)
            .expect("replays")
            .expect("a record this document proves"),
    );
    let blocked = todos.list(&ListRequest {
        status: Some(Status::Blocked),
        ..ListRequest::default()
    });
    assert_eq!(
        blocked.todos.len(),
        1,
        "the fold is what a reader filters on"
    );
    assert_eq!(blocked.todos[0].declared_status, Status::Executing);
    let executing = todos.list(&ListRequest {
        status: Some(Status::Executing),
        ..ListRequest::default()
    });
    assert!(executing.todos.is_empty());
}

#[test]
fn a_comment_that_says_nothing_is_not_recorded() {
    let (mut todos, todo) = one("port the ledger");
    for blank in ["", "  \n "] {
        assert_eq!(
            landed_comment(&mut todos, &todo, blank, None, 20)
                .expect_err("a blank comment")
                .code,
            ErrorCode::Invalid
        );
    }
    let comment = landed_comment(&mut todos, &todo, "started", Some("planner".to_owned()), 20)
        .expect("a comment");
    assert_eq!(comment.seq, 0);
    assert_eq!(comment.actor.as_deref(), Some("planner"));
    assert_eq!(todos.record(&todo).expect("a record").comments.len(), 1);
}

#[test]
fn an_adopted_id_mints_forward_so_a_later_create_cannot_collide() {
    let mut todos = store();
    todos.adopt(
        "default-7",
        replay(&document(&[created_line("old")]))
            .expect("replays")
            .expect("a record this document proves"),
    );
    let created = landed_create(&mut todos, spec("new"), 30).expect("a Todo");
    assert_eq!(created.todo_id, "default-8");
}

#[test]
fn a_terminal_move_puts_a_close_on_the_bus_beside_it() {
    let change = StatusChange {
        seq: 0,
        from: Status::InReview,
        to: Status::Done,
        actor: None,
        note: None,
        at_ms: 10,
        extra: Extensions::new(),
    };
    let events = Todos::move_events(&change);
    assert_eq!(events.len(), 2);
    assert!(matches!(
        events[1],
        EventKind::Closed {
            status: Status::Done
        }
    ));
    let open = StatusChange {
        to: Status::Executing,
        from: Status::Backlog,
        ..change
    };
    assert_eq!(Todos::move_events(&open).len(), 1);
}

#[test]
fn the_event_feed_is_a_cursor_and_reports_what_it_dropped() {
    let (mut todos, todo) = one("port the ledger");
    for index in 0..(EVENT_RING as u64 + 5) {
        todos.record_event(
            &todo,
            EventKind::Commented {
                comment_id: format!("c{index}"),
                actor: None,
            },
        );
    }
    let page = todos.events_since(&todo, None, Some(10)).expect("a page");
    assert_eq!(page.events.len(), 10);
    assert_eq!(page.dropped, 5, "a feed that lost history says so");
    let next = todos
        .events_since(&todo, Some(page.next_after), Some(10))
        .expect("a page");
    assert!(next.events.iter().all(|event| event.seq > page.next_after));
}

#[test]
fn a_plan_that_was_never_committed_leaves_the_registry_exactly_where_it_was() {
    // The seam's ordering law, at the level it is defined: a plan is a
    // computation, and a store that could not make the record durable
    // simply never calls the commit. Nothing a reader can see moves.
    let (mut todos, todo) = one("port the ledger");
    let before = todos.record(&todo).expect("a record");

    // A legal move, planned and then dropped on the floor — which is
    // exactly what a refused `append` leaves behind.
    let planned = todos
        .plan_update(
            &todo,
            Status::Executing,
            Some("planner".to_owned()),
            None,
            20,
        )
        .expect("a plan");
    assert!(matches!(planned, Moved::Changed(_)));
    let after = todos.record(&todo).expect("a record");
    assert_eq!(after.declared_status, before.declared_status);
    assert_eq!(after.status, before.status);
    assert_eq!(after.history.len(), 0);

    // Planning twice does not accumulate either: the second plan is the
    // same move from the same place, because the first changed nothing.
    let again = todos
        .plan_update(&todo, Status::Executing, None, None, 21)
        .expect("a plan");
    let (Moved::Changed(first), Moved::Changed(second)) = (&planned, &again) else {
        panic!("both plans are the same legal move");
    };
    assert_eq!((first.from, first.to), (second.from, second.to));
    assert_eq!(first.seq, second.seq);

    // A REFUSAL is a plan too, and an uncommitted one leaves the refused
    // list empty — a store that could not record the attempt does not
    // report one.
    let refused = todos
        .plan_update(&todo, Status::Done, None, None, 22)
        .expect("a plan");
    assert!(matches!(refused, Moved::Refused(..)));
    assert!(todos.record(&todo).expect("a record").refused.is_empty());

    // A planned COMMENT and a planned DISPATCH are the same: nothing is
    // there until it is committed.
    todos
        .plan_comment(&todo, "started", None, 23)
        .expect("a plan");
    todos
        .plan_dispatch(&todo, &DispatchSpec::default(), None, 24)
        .expect("a plan");
    let untouched = todos.record(&todo).expect("a record");
    assert!(untouched.comments.is_empty());
    assert!(untouched.dispatches.is_empty());
    assert_eq!(untouched.declared_status, Status::Backlog);

    // And committing what was planned is what moves it — so the law is
    // an ORDER, not a refusal to act.
    let Moved::Changed(change) = planned else {
        unreachable!("checked above")
    };
    todos.commit_change(&todo, &change);
    assert_eq!(
        todos.record(&todo).expect("a record").declared_status,
        Status::Executing
    );
}

#[test]
fn an_append_onto_a_torn_tail_makes_a_hole_the_reader_refuses() {
    // The mechanism `FINDINGS.md` #34 is filed on, reproduced where it
    // can be reproduced deterministically: `jinn:fs` can append and it
    // can rewrite, but it cannot DROP A SUFFIX. A tolerable torn TAIL
    // that is appended onto stops being a tail — it fuses with the new
    // record into one undecodable line in the MIDDLE of the document,
    // which the reader refuses, and the Todo that replayed fine one boot
    // ago no longer replays at all.
    let whole = document(&[
        created_line("port the ledger"),
        moved(Status::Backlog, Status::Executing, 20),
    ]);
    let mut torn = whole.clone();
    torn.extend_from_slice(br#"{"kind":"status-changed","at-ms":30,"from":"exec"#);

    // A torn TAIL alone is absence: the Todo reads back at its last whole
    // line, and the reader says how many bytes were not a record.
    let replayed = replay(&torn)
        .expect("a torn tail is absence, not damage")
        .expect("a record this document proves");
    assert_eq!(replayed.declared_status, Status::Executing);
    assert_eq!(replayed.history.len(), 1);
    assert!(replayed.torn_tail_bytes > 0);

    // Now the next append lands on the END of those bytes, because that
    // is the only thing an append-only contract can do with them.
    let mut fused = torn.clone();
    fused.extend_from_slice(&moved(Status::Executing, Status::InReview, 40).line());
    let refused = replay(&fused).expect_err("a hole in the middle is corruption");
    assert!(
        refused.contains("journal line"),
        "the reader names the line it refused: {refused}"
    );
    // Printed so a run of this test is a TRANSCRIPT of the defect and
    // not only a green tick — `FINDINGS.md` #34 quotes this line.
    eprintln!("append-onto-a-tear replays as: {refused}");

    // Healing the tail FIRST — the whole-document rewrite this store does
    // on adoption — is what makes the same append readable.
    let mut healed = whole.clone();
    healed.extend_from_slice(&moved(Status::Executing, Status::InReview, 40).line());
    let after = replay(&healed)
        .expect("the healed document replays")
        .expect("a record this document proves");
    assert_eq!(after.declared_status, Status::InReview);
    assert_eq!(after.history.len(), 2);
}

#[test]
fn a_document_whose_only_created_line_is_torn_holds_no_todo_at_all() {
    // The same class as `FINDINGS.md` #36 one layer up: a daemon killed
    // inside the very FIRST append leaves bytes that were never a record,
    // and a default `Replayed` — empty spec, default status — is a
    // sentinel that would pass for a Todo nobody created.
    let whole = document(&[created_line("t")]);
    assert_eq!(
        replay(&whole[..1]).expect("a torn first line is absence, not damage"),
        None
    );
    assert_eq!(replay(&[]).expect("an empty document is absence"), None);
}
