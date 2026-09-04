//! §5.3's `imports` program, committed as the kit's own test: the Boa
//! provider's component imports are EXACTLY the four interfaces of the
//! plugin world — `types`, `effects`, `events`, `services` — and nothing
//! else. No WASI, no entropy, no clock import beyond the one contract
//! call the guest makes over `services` under a `jinn:clock` grant: the
//! JS inside has no host calls, by construction and on the record here.
//! Builds the guest for wasm32-unknown-unknown, so it costs a guest build
//! per run, which is what makes the assertion worth having.

use ext_kit::{component_imports, BOA_GUEST};

#[test]
fn the_boa_provider_imports_exactly_the_four_plugin_world_interfaces() {
    let (bytes, hash) = cron_kit::component("ext", BOA_GUEST);
    let imports = component_imports(&bytes);
    let mut sorted = imports.clone();
    sorted.sort();
    assert_eq!(
        sorted,
        [
            "jinn:plugin/effects@0.11.0",
            "jinn:plugin/events@0.11.0",
            "jinn:plugin/services@0.11.0",
            "jinn:plugin/types@0.11.0",
        ],
        "the component's imports, in declaration order: {imports:?}"
    );
    eprintln!(
        "ext-kit imports: {} bytes, sha256 {hash}, imports {imports:?}",
        bytes.len()
    );
}
