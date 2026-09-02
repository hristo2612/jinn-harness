//! The `jinn:ui-bundle` embedded provider: the kit's archive and manifest
//! compiled in from `$JINN_UI_BUNDLE_DIR`, answered verbatim. No grant but
//! its own contract, no config, no state: the artifact IS the UI.

use jinn_ui::{API_VERSION, BUNDLE_CONTRACT, OP_BUNDLE, OP_MANIFEST};

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::{effects, services};

const EFFECT_TOKEN: u64 = 1;
/// The kit's archive (`jinn_ui::encode_bundle`'s shape).
static BUNDLE: &[u8] = include_bytes!(concat!(env!("JINN_UI_BUNDLE_DIR"), "/bundle.bin"));
/// The kit's manifest of that archive (`jinn_ui::Manifest`, JSON).
static MANIFEST: &[u8] = include_bytes!(concat!(env!("JINN_UI_BUNDLE_DIR"), "/manifest.json"));

fn fault(context: &str, error: impl std::fmt::Debug) -> GuestFault {
    GuestFault::Failed(format!("{context}: {error:?}"))
}

struct Embedded;

impl Guest for Embedded {
    fn activate(_config: Vec<u8>) -> Result<(), GuestFault> {
        effects::register("jinn-ui-bundle-embedded on duty", EFFECT_TOKEN)
            .map_err(|error| fault("effect", error))?;
        services::provide(BUNDLE_CONTRACT).map_err(|error| fault("provide", error))?;
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
        _payload: Vec<u8>,
    ) -> Result<Vec<u8>, GuestFault> {
        Ok(match operation.as_str() {
            OP_MANIFEST => MANIFEST.to_vec(),
            OP_BUNDLE => BUNDLE.to_vec(),
            // A refusal is an answer, not a fault (R11): the envelope every
            // seam's error carries, so a caller reads a typed `not-found`.
            other => serde_json::json!({ "api-version": API_VERSION,
                                          "error": { "code": "not-found",
                                                     "detail": format!("unknown operation {other:?}") } })
                .to_string()
                .into_bytes(),
        })
    }

    fn snapshot() -> Vec<u8> {
        Vec::new()
    }

    fn restore(_blob: Vec<u8>) -> Result<(), GuestFault> {
        Ok(())
    }
}

export!(Embedded);
