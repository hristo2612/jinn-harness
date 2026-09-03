//! The definition's own proofs: the schema is closed, a duplicate topic
//! is a fault, the breadcrumbs are the card's, and the two programs are
//! well-formed JS with the payload carried as data.

use super::*;

const GREEN: &str = "(p) => ({ ...p, text: p.text + ' 🟢' })";

#[test]
fn the_config_schema_is_closed_and_typed() {
    let good = parse_config(
        br#"{ "topics": ["jinn:ui/before-send"], "source": "(p) => p", "origin": "human" }"#,
    )
    .expect("parses");
    assert_eq!(good.origin, Origin::Human);
    assert_eq!(good.topics, ["jinn:ui/before-send"]);
    let unknown =
        parse_config(br#"{ "topics": [], "source": "(p) => p", "origin": "agent", "budget": 5 }"#);
    assert!(
        unknown.is_err_and(|error| error.contains("budget")),
        "an unknown field is a fault naming it"
    );
    assert!(parse_config(br#"{ "topics": [], "source": "(p) => p" }"#).is_err());
    assert!(
        parse_config(br#"{ "topics": [], "source": "(p) => p", "origin": "robot" }"#).is_err(),
        "origin is agent | human and nothing else"
    );
    let twice = parse_config(
        br#"{ "topics": ["jinn:ui/before-send", "jinn:ui/before-send"], "source": "(p) => p", "origin": "human" }"#,
    );
    assert_eq!(
        twice,
        Err("topic \"jinn:ui/before-send\" is listed twice".into())
    );
}

#[test]
fn the_breadcrumbs_and_the_source_hash_are_the_cards() {
    assert_eq!(
        BREADCRUMBS,
        [
            "activate entered",
            "config parsed",
            "js context built",
            "js evaluated"
        ]
    );
    let crumb = source_breadcrumb(GREEN);
    assert!(crumb.starts_with("source sha256:"));
    assert_eq!(crumb.len(), "source sha256:".len() + 64);
    assert_ne!(crumb, source_breadcrumb("(p) => p"));
}

#[test]
fn the_delivery_program_carries_the_payload_as_data_and_not_as_code() {
    let payload = br#"{"text":"hello\", 1); alert(\"x","session-id":"s"}"#;
    let program = delivery(GREEN, payload).expect("utf-8");
    assert!(program.contains("JSON.parse(\"{"));
    assert!(program.contains(GREEN));
    assert!(
        !program.contains("alert(\"x"),
        "the payload's quotes are escaped inside the literal: {program}"
    );
    assert!(
        delivery(GREEN, &[0xff]).is_err(),
        "a non-UTF-8 payload is refused"
    );
    assert_eq!(
        self_test(GREEN),
        format!("typeof (\n{GREEN}\n) === \"function\"")
    );
}
