//! The extension tier's profile ENTRY shape, the sources the suite and
//! the `ui` profile mount, and the component-imports reading (one home
//! per fact); `main.rs` builds the provider.

use std::path::Path;

use jinn_ext::{Origin, BOA_PACKAGE, CLOCK_CONTRACT};

/// The provider guest's crate name (its artifact basename).
pub const BOA_GUEST: &str = "jinn-ext-js-boa";
/// The operator's example from §6, mounted in the `ui` profile.
pub const GREEN_ID: &str = "ext-green";
/// See [`GREEN_ID`]: `hello` becomes `hello 🟢`.
pub const GREEN_SOURCE: &str = "(p) => ({ ...p, text: p.text + ' 🟢' })";
/// A second extension appending a different marker (proof 3).
pub const BLUE_ID: &str = "ext-blue";
/// See [`BLUE_ID`].
pub const BLUE_SOURCE: &str = "(p) => ({ ...p, text: p.text + ' 🔵' })";
/// A source that throws on every delivery (proof 4).
pub const THROWING_ID: &str = "ext-throwing";
/// See [`THROWING_ID`].
pub const THROWING_SOURCE: &str = "(p) => { throw new Error('the throwing extension'); }";
/// A source that returns `undefined`: the pass-through case (proof 4).
pub const UNDEFINED_SOURCE: &str = "(p) => undefined";
/// A source that does not parse: a failed fiber on the record (proof 8).
pub const BROKEN_SOURCE: &str = "(p) => { this is not javascript";
/// A source that loops forever on delivery (proof 7).
pub const LOOPING_SOURCE: &str = "(p) => { while (true) {} }";

/// A source whose ACTIVATION is slow by construction: a bounded counting
/// loop under fuel, run when the source expression is evaluated, then
/// the fold — never `while(true)` (proof 5's restart window).
#[must_use]
pub fn slow_source(iterations: u64, marker: &str) -> String {
    format!("(function () {{ var i = 0; while (i < {iterations}) i++; return (p) => ({{ ...p, text: p.text + ' {marker}' }}); }})()")
}

/// The extension entry in §6's "Install" shape: `config.data` carries
/// the topics, the source and the origin; `config.grants` is the topics
/// (each topic is its own grant name) plus the ONE host provider the
/// engine reads (`jinn:clock`); `injects` is absent — an extension
/// injects no service.
#[must_use]
pub fn ext_entry(
    id: &str,
    hash: &str,
    topics: &[&str],
    source: &str,
    origin: Origin,
) -> serde_json::Value {
    let mut grants: Vec<serde_json::Value> = topics.iter().map(|t| serde_json::json!(t)).collect();
    grants.push(serde_json::json!(CLOCK_CONTRACT));
    serde_json::json!({ "id": id, "package": BOA_PACKAGE, "hash": hash,
                        "config": { "grants": grants,
                                    "data": { "topics": topics, "source": source,
                                              "origin": origin.as_str() } } })
}

/// Builds and writes the Boa provider under `artifacts`; answers its pin
/// and its size in bytes.
#[must_use]
pub fn build(artifacts: &Path) -> (String, usize) {
    let (bytes, hash) = cron_kit::component("ext", BOA_GUEST);
    cron_kit::write_artifact(artifacts, BOA_GUEST, &bytes, &hash);
    (hash, bytes.len())
}

/// A component's TOP-LEVEL imports, by name (`jinn:plugin/types@0.10.0`
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
