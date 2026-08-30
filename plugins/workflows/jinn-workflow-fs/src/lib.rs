//! The DURABLE run store: `jinn:workflow.<store-id>` backed by one
//! append-only JSONL journal per workflow and one per run over `jinn:fs`.
//! `describe` says `durable: true`, and this file is what makes that
//! true.
//!
//! Everything that is not the journal is the shared store
//! (`store-core/store.rs`): the graph walk, the Todo driving, the poll,
//! the deferred emits, and every operation. The difference between this
//! store and the ephemeral one is where the records go — and the fact
//! that this one reads them back.
//!
//! # Restart honesty is an ORDERING, and it lives here
//!
//! The shared store appends a node's `pending -> running` line before it
//! asks any Todo store for anything. So a daemon that stops at any point
//! after that comes back to a document holding a node declared `running`
//! with no ending, which [`jinn_workflow::journal::replay`] reads back
//! exactly as written. What turns that into an honest answer is the ORDER
//! `store::activate` runs in: it replays, records every adopted run's
//! recovery, and only THEN provides its contract. Nothing in this file
//! needs a separate crash-recovery pass, and no caller can ever see a
//! `running` that no durable line justifies — the window in which one
//! exists closes before the contract is provided.
//!
//! # History is not rewritten to make it true
//!
//! The line that says a node started stays exactly as it was written.
//! What the recovery adds is NEW lines: a `node-state-changed` per open
//! node (`running -> interrupted`, carrying the reason the definition
//! names) and a `run-ended` for the run — so the ledger a caller can act
//! on and the state a reader is shown are the same state
//! ([`jinn_workflow::Workflows::plan_recovery`]). Both readings are then
//! in the document, in order, and neither replaced the other. The journal
//! is append-only in the strong sense: this store has no code path that
//! edits or removes a line it wrote, and the only rewrite it ever
//! performs drops a torn TAIL that was never a record (see `heal`).

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::types::KernelError;
use jinn::plugin::{effects, services};

use jinn_workflow::{store_contract, OP_DESCRIBE};

#[path = "../../store-core/store.rs"]
mod store;

/// This package, as `describe` names it for an operator reading a swap.
const PROVIDER: &str = "workflows/jinn-workflow-fs";
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
        // `store::activate` adopts every journal AND records what every
        // adopted run owes, so by the time the contract below is
        // provided, no run is declared `running` that this store cannot
        // account for. The provision is deliberately the LAST thing that
        // happens.
        let config = store::activate(&config).map_err(GuestFault::Failed)?;
        effects::register("jinn-workflow-fs on duty", EFFECT_TOKEN)
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
        // The JOURNALS are the durable state, not a snapshot blob. A
        // successor reads the documents; a blob would be a second copy of
        // the same fact, free to disagree with the one on disk.
        Vec::new()
    }

    fn restore(_blob: Vec<u8>) -> Result<(), GuestFault> {
        Ok(())
    }
}

export!(Fs);
