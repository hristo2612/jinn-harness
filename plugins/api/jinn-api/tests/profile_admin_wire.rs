//! The `jinn:profile-admin` routes as the definition shapes them (pin
//! `f8b285b`, jinnd M2-K23; FINDINGS #37): which write a request names,
//! the broker wire each write crosses on, and how a typed refusal comes
//! back as this seam's `refused`.

use jinn_api::profile_admin::{
    admin_answer, admin_payload, admin_route, AdminRoute, AdminWrite, ADMIN_CONTRACT,
};
use jinn_api::ErrorCode;

fn segments(payload: &[u8]) -> Vec<String> {
    let mut at = 0;
    let mut out = Vec::new();
    while at < payload.len() {
        let len = u32::from_le_bytes(payload[at..at + 4].try_into().expect("u32")) as usize;
        at += 4;
        out.push(String::from_utf8(payload[at..at + len].to_vec()).expect("utf-8"));
        at += len;
    }
    out
}

#[test]
fn the_five_writes_are_shaped_by_method_path_and_body() {
    assert_eq!(ADMIN_CONTRACT, "jinn:profile-admin");
    let add = admin_route(
        "POST",
        "/v1/profile/entries",
        &serde_json::json!({ "id": "x", "package": "p", "hash": "h" }),
    )
    .expect("shaped")
    .expect("a write");
    assert_eq!(
        add,
        AdminRoute {
            id: "x".into(),
            write: AdminWrite::Add(serde_json::json!({ "id": "x", "package": "p", "hash": "h" }))
        }
    );
    let remove = admin_route("DELETE", "/v1/profile/entries/x", &serde_json::Value::Null)
        .expect("shaped")
        .expect("a write");
    assert_eq!(remove.write, AdminWrite::Remove);
    let disabled = admin_route(
        "PATCH",
        "/v1/profile/entries/x",
        &serde_json::json!({ "disabled": true }),
    )
    .expect("shaped")
    .expect("a write");
    assert_eq!(disabled.write, AdminWrite::SetDisabled(true));
    let grants = admin_route(
        "PATCH",
        "/v1/profile/entries/x",
        &serde_json::json!({ "grants": ["jinn:clock"] }),
    )
    .expect("shaped")
    .expect("a write");
    assert_eq!(
        grants.write,
        AdminWrite::SetGrants(serde_json::json!(["jinn:clock"]))
    );
    let swap = admin_route(
        "PATCH",
        "/v1/profile/entries/x",
        &serde_json::json!({ "package": "p", "hash": "h" }),
    )
    .expect("shaped")
    .expect("a write");
    assert_eq!(
        swap.write,
        AdminWrite::SwapPlugin {
            package: "p".into(),
            version: String::new(),
            hash: "h".into()
        }
    );
}

#[test]
fn a_config_patch_is_not_an_admin_write_and_a_mixed_body_is_invalid() {
    // The config route's own body: `None` — the static route answers it.
    assert!(admin_route(
        "PATCH",
        "/v1/profile/entries/x",
        &serde_json::json!({ "config": { "data": {} } })
    )
    .is_none());
    // One write per call: a body carrying two is `invalid` before any kernel call.
    let mixed = admin_route(
        "PATCH",
        "/v1/profile/entries/x",
        &serde_json::json!({ "disabled": true, "grants": [] }),
    )
    .expect("shaped")
    .expect_err("invalid");
    assert_eq!(mixed.code, ErrorCode::Invalid);
    // A swap needs both halves of the pin.
    let half = admin_route(
        "PATCH",
        "/v1/profile/entries/x",
        &serde_json::json!({ "package": "p" }),
    )
    .expect("shaped")
    .expect_err("invalid");
    assert_eq!(half.code, ErrorCode::Invalid);
    // Not this surface at all.
    assert!(admin_route("GET", "/v1/profile/entries/x", &serde_json::Value::Null).is_none());
    assert!(admin_route("POST", "/v1/profile/entries/x", &serde_json::Value::Null).is_none());
    assert!(admin_route("DELETE", "/v1/profile/entries", &serde_json::Value::Null).is_none());
}

#[test]
fn every_write_crosses_the_broker_as_length_prefixed_segments() {
    let (operation, payload) = admin_payload("x", &AdminWrite::SetDisabled(true));
    assert_eq!(operation, "set-disabled");
    assert_eq!(segments(&payload), ["x", "true"]);
    let (operation, payload) = admin_payload(
        "x",
        &AdminWrite::SwapPlugin {
            package: "p".into(),
            version: "1".into(),
            hash: "h".into(),
        },
    );
    assert_eq!(operation, "swap-plugin");
    assert_eq!(segments(&payload), ["x", "p", "1", "h"]);
    let (operation, payload) = admin_payload("x", &AdminWrite::Remove);
    assert_eq!(operation, "remove-entry");
    assert_eq!(segments(&payload), ["x"]);
    let (operation, payload) = admin_payload("x", &AdminWrite::SetGrants(serde_json::json!(["a"])));
    assert_eq!(operation, "set-grants");
    assert_eq!(segments(&payload), ["x", "[\"a\"]"]);
    let (operation, payload) =
        admin_payload("x", &AdminWrite::Add(serde_json::json!({ "id": "x" })));
    assert_eq!(operation, "add-entry");
    assert_eq!(
        segments(&payload),
        ["x"].map(|_| "{\"id\":\"x\"}".to_owned())
    );
}

#[test]
fn the_answer_is_the_rows_sequence_or_a_typed_refusal_with_its_class() {
    let mut accepted = vec![2u8];
    accepted.extend(42u64.to_le_bytes());
    assert_eq!(
        admin_answer("set-disabled", &accepted).expect("accepted"),
        42
    );
    let mut refused = vec![1u8, 3];
    refused.extend(b"an operation is in flight");
    let error = admin_answer("set-disabled", &refused).expect_err("refused");
    assert_eq!(error.code, ErrorCode::Refused);
    assert_eq!(error.extra["class"], "conflict");
    assert_eq!(error.extra["retryable"], true);
    assert!(
        error.detail.contains("set-disabled") && error.detail.contains("in flight"),
        "{error:?}"
    );
    for (byte, class) in [(1u8, "unauthorized"), (2, "malformed"), (4, "irreversible")] {
        let error = admin_answer("remove-entry", &[1, byte]).expect_err("refused");
        assert_eq!(error.extra["class"], class);
        assert_eq!(error.extra["retryable"], false);
    }
    let garbage = admin_answer("remove-entry", &[9]).expect_err("malformed answer");
    assert_eq!(garbage.code, ErrorCode::Refused);
}
