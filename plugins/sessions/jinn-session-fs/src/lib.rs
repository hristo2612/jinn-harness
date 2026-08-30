//! The DURABLE session store: `jinn:session.<store-id>` backed by one
//! append-only JSONL journal per session over `jinn:fs`. `describe` says
//! `durable: true`, and this file is what makes that true.
//!
//! Everything that is not the journal is the shared store
//! (`store-core/store.rs`): the engine driving, the poll, the deferred
//! emits, and every operation. The difference between this store and the
//! ephemeral one is where the records go — and the fact that this one
//! reads them back.
//!
//! # Restart honesty is an ORDERING, and it lives here
//!
//! The shared store appends `turn-started` before it asks any engine for
//! anything. So a daemon that stops at any point after that comes back to
//! a journal holding a started turn with no ending, which
//! [`jinn_session::journal::replay`] reads as `interrupted` with a
//! reason. Nothing in this file can produce `running` from a document,
//! and nothing needs a crash-recovery pass: the conservative answer is
//! what an unfinished record MEANS, not something a later sweep repairs.
//!
//! # The tear, and the guarantee this store does not lean on
//!
//! `jinn:fs`'s `append` commits whole-document atomically since pin
//! `3fd7b05` (stage + fsync + rename — `FINDINGS.md` #22), so a torn
//! write should be unreachable through this path. The reader does not
//! rely on it: a trailing unterminated line is admitted as ABSENCE and a
//! hole anywhere earlier is REFUSED, because that guarantee belongs to a
//! contract this seam does not own and a reader that trusts it has no
//! answer the day it changes.

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
const PROVIDER: &str = "sessions/jinn-session-fs";
/// The store's own promise about where its records live.
const DURABLE: bool = true;
const EFFECT_TOKEN: u64 = 1;

mod journal;

fn fault(context: &str, error: KernelError) -> GuestFault {
    GuestFault::Failed(format!("{context}: {error:?}"))
}

struct Fs;

impl Guest for Fs {
    fn activate(config: Vec<u8>) -> Result<(), GuestFault> {
        // `store::activate` calls `journal::adopt_all`, so by the time
        // this answers, every session the last incarnation left behind is
        // back — with every unfinished turn already `interrupted`.
        let config = store::activate(&config).map_err(GuestFault::Failed)?;
        effects::register("jinn-session-fs on duty", EFFECT_TOKEN)
            .map_err(|error| fault("effect", error))?;
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
        let (Some(instant), true, true) = (
            instant,
            topic == store::WAKE_TOPIC,
            token == store::ALARM_TOKEN,
        ) else {
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
        // The JOURNAL is the durable state, not a snapshot blob. A
        // successor reads the documents; a blob would be a second copy of
        // the same fact, free to disagree with the one on disk.
        Vec::new()
    }

    fn restore(_blob: Vec<u8>) -> Result<(), GuestFault> {
        Ok(())
    }
}

export!(Fs);
