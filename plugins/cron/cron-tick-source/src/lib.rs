//! The tick source: one fiber whose config IS the current time. Each
//! config edit restarts the fiber (the kernel's reconcile-by-id lane); the
//! fresh activation emits the tick on `jinn:cron/tick` and is done. Seq 0
//! is the boot seed and is never dispatched — a boot must not replay time.
//!
//! A replayed tick (daemon restart re-activating the last-written config)
//! is absorbed by the scheduler's firing law: no new boundary, no fire.

use jinn_cron::{TickPayload, TICK_TOPIC};

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::types::{DispatchMode, Selector};
use jinn::plugin::{effects, events};

const EFFECT_TOKEN: u64 = 1;

struct TickSource;

impl Guest for TickSource {
    fn activate(config: Vec<u8>) -> Result<(), GuestFault> {
        let tick: TickPayload = serde_json::from_slice(&config)
            .map_err(|error| GuestFault::Failed(format!("malformed tick config: {error}")))?;
        effects::register(&format!("tick {}", tick.seq), EFFECT_TOKEN)
            .map_err(|error| GuestFault::Failed(format!("effect: {error:?}")))?;
        if tick.seq == 0 {
            return Ok(());
        }
        // Failing loud on a refused dispatch leaves the fiber Failed and
        // visible; the next tick's config edit re-arms it (aim change).
        events::emit(TICK_TOPIC, DispatchMode::Serial, &Selector::All, &config)
            .map_err(|error| GuestFault::Failed(format!("tick dispatch: {error:?}")))?;
        Ok(())
    }

    fn check(_consumer: u64) -> bool {
        true
    }

    fn undo(_token: u64) -> Result<(), GuestFault> {
        Ok(())
    }

    fn handle_event(_token: u64, topic: String, _payload: Vec<u8>) -> Result<Vec<u8>, GuestFault> {
        Err(GuestFault::Failed(format!("unexpected event {topic:?}")))
    }

    fn handle_call(
        _caller: u64,
        _contract: String,
        operation: String,
        _payload: Vec<u8>,
    ) -> Result<Vec<u8>, GuestFault> {
        Err(GuestFault::Failed(format!(
            "unknown operation {operation:?}"
        )))
    }

    fn snapshot() -> Vec<u8> {
        Vec::new()
    }

    fn restore(_blob: Vec<u8>) -> Result<(), GuestFault> {
        Ok(())
    }
}

export!(TickSource);
