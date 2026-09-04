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
    assert_eq!(good.budget, None, "no budget declared: a plain listen");
    let unknown = parse_config(
        br#"{ "topics": [], "source": "(p) => p", "origin": "agent", "deadline": 5 }"#,
    );
    assert!(
        unknown.is_err_and(|error| error.contains("deadline")),
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

/// Pin-bump 8 (jinnd M2-K25): the entry's `budget` is the kernel's
/// `delivery-budget` record spelled on the entry — `{ "fuel": <u64> }`,
/// optional, typed, and closed. A bare number is not a budget; zero is
/// carried as declared (the kernel refuses it at `listen`, on the
/// record — the provider never clamps).
#[test]
fn the_budget_is_the_kernels_delivery_budget_record_and_optional() {
    let budgeted = parse_config(
        br#"{ "topics": ["jinn:ui/before-send"], "source": "(p) => p", "origin": "human", "budget": { "fuel": 50000000 } }"#,
    )
    .expect("parses");
    assert_eq!(budgeted.budget, Some(Budget { fuel: 50_000_000 }));
    let zero = parse_config(
        br#"{ "topics": [], "source": "(p) => p", "origin": "human", "budget": { "fuel": 0 } }"#,
    )
    .expect("zero is carried, not clamped: the kernel's refusal is the record");
    assert_eq!(zero.budget, Some(Budget { fuel: 0 }));
    assert!(
        parse_config(br#"{ "topics": [], "source": "(p) => p", "origin": "human", "budget": 5 }"#)
            .is_err(),
        "a bare number is not the record"
    );
    assert!(
        parse_config(
            br#"{ "topics": [], "source": "(p) => p", "origin": "human", "budget": { "fuel": 1, "deadline": 1 } }"#
        )
        .is_err(),
        "the record is closed"
    );
    let round_trip = serde_json::to_value(&budgeted).expect("encodes");
    assert_eq!(
        round_trip["budget"],
        serde_json::json!({ "fuel": 50_000_000 })
    );
    let plain = serde_json::to_value(
        parse_config(br#"{ "topics": [], "source": "(p) => p", "origin": "human" }"#)
            .expect("parses"),
    )
    .expect("encodes");
    assert!(
        plain.get("budget").is_none(),
        "absent stays absent: {plain}"
    );
}
