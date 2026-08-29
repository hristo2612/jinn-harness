//! `jinn:engine` — the engines seam's service definition (phase 2.3).
//!
//! An engine is a coding agent behind a typed contract: a request goes in,
//! a run id comes back at once, and the run's progress arrives on the bus
//! as typed events until it exits with a status and usage. Nothing here
//! knows about a CLI, a protocol, or a vendor — a provider adds exactly
//! two things of its own, an argv and a stream codec.
//!
//! # One contract per engine id
//!
//! The kernel holds ONE provider slot per contract name (`broker.rs`: a
//! second provider of an occupied slot is refused, `DuplicateProvision`),
//! so N engines coexisting means N contract names. The seam's name is
//! therefore INSTANCED: `jinn:engine.<engine-id>` ([`engine_contract`]),
//! the engine id carried in the provider's own `config.data.engine` and
//! nowhere else. That one decision buys all three malleability proofs:
//!
//! - **Switch** — change an entry's `package`/`hash`, keep its id and its
//!   `engine`: a different implementation serves the same contract name
//!   and every consumer is untouched.
//! - **Coexistence** — a second entry with a different `engine` provides a
//!   different contract name; a consumer routes by engine id, which IS the
//!   contract name ([`engine_contract`]).
//! - **Extension** — a third provider is an entry and a grant, no change
//!   to this crate.
//!
//! The kernel has no shape for instance multiplicity of one contract;
//! FINDINGS.md #28 records the friction and what would retire the
//! encoding.
//!
//! # Secrets
//!
//! A request never carries secret material. [`RunRequest::secrets`] maps a
//! child environment variable to a [`SecretRef`] — the settings seam's
//! typed `{"$secret": "<key>"}` (one home per fact) — and the PROVIDER
//! resolves it through its granted `jinn:keystore` prefix at spawn time.
//! The profile document, the ledger and this crate hold key NAMES only.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub use jinn_settings::{is_secret_ref, SecretRef, SECRET_REF_KEY};

#[cfg(test)]
mod tests;

/// Additive JSON: fields a newer peer sends survive a round trip.
pub type Extensions = serde_json::Map<String, serde_json::Value>;

/// The answer envelope's version (additive within `0.x`).
pub const API_VERSION: &str = "0.1";

/// The seam's contract-name prefix. A full name is
/// `jinn:engine.<engine-id>` — see [`engine_contract`].
pub const ENGINE_CONTRACT_PREFIX: &str = "jinn:engine.";

/// The topic every provider publishes its run events on. One topic for
/// the whole seam: a consumer listens once and routes on the event's own
/// `engine` and `run-id`.
pub const EVENT_TOPIC: &str = "jinn:engine/event";

/// The settings namespace this definition owns (AGENTS.md standing order
/// 4): the operator's engine defaults.
pub const SETTINGS_NAMESPACE: &str = "engines";

/// Operation: what this provider is and what it can do.
pub const OP_DESCRIBE: &str = "describe";
/// Operation: start a run; answers a [`RunAccepted`] at once.
pub const OP_RUN: &str = "run";
/// Operation: one run's record so far.
pub const OP_RUN_GET: &str = "run-get";
/// Operation: cancel a run (kills its child).
pub const OP_CANCEL: &str = "cancel";

/// The contract name engine `id` is served under.
#[must_use]
pub fn engine_contract(id: &str) -> String {
    format!("{ENGINE_CONTRACT_PREFIX}{id}")
}

/// The engine id a contract name carries, or `None` when it is not this
/// seam's. An empty id is not an engine.
#[must_use]
pub fn engine_id_of(contract: &str) -> Option<&str> {
    contract
        .strip_prefix(ENGINE_CONTRACT_PREFIX)
        .filter(|id| !id.is_empty())
}

/// How hard the engine should think, when it has such a control.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Effort {
    Low,
    Medium,
    High,
}

/// What the run may do besides answer. Default-deny: an absent policy is
/// [`ToolMode::Denied`], never "whatever the CLI defaults to".
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolMode {
    /// No tool may run.
    #[default]
    Denied,
    /// Only the named tools may run.
    Allowlist,
}

/// The run's tool policy.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ToolPolicy {
    #[serde(default)]
    pub mode: ToolMode,
    /// The allowlist, meaningful only under [`ToolMode::Allowlist`].
    #[serde(default)]
    pub allow: Vec<String>,
}

impl ToolPolicy {
    /// The names the policy admits — empty under [`ToolMode::Denied`],
    /// whatever the allowlist says otherwise.
    #[must_use]
    pub fn admitted(&self) -> &[String] {
        match self.mode {
            ToolMode::Denied => &[],
            ToolMode::Allowlist => &self.allow,
        }
    }
}

/// What a run may spend. Both bounds are the provider's to enforce (R9);
/// [`Runs`] holds the accounting.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Budget {
    /// Wall clock, from the spawn. Past it the child is killed and the run
    /// exits `cancelled` with `reason: "budget"`.
    pub wall_ms: u64,
    /// Stdout bytes the provider will read before it stops reading and
    /// kills the child (the `jinn:process` bundle's own cap is 1 MiB for
    /// `run`; a long-lived spawn has no cap but this one).
    pub output_bytes: u64,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            wall_ms: 120_000,
            output_bytes: 1_048_576,
        }
    }
}

/// One run request. `engine` is the ROUTE (it names the contract, see
/// [`engine_contract`]); everything else is the run.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RunRequest {
    #[serde(default)]
    pub api_version: String,
    pub engine: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,
    /// The prompt. Delivered to the child on STDIN by every provider in
    /// this seam, never in argv — a prompt is personal data and argv is
    /// world-readable in the host's process table.
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default)]
    pub tools: ToolPolicy,
    #[serde(default)]
    pub budget: Budget,
    /// Child environment variable → keystore key. Names only.
    #[serde(default)]
    pub secrets: BTreeMap<String, SecretRef>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `run` answer: the run started, here is its handle.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RunAccepted {
    pub api_version: String,
    pub run_id: String,
    pub engine: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `{ "run-id": ... }` document. ONE shape names a run, and both
/// operations that take one — `cancel` and `run-get` — read it: a second
/// spelling would be an interop split between providers for no gain.
/// Both answer the run's [`RunRecord`].
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct CancelRequest {
    pub run_id: String,
}

/// See [`CancelRequest`] — the same document, named for the other
/// operation that reads it.
pub type RunGetRequest = CancelRequest;

/// What a provider says about itself (`describe`). A consumer builds its
/// engine list from these, never from a table of its own.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Description {
    pub api_version: String,
    /// The engine id this provider serves — the second half of its
    /// contract name.
    pub engine: String,
    /// The package serving it, for an operator reading a swap.
    pub provider: String,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    #[serde(default)]
    pub capabilities: Capabilities,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// What a provider can do, declared rather than assumed.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Capabilities {
    /// Emits `delta` events as the answer arrives.
    #[serde(default)]
    pub streaming: bool,
    /// Reports `tool-call` / `tool-result`.
    #[serde(default)]
    pub tool_calls: bool,
    /// `cancel` kills a live run.
    #[serde(default)]
    pub cancel: bool,
    /// Reports token usage on exit.
    #[serde(default)]
    pub usage: bool,
    /// Needs an external CLI on the host (so a run can be
    /// environment-gated); `false` for a self-contained provider.
    #[serde(default)]
    pub external_cli: bool,
}

/// What a run cost.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Usage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    /// Cost in micro-USD — an integer, so the record is exact and the type
    /// stays `Eq` (a float cost is a rounding argument nobody wins).
    #[serde(default)]
    pub cost_micro_usd: u64,
}

/// One run event, as it goes on the bus under [`EVENT_TOPIC`]. The
/// variants are the packet's: started, delta, tool-call, tool-result,
/// turn-end, exited.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Event {
    /// The child is spawned and the run is live.
    Started {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
    },
    /// A chunk of the answer.
    Delta { text: String },
    /// The engine called a tool.
    ToolCall {
        name: String,
        #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
        input: serde_json::Value,
    },
    /// A tool answered.
    ToolResult {
        name: String,
        #[serde(default)]
        ok: bool,
    },
    /// One turn finished.
    TurnEnd {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// The run is over: the child's status (negated signal number for a
    /// signal death, the `jinn:process` bundle's convention), what it
    /// cost, and whether the provider cut the stream on the budget.
    ///
    /// `error` is the case a status alone cannot express: the ENGINE
    /// reported a failed turn while its process still exited 0. Without
    /// it such a run reads as a clean success, which is the one lie this
    /// seam must not tell. `None` means the engine reported no failure —
    /// never "we did not look".
    Exited {
        status: i32,
        #[serde(default)]
        usage: Usage,
        #[serde(default)]
        truncated: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// The run ended because someone asked, or because a bound was hit.
    Cancelled { reason: String },
    /// A kind THIS version does not know, from a newer provider. Kept as
    /// a fact rather than a decode failure (R12's additivity on the bus):
    /// a listener orders and counts it, and never guesses what it meant.
    #[serde(other)]
    Unknown,
}

/// One run event with its attribution, as a listener receives it. The
/// envelope carries NO `extra`: `event` is an internally tagged enum, and
/// a second flattened map would swallow its own fields on the way back in
/// (the round trip proves it). Forward compatibility lives where it
/// belongs instead — in [`Event::Unknown`].
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RunEvent {
    pub api_version: String,
    pub engine: String,
    pub run_id: String,
    /// Per-run sequence from 0 — a listener orders and de-duplicates on
    /// this, never on arrival.
    pub seq: u64,
    #[serde(flatten)]
    pub event: Event,
}

/// Where a run is.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunState {
    /// Accepted, the child not yet spawned.
    #[default]
    Starting,
    /// The child is live.
    Running,
    /// The child exited on its own.
    Exited,
    /// Killed — by `cancel`, by a budget, or by a suspend.
    Cancelled,
    /// The provider could not carry the run (no CLI, a refused spawn).
    Failed,
}

impl RunState {
    /// Whether nothing more will happen to this run.
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Exited | Self::Cancelled | Self::Failed)
    }
}

/// One run's record: what a `run-get` answers and what the provider keeps.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RunRecord {
    pub api_version: String,
    pub run_id: String,
    pub engine: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub state: RunState,
    /// Every event emitted for this run, in sequence — the same records
    /// the bus carried, kept so a consumer that missed the stream can
    /// still read the run.
    #[serde(default)]
    pub events: Vec<Event>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<i32>,
    #[serde(default)]
    pub usage: Usage,
    /// The answer text, assembled from the deltas.
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub truncated: bool,
    /// The engine's own failure on a run whose process exited cleanly —
    /// see [`Event::Exited`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// Why an engine call was refused. Same discipline as the settings seam:
/// callers classify by CASE, never by folding a message.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    /// Malformed request.
    Invalid,
    /// No such run, or no such engine.
    NotFound,
    /// The kernel refused a grant, a scope, or an operation class.
    Refused,
    /// The provider cannot carry the run in THIS environment (its CLI is
    /// absent or unauthenticated) — an honest gate, never a faked run.
    Unavailable,
    /// The provider tried and the run failed.
    Failed,
}

/// A typed refusal.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct EngineError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl EngineError {
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            extra: Extensions::new(),
        }
    }

    /// The honest environment gate: the provider is mounted and correct,
    /// this box cannot run it.
    #[must_use]
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::Unavailable, message)
    }
}

/// One answer on the wire.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Answer {
    pub api_version: String,
    #[serde(flatten)]
    pub outcome: Outcome,
}

/// An answer's two shapes.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Ok(serde_json::Value),
    Error(EngineError),
}

impl Answer {
    /// # Panics
    ///
    /// Never in practice: the seam's own types all encode.
    #[must_use]
    pub fn ok<T: Serialize>(value: T) -> Self {
        Self {
            api_version: API_VERSION.to_owned(),
            outcome: Outcome::Ok(serde_json::to_value(value).expect("an answer encodes")),
        }
    }

    #[must_use]
    pub fn error(error: EngineError) -> Self {
        Self {
            api_version: API_VERSION.to_owned(),
            outcome: Outcome::Error(error),
        }
    }

    /// The wire bytes.
    ///
    /// # Panics
    ///
    /// Never in practice (see [`Answer::ok`]).
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("an answer encodes")
    }

    /// The `ok` payload, or the typed refusal.
    ///
    /// # Errors
    ///
    /// The provider's [`EngineError`].
    pub fn into_result(self) -> Result<serde_json::Value, EngineError> {
        match self.outcome {
            Outcome::Ok(value) => Ok(value),
            Outcome::Error(error) => Err(error),
        }
    }
}

/// One engine as the KERNEL's own view shows it: an entry that provides a
/// `jinn:engine.<id>` contract. See [`engines_in`].
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct EngineSlot {
    pub engine: String,
    pub contract: String,
    /// The profile entry serving it — what an operator edits to swap the
    /// implementation.
    pub entry: String,
}

/// Every engine live in a composition, from `(entry-id, provisions)` pairs
/// as `jinn:introspect` reports them — the kernel's knowledge, not a table
/// a consumer keeps. Sorted by engine id; an entry providing two engine
/// contracts contributes both.
#[must_use]
pub fn engines_in<'a, I, P>(entries: I) -> Vec<EngineSlot>
where
    I: IntoIterator<Item = (&'a str, P)>,
    P: IntoIterator<Item = &'a str>,
{
    let mut slots: Vec<EngineSlot> = entries
        .into_iter()
        .flat_map(|(entry, provisions)| {
            provisions
                .into_iter()
                .filter_map(move |contract| {
                    engine_id_of(contract).map(|engine| EngineSlot {
                        engine: engine.to_owned(),
                        contract: contract.to_owned(),
                        entry: entry.to_owned(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();
    slots.sort_by(|left, right| left.engine.cmp(&right.engine));
    slots
}

/// The run registry every provider keeps: run ids, event sequencing, the
/// assembled answer, budget accounting, terminal state. Pure — no host
/// call, no clock of its own (the caller passes the kernel's `now`), so
/// every provider's run semantics are ONE implementation, tested here.
#[derive(Debug, Default)]
pub struct Runs {
    engine: String,
    minted: u64,
    live: BTreeMap<String, Live>,
}

/// One run the registry holds.
#[derive(Debug)]
struct Live {
    record: RunRecord,
    seq: u64,
    started_ms: u64,
    budget: Budget,
    read_bytes: u64,
}

impl Runs {
    /// A registry for the engine `id` this provider serves.
    #[must_use]
    pub fn new(engine: impl Into<String>) -> Self {
        Self {
            engine: engine.into(),
            minted: 0,
            live: BTreeMap::new(),
        }
    }

    /// The engine id every run here belongs to.
    #[must_use]
    pub fn engine(&self) -> &str {
        &self.engine
    }

    /// Accepts a request: mints the run id and records it `starting`. Run
    /// ids are `<engine>-<n>` with `n` monotone in this incarnation — a
    /// restart mints from 0 again, which is why a record is per
    /// incarnation and a consumer holds the id it was answered.
    pub fn accept(&mut self, request: &RunRequest, now_ms: u64) -> RunAccepted {
        self.minted += 1;
        let run_id = format!("{}-{}", self.engine, self.minted);
        let record = RunRecord {
            api_version: API_VERSION.to_owned(),
            run_id: run_id.clone(),
            engine: self.engine.clone(),
            model: request.model.clone(),
            state: RunState::Starting,
            ..RunRecord::default()
        };
        self.live.insert(
            run_id.clone(),
            Live {
                record,
                seq: 0,
                started_ms: now_ms,
                budget: request.budget,
                read_bytes: 0,
            },
        );
        RunAccepted {
            api_version: API_VERSION.to_owned(),
            run_id,
            engine: self.engine.clone(),
            model: request.model.clone(),
            extra: Extensions::new(),
        }
    }

    /// Records `event` against `run_id` and answers the bus record to
    /// emit. `None` for an unknown run — a provider never emits for a run
    /// it does not hold.
    pub fn record(&mut self, run_id: &str, event: Event) -> Option<RunEvent> {
        let live = self.live.get_mut(run_id)?;
        let seq = live.seq;
        live.seq += 1;
        match &event {
            Event::Started { model } => {
                live.record.state = RunState::Running;
                if live.record.model.is_none() {
                    live.record.model.clone_from(model);
                }
            }
            Event::Delta { text } => live.record.text.push_str(text),
            Event::TurnEnd { text } => {
                if live.record.text.is_empty() {
                    if let Some(text) = text {
                        live.record.text.clone_from(text);
                    }
                }
            }
            Event::Exited {
                status,
                usage,
                truncated,
                error,
            } => {
                live.record.state = RunState::Exited;
                live.record.status = Some(*status);
                live.record.usage = *usage;
                live.record.truncated |= *truncated;
                live.record.error.clone_from(error);
            }
            Event::Cancelled { .. } => live.record.state = RunState::Cancelled,
            Event::ToolCall { .. } | Event::ToolResult { .. } | Event::Unknown => {}
        }
        live.record.events.push(event.clone());
        Some(RunEvent {
            api_version: API_VERSION.to_owned(),
            engine: self.engine.clone(),
            run_id: run_id.to_owned(),
            seq,
            event,
        })
    }

    /// Marks a run failed before it ever ran (a refused spawn, an absent
    /// CLI). Answers the bus record.
    pub fn fail(&mut self, run_id: &str, reason: impl Into<String>) -> Option<RunEvent> {
        let emitted = self.record(
            run_id,
            Event::Cancelled {
                reason: reason.into(),
            },
        );
        if let Some(live) = self.live.get_mut(run_id) {
            live.record.state = RunState::Failed;
        }
        emitted
    }

    /// Accounts `bytes` read for `run_id`; `true` once the output budget
    /// is spent (the provider stops reading and kills the child).
    pub fn read(&mut self, run_id: &str, bytes: u64) -> bool {
        let Some(live) = self.live.get_mut(run_id) else {
            return false;
        };
        live.read_bytes = live.read_bytes.saturating_add(bytes);
        if live.read_bytes > live.budget.output_bytes {
            live.record.truncated = true;
            return true;
        }
        false
    }

    /// Whether `run_id` has outlived its wall budget at `now_ms`.
    #[must_use]
    pub fn over_wall_budget(&self, run_id: &str, now_ms: u64) -> bool {
        self.live
            .get(run_id)
            .is_some_and(|live| now_ms.saturating_sub(live.started_ms) > live.budget.wall_ms)
    }

    /// One run's record.
    #[must_use]
    pub fn get(&self, run_id: &str) -> Option<&RunRecord> {
        self.live.get(run_id).map(|live| &live.record)
    }

    /// Every run id that is not finished — what a poll wake iterates and
    /// what a suspend cancels.
    #[must_use]
    pub fn live_ids(&self) -> Vec<String> {
        self.live
            .iter()
            .filter(|(_, live)| !live.record.state.is_terminal())
            .map(|(run_id, _)| run_id.clone())
            .collect()
    }

    /// How many runs the registry holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.live.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }

    /// Drops the oldest finished records once more than `keep` are held —
    /// a provider's memory is bounded (R9), and a run record is evidence
    /// the ledger already carries.
    pub fn retain_recent(&mut self, keep: usize) {
        let finished: Vec<String> = self
            .live
            .iter()
            .filter(|(_, live)| live.record.state.is_terminal())
            .map(|(run_id, _)| run_id.clone())
            .collect();
        let excess = finished.len().saturating_sub(keep);
        for run_id in finished.into_iter().take(excess) {
            self.live.remove(&run_id);
        }
    }
}
