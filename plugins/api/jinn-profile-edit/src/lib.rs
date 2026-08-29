//! The `jinn:api-profile` consumer: the operator edit lane (FINDINGS.md
//! #9). `get` answers the profile document of record (through the
//! granted `jinn:fs` scope; #25 names where that read cannot reach);
//! `patch-entry` applies the definition's entry-patch law to ONE entry
//! through the kernel's own `jinn:profile` `patch-entry` since pin
//! `57360cc` (jinnd M2-K7): the LOADER validates, writes the document
//! back atomically, restarts exactly the patched fiber, and records
//! `ProfilePatched` — operator intent with NO fs inverse and NO fiber
//! journal entry, so disposing this entry never touches the document
//! (#21 closed; the torn-write window of #22 closed for the profile). The
//! profile is never bypassed as the source of truth.
//!
//! A patch that changes nothing is answered without a kernel call; a
//! kernel refusal (scope, validation, the loader's retryable conflict,
//! an entry patching itself) is a typed `refused` answer, on the record.

use std::sync::Mutex;

use jinn_api::{
    decode_profile_answer, kernel_merge_patch, patch_entry, profile_patch_payload,
    refusal_is_retryable, Answer, ApiError, ErrorCode, PatchEntryRequest, ProfileDocument,
    API_VERSION, FINDING_NO_DOCUMENT_READ, KERNEL_PROFILE_CONTRACT, OP_KERNEL_PATCH_ENTRY,
    OP_PATCH_ENTRY, OP_PROFILE_GET, PROFILE_CONTRACT,
};
use serde::Deserialize;

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::{effects, fs, services};

const EFFECT_TOKEN: u64 = 1;

#[derive(Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct EditConfig {
    /// The profile document, under the granted `jinn:fs` scope.
    #[serde(default = "default_profile_path")]
    profile_path: String,
}

fn default_profile_path() -> String {
    "profile.json".into()
}

static PATH: Mutex<String> = Mutex::new(String::new());

fn fault(context: &str, error: impl std::fmt::Debug) -> GuestFault {
    GuestFault::Failed(format!("{context}: {error:?}"))
}

/// The document of record, typed.
fn read_profile(path: &str) -> Result<serde_json::Value, ApiError> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|error| ApiError::new(ErrorCode::Invalid, format!("profile: {error}"))),
        Err(fs::FsError::NotFound) => Err(ApiError::unavailable(
            FINDING_NO_DOCUMENT_READ,
            format!("profile document {path:?} is not under the granted jinn:fs scope"),
        )),
        Err(refused) => Err(ApiError::new(
            ErrorCode::Refused,
            format!("profile read refused: {refused:?}"),
        )),
    }
}

fn get(path: &str) -> Result<ProfileDocument, ApiError> {
    Ok(ProfileDocument {
        api_version: API_VERSION.to_owned(),
        profile: read_profile(path)?,
        extra: jinn_api::Extensions::new(),
    })
}

/// The kernel applies the patch: one granted `jinn:profile` call.
fn kernel_patch(id: &str, merge: &serde_json::Value) -> Result<(), ApiError> {
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

fn patch(path: &str, payload: &[u8]) -> Result<jinn_api::PatchEntryAnswer, ApiError> {
    let request: PatchEntryRequest = serde_json::from_slice(payload)
        .map_err(|error| ApiError::new(ErrorCode::Invalid, format!("patch-entry: {error}")))?;
    // The law, applied locally first: unknown ids and no-op patches are
    // answered without a kernel call.
    let mut document = read_profile(path)?;
    let answer = patch_entry(&mut document, &request)?;
    if !answer.changed {
        return Ok(answer);
    }
    kernel_patch(&request.id, &kernel_merge_patch(&request.config))?;
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
    fn activate(config: Vec<u8>) -> Result<(), GuestFault> {
        let parsed: EditConfig = serde_json::from_slice(&config)
            .map_err(|error| GuestFault::Failed(format!("malformed config: {error}")))?;
        *PATH.lock().unwrap() = parsed.profile_path;
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
        let path = PATH.lock().unwrap().clone();
        let answered = match operation.as_str() {
            OP_PROFILE_GET => answer(get(&path)),
            OP_PATCH_ENTRY => answer(patch(&path, &payload)),
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
