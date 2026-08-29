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

#[test]
fn a_plan_reports_exactly_what_the_layers_resolve_to_afterwards() {
    // The consistency law: `resolved` is computed FROM the post-state
    // layers, never from "resolved ⊕ patch" — so a `null` in a hot patch
    // that a lower layer still defines is caught, not reported as gone.
    let declaration = declaration();
    let mut layers = layers();
    layers.entry = json!({ "tick-ms": 500, "jobs": [{ "id": "a" }],
                           "notify-token": { "$secret": "entry" } });
    layers.overlay = json!({ "notify-token": { "$secret": "overlay" } });
    let removal = plan_patch(&declaration, &layers, &json!({ "notify-token": null })).unwrap_err();
    assert_eq!(removal.code, ErrorCode::Invalid, "{removal:?}");
    assert_eq!(
        removal.shadowed,
        Some(Shadowed {
            key: "notify-token".into(),
            layer: LayerName::Entry,
        }),
        "{removal:?}"
    );
    let hot = plan_patch(&declaration, &layers, &json!({ "jobs": [{ "id": "b" }] })).expect("hot");
    assert_eq!(
        hot.resolved,
        resolve(&Layers {
            overlay: hot.layer.clone(),
            ..layers.clone()
        }),
        "the reported settings ARE the post-state resolution"
    );
}

#[test]
fn a_mixed_patch_over_an_existing_overlay_is_refused_whole_as_shadowed() {
    // The verifier's probe (PLA-314 round 1): an overlay holds `jobs`;
    // a mixed hot+cold patch would land whole in the entry, where the
    // overlay's `jobs` shadows it on the next resolve. Refused whole,
    // typed: nothing to apply, the event never lies.
    let declaration = declaration();
    let mut layers = layers();
    layers.overlay = json!({ "jobs": [{ "id": "overlay" }] });
    let refused = plan_patch(
        &declaration,
        &layers,
        &json!({ "jobs": [{ "id": "requested" }], "tick-ms": 250 }),
    )
    .unwrap_err();
    assert_eq!(refused.code, ErrorCode::Invalid, "{refused:?}");
    assert_eq!(
        refused.shadowed,
        Some(Shadowed {
            key: "jobs".into(),
            layer: LayerName::Overlay,
        }),
        "{refused:?}"
    );
    assert!(refused.detail.contains("shadowed"), "{refused:?}");
    let wire = serde_json::to_value(&refused).expect("encodes");
    assert_eq!(
        wire["shadowed"],
        json!({ "key": "jobs", "layer": "overlay" })
    );
    assert_eq!(
        serde_json::from_value::<SettingsError>(wire).expect("decodes"),
        refused
    );
    // A cold-only patch beside the overlay is fine: nothing it touches is
    // shadowed, and it reports the overlay's `jobs` — what GET resolves.
    let cold = plan_patch(&declaration, &layers, &json!({ "tick-ms": 250 })).expect("cold");
    assert_eq!(cold.applied, Applied::Restart);
    assert_eq!(cold.resolved["jobs"], json!([{ "id": "overlay" }]));
    assert_eq!(cold.resolved["tick-ms"], 250);
    // A shadowed error without the field still decodes (additive).
    let bare: SettingsError =
        serde_json::from_value(json!({ "code": "invalid", "detail": "x" })).expect("decodes");
    assert_eq!(bare.shadowed, None);
}

#[test]
fn the_inverse_order_is_consistent_without_a_refusal() {
    // Mixed patch FIRST (no overlay yet): lands whole in the entry, and
    // the report equals the post-state resolution. A hot patch after it
    // lands in the overlay and the report equals that resolution too.
    let declaration = declaration();
    let layers = layers();
    let mixed = plan_patch(
        &declaration,
        &layers,
        &json!({ "jobs": [{ "id": "requested" }], "tick-ms": 250 }),
    )
    .expect("mixed, no overlay");
    assert_eq!(mixed.applied, Applied::Restart);
    let after_mixed = Layers {
        entry: mixed.layer.clone(),
        ..layers
    };
    assert_eq!(mixed.resolved, resolve(&after_mixed));
    assert_eq!(mixed.resolved["jobs"], json!([{ "id": "requested" }]));
    let hot = plan_patch(
        &declaration,
        &after_mixed,
        &json!({ "jobs": [{ "id": "hot" }] }),
    )
    .expect("hot after mixed");
    assert_eq!(hot.applied, Applied::Hot);
    let after_hot = Layers {
        overlay: hot.layer.clone(),
        ..after_mixed
    };
    assert_eq!(hot.resolved, resolve(&after_hot));
    assert_eq!(hot.resolved["jobs"], json!([{ "id": "hot" }]));
    assert_eq!(hot.resolved["tick-ms"], 250);
}
