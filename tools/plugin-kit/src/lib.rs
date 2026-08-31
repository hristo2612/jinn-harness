//! The plugins seam's profile entries (one home per fact: every profile
//! that mounts the seam mounts these exact entries).
//!
//! # The swap this profile is BUILT to make reachable through the API
//!
//! `jinn:profile.patch-entry` may write ONE subtree: `config`. So the
//! package-and-hash swap every other seam proves by editing the profile
//! file is NOT expressible through the operator API (`FINDINGS.md` #37).
//! What IS expressible is a config edit — and this seam is shaped so that
//! WHICH PACKAGE answers a catalog name is decided by config:
//!
//! - both providers read their catalog id from `config.data.catalog`, and
//!   provide `jinn:plugins.<that id>` at activation;
//! - both are granted BOTH catalog names up front, so either may take
//!   either name without a grant edit.
//!
//! Two patches through `PATCH /v1/profile/entries/{id}` therefore move
//! `jinn:plugins.main` from one package to the other: park the incumbent,
//! then claim the name. The API entry is not touched and does not
//! restart, because it resolves a catalog contract PER REQUEST over the
//! string-keyed lane rather than holding it as an injection.
//!
//! The order matters and is not cosmetic. The kernel holds one provider
//! slot per contract name, so claiming an occupied name REFUSES at
//! `provide` and the claimant fails its activation. Parking first is what
//! makes the swap a swap; doing it the other way round is a real failed
//! activation, which this profile also mounts on purpose.

use serde_json::json;

/// The LIVE catalog's entry: the document of record as the entry set.
pub const LIVE_ID: &str = "jinn-plugins-live";
/// The FIXED catalog's entry: a declared entry set, no `jinn:profile`.
pub const FIXED_ID: &str = "jinn-plugins-appliance";
/// A catalog entry the document DISABLES: the mounted-but-no-incarnation
/// reading, from a source that positively says why.
pub const SHELVED_ID: &str = "jinn-plugins-shelf";
/// An entry whose activation genuinely FAILS, with a reason the kernel
/// itself records: its `jinn:net` grant admits exactly one port and its
/// config names another, so the bind is refused at the broker on the
/// record and the fiber fails, contained per R11.
pub const FAILING_ID: &str = "jinn-api-http-misbound";

pub const LIVE_PACKAGE: &str = "plugins/jinn-plugins-profile";
pub const FIXED_PACKAGE: &str = "plugins/jinn-plugins-static";

/// The catalog id an operator addresses. The SWITCHABLE name.
pub const MAIN_CATALOG: &str = "main";
/// Where a catalog goes when it gives up the switchable name.
pub const PARKED_CATALOG: &str = "parked";

/// Every plugins guest the kit builds.
pub const GUESTS: [&str; 2] = ["jinn-plugins-profile", "jinn-plugins-static"];

/// How many ledger lines an answer reads. Small on purpose: the window
/// is stated in every answer, so a bound that is easy to reason about is
/// worth more than one that is merely large.
pub const LEDGER_LIMIT: u32 = 400;

fn catalog_grants() -> Vec<serde_json::Value> {
    vec![
        json!(jinn_plugins::catalog_contract(MAIN_CATALOG)),
        json!(jinn_plugins::catalog_contract(PARKED_CATALOG)),
        json!(jinn_api::INTROSPECT_CONTRACT),
        json!(jinn_api::LEDGER_CONTRACT),
    ]
}

/// The entry set a FIXED catalog is built with. Deliberately a mix: two
/// entries that really are mounted, and one that is not, so a reader can
/// see that `not-mounted` is a reading this catalog can produce.
#[must_use]
pub fn declared_entries() -> serde_json::Value {
    json!([
        { "id": api_kit::PROVIDER_ID, "package": "api/jinn-api-http",
          "grants": [{ "contract": "jinn:net" }] },
        { "id": "jinn-status", "package": "api/jinn-status", "grants": [] },
        { "id": "a-plugin-this-appliance-was-built-with", "package": "plugins/absent",
          "grants": [] },
    ])
}

/// The live catalog's entry. It reads the document of record through a
/// `jinn:profile` grant attenuated to `ops: ["document"]` — a viewer that
/// CANNOT patch. Reshaping the machine stays with `jinn-profile-edit`.
#[must_use]
pub fn live_entry(hash: &str, catalog: &str) -> serde_json::Value {
    let mut grants = catalog_grants();
    grants.push(json!({
        "contract": jinn_api::KERNEL_PROFILE_CONTRACT,
        "scope": [jinn_api::KERNEL_PROFILE_SCOPE_ALL],
        "ops": jinn_api::KERNEL_PROFILE_READ_OPS,
    }));
    json!({ "id": LIVE_ID, "package": LIVE_PACKAGE, "hash": hash,
            "config": { "grants": grants,
                        "data": { "catalog": catalog, "ledger-limit": LEDGER_LIMIT } } })
}

/// The fixed catalog's entry. NO `jinn:profile` grant at all — the
/// authority half of `grants.source = "catalog-declaration"`.
#[must_use]
pub fn fixed_entry(id: &str, hash: &str, catalog: &str, disabled: bool) -> serde_json::Value {
    let mut entry = json!({ "id": id, "package": FIXED_PACKAGE, "hash": hash,
            "config": { "grants": catalog_grants(),
                        "data": { "catalog": catalog, "ledger-limit": LEDGER_LIMIT,
                                  "entries": declared_entries() } } });
    if disabled {
        entry["disabled"] = json!(true);
    }
    entry
}

/// An entry that FAILS to activate, for a reason the kernel records. Its
/// `jinn:net` grant admits exactly `granted`, and its config names
/// `bound`; the broker refuses the bind on the record (`GrantRefused`)
/// and the fiber fails. Mounted on purpose: a seam that claims to report
/// a failed activation honestly has to have one to report.
#[must_use]
pub fn misbound_entry(hash: &str, granted: u16, bound: u16) -> serde_json::Value {
    json!({ "id": FAILING_ID, "package": "api/jinn-api-http", "hash": hash,
            "config": { "grants": [{ "contract": "jinn:net",
                                     "scope": { "bind": [granted, granted] } }],
                        "data": { "port": bound } } })
}

/// The catalog contracts the operator API may route to. As with every
/// other seam, the grant list IS the authority the kernel enforces.
#[must_use]
pub fn api_catalog_grants(catalogs: &[&str]) -> Vec<serde_json::Value> {
    catalogs
        .iter()
        .map(|catalog| json!(jinn_plugins::catalog_contract(catalog)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grants(entry: &serde_json::Value) -> Vec<String> {
        entry["config"]["grants"]
            .as_array()
            .expect("grants")
            .iter()
            .map(|grant| {
                grant
                    .as_str()
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| grant["contract"].as_str().expect("contract").to_owned())
            })
            .collect()
    }

    #[test]
    fn a_fixed_catalog_holds_no_profile_authority_at_all() {
        let fixed = fixed_entry(FIXED_ID, "h", PARKED_CATALOG, false);
        assert!(
            !grants(&fixed).contains(&jinn_api::KERNEL_PROFILE_CONTRACT.to_owned()),
            "a declared catalog must not be able to read the document it claims about"
        );
        // The precondition that makes that non-vacuous: the LIVE one does
        // hold it, so the absence above is this entry's and not the
        // helper's.
        assert!(grants(&live_entry("h", MAIN_CATALOG))
            .contains(&jinn_api::KERNEL_PROFILE_CONTRACT.to_owned()));
    }

    #[test]
    fn the_live_catalog_can_read_the_document_and_cannot_patch_it() {
        let live = live_entry("h", MAIN_CATALOG);
        let profile = live["config"]["grants"]
            .as_array()
            .expect("grants")
            .iter()
            .find(|grant| grant["contract"] == jinn_api::KERNEL_PROFILE_CONTRACT)
            .expect("a profile grant");
        let ops: Vec<&str> = profile["ops"]
            .as_array()
            .expect("ops")
            .iter()
            .map(|op| op.as_str().expect("op"))
            .collect();
        assert!(ops.contains(&"document"));
        assert!(
            !ops.contains(&jinn_api::OP_KERNEL_PATCH_ENTRY),
            "a catalog reads the machine; it does not reshape it"
        );
    }

    #[test]
    fn both_catalogs_may_take_either_name_so_a_swap_needs_no_grant_edit() {
        for entry in [
            live_entry("h", MAIN_CATALOG),
            fixed_entry(FIXED_ID, "h", PARKED_CATALOG, false),
        ] {
            let held = grants(&entry);
            for catalog in [MAIN_CATALOG, PARKED_CATALOG] {
                assert!(
                    held.contains(&jinn_plugins::catalog_contract(catalog)),
                    "{} cannot take {catalog}",
                    entry["id"]
                );
            }
        }
    }

    #[test]
    fn a_catalog_is_granted_no_seam_contract_at_all() {
        for entry in [
            live_entry("h", MAIN_CATALOG),
            fixed_entry(FIXED_ID, "h", PARKED_CATALOG, false),
        ] {
            for held in grants(&entry) {
                assert!(
                    !held.starts_with("jinn:todo.")
                        && !held.starts_with("jinn:session.")
                        && !held.starts_with("jinn:engine.")
                        && !held.starts_with("jinn:workflow."),
                    "a catalog reached {held}, which it has no business knowing about"
                );
            }
        }
    }

    #[test]
    fn the_misbound_entry_is_configured_to_fail_its_bind() {
        let entry = misbound_entry("h", 7000, 7001);
        let range = &entry["config"]["grants"][0]["scope"]["bind"];
        assert_eq!(range, &serde_json::json!([7000, 7000]));
        assert_eq!(entry["config"]["data"]["port"], 7001);
    }

    #[test]
    fn a_shelved_catalog_is_disabled_in_the_document_and_a_live_one_is_not() {
        assert_eq!(
            fixed_entry(SHELVED_ID, "h", "shelf", true)["disabled"],
            serde_json::json!(true)
        );
        assert!(fixed_entry(FIXED_ID, "h", PARKED_CATALOG, false)
            .get("disabled")
            .is_none());
    }
}
