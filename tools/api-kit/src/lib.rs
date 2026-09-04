//! The operator-API seam's profile ENTRIES (one home per fact: every
//! profile that mounts the api trio or the settings pair mounts these
//! exact entries). The binary that writes the operator profile from them
//! is `main.rs`; `engine-kit` mounts them beside the engines seam.

/// The HTTP provider's id in the operator profile.
pub const PROVIDER_ID: &str = "jinn-api-http";
/// The settings provider's and overlay store's ids.
pub const SETTINGS_ID: &str = "jinn-settings-profile";
/// See [`SETTINGS_ID`].
pub const STORE_ID: &str = "jinn-settings-store";

/// The api trio's profile entries. Authority is the profile side's: the
/// provider's `jinn:net` grant is scoped to exactly its port (loopback is
/// the bundle's own v0.1 bound), it holds NO clock (served from the
/// kernel's readiness wakes), and it holds `jinn:auth` BARE — the door
/// (packet 2.8): the bundle declares no scope, a scoped grant would
/// refuse at admission, and without the grant the provider cannot ask
/// and answers every request `refused`; the status consumer holds the read-only
/// kernel contracts (`jinn:introspect`, `jinn:ledger`) and reads the
/// document of record through a `jinn:profile` grant attenuated to
/// `ops: ["document"]` — a viewer that CANNOT patch (FINDINGS.md #24
/// closed at pin `3fd7b05`); the editor holds `jinn:profile` over every
/// entry (the scope written out — a bare grant patches nothing) with the
/// reads AND the write. Neither consumer holds any `jinn:fs` authority
/// over the document any more (#25 closed: the read no longer depends on
/// where the document sits). Each is granted the contract it provides;
/// the status consumer probes `jinn:cron`.
#[must_use]
pub fn api_entries(http: &str, status: &str, edit: &str, port: u16) -> Vec<serde_json::Value> {
    let profile_grant = |ops: &[&str]| {
        serde_json::json!({ "contract": jinn_api::KERNEL_PROFILE_CONTRACT,
                            "scope": [jinn_api::KERNEL_PROFILE_SCOPE_ALL],
                            "ops": ops })
    };
    vec![
        serde_json::json!({ "id": PROVIDER_ID, "package": "api/jinn-api-http", "hash": http,
          "config": { "grants": [
                          { "contract": "jinn:net", "scope": { "bind": [port, port] } },
                          jinn_api::AUTH_CONTRACT,
                          jinn_api::STATUS_CONTRACT, jinn_api::PROFILE_CONTRACT,
                          jinn_settings::SETTINGS_CONTRACT ],
                      "data": { "port": port } } }),
        serde_json::json!({ "id": "jinn-status", "package": "api/jinn-status", "hash": status,
          "config": { "grants": [jinn_api::STATUS_CONTRACT, jinn_cron::CRON_CONTRACT,
                                 jinn_api::INTROSPECT_CONTRACT, jinn_api::LEDGER_CONTRACT,
                                 profile_grant(&jinn_api::KERNEL_PROFILE_READ_OPS)],
                      "data": { "probes": [ { "contract": jinn_cron::CRON_CONTRACT, "operation": jinn_cron::OP_JOBS } ] } } }),
        serde_json::json!({ "id": "jinn-profile-edit", "package": "api/jinn-profile-edit", "hash": edit,
          "config": { "grants": [jinn_api::PROFILE_CONTRACT,
                                 profile_grant(&jinn_api::KERNEL_PROFILE_EDIT_OPS)],
                      "data": {} } }),
    ]
}

/// The settings seam's entries: the provider (granted `jinn:settings` to
/// provide, `jinn:settings-store` to read the overlay, the two topics it
/// EMITS — `jinn:settings/changed` and `jinn:settings/refused`; at pin
/// `138fdce` an emit is covered by the topic's own grant, jinnd M2-K26
/// (e), FINDINGS #49 — and `jinn:profile` scoped to exactly the entries
/// it may patch — every namespace owner and the store) and the store
/// (granted only what it provides). The
/// store's `overlays` is the hot layer's home in the document; the kit
/// writes it empty.
#[must_use]
pub fn settings_entries(provider: &str, store: &str, owners: &[&str]) -> Vec<serde_json::Value> {
    let mut scope: Vec<&str> = owners.to_vec();
    scope.push(STORE_ID);
    vec![
        serde_json::json!({ "id": SETTINGS_ID, "package": "settings/jinn-settings-profile", "hash": provider,
          "config": { "grants": [jinn_settings::SETTINGS_CONTRACT, jinn_settings::STORE_CONTRACT,
                                 jinn_settings::CHANGED_TOPIC, jinn_settings::REFUSED_TOPIC,
                                 { "contract": jinn_api::KERNEL_PROFILE_CONTRACT, "scope": scope }],
                      "data": { "store": STORE_ID } } }),
        serde_json::json!({ "id": STORE_ID, "package": "settings/jinn-settings-store", "hash": store,
          "config": { "grants": [jinn_settings::STORE_CONTRACT],
                      "data": { "overlays": {} } } }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The settings provider EMITS `jinn:settings/changed` after a landed
    /// patch and `jinn:settings/refused` after a refused one; at pin
    /// `138fdce` (jinnd M2-K26 (e); FINDINGS #49) an emit is covered by the
    /// topic's own grant, so the provider entry carries both topics.
    #[test]
    fn the_settings_provider_is_granted_the_two_topics_it_emits() {
        let entries = settings_entries("abc", "def", &["cron-scheduler"]);
        let provider = &entries[0];
        assert_eq!(provider["id"], SETTINGS_ID);
        let grants = provider["config"]["grants"].as_array().expect("grants");
        for topic in [jinn_settings::CHANGED_TOPIC, jinn_settings::REFUSED_TOPIC] {
            assert!(
                grants.contains(&serde_json::json!(topic)),
                "the provider is granted {topic}, a topic it emits: {grants:?}"
            );
        }
    }
}
