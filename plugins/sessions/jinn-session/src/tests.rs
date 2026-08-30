//! The definition's own proofs. The journal ones are the seam's honesty
//! law under test: every case where a status could be invented instead of
//! proven.

use super::*;
use crate::journal::{replay, Kind, Record, INTERRUPTED_REASON};

fn spec(engine: &str) -> SessionSpec {
    SessionSpec {
        engine: EngineBinding {
            engine: engine.to_owned(),
            ..EngineBinding::default()
        },
        ..SessionSpec::default()
    }
}

fn document(records: &[Record]) -> Vec<u8> {
    records.iter().flat_map(Record::line).collect()
}

#[test]
fn a_contract_name_carries_the_store_id_and_gives_it_back() {
    assert_eq!(store_contract("fs"), "jinn:session.fs");
    assert_eq!(store_id_of("jinn:session.memory"), Some("memory"));
    assert_eq!(store_id_of("jinn:session."), None);
    assert_eq!(store_id_of("jinn:engine.claude"), None);
}

#[test]
fn the_stores_of_a_composition_are_the_kernels_own_view() {
    let stores = stores_in([
        ("sessions-memory", vec!["jinn:session.memory"]),
        ("sessions-fs", vec!["jinn:session.fs", "jinn:cron"]),
        ("engine-echo", vec!["jinn:engine.echo"]),
    ]);
    assert_eq!(
        stores
            .iter()
            .map(|slot| (slot.store.as_str(), slot.entry.as_str()))
            .collect::<Vec<_>>(),
        [("fs", "sessions-fs"), ("memory", "sessions-memory")]
    );
}

#[test]
fn a_started_turn_with_no_ending_replays_interrupted_never_running() {
    let document = document(&[
        Record::created(spec("echo"), 10),
        Record::turn_started("t1", "hello", 20),
    ]);
    let replayed = replay(&document)
        .expect("replays")
        .expect("a record this document proves");
    assert_eq!(replayed.turns.len(), 1);
    assert_eq!(replayed.turns[0].status, TurnStatus::Interrupted);
    assert_eq!(
        replayed.turns[0].reason.as_deref(),
        Some(INTERRUPTED_REASON),
        "the interruption names itself"
    );
    assert_eq!(replayed.status(), SessionStatus::Failed);
}

#[test]
fn only_a_terminal_record_can_make_a_turn_done() {
    let ended = Turn {
        turn_id: "t1".to_owned(),
        status: TurnStatus::Done,
        answer: "hi".to_owned(),
        ..Turn::default()
    };
    let document = document(&[
        Record::created(spec("echo"), 10),
        Record::turn_started("t1", "hello", 20),
        Record::turn_ended(&ended, 30).expect("a terminal turn records"),
    ]);
    let replayed = replay(&document)
        .expect("replays")
        .expect("a record this document proves");
    assert_eq!(replayed.turns[0].status, TurnStatus::Done);
    assert_eq!(replayed.turns[0].answer, "hi");
    assert_eq!(replayed.status(), SessionStatus::Idle);
}

#[test]
fn a_running_turn_cannot_be_written_as_an_ending() {
    let running = Turn {
        turn_id: "t1".to_owned(),
        status: TurnStatus::Running,
        ..Turn::default()
    };
    let refused = Record::turn_ended(&running, 30).expect_err("running is not an ending");
    assert!(refused.contains("not an ending"), "{refused}");
}

#[test]
fn a_torn_tail_reads_as_absence_and_says_how_many_bytes() {
    let whole = document(&[
        Record::created(spec("echo"), 10),
        Record::turn_started("t1", "hello", 20),
    ]);
    let mut torn = whole.clone();
    torn.extend_from_slice(br#"{"kind":"turn-ende"#);
    let replayed = replay(&torn)
        .expect("a torn tail is absence, not damage")
        .expect("a record this document proves");
    assert_eq!(replayed.turns.len(), 1);
    assert_eq!(replayed.turns[0].status, TurnStatus::Interrupted);
    assert_eq!(replayed.torn_tail_bytes, torn.len() - whole.len());
}

#[test]
fn a_hole_in_the_middle_is_refused_not_skipped() {
    let mut document = document(&[Record::created(spec("echo"), 10)]);
    document.extend_from_slice(b"{not a record}\n");
    document.extend_from_slice(&Record::turn_started("t1", "hello", 20).line());
    let refused = replay(&document).expect_err("a hole is corruption, not a tear");
    assert!(refused.contains("journal line 2"), "{refused}");
}

#[test]
fn an_ending_for_a_turn_that_never_started_is_refused() {
    let ended = Turn {
        turn_id: "ghost".to_owned(),
        status: TurnStatus::Done,
        ..Turn::default()
    };
    let document = document(&[
        Record::created(spec("echo"), 10),
        Record::turn_ended(&ended, 30).expect("records"),
    ]);
    let refused = replay(&document).expect_err("no such turn");
    assert!(refused.contains("never started"), "{refused}");
}

#[test]
fn a_journal_kind_this_version_cannot_name_is_refused() {
    let refused = serde_json::from_str::<Record>(r#"{"kind":"vacuumed","at-ms":1}"#)
        .expect_err("a closed surface refuses");
    let message = refused.to_string();
    assert!(message.contains("closed surface"), "{message}");
    assert!(message.contains("`kind`"), "{message}");
}

#[test]
fn a_replayed_session_is_adopted_without_becoming_runnable_again() {
    let document = document(&[
        Record::created(spec("echo"), 10),
        Record::turn_started("t1", "hello", 20),
    ]);
    let replayed = replay(&document)
        .expect("replays")
        .expect("a record this document proves");
    let mut sessions = Sessions::new("fs");
    sessions.adopt("fs-7", replayed);
    let record = sessions.record("fs-7").expect("adopted");
    assert_eq!(record.status, SessionStatus::Failed);
    assert_eq!(record.log[0].status, TurnStatus::Interrupted);
    // The adopted turn is terminal, so the session accepts a new one and
    // the mint does not collide with the id it was adopted under.
    let created = sessions.create(spec("echo"), 40);
    assert_eq!(created.session_id, "fs-8");
    sessions.send("fs-7", "again", 50).expect("a new turn");
}

#[test]
fn a_closed_session_refuses_a_send_and_one_turn_runs_at_a_time() {
    let mut sessions = Sessions::new("memory");
    let created = sessions.create(spec("echo"), 1);
    sessions.send(&created.session_id, "one", 2).expect("first");
    let busy = sessions
        .send(&created.session_id, "two", 3)
        .expect_err("one at a time");
    assert_eq!(busy.code, ErrorCode::Refused);
    let turn = sessions
        .in_flight(&created.session_id)
        .expect("in flight")
        .turn_id
        .clone();
    sessions
        .end_turn(&created.session_id, &turn, TurnStatus::Done, None, 4)
        .expect("ends");
    sessions.close(&created.session_id).expect("closes");
    let closed = sessions
        .send(&created.session_id, "three", 5)
        .expect_err("closed");
    assert_eq!(closed.code, ErrorCode::Refused);
    assert_eq!(
        sessions.record(&created.session_id).expect("record").status,
        SessionStatus::Closed
    );
}

#[test]
fn an_ending_that_is_not_done_must_carry_its_reason() {
    let mut sessions = Sessions::new("memory");
    let created = sessions.create(spec("echo"), 1);
    let accepted = sessions.send(&created.session_id, "one", 2).expect("turn");
    let refused = sessions
        .end_turn(
            &created.session_id,
            &accepted.turn_id,
            TurnStatus::Failed,
            None,
            3,
        )
        .expect_err("a failure names itself");
    assert_eq!(refused.code, ErrorCode::Invalid);
}

#[test]
fn a_page_names_a_next_only_when_there_is_one() {
    let mut sessions = Sessions::new("memory");
    let created = sessions.create(spec("echo"), 1);
    for index in 0..3 {
        let accepted = sessions
            .send(&created.session_id, &format!("m{index}"), 2)
            .expect("turn");
        sessions
            .end_turn(
                &created.session_id,
                &accepted.turn_id,
                TurnStatus::Done,
                None,
                3,
            )
            .expect("ends");
    }
    let first = sessions
        .page(&created.session_id, 0, Some(2))
        .expect("a page");
    assert_eq!(first.messages.len(), 2);
    assert_eq!(first.total, 3);
    assert_eq!(first.next_offset, Some(2));
    let last = sessions
        .page(&created.session_id, 2, Some(2))
        .expect("a page");
    assert_eq!(last.messages.len(), 1);
    assert_eq!(last.next_offset, None, "absence is the end of the log");
}

#[test]
fn a_status_this_version_cannot_name_is_refused_by_case() {
    let refused = serde_json::from_str::<TurnStatus>("\"vibing\"").expect_err("closed");
    assert!(refused.to_string().contains("closed surface"));
    let refused = serde_json::from_str::<SessionStatus>("\"vibing\"").expect_err("closed");
    assert!(refused.to_string().contains("closed surface"));
}

#[test]
fn every_closed_variant_round_trips_through_its_own_encoding() {
    for status in [
        TurnStatus::Running,
        TurnStatus::Done,
        TurnStatus::Failed,
        TurnStatus::Cancelled,
        TurnStatus::Interrupted,
    ] {
        let encoded = serde_json::to_string(&status).expect("encodes");
        assert_eq!(
            serde_json::from_str::<TurnStatus>(&encoded).expect("decodes"),
            status
        );
    }
    for kind in [
        Kind::Created,
        Kind::TurnStarted,
        Kind::TurnEnded,
        Kind::Closed,
    ] {
        let encoded = serde_json::to_string(&kind).expect("encodes");
        assert_eq!(
            serde_json::from_str::<Kind>(&encoded).expect("decodes"),
            kind
        );
    }
}

#[test]
fn a_journal_that_claims_a_live_turn_is_refused_not_believed() {
    // The WRITER refuses `running` (`Record::turn_ended`), so this line
    // cannot come from this seam. It can come from a corrupted byte, a
    // half-migrated document, or a future version that means something
    // else by it — and a reader that believes it hands back a session
    // eternally "working" that nothing will ever finish. The dangerous
    // answer needs proof, so the READER refuses it too. The turn is
    // properly STARTED here: the only thing wrong with this document is
    // the status on its ending.
    let mut hostile = Record::turn_started("t1", "hello", 3);
    hostile.kind = Kind::TurnEnded;
    hostile.status = Some(TurnStatus::Running);
    hostile.message = None;
    let claimed_live = document(&[
        Record::created(spec("echo"), 1),
        Record::turn_started("t1", "hello", 2),
        hostile,
    ]);
    let refused = replay(&claimed_live).expect_err("a live turn is not an ending");
    assert!(
        refused.contains("journal line 3") && refused.contains("Running"),
        "the refusal names the line and what it claimed: {refused}"
    );
    // The same document WITHOUT that line replays clean, which is what
    // makes the refusal above attributable to the status and to nothing
    // else.
    let honest = document(&[
        Record::created(spec("echo"), 1),
        Record::turn_started("t1", "hello", 2),
    ]);
    assert_eq!(
        replay(&honest)
            .expect("a started turn")
            .expect("a record this document proves")
            .turns[0]
            .status,
        TurnStatus::Interrupted
    );
}

#[test]
fn a_terminal_record_with_no_reason_keeps_the_conservative_one() {
    // A non-`done` ending carries a reason by the registry's rule
    // (`Sessions::end_turn`). A journal line that ends one WITHOUT a
    // reason is not proof that there was none: the reader keeps the
    // conservative reason the started turn already carried rather than
    // reporting an ending nobody can explain.
    let mut ended = Record::turn_started("t1", "hello", 3);
    ended.kind = Kind::TurnEnded;
    ended.status = Some(TurnStatus::Failed);
    ended.message = None;
    let replayed = replay(&document(&[
        Record::created(spec("echo"), 1),
        Record::turn_started("t1", "hello", 2),
        ended,
    ]))
    .expect("a complete journal")
    .expect("a record this document proves");
    let turn = &replayed.turns[0];
    assert_eq!(turn.status, TurnStatus::Failed);
    assert_eq!(
        turn.reason.as_deref(),
        Some(INTERRUPTED_REASON),
        "an ending with no reason keeps the one the turn already had"
    );
    // A `done` needs no reason and gets none: it is the one status whose
    // whole claim is that the answer is there.
    let mut done = Record::turn_started("t1", "hello", 3);
    done.kind = Kind::TurnEnded;
    done.status = Some(TurnStatus::Done);
    done.message = None;
    let replayed = replay(&document(&[
        Record::created(spec("echo"), 1),
        Record::turn_started("t1", "hello", 2),
        done,
    ]))
    .expect("a complete journal")
    .expect("a record this document proves");
    assert_eq!(replayed.turns[0].reason, None);
}

#[test]
fn a_feed_hands_back_every_event_once_and_says_what_it_dropped() {
    let mut sessions = Sessions::new("memory");
    let created = sessions.create(spec("echo"), 1);
    let id = created.session_id.clone();
    sessions.record_event(
        &id,
        EventKind::Created {
            engine: "echo".to_owned(),
        },
    );
    for index in 0..3_u64 {
        sessions.record_event(
            &id,
            EventKind::Delta {
                turn_id: "t1".to_owned(),
                text: format!("d{index}"),
            },
        );
    }
    let first = sessions.events_since(&id, None, Some(2)).expect("a page");
    assert_eq!(first.events.len(), 2);
    assert_eq!(first.next_after, 1);
    assert_eq!(first.dropped, 0);
    let next = sessions
        .events_since(&id, Some(first.next_after), Some(10))
        .expect("a page");
    assert_eq!(next.events.len(), 2, "everything after the cursor, once");
    assert_eq!(next.next_after, 3);
    // A caught-up reader keeps its cursor rather than being sent back to
    // the start of a feed it has already read.
    let caught_up = sessions
        .events_since(&id, Some(next.next_after), None)
        .expect("a page");
    assert!(caught_up.events.is_empty());
    assert_eq!(caught_up.next_after, 3);
}

#[test]
fn a_ring_that_overflows_reports_the_gap_rather_than_hiding_it() {
    let mut sessions = Sessions::new("memory");
    let created = sessions.create(spec("echo"), 1);
    let id = created.session_id.clone();
    for index in 0..(EVENT_RING as u64 + 5) {
        sessions.record_event(
            &id,
            EventKind::Delta {
                turn_id: "t1".to_owned(),
                text: format!("d{index}"),
            },
        );
    }
    let page = sessions.events_since(&id, None, Some(1)).expect("a page");
    assert_eq!(page.dropped, 5, "a feed that lost history says so");
    assert_eq!(
        page.events[0].seq, 5,
        "the sequence is the session's, not the ring's index"
    );
}

#[test]
fn a_document_whose_only_created_line_is_torn_holds_no_session_at_all() {
    // The same class as `FINDINGS.md` #36 two layers up: bytes that were
    // never a record are the absence of the session, not a session
    // nobody created sitting `idle`.
    let whole = document(&[Record::created(spec("echo"), 10)]);
    assert_eq!(
        replay(&whole[..1]).expect("a torn first line is absence, not damage"),
        None
    );
    assert_eq!(replay(&[]).expect("an empty document is absence"), None);
}
