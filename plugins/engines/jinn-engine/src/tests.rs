//! The definition's own tests: the contract-name algebra, the wire's
//! additivity, and the run registry every provider shares. Nothing here
//! touches a host — a provider's CLI behaviour is proven in the
//! composition suite against the real daemon.

use super::*;

fn request(engine: &str) -> RunRequest {
    RunRequest {
        api_version: API_VERSION.to_owned(),
        engine: engine.to_owned(),
        prompt: "say ok".to_owned(),
        ..RunRequest::default()
    }
}

#[test]
fn a_contract_name_carries_exactly_the_engine_id() {
    assert_eq!(engine_contract("codex"), "jinn:engine.codex");
    assert_eq!(engine_id_of("jinn:engine.codex"), Some("codex"));
    assert_eq!(engine_id_of(&engine_contract("default")), Some("default"));
    // Not this seam's, and never a nameless engine.
    assert_eq!(engine_id_of("jinn:engine"), None);
    assert_eq!(engine_id_of("jinn:engine."), None);
    assert_eq!(engine_id_of("jinn:settings"), None);
    // A dotted engine id survives the round trip whole — the prefix is
    // stripped once, not split on every dot.
    assert_eq!(engine_id_of("jinn:engine.claude.fast"), Some("claude.fast"));
}

#[test]
fn engines_come_from_the_kernels_own_view_sorted_and_attributed() {
    let slots = engines_in([
        ("jinn-engine-codex", vec!["jinn:engine.codex"]),
        ("jinn-settings-profile", vec!["jinn:settings"]),
        (
            "jinn-engine-default",
            vec!["jinn:engine.default", "jinn:engine.echo"],
        ),
    ]);
    assert_eq!(
        slots
            .iter()
            .map(|slot| (slot.engine.as_str(), slot.entry.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("codex", "jinn-engine-codex"),
            ("default", "jinn-engine-default"),
            ("echo", "jinn-engine-default"),
        ]
    );
    assert_eq!(slots[0].contract, "jinn:engine.codex");
    // A composition with no engine mounted lists none — never a guess.
    assert!(engines_in([("jinn-status", vec!["jinn:api-status"])]).is_empty());
}

#[test]
fn a_request_carries_key_names_and_never_secret_material() {
    let wire = serde_json::json!({
        "api-version": "0.1",
        "engine": "default",
        "prompt": "say ok",
        "tools": { "mode": "allowlist", "allow": ["Read"] },
        "budget": { "wall-ms": 5000, "output-bytes": 4096 },
        "secrets": { "ANTHROPIC_API_KEY": { "$secret": "engines/anthropic" } },
        "unknown-to-this-version": true
    });
    let request: RunRequest = serde_json::from_value(wire).expect("decodes");
    assert_eq!(request.tools.admitted(), ["Read"]);
    assert_eq!(request.budget.wall_ms, 5_000);
    assert_eq!(
        request.secrets["ANTHROPIC_API_KEY"].secret,
        "engines/anthropic"
    );
    // Additive: a field this version does not know survives a round trip.
    assert_eq!(request.extra["unknown-to-this-version"], true);
    let round = serde_json::to_value(&request).expect("encodes");
    assert_eq!(round["unknown-to-this-version"], true);
    assert!(is_secret_ref(&round["secrets"]["ANTHROPIC_API_KEY"]));
    // The default policy is DENY: an absent `tools` admits nothing.
    assert!(RunRequest::default().tools.admitted().is_empty());
}

#[test]
fn events_are_tagged_by_kind_on_the_wire() {
    let event = RunEvent {
        api_version: API_VERSION.to_owned(),
        engine: "echo".to_owned(),
        run_id: "echo-1".to_owned(),
        seq: 3,
        event: Event::Delta {
            text: "OK".to_owned(),
        },
    };
    let wire = serde_json::to_value(&event).expect("encodes");
    assert_eq!(wire["kind"], "delta");
    assert_eq!(wire["text"], "OK");
    assert_eq!(wire["run-id"], "echo-1");
    assert_eq!(
        serde_json::from_value::<RunEvent>(wire).expect("decodes"),
        event
    );
    let exited = serde_json::to_value(Event::Exited {
        status: 0,
        usage: Usage {
            input_tokens: 10,
            output_tokens: 3,
            cost_micro_usd: 19_304,
        },
        truncated: false,
        error: None,
    })
    .expect("encodes");
    assert_eq!(exited["kind"], "exited");
    assert_eq!(exited["usage"]["input-tokens"], 10);
    // A kind a newer provider invented is a fact, not a decode failure:
    // a listener still orders and counts it (R12 on the bus).
    let ahead: RunEvent = serde_json::from_value(serde_json::json!({
        "api-version": "0.2", "engine": "echo", "run-id": "echo-1", "seq": 9,
        "kind": "thinking", "tokens": 4
    }))
    .expect("an unknown kind decodes");
    assert_eq!(ahead.event, Event::Unknown);
    assert_eq!(ahead.seq, 9);
}

#[test]
fn a_run_is_minted_sequenced_and_assembled() {
    let mut runs = Runs::new("echo");
    let accepted = runs.accept(&request("echo"), 1_000);
    assert_eq!(accepted.run_id, "echo-1");
    assert_eq!(
        runs.get(&accepted.run_id).expect("held").state,
        RunState::Starting
    );
    assert_eq!(runs.live_ids(), ["echo-1"]);

    let started = runs
        .record(
            &accepted.run_id,
            Event::Started {
                model: Some("echo-1".to_owned()),
            },
        )
        .expect("held");
    assert_eq!(started.seq, 0);
    assert_eq!(
        runs.get(&accepted.run_id).expect("held").state,
        RunState::Running
    );

    for (index, chunk) in ["O", "K"].into_iter().enumerate() {
        let emitted = runs
            .record(
                &accepted.run_id,
                Event::Delta {
                    text: chunk.to_owned(),
                },
            )
            .expect("held");
        assert_eq!(emitted.seq as usize, index + 1);
    }
    runs.record(
        &accepted.run_id,
        Event::Exited {
            status: 0,
            usage: Usage {
                input_tokens: 7,
                output_tokens: 1,
                cost_micro_usd: 0,
            },
            truncated: false,
            error: None,
        },
    )
    .expect("held");

    let record = runs.get(&accepted.run_id).expect("held");
    assert_eq!(record.text, "OK");
    assert_eq!(record.state, RunState::Exited);
    assert_eq!(record.status, Some(0));
    assert_eq!(record.usage.input_tokens, 7);
    assert_eq!(record.events.len(), 4);
    // A finished run is no longer polled.
    assert!(runs.live_ids().is_empty());
    // A provider never emits for a run it does not hold.
    assert!(runs
        .record("echo-99", Event::TurnEnd { text: None })
        .is_none());
}

#[test]
fn a_turn_end_only_fills_an_answer_the_deltas_never_gave() {
    // Codex reports one completed message rather than token deltas; claude
    // gives deltas AND a final result. Both must land the same answer.
    let mut streamed = Runs::new("claude");
    let run = streamed.accept(&request("claude"), 0).run_id;
    streamed.record(&run, Event::Delta { text: "O".into() });
    streamed.record(&run, Event::Delta { text: "K".into() });
    streamed.record(
        &run,
        Event::TurnEnd {
            text: Some("OK".into()),
        },
    );
    assert_eq!(streamed.get(&run).expect("held").text, "OK");

    let mut whole = Runs::new("codex");
    let run = whole.accept(&request("codex"), 0).run_id;
    whole.record(
        &run,
        Event::TurnEnd {
            text: Some("OK".into()),
        },
    );
    assert_eq!(whole.get(&run).expect("held").text, "OK");
}

#[test]
fn both_budgets_bound_a_run() {
    let mut runs = Runs::new("echo");
    let mut request = request("echo");
    request.budget = Budget {
        wall_ms: 500,
        output_bytes: 8,
    };
    let run = runs.accept(&request, 1_000).run_id;

    assert!(!runs.over_wall_budget(&run, 1_400));
    assert!(runs.over_wall_budget(&run, 1_600));
    assert!(!runs.over_wall_budget("echo-404", u64::MAX));

    assert!(!runs.read(&run, 8));
    assert!(!runs.get(&run).expect("held").truncated);
    assert!(runs.read(&run, 1));
    assert!(runs.get(&run).expect("held").truncated);
}

#[test]
fn a_failed_run_is_terminal_and_says_why() {
    let mut runs = Runs::new("claude");
    let run = runs.accept(&request("claude"), 0).run_id;
    let emitted = runs
        .fail(&run, "the claude CLI is not on this host")
        .expect("held");
    assert_eq!(
        emitted.event,
        Event::Cancelled {
            reason: "the claude CLI is not on this host".to_owned()
        }
    );
    let record = runs.get(&run).expect("held");
    assert_eq!(record.state, RunState::Failed);
    assert!(record.state.is_terminal());
    assert!(runs.live_ids().is_empty());
}

#[test]
fn finished_records_are_bounded_and_live_ones_are_never_dropped() {
    let mut runs = Runs::new("echo");
    for index in 0..5 {
        let run = runs.accept(&request("echo"), index).run_id;
        runs.record(
            &run,
            Event::Exited {
                status: 0,
                usage: Usage::default(),
                truncated: false,
                error: None,
            },
        );
    }
    let live = runs.accept(&request("echo"), 9).run_id;
    runs.record(&live, Event::Started { model: None });

    runs.retain_recent(2);
    assert_eq!(runs.len(), 3, "two finished records plus the live one");
    assert!(runs.get(&live).is_some(), "a live run is never dropped");
    assert!(
        runs.get("echo-1").is_none(),
        "the oldest finished went first"
    );
    assert!(runs.get("echo-5").is_some(), "the newest finished stayed");
}

#[test]
fn an_answer_is_typed_either_way() {
    let ok = Answer::ok(Description {
        api_version: API_VERSION.to_owned(),
        engine: "echo".to_owned(),
        provider: "engines/jinn-engine-echo".to_owned(),
        models: vec!["echo-1".to_owned()],
        default_model: Some("echo-1".to_owned()),
        capabilities: Capabilities {
            streaming: true,
            cancel: true,
            ..Capabilities::default()
        },
        extra: Extensions::new(),
    });
    let decoded: Answer = serde_json::from_slice(&ok.encode()).expect("decodes");
    let value = decoded.into_result().expect("ok");
    assert_eq!(value["engine"], "echo");
    assert_eq!(value["capabilities"]["external-cli"], false);

    let gated = Answer::error(EngineError::unavailable("no CLI on this host"));
    let error = gated.into_result().expect_err("an error");
    assert_eq!(error.code, ErrorCode::Unavailable);
}
