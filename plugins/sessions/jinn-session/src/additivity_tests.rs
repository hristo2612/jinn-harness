//! Additivity, proven EXHAUSTIVELY rather than by example or by sampling.
//!
//! The law lives in `lib.rs`: for every type and every variant of this
//! seam, at every nesting depth, decode-then-encode is lossless for
//! content the schema does not know.
//!
//! The engines seam proves the same law with a seeded generator that
//! sprinkles unknown keys at random depths. This seam's inventory is small
//! enough to do better: every object node in every canonical document gets
//! an unknown key, deterministically, so the proof covers the whole
//! placement space instead of a sample of it and a failure needs no seed
//! to reproduce. The law has one home; a proof of it may be sharper where
//! the shape allows.

use super::*;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

/// A `{"$secret": ...}` reference is the settings seam's CLOSED shape: its
/// decoder refuses an object with a second key, so nothing is planted
/// there. What that surface owes instead is the refusal, which is the
/// settings seam's own proof.
fn is_closed(object: &serde_json::Map<String, Value>) -> bool {
    object.contains_key("$secret")
}

/// Plants one unknown key at EVERY object node, keyed by its path so a
/// failure names exactly which node lost it.
fn plant(doc: &mut Value, planted: &mut Vec<String>, path: &str) {
    match doc {
        Value::Object(object) => {
            if is_closed(object) {
                return;
            }
            let keys: Vec<String> = object.keys().cloned().collect();
            for key in keys {
                let child = object.get_mut(&key).expect("just listed");
                plant(child, planted, &format!("{path}/{key}"));
            }
            let key = format!("x-unknown{}", path.replace('/', "-"));
            planted.push(format!("{path}/{key}"));
            object.insert(key, json!({ "from": "a newer peer", "depth": [1, 2] }));
        }
        Value::Array(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                plant(item, planted, &format!("{path}/{index}"));
            }
        }
        _ => {}
    }
}

fn round_trip<T: Serialize + DeserializeOwned>(doc: &Value) -> Value {
    let decoded: T = serde_json::from_value(doc.clone())
        .unwrap_or_else(|error| panic!("a canonical document decodes: {error}\n{doc:#}"));
    serde_json::to_value(&decoded).expect("it encodes")
}

struct Wire {
    name: &'static str,
    canonical: fn() -> Vec<Value>,
    round_trip: fn(&Value) -> Value,
}

fn spec_doc() -> Value {
    json!({
        "api-version": API_VERSION,
        "engine": { "engine": "echo", "model": "m", "effort": "high" },
        "cwd": "work",
        "tools": { "mode": "allowlist", "allow": ["Read"] },
        "attribution": { "owner": "operator" },
        "metadata": { "label": "a session" }
    })
}

fn turn_doc() -> Value {
    json!({
        "turn-id": "fs-1-t1", "seq": 0, "status": "done", "message": "hello",
        "answer": "hi", "run-id": "echo-1",
        "usage": { "input-tokens": 1, "output-tokens": 2, "cost-micro-usd": 3 },
        "started-ms": 10, "ended-ms": 20
    })
}

fn event_docs() -> Vec<Value> {
    vec![
        json!({ "kind": "created", "engine": "echo" }),
        json!({ "kind": "turn-started", "turn-id": "t1", "message": "hello" }),
        json!({ "kind": "delta", "turn-id": "t1", "text": "hi" }),
        json!({ "kind": "turn-ended", "turn-id": "t1",
                "usage": { "input-tokens": 1, "output-tokens": 2, "cost-micro-usd": 3 } }),
        json!({ "kind": "turn-failed", "turn-id": "t1", "reason": "the engine refused" }),
        json!({ "kind": "closed" }),
        // A kind from a NEWER peer: every field of it is unknown.
        json!({ "kind": "compacted", "kept": 4, "dropped": 12 }),
    ]
}

fn session_event_docs() -> Vec<Value> {
    event_docs()
        .into_iter()
        .map(|event| {
            let mut doc =
                json!({ "api-version": API_VERSION, "store": "fs", "session-id": "fs-1", "seq": 3 });
            let object = doc.as_object_mut().expect("an object");
            for (key, value) in event.as_object().expect("an object") {
                object.insert(key.clone(), value.clone());
            }
            doc
        })
        .collect()
}

fn inventory() -> Vec<Wire> {
    vec![
        Wire {
            name: "SessionSpec",
            canonical: || vec![spec_doc()],
            round_trip: round_trip::<SessionSpec>,
        },
        Wire {
            name: "CreateRequest",
            canonical: || vec![json!({ "spec": spec_doc() })],
            round_trip: round_trip::<CreateRequest>,
        },
        Wire {
            name: "SessionCreated",
            canonical: || {
                vec![json!({ "api-version": API_VERSION, "session-id": "fs-1",
                             "store": "fs", "engine": "echo" })]
            },
            round_trip: round_trip::<SessionCreated>,
        },
        Wire {
            name: "SendRequest",
            canonical: || vec![json!({ "session-id": "fs-1", "message": "hello" })],
            round_trip: round_trip::<SendRequest>,
        },
        Wire {
            name: "TurnAccepted",
            canonical: || {
                vec![json!({ "api-version": API_VERSION, "session-id": "fs-1",
                             "turn-id": "fs-1-t1" })]
            },
            round_trip: round_trip::<TurnAccepted>,
        },
        Wire {
            name: "MessagesRequest",
            canonical: || vec![json!({ "session-id": "fs-1", "offset": 2, "limit": 10 })],
            round_trip: round_trip::<MessagesRequest>,
        },
        Wire {
            name: "Turn",
            canonical: || vec![turn_doc()],
            round_trip: round_trip::<Turn>,
        },
        Wire {
            name: "SessionRecord",
            canonical: || {
                vec![
                    json!({ "api-version": API_VERSION, "session-id": "fs-1", "store": "fs",
                             "engine": "echo", "model": "m", "owner": "operator",
                             "status": "idle", "turns": 1, "log": [turn_doc()],
                             "created-ms": 5, "metadata": { "label": "a session" } }),
                ]
            },
            round_trip: round_trip::<SessionRecord>,
        },
        Wire {
            name: "Page",
            canonical: || {
                vec![
                    json!({ "api-version": API_VERSION, "session-id": "fs-1", "offset": 0,
                             "messages": [turn_doc()], "total": 3, "next-offset": 1 }),
                ]
            },
            round_trip: round_trip::<Page>,
        },
        Wire {
            name: "Event",
            canonical: event_docs,
            round_trip: round_trip::<Event>,
        },
        Wire {
            name: "SessionEvent",
            canonical: session_event_docs,
            round_trip: round_trip::<SessionEvent>,
        },
        Wire {
            name: "EventsRequest",
            canonical: || vec![json!({ "session-id": "fs-1", "after": 3, "limit": 10 })],
            round_trip: round_trip::<EventsRequest>,
        },
        Wire {
            name: "EventPage",
            canonical: || {
                vec![json!({ "api-version": API_VERSION, "session-id": "fs-1",
                             "events": session_event_docs(), "next-after": 4, "dropped": 0 })]
            },
            round_trip: round_trip::<EventPage>,
        },
        Wire {
            name: "journal::Record",
            canonical: || {
                vec![
                    json!({ "api-version": API_VERSION, "kind": "created", "at-ms": 1,
                            "spec": spec_doc() }),
                    json!({ "api-version": API_VERSION, "kind": "turn-started", "at-ms": 2,
                            "turn-id": "t1", "message": "hello" }),
                    json!({ "api-version": API_VERSION, "kind": "turn-ended", "at-ms": 3,
                            "turn-id": "t1", "status": "done", "answer": "hi",
                            "run-id": "echo-1",
                            "usage": { "input-tokens": 1, "output-tokens": 2,
                                       "cost-micro-usd": 3 } }),
                    json!({ "api-version": API_VERSION, "kind": "closed", "at-ms": 4 }),
                ]
            },
            round_trip: round_trip::<journal::Record>,
        },
        Wire {
            name: "Answer",
            canonical: || {
                vec![
                    json!({ "api-version": API_VERSION, "ok": { "session-id": "fs-1" } }),
                    json!({ "api-version": API_VERSION,
                            "error": { "code": "not-found", "message": "no such session" } }),
                ]
            },
            round_trip: round_trip::<Answer>,
        },
        Wire {
            name: "StoreSlot",
            canonical: || {
                vec![json!({ "store": "fs", "contract": "jinn:session.fs",
                             "entry": "sessions-fs" })]
            },
            round_trip: round_trip::<StoreSlot>,
        },
    ]
}

#[test]
fn every_wire_type_carries_unknown_content_at_every_node_through_a_round_trip() {
    for wire in inventory() {
        for (index, canonical) in (wire.canonical)().into_iter().enumerate() {
            let mut doc = canonical.clone();
            let mut planted = Vec::new();
            plant(&mut doc, &mut planted, "");
            assert!(
                !planted.is_empty(),
                "{} document {index} has no object node to plant in",
                wire.name
            );
            let back = (wire.round_trip)(&doc);
            assert_eq!(
                back, doc,
                "{} document {index} lost content it does not know (planted {planted:?})",
                wire.name
            );
        }
    }
}

#[test]
fn a_secret_reference_carrying_a_sibling_is_refused_naming_the_surface() {
    let refused = serde_json::from_value::<jinn_settings::SecretRef>(
        json!({ "$secret": "sessions/key", "x-unknown": 1 }),
    )
    .expect_err("a closed surface refuses");
    let message = refused.to_string();
    assert!(message.contains("closed surface"), "{message}");
    assert!(message.contains("$secret"), "{message}");
}
