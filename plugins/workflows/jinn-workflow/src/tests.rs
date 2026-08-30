//! The seam's law, proven on the host: the pin, the node-state table as
//! the registry applies it, the graph walk, the journal's record law, and
//! the recovery that makes "never eternally `running`" a property of the
//! ORDER a store activates in rather than of a provider remembering.

use super::*;

const NOW: u64 = 1_000;

fn checkpoint(id: &str) -> NodeSpec {
    NodeSpec {
        id: id.to_owned(),
        kind: NodeKind::Checkpoint,
        ..NodeSpec::default()
    }
}

fn dispatch_node(id: &str) -> NodeSpec {
    NodeSpec {
        id: id.to_owned(),
        kind: NodeKind::Dispatch,
        title: id.to_owned(),
        todo: Some(TodoBinding {
            store: "default".to_owned(),
            dispatch: jinn_todo::DispatchSpec {
                store: "default".to_owned(),
                ..jinn_todo::DispatchSpec::default()
            },
            ..TodoBinding::default()
        }),
        ..NodeSpec::default()
    }
}

fn edge(from: &str, to: &str, kind: EdgeKind) -> EdgeSpec {
    EdgeSpec {
        from: from.to_owned(),
        to: to.to_owned(),
        kind,
        ..EdgeSpec::default()
    }
}

/// `open -> run-it` on done, `open -> bail` on not-done. Two lanes, so a
/// skip is a positive reading of a decided graph.
fn two_lane_spec(name: &str) -> WorkflowSpec {
    WorkflowSpec {
        name: name.to_owned(),
        nodes: vec![
            checkpoint("open"),
            dispatch_node("run-it"),
            checkpoint("bail"),
        ],
        edges: vec![
            edge("open", "run-it", EdgeKind::OnDone),
            edge("open", "bail", EdgeKind::OnNotDone),
        ],
        ..WorkflowSpec::default()
    }
}

/// A registry holding one workflow at revision 1, and the definition.
fn defined(spec: WorkflowSpec) -> (Workflows, Definition) {
    let mut registry = Workflows::new("default");
    let definition = registry
        .plan_define(&spec, None, NOW)
        .expect("a spec this seam records");
    registry.commit_define(&definition);
    (registry, definition)
}

fn started(registry: &mut Workflows, workflow_id: &str, revision: Option<u64>) -> String {
    let request = StartRequest {
        workflow_id: workflow_id.to_owned(),
        revision,
        ..StartRequest::default()
    };
    let plan = registry.plan_start(&request).expect("a run opens");
    registry.commit_start(&plan, NOW);
    plan.run_id
}

/// Moves a node, journaling nothing (the registry's half alone).
fn move_node(registry: &mut Workflows, run: &str, node: &str, to: NodeState) {
    match registry
        .plan_node_move(run, node, to, None, Some("a note".to_owned()), NOW)
        .expect("planned")
    {
        Moved::Changed(change) => registry.commit_node_change(run, &change),
        Moved::Refused(_, error) => panic!("{node} -> {to:?} refused: {}", error.message),
    }
}

// ---- the pin ---------------------------------------------------------

#[test]
fn a_definition_edited_mid_flight_cannot_reach_a_run_already_in_flight() {
    let (mut registry, first) = defined(two_lane_spec("the release lane"));
    let run = started(&mut registry, &first.workflow_id, None);
    assert_eq!(registry.run(&run).expect("a run").definition_revision, 1);

    // The definition is edited: a THIRD lane, a different name.
    let mut edited = two_lane_spec("the release lane, revised");
    edited.nodes.push(checkpoint("audit"));
    edited.edges.push(edge("open", "audit", EdgeKind::Always));
    let second = registry
        .plan_define(&edited, Some(&first.workflow_id), NOW + 1)
        .expect("a new revision");
    registry.commit_define(&second);
    assert_eq!(second.revision, 2);
    assert_ne!(second.spec_digest, first.spec_digest);

    // The run in flight is untouched: same revision, same digest, same
    // nodes. It never learns the workflow was edited.
    let record = registry.run(&run).expect("a run");
    assert_eq!(record.definition_revision, 1);
    assert_eq!(record.spec_digest, first.spec_digest);
    assert_eq!(record.spec.name, "the release lane");
    assert!(record.node("audit").is_none(), "the edit reached the run");
    assert_eq!(record.nodes.len(), 3);

    // And a run started NOW pins the edit, because "latest" is resolved
    // once, at start.
    let after = started(&mut registry, &first.workflow_id, None);
    let record = registry.run(&after).expect("a run");
    assert_eq!(record.definition_revision, 2);
    assert!(record.node("audit").is_some());
}

#[test]
fn a_run_reports_the_revision_it_is_executing_and_a_named_one_is_honoured() {
    let (mut registry, first) = defined(two_lane_spec("v1"));
    let second = registry
        .plan_define(&two_lane_spec("v2"), Some(&first.workflow_id), NOW + 1)
        .expect("a new revision");
    registry.commit_define(&second);

    let pinned = started(&mut registry, &first.workflow_id, Some(1));
    let record = registry.run(&pinned).expect("a run");
    assert_eq!(record.definition_revision, 1);
    assert_eq!(record.spec.name, "v1");
    assert_eq!(record.spec_digest, first.spec_digest);

    // A revision that is not there is a typed refusal naming the latest,
    // never a silent fall back to one the caller did not ask for.
    let refused = registry
        .plan_start(&StartRequest {
            workflow_id: first.workflow_id.clone(),
            revision: Some(9),
            ..StartRequest::default()
        })
        .expect_err("no revision 9");
    assert_eq!(refused.code, ErrorCode::NotFound);
    assert!(refused.message.contains('9'), "{}", refused.message);
    assert!(refused.message.contains('2'), "{}", refused.message);
}

#[test]
fn a_revision_never_replaces_the_one_before_it() {
    let (mut registry, first) = defined(two_lane_spec("v1"));
    for (index, name) in ["v2", "v3"].into_iter().enumerate() {
        let next = registry
            .plan_define(&two_lane_spec(name), Some(&first.workflow_id), NOW)
            .expect("a new revision");
        assert_eq!(next.revision, index as u64 + 2);
        registry.commit_define(&next);
    }
    let record = registry.workflow(&first.workflow_id).expect("the workflow");
    assert_eq!(record.latest_revision, 3);
    assert_eq!(record.revisions.len(), 3);
    assert_eq!(record.revision(Some(1)).expect("rev 1").spec.name, "v1");
    assert_eq!(record.revision(None).expect("latest").spec.name, "v3");
}

// ---- the node-state law ----------------------------------------------

#[test]
fn an_illegal_node_move_is_refused_typed_and_carries_the_record_to_ledger() {
    let (mut registry, definition) = defined(two_lane_spec("lane"));
    let run = started(&mut registry, &definition.workflow_id, None);

    // `pending -> done`: a node that never started cannot claim it
    // finished. The refusal arrives WITH the record its provider must
    // append; there is no code path that produces one without the other.
    let Moved::Refused(refused, error) = registry
        .plan_node_move(
            &run,
            "run-it",
            NodeState::Done,
            Some("planner".into()),
            None,
            NOW,
        )
        .expect("planned")
    else {
        panic!("pending -> done is not in the table");
    };
    assert_eq!(error.code, ErrorCode::Refused);
    assert_eq!(error.extra["node"], "run-it");
    assert_eq!(error.extra["from"], "pending");
    assert_eq!(error.extra["to"], "done");
    assert!(
        error.message.contains("pending -> done"),
        "{}",
        error.message
    );
    assert_eq!(refused.node_id, "run-it");
    assert_eq!(
        (refused.from, refused.to),
        (NodeState::Pending, NodeState::Done)
    );

    // Nothing moved, and the attempt is a fact the ledger holds once its
    // line is durable.
    assert_eq!(
        registry
            .run(&run)
            .expect("a run")
            .node("run-it")
            .expect("a node")
            .state,
        NodeState::Pending
    );
    registry.commit_refusal(&run, &refused);
    let record = registry.run(&run).expect("a run");
    assert_eq!(record.refused.len(), 1);
    assert_eq!(
        record.node("run-it").expect("a node").state,
        NodeState::Pending
    );
}

#[test]
fn nothing_advances_until_the_record_is_durable() {
    let (mut registry, definition) = defined(two_lane_spec("lane"));
    let run = started(&mut registry, &definition.workflow_id, None);
    // A plan that is never committed — the shape of an append that was
    // refused — leaves the reported state exactly where it was.
    let planned = registry
        .plan_node_move(&run, "open", NodeState::Running, None, None, NOW)
        .expect("planned");
    assert!(matches!(planned, Moved::Changed(_)));
    let record = registry.run(&run).expect("a run");
    assert_eq!(
        record.node("open").expect("a node").state,
        NodeState::Pending
    );
    assert!(record.history.is_empty());
}

#[test]
fn a_node_outside_the_pinned_revision_cannot_be_moved() {
    let (mut registry, definition) = defined(two_lane_spec("lane"));
    let run = started(&mut registry, &definition.workflow_id, None);
    let refused = registry
        .plan_node_move(&run, "audit", NodeState::Running, None, None, NOW)
        .expect_err("not in the revision");
    assert_eq!(refused.code, ErrorCode::NotFound);
    assert!(
        refused.message.contains("revision 1"),
        "{}",
        refused.message
    );
}

// ---- the graph walk --------------------------------------------------

#[test]
fn a_node_no_edge_reached_is_skipped_and_a_skip_is_not_a_failure() {
    let (mut registry, definition) = defined(two_lane_spec("lane"));
    let run = started(&mut registry, &definition.workflow_id, None);

    assert_eq!(registry.ready_nodes(&run), vec!["open".to_owned()]);
    assert!(
        registry.skipped_nodes(&run).is_empty(),
        "nothing is decided yet"
    );

    move_node(&mut registry, &run, "open", NodeState::Running);
    // A node whose source has not ended is neither ready nor skipped.
    assert!(registry.ready_nodes(&run).is_empty());
    assert!(registry.skipped_nodes(&run).is_empty());

    move_node(&mut registry, &run, "open", NodeState::Done);
    assert_eq!(registry.ready_nodes(&run), vec!["run-it".to_owned()]);
    assert_eq!(registry.skipped_nodes(&run), vec!["bail".to_owned()]);

    move_node(&mut registry, &run, "bail", NodeState::Skipped);
    move_node(&mut registry, &run, "run-it", NodeState::Running);
    assert!(registry.run_would_end(&run).is_none(), "a node still moves");
    move_node(&mut registry, &run, "run-it", NodeState::Done);
    // A skipped node is the graph working, so the run is done.
    assert_eq!(
        registry.run_would_end(&run),
        Some((RunStatus::Done, None)),
        "a skipped node is not a failure"
    );
}

#[test]
fn a_run_is_done_only_when_every_node_that_ran_reached_done() {
    let (mut registry, definition) = defined(two_lane_spec("lane"));
    let run = started(&mut registry, &definition.workflow_id, None);
    move_node(&mut registry, &run, "open", NodeState::Running);
    move_node(&mut registry, &run, "open", NodeState::Failed);
    move_node(&mut registry, &run, "run-it", NodeState::Skipped);
    move_node(&mut registry, &run, "bail", NodeState::Running);
    move_node(&mut registry, &run, "bail", NodeState::Done);
    let (status, reason) = registry.run_would_end(&run).expect("an ending");
    assert_eq!(status, RunStatus::Failed);
    assert!(reason.expect("a reason").contains("\"open\""));
}

#[test]
fn an_ending_that_needs_a_reason_and_carries_none_is_refused() {
    let (mut registry, definition) = defined(two_lane_spec("lane"));
    let run = started(&mut registry, &definition.workflow_id, None);
    for status in [
        RunStatus::Failed,
        RunStatus::Cancelled,
        RunStatus::Interrupted,
    ] {
        for blank in [None, Some("   ".to_owned())] {
            let refused = registry
                .plan_run_end(&run, status, blank, NOW)
                .expect_err("no reason");
            assert_eq!(refused.code, ErrorCode::Invalid);
        }
    }
    assert!(registry
        .plan_run_end(&run, RunStatus::Done, None, NOW)
        .is_ok());
    assert!(registry
        .plan_run_end(&run, RunStatus::Running, None, NOW)
        .is_err());
}

#[test]
fn a_run_that_ended_does_not_end_twice_and_its_nodes_do_not_move_again() {
    let (mut registry, definition) = defined(two_lane_spec("lane"));
    let run = started(&mut registry, &definition.workflow_id, None);
    registry.commit_run_end(&run, RunStatus::Cancelled, Some("stopped".into()), NOW);
    let refused = registry
        .plan_run_end(&run, RunStatus::Done, None, NOW)
        .expect_err("already ended");
    assert_eq!(refused.code, ErrorCode::Refused);
    let refused = registry
        .plan_node_move(&run, "open", NodeState::Running, None, None, NOW)
        .expect_err("already ended");
    assert_eq!(refused.code, ErrorCode::Refused);
    assert!(registry.ready_nodes(&run).is_empty());
}

// ---- the journal, and the recovery -----------------------------------

/// The document one run's journal holds after `open` started and the
/// daemon died on it.
fn crashed_document(definition: &Definition) -> Vec<u8> {
    let mut document = journal::Record::run_started(
        &definition.workflow_id,
        definition.revision,
        &definition.spec_digest,
        &definition.spec,
        &Extensions::new(),
        None,
        NOW,
    )
    .line();
    let change = NodeChange {
        seq: 0,
        node_id: "open".to_owned(),
        from: NodeState::Pending,
        to: NodeState::Running,
        actor: None,
        note: None,
        at_ms: NOW + 1,
        extra: Extensions::new(),
    };
    document.extend(
        journal::Record::node_state_changed(&change, &NodeRun::default())
            .expect("a legal move")
            .line(),
    );
    document
}

#[test]
fn a_node_left_running_by_a_crash_comes_back_recorded_interrupted_with_a_reason() {
    let (_, definition) = defined(two_lane_spec("lane"));
    let document = crashed_document(&definition);

    // The replay says what the document SAYS — including `running`,
    // because inventing a line nobody wrote is not a reader's job.
    let replayed = journal::replay(&document).expect("a replay");
    assert_eq!(replayed.status, RunStatus::Running);
    assert_eq!(
        replayed
            .open_nodes()
            .iter()
            .map(|node| node.node_id.as_str())
            .collect::<Vec<_>>(),
        vec!["open"]
    );
    assert!(replayed.run_open());

    // A store adopts it and plans what it OWES before it may serve.
    let mut registry = Workflows::new("default");
    registry.commit_define(&definition);
    registry.adopt_run("default-r1", replayed);
    let recovery = registry.plan_recovery("default-r1", NOW + 500);
    assert_eq!(recovery.node_changes.len(), 1);
    let change = &recovery.node_changes[0];
    assert_eq!(change.node_id, "open");
    assert_eq!(
        (change.from, change.to),
        (NodeState::Running, NodeState::Interrupted)
    );
    assert_eq!(change.note.as_deref(), Some(INTERRUPTED_NODE_REASON));
    let (status, reason) = recovery.run_end.clone().expect("a run ending");
    assert_eq!(status, RunStatus::Interrupted);
    assert!(!reason.trim().is_empty(), "an ending with no reason");

    // Those records go on the log — appended AFTER the ones already
    // there, never an edit of one.
    let mut document = document;
    document.extend(
        journal::Record::node_state_changed(change, &NodeRun::default())
            .expect("running -> interrupted is legal")
            .line(),
    );
    document.extend(
        journal::Record::run_ended(status, Some(&reason), NOW + 500)
            .expect("an ending with a reason")
            .line(),
    );

    // And the NEXT boot reads a run that is interrupted, with a reason,
    // with its whole history intact — and nothing declared `running`.
    let again = journal::replay(&document).expect("a replay");
    assert_eq!(again.status, RunStatus::Interrupted);
    assert!(
        again.open_nodes().is_empty(),
        "still running after a recovery"
    );
    assert!(!again.run_open());
    let open = again
        .nodes
        .iter()
        .find(|node| node.node_id == "open")
        .expect("open");
    assert_eq!(open.state, NodeState::Interrupted);
    assert_eq!(open.reason.as_deref(), Some(INTERRUPTED_NODE_REASON));
    // Both readings are in the document: that the node started, and that
    // the daemon died on it.
    assert_eq!(again.history.len(), 2);
    assert_eq!(again.history[0].to, NodeState::Running);
    assert_eq!(again.history[1].to, NodeState::Interrupted);
}

#[test]
fn a_run_that_owes_nothing_gets_no_recovery_records() {
    let (mut registry, definition) = defined(two_lane_spec("lane"));
    let run = started(&mut registry, &definition.workflow_id, None);
    registry.commit_run_end(&run, RunStatus::Done, None, NOW);
    assert!(registry.plan_recovery(&run, NOW).is_empty());
}

#[test]
fn a_torn_tail_is_absence_and_a_hole_anywhere_earlier_is_refused() {
    let (_, definition) = defined(two_lane_spec("lane"));
    let whole = crashed_document(&definition);

    // A tail written short reads as ABSENCE: the run before it survives.
    let mut torn = whole.clone();
    torn.truncate(whole.len() - 10);
    let replayed = journal::replay(&torn).expect("a torn tail is absence");
    assert!(
        replayed.torn_tail_bytes > 0,
        "the discarded bytes are reported"
    );
    assert_eq!(replayed.workflow_id, definition.workflow_id);
    assert!(
        replayed.open_nodes().is_empty(),
        "the torn line was never a record"
    );

    // A hole in the MIDDLE is corruption, and is refused rather than
    // skipped: answering the two the same way would let real damage
    // masquerade as a clean stop.
    let first_newline = whole
        .iter()
        .position(|byte| *byte == b'\n')
        .expect("a line");
    let mut holed = whole[..=first_newline].to_vec();
    holed.extend(b"{not a record\n");
    holed.extend(&whole[first_newline + 1..]);
    let refused = journal::replay(&holed).expect_err("a hole");
    assert!(refused.contains("journal line 2"), "{refused}");
}

#[test]
fn an_append_onto_a_torn_tail_makes_a_hole_the_reader_refuses() {
    // `FINDINGS.md` #34's mechanism, at this layer: a tolerable torn TAIL
    // that is appended onto stops being a tail — it fuses with the new
    // record into one undecodable line in the MIDDLE of the document.
    // A store that tolerates a tear must HEAL the document rather than
    // append past it.
    let (_, definition) = defined(two_lane_spec("lane"));
    let whole = crashed_document(&definition);
    let mut torn = whole.clone();
    torn.truncate(whole.len() - 10);
    assert!(journal::replay(&torn).is_ok(), "a tail is absence");

    let ending =
        journal::Record::run_ended(RunStatus::Interrupted, Some("the daemon stopped"), NOW)
            .expect("an ending");
    let mut fused = torn.clone();
    fused.extend(ending.line());
    let refused = journal::replay(&fused).expect_err("the tear fused into a hole");
    println!("append-onto-a-tear replays as: {refused}");

    // The same append onto a HEALED document is fine.
    let healed_len = torn
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |last| last + 1);
    let mut healed = torn[..healed_len].to_vec();
    healed.extend(ending.line());
    assert_eq!(
        journal::replay(&healed)
            .expect("a healed document replays")
            .status,
        RunStatus::Interrupted
    );
}

#[test]
fn a_journal_line_the_seam_could_not_have_written_is_refused() {
    let (_, definition) = defined(two_lane_spec("lane"));
    let base = journal::Record::run_started(
        &definition.workflow_id,
        definition.revision,
        &definition.spec_digest,
        &definition.spec,
        &Extensions::new(),
        None,
        NOW,
    )
    .line();

    // A move from a state the node was never in.
    let mut wrong_from = base.clone();
    wrong_from.extend(
        serde_json::to_vec(&serde_json::json!({
            "api-version": API_VERSION, "kind": "node-state-changed", "at-ms": NOW,
            "node-id": "open", "from": "running", "to": "done"
        }))
        .expect("encodes"),
    );
    wrong_from.push(b'\n');
    let refused = journal::replay(&wrong_from).expect_err("a move from nowhere");
    assert!(refused.contains("stood at pending"), "{refused}");

    // A move the TABLE does not admit.
    let mut illegal = base.clone();
    illegal.extend(
        serde_json::to_vec(&serde_json::json!({
            "api-version": API_VERSION, "kind": "node-state-changed", "at-ms": NOW,
            "node-id": "open", "from": "pending", "to": "done"
        }))
        .expect("encodes"),
    );
    illegal.push(b'\n');
    let refused = journal::replay(&illegal).expect_err("an illegal move");
    assert!(refused.contains("pending -> done"), "{refused}");

    // A node the PINNED revision does not contain.
    let mut stranger = base.clone();
    stranger.extend(
        serde_json::to_vec(&serde_json::json!({
            "api-version": API_VERSION, "kind": "node-state-changed", "at-ms": NOW,
            "node-id": "audit", "from": "pending", "to": "running"
        }))
        .expect("encodes"),
    );
    stranger.push(b'\n');
    let refused = journal::replay(&stranger).expect_err("a stranger node");
    assert!(refused.contains("pinned"), "{refused}");

    // A `run-ended` that is not an ending.
    let mut alive = base.clone();
    alive.extend(
        serde_json::to_vec(&serde_json::json!({
            "api-version": API_VERSION, "kind": "run-ended", "at-ms": NOW, "status": "running"
        }))
        .expect("encodes"),
    );
    alive.push(b'\n');
    let refused = journal::replay(&alive).expect_err("running is not an ending");
    assert!(refused.contains("cannot be running"), "{refused}");

    // A document that does not open with the run it records.
    let refused = journal::replay(&illegal[base.len()..]).expect_err("no run-started");
    assert!(refused.contains("opens with"), "{refused}");
}

#[test]
fn a_writer_cannot_record_an_ending_that_is_not_one() {
    assert!(journal::Record::run_ended(RunStatus::Running, None, NOW).is_err());
    assert!(journal::Record::run_ended(RunStatus::Failed, None, NOW).is_err());
    assert!(journal::Record::run_ended(RunStatus::Failed, Some("  "), NOW).is_err());
    assert!(journal::Record::run_ended(RunStatus::Done, None, NOW).is_ok());
    let illegal = NodeChange {
        seq: 0,
        node_id: "open".to_owned(),
        from: NodeState::Pending,
        to: NodeState::Done,
        actor: None,
        note: None,
        at_ms: NOW,
        extra: Extensions::new(),
    };
    assert!(journal::Record::node_state_changed(&illegal, &NodeRun::default()).is_err());
}

#[test]
fn a_workflow_document_whose_revisions_skip_or_disagree_is_refused() {
    let (_, first) = defined(two_lane_spec("v1"));
    let mut document = journal::Record::defined(&first).line();
    assert_eq!(
        journal::replay_workflow(&document)
            .expect("one revision")
            .0
            .len(),
        1
    );

    // Revision 3 where 2 was due: a document this seam did not write.
    let skipped = Definition::new(&first.workflow_id, 3, two_lane_spec("v3"), NOW);
    let mut skipping = document.clone();
    skipping.extend(journal::Record::defined(&skipped).line());
    let refused = journal::replay_workflow(&skipping).expect_err("a skip");
    assert!(refused.contains("consecutive"), "{refused}");

    // A revision whose digest does not match the spec it carries.
    let mut forged = journal::Record::defined(&Definition::new(
        &first.workflow_id,
        2,
        two_lane_spec("v2"),
        NOW,
    ));
    forged.spec_digest = Some("fnv1a64:0000000000000000".to_owned());
    document.extend(forged.line());
    let refused = journal::replay_workflow(&document).expect_err("a digest that disagrees");
    assert!(refused.contains("digest"), "{refused}");
}

// ---- the feed --------------------------------------------------------

#[test]
fn the_event_ring_reports_what_it_dropped_rather_than_leaving_a_quiet_gap() {
    let (mut registry, definition) = defined(two_lane_spec("lane"));
    let run = started(&mut registry, &definition.workflow_id, None);
    for _ in 0..EVENT_RING + 5 {
        registry.record_event(
            &run,
            EventKind::NodeStarted {
                node_id: "open".to_owned(),
            },
        );
    }
    let page = registry.events_since(&run, None, None).expect("a page");
    assert_eq!(page.events.len(), EVENT_RING);
    assert_eq!(page.dropped, 5);
    assert_eq!(page.run_id, run);
    // A page after a sequence carries only what came later.
    let after = page.events[10].seq;
    let page = registry
        .events_since(&run, Some(after), Some(3))
        .expect("a page");
    assert_eq!(page.events.len(), 3);
    assert!(page.events.iter().all(|event| event.seq > after));
}

#[test]
fn a_move_emits_exactly_what_it_means() {
    let start = NodeChange {
        seq: 0,
        node_id: "open".to_owned(),
        from: NodeState::Pending,
        to: NodeState::Running,
        actor: None,
        note: None,
        at_ms: NOW,
        extra: Extensions::new(),
    };
    assert_eq!(
        Workflows::move_events(&start),
        vec![EventKind::NodeStarted {
            node_id: "open".to_owned()
        }]
    );
    let end = NodeChange {
        from: NodeState::Running,
        to: NodeState::Failed,
        note: Some("the engine refused".to_owned()),
        ..start
    };
    assert_eq!(
        Workflows::move_events(&end),
        vec![EventKind::NodeEnded {
            node_id: "open".to_owned(),
            outcome: NodeState::Failed,
            reason: Some("the engine refused".to_owned()),
        }]
    );
}

// ---- the contract name -----------------------------------------------

#[test]
fn a_store_is_reached_by_its_own_contract_name_and_nothing_else() {
    assert_eq!(store_contract("default"), "jinn:workflow.default");
    assert_eq!(store_id_of("jinn:workflow.memory"), Some("memory"));
    assert_eq!(store_id_of("jinn:workflow."), None);
    assert_eq!(store_id_of("jinn:todo.default"), None);
    let slots = stores_in([
        (
            "jinn-workflow-default",
            vec!["jinn:workflow.default", "jinn:clock"],
        ),
        ("jinn-workflow-memory", vec!["jinn:workflow.memory"]),
        ("jinn-todo-default", vec!["jinn:todo.default"]),
    ]);
    assert_eq!(slots.len(), 2);
    assert_eq!(slots[0].store, "default");
    assert_eq!(slots[0].entry, "jinn-workflow-default");
    assert_eq!(slots[1].store, "memory");
}

#[test]
fn an_input_the_pinned_revision_does_not_declare_is_refused_at_start() {
    let mut spec = two_lane_spec("lane");
    spec.input = InputSchema {
        fields: vec![FieldSpec {
            name: "ticket".to_owned(),
            kind: FieldKind::String,
            required: true,
            ..FieldSpec::default()
        }],
        ..InputSchema::default()
    };
    let (mut registry, definition) = defined(spec);
    let refused = registry
        .plan_start(&StartRequest {
            workflow_id: definition.workflow_id.clone(),
            ..StartRequest::default()
        })
        .expect_err("a required field is absent");
    assert_eq!(refused.code, ErrorCode::Invalid);

    let mut input = Extensions::new();
    input.insert("ticket".to_owned(), serde_json::json!("PLA-1"));
    let plan = registry
        .plan_start(&StartRequest {
            workflow_id: definition.workflow_id.clone(),
            input: input.clone(),
            ..StartRequest::default()
        })
        .expect("a run opens");
    registry.commit_start(&plan, NOW);
    assert_eq!(registry.run(&plan.run_id).expect("a run").input, input);
}

#[test]
fn a_document_whose_only_run_started_is_torn_holds_no_run_at_all() {
    // The daemon was killed INSIDE the very first append. What is on
    // disk is one byte that was never a record — and a run is a positive
    // reading, so this document holds no run to report.
    let (_, definition) = defined(two_lane_spec("lane"));
    let whole = crashed_document(&definition);
    let torn = whole[..1].to_vec();

    let document = journal::replay(&torn).expect("a torn first line is absence, not damage");
    assert_eq!(
        document,
        journal::RunDocument::Absent {
            torn_tail_bytes: 1
        },
        "one byte of noise is absence and nothing else"
    );
    assert!(
        document.run().is_none(),
        "no complete `run-started` was read, so there is no run"
    );

    // An empty document is the same absence, with nothing to discard.
    assert_eq!(
        journal::replay(&[]).expect("an empty document is absence"),
        journal::RunDocument::Absent {
            torn_tail_bytes: 0
        }
    );
}

#[test]
fn a_run_no_node_ever_ran_cannot_be_ended_done() {
    // `done` is the claim that the procedure was CARRIED OUT. Over an
    // empty set of nodes that claim is vacuously true and factually
    // unfounded, which is exactly the shape a fabricated run takes. A
    // spec with no nodes is refused at `define` (`spec.rs`), so this is
    // the second lock on a door that should already be shut.
    assert_eq!(
        run_ending(&[]),
        None,
        "nothing recorded proves an ending"
    );
}
