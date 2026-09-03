//! The `jinn:ext` service definition: the extension entry's config
//! schema, the activation law's names, and the two JS programs an engine
//! provider evaluates — the activation self-test and the per-delivery
//! fold. Not a service anyone calls (an extension is a LISTENER; nothing
//! provides `jinn:ext`): the home of the shape every engine provider and
//! the kit compile in. The prose law is this crate's README. Pure types;
//! serde, serde_json and sha2 only.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[cfg(test)]
mod tests;

/// The seam's name. Provided by nothing: the definition is types.
pub const CONTRACT: &str = "jinn:ext";
/// The first engine provider's package (its artifact basename).
pub const BOA_PACKAGE: &str = "ext/jinn-ext-js-boa";
/// The ONE kernel host provider an engine reads: the clock, once per
/// delivery, so a JS engine has a `Date` (§5.4 lesson 1). A host
/// provider is never a guest, so the read can close no wait cycle.
pub const CLOCK_CONTRACT: &str = "jinn:clock";
/// See [`CLOCK_CONTRACT`]; answer = 8-byte LE unix milliseconds.
pub const OP_NOW: &str = "now";
/// The activation breadcrumbs, registered as effects in this order — the
/// activation discipline until FINDINGS.md #38 closes: a fiber that fails
/// between two of them says so by which was written last (§5.4).
pub const BREADCRUMBS: [&str; 4] = [
    "activate entered",
    "config parsed",
    "js context built",
    "js evaluated",
];
/// The fifth row: WHAT CODE RAN, on the record (Law 2; §8 ruling 1).
pub const SOURCE_BREADCRUMB_PREFIX: &str = "source sha256:";

/// Who wrote the source — the operator's declaration on the entry
/// (constitution 05's `[provenance] origin`, restated for data), shown
/// on the plugins page and read by nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    Agent,
    Human,
}

impl Origin {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Human => "human",
        }
    }
}

/// The entry's `config.data`. CLOSED: an unknown field is an activation
/// fault (R3; the settings seam's closed-surface law). No `budget` field
/// — nothing at this pin can honor one (KG-2).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct ExtConfig {
    /// The topics the extension listens on, each of which must ALSO be
    /// in the entry's `config.grants` (the grant is the authority; this
    /// is the listener's statement of intent).
    pub topics: Vec<String>,
    /// The operator's JS: one expression evaluating to a function of the
    /// payload.
    pub source: String,
    pub origin: Origin,
}

/// Reads the config the kernel hands `activate`: the closed schema, and
/// a topic listed twice is a per-entry fault (one listen per topic; the
/// kit never writes it, the guest refuses it).
///
/// # Errors
///
/// Malformed JSON, an unknown field, a missing one, or a duplicate topic.
pub fn parse_config(bytes: &[u8]) -> Result<ExtConfig, String> {
    let config: ExtConfig =
        serde_json::from_slice(bytes).map_err(|error| format!("malformed config: {error}"))?;
    for (index, topic) in config.topics.iter().enumerate() {
        if config.topics[..index].contains(topic) {
            return Err(format!("topic {topic:?} is listed twice"));
        }
    }
    Ok(config)
}

/// `source sha256:<hex>` for the source as configured.
#[must_use]
pub fn source_breadcrumb(source: &str) -> String {
    format!(
        "{SOURCE_BREADCRUMB_PREFIX}{:x}",
        Sha256::digest(source.as_bytes())
    )
}

/// The activation self-test: evaluates the source ONCE and answers
/// whether it is a function. A syntax error fails the evaluation; a
/// value that is not a function is a failed fiber on the record, never
/// a silent no-op listener (R11).
#[must_use]
pub fn self_test(source: &str) -> String {
    format!("typeof (\n{source}\n) === \"function\"")
}

/// The per-delivery program: the payload parsed, the source applied, the
/// answer folded. `undefined` answers the EMPTY string (the kernel treats
/// empty output as "leave the payload unchanged"); anything that is not
/// an object throws, which the provider answers as its contained fault
/// (R9: recorded, the walk continues).
///
/// # Errors
///
/// The payload is not UTF-8 (a JSON payload always is).
pub fn delivery(source: &str, payload: &[u8]) -> Result<String, String> {
    let payload = std::str::from_utf8(payload).map_err(|error| format!("payload: {error}"))?;
    // A JSON string literal is a valid JS string literal, so the payload
    // rides as `JSON.parse(<literal>)` — never spliced as code.
    let literal = serde_json::to_string(payload).expect("a string encodes");
    Ok(format!(
        "(function (__payload) {{\n  var __answer = (\n{source}\n)(__payload);\n  if (__answer === undefined) return \"\";\n  if (__answer === null || typeof __answer !== \"object\") throw new TypeError(\"the extension answered a \" + typeof __answer + \", not an object\");\n  return JSON.stringify(__answer);\n}})(JSON.parse({literal}))"
    ))
}
