//! The overlay store: the hot settings layer, held in this entry's
//! `config.data.overlays` in the profile document and answered verbatim
//! over `jinn:settings-store`. A hot patch restarts THIS fiber (the
//! loader's `ConfigChanged`), which calls nothing at activation — so the
//! provider may patch it synchronously from inside its own `patch`
//! (no nested dispatch, FINDINGS.md #26).

use std::sync::Mutex;

use jinn_settings::{Overlays, OP_OVERLAYS, STORE_CONTRACT};

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::{effects, services};

const EFFECT_TOKEN: u64 = 1;

static OVERLAYS: Mutex<Option<Overlays>> = Mutex::new(None);

struct Store;

impl Guest for Store {
    fn activate(config: Vec<u8>) -> Result<(), GuestFault> {
        let overlays: Overlays = if config.is_empty() {
            Overlays::default()
        } else {
            serde_json::from_slice(&config)
                .map_err(|error| GuestFault::Failed(format!("malformed config: {error}")))?
        };
        *OVERLAYS.lock().unwrap() = Some(overlays);
        effects::register("jinn-settings-store on duty", EFFECT_TOKEN)
            .map_err(|error| GuestFault::Failed(format!("effect: {error:?}")))?;
        services::provide(STORE_CONTRACT)
            .map_err(|error| GuestFault::Failed(format!("provide: {error:?}")))?;
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
        match operation.as_str() {
            OP_OVERLAYS => {
                let held = OVERLAYS.lock().unwrap().clone().unwrap_or_default();
                Ok(serde_json::to_vec(&held).expect("overlays encode"))
            }
            other => Err(GuestFault::Failed(format!("unknown operation {other:?}"))),
        }
    }

    fn snapshot() -> Vec<u8> {
        Vec::new()
    }

    fn restore(_blob: Vec<u8>) -> Result<(), GuestFault> {
        Ok(())
    }
}

export!(Store);
