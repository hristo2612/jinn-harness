//! Additivity, proven by PROPERTY rather than by example.
//!
//! The law lives in one place (`lib.rs`, "Additivity"): for every type and
//! every variant of this seam, at every nesting depth, decode-then-encode
//! is lossless for content the schema does not know. This module is its
//! proof — a generator sprinkles unknown keys at random depths into a
//! canonical document of each wire type, round-trips it through that
//! type, and asserts the encoded document is byte-for-byte the one that
//! went in.
//!
//! It is a property and not a table because the two additivity defects
//! this seam shipped were both "a spot nobody wrote an example for": a
//! generator that produces the whole space of unknown-key placements
//! would have produced each of them on its own, and does (see
//! [`the_verifiers_own_probe_is_one_sample_of_the_property`]).

use super::*;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};

/// A seeded xorshift — the generator is a PROPERTY over many samples and
/// a failure has to be reproducible from its seed, which a thread-local
/// entropy source cannot give.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // A zero state is a fixed point of xorshift; never start there.
        Self(seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound.max(1)
    }

    /// An unknown VALUE a newer peer might carry: scalars, but also whole
    /// records and arrays, so nesting inside the unknown is covered too.
    fn value(&mut self, depth: u32) -> Value {
        match self.below(if depth == 0 { 5 } else { 7 }) {
            0 => Value::Null,
            1 => Value::Bool(self.next().is_multiple_of(2)),
            2 => json!(self.below(1_000_000)),
            3 => json!(format!("v{}", self.below(1_000))),
            4 => json!(-(self.below(1_000) as i64)),
            5 => Value::Array((0..self.below(3)).map(|_| self.value(depth - 1)).collect()),
            _ => {
                let mut object = serde_json::Map::new();
                for index in 0..self.below(3) {
                    object.insert(format!("n{index}"), self.value(depth - 1));
                }
                Value::Object(object)
            }
        }
    }
}

/// A `{"$secret": ...}` reference is the SETTINGS seam's closed shape —
/// `jinn_settings::is_secret_ref` refuses an object with a second key, so
/// a reference carrying an extra one is not a reference. It is the one
/// nested surface this seam does not inject into; the definition README
/// names it.
fn is_closed(object: &serde_json::Map<String, Value>) -> bool {
    object.contains_key("$secret")
}

/// A homogeneous MAP of a closed shape (`secrets`) is not a record with
/// room for an unknown field: an extra key there is a malformed entry of
/// the map's own value type, not a newer peer's addition. Injection stops
/// at such a node; its values are still walked.
fn is_map_of_closed(object: &serde_json::Map<String, Value>) -> bool {
    !object.is_empty()
        && object
            .values()
            .all(|value| value.as_object().is_some_and(is_closed))
}

/// Sprinkles unknown keys through `doc` at random depths, answering the
/// keys it planted so the assertion can name what was lost. Every object
/// node in the document is a candidate, which is what makes this cover
/// known variants, unknown variants and nested records alike.
fn sprinkle(doc: &mut Value, rng: &mut Rng, planted: &mut Vec<String>, path: String) {
    match doc {
        Value::Object(object) => {
            if is_closed(object) {
                return;
            }
            let closed_map = is_map_of_closed(object);
            for (key, child) in object.iter_mut() {
                sprinkle(child, rng, planted, format!("{path}/{key}"));
            }
            if closed_map {
                return;
            }
            // Two thirds of the nodes get one, so a document usually has
            // several and sometimes has none at that level.
            if rng.below(3) != 0 {
                let key = format!("x-{}", rng.below(10_000));
                let value = rng.value(2);
                planted.push(format!("{path}/{key}"));
                object.insert(key, value);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                sprinkle(item, rng, planted, format!("{path}/{index}"));
            }
        }
        _ => {}
    }
}

/// Decode `doc` as `T`, encode it again, and answer what came back.
fn round_trip<T: Serialize + DeserializeOwned>(doc: &Value) -> Value {
    let decoded: T = serde_json::from_value(doc.clone())
        .unwrap_or_else(|error| panic!("a canonical document decodes: {error}\n{doc:#}"));
    serde_json::to_value(&decoded).expect("it encodes")
}

/// One wire type: its name and the round trip through it.
struct Wire {
    name: &'static str,
    canonical: fn() -> Vec<Value>,
    round_trip: fn(&Value) -> Value,
}

fn event_docs() -> Vec<Value> {
    vec![
        json!({ "kind": "started", "model": "m" }),
        json!({ "kind": "delta", "text": "hello" }),
        json!({ "kind": "tool-call", "name": "Read", "input": { "path": "a" } }),
        json!({ "kind": "tool-result", "name": "Read", "ok": true }),
        json!({ "kind": "turn-end", "text": "done" }),
        json!({ "kind": "exited", "status": 0, "usage": usage_doc(),
                "truncated": false, "error": "engine said no" }),
        json!({ "kind": "cancelled", "reason": "asked" }),
        json!({ "kind": "truncated", "limit-bytes": 32, "read-bytes": 746 }),
        // A kind from a NEWER peer: every field of it is unknown.
        json!({ "kind": "reasoning", "text": "thinking", "depth": 3 }),
    ]
}

fn usage_doc() -> Value {
    json!({ "input-tokens": 11, "output-tokens": 22, "cost-micro-usd": 33 })
}

fn tools_doc() -> Value {
    json!({ "mode": "allowlist", "allow": ["Read", "Grep"] })
}

fn budget_doc() -> Value {
    json!({ "wall-ms": 5_000, "output-bytes": 4_096 })
}

fn capabilities_doc() -> Value {
    json!({ "streaming": true, "tool-calls": false, "cancel": true,
            "usage": true, "external-cli": false })
}

fn request_docs() -> Vec<Value> {
    vec![json!({
        "api-version": API_VERSION, "engine": "default", "model": "m",
        "effort": "high", "prompt": "say ok", "cwd": "work",
        "tools": tools_doc(), "budget": budget_doc(),
        "secrets": { "ANTHROPIC_API_KEY": { "$secret": "engines/anthropic" } }
    })]
}

fn record_docs() -> Vec<Value> {
    vec![json!({
        "api-version": API_VERSION, "run-id": "default-1", "engine": "default",
        "model": "m", "state": "running", "events": event_docs(),
        "status": 0, "usage": usage_doc(), "text": "hello",
        "truncated": false, "error": "engine said no"
    })]
}

fn run_event_docs() -> Vec<Value> {
    event_docs()
        .into_iter()
        .map(|event| {
            let mut doc = json!({ "api-version": API_VERSION, "engine": "default",
                                  "run-id": "default-1", "seq": 4 });
            let object = doc.as_object_mut().expect("an object");
            for (key, value) in event.as_object().expect("an object") {
                object.insert(key.clone(), value.clone());
            }
            doc
        })
        .collect()
}

fn answer_docs() -> Vec<Value> {
    vec![
        json!({ "api-version": API_VERSION, "ok": { "run-id": "default-1" } }),
        json!({ "api-version": API_VERSION,
                "error": { "code": "refused", "message": "no" } }),
    ]
}

/// EVERY wire type this seam publishes. A type absent from this table is
/// a type nothing proves, so the table is the definition's own inventory
/// — [`the_table_covers_every_wire_type`] holds it to that.
fn wires() -> Vec<Wire> {
    vec![
        Wire {
            name: "ToolPolicy",
            canonical: || vec![tools_doc()],
            round_trip: round_trip::<ToolPolicy>,
        },
        Wire {
            name: "Budget",
            canonical: || vec![budget_doc()],
            round_trip: round_trip::<Budget>,
        },
        Wire {
            name: "Usage",
            canonical: || vec![usage_doc()],
            round_trip: round_trip::<Usage>,
        },
        Wire {
            name: "Capabilities",
            canonical: || vec![capabilities_doc()],
            round_trip: round_trip::<Capabilities>,
        },
        Wire {
            name: "RunRequest",
            canonical: request_docs,
            round_trip: round_trip::<RunRequest>,
        },
        Wire {
            name: "RunAccepted",
            canonical: || {
                vec![json!({ "api-version": API_VERSION,
                                          "run-id": "default-1",
                                          "engine": "default", "model": "m" })]
            },
            round_trip: round_trip::<RunAccepted>,
        },
        Wire {
            name: "CancelRequest",
            canonical: || vec![json!({ "run-id": "default-1" })],
            round_trip: round_trip::<CancelRequest>,
        },
        Wire {
            name: "Description",
            canonical: || {
                vec![json!({ "api-version": API_VERSION,
                                          "engine": "default", "provider": "p",
                                          "models": ["m"], "default-model": "m",
                                          "capabilities": capabilities_doc() })]
            },
            round_trip: round_trip::<Description>,
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
            name: "RunRecord",
            canonical: record_docs,
            round_trip: round_trip::<RunRecord>,
        },
        Wire {
            name: "EngineError",
            canonical: || vec![json!({ "code": "refused", "message": "no" })],
            round_trip: round_trip::<EngineError>,
        },
        Wire {
            name: "Answer",
            canonical: answer_docs,
            round_trip: round_trip::<Answer>,
        },
        Wire {
            name: "EngineSlot",
            canonical: || {
                vec![json!({ "engine": "default",
                                          "contract": "jinn:engine.default",
                                          "entry": "jinn-engine-echo" })]
            },
            round_trip: round_trip::<EngineSlot>,
        },
    ]
}

/// How many seeds each canonical document is sprinkled with. Every seed
/// is a different placement of unknown keys at different depths.
const SEEDS: u64 = 256;

#[test]
fn every_wire_type_preserves_unknown_content_at_every_depth() {
    for wire in wires() {
        for (index, canonical) in (wire.canonical)().into_iter().enumerate() {
            // The canonical document alone must survive: no seed, no
            // unknown key, nothing but the shape this version knows.
            assert_eq!(
                (wire.round_trip)(&canonical),
                canonical,
                "{} #{index}: a canonical document does not round-trip",
                wire.name
            );
            for seed in 0..SEEDS {
                let mut rng = Rng::new(seed ^ ((index as u64) << 32));
                let mut doc = canonical.clone();
                let mut planted = Vec::new();
                sprinkle(&mut doc, &mut rng, &mut planted, String::new());
                let round = (wire.round_trip)(&doc);
                assert_eq!(
                    round, doc,
                    "{} #{index} seed {seed}: unknown content was not preserved\n\
                     planted: {planted:?}",
                    wire.name
                );
            }
        }
    }
}

/// The `reasoning-tokens` probe the verifier wrote by hand is ONE sample
/// of the property above — the generator plants exactly this shape on its
/// own. It is kept as a named test because a regression here has a name
/// worth reading in a failure list.
#[test]
fn the_verifiers_own_probe_is_one_sample_of_the_property() {
    let doc = json!({
        "api-version": API_VERSION, "engine": "default", "kind": "delta",
        "run-id": "default-1", "reasoning-tokens": 7, "seq": 4, "text": "hello"
    });
    let round: Value = round_trip::<RunEvent>(&doc);
    assert_eq!(
        round, doc,
        "known event kinds must preserve additive fields"
    );
}

/// The named exception, held to its own promise. A CLOSED VALUE SPACE —
/// `effort`, `mode`, `state`, `code` — cannot preserve a value this
/// version cannot name: there is nowhere in an enum to put one, and
/// guessing which known value a future `effort: "ultra"` meant is the
/// silent-wrong-answer shape the seam forbids. So it REFUSES, loudly, and
/// the definition README names the surface. What must never happen is the
/// third possibility: decoding to a default and dropping the fact.
#[test]
fn a_value_a_closed_space_cannot_name_is_refused_and_never_defaulted() {
    let ahead = json!({
        "api-version": API_VERSION, "engine": "default", "prompt": "p",
        "effort": "ultra"
    });
    let refused = serde_json::from_value::<RunRequest>(ahead)
        .expect_err("a value this version cannot name is a decode error");
    assert!(
        refused.to_string().contains("ultra"),
        "the refusal names the value it could not read: {refused}"
    );
    // Not silently the nearest known one.
    let state = json!({ "api-version": API_VERSION, "run-id": "default-1",
                        "engine": "default", "state": "quiescing" });
    assert!(serde_json::from_value::<RunRecord>(state).is_err());
}
