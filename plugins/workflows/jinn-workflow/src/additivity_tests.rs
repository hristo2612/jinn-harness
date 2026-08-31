//! Additivity, proven EXHAUSTIVELY rather than by example or by sampling.
//!
//! The law lives in `lib.rs`: for every type and every variant of this
//! seam, at every nesting depth, decode-then-encode is lossless for
//! content the schema does not know. The walk itself is the sessions
//! seam's, borrowed unchanged through the todos seam — an unknown key
//! planted at EVERY object node, keyed by its path so a failure names
//! which node lost it — because the proof technique has one home and
//! this seam's inventory is the same shape.

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

fn todo_binding_doc() -> Value {
    json!({
        "store": "default",
        "todo": {
            "api-version": jinn_todo::API_VERSION,
            "title": "port the ledger",
            "body": "fold state from an append-only log",
            "acceptance": "the suite is green",
            "metadata": { "label": "a Todo" }
        },
        "dispatch": {
            "store": "default",
            "engine": { "engine": "echo", "model": "m", "effort": "high" },
            "cwd": "work",
            "message": "do the thing"
        }
    })
}

fn node_doc() -> Value {
    json!({ "id": "run-it", "kind": "dispatch", "title": "run it",
            "todo": todo_binding_doc() })
}

fn spec_doc() -> Value {
    json!({
        "api-version": API_VERSION,
        "name": "the release lane",
        "description": "build, verify, land",
        "nodes": [json!({ "id": "open", "kind": "checkpoint", "title": "open" }), node_doc()],
        "edges": [json!({ "from": "open", "to": "run-it", "kind": "on-done" })],
        "input": { "fields": [json!({ "name": "ticket", "kind": "string", "required": true })] },
        "actor": "planner",
        "metadata": { "label": "a workflow" }
    })
}

fn definition_doc() -> Value {
    json!({
        "api-version": API_VERSION, "workflow-id": "default-w1", "revision": 1,
        "spec": spec_doc(), "spec-digest": "fnv1a64:0000000000000000",
        "defined-ms": 10, "actor": "planner"
    })
}

fn node_run_doc() -> Value {
    json!({ "node-id": "run-it", "kind": "dispatch", "state": "done",
            "todo-store": "default", "todo-id": "default-1",
            "dispatch-id": "default-1-d1", "answer": "the work",
            "started-ms": 10, "ended-ms": 20 })
}

fn node_change_doc() -> Value {
    json!({ "seq": 0, "node-id": "run-it", "from": "pending", "to": "running",
            "actor": "planner", "note": "off we go", "at-ms": 10 })
}

fn refused_doc() -> Value {
    json!({ "seq": 0, "node-id": "run-it", "from": "pending", "to": "done",
            "actor": "planner", "at-ms": 10 })
}

fn run_doc() -> Value {
    json!({
        "api-version": API_VERSION, "run-id": "default-r1", "store": "default",
        "workflow-id": "default-w1", "definition-revision": 1,
        "spec-digest": "fnv1a64:0000000000000000", "spec": spec_doc(),
        "status": "running", "input": { "ticket": "PLA-1" },
        "nodes": [node_run_doc()], "history": [node_change_doc()],
        "refused": [refused_doc()], "actor": "planner", "started-ms": 10
    })
}

fn run_summary_doc() -> Value {
    json!({ "run-id": "default-r1", "workflow-id": "default-w1",
            "definition-revision": 1, "status": "failed", "reason": "node \"x\" ended failed",
            "nodes-ended": 1, "nodes-total": 2, "started-ms": 10, "ended-ms": 20 })
}

fn event_docs() -> Vec<Value> {
    vec![
        json!({ "kind": "defined", "workflow-id": "default-w1", "revision": 2 }),
        json!({ "kind": "run-started", "workflow-id": "default-w1", "revision": 2 }),
        json!({ "kind": "node-started", "node-id": "run-it" }),
        json!({ "kind": "node-ended", "node-id": "run-it", "outcome": "done" }),
        json!({ "kind": "node-ended", "node-id": "run-it", "outcome": "failed",
                "reason": "the engine refused" }),
        json!({ "kind": "node-transition-refused", "node-id": "run-it",
                "from": "pending", "to": "done", "actor": "planner" }),
        json!({ "kind": "run-ended", "status": "done" }),
        json!({ "kind": "run-ended", "status": "interrupted", "reason": "the daemon stopped" }),
        // A kind this version does not know rides through whole.
        json!({ "kind": "node-retried", "node-id": "run-it", "attempt": 2 }),
    ]
}

fn run_event_docs() -> Vec<Value> {
    event_docs()
        .into_iter()
        .map(|event| {
            let mut doc = json!({ "api-version": API_VERSION, "store": "default",
                                  "run-id": "default-r1", "seq": 3 });
            let object = doc.as_object_mut().expect("an object");
            for (key, value) in event.as_object().expect("an object") {
                object.insert(key.clone(), value.clone());
            }
            doc
        })
        .collect()
}

fn journal_docs() -> Vec<Value> {
    vec![
        json!({ "api-version": API_VERSION, "kind": "defined", "at-ms": 10,
                "workflow-id": "default-w1", "revision": 1, "spec": spec_doc(),
                "spec-digest": "fnv1a64:0000000000000000", "actor": "planner" }),
        json!({ "api-version": API_VERSION, "kind": "run-started", "at-ms": 10,
                "workflow-id": "default-w1", "revision": 1, "spec": spec_doc(),
                "spec-digest": "fnv1a64:0000000000000000",
                "input": { "ticket": "PLA-1" }, "actor": "planner" }),
        json!({ "api-version": API_VERSION, "kind": "node-state-changed", "at-ms": 20,
                "node-id": "run-it", "from": "pending", "to": "running",
                "todo-store": "default", "todo-id": "default-1",
                "dispatch-id": "default-1-d1", "actor": "planner" }),
        json!({ "api-version": API_VERSION, "kind": "node-transition-refused", "at-ms": 20,
                "node-id": "run-it", "from": "pending", "to": "done" }),
        json!({ "api-version": API_VERSION, "kind": "run-ended", "at-ms": 30,
                "status": "failed", "reason": "node \"run-it\" ended failed" }),
    ]
}

fn inventory() -> Vec<Wire> {
    vec![
        Wire {
            name: "TodoBinding",
            canonical: || vec![todo_binding_doc()],
            round_trip: round_trip::<TodoBinding>,
        },
        Wire {
            name: "NodeSpec",
            canonical: || vec![node_doc()],
            round_trip: round_trip::<NodeSpec>,
        },
        Wire {
            name: "EdgeSpec",
            canonical: || vec![json!({ "from": "a", "to": "b", "kind": "on-not-done" })],
            round_trip: round_trip::<EdgeSpec>,
        },
        Wire {
            name: "FieldSpec",
            canonical: || vec![json!({ "name": "ticket", "kind": "number", "required": false })],
            round_trip: round_trip::<FieldSpec>,
        },
        Wire {
            name: "InputSchema",
            canonical: || {
                vec![json!({ "fields": [json!({ "name": "ticket", "kind": "bool",
                                                "required": true })] })]
            },
            round_trip: round_trip::<InputSchema>,
        },
        Wire {
            name: "WorkflowSpec",
            canonical: || vec![spec_doc()],
            round_trip: round_trip::<WorkflowSpec>,
        },
        Wire {
            name: "DefineRequest",
            canonical: || {
                vec![
                    json!({ "spec": spec_doc() }),
                    json!({ "spec": spec_doc(), "workflow-id": "default-w1" }),
                ]
            },
            round_trip: round_trip::<DefineRequest>,
        },
        Wire {
            name: "WorkflowDefined",
            canonical: || {
                vec![
                    json!({ "api-version": API_VERSION, "workflow-id": "default-w1",
                             "store": "default", "revision": 2,
                             "spec-digest": "fnv1a64:0000000000000000" }),
                ]
            },
            round_trip: round_trip::<WorkflowDefined>,
        },
        Wire {
            name: "WorkflowRequest",
            canonical: || {
                vec![
                    json!({ "workflow-id": "default-w1" }),
                    json!({ "workflow-id": "default-w1", "revision": 1 }),
                ]
            },
            round_trip: round_trip::<WorkflowRequest>,
        },
        Wire {
            name: "StartRequest",
            canonical: || {
                vec![json!({ "workflow-id": "default-w1", "revision": 2,
                             "input": { "ticket": "PLA-1" }, "actor": "planner" })]
            },
            round_trip: round_trip::<StartRequest>,
        },
        Wire {
            name: "RunRequest",
            canonical: || vec![json!({ "run-id": "default-r1" })],
            round_trip: round_trip::<RunRequest>,
        },
        Wire {
            name: "CancelRequest",
            canonical: || {
                vec![
                    json!({ "run-id": "default-r1", "reason": "the operator stopped it",
                             "actor": "planner" }),
                ]
            },
            round_trip: round_trip::<CancelRequest>,
        },
        Wire {
            name: "ListRunsRequest",
            canonical: || vec![json!({ "workflow-id": "default-w1", "status": "running" })],
            round_trip: round_trip::<ListRunsRequest>,
        },
        Wire {
            name: "EventsRequest",
            canonical: || vec![json!({ "run-id": "default-r1", "after": 3, "limit": 10 })],
            round_trip: round_trip::<EventsRequest>,
        },
        Wire {
            name: "Definition",
            canonical: || vec![definition_doc()],
            round_trip: round_trip::<Definition>,
        },
        Wire {
            name: "WorkflowRecord",
            canonical: || {
                vec![
                    json!({ "api-version": API_VERSION, "workflow-id": "default-w1",
                             "store": "default", "latest-revision": 1,
                             "revisions": [definition_doc()] }),
                ]
            },
            round_trip: round_trip::<WorkflowRecord>,
        },
        Wire {
            name: "NodeRun",
            canonical: || vec![node_run_doc()],
            round_trip: round_trip::<NodeRun>,
        },
        Wire {
            name: "NodeChange",
            canonical: || vec![node_change_doc()],
            round_trip: round_trip::<NodeChange>,
        },
        Wire {
            name: "RefusedChange",
            canonical: || vec![refused_doc()],
            round_trip: round_trip::<RefusedChange>,
        },
        Wire {
            name: "RunRecord",
            canonical: || vec![run_doc()],
            round_trip: round_trip::<RunRecord>,
        },
        Wire {
            name: "RunSummary",
            canonical: || vec![run_summary_doc()],
            round_trip: round_trip::<RunSummary>,
        },
        Wire {
            name: "RunList",
            canonical: || {
                vec![json!({ "api-version": API_VERSION, "store": "default",
                             "runs": [run_summary_doc()] })]
            },
            round_trip: round_trip::<RunList>,
        },
        Wire {
            name: "WorkflowSummary",
            canonical: || {
                vec![
                    json!({ "workflow-id": "default-w1", "name": "the release lane",
                             "latest-revision": 2,
                             "spec-digest": "fnv1a64:0000000000000000", "nodes": 2 }),
                ]
            },
            round_trip: round_trip::<WorkflowSummary>,
        },
        Wire {
            name: "WorkflowList",
            canonical: || {
                vec![json!({ "api-version": API_VERSION, "store": "default",
                             "workflows": [json!({ "workflow-id": "default-w1",
                                                   "name": "n", "latest-revision": 1,
                                                   "spec-digest": "d", "nodes": 1 })] })]
            },
            round_trip: round_trip::<WorkflowList>,
        },
        Wire {
            name: "Event",
            canonical: event_docs,
            round_trip: round_trip::<Event>,
        },
        Wire {
            name: "RunEvent",
            canonical: run_event_docs,
            round_trip: round_trip::<RunEvent>,
        },
        Wire {
            name: "EventsPage",
            canonical: || {
                vec![json!({ "api-version": API_VERSION, "store": "default",
                             "run-id": "default-r1", "events": run_event_docs(),
                             "dropped": 2 })]
            },
            round_trip: round_trip::<EventsPage>,
        },
        Wire {
            name: "journal::Record",
            canonical: journal_docs,
            round_trip: round_trip::<journal::Record>,
        },
        Wire {
            name: "WorkflowError",
            canonical: || {
                vec![
                    json!({ "code": "refused", "message": "node \"a\" cannot move",
                             "node": "a", "from": "pending", "to": "done" }),
                ]
            },
            round_trip: round_trip::<WorkflowError>,
        },
        Wire {
            name: "Answer",
            canonical: || {
                vec![
                    json!({ "api-version": API_VERSION, "ok": run_doc() }),
                    json!({ "api-version": API_VERSION,
                            "error": { "code": "not-found", "message": "no such run" } }),
                ]
            },
            round_trip: round_trip::<Answer>,
        },
        Wire {
            name: "StoreSlot",
            canonical: || {
                vec![
                    json!({ "store": "default", "contract": "jinn:workflow.default",
                             "entry": "jinn-workflow-default" }),
                ]
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
