//! The codex provider's wire: `codex exec --json` JSONL in, the engines
//! seam's [`Event`]s out, plus the argv a run is spawned with. Pure — no
//! host call, no clock, no I/O — so the provider's whole vendor-specific
//! surface is two functions tested on the host, and the guest that
//! embeds it (which builds for wasm32 only, and therefore cannot run a
//! test of its own) adds nothing but kernel plumbing.
//!
//! # Chunks, not lines
//!
//! [`Decoder::feed`] is the whole reading API because that is exactly how
//! the provider reads: `jinn:process`'s `read` answers whatever bytes the
//! pipe's bounded buffer holds, which splits lines at arbitrary offsets.
//! The decoder buffers the incomplete tail and emits only on a complete
//! line, so a stream cut anywhere decodes identically (there is a test
//! over every split point). [`Decoder::flush`] closes the honest gap at
//! EOF: a last line the child never terminated.
//!
//! # Evidence
//!
//! Every mapping below is either CAPTURED — read off a live
//! `codex exec --json --sandbox read-only --skip-git-repo-check
//! --ephemeral -` run — or INFERRED from the CLI's documented item
//! vocabulary. Nothing is guessed:
//!
//! | line | mapping | evidence |
//! |---|---|---|
//! | `thread.started` | [`Event::Started`] | captured |
//! | `turn.started` | nothing | captured |
//! | `item.started` of a tool item | [`Event::ToolCall`] | captured (`command_execution`) |
//! | `item.completed` of a tool item | [`Event::ToolResult`] | captured (`command_execution`) |
//! | `item.completed` of `agent_message` | [`Event::TurnEnd`] | captured |
//! | `item.completed` of `error` | [`Event::ToolResult`] `ok: false` + [`Decoder::errors`] | captured |
//! | `turn.completed` | [`Decoder::usage`] | captured |
//! | `file-change`, `mcp_tool_call`, `web_search`, `patch_apply`, `todo_list` | as `command_execution` | INFERRED |
//! | `reasoning` | nothing | INFERRED |
//! | anything else | nothing | — |
//!
//! # What this codec deliberately does NOT do
//!
//! - **No [`Event::Exited`].** An exit belongs to the PROCESS, not to its
//!   stream: the provider reads it from `jinn:process`'s `wait` status and
//!   emits it there. A codec that invented one would be reporting a fact
//!   it cannot know.
//! - **No [`Event::Delta`].** Codex reports one COMPLETED
//!   `agent_message` rather than token deltas, so the run streams no
//!   partial answer. `Runs::record` already assembles a record's `text`
//!   from a [`Event::TurnEnd`] when no delta ever came, so faking deltas
//!   would buy nothing and lie about the CLI's capability — the provider
//!   declares `streaming: false` for the same reason.
//! - **No thread id, ever.** `thread.started` carries codex's session
//!   identifier. It is personal data; it is dropped at the decode and
//!   never reaches an event, a record, or the ledger.
//! - **No tool output on the bus.** A `command_execution` item's
//!   `aggregated_output` is the child's data and can be arbitrarily
//!   large; [`Event::ToolResult`] carries only `ok`, and the result-shaped
//!   fields are stripped from a [`Event::ToolCall`]'s `input` (which
//!   keeps the REQUEST fields, verbatim).
//!
//! # `error` items are not necessarily fatal
//!
//! Captured evidence: a run emitted
//! `{"type":"item.completed","item":{"type":"error","message":"…deprecated…"}}`
//! BEFORE `turn.started` and then completed normally with an
//! `agent_message`. So an `error` item is a DIAGNOSTIC, not a verdict on
//! the run — the provider must not fail a run on one. It is never
//! dropped: it goes on the bus as a failed result named `error`, and its
//! message (which can quote the model's own text) is kept off the bus in
//! [`Decoder::errors`], where the provider can read it.

use std::collections::BTreeSet;

use jinn_engine::{Event, ToolMode, ToolPolicy, Usage};

#[cfg(test)]
mod tests;

/// The largest single JSONL line the decoder will buffer before it gives
/// up on that line (R9: a codec's memory is bounded by construction, not
/// by the child's good behaviour). The oversized line is dropped whole —
/// never a truncated prefix decoded as if it were the line — and the
/// stream resumes at the next newline.
pub const LINE_CAP: usize = 1024 * 1024;

/// The item types this codec reports as tool activity. `command_execution`
/// is CAPTURED; the rest are INFERRED from the CLI's documented item
/// vocabulary and are decoded by the same rules — the fields that happen
/// to be present, never a field invented for them.
const TOOL_ITEMS: &[&str] = &[
    "command_execution",
    "file_change",
    "mcp_tool_call",
    "patch_apply",
    "todo_list",
    "web_search",
];

/// Item fields that describe the RESULT rather than the request; they are
/// kept out of a [`Event::ToolCall`]'s `input`.
const RESULT_FIELDS: &[&str] = &["aggregated_output", "exit_code", "id", "status", "type"];

/// The `codex exec --json` stream, decoded.
#[derive(Debug, Default)]
pub struct Decoder {
    /// The incomplete line carried between chunks.
    tail: Vec<u8>,
    /// Set while an oversized line is being discarded up to its newline.
    dropping: bool,
    usage: Usage,
    errors: Vec<String>,
    /// Tool items seen on `item.started`, so a completion knows whether
    /// its call was already reported.
    started: BTreeSet<String>,
}

impl Decoder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Decodes the next chunk of the child's stdout. Emits one
    /// [`Event`] per complete line that carries one; a blank line, a
    /// non-JSON line, an unknown `type`, and an incomplete tail all emit
    /// nothing and never panic.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Event> {
        let mut events = Vec::new();
        for byte in bytes {
            if *byte == b'\n' {
                let line = std::mem::take(&mut self.tail);
                if self.dropping {
                    self.dropping = false;
                } else {
                    events.extend(self.line(&line));
                }
                continue;
            }
            if self.dropping {
                continue;
            }
            self.tail.push(*byte);
            if self.tail.len() > LINE_CAP {
                self.tail.clear();
                self.dropping = true;
            }
        }
        events
    }

    /// Decodes whatever the child left unterminated at EOF. Idempotent:
    /// a second call has nothing to decode.
    pub fn flush(&mut self) -> Vec<Event> {
        let line = std::mem::take(&mut self.tail);
        if self.dropping {
            self.dropping = false;
            return Vec::new();
        }
        self.line(&line)
    }

    /// What the run has cost so far, as the seam counts it.
    ///
    /// Codex reports one `turn.completed` per turn and `codex exec` runs
    /// a single turn, so the last one seen IS the run's. `cached_input_tokens`,
    /// `cache_write_input_tokens` and `reasoning_output_tokens` have no
    /// home in [`Usage`] and are NOT folded into the two counters that do
    /// — a sum of counters whose overlap is undocumented would be a
    /// guess. `cost_micro_usd` stays 0: codex reports tokens, never a
    /// price, and this provider does not carry a price table.
    #[must_use]
    pub fn usage(&self) -> Usage {
        self.usage.clone()
    }

    /// Every `error` item's message, in order. See the crate doc: these
    /// are diagnostics, not a verdict on the run.
    #[must_use]
    pub fn errors(&self) -> &[String] {
        &self.errors
    }

    /// One complete line.
    fn line(&mut self, line: &[u8]) -> Vec<Event> {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(line) else {
            return Vec::new();
        };
        match value.get("type").and_then(serde_json::Value::as_str) {
            // The thread id is a session identifier and stops here.
            Some("thread.started") => vec![Event::Started { model: None }],
            Some("turn.completed") => {
                self.usage = usage_of(value.get("usage"));
                Vec::new()
            }
            Some("item.started") => self.item_started(value.get("item")),
            Some("item.completed") => self.item_completed(value.get("item")),
            // `turn.started`, and every type a newer codex may add.
            _ => Vec::new(),
        }
    }

    fn item_started(&mut self, item: Option<&serde_json::Value>) -> Vec<Event> {
        let Some((item, kind)) = typed_item(item) else {
            return Vec::new();
        };
        if !TOOL_ITEMS.contains(&kind) {
            return Vec::new();
        }
        if let Some(id) = item.get("id").and_then(serde_json::Value::as_str) {
            self.started.insert(id.to_owned());
        }
        vec![tool_call(item, kind)]
    }

    fn item_completed(&mut self, item: Option<&serde_json::Value>) -> Vec<Event> {
        let Some((item, kind)) = typed_item(item) else {
            return Vec::new();
        };
        match kind {
            "agent_message" => vec![Event::TurnEnd {
                text: item
                    .get("text")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
            }],
            "error" => {
                if let Some(message) = item.get("message").and_then(serde_json::Value::as_str) {
                    self.errors.push(message.to_owned());
                }
                vec![Event::ToolResult {
                    name: "error".to_owned(),
                    ok: false,
                }]
            }
            // Thinking is not the answer: a delta here would be appended
            // to the run record's `text` and corrupt it.
            "reasoning" => Vec::new(),
            kind if TOOL_ITEMS.contains(&kind) => {
                let seen = item
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|id| self.started.remove(id));
                let mut events = Vec::new();
                // A completion whose start we never saw still reports the
                // call it is the result of — the completed item carries
                // the request fields.
                if !seen {
                    events.push(tool_call(item, kind));
                }
                events.push(Event::ToolResult {
                    name: kind.to_owned(),
                    ok: item_ok(item),
                });
                events
            }
            _ => Vec::new(),
        }
    }
}

/// One item object and its `type`, or `None` when the line does not carry
/// a typed item.
fn typed_item(
    item: Option<&serde_json::Value>,
) -> Option<(&serde_json::Map<String, serde_json::Value>, &str)> {
    let item = item?.as_object()?;
    let kind = item.get("type")?.as_str()?;
    Some((item, kind))
}

/// The call an item describes: its REQUEST fields, verbatim, with the
/// result-shaped ones (and the item's own bookkeeping) removed. `input`
/// is null when nothing is left — a call with no arguments, not an
/// invented one.
fn tool_call(item: &serde_json::Map<String, serde_json::Value>, kind: &str) -> Event {
    let input: serde_json::Map<String, serde_json::Value> = item
        .iter()
        .filter(|(key, _)| !RESULT_FIELDS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    Event::ToolCall {
        name: kind.to_owned(),
        input: if input.is_empty() {
            serde_json::Value::Null
        } else {
            serde_json::Value::Object(input)
        },
    }
}

/// Whether a completed tool item succeeded: its `exit_code` when it has
/// one, else its `status`, else — nothing said, nothing wrong.
fn item_ok(item: &serde_json::Map<String, serde_json::Value>) -> bool {
    if let Some(code) = item.get("exit_code").and_then(serde_json::Value::as_i64) {
        return code == 0;
    }
    match item.get("status").and_then(serde_json::Value::as_str) {
        Some(status) => matches!(status, "completed" | "success"),
        None => true,
    }
}

/// The seam's counters out of codex's usage object. An absent counter is
/// zero, never a decode failure (R12: a newer codex adding counters must
/// not break an older reader).
fn usage_of(usage: Option<&serde_json::Value>) -> Usage {
    let counter = |name: &str| {
        usage
            .and_then(|usage| usage.get(name))
            .and_then(serde_json::Value::as_u64)
            .unwrap_or_default()
    };
    Usage {
        input_tokens: counter("input_tokens"),
        output_tokens: counter("output_tokens"),
        cost_micro_usd: 0,
        ..Usage::default()
    }
}

/// The argv a run is spawned with — the provider's ONLY vendor knowledge
/// besides the codec, kept pure so it is testable on the host.
///
/// `exec --json` is the non-interactive JSONL lane; `--skip-git-repo-check`
/// lets a run start outside a repository; `--ephemeral` keeps codex from
/// persisting session files; the trailing `-` makes it read the prompt
/// from STDIN. The prompt is NEVER an argv element: argv is world-readable
/// in the host's process table and a prompt is personal data.
///
/// The executable itself is not here. It comes from the entry's
/// `config.data.command` — a machine-specific path never belongs in
/// source.
///
/// # Sandbox: a floor, not a translation
///
/// Codex's sandbox modes are COARSER than the seam's per-tool allowlist:
/// [`ToolMode::Denied`] maps to `read-only` and [`ToolMode::Allowlist`] to
/// `workspace-write`, whatever the allowlist names. The mapping is
/// therefore a FLOOR — the policy's ceiling on what a run may touch —
/// and not a faithful rendering of the named tools, which codex has no
/// flag to express. `danger-full-access`,
/// `--dangerously-bypass-approvals-and-sandbox` and
/// `--allow-dangerously-skip-permissions` are unreachable from here under
/// every policy (there is a test).
#[must_use]
pub fn argv(model: Option<&str>, tools: &ToolPolicy) -> Vec<String> {
    let mut argv = vec![
        "exec".to_owned(),
        "--json".to_owned(),
        "--skip-git-repo-check".to_owned(),
        "--ephemeral".to_owned(),
    ];
    if let Some(model) = model {
        argv.push("--model".to_owned());
        argv.push(model.to_owned());
    }
    argv.push("--sandbox".to_owned());
    argv.push(
        match tools.mode {
            ToolMode::Denied => "read-only",
            ToolMode::Allowlist => "workspace-write",
        }
        .to_owned(),
    );
    // Last, and positional: the prompt arrives on stdin.
    argv.push("-".to_owned());
    argv
}
