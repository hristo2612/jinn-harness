//! The extension tier's profile ENTRY shape, the one source the `ui`
//! profile mounts, and the component-imports reading (one home per
//! fact); `main.rs` builds the provider. The proof-only variants (a
//! throwing source, a looping one, …) are the composition suite's own
//! fixtures, spelled there so the proofs stand without this crate
//! (§9.7 amendment 8(e)).

use std::path::Path;

use jinn_ext::{Budget, Origin, BOA_PACKAGE, CLOCK_CONTRACT};

/// The provider guest's crate name (its artifact basename).
pub const BOA_GUEST: &str = "jinn-ext-js-boa";
/// The operator's example from §6, mounted in the `ui` profile.
pub const GREEN_ID: &str = "ext-green";
/// See [`GREEN_ID`]: `hello` becomes `hello 🟢`.
pub const GREEN_SOURCE: &str = "(p) => ({ ...p, text: p.text + ' 🟢' })";
/// The budget the `ui` profile mounts `ext-green` under, in fuel: the
/// operator's example declares its bound now that the kernel honors one
/// (pin `b1dbe8f`, M2-K25). Sized from proof 2's measurement — one fold
/// is a fresh Boa context plus the source, well under a hundredth of
/// this — so the number bounds a runaway, never a fold.
pub const GREEN_BUDGET: Budget = Budget {
    fuel: 4_000_000_000,
};

/// The extension entry in §6's "Install" shape: `config.data` carries
/// the topics, the source, the origin and — when the entry declares one
/// — its per-delivery `budget`; `config.grants` is the topics (each
/// topic is its own grant name) plus the ONE host provider the engine
/// reads (`jinn:clock`); `injects` is absent — an extension injects no
/// service.
#[must_use]
pub fn ext_entry(
    id: &str,
    hash: &str,
    topics: &[&str],
    source: &str,
    origin: Origin,
    budget: Option<Budget>,
) -> serde_json::Value {
    let mut grants: Vec<serde_json::Value> = topics.iter().map(|t| serde_json::json!(t)).collect();
    grants.push(serde_json::json!(CLOCK_CONTRACT));
    let mut data = serde_json::json!({ "topics": topics, "source": source,
                                       "origin": origin.as_str() });
    if let Some(budget) = budget {
        data["budget"] = serde_json::json!({ "fuel": budget.fuel });
    }
    serde_json::json!({ "id": id, "package": BOA_PACKAGE, "hash": hash,
                        "config": { "grants": grants, "data": data } })
}

/// Builds and writes the Boa provider under `artifacts`; answers its pin
/// and its size in bytes.
#[must_use]
pub fn build(artifacts: &Path) -> (String, usize) {
    let (bytes, hash) = cron_kit::component("ext", BOA_GUEST);
    cron_kit::write_artifact(artifacts, BOA_GUEST, &bytes, &hash);
    (hash, bytes.len())
}

/// A component's TOP-LEVEL imports, by name (`jinn:plugin/types@0.12.0`
/// and so on), in declaration order — the §5.3 `imports` program. The
/// encoder nests a shim component whose own imports are the export glue
/// (`import-func-activate`, …); those are depth 1 and not the host
/// surface, so only depth 0 is read.
///
/// # Panics
///
/// If the bytes are not a component.
#[must_use]
pub fn component_imports(bytes: &[u8]) -> Vec<String> {
    use wasmparser::Payload;
    let mut names = Vec::new();
    let mut depth = 0usize;
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        match payload.expect("a component parses") {
            Payload::ModuleSection { .. } | Payload::ComponentSection { .. } => depth += 1,
            Payload::End(_) => depth = depth.saturating_sub(1),
            Payload::ComponentImportSection(section) if depth == 0 => {
                for import in section {
                    names.push(import.expect("an import parses").name.0.to_owned());
                }
            }
            _ => {}
        }
    }
    names
}
