//! The `jinn:api-profile` consumer: the operator edit lane (FINDINGS.md
//! #9). `get` answers the profile document of record and `patch-entry`
//! reads it to apply the entry-patch law locally first — both through the
//! kernel's own `jinn:profile` `document` read since pin `3fd7b05` (jinnd
//! M2-K8), so neither depends on where the document sits (#25 closed) and
//! this entry needs no `jinn:fs` authority over it at all (#24). Its
//! grant's operation class is the editor's — the reads AND the write,
//! unlike the status viewer's read-only one (`tools/api-kit`).
//!
//! `patch-entry` applies the definition's entry-patch law to ONE entry
//! through the kernel's own `jinn:profile` `patch-entry` since pin
//! `57360cc` (jinnd M2-K7): the LOADER validates, writes the document
//! back atomically, restarts exactly the patched fiber, and records
//! `ProfilePatched` — operator intent with NO fs inverse and NO fiber
//! journal entry, so disposing this entry never touches the document
//! (#21 closed; the torn-write window of #22 closed for the profile). The
//! profile is never bypassed as the source of truth. Since pin `3fd7b05`
//! (`jinn:profile` 0.2.0, #26) the answer is `accepted(seq)` and the
//! restart is SCHEDULED, not awaited: the answer carries `patched-seq`,
//! the `ProfilePatched` row the restart's transitions land after, so an
//! operator can follow the restart through `jinn:ledger` instead of
//! inferring it from a call that blocked until it finished.
//!
//! A patch that changes nothing is answered without a kernel call; a
//! kernel refusal (scope, validation, the loader's retryable conflict,
//! an entry patching itself) is a typed `refused` answer, on the record.

use jinn_api::{
    decode_profile_answer, decode_profile_document, kernel_merge_patch, patch_entry,
    profile_patch_payload, refusal_is_retryable, Answer, ApiError, ErrorCode, PatchEntryRequest,
    ProfileDocument, API_VERSION, FINDING_NO_DOCUMENT_READ, KERNEL_PROFILE_CONTRACT,
    OP_KERNEL_DOCUMENT, OP_KERNEL_PATCH_ENTRY, OP_PATCH_ENTRY, OP_PROFILE_GET, PROFILE_CONTRACT,
};

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::{effects, services};

const EFFECT_TOKEN: u64 = 1;

fn fault(context: &str, error: impl std::fmt::Debug) -> GuestFault {
    GuestFault::Failed(format!("{context}: {error:?}"))
}

/// The document of record through the kernel's own `jinn:profile`
/// `document` read (pin `3fd7b05`): the entries this entry's scope
/// admits, wherever the document sits. A grant this entry does not hold,
/// or one whose scope or `ops` refuse the read, is the typed
/// `unavailable` answer — the lane says no without faulting its fiber.
fn read_profile() -> Result<serde_json::Value, ApiError> {
    let handle = services::resolve(KERNEL_PROFILE_CONTRACT).map_err(|error| {
        ApiError::unavailable(
            FINDING_NO_DOCUMENT_READ,
            format!("{KERNEL_PROFILE_CONTRACT} is not resolvable from this entry: {error:?}"),
        )
    })?;
    let bytes = services::call(handle, OP_KERNEL_DOCUMENT, &[]).map_err(|error| {
        ApiError::unavailable(
            FINDING_NO_DOCUMENT_READ,
            format!("{KERNEL_PROFILE_CONTRACT}/{OP_KERNEL_DOCUMENT} refused: {error:?}"),
        )
    })?;
    decode_profile_document(&bytes)
        .map_err(|error| ApiError::new(ErrorCode::Invalid, format!("profile: {error}")))
}

fn get() -> Result<ProfileDocument, ApiError> {
    Ok(ProfileDocument {
        api_version: API_VERSION.to_owned(),
        profile: read_profile()?,
        extra: jinn_api::Extensions::new(),
    })
}

/// The kernel applies the patch: one granted `jinn:profile` call. The
/// accepted patch's `ProfilePatched` ledger sequence, when the pinned
/// 0.2.0 provider answers one.
fn kernel_patch(id: &str, merge: &serde_json::Value) -> Result<Option<u64>, ApiError> {
    let handle = services::resolve(KERNEL_PROFILE_CONTRACT).map_err(|error| {
        ApiError::new(
            ErrorCode::Refused,
            format!("{KERNEL_PROFILE_CONTRACT}: {error:?}"),
        )
    })?;
    let bytes = services::call(handle, OP_KERNEL_PATCH_ENTRY, &profile_patch_payload(id, merge))
        .map_err(|error| ApiError::new(ErrorCode::Refused, format!("patch-entry: {error:?}")))?;
    decode_profile_answer(&bytes).map_err(|reason| {
        let mut error = ApiError::new(ErrorCode::Refused, format!("patch-entry refused: {reason}"));
        error
            .extra
            .insert("retryable".into(), serde_json::Value::Bool(refusal_is_retryable(&reason)));
        error
    })
}

fn patch(payload: &[u8]) -> Result<jinn_api::PatchEntryAnswer, ApiError> {
    let request: PatchEntryRequest = serde_json::from_slice(payload)
        .map_err(|error| ApiError::new(ErrorCode::Invalid, format!("patch-entry: {error}")))?;
    // The law, applied locally first: unknown ids and no-op patches are
    // answered without a kernel call.
    let mut document = read_profile()?;
    let answer = patch_entry(&mut document, &request)?;
    if !answer.changed {
        return Ok(answer);
    }
    let mut answer = answer;
    if let Some(sequence) = kernel_patch(&request.id, &kernel_merge_patch(&request.config))? {
        answer
            .extra
            .insert("patched-seq".into(), serde_json::json!(sequence));
    }
    Ok(answer)
}

fn answer<T: serde::Serialize>(result: Result<T, ApiError>) -> Answer {
    match result {
        Ok(value) => Answer::ok(serde_json::to_value(value).expect("an answer encodes")),
        Err(error) => Answer::error(error),
    }
}

struct Edit;

impl Guest for Edit {
    fn activate(_config: Vec<u8>) -> Result<(), GuestFault> {
        effects::register("jinn-profile-edit on duty", EFFECT_TOKEN)
            .map_err(|error| fault("effect", error))?;
        services::provide(PROFILE_CONTRACT).map_err(|error| fault("provide", error))?;
        Ok(())
    }

    fn check(_consumer: u64) -> bool {
        true
    }

    fn undo(_token: u64) -> Result<(), GuestFault> {
        Ok(())
    }

    fn handle_event(token: u64, topic: String, payload: Vec<u8>) -> Result<Vec<u8>, GuestFault> {
        Err(GuestFault::Failed(format!(
            "unexpected event {topic:?} (token {token}, {} bytes)",
            payload.len()
        )))
    }

    fn handle_call(
        _caller: u64,
        _contract: String,
        operation: String,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, GuestFault> {
        let answered = match operation.as_str() {
            OP_PROFILE_GET => answer(get()),
            OP_PATCH_ENTRY => answer(patch(&payload)),
            other => Answer::error(ApiError::new(
                ErrorCode::NotFound,
                format!("unknown operation {other:?}"),
            )),
        };
        Ok(answered.encode())
    }

    fn snapshot() -> Vec<u8> {
        Vec::new()
    }

    fn restore(_blob: Vec<u8>) -> Result<(), GuestFault> {
        Ok(())
    }
}

export!(Edit);
