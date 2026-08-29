//! The `jinn:api-profile` consumer: the operator edit lane (FINDINGS.md
//! #9). `get` answers the profile document of record; `patch-entry`
//! applies the definition's entry-patch law to ONE entry and writes the
//! whole document back through the granted `jinn:fs` scope in one `write`
//! — a ledgered, revertible effect of this fiber (the kernel keeps its
//! inverse: FINDINGS.md #21 names what that means for the operator's
//! edits when this entry is disposed; #22 names the write's shape). The
//! daemon's file watcher then reconciles the edit exactly as it would an
//! operator's: reconcile-by-id restarts the patched entry and nothing
//! else. The profile is never bypassed as the source of truth.
//!
//! The idempotency key a request carries is passed to the write verbatim
//! (keyed exactly-once per fiber, kernel constitution 03); an identical
//! patch that changes nothing is answered without a write.

use std::sync::Mutex;

use jinn_api::{
    patch_entry, render_profile, Answer, ApiError, ErrorCode, PatchEntryRequest, ProfileDocument,
    API_VERSION, OP_PATCH_ENTRY, OP_PROFILE_GET, PROFILE_CONTRACT,
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
        Err(fs::FsError::NotFound) => Err(ApiError::new(
            ErrorCode::NotFound,
            format!("profile document {path:?} is absent"),
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

fn patch(path: &str, payload: &[u8]) -> Result<jinn_api::PatchEntryAnswer, ApiError> {
    let request: PatchEntryRequest = serde_json::from_slice(payload)
        .map_err(|error| ApiError::new(ErrorCode::Invalid, format!("patch-entry: {error}")))?;
    let mut document = read_profile(path)?;
    let answer = patch_entry(&mut document, &request)?;
    if answer.changed {
        fs::write(path, &render_profile(&document), &request.idempotency_key).map_err(|error| {
            ApiError::new(
                ErrorCode::Refused,
                format!("profile write refused: {error:?}"),
            )
        })?;
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
