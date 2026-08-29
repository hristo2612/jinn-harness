//! The codec's tests run against the LINE SHAPES a live
//! `codex exec --json --sandbox read-only --skip-git-repo-check
//! --ephemeral -` run emits (see the crate doc for which are captured and
//! which are inferred). Thread ids are session identifiers: every fixture
//! here carries a synthetic one.

use jinn_engine::{Event, EventKind, ToolMode, ToolPolicy, Usage};

use crate::{argv, Decoder};

/// One captured run, verbatim but for the synthetic thread id: a
/// non-fatal `error` item BEFORE the turn, a `command_execution` that
/// starts and completes, the agent's message, and the usage.
const CAPTURED: &str = concat!(
    r#"{"type":"thread.started","thread_id":"thread-under-test"}"#,
    "\n",
    r#"{"type":"item.completed","item":{"id":"item_0","type":"error","message":"web_search_request is deprecated"}}"#,
    "\n",
    r#"{"type":"turn.started"}"#,
    "\n",
    r#"{"type":"item.started","item":{"id":"item_1","type":"command_execution","command":"/bin/zsh -lc 'echo hello'","aggregated_output":"","exit_code":null,"status":"in_progress"}}"#,
    "\n",
    r#"{"type":"item.completed","item":{"id":"item_1","type":"command_execution","command":"/bin/zsh -lc 'echo hello'","aggregated_output":"hello\n","exit_code":0,"status":"completed"}}"#,
    "\n",
    r#"{"type":"item.completed","item":{"id":"item_2","type":"agent_message","text":"OK"}}"#,
    "\n",
    r#"{"type":"turn.completed","usage":{"input_tokens":27315,"cached_input_tokens":25344,"cache_write_input_tokens":0,"output_tokens":38,"reasoning_output_tokens":0}}"#,
    "\n",
);

fn decode_all(stream: &str) -> (Vec<Event>, Decoder) {
    let mut decoder = Decoder::new();
    let mut events = decoder.feed(stream.as_bytes());
    events.extend(decoder.flush());
    (events, decoder)
}

#[test]
fn a_captured_run_decodes_to_the_seams_events() {
    let (events, _) = decode_all(CAPTURED);
    assert_eq!(
        events,
        vec![
            Event::started(None),
            Event::tool_result("error".to_owned(), false),
            Event::tool_call(
                "command_execution".to_owned(),
                serde_json::json!({ "command": "/bin/zsh -lc 'echo hello'" })
            ),
            Event::tool_result("command_execution".to_owned(), true),
            Event::turn_end(Some("OK".to_owned())),
        ]
    );
}

#[test]
fn the_thread_id_never_reaches_an_event() {
    let (events, _) = decode_all(CAPTURED);
    let encoded = serde_json::to_string(&events).expect("events encode");
    assert!(!encoded.contains("thread-under-test"), "{encoded}");
}

#[test]
fn the_codec_never_emits_an_exit_the_process_owns_it() {
    let (events, _) = decode_all(CAPTURED);
    assert!(!events
        .iter()
        .any(|event| matches!(event.kind, EventKind::Exited { .. })));
}

#[test]
fn a_stream_split_at_any_byte_boundary_decodes_the_same() {
    let (whole, _) = decode_all(CAPTURED);
    let bytes = CAPTURED.as_bytes();
    for split in 0..bytes.len() {
        let mut decoder = Decoder::new();
        let mut events = decoder.feed(&bytes[..split]);
        events.extend(decoder.feed(&bytes[split..]));
        events.extend(decoder.flush());
        assert_eq!(events, whole, "split at {split}");
        assert_eq!(decoder.usage().output_tokens, 38, "split at {split}");
    }
}

#[test]
fn a_stream_fed_one_byte_at_a_time_decodes_the_same() {
    let (whole, _) = decode_all(CAPTURED);
    let mut decoder = Decoder::new();
    let mut events = Vec::new();
    for byte in CAPTURED.as_bytes() {
        events.extend(decoder.feed(&[*byte]));
    }
    events.extend(decoder.flush());
    assert_eq!(events, whole);
}

#[test]
fn an_incomplete_tail_yields_nothing_until_its_newline() {
    let mut decoder = Decoder::new();
    assert!(decoder
        .feed(br#"{"type":"item.completed","item":{"id":"i","type":"agent_m"#)
        .is_empty());
    assert_eq!(
        decoder.feed(b"essage\",\"text\":\"OK\"}}\n"),
        vec![Event::turn_end(Some("OK".to_owned()))]
    );
}

#[test]
fn garbage_blank_and_unknown_lines_yield_nothing() {
    let stream = concat!(
        "\n",
        "   \n",
        "not json at all\n",
        "[1,2,3]\n",
        "\"a bare string\"\n",
        "{}\n",
        r#"{"type":"turn.started"}"#,
        "\n",
        r#"{"type":"session.configured","payload":{"whatever":1}}"#,
        "\n",
        r#"{"type":"item.completed","item":{"id":"i","type":"a_type_from_the_future"}}"#,
        "\n",
        r#"{"type":"item.completed"}"#,
        "\n",
        "{\"type\":\"item.completed\",\"item\":\n",
    );
    let (events, decoder) = decode_all(stream);
    assert!(events.is_empty(), "{events:?}");
    assert_eq!(decoder.usage(), Usage::default());
}

#[test]
fn an_unterminated_final_line_decodes_on_flush_never_before() {
    let mut decoder = Decoder::new();
    assert!(decoder
        .feed(br#"{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"OK"}}"#)
        .is_empty());
    assert_eq!(
        decoder.flush(),
        vec![Event::turn_end(Some("OK".to_owned()))]
    );
    assert!(decoder.flush().is_empty(), "flush is not repeatable");
}

#[test]
fn the_usage_maps_onto_the_seams_counters_and_claims_no_cost() {
    let (_, decoder) = decode_all(CAPTURED);
    assert_eq!(
        decoder.usage(),
        Usage {
            input_tokens: 27_315,
            output_tokens: 38,
            ..Usage::default()
        }
    );
}

#[test]
fn usage_stays_default_until_the_turn_completes() {
    let mut decoder = Decoder::new();
    decoder.feed(br#"{"type":"thread.started","thread_id":"thread-under-test"}"#);
    decoder.feed(b"\n");
    assert_eq!(decoder.usage(), Usage::default());
}

#[test]
fn a_missing_usage_counter_reads_as_zero_never_as_a_decode_failure() {
    let mut decoder = Decoder::new();
    let events = decoder.feed(b"{\"type\":\"turn.completed\",\"usage\":{\"output_tokens\":7}}\n");
    assert!(events.is_empty());
    assert_eq!(
        decoder.usage(),
        Usage {
            output_tokens: 7,
            ..Usage::default()
        }
    );
}

#[test]
fn an_error_item_is_surfaced_as_a_failed_result_and_kept_for_the_provider() {
    let mut decoder = Decoder::new();
    let events = decoder.feed(
        b"{\"type\":\"item.completed\",\"item\":{\"id\":\"item_0\",\"type\":\"error\",\"message\":\"boom\"}}\n",
    );
    assert_eq!(events, vec![Event::tool_result("error".to_owned(), false)]);
    assert_eq!(decoder.errors(), ["boom".to_owned()]);
}

#[test]
fn a_tool_item_that_never_started_still_gets_its_call() {
    let mut decoder = Decoder::new();
    let events = decoder.feed(
        b"{\"type\":\"item.completed\",\"item\":{\"id\":\"item_9\",\"type\":\"command_execution\",\"command\":\"ls\",\"exit_code\":2,\"status\":\"failed\"}}\n",
    );
    assert_eq!(
        events,
        vec![
            Event::tool_call(
                "command_execution".to_owned(),
                serde_json::json!({ "command": "ls" })
            ),
            Event::tool_result("command_execution".to_owned(), false),
        ]
    );
}

#[test]
fn a_tool_results_output_never_reaches_the_bus() {
    let mut decoder = Decoder::new();
    let events = decoder.feed(
        b"{\"type\":\"item.started\",\"item\":{\"id\":\"item_1\",\"type\":\"command_execution\",\"command\":\"ls\",\"aggregated_output\":\"secret\",\"exit_code\":null,\"status\":\"in_progress\"}}\n",
    );
    let encoded = serde_json::to_string(&events).expect("events encode");
    assert!(!encoded.contains("secret"), "{encoded}");
    assert!(!encoded.contains("in_progress"), "{encoded}");
}

#[test]
fn reasoning_is_not_the_answer_and_never_becomes_a_delta() {
    let mut decoder = Decoder::new();
    let events = decoder.feed(
        b"{\"type\":\"item.completed\",\"item\":{\"id\":\"item_1\",\"type\":\"reasoning\",\"text\":\"thinking\"}}\n",
    );
    assert!(events.is_empty(), "{events:?}");
}

#[test]
fn an_oversized_line_is_dropped_and_the_stream_recovers() {
    let mut decoder = Decoder::new();
    let flood = vec![b'x'; crate::LINE_CAP + 1];
    assert!(decoder.feed(&flood).is_empty());
    assert_eq!(
        decoder.feed(
            b"tail of the dropped line\n{\"type\":\"item.completed\",\"item\":{\"id\":\"i\",\"type\":\"agent_message\",\"text\":\"OK\"}}\n"
        ),
        vec![Event::turn_end(Some("OK".to_owned()))]
    );
}

#[test]
fn a_denied_run_with_a_model_reads_the_prompt_from_stdin_under_a_read_only_sandbox() {
    assert_eq!(
        argv(Some("gpt-5.1-codex-max"), &ToolPolicy::default()),
        [
            "exec",
            "--json",
            "--skip-git-repo-check",
            "--ephemeral",
            "--model",
            "gpt-5.1-codex-max",
            "--sandbox",
            "read-only",
            "-",
        ]
    );
}

#[test]
fn an_allowlist_run_without_a_model_gets_workspace_write() {
    assert_eq!(
        argv(
            None,
            &ToolPolicy {
                mode: ToolMode::Allowlist,
                allow: vec!["shell".to_owned()],
                ..ToolPolicy::default()
            }
        ),
        [
            "exec",
            "--json",
            "--skip-git-repo-check",
            "--ephemeral",
            "--sandbox",
            "workspace-write",
            "-",
        ]
    );
}

#[test]
fn no_policy_ever_produces_a_sandbox_bypass_flag() {
    for policy in [
        ToolPolicy::default(),
        ToolPolicy {
            mode: ToolMode::Allowlist,
            ..ToolPolicy::default()
        },
        ToolPolicy {
            mode: ToolMode::Allowlist,
            allow: vec!["anything".to_owned()],
            ..ToolPolicy::default()
        },
    ] {
        for model in [None, Some("m")] {
            let argv = argv(model, &policy);
            assert!(
                !argv.iter().any(|arg| arg.contains("dangerous")
                    || arg.contains("bypass")
                    || arg == "danger-full-access"),
                "{argv:?}"
            );
            assert_eq!(argv.last().map(String::as_str), Some("-"), "{argv:?}");
        }
    }
}
