//! The EPHEMERAL session store: `jinn:session.<store-id>` served entirely
//! from this incarnation's memory. Nothing here outlives the fiber, and
//! that is the declaration `describe` makes — `durable: false` — so a
//! consumer gates on the store's own word rather than on its package name.
//!
//! Two jobs, and both are real. It is the genuine store for throwaway and
//! test sessions: no disk, no cleanup, and no grant beyond the engine it
//! drives. And it is the SWAP proof — moving a profile entry's package
//! from `jinn-session-fs` to this one changes where sessions live and
//! nothing else, with the API and the engine providers untouched.
//!
//! Everything but that declaration is the shared store (`store-core/`):
//! the engine driving, the poll, the deferred emits, and every operation.
//! This file is the difference, and the difference is that its journal
//! writes nowhere.

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::types::KernelError;
use jinn::plugin::{effects, services};

use jinn_session::{store_contract, OP_DESCRIBE};

#[path = "../../store-core/store.rs"]
mod store;

/// This package, as `describe` names it for an operator reading a swap.
const PROVIDER: &str = "sessions/jinn-session-memory";
/// The store's own promise about where its records live.
const DURABLE: bool = false;
const EFFECT_TOKEN: u64 = 1;

/// The five points where a durable store would write, and where this one
/// does not. Not stubs standing in for unfinished work: "nothing is
/// recorded" is this store's whole contract, and `describe` says so.
mod journal {
    use jinn_session::{SessionError, SessionSpec, Turn};

    use crate::store::StoreConfig;

    pub fn created(_session: &str, _spec: &SessionSpec, _at_ms: u64) -> Result<(), SessionError> {
        Ok(())
    }

    pub fn turn_started(
        _session: &str,
        _turn: &str,
        _message: &str,
        _at_ms: u64,
    ) -> Result<(), SessionError> {
        Ok(())
    }

    pub fn turn_ended(_session: &str, _turn: &Turn, _at_ms: u64) -> Result<(), SessionError> {
        Ok(())
    }

    pub fn closed(_session: &str, _at_ms: u64) -> Result<(), SessionError> {
        Ok(())
    }

    /// A fresh incarnation of an ephemeral store holds nothing, and
    /// answering an empty registry is the honest state of one — never a
    /// silent failure to find records that were never written.
    pub fn adopt_all(_config: &StoreConfig) -> Result<(), SessionError> {
        Ok(())
    }
}

fn fault(context: &str, error: KernelError) -> GuestFault {
    GuestFault::Failed(format!("{context}: {error:?}"))
}

struct Memory;

impl Guest for Memory {
    fn activate(config: Vec<u8>) -> Result<(), GuestFault> {
        let config = store::activate(&config).map_err(GuestFault::Failed)?;
        effects::register("jinn-session-memory on duty", EFFECT_TOKEN)
            .map_err(|error| fault("effect", error))?;
        // NO alarm here: this store holds no schedule of its own. Every
        // alarm it ever asks for is a poll of a run it is driving.
        services::provide(&store_contract(&config.store))
            .map_err(|error| fault("provide", error))?;
        Ok(())
    }

    fn check(_consumer: u64) -> bool {
        true
    }

    fn undo(_token: u64) -> Result<(), GuestFault> {
        Ok(())
    }

    fn handle_event(token: u64, topic: String, payload: Vec<u8>) -> Result<Vec<u8>, GuestFault> {
        let instant: Option<[u8; 8]> = payload.as_slice().try_into().ok();
        let (Some(instant), true, true) =
            (instant, topic == store::WAKE_TOPIC, token == store::ALARM_TOKEN)
        else {
            return Err(GuestFault::Failed(format!(
                "unexpected event {topic:?} (token {token}, {} bytes)",
                payload.len()
            )));
        };
        let driving = store::on_wake(u64::from_le_bytes(instant)).map_err(GuestFault::Failed)?;
        Ok(
            serde_json::to_vec(&serde_json::json!({ "driving": driving }))
                .expect("the wake summary encodes"),
        )
    }

    fn handle_call(
        _caller: u64,
        _contract: String,
        operation: String,
        payload: Vec<u8>,
    ) -> Result<Vec<u8>, GuestFault> {
        let answer = match operation.as_str() {
            OP_DESCRIBE => store::describe(PROVIDER, DURABLE),
            other => store::dispatch(other, &payload),
        };
        Ok(answer.encode())
    }

    fn snapshot() -> Vec<u8> {
        // Ephemeral BY DECLARATION: a successor starts empty, which is
        // what `durable: false` promises. Carrying state across here
        // would make that promise — and the swap proof that rests on it
        // — a lie.
        Vec::new()
    }

    fn restore(_blob: Vec<u8>) -> Result<(), GuestFault> {
        Ok(())
    }
}

export!(Memory);
