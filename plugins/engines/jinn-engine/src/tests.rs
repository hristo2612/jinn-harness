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
            ..Usage::default()
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
    assert_eq!(
        ahead.event,
        Event::Unknown {
            kind: "thinking".to_owned(),
            fields: [("tokens".to_owned(), serde_json::json!(4))]
                .into_iter()
                .collect(),
        }
    );
    assert_eq!(ahead.seq, 9);
}

/// The additivity law, at EVERY nesting level: a field a newer peer sends
/// survives a round trip whether it sits on the envelope, on a nested
/// object, or on an event kind this version has never heard of. A schema
/// that preserves only at the top level fails the law — a `budget` or a
/// `usage` a newer provider extended would be silently truncated by the
/// hop through this version.
#[test]
fn every_nested_schema_preserves_a_future_peers_fields() {
    // One future-shaped request: an unknown field on the envelope AND on
    // every nested object it carries.
    let wire = serde_json::json!({
        "api-version": "0.9",
        "engine": "default",
        "prompt": "say ok",
        "tools": { "mode": "allowlist", "allow": ["Read"], "deny-network": true },
        "budget": { "wall-ms": 5000, "output-bytes": 4096, "max-turns": 3 },
        "secrets": { "KEY": { "$secret": "engines/key" } },
        "beyond-this-version": { "nested": [1, 2] }
    });
    let request: RunRequest = serde_json::from_value(wire.clone()).expect("decodes");
    // Decoded, not swallowed: the known fields still mean what they mean.
    assert_eq!(request.tools.admitted(), ["Read"]);
    assert_eq!(request.budget.output_bytes, 4_096);
    // And byte-preserved: the round trip is the ORIGINAL document.
    assert_eq!(serde_json::to_value(&request).expect("encodes"), wire);

    // The same law on the answer side: `capabilities` is nested inside a
    // `describe`, and a capability this version cannot name is still a
    // fact the next hop must not drop.
    let described = serde_json::json!({
        "api-version": "0.9",
        "engine": "default",
        "provider": "engines/example",
        "models": ["m-1"],
        "default-model": "m-1",
        "capabilities": { "streaming": true, "tool-calls": false, "cancel": true,
                          "usage": true, "external-cli": false, "vision": true },
        "beyond-this-version": 1
    });
    let description: Description = serde_json::from_value(described.clone()).expect("decodes");
    assert!(description.capabilities.streaming);
    assert_eq!(
        serde_json::to_value(&description).expect("encodes"),
        described
    );

    // And inside an EVENT: `usage` is two levels down from the bus record.
    let exited = serde_json::json!({
        "api-version": "0.9", "engine": "default", "run-id": "default-1", "seq": 4,
        "kind": "exited", "status": 0, "truncated": false,
        "usage": { "input-tokens": 10, "output-tokens": 3, "cost-micro-usd": 19_304,
                   "cache-read-tokens": 7 }
    });
    let event: RunEvent = serde_json::from_value(exited.clone()).expect("decodes");
    assert_eq!(serde_json::to_value(&event).expect("encodes"), exited);

    // A run record round-trips whole, nested extensions and all.
    let record = serde_json::json!({
        "api-version": "0.9", "run-id": "default-1", "engine": "default",
        "state": "exited", "events": [], "text": "OK", "truncated": false,
        "usage": { "input-tokens": 1, "output-tokens": 1, "cost-micro-usd": 0,
                   "cache-read-tokens": 2 },
        "beyond-this-version": true
    });
    let decoded: RunRecord = serde_json::from_value(record.clone()).expect("decodes");
    assert_eq!(serde_json::to_value(&decoded).expect("encodes"), record);
}

/// An event kind a newer provider invented reaches a listener WHOLE: the
/// kind it was sent under and every field it carried. Keeping the kind is
/// what lets an operator see what a newer peer is doing; dropping it made
/// every future event indistinguishable from every other.
#[test]
fn an_unknown_event_kind_keeps_its_name_and_its_payload() {
    let wire = serde_json::json!({
        "api-version": "0.9", "engine": "default", "run-id": "default-1", "seq": 2,
        "kind": "thinking", "text": "hmm", "tokens": 4
    });
    let event: RunEvent = serde_json::from_value(wire.clone()).expect("decodes");
    let Event::Unknown { kind, fields } = &event.event else {
        panic!("an unheard-of kind is Unknown, not a decode failure: {event:?}");
    };
    assert_eq!(kind, "thinking");
    assert_eq!(fields["tokens"], 4);
    assert_eq!(fields["text"], "hmm");
    // Byte-preserving: what a listener forwards is what it was sent.
    assert_eq!(serde_json::to_value(&event).expect("encodes"), wire);
    // It is still ORDERED and COUNTED like any other event — the whole
    // reason it is a fact rather than an error.
    assert_eq!(event.seq, 2);
    let mut runs = Runs::new("default");
    let accepted = runs.accept(&request("default"), 0);
    let recorded = runs
        .record(&accepted.run_id, event.event.clone())
        .expect("held");
    assert_eq!(recorded.seq, 0);
    // An unknown kind never moves the run's state: this version cannot
    // know what it meant, so it guesses nothing.
    assert_eq!(
        runs.get(&accepted.run_id).expect("held").state,
        RunState::Starting
    );
}

/// The output budget is a fact a CONSUMER must see, and a consumer sees
/// events. Setting `RunRecord.truncated` alone is invisible on the bus,
/// so the cut is its own typed event, ordered with the rest of the run.
#[test]
fn spending_the_output_budget_is_a_typed_event_on_the_wire() {
    let mut runs = Runs::new("default");
    let accepted = runs.accept(
        &RunRequest {
            budget: Budget {
                wall_ms: 60_000,
                output_bytes: 8,
                extra: Extensions::new(),
            },
            ..request("default")
        },
        0,
    );
    let run_id = accepted.run_id.clone();
    // Within budget: nothing to say.
    assert_eq!(runs.read(&run_id, 4), None);
    assert!(!runs.get(&run_id).expect("held").truncated);
    // Past it: the event the provider records and emits before it kills
    // the child.
    let cut = runs.read(&run_id, 12).expect("the budget is spent");
    assert_eq!(
        cut,
        Event::Truncated {
            limit_bytes: 8,
            read_bytes: 16
        }
    );
    let emitted = runs.record(&run_id, cut).expect("held");
    assert_eq!(
        serde_json::to_value(&emitted).expect("encodes")["kind"],
        "truncated"
    );
    // Recording it is what marks the record — one source for the fact.
    assert!(runs.get(&run_id).expect("held").truncated);
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
                ..Usage::default()
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
        ..Budget::default()
    };
    let run = runs.accept(&request, 1_000).run_id;

    assert!(!runs.over_wall_budget(&run, 1_400));
    assert!(runs.over_wall_budget(&run, 1_600));
    assert!(!runs.over_wall_budget("echo-404", u64::MAX));

    assert_eq!(runs.read(&run, 8), None);
    assert!(!runs.get(&run).expect("held").truncated);
    let cut = runs.read(&run, 1).expect("the budget is spent");
    runs.record(&run, cut).expect("held");
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
