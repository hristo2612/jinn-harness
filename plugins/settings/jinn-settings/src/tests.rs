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

/// The closed surface REFUSES on decode — it does not quietly drop the
/// sibling and answer a well-formed reference. Consumers of this type in
/// other seams inherit exactly this (`closed`, and the engines seam's
/// `a_secret_reference_carrying_a_sibling_is_refused_naming_the_surface`).
#[test]
fn a_sibling_inside_a_secret_reference_is_refused_naming_the_surface() {
    let refused = serde_json::from_value::<SecretRef>(json!({ "$secret": "k", "scope": "eu" }))
        .expect_err("a closed surface refuses");
    let said = refused.to_string();
    assert!(said.contains("$secret"), "{said}");
    assert!(said.contains("scope"), "{said}");
    assert!(said.contains("closed"), "{said}");
    // A reference that names nothing is still the validator's call, not
    // the decoder's: the shape is well formed.
    assert!(serde_json::from_value::<SecretRef>(json!({ "$secret": "" })).is_ok());
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
            path: vec!["notify-token".into()],
            layer: LayerName::Entry,
            recovery: Some(Box::new(Recovery {
                namespace: "cron".into(),
                patch: json!({ "notify-token": null }),
                layer: PatchLayer::Entry,
            })),
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
            path: vec!["jobs".into()],
            layer: LayerName::Overlay,
            recovery: Some(Box::new(Recovery {
                namespace: "cron".into(),
                patch: json!({ "jobs": null }),
                layer: PatchLayer::Overlay,
            })),
        }),
        "{refused:?}"
    );
    assert!(refused.detail.contains("shadowed"), "{refused:?}");
    let wire = serde_json::to_value(&refused).expect("encodes");
    assert_eq!(wire["shadowed"]["key"], "jobs");
    assert_eq!(wire["shadowed"]["layer"], "overlay");
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

#[test]
fn a_shadowed_refusal_names_a_recovery_that_succeeds() {
    // The verifier's scenario (PLA-314 round 2): the entry AND the
    // overlay hold `notify-token`; `{notify-token: null}` is hot, lands in
    // the overlay, and the entry would still resolve it. The refusal
    // must name the exact call that clears the shadowing layer — and
    // executing it, then retrying, must land with the key gone.
    let declaration = declaration();
    let mut layers = layers();
    layers.entry = json!({ "tick-ms": 500, "jobs": [{ "id": "a" }],
                           "notify-token": { "$secret": "entry" } });
    layers.overlay = json!({ "notify-token": { "$secret": "overlay" } });
    let removal = json!({ "notify-token": null });
    let refused = plan_patch(&declaration, &layers, &removal).unwrap_err();
    let shadowed = refused.shadowed.clone().expect("typed");
    assert_eq!(shadowed.layer, LayerName::Entry);
    let recovery = shadowed
        .recovery
        .expect("a recovery the operator can execute");
    assert_eq!(recovery.namespace, "cron");
    assert_eq!(recovery.patch, removal);
    assert_eq!(recovery.layer, PatchLayer::Entry);
    assert!(
        refused
            .detail
            .contains(r#"patch("cron", {"notify-token":null}, layer: entry)"#),
        "the advice names the exact call: {}",
        refused.detail
    );
    assert!(refused.detail.contains("then retry"), "{}", refused.detail);
    // Execute the advised call: an explicit-layer removal clears THAT
    // layer, honestly reporting what still resolves (the overlay's).
    let cleared = plan_patch_in(&declaration, &layers, &recovery.patch, Some(recovery.layer))
        .expect("the advised recovery succeeds");
    assert_eq!(cleared.applied, Applied::Restart);
    assert!(
        cleared.layer.get("notify-token").is_none(),
        "{:?}",
        cleared.layer
    );
    assert_eq!(
        cleared.resolved["notify-token"],
        json!({ "$secret": "overlay" }),
        "the report is the post-state resolution"
    );
    let after = Layers {
        entry: cleared.layer.clone(),
        ..layers
    };
    assert_eq!(cleared.resolved, resolve(&after));
    // Retry the original patch: nothing shadows it now.
    let retried = plan_patch(&declaration, &after, &removal).expect("the retry lands");
    assert_eq!(retried.applied, Applied::Hot);
    assert!(
        retried.resolved.get("notify-token").is_none(),
        "{:?}",
        retried.resolved
    );
    let wire = serde_json::to_value(&refused).expect("encodes");
    assert_eq!(
        wire["shadowed"]["recovery"],
        json!({ "namespace": "cron", "patch": { "notify-token": null }, "layer": "entry" })
    );
    assert_eq!(
        serde_json::from_value::<SettingsError>(wire).expect("decodes"),
        refused
    );
    // A round-2 reader's `shadowed` without a recovery still decodes.
    let bare: Shadowed =
        serde_json::from_value(json!({ "key": "k", "layer": "overlay" })).expect("decodes");
    assert_eq!(bare.recovery, None);
}

#[test]
fn the_round_one_probe_recovers_through_the_overlay() {
    let declaration = declaration();
    let mut layers = layers();
    layers.overlay = json!({ "jobs": [{ "id": "overlay" }] });
    let mixed = json!({ "jobs": [{ "id": "requested" }], "tick-ms": 250 });
    let refused = plan_patch(&declaration, &layers, &mixed).unwrap_err();
    let recovery = refused.shadowed.expect("typed").recovery.expect("recovery");
    assert_eq!(recovery.layer, PatchLayer::Overlay);
    assert_eq!(recovery.patch, json!({ "jobs": null }));
    let cleared = plan_patch_in(&declaration, &layers, &recovery.patch, Some(recovery.layer))
        .expect("clearing the overlay succeeds");
    assert_eq!(cleared.applied, Applied::Hot);
    assert_eq!(cleared.layer, json!({}));
    assert_eq!(
        cleared.resolved["jobs"],
        json!([{ "id": "a" }]),
        "the entry's, honestly"
    );
    let after = Layers {
        overlay: cleared.layer.clone(),
        ..layers
    };
    let retried = plan_patch(&declaration, &after, &mixed).expect("the retry lands");
    assert_eq!(retried.applied, Applied::Restart);
    assert_eq!(retried.resolved["jobs"], json!([{ "id": "requested" }]));
    assert_eq!(retried.resolved["tick-ms"], 250);
}

#[test]
fn an_explicit_layer_addresses_that_layer_and_nothing_else() {
    let declaration = declaration();
    let layers = layers();
    // A hot key addressed to the entry lands there (restart path).
    let entry = plan_patch_in(
        &declaration,
        &layers,
        &json!({ "jobs": [{ "id": "b" }] }),
        Some(PatchLayer::Entry),
    )
    .expect("entry");
    assert_eq!(entry.applied, Applied::Restart);
    assert_eq!(entry.layer["jobs"], json!([{ "id": "b" }]));
    // A cold key cannot be SET in the overlay: the owner plans its
    // activation on the entry layer alone and would never honor it.
    let cold = plan_patch_in(
        &declaration,
        &layers,
        &json!({ "tick-ms": 250 }),
        Some(PatchLayer::Overlay),
    )
    .unwrap_err();
    assert_eq!(cold.code, ErrorCode::Invalid);
    assert!(cold.detail.contains("hot key"), "{}", cold.detail);
    // Clearing any key from the overlay is always admitted.
    let cleared = plan_patch_in(
        &declaration,
        &layers,
        &json!({ "tick-ms": null }),
        Some(PatchLayer::Overlay),
    )
    .expect("a removal clears the named layer");
    assert_eq!(cleared.applied, Applied::Hot);
    // A SET addressed to the entry that the overlay shadows is refused
    // with the overlay-clearing recovery — never a silent no-op.
    let mut shadowing = layers.clone();
    shadowing.overlay = json!({ "jobs": [{ "id": "overlay" }] });
    let refused = plan_patch_in(
        &declaration,
        &shadowing,
        &json!({ "jobs": [{ "id": "requested" }] }),
        Some(PatchLayer::Entry),
    )
    .unwrap_err();
    let shadowed = refused.shadowed.expect("typed");
    assert_eq!(shadowed.layer, LayerName::Overlay);
    assert_eq!(
        shadowed.recovery.expect("recovery").layer,
        PatchLayer::Overlay
    );
    // The default layer is the keys' choice, unchanged.
    assert_eq!(
        plan_patch_in(&declaration, &layers, &json!({ "jobs": [] }), None)
            .expect("default")
            .applied,
        Applied::Hot
    );
    // A removal of a key only the defaults define has no executable
    // recovery: defaults are the owner's declaration.
    let default_only = plan_patch(&declaration, &layers, &json!({ "tick-ms": null }));
    let mut no_entry = layers.clone();
    no_entry.entry = json!({ "jobs": [{ "id": "a" }] });
    let refused = plan_patch(&declaration, &no_entry, &json!({ "tick-ms": null })).unwrap_err();
    let shadowed = refused.shadowed.expect("typed");
    assert_eq!(shadowed.layer, LayerName::Defaults);
    assert_eq!(shadowed.recovery, None);
    assert!(refused.detail.contains("default"), "{}", refused.detail);
    drop(default_only);
    // The wire selector: `layer` on the request, kebab-case, optional.
    let request: PatchRequest =
        serde_json::from_value(json!({ "namespace": "cron", "patch": {}, "layer": "overlay" }))
            .expect("decodes");
    assert_eq!(request.layer, Some(PatchLayer::Overlay));
    let bare: PatchRequest =
        serde_json::from_value(json!({ "namespace": "cron", "patch": {} })).expect("decodes");
    assert_eq!(bare.layer, None);
    assert!(
        serde_json::from_value::<PatchRequest>(json!({ "namespace": "cron", "layer": "defaults" }))
            .is_err(),
        "defaults are not addressable"
    );
}

/// A namespace with an object-valued hot key, for the leaf-path cases.
fn nested_declaration() -> Declaration {
    Declaration {
        namespace: "nested".into(),
        entry: "nested-owner".into(),
        schema: Schema {
            properties: [
                ("cold".to_owned(), Field::new(Kind::Bool)),
                ("group".to_owned(), Field::new(Kind::Object)),
            ]
            .into_iter()
            .collect(),
            additional: false,
            extra: Extensions::new(),
        },
        defaults: json!({}),
        hot_keys: vec!["group".into()],
        extra: Extensions::new(),
    }
}

/// The nested declaration with an open schema, for cases whose layers
/// hold an atomic where the typed schema wants an object.
fn open_declaration() -> Declaration {
    Declaration {
        schema: Schema {
            properties: Default::default(),
            additional: true,
            extra: Extensions::new(),
        },
        ..nested_declaration()
    }
}

#[test]
fn a_nested_shadowed_recovery_removes_only_the_shadowed_leaf() {
    // The verifier's probe (PLA-314 round 3): the overlay holds
    // `group.changed` beside an untouched sibling; a mixed patch touching
    // only `group.changed` (and a cold key) lands in the entry, and the
    // overlay would still resolve the leaf. The refusal must name the
    // LEAF PATH, and its recovery must remove that leaf alone — the
    // overlay's `untouched` sibling survives, and the retry resolves
    // the requested value.
    let declaration = nested_declaration();
    let layers = Layers {
        defaults: json!({}),
        entry: json!({ "cold": false, "group": { "changed": 1, "untouched": "entry" } }),
        overlay: json!({ "group": { "changed": 5, "untouched": "overlay" } }),
        extra: Extensions::new(),
    };
    let mixed = json!({ "cold": true, "group": { "changed": 9 } });
    let refused = plan_patch(&declaration, &layers, &mixed).unwrap_err();
    assert_eq!(refused.code, ErrorCode::Invalid);
    let shadowed = refused.shadowed.clone().expect("typed");
    assert_eq!(shadowed.key, "group.changed", "the exact leaf path");
    assert_eq!(shadowed.path, vec!["group", "changed"]);
    assert_eq!(shadowed.layer, LayerName::Overlay);
    let recovery = shadowed.recovery.expect("recovery");
    assert_eq!(recovery.layer, PatchLayer::Overlay);
    assert_eq!(
        recovery.patch,
        json!({ "group": { "changed": null } }),
        "path-precise: a null at the leaf, never the top-level key"
    );
    assert!(
        refused
            .detail
            .contains(r#""group.changed" is shadowed by the overlay layer"#)
            && refused.detail.contains(
                r#"patch("nested", {"group":{"changed":null}}, layer: overlay), then retry"#
            ),
        "{}",
        refused.detail
    );
    // Execute the advised recovery: only the leaf leaves the overlay.
    let cleared = plan_patch_in(&declaration, &layers, &recovery.patch, Some(recovery.layer))
        .expect("the advised recovery succeeds");
    assert_eq!(cleared.applied, Applied::Hot);
    assert_eq!(
        cleared.layer,
        json!({ "group": { "untouched": "overlay" } }),
        "the untouched overlay sibling survives"
    );
    assert_eq!(
        cleared.resolved,
        json!({ "cold": false, "group": { "changed": 1, "untouched": "overlay" } }),
        "honest: the entry's leaf resolves now"
    );
    let after = Layers {
        overlay: cleared.layer.clone(),
        ..layers
    };
    assert_eq!(cleared.resolved, resolve(&after));
    // Retry: it lands whole in the entry and resolves the requested value.
    let retried = plan_patch(&declaration, &after, &mixed).expect("the retry lands");
    assert_eq!(retried.applied, Applied::Restart);
    assert_eq!(
        retried.resolved,
        json!({ "cold": true, "group": { "changed": 9, "untouched": "overlay" } })
    );
    assert_eq!(
        retried.resolved["group"]["untouched"], "overlay",
        "recovery must preserve the untouched overlay sibling"
    );
    // On the wire: the dotted leaf path and its structured form, both.
    let wire = serde_json::to_value(&refused).expect("encodes");
    assert_eq!(wire["shadowed"]["key"], "group.changed");
    assert_eq!(wire["shadowed"]["path"], json!(["group", "changed"]));
    assert_eq!(
        wire["shadowed"]["recovery"]["patch"],
        json!({ "group": { "changed": null } })
    );
    // A round-3 reader's `shadowed` without `path` still decodes.
    let bare: Shadowed =
        serde_json::from_value(json!({ "key": "k", "layer": "overlay" })).expect("decodes");
    assert_eq!(bare.path, Vec::<String>::new());
}

#[test]
fn a_two_level_deep_shadowed_leaf_is_named_and_cleared_exactly() {
    let declaration = nested_declaration();
    let layers = Layers {
        defaults: json!({}),
        entry: json!({ "cold": false,
                       "group": { "inner": { "changed": 1, "keep": "entry" },
                                  "untouched": "entry" } }),
        overlay: json!({ "group": { "inner": { "changed": 5, "keep": "overlay" },
                                    "untouched": "overlay" } }),
        extra: Extensions::new(),
    };
    let mixed = json!({ "cold": true, "group": { "inner": { "changed": 9 } } });
    let refused = plan_patch(&declaration, &layers, &mixed).unwrap_err();
    let shadowed = refused.shadowed.expect("typed");
    assert_eq!(shadowed.key, "group.inner.changed");
    assert_eq!(shadowed.path, vec!["group", "inner", "changed"]);
    assert_eq!(shadowed.layer, LayerName::Overlay);
    let recovery = shadowed.recovery.expect("recovery");
    assert_eq!(
        recovery.patch,
        json!({ "group": { "inner": { "changed": null } } })
    );
    let cleared = plan_patch_in(&declaration, &layers, &recovery.patch, Some(recovery.layer))
        .expect("the advised recovery succeeds");
    assert_eq!(
        cleared.layer,
        json!({ "group": { "inner": { "keep": "overlay" }, "untouched": "overlay" } }),
        "every sibling at every level survives"
    );
    let after = Layers {
        overlay: cleared.layer.clone(),
        ..layers
    };
    let retried = plan_patch(&declaration, &after, &mixed).expect("the retry lands");
    assert_eq!(
        retried.resolved,
        json!({ "cold": true,
                "group": { "inner": { "changed": 9, "keep": "overlay" },
                           "untouched": "overlay" } })
    );
}

#[test]
fn an_atomic_ancestor_names_the_ancestor_and_its_layer() {
    // The verifier's probe (PLA-314 round 4): the overlay holds an ATOMIC
    // at `group.inner` beside an untouched sibling; an explicit entry
    // patch sets the leaf `group.inner.changed` below it. The overlay's
    // atomic ancestor is what resolves that leaf (absent), so the refusal
    // names the ANCESTOR in the OVERLAY — never the leaf, never the entry
    // — and its recovery removes exactly that node; the retry lands.
    let declaration = nested_declaration();
    let layers = Layers {
        defaults: json!({}),
        entry: json!({ "cold": false, "group": { "untouched": "entry" } }),
        overlay: json!({ "group": { "inner": 5, "untouched": "overlay" } }),
        extra: Extensions::new(),
    };
    let patch = json!({ "group": { "inner": { "changed": 9 } } });
    let refused =
        plan_patch_in(&declaration, &layers, &patch, Some(PatchLayer::Entry)).unwrap_err();
    let shadowed = refused.shadowed.clone().expect("typed");
    assert_eq!(
        shadowed.key, "group.inner",
        "the atomic ancestor, not the leaf"
    );
    assert_eq!(shadowed.path, vec!["group", "inner"]);
    assert_eq!(shadowed.layer, LayerName::Overlay, "{refused:?}");
    let recovery = shadowed.recovery.expect("recovery");
    assert_eq!(recovery.layer, PatchLayer::Overlay);
    assert_eq!(recovery.patch, json!({ "group": { "inner": null } }));
    let cleared = plan_patch_in(&declaration, &layers, &recovery.patch, Some(recovery.layer))
        .expect("the advised recovery succeeds");
    assert_eq!(
        cleared.layer,
        json!({ "group": { "untouched": "overlay" } })
    );
    let after = Layers {
        overlay: cleared.layer.clone(),
        ..layers
    };
    let retried = plan_patch_in(&declaration, &after, &patch, Some(PatchLayer::Entry))
        .expect("the advertised recovery must make the retry land");
    assert_eq!(
        retried.resolved,
        json!({ "cold": false,
                "group": { "inner": { "changed": 9 }, "untouched": "overlay" } })
    );
}

#[test]
fn an_empty_object_is_an_object_valued_leaf_that_sets_the_key() {
    // The verifier's probe (PLA-314 round 5): the overlay holds an ATOMIC
    // `group: 5` beside an untouched sibling; a mixed cold+hot patch sets
    // `group` to `{}` — under RFC 7396 an object replaces an atomic, so
    // the requested resolution holds `group: {}`. The patch lands in the
    // entry (a cold key chooses it), where the overlay's atomic still
    // wins: refused, naming `group` in the overlay; the recovery removes
    // exactly that node; the retry lands with `group: {}` and the
    // overlay's sibling intact.
    let declaration = open_declaration();
    let layers = Layers {
        defaults: json!({}),
        entry: json!({ "cold": false }),
        overlay: json!({ "group": 5, "untouched": "overlay" }),
        extra: Extensions::new(),
    };
    let patch = json!({ "cold": true, "group": {} });
    let refused = plan_patch(&declaration, &layers, &patch).unwrap_err();
    let shadowed = refused.shadowed.clone().expect("typed");
    assert_eq!(shadowed.key, "group", "{refused:?}");
    assert_eq!(shadowed.layer, LayerName::Overlay);
    let recovery = shadowed.recovery.expect("recovery");
    assert_eq!(recovery.layer, PatchLayer::Overlay);
    assert_eq!(recovery.patch, json!({ "group": null }));
    let cleared = plan_patch_in(&declaration, &layers, &recovery.patch, Some(recovery.layer))
        .expect("the advised recovery succeeds");
    assert_eq!(cleared.layer, json!({ "untouched": "overlay" }));
    let after = Layers {
        overlay: cleared.layer.clone(),
        ..layers
    };
    let retried = plan_patch(&declaration, &after, &patch)
        .expect("the advertised recovery must make the retry land");
    assert_eq!(
        retried.resolved,
        json!({ "cold": true, "group": {}, "untouched": "overlay" })
    );
    // Over an existing object, `{}` asks for nothing new (RFC 7396 never
    // removes children) and lands as a no-op.
    let held = Layers {
        entry: json!({ "cold": false, "group": { "x": 1 } }),
        overlay: json!({}),
        ..after
    };
    let noop = plan_patch(&declaration, &held, &json!({ "group": {} })).expect("lands");
    assert_eq!(noop.resolved, json!({ "cold": false, "group": { "x": 1 } }));
}

#[test]
fn an_object_over_an_atomic_replaces_the_whole_subtree() {
    // RFC 7396 in a layered document: the overlay's atomic `group: 5`
    // wiped the entry's `group.x`; a hot patch laying an object over that
    // atomic asks for exactly the requested subtree (`group: { h: 7 }`),
    // but writing the object into the overlay un-wipes the entry, whose
    // `group.x` would leak into the next get. Refused, naming the leaking
    // leaf in the ENTRY; the path-precise recovery removes it alone (the
    // entry's sibling `cold` survives); the retry lands.
    let declaration = open_declaration();
    let layers = Layers {
        defaults: json!({}),
        entry: json!({ "cold": false, "group": { "x": 1 } }),
        overlay: json!({ "group": 5 }),
        extra: Extensions::new(),
    };
    let patch = json!({ "group": { "h": 7 } });
    let refused = plan_patch(&declaration, &layers, &patch).unwrap_err();
    let shadowed = refused.shadowed.clone().expect("typed");
    assert_eq!(shadowed.key, "group.x", "{refused:?}");
    assert_eq!(shadowed.layer, LayerName::Entry);
    let recovery = shadowed.recovery.expect("recovery");
    assert_eq!(recovery.layer, PatchLayer::Entry);
    assert_eq!(recovery.patch, json!({ "group": { "x": null } }));
    let cleared = plan_patch_in(&declaration, &layers, &recovery.patch, Some(recovery.layer))
        .expect("the advised recovery succeeds");
    assert_eq!(cleared.layer, json!({ "cold": false, "group": {} }));
    let after = Layers {
        entry: cleared.layer.clone(),
        ..layers
    };
    let retried = plan_patch(&declaration, &after, &patch)
        .expect("the advertised recovery must make the retry land");
    assert_eq!(retried.applied, Applied::Hot);
    assert_eq!(
        retried.resolved,
        json!({ "cold": false, "group": { "h": 7 } })
    );
}

#[test]
fn an_atomic_over_an_object_replaces_the_whole_subtree() {
    // The mirror: an explicit entry patch sets `group: 7` where the
    // overlay holds an object — the requested resolution is the atomic,
    // the overlay's object would still merge over it. Refused, naming the
    // overlay's node; removing it makes the retry land as asked.
    let declaration = open_declaration();
    let layers = Layers {
        defaults: json!({}),
        entry: json!({ "cold": false }),
        overlay: json!({ "group": { "y": 2 } }),
        extra: Extensions::new(),
    };
    let patch = json!({ "group": 7 });
    let refused =
        plan_patch_in(&declaration, &layers, &patch, Some(PatchLayer::Entry)).unwrap_err();
    let shadowed = refused.shadowed.clone().expect("typed");
    assert_eq!(shadowed.key, "group", "{refused:?}");
    assert_eq!(shadowed.layer, LayerName::Overlay);
    let recovery = shadowed.recovery.expect("recovery");
    assert_eq!(recovery.patch, json!({ "group": null }));
    let cleared = plan_patch_in(&declaration, &layers, &recovery.patch, Some(recovery.layer))
        .expect("the advised recovery succeeds");
    let after = Layers {
        overlay: cleared.layer.clone(),
        ..layers
    };
    let retried = plan_patch_in(&declaration, &after, &patch, Some(PatchLayer::Entry))
        .expect("the advertised recovery must make the retry land");
    assert_eq!(retried.resolved, json!({ "cold": false, "group": 7 }));
}

/// A tiny deterministic generator (xorshift64*) so the property run is
/// reproducible from its seed and depends on nothing but this crate.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn key(&mut self) -> String {
        ["a", "b", "c", "d"][self.below(4) as usize].to_owned()
    }

    fn atomic(&mut self) -> serde_json::Value {
        match self.below(4) {
            0 => json!(self.below(100)),
            1 => json!(format!("s{}", self.below(100))),
            2 => json!([self.below(10)]),
            _ => json!(self.below(2) == 1),
        }
    }

    /// A random value from the WHOLE RFC 7396 domain at this depth: an
    /// atomic, `null`, an empty object, or (below depth 3) a nested
    /// object of zero or more such values — so atomics land over objects,
    /// objects over atomics, `{}` and `null` at every depth.
    fn value(&mut self, depth: u32) -> serde_json::Value {
        match self.below(6) {
            0 => serde_json::Value::Null,
            1 => json!({}),
            2 if depth < 3 => self.tree(depth + 1),
            _ => self.atomic(),
        }
    }

    /// A random object tree, depth ≤ 3, any domain value at any depth
    /// (a layer holds whatever a document may hold: a `null` there is an
    /// atomic that removes what lies below it).
    fn tree(&mut self, depth: u32) -> serde_json::Value {
        let mut object = serde_json::Map::new();
        for _ in 0..self.below(4) {
            let value = self.value(depth);
            object.insert(self.key(), value);
        }
        serde_json::Value::Object(object)
    }

    /// A random merge patch: the same domain (zero or more fields; a
    /// field may be `{}`, `null`, an atomic or a nested patch).
    fn patch(&mut self, depth: u32) -> serde_json::Value {
        self.tree(depth)
    }
}

/// Every leaf path of a tree (an atomic, or an empty object).
fn leaf_paths(value: &serde_json::Value, prefix: &[String], out: &mut Vec<Vec<String>>) {
    match value.as_object() {
        Some(fields) if !fields.is_empty() => {
            for (key, value) in fields {
                leaf_paths(value, &[prefix, std::slice::from_ref(key)].concat(), out);
            }
        }
        _ => out.push(prefix.to_vec()),
    }
}

/// The (path, wanted) pairs a merge patch asks for: `Some(v)` a set
/// (an empty object is an object-valued leaf, a set), `None` a removal.
fn asks(
    patch: &serde_json::Value,
    prefix: &[String],
    out: &mut Vec<(Vec<String>, Option<serde_json::Value>)>,
) {
    for (key, value) in patch.as_object().expect("an object") {
        let path = [prefix, std::slice::from_ref(key)].concat();
        match value {
            serde_json::Value::Null => out.push((path, None)),
            serde_json::Value::Object(fields) if !fields.is_empty() => asks(value, &path, out),
            _ => out.push((path, Some(value.clone()))),
        }
    }
}

fn related(a: &[String], b: &[String]) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

/// The requested resolution: RFC 7396 applied to the document as it
/// resolves — an atomic replaces a subtree, an object replaces an
/// atomic, `{}` over an object asks nothing new, `null` removes.
fn requested(
    state: &Layers,
    patch: &serde_json::Value,
    explicit: bool,
) -> (serde_json::Value, Vec<Vec<String>>) {
    let (asked, cleared) = asking(patch, explicit);
    let mut requested = resolve(state);
    merge_patch(&mut requested, &asked);
    (requested, cleared)
}

/// (b): under every key the patch names, the resolution after `plan` is
/// leaf-for-leaf the requested resolution (an explicit-layer removal
/// asks nothing: the operator is clearing that layer).
fn assert_asked(
    plan: &PatchPlan,
    state: &Layers,
    patch: &serde_json::Value,
    explicit: bool,
    case: &str,
) {
    let (requested, cleared) = requested(state, patch, explicit);
    for key in patch.as_object().expect("an object").keys() {
        let key = vec![key.clone()];
        let mut paths = Vec::new();
        leaf_paths(
            value_at(&requested, &key).unwrap_or(&json!(null)),
            &key,
            &mut paths,
        );
        leaf_paths(
            value_at(&plan.resolved, &key).unwrap_or(&json!(null)),
            &key,
            &mut paths,
        );
        for path in paths {
            if cleared.iter().any(|node| path.starts_with(node)) {
                continue;
            }
            let (want, got) = (value_at(&requested, &path), value_at(&plan.resolved, &path));
            assert_eq!(got, want, "{path:?} must resolve as requested\n{case}");
        }
    }
}

/// (c): every path neither the patch nor the recovery addressed is
/// byte-identical in `before` and `after`.
fn assert_untouched(
    before: &serde_json::Value,
    after: &serde_json::Value,
    addressed: &[Vec<String>],
    case: &str,
) {
    let mut paths = Vec::new();
    leaf_paths(before, &[], &mut paths);
    leaf_paths(after, &[], &mut paths);
    for path in paths {
        if addressed.iter().any(|node| related(node, &path)) {
            continue;
        }
        assert_eq!(
            value_at(before, &path),
            value_at(after, &path),
            "unaddressed {path:?} changed\n{case}"
        );
    }
}

fn apply(layers: &Layers, plan: &PatchPlan) -> Layers {
    match plan.applied {
        Applied::Hot => Layers {
            overlay: plan.layer.clone(),
            ..layers.clone()
        },
        Applied::Restart => Layers {
            entry: plan.layer.clone(),
            ..layers.clone()
        },
    }
}

#[test]
fn shadowing_is_one_definition_over_random_two_layer_trees() {
    // The formal definition (PLA-314 round 5), proven as a property over
    // random two-layer trees, random merge patches and a random target
    // layer: (a) refused ⇒ the advertised recovery, then the retry, lands
    // and resolves what was asked; (b) not refused ⇒ it resolves what was
    // asked; (c) every path neither the patch nor the recovery addressed
    // is byte-identical in both layers afterwards.
    let open = Schema {
        properties: Default::default(),
        additional: true,
        extra: Extensions::new(),
    };
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let (mut landed, mut recovered) = (0, 0);
    for case_no in 0..10_000 {
        let hot_keys: Vec<String> = ["a", "b", "c", "d"]
            .iter()
            .filter(|_| rng.below(2) == 1)
            .map(|k| (*k).to_owned())
            .collect();
        let declaration = Declaration {
            namespace: "prop".into(),
            entry: "owner".into(),
            schema: open.clone(),
            defaults: json!({}),
            hot_keys,
            extra: Extensions::new(),
        };
        let layers = Layers {
            defaults: json!({}),
            entry: rng.tree(1),
            overlay: rng.tree(1),
            extra: Extensions::new(),
        };
        let patch = rng.patch(1);
        let target = match rng.below(3) {
            0 => None,
            1 => Some(PatchLayer::Entry),
            _ => Some(PatchLayer::Overlay),
        };
        let case = format!(
            "case {case_no}: hot={:?} entry={} overlay={} patch={patch} target={target:?}",
            declaration.hot_keys, layers.entry, layers.overlay
        );
        let mut addressed = Vec::new();
        let mut asked = Vec::new();
        asks(&patch, &[], &mut asked);
        addressed.extend(asked.into_iter().map(|(path, _)| path));
        let (state, plan) = match plan_patch_in(&declaration, &layers, &patch, target) {
            Ok(plan) => {
                landed += 1;
                (layers.clone(), plan)
            }
            Err(refused) => {
                let Some(shadowed) = refused.shadowed.clone() else {
                    assert!(refused.detail.contains("not a hot key"), "only the overlay's cold-key rule may refuse otherwise\n{case}\n{refused:?}");
                    continue;
                };
                let recovery = shadowed.recovery.clone().unwrap_or_else(|| {
                    panic!("two layers, no defaults: every refusal recovers\n{case}\n{refused:?}")
                });
                let mut nodes = Vec::new();
                asks(&recovery.patch, &[], &mut nodes);
                assert!(
                    nodes.iter().all(|(_, want)| want.is_none()),
                    "a recovery only removes\n{case}\n{refused:?}"
                );
                assert!(
                    nodes.iter().any(|(node, _)| *node == shadowed.path),
                    "the named node is in the recovery\n{case}\n{refused:?}"
                );
                assert_eq!(shadowed.layer, recovery.layer.name(), "{case}\n{refused:?}");
                let cleared =
                    plan_patch_in(&declaration, &layers, &recovery.patch, Some(recovery.layer))
                        .unwrap_or_else(|error| {
                            panic!(
                                "the advertised recovery executes\n{case}\n{refused:?}\n{error:?}"
                            )
                        });
                assert_eq!(
                    cleared.resolved,
                    resolve(&apply(&layers, &cleared)),
                    "{case}"
                );
                let state = apply(&layers, &cleared);
                let retried =
                    plan_patch_in(&declaration, &state, &patch, target).unwrap_or_else(|error| {
                        panic!("the retry lands after the recovery\n{case}\n{refused:?}\n{error:?}")
                    });
                addressed.extend(nodes.into_iter().map(|(node, _)| node));
                recovered += 1;
                (state, retried)
            }
        };
        let after = apply(&state, &plan);
        assert_eq!(
            plan.resolved,
            resolve(&after),
            "the report is the post-state resolution\n{case}"
        );
        assert_asked(&plan, &state, &patch, target.is_some(), &case);
        assert_untouched(&layers.entry, &after.entry, &addressed, &case);
        assert_untouched(&layers.overlay, &after.overlay, &addressed, &case);
    }
    assert!(
        landed > 500 && recovered > 500,
        "the generator must exercise both outcomes: landed={landed} recovered={recovered}"
    );
}
