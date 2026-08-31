//! The LIVE catalog: `jinn:plugins.<catalog>` whose entry set is the
//! DOCUMENT OF RECORD, read through `jinn:profile`'s `document` view.
//!
//! This is the catalog an operator wants almost always: it reports the
//! machine as it is actually configured, and its grant lists are the
//! authority the kernel enforces rather than a claim about it. That is
//! why every entry it answers carries `grants.source =
//! "profile-document"`.
//!
//! What it is NOT: a second source of truth. It reads the document and
//! joins the kernel's own composition and ledger onto it. It writes
//! nothing, patches nothing, and holds a `jinn:profile` grant attenuated
//! to `ops: ["document"]` — a viewer that CANNOT patch. An operator's
//! edit still goes through `jinn-profile-edit`, which this seam consumes
//! and does not replace.

wit_bindgen::generate!({
    path: "../../../kernel-pin/wit",
    world: "plugin",
});

use exports::jinn::plugin::lifecycle::{Guest, GuestFault};
use jinn::plugin::types::KernelError;
use jinn::plugin::{effects, services};

use jinn_plugins::{catalog_contract, GrantSource};

#[path = "../../catalog-core/catalog.rs"]
mod catalog;

/// This package, as every answer names it — the BINDING half of a swap.
const PROVIDER: &str = "plugins/jinn-plugins-profile";
/// Its entry set and its grant lists are the document of record.
const SOURCE: GrantSource = GrantSource::ProfileDocument;
const EFFECT_TOKEN: u64 = 1;

/// The one difference between the two catalogs: where the entry set comes
/// from. Everything else is `catalog-core`.
mod source {
    use jinn_plugins::catalog::{Catalog, Declared};
    use jinn_plugins::PluginsError;

    use crate::catalog::CatalogConfig;

    /// The document of record, through the kernel's own read view. A
    /// refusal is a typed `unavailable` naming `jinn:profile` — never an
    /// empty catalog, which would report a configured machine as bare.
    pub fn declared(_config: &CatalogConfig) -> Result<Vec<Declared>, PluginsError> {
        let bytes = crate::catalog::read_profile_document()?;
        let document: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
            PluginsError::new(
                jinn_plugins::ErrorCode::Failed,
                format!("malformed profile document: {error}"),
            )
        })?;
        Catalog::parse_document(&document)
    }
}

fn fault(context: &str, error: KernelError) -> GuestFault {
    GuestFault::Failed(format!("{context}: {error:?}"))
}

struct Live;

impl Guest for Live {
    fn activate(config: Vec<u8>) -> Result<(), GuestFault> {
        let config = catalog::activate(&config).map_err(GuestFault::Failed)?;
        effects::register("jinn-plugins-profile on duty", EFFECT_TOKEN)
            .map_err(|error| fault("effect", error))?;
        // The provision is the LAST thing: nothing resolves this catalog
        // before it can answer.
        services::provide(&catalog_contract(&config.catalog))
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
        Ok(catalog::dispatch(&operation, &payload).encode())
    }

    fn snapshot() -> Vec<u8> {
        // A catalog holds no state of its own: every answer is a fresh
        // join of three kernel reads. A blob here would be a second copy
        // free to disagree with the machine.
        Vec::new()
    }

    fn restore(_blob: Vec<u8>) -> Result<(), GuestFault> {
        Ok(())
    }
}

export!(Live);
