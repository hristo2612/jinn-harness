//! Additivity, proven EXHAUSTIVELY rather than by example or by sampling.
//!
//! The law lives in `lib.rs`: for every type and every variant of this
//! seam, at every nesting depth, decode-then-encode is lossless for
//! content the schema does not know. The walk itself is the sessions
//! seam's — an unknown key planted at EVERY object node, keyed by its
//! path so a failure names which node lost it — because the proof
//! technique has one home and this seam's inventory is the same shape.

use super::*;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

/// A `{"$secret": ...}` reference is the settings seam's CLOSED shape.
fn is_closed(object: &serde_json::Map<String, Value>) -> bool {
    object.contains_key("$secret")
}

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
        "title": "port the ledger",
        "body": "fold state from an append-only log",
        "acceptance": "the suite is green",
        "department": "platform",
        "priority": 1,
        "actor": "planner",
        "metadata": { "label": "a Todo" }
    })
}

fn dispatch_spec_doc() -> Value {
    json!({
        "store": "default",
        "engine": { "engine": "echo", "model": "m", "effort": "high" },
        "cwd": "work",
        "message": "do the thing"
    })
}

fn dispatch_doc() -> Value {
    json!({
        "dispatch-id": "default-1-d1", "session-store": "default", "engine": "echo",
        "session-id": "default-1", "turn-id": "default-1-t1", "status": "done",
        "answer": "did it", "started-ms": 20, "ended-ms": 30
    })
}

fn change_doc() -> Value {
    json!({ "seq": 0, "from": "backlog", "to": "executing", "actor": "planner",
            "note": "starting", "at-ms": 20 })
}

fn comment_doc() -> Value {
    json!({ "comment-id": "default-1-c1", "seq": 0, "body": "started",
            "actor": "planner", "at-ms": 20 })
}

fn summary_doc() -> Value {
    json!({ "todo-id": "default-1", "store": "default", "title": "port the ledger",
            "status": "executing", "declared-status": "executing", "department": "platform",
            "priority": 1, "comments": 1, "created-ms": 10 })
}

fn record_doc() -> Value {
    json!({
        "api-version": API_VERSION, "todo-id": "default-1", "store": "default",
        "title": "port the ledger", "body": "the body", "acceptance": "green",
        "department": "platform", "priority": 1,
        "status": "blocked", "declared-status": "executing",
        "status-reason": INTERRUPTED_STATUS_REASON,
        "history": [change_doc()],
        "refused": [json!({ "seq": 0, "from": "executing", "to": "done",
                            "actor": "the producer", "at-ms": 25 })],
        "comments": [comment_doc()],
        "dispatches": [dispatch_doc()],
        "actor": "planner", "created-ms": 10, "metadata": { "label": "a Todo" }
    })
}

fn event_docs() -> Vec<Value> {
    vec![
        json!({ "kind": "created", "title": "port the ledger" }),
        json!({ "kind": "status-changed", "from": "backlog", "to": "executing",
                "actor": "planner" }),
        json!({ "kind": "transition-refused", "from": "executing", "to": "done",
                "actor": "the producer" }),
        json!({ "kind": "commented", "comment-id": "default-1-c1", "actor": "planner" }),
        json!({ "kind": "dispatched", "dispatch-id": "default-1-d1",
                "session-store": "default", "engine": "echo" }),
        json!({ "kind": "dispatch-ended", "dispatch-id": "default-1-d1",
                "dispatch-status": "interrupted", "reason": "the daemon stopped" }),
        json!({ "kind": "closed", "status": "done" }),
        // A kind from a NEWER peer: every field of it is unknown.
        json!({ "kind": "escalated", "to": "the COO", "after-ms": 900 }),
    ]
}

fn todo_event_docs() -> Vec<Value> {
    event_docs()
        .into_iter()
        .map(|event| {
            let mut doc = json!({ "api-version": API_VERSION, "store": "default",
                                  "todo-id": "default-1", "seq": 3 });
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
            name: "TodoSpec",
            canonical: || vec![spec_doc()],
            round_trip: round_trip::<TodoSpec>,
        },
        Wire {
            name: "CreateRequest",
            canonical: || vec![json!({ "spec": spec_doc() })],
            round_trip: round_trip::<CreateRequest>,
        },
        Wire {
            name: "TodoCreated",
            canonical: || {
                vec![json!({ "api-version": API_VERSION, "todo-id": "default-1",
                             "store": "default", "status": "backlog" })]
            },
            round_trip: round_trip::<TodoCreated>,
        },
        Wire {
            name: "UpdateRequest",
            canonical: || {
                vec![json!({ "todo-id": "default-1", "status": "in-review",
                             "note": "ready", "actor": "planner" })]
            },
            round_trip: round_trip::<UpdateRequest>,
        },
        Wire {
            name: "CommentRequest",
            canonical: || {
                vec![json!({ "todo-id": "default-1", "body": "started",
                             "actor": "planner" })]
            },
            round_trip: round_trip::<CommentRequest>,
        },
        Wire {
            name: "ListRequest",
            canonical: || {
                vec![json!({ "status": "executing", "department": "platform",
                             "roots-only": true })]
            },
            round_trip: round_trip::<ListRequest>,
        },
        Wire {
            name: "DispatchSpec",
            canonical: || vec![dispatch_spec_doc()],
            round_trip: round_trip::<DispatchSpec>,
        },
        Wire {
            name: "DispatchRequest",
            canonical: || {
                vec![
                    json!({ "todo-id": "default-1", "dispatch": dispatch_spec_doc(),
                             "actor": "planner" }),
                ]
            },
            round_trip: round_trip::<DispatchRequest>,
        },
        Wire {
            name: "Dispatch",
            canonical: || vec![dispatch_doc()],
            round_trip: round_trip::<Dispatch>,
        },
        Wire {
            name: "StatusChange",
            canonical: || vec![change_doc()],
            round_trip: round_trip::<StatusChange>,
        },
        Wire {
            name: "Comment",
            canonical: || vec![comment_doc()],
            round_trip: round_trip::<Comment>,
        },
        Wire {
            name: "TodoRecord",
            canonical: || vec![record_doc()],
            round_trip: round_trip::<TodoRecord>,
        },
        Wire {
            name: "TodoSummary",
            canonical: || vec![summary_doc()],
            round_trip: round_trip::<TodoSummary>,
        },
        Wire {
            name: "TodoList",
            canonical: || {
                vec![json!({ "api-version": API_VERSION, "store": "default",
                             "todos": [summary_doc()], "total": 3 })]
            },
            round_trip: round_trip::<TodoList>,
        },
        Wire {
            name: "Tree",
            canonical: || {
                let mut child = summary_doc();
                child["todo-id"] = json!("default-2");
                child["parent"] = json!("default-1");
                vec![json!({
                    "api-version": API_VERSION, "store": "default",
                    "root": { "todo-id": "default-1", "store": "default",
                              "title": "root", "status": "executing",
                              "declared-status": "executing", "comments": 0,
                              "created-ms": 10,
                              "children": [{ "todo-id": "default-2", "store": "default",
                                             "title": "child", "status": "backlog",
                                             "declared-status": "backlog",
                                             "parent": "default-1", "comments": 0,
                                             "created-ms": 11, "children": [] }] }
                })]
            },
            round_trip: round_trip::<Tree>,
        },
        Wire {
            name: "Event",
            canonical: event_docs,
            round_trip: round_trip::<Event>,
        },
        Wire {
            name: "TodoEvent",
            canonical: todo_event_docs,
            round_trip: round_trip::<TodoEvent>,
        },
        Wire {
            name: "EventsRequest",
            canonical: || vec![json!({ "todo-id": "default-1", "after": 3, "limit": 10 })],
            round_trip: round_trip::<EventsRequest>,
        },
        Wire {
            name: "EventPage",
            canonical: || {
                vec![json!({ "api-version": API_VERSION, "todo-id": "default-1",
                             "events": todo_event_docs(), "next-after": 4, "dropped": 0 })]
            },
            round_trip: round_trip::<EventPage>,
        },
        Wire {
            name: "journal::Record",
            canonical: || {
                vec![
                    json!({ "api-version": API_VERSION, "kind": "created", "at-ms": 1,
                            "spec": spec_doc(), "actor": "planner" }),
                    json!({ "api-version": API_VERSION, "kind": "status-changed", "at-ms": 2,
                            "from": "backlog", "to": "executing", "actor": "planner",
                            "note": "starting" }),
                    json!({ "api-version": API_VERSION, "kind": "transition-refused",
                            "at-ms": 3, "from": "executing", "to": "done",
                            "actor": "the producer" }),
                    json!({ "api-version": API_VERSION, "kind": "commented", "at-ms": 4,
                            "comment-id": "default-1-c1", "body": "started",
                            "actor": "planner" }),
                    json!({ "api-version": API_VERSION, "kind": "dispatch-started",
                            "at-ms": 5, "dispatch-id": "default-1-d1",
                            "session-store": "default", "engine": "echo" }),
                    json!({ "api-version": API_VERSION, "kind": "dispatch-ended", "at-ms": 6,
                            "dispatch-id": "default-1-d1", "session-id": "default-1",
                            "turn-id": "default-1-t1", "dispatch-status": "done",
                            "answer": "did it" }),
                ]
            },
            round_trip: round_trip::<journal::Record>,
        },
        Wire {
            name: "Answer",
            canonical: || {
                vec![
                    json!({ "api-version": API_VERSION, "ok": { "todo-id": "default-1" } }),
                    json!({ "api-version": API_VERSION,
                            "error": { "code": "refused",
                                       "message": "this Todo cannot move executing -> done",
                                       "from": "executing", "to": "done" } }),
                ]
            },
            round_trip: round_trip::<Answer>,
        },
        Wire {
            name: "StoreSlot",
            canonical: || {
                vec![json!({ "store": "default", "contract": "jinn:todo.default",
                             "entry": "jinn-todo-default" })]
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
