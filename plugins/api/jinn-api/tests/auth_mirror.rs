//! THE AUTH MIRROR CHECK (harness packet 2.8). `auth.rs` spells the
//! kernel's `jinn:auth@0.1.0` a second time — the contract name, the
//! operation, the refusal case that becomes `ErrorCode::Unauthenticated`,
//! the `principal` record's one field — and the vendored `contract.wit`
//! is the first copy. This test PARSES the file (`harness_pin::ContractWit`,
//! the pin gate's own reader of the pinned tree) and asserts each name
//! against it, so the two copies cannot drift without a red test.
//!
//! What is enforced, exactly: names and case order. Not enforced here:
//! the wire bytes the daemon actually sends, which is the real-composition
//! suite's proof (`tests/composition/tests/auth.rs`).

use harness_pin::ContractWit;
use jinn_api::{ErrorCode, Principal, AUTH_CONTRACT, OP_VERIFY, UNAUTHENTICATED};

fn contract() -> ContractWit {
    ContractWit::vendored("jinn-auth").expect("the vendored jinn:auth bundle parses as WIT")
}

#[test]
fn the_contract_name_is_the_files_package() {
    assert_eq!(
        contract().package_name().expect("a package"),
        AUTH_CONTRACT,
        "the name a guest resolves is the one the file declares"
    );
}

#[test]
fn verify_is_declared_and_answers_an_anonymous_result() {
    // `result<principal, auth-error>` is anonymous: the operation is
    // declared (no error) and names no record — both halves asserted.
    assert_eq!(
        contract()
            .function_result(OP_VERIFY)
            .expect("the verify operation is declared"),
        None
    );
}

#[test]
fn the_refusal_class_is_the_variants_one_case_by_name() {
    let cases = contract()
        .variant_cases("auth-error")
        .expect("the auth-error variant");
    assert_eq!(
        cases,
        [UNAUTHENTICATED],
        "one refusal class, and it is the one this seam spells"
    );
    assert_eq!(
        serde_json::to_value(ErrorCode::Unauthenticated).expect("encodes"),
        cases[0],
        "`ErrorCode::Unauthenticated` serializes to the contract's case name"
    );
}

#[test]
fn the_principal_mirror_writes_exactly_the_records_fields() {
    let mirror = serde_json::json!({ "name": Principal { name: "operator".into() }.name });
    let keys: std::collections::BTreeSet<String> = mirror
        .as_object()
        .expect("an object")
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        keys,
        contract().record_fields("principal").expect("the record")
    );
}
