//! The FIXED catalog: `jinn:plugins.<catalog>` whose entry set is
//! DECLARED in this entry's own config.
//!
//! Two genuine uses. It is the catalog of a read-only appliance profile,
//! where the plugin tree is decided when the appliance is built and an
//! operator reads it rather than reshapes it. And it is the SWAP proof:
//! moving which package answers one `jinn:plugins.<catalog>` name changes
//! where the entry set comes from and nothing else, with the operator API
//! above it untouched.
//!
//! # It holds no `jinn:profile` grant at all
//!
//! That is the authority half of `grants.source = "catalog-declaration"`.
//! A fixed catalog could not read the document of record if its code
//! tried, so its grant lists are necessarily a CLAIM about the authority
//! rather than the authority — and every answer says so, in the response
//! the consumer reads, rather than only in this file. The lifecycle it
//! reports is not a claim: that comes from `jinn:introspect` and the
//! ledger, exactly as the live catalog's does.

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
const PROVIDER: &str = "plugins/jinn-plugins-static";
/// Its entry set and its grant lists are its own declaration.
const SOURCE: GrantSource = GrantSource::CatalogDeclaration;
const EFFECT_TOKEN: u64 = 1;

/// The one difference between the two catalogs.
mod source {
    use jinn_plugins::catalog::Declared;
    use jinn_plugins::PluginsError;

    use crate::catalog::CatalogConfig;

    /// The declared set, verbatim. An empty declaration is an empty
    /// catalog and is honest about it: this provider was configured with
    /// no entries, which is a reading of its config and not a failure to
    /// read anything.
    pub fn declared(config: &CatalogConfig) -> Result<Vec<Declared>, PluginsError> {
        Ok(config.entries.clone())
    }
}

fn fault(context: &str, error: KernelError) -> GuestFault {
    GuestFault::Failed(format!("{context}: {error:?}"))
}

struct Fixed;

impl Guest for Fixed {
    fn activate(config: Vec<u8>) -> Result<(), GuestFault> {
        let config = catalog::activate(&config).map_err(GuestFault::Failed)?;
        effects::register("jinn-plugins-static on duty", EFFECT_TOKEN)
            .map_err(|error| fault("effect", error))?;
        services::provide(&catalog_contract(&config.catalog))
            .map_err(|error| fault("provide", error))?;
        Ok(())
    }

    fn check(_consumer: u64) -> bool {
        true
    }

    fn undo(token: u64) -> Result<(), GuestFault> {
        // The transitions subscription is being withdrawn: what it
        // witnessed stops being fed, so it stops being answered.
        if token == catalog::WITNESS_TOKEN {
            catalog::withdraw_witness();
        }
        Ok(())
    }

    fn handle_event(_token: u64, topic: String, payload: Vec<u8>) -> Result<Vec<u8>, GuestFault> {
        catalog::witness(&topic, &payload).map_err(GuestFault::Failed)?;
        Ok(Vec::new())
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
        Vec::new()
    }

    fn restore(_blob: Vec<u8>) -> Result<(), GuestFault> {
        Ok(())
    }
}

export!(Fixed);
