//! The `claude` provider's own two things (see `jinn-engine`'s crate
//! doc): the CLI's ARGV and its STREAM CODEC. Nothing here calls a host
//! or knows a path — the binary's location is the profile entry's
//! (`config.data.command`), and every decision this crate makes is a pure
//! function of bytes it was handed.
//!
//! # The stream
//!
//! `claude -p --output-format stream-json --verbose` writes NDJSON on
//! stdout: one JSON object per line, arriving from a pipe in CHUNKS that
//! respect no line boundary. [`Decoder`] is therefore fed BYTES
//! ([`Decoder::feed`]), buffers an incomplete tail, and answers only on
//! complete lines — the shape the provider's `jinn:process` read loop
//! hands it. A line it does not understand — blank, not JSON, a `type`
//! from a newer CLI — yields NOTHING: never a panic, never an invented
//! event.
//!
//! The kinds that carry meaning, and only these:
//!
//! | line | event |
//! |---|---|
//! | `system` / `init` | [`Event::Started`] with the run's model |
//! | `assistant` text block | one [`Event::Delta`] per block |
//! | `assistant` `tool_use` block | [`Event::ToolCall`] |
//! | `user` `tool_result` block | [`Event::ToolResult`] |
//! | `result` | [`Event::TurnEnd`] |
//!
//! A `tool_result` line carries no tool NAME, only the `tool_use_id` of
//! the call it answers, so the decoder keeps that correlation
//! ([`Decoder::tools`]) and falls back to an EMPTY name rather than
//! inventing one.
//!
//! # What the codec deliberately does not do
//!
//! It never emits [`Event::Exited`]. The exit belongs to the PROCESS, not
//! to the stream: the provider emits it from the real `wait` status. What
//! the `result` line carried for that event is exposed instead —
//! [`Decoder::usage`], [`Decoder::failed`], [`Decoder::result_subtype`].

use jinn_engine::{Event, ToolPolicy, Usage};
use serde::Deserialize;
use serde_json::Value;

#[cfg(test)]
mod tests;

/// The longest single NDJSON line the decoder will hold. Past it the line
/// is dropped WHOLE (a truncated prefix is not JSON anyway) so a runaway
/// child costs bounded guest memory, never an unbounded buffer (R9).
pub const LINE_CAP: usize = 1 << 20;

/// The most `tool_use_id → name` correlations held at once. A run that
/// calls more tools than this loses the NAME of the oldest calls, which
/// the decoder then reports honestly empty — bounded memory beats a
/// prettier label.
pub const TOOL_CAP: usize = 1024;

/// The poll cadence a profile entry gets when it names none — the
/// kernel's own alarm resolution floor.
pub const DEFAULT_POLL_MS: u64 = 250;

/// The fastest poll an entry may ask for. A zero (or near-zero) period
/// is a busy loop against the ledger, so the floor is a bound, not a
/// suggestion.
pub const MIN_POLL_MS: u64 = 50;

/// How many finished run records a provider keeps by default.
pub const DEFAULT_KEEP_RUNS: usize = 8;

/// The CLI's argv for one run: the stream lane, the model when one is
/// chosen, and the tool policy.
///
/// `--allowedTools` is VARIADIC in the CLI ("comma or space-separated
/// list of tool names"), so it must be LAST — anything after it would be
/// swallowed as another tool name. [`ToolMode::Denied`] and an empty
/// allowlist both come out as the one empty argument, which admits
/// nothing.
///
/// The PROMPT is never here. Bare `-p` makes the CLI read it from stdin,
/// which is the whole point: argv is world-readable in the host's process
/// table and a prompt is personal data (`RunRequest::prompt`).
///
/// [`ToolMode::Denied`]: jinn_engine::ToolMode::Denied
#[must_use]
pub fn argv(model: Option<&str>, tools: &ToolPolicy) -> Vec<String> {
    let mut args = vec![
        "-p".to_owned(),
        "--output-format".to_owned(),
        "stream-json".to_owned(),
        "--verbose".to_owned(),
    ];
    if let Some(model) = model {
        args.push("--model".to_owned());
        args.push(model.to_owned());
    }
    args.push("--allowedTools".to_owned());
    args.push(tools.admitted().join(","));
    args
}

/// One provider entry's `config.data`: every machine-specific value the
/// guest needs, held in the PROFILE and nowhere else — no path, no model
/// list and no cadence is compiled into the source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProviderConfig {
    /// The engine id this entry serves; it names the contract
    /// (`jinn:engine.<id>`).
    pub engine: String,
    /// Absolute path to the CLI binary.
    pub command: String,
    /// The models the provider advertises in `describe`.
    pub models: Vec<String>,
    /// The model a request that names none runs on.
    pub default_model: Option<String>,
    /// The wake cadence while a run is live (never while idle).
    pub poll_ms: u64,
    /// Finished run records kept for `run-get`.
    pub keep_runs: usize,
}

/// The entry as it is written, before the bounds are applied.
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
struct RawConfig {
    #[serde(default)]
    engine: String,
    #[serde(default)]
    command: String,
    #[serde(default)]
    models: Vec<String>,
    #[serde(default)]
    default_model: Option<String>,
    #[serde(default)]
    poll_ms: Option<u64>,
    #[serde(default)]
    keep_runs: Option<usize>,
}

/// Reads one entry's `config.data`.
///
/// # Errors
///
/// The config is not JSON, or it names no engine or no CLI — an entry
/// that cannot say what it serves or what it runs is not a provider, and
/// activation fails loudly rather than serving a contract it invented.
pub fn parse_config(bytes: &[u8]) -> Result<ProviderConfig, String> {
    let raw: RawConfig =
        serde_json::from_slice(bytes).map_err(|error| format!("malformed config: {error}"))?;
    if raw.engine.is_empty() {
        return Err("config.data.engine is required (it names the contract)".to_owned());
    }
    if raw.command.is_empty() {
        return Err("config.data.command is required (the CLI's absolute path)".to_owned());
    }
    Ok(ProviderConfig {
        engine: raw.engine,
        command: raw.command,
        models: raw.models,
        default_model: raw.default_model,
        poll_ms: raw.poll_ms.unwrap_or(DEFAULT_POLL_MS).max(MIN_POLL_MS),
        keep_runs: raw.keep_runs.unwrap_or(DEFAULT_KEEP_RUNS),
    })
}

/// The stream codec: bytes in, the seam's events out.
#[derive(Debug, Default)]
pub struct Decoder {
    /// The incomplete tail of the last chunk.
    buffer: Vec<u8>,
    /// Dropping the remainder of a line that outgrew [`LINE_CAP`].
    discarding: bool,
    /// `tool_use_id → tool name`, so a `tool_result` can be named.
    tools: Vec<(String, String)>,
    /// What the `result` line said this run cost.
    usage: Usage,
    /// The `result` line's `subtype`, once one arrived.
    subtype: Option<String>,
    /// Whether that result reported a failure.
    failed: bool,
}

impl Decoder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Absorbs one chunk from the pipe and answers the events its
    /// COMPLETE lines carried. An incomplete tail is held for the next
    /// chunk; [`Decoder::flush`] is the last word at EOF.
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Event> {
        self.buffer.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some(at) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=at).collect();
            if self.discarding {
                // The tail of an over-long line; it ends here.
                self.discarding = false;
                continue;
            }
            events.extend(self.decode_bytes(&line[..line.len() - 1]));
        }
        if !self.discarding && self.buffer.len() > LINE_CAP {
            self.discarding = true;
            self.buffer.clear();
        }
        events
    }

    /// Decodes whatever is buffered as a final line — a pipe may end
    /// without a newline. Idempotent: the buffer is consumed.
    pub fn flush(&mut self) -> Vec<Event> {
        let line = std::mem::take(&mut self.buffer);
        if std::mem::replace(&mut self.discarding, false) {
            return Vec::new();
        }
        self.decode_bytes(&line)
    }

    /// One line's events. Public because a line IS the unit of this
    /// format: a caller that already has whole lines needs no buffering.
    pub fn decode_line(&mut self, line: &str) -> Vec<Event> {
        let line = line.trim();
        if line.is_empty() {
            return Vec::new();
        }
        // Not JSON, or JSON that is not an object with a `type`: a fact
        // this version does not know, and nothing is the honest answer.
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            return Vec::new();
        };
        match value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "system" => Self::system(&value),
            "assistant" => self.assistant(&value),
            "user" => self.user(&value),
            "result" => self.result(&value),
            _ => Vec::new(),
        }
    }

    /// What the `result` line reported this run cost. Zero until one
    /// arrives — the provider attaches it to the [`Event::Exited`] it
    /// emits from the real `wait` status.
    #[must_use]
    pub fn usage(&self) -> Usage {
        self.usage.clone()
    }

    /// Whether the `result` line reported a failed turn (`is_error`, or
    /// any `subtype` other than `success`). A failed turn is still a
    /// [`Event::TurnEnd`]; this is how the provider sees past the text.
    #[must_use]
    pub fn failed(&self) -> bool {
        self.failed
    }

    /// The `result` line's `subtype` verbatim, once one arrived.
    #[must_use]
    pub fn result_subtype(&self) -> Option<&str> {
        self.subtype.as_deref()
    }

    /// Bytes of an incomplete line currently held (the memory bound in
    /// [`LINE_CAP`] is about this number).
    #[must_use]
    pub fn buffered(&self) -> usize {
        self.buffer.len()
    }

    /// One line's bytes: a `\r\n` terminator is ordinary, and bytes that
    /// are not UTF-8 are not JSON.
    fn decode_bytes(&mut self, line: &[u8]) -> Vec<Event> {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        match std::str::from_utf8(line) {
            Ok(text) => self.decode_line(text),
            Err(_) => Vec::new(),
        }
    }

    /// `system` lines: only `init` is the run starting. `hook_started`,
    /// `hook_response`, `thinking_tokens` and every other subtype are the
    /// CLI talking to itself.
    fn system(value: &Value) -> Vec<Event> {
        if value.get("subtype").and_then(Value::as_str) != Some("init") {
            return Vec::new();
        }
        vec![Event::started(
            value
                .get("model")
                .and_then(Value::as_str)
                .map(str::to_owned),
        )]
    }

    /// `assistant` lines: one [`Event::Delta`] per `text` block, one
    /// [`Event::ToolCall`] per `tool_use` block, and nothing at all for a
    /// `thinking` block — reasoning is not the answer.
    fn assistant(&mut self, value: &Value) -> Vec<Event> {
        let mut events = Vec::new();
        for block in content(value) {
            match block
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or_default()
            {
                "text" => events.push(Event::delta(
                    block
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned(),
                )),
                "tool_use" => {
                    let name = block
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned();
                    if let Some(id) = block.get("id").and_then(Value::as_str) {
                        self.remember(id, &name);
                    }
                    events.push(Event::tool_call(
                        name,
                        block.get("input").cloned().unwrap_or(Value::Null),
                    ));
                }
                _ => {}
            }
        }
        events
    }

    /// `user` lines: the tool results coming back. The block carries the
    /// `tool_use_id` and no name, so the name comes from the call this
    /// decoder saw — or stays empty, honestly.
    fn user(&mut self, value: &Value) -> Vec<Event> {
        let mut events = Vec::new();
        for block in content(value) {
            if block.get("type").and_then(Value::as_str) != Some("tool_result") {
                continue;
            }
            let name = block
                .get("tool_use_id")
                .and_then(Value::as_str)
                .and_then(|id| self.recall(id))
                .unwrap_or_default();
            events.push(Event::tool_result(
                name,
                !block
                    .get("is_error")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            ));
        }
        events
    }

    /// The `result` line: the turn's end, plus everything the PROVIDER
    /// needs for the exit event it will emit itself.
    fn result(&mut self, value: &Value) -> Vec<Event> {
        let subtype = value
            .get("subtype")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        self.failed = subtype != "success"
            || value
                .get("is_error")
                .and_then(Value::as_bool)
                .unwrap_or(false);
        self.subtype = Some(subtype);
        let usage = value.get("usage");
        let count = |key: &str| {
            usage
                .and_then(|usage| usage.get(key))
                .and_then(Value::as_u64)
                .unwrap_or(0)
        };
        self.usage = Usage {
            // A cache read and a cache write are still input tokens the
            // run consumed, and the seam has ONE home for input tokens:
            // reporting only the uncached counter would understate a run
            // by orders of magnitude.
            input_tokens: count("input_tokens")
                .saturating_add(count("cache_read_input_tokens"))
                .saturating_add(count("cache_creation_input_tokens")),
            output_tokens: count("output_tokens"),
            cost_micro_usd: micro_usd(value.get("total_cost_usd").and_then(Value::as_f64)),
            ..Usage::default()
        };
        vec![Event::turn_end(
            value
                .get("result")
                .and_then(Value::as_str)
                .map(str::to_owned),
        )]
    }

    /// Correlates one call id with its tool name, bounded by [`TOOL_CAP`].
    fn remember(&mut self, id: &str, name: &str) {
        if self.tools.len() >= TOOL_CAP {
            self.tools.remove(0);
        }
        self.tools.push((id.to_owned(), name.to_owned()));
    }

    /// The name of the call `id` answered — consumed, since a result
    /// arrives once and the map stays small.
    fn recall(&mut self, id: &str) -> Option<String> {
        let at = self.tools.iter().position(|(held, _)| held == id)?;
        Some(self.tools.remove(at).1)
    }
}

/// A message line's content blocks, or nothing when the line has none.
fn content(value: &Value) -> &[Value] {
    value
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// The ONE place a float cost becomes an integer (`Usage::cost_micro_usd`
/// is a `u64` so a record is exact and comparable). Rounded to nearest,
/// never truncated — half a micro-dollar is not worth a silent loss — and
/// a cost that is negative or not finite is not a cost.
fn micro_usd(dollars: Option<f64>) -> u64 {
    let Some(dollars) = dollars else { return 0 };
    if !dollars.is_finite() || dollars <= 0.0 {
        return 0;
    }
    let micro = (dollars * 1_000_000.0).round();
    if micro >= u64::MAX as f64 {
        u64::MAX
    } else {
        micro as u64
    }
}
