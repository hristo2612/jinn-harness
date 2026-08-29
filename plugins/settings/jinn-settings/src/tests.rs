use serde_json::json;

use super::*;

fn schema() -> Schema {
    Schema {
        properties: [
            ("jobs".to_owned(), Field::required(Kind::Array)),
            ("tick-ms".to_owned(), Field::new(Kind::Integer)),
            ("notify-token".to_owned(), Field::new(Kind::SecretRef)),
        ]
        .into_iter()
        .collect(),
        additional: false,
        extra: Extensions::new(),
    }
}

fn declaration() -> Declaration {
    Declaration {
        namespace: "cron".into(),
        entry: "cron-scheduler".into(),
        schema: schema(),
        defaults: json!({ "tick-ms": 60000, "jobs": [] }),
        hot_keys: vec!["jobs".into(), "notify-token".into()],
        extra: Extensions::new(),
    }
}

fn layers() -> Layers {
    Layers {
        defaults: json!({ "tick-ms": 60000, "jobs": [] }),
        entry: json!({ "tick-ms": 500, "jobs": [{ "id": "a" }] }),
        overlay: json!({}),
        extra: Extensions::new(),
    }
}

#[test]
fn resolution_layers_defaults_under_entry_under_overlay() {
    let mut layers = layers();
    assert_eq!(
        resolve(&layers),
        json!({ "tick-ms": 500, "jobs": [{ "id": "a" }] })
    );
    layers.overlay = json!({ "jobs": [{ "id": "b" }] });
    assert_eq!(resolve(&layers)["jobs"], json!([{ "id": "b" }]));
    layers.overlay = json!({ "jobs": null });
    assert_eq!(resolve(&layers), json!({ "tick-ms": 500 }), "null removes");
    layers.entry = json!("not an object");
    assert_eq!(
        resolve(&Layers {
            overlay: json!({}),
            ..layers
        })["tick-ms"],
        60000,
        "a non-object layer contributes nothing"
    );
}

#[test]
fn the_validator_decides_membership_and_nothing_else() {
    let schema = schema();
    assert!(validate(&schema, &json!({ "jobs": [], "tick-ms": 5 })).is_ok());
    assert!(validate(&schema, &json!({ "tick-ms": 5 }))
        .unwrap_err()
        .contains("required"));
    assert!(validate(&schema, &json!({ "jobs": "x" }))
        .unwrap_err()
        .contains("Array"));
    assert!(validate(&schema, &json!({ "jobs": [], "tick-ms": -1 })).is_err());
    assert!(validate(&schema, &json!({ "jobs": [], "stray": 1 }))
        .unwrap_err()
        .contains("not a declared"));
    assert!(validate(&schema, &json!([])).is_err());
    let open = Schema {
        additional: true,
        ..schema
    };
    assert!(validate(&open, &json!({ "jobs": [], "stray": 1 })).is_ok());
}

#[test]
fn a_secret_reference_is_typed_and_a_bare_secret_is_refused() {
    assert!(is_secret_ref(&json!({ "$secret": "cron/notify" })));
    assert!(!is_secret_ref(&json!({ "$secret": "" })));
    assert!(!is_secret_ref(&json!({ "$secret": "k", "leak": "v" })));
    assert!(!is_secret_ref(&json!("hunter2")));
    let schema = schema();
    assert!(validate(
        &schema,
        &json!({ "jobs": [], "notify-token": { "$secret": "cron/notify" } })
    )
    .is_ok());
    let refused = validate(&schema, &json!({ "jobs": [], "notify-token": "hunter2" })).unwrap_err();
    assert!(refused.contains("holds no secret"), "{refused}");
    let wire: SecretRef = serde_json::from_value(json!({ "$secret": "k" })).expect("decodes");
    assert_eq!(wire.secret, "k");
}

#[test]
fn a_patch_is_planned_into_the_layer_its_keys_name() {
    let declaration = declaration();
    let layers = layers();
    let hot = plan_patch(&declaration, &layers, &json!({ "jobs": [{ "id": "b" }] })).expect("hot");
    assert_eq!(hot.applied, Applied::Hot);
    assert_eq!(hot.layer, json!({ "jobs": [{ "id": "b" }] }), "the overlay");
    assert_eq!(hot.resolved["tick-ms"], 500, "the rest untouched");
    let restart = plan_patch(&declaration, &layers, &json!({ "tick-ms": 250 })).expect("restart");
    assert_eq!(restart.applied, Applied::Restart);
    assert_eq!(
        restart.layer,
        json!({ "tick-ms": 250, "jobs": [{ "id": "a" }] }),
        "the entry layer, patched"
    );
    let mixed = plan_patch(
        &declaration,
        &layers,
        &json!({ "jobs": [], "tick-ms": 250 }),
    )
    .expect("mixed");
    assert_eq!(
        mixed.applied,
        Applied::Restart,
        "one cold key makes it a restart"
    );
    let empty = plan_patch(&declaration, &layers, &json!({})).expect("empty");
    assert_eq!(empty.applied, Applied::Restart, "an empty patch is not hot");
}

#[test]
fn a_patch_is_validated_as_a_whole_before_anything_applies() {
    let declaration = declaration();
    let layers = layers();
    let refused = plan_patch(&declaration, &layers, &json!({ "jobs": "nope" })).unwrap_err();
    assert_eq!(refused.code, ErrorCode::Invalid);
    assert!(refused.detail.contains("Array"), "{refused:?}");
    let secret =
        plan_patch(&declaration, &layers, &json!({ "notify-token": "hunter2" })).unwrap_err();
    assert!(secret.detail.contains("holds no secret"), "{secret:?}");
    let shape = plan_patch(&declaration, &layers, &json!([1])).unwrap_err();
    assert_eq!(shape.code, ErrorCode::Invalid);
    let removal = plan_patch(&declaration, &layers, &json!({ "jobs": null })).unwrap_err();
    assert!(
        removal.detail.contains("required"),
        "removing a required key: {removal:?}"
    );
}

#[test]
fn the_envelope_and_schemas_round_trip_additively() {
    let ok = Answer::ok(Resolved {
        api_version: API_VERSION.into(),
        namespace: "cron".into(),
        entry: "cron-scheduler".into(),
        settings: json!({ "jobs": [] }),
        layers: layers(),
        revision: 3,
        hot_keys: vec!["jobs".into()],
        extra: Extensions::new(),
    });
    assert_eq!(Answer::decode(&ok.encode()), ok);
    let error = Answer::error(SettingsError::new(ErrorCode::NotFound, "no namespace"));
    assert_eq!(Answer::decode(&error.encode()), error);
    assert!(matches!(
        Answer::decode(b"garbage").outcome,
        Outcome::Error(SettingsError {
            code: ErrorCode::Refused,
            ..
        })
    ));
    let wire = json!({ "namespace": "cron", "entry": "e",
                       "schema": { "properties": {}, "additional": false, "novel": 1 },
                       "defaults": {}, "hot-keys": [], "current": { "x": 1 }, "future": true });
    let request: DeclareRequest = serde_json::from_value(wire.clone()).expect("decodes");
    assert_eq!(request.declaration.schema.extra["novel"], 1);
    assert_eq!(request.declaration.extra["future"], true);
    assert_eq!(serde_json::to_value(&request).expect("encodes"), wire);
    let changed: Changed = serde_json::from_value(json!({ "namespace": "cron", "applied": "hot",
        "settings": {}, "revision": 1, "more": [] }))
    .expect("decodes");
    assert_eq!(changed.applied, Some(Applied::Hot));
    assert_eq!(changed.extra["more"], json!([]));
}
