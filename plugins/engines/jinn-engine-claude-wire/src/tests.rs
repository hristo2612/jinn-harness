//! The codec's tests, written against LINE SHAPES captured from a live
//! `claude -p --output-format stream-json --verbose` run (the identifying
//! fields — cwd, session id, model tag — are replaced by neutral stand-ins
//! per AGENTS.md's zero-personal-data bound; the SHAPES are verbatim).

use jinn_engine::ToolMode;

use super::*;
use jinn_engine::EventKind;

const INIT: &str = r#"{"type":"system","subtype":"init","cwd":"/work","session_id":"s-1","model":"claude-opus-5","tools":["Bash","Read"],"permissionMode":"default"}"#;
const HOOK_STARTED: &str = r#"{"type":"system","subtype":"hook_started","hook_id":"h-1","hook_name":"SessionStart:startup"}"#;
const HOOK_RESPONSE: &str =
    r#"{"type":"system","subtype":"hook_response","hook_id":"h-1","output":"{}","exit_code":0}"#;
const THINKING_TOKENS: &str =
    r#"{"type":"system","subtype":"thinking_tokens","thinking_tokens":1024}"#;
const ASSISTANT_TEXT: &str = r#"{"type":"assistant","message":{"model":"claude-opus-5","id":"msg_1","type":"message","role":"assistant","content":[{"type":"text","text":"OK"}],"usage":{"input_tokens":2,"output_tokens":1}},"parent_tool_use_id":null}"#;
const ASSISTANT_TWO_TEXTS: &str = r#"{"type":"assistant","message":{"model":"claude-opus-5","content":[{"type":"text","text":"first"},{"type":"text","text":" second"}]}}"#;
const ASSISTANT_THINKING_THEN_TOOL: &str = r#"{"type":"assistant","message":{"model":"claude-opus-5","content":[{"type":"thinking","thinking":"weighing it","signature":"sig"},{"type":"tool_use","id":"toolu_1","name":"Bash","input":{"command":"echo ok"}}]}}"#;
const TOOL_RESULT_OK: &str = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","is_error":false,"content":"ok"}]}}"#;
const TOOL_RESULT_ERR: &str = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_1","is_error":true,"content":"boom"}]}}"#;
const TOOL_RESULT_STRAY: &str = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_stray","is_error":false}]}}"#;
const RATE_LIMIT: &str =
    r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1788009000}}"#;
const RESULT_SUCCESS: &str = r#"{"type":"result","subtype":"success","is_error":false,"result":"OK","num_turns":1,"duration_ms":1623,"total_cost_usd":0.0193045,"usage":{"input_tokens":10,"output_tokens":67,"cache_read_input_tokens":13615,"cache_creation_input_tokens":8799}}"#;
const RESULT_FAILED: &str = r#"{"type":"result","subtype":"error_max_turns","is_error":true,"result":"turn limit","num_turns":9,"duration_ms":900,"total_cost_usd":0.5,"usage":{"input_tokens":1,"output_tokens":2}}"#;

/// Every line of one whole run, as the pipe carries it.
fn stream() -> String {
    [
        HOOK_STARTED,
        HOOK_RESPONSE,
        INIT,
        THINKING_TOKENS,
        ASSISTANT_THINKING_THEN_TOOL,
        TOOL_RESULT_OK,
        ASSISTANT_TEXT,
        RATE_LIMIT,
        RESULT_SUCCESS,
    ]
    .join("\n")
        + "\n"
}

fn decode_all(text: &str) -> Vec<Event> {
    let mut decoder = Decoder::new();
    let mut events = decoder.feed(text.as_bytes());
    events.extend(decoder.flush());
    events
}

#[test]
fn an_init_line_starts_the_run_with_the_model_it_names() {
    assert_eq!(
        decode_all(&format!("{INIT}\n")),
        vec![Event::started(Some("claude-opus-5".to_owned()))]
    );
}

#[test]
fn noise_is_nothing_never_a_fabricated_event_and_never_a_panic() {
    for line in [
        HOOK_STARTED,
        HOOK_RESPONSE,
        THINKING_TOKENS,
        RATE_LIMIT,
        "",
        "   ",
        "not json at all",
        "{",
        "[1,2,3]",
        "null",
        r#"{"type":"a_kind_from_a_newer_cli","payload":{}}"#,
        r#"{"no-type":true}"#,
        r#"{"type":"system","subtype":"init"}"#, // an init with no model
    ] {
        let events = decode_all(&format!("{line}\n"));
        let expected: Vec<Event> = if line == r#"{"type":"system","subtype":"init"}"# {
            vec![Event::started(None)]
        } else {
            Vec::new()
        };
        assert_eq!(events, expected, "line {line:?}");
    }
}

#[test]
fn an_assistant_message_yields_one_delta_per_text_block() {
    assert_eq!(
        decode_all(&format!("{ASSISTANT_TWO_TEXTS}\n")),
        vec![
            Event::delta("first".to_owned()),
            Event::delta(" second".to_owned()),
        ]
    );
}

#[test]
fn a_thinking_block_is_not_the_answer_and_yields_nothing() {
    let events = decode_all(&format!("{ASSISTANT_THINKING_THEN_TOOL}\n"));
    assert_eq!(
        events,
        vec![Event::tool_call(
            "Bash".to_owned(),
            serde_json::json!({ "command": "echo ok" })
        )]
    );
}

#[test]
fn a_tool_use_and_its_result_correlate_through_the_tool_use_id() {
    let events = decode_all(&format!(
        "{ASSISTANT_THINKING_THEN_TOOL}\n{TOOL_RESULT_OK}\n"
    ));
    assert_eq!(
        events,
        vec![
            Event::tool_call(
                "Bash".to_owned(),
                serde_json::json!({ "command": "echo ok" })
            ),
            Event::tool_result("Bash".to_owned(), true),
        ]
    );
}

#[test]
fn a_failing_tool_result_is_not_ok() {
    let events = decode_all(&format!(
        "{ASSISTANT_THINKING_THEN_TOOL}\n{TOOL_RESULT_ERR}\n"
    ));
    assert_eq!(events[1], Event::tool_result("Bash".to_owned(), false));
}

#[test]
fn an_uncorrelated_tool_result_is_honestly_nameless_never_invented() {
    assert_eq!(
        decode_all(&format!("{TOOL_RESULT_STRAY}\n")),
        vec![Event::tool_result(String::new(), true)]
    );
}

#[test]
fn the_result_line_ends_the_turn_and_the_codec_never_exits_the_run() {
    let events = decode_all(&stream());
    assert_eq!(events.last(), Some(&Event::turn_end(Some("OK".to_owned()))));
    // The exit belongs to the PROCESS, not the stream: the provider emits
    // it from the real `wait` status.
    assert!(!events
        .iter()
        .any(|event| matches!(event.kind, EventKind::Exited { .. })));
}

#[test]
fn the_result_lines_usage_is_the_provider_s_to_attach_cost_rounded_to_micro_usd() {
    let mut decoder = Decoder::new();
    // Nothing to attach before the result line.
    assert_eq!(decoder.usage(), Usage::default());
    decoder.feed(stream().as_bytes());
    assert_eq!(
        decoder.usage(),
        Usage {
            // Cache reads and cache writes ARE input tokens read; the seam
            // has ONE home for them, so all three counters land there
            // (10 + 13_615 + 8_799).
            input_tokens: 22_424,
            output_tokens: 67,
            // 0.0193045 USD → 19_304.5 micro-USD → rounded, never floored.
            cost_micro_usd: 19_305,
            ..Usage::default()
        }
    );
    assert!(!decoder.failed());
    assert_eq!(decoder.result_subtype(), Some("success"));
}

#[test]
fn a_failed_result_is_still_a_turn_end_and_the_failure_stays_visible() {
    let mut decoder = Decoder::new();
    let events = decoder.feed(format!("{RESULT_FAILED}\n").as_bytes());
    assert_eq!(events, vec![Event::turn_end(Some("turn limit".to_owned()))]);
    assert!(decoder.failed());
    assert_eq!(decoder.result_subtype(), Some("error_max_turns"));
    assert_eq!(decoder.usage().cost_micro_usd, 500_000);
}

#[test]
fn a_success_subtype_with_is_error_set_still_reads_as_failed() {
    let mut decoder = Decoder::new();
    decoder.feed(br#"{"type":"result","subtype":"success","is_error":true,"result":"x"}"#);
    decoder.feed(b"\n");
    assert!(decoder.failed());
}

#[test]
fn ndjson_arrives_in_chunks_so_every_byte_boundary_decodes_the_same() {
    let whole = stream();
    let expected = decode_all(&whole);
    assert!(expected.len() > 3, "the fixture stream carries real events");
    for split in 0..=whole.len() {
        let mut decoder = Decoder::new();
        let mut events = decoder.feed(&whole.as_bytes()[..split]);
        events.extend(decoder.feed(&whole.as_bytes()[split..]));
        events.extend(decoder.flush());
        assert_eq!(events, expected, "split at byte {split}");
    }
}

#[test]
fn a_partial_line_waits_for_its_newline_and_a_trailing_one_flushes() {
    let mut decoder = Decoder::new();
    assert!(decoder.feed(&INIT.as_bytes()[..20]).is_empty());
    assert!(decoder.feed(&INIT.as_bytes()[20..]).is_empty());
    // No newline ever came (the pipe hit EOF): `flush` is the last word.
    assert_eq!(
        decoder.flush(),
        vec![Event::started(Some("claude-opus-5".to_owned()))]
    );
    assert!(decoder.flush().is_empty());
}

#[test]
fn carriage_returns_and_a_final_line_without_a_newline_are_both_ordinary() {
    let mut decoder = Decoder::new();
    let mut events = decoder.feed(format!("{INIT}\r\n{ASSISTANT_TEXT}").as_bytes());
    events.extend(decoder.flush());
    assert_eq!(
        events,
        vec![
            Event::started(Some("claude-opus-5".to_owned())),
            Event::delta("OK".to_owned()),
        ]
    );
}

#[test]
fn a_line_past_the_cap_is_dropped_whole_so_memory_stays_bounded() {
    let mut decoder = Decoder::new();
    let flood = vec![b'x'; LINE_CAP + 1];
    assert!(decoder.feed(&flood).is_empty());
    assert!(decoder.buffered() <= LINE_CAP);
    // The rest of that line is dropped with it, and the NEXT line decodes.
    assert!(decoder.feed(b"more of the same line").is_empty());
    assert_eq!(
        decoder.feed(format!("\n{INIT}\n").as_bytes()),
        vec![Event::started(Some("claude-opus-5".to_owned()))]
    );
}

#[test]
fn bytes_that_are_not_utf8_are_not_json_and_yield_nothing() {
    let mut decoder = Decoder::new();
    assert!(decoder.feed(&[0xff, 0xfe, b'\n']).is_empty());
    assert_eq!(
        decoder.feed(format!("{INIT}\n").as_bytes()),
        vec![Event::started(Some("claude-opus-5".to_owned()))]
    );
}

#[test]
fn the_argv_is_the_stream_json_lane_with_the_tool_policy_last() {
    // Default-deny with a model — the packet's worked example.
    assert_eq!(
        argv(Some("claude-opus-5"), &ToolPolicy::default()),
        vec![
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--model",
            "claude-opus-5",
            "--allowedTools",
            "",
        ]
    );
    // No model: the CLI's own default stands, never a name we invented.
    assert_eq!(
        argv(None, &ToolPolicy::default()),
        vec![
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--allowedTools",
            "",
        ]
    );
    // An allowlist, comma-joined (`--allowedTools` takes "comma or
    // space-separated" names and is VARIADIC, so it is always last).
    assert_eq!(
        argv(
            None,
            &ToolPolicy {
                mode: ToolMode::Allowlist,
                allow: vec!["Read".to_owned(), "Bash(git *)".to_owned()],
                ..ToolPolicy::default()
            }
        )
        .last()
        .map(String::as_str),
        Some("Read,Bash(git *)")
    );
    // An empty allowlist admits nothing, exactly like denied.
    assert_eq!(
        argv(
            None,
            &ToolPolicy {
                mode: ToolMode::Allowlist,
                ..ToolPolicy::default()
            }
        ),
        argv(None, &ToolPolicy::default())
    );
    // The prompt is NEVER an argv element (argv is world-readable).
    assert!(argv(Some("m"), &ToolPolicy::default())
        .iter()
        .all(|arg| !arg.contains("prompt")));
}

#[test]
fn the_config_carries_every_machine_specific_value_with_working_defaults() {
    let parsed = parse_config(
        br#"{"engine":"claude","command":"/opt/bin/claude","models":["a","b"],"default-model":"a"}"#,
    )
    .expect("a minimal entry parses");
    assert_eq!(parsed.engine, "claude");
    assert_eq!(parsed.command, "/opt/bin/claude");
    assert_eq!(parsed.models, vec!["a".to_owned(), "b".to_owned()]);
    assert_eq!(parsed.default_model.as_deref(), Some("a"));
    assert_eq!(parsed.poll_ms, DEFAULT_POLL_MS);
    assert_eq!(parsed.keep_runs, DEFAULT_KEEP_RUNS);

    let full = parse_config(
        br#"{"engine":"claude-fast","command":"/opt/bin/claude","poll-ms":1,"keep-runs":3,"entry-id":"engines/claude"}"#,
    )
    .expect("unknown keys are additive, never fatal");
    assert_eq!(full.engine, "claude-fast");
    // A zero or near-zero poll is a busy loop; the floor is the bound.
    assert_eq!(full.poll_ms, MIN_POLL_MS);
    assert_eq!(full.keep_runs, 3);

    // An entry that cannot name its engine or its CLI is not a provider.
    assert!(parse_config(b"").is_err());
    assert!(parse_config(b"{}").is_err());
    assert!(parse_config(br#"{"engine":"claude"}"#).is_err());
    assert!(parse_config(br#"{"engine":"","command":"/opt/bin/claude"}"#).is_err());
    assert!(parse_config(br#"{"engine":"claude","command":""}"#).is_err());
    assert!(parse_config(b"not json").is_err());
}
