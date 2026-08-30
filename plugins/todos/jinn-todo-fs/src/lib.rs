//! The DURABLE Todo store: `jinn:todo.<store-id>` backed by one
//! append-only JSONL journal per Todo over `jinn:fs`. `describe` says
//! `durable: true`, and this file is what makes that true.
//!
//! Everything that is not the journal is the shared store
//! (`store-core/store.rs`): the session driving, the poll, the deferred
//! emits, and every operation. The difference between this store and the
//! ephemeral one is where the records go — and the fact that this one
//! reads them back.
//!
//! # Restart honesty is an ORDERING, and it lives here
//!
//! The shared store appends `dispatch-started` before it asks any session
//! for anything. So a daemon that stops at any point after that comes
//! back to a journal holding a started dispatch with no ending, which
//! [`jinn_todo::journal::replay`] reads as `interrupted` with a reason and
//! the fold ([`jinn_todo::reported_status`]) reports as `blocked`.
//! Nothing in this file can produce a `running` dispatch from a document,
//! and nothing needs a crash-recovery pass: the conservative answer is
//! what an unfinished record MEANS, not something a later sweep repairs.
//!
//! # History is not rewritten to make it true
//!
//! The interrupted Todo's `declared-status` stays `executing`, because
//! that is what happened. The journal is append-only in the strong sense:
//! this store has no code path that edits or removes a line it wrote. The
//! honest reading is a DERIVATION over the whole document, not a repair
//! of part of it.

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::types::KernelError;
use jinn::plugin::{effects, services};

use jinn_todo::{store_contract, OP_DESCRIBE};

#[path = "../../store-core/store.rs"]
mod store;

/// This package, as `describe` names it for an operator reading a swap.
const PROVIDER: &str = "todos/jinn-todo-fs";
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
        // this answers, every Todo the last incarnation left behind is
        // back — with every unfinished dispatch already `interrupted`.
        let config = store::activate(&config).map_err(GuestFault::Failed)?;
        effects::register("jinn-todo-fs on duty", EFFECT_TOKEN)
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
