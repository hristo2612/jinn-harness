use super::*;

fn ready() -> AppServer {
    let mut wire = AppServer::new(
        "hello".into(),
        "gpt-6-astra".into(),
        "high".into(),
        "/text-chat".into(),
    );
    wire.thread = Some("thread-test".into());
    wire.turn = Some("turn-test".into());
    wire.expected = 0;
    let _ = wire.take_outgoing();
    wire
}

fn notification(method: &str, params: Value) -> Vec<u8> {
    format!("{}\n", json!({"method":method,"params":params})).into_bytes()
}

fn partial(text: &str) -> Vec<u8> {
    notification(
        "item/agentMessage/delta",
        json!({"threadId":"thread-test","turnId":"turn-test","itemId":"message-test","delta":text}),
    )
}

fn message() -> Vec<u8> {
    notification(
        "item/completed",
        json!({"threadId":"thread-test","turnId":"turn-test","item":{"type":"agentMessage","id":"message-test","text":"Hello 🌱"}}),
    )
}

fn finish() -> Vec<u8> {
    notification(
        "turn/completed",
        json!({"threadId":"thread-test","turn":{"id":"turn-test","status":"completed"}}),
    )
}

#[test]
fn app_server_partial_output_reaches_the_engine_before_completion() {
    let mut wire = ready();
    assert_eq!(wire.feed(&partial("Hello")), vec![Event::delta("Hello")]);
    assert!(!wire.terminal());
}

#[test]
fn every_fragmentation_and_duplicate_final_preserves_the_actual_deltas() {
    let bytes = [
        partial("Hello "),
        partial("🌱"),
        message(),
        message(),
        finish(),
    ]
    .concat();
    for split in 0..=bytes.len() {
        let mut wire = ready();
        let mut events = wire.feed(&bytes[..split]);
        events.extend(wire.feed(&bytes[split..]));
        assert_eq!(
            events,
            vec![
                Event::delta("Hello "),
                Event::delta("🌱"),
                Event::turn_end(None)
            ],
            "split {split}"
        );
        assert!(wire.terminal());
        assert!(wire.errors().is_empty(), "{:?}", wire.errors());
    }
}

#[test]
fn repeated_text_deltas_are_not_mistaken_for_duplicate_events() {
    let mut wire = ready();
    assert_eq!(
        wire.feed(&[partial("ha"), partial("ha")].concat()),
        vec![Event::delta("ha"), Event::delta("ha")]
    );
    assert_eq!(wire.messages["message-test"], "haha");
}

#[test]
fn exit_without_terminal_and_changed_final_are_failures() {
    let mut wire = ready();
    wire.feed(&partial("Hello"));
    wire.flush();
    assert!(wire.errors()[0].contains("terminal"));
    let mut wire = ready();
    wire.feed(&partial("Hello"));
    wire.feed(&message());
    assert!(wire.errors()[0].contains("differs"));
}

#[test]
fn malformed_oversized_and_unexpected_server_requests_fail_closed() {
    for frame in [
        b"not json\n".to_vec(),
        vec![b'x'; MAX_LINE + 1],
        b"{\"id\":99,\"method\":\"item/commandExecution/requestApproval\",\"params\":{}}\n"
            .to_vec(),
    ] {
        let mut wire = ready();
        assert!(wire.feed(&frame).is_empty());
        assert!(wire.terminal());
        assert!(!wire.errors().is_empty());
    }
}

#[test]
fn an_attempted_tool_is_not_a_completed_text_answer() {
    let mut wire = ready();
    wire.feed(&notification("rawResponseItem/completed", json!({"threadId":"thread-test","turnId":"turn-test","item":{"type":"custom_tool_call","name":"exec"}})));
    assert_eq!(wire.errors(), &["Tools are unavailable in text Chat"]);
}

#[test]
fn effective_config_and_thread_confirmation_gate_the_prompt() {
    let mut wire = ready();
    wire.expected = 2;
    wire.response(&json!({"config":{"mcp_servers":{"inherited":{}}}}));
    assert!(wire.errors()[0].contains("isolated"));
    assert!(wire.take_outgoing().is_empty());
    let mut wire = ready();
    wire.expected = 4;
    wire.response(&json!({"model":"other","reasoningEffort":"high","approvalPolicy":"never","instructionSources":[],"thread":{"id":"thread-test"}}));
    assert!(wire.errors()[0].contains("model"));
    assert!(wire.take_outgoing().is_empty());
}
