//! The `jinn:cron` service definition: names, payload schemas, and the
//! firing law as pure functions. The prose law lives in this crate's
//! README; this code is its schema. Everything on the seam is UTF-8 JSON
//! with kebab-case keys.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

mod schedule;

pub use schedule::{plan_tick, TickPlan};

/// The contract the scheduler provides.
pub const CRON_CONTRACT: &str = "jinn:cron";
/// The topic time enters on.
pub const TICK_TOPIC: &str = "jinn:cron/tick";
/// Introspection operation: the live job table.
pub const OP_JOBS: &str = "jobs";
/// Introspection operation: the bounded run history.
pub const OP_HISTORY: &str = "history";
/// The scheduler's persisted state, under its `jinn:fs` scope.
pub const STATE_PATH: &str = "cron/state.json";
/// The bounded run history, under its `jinn:fs` scope.
pub const HISTORY_PATH: &str = "cron/history.json";
/// History keeps the newest this-many records (an operational window; the
/// ledger is the archive).
pub const HISTORY_CAP: usize = 500;

/// One job entry of the scheduler's settings namespace.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct JobSpec {
    pub id: String,
    /// Schedule spec v0.1: a fixed period anchored at the Unix epoch;
    /// boundaries are `k * every-ms`, k >= 1.
    pub every_ms: u64,
    /// Where this job's fire events go.
    pub topic: String,
    /// Opaque JSON handed through to every fire event.
    #[serde(default)]
    pub payload: serde_json::Value,
}

/// The scheduler's whole settings subtree.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct CronConfig {
    #[serde(default)]
    pub jobs: Vec<JobSpec>,
}

/// One tick on [`TICK_TOPIC`]: the seam's only time source.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TickPayload {
    /// The tick source's monotonic edition; `0` is the boot seed and is
    /// never dispatched.
    pub seq: u64,
    /// Wall-clock milliseconds since the Unix epoch.
    pub now_ms: u64,
}

/// One fire event, emitted on the job's topic (mode `serial`, selector
/// `all`).
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct FirePayload {
    pub job: String,
    /// The boundary that fired.
    pub scheduled_ms: u64,
    /// The tick clock when it fired.
    pub now_ms: u64,
    /// Boundaries skipped (recorded, never fired) before this one.
    pub missed_before: u64,
    pub tick_seq: u64,
    pub payload: serde_json::Value,
}

/// How one run record settled. Externally tagged, kebab-case; variants are
/// additive within 0.x.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunOutcome {
    /// The boundary fired; `answers` counts settled listener answers.
    /// Zero answers is a visible duty gap, not an error.
    Fired { answers: u64 },
    /// Boundaries skipped under the firing law: recorded, never fired.
    #[serde(rename_all = "kebab-case")]
    Skipped {
        boundaries: u64,
        first_ms: u64,
        last_ms: u64,
    },
    /// A job with no state started its schedule at this tick (law #4).
    ScheduleStarted,
    /// A config entry was excluded; the schedule never saw it.
    ConfigFault { detail: String },
    /// The fire's emit crossing was refused by the kernel.
    EmitFailed { detail: String },
}

/// One run-history record.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct RunRecord {
    pub job: String,
    pub scheduled_ms: u64,
    pub now_ms: u64,
    pub tick_seq: u64,
    pub outcome: RunOutcome,
}

/// The scheduler's persisted state: per job, the newest boundary already
/// processed. Serialized to [`STATE_PATH`] and carried across hot-swaps as
/// the snapshot blob.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SchedulerState {
    #[serde(default)]
    pub last: BTreeMap<String, u64>,
}

/// Parses the scheduler's settings subtree: well-formed jobs in config
/// order, plus one fault string per excluded entry (duplicate id, zero
/// period, empty topic). A malformed document is an `Err` — that is an
/// activation failure, not a config fault.
///
/// # Errors
///
/// The document is not valid JSON for [`CronConfig`].
pub fn parse_config(bytes: &[u8]) -> Result<(Vec<JobSpec>, Vec<String>), String> {
    let config: CronConfig =
        serde_json::from_slice(bytes).map_err(|error| format!("malformed cron config: {error}"))?;
    let mut jobs: Vec<JobSpec> = Vec::new();
    let mut faults = Vec::new();
    for job in config.jobs {
        if job.every_ms == 0 {
            faults.push(format!("job {:?}: every-ms 0 is not a schedule", job.id));
        } else if job.topic.is_empty() {
            faults.push(format!("job {:?}: empty topic", job.id));
        } else if jobs.iter().any(|kept| kept.id == job.id) {
            faults.push(format!("job {:?}: duplicate id", job.id));
        } else {
            jobs.push(job);
        }
    }
    Ok((jobs, faults))
}

/// Appends `new` to `history`, keeping the newest `cap` records.
#[must_use]
pub fn bounded_history(
    mut history: Vec<RunRecord>,
    new: impl IntoIterator<Item = RunRecord>,
    cap: usize,
) -> Vec<RunRecord> {
    history.extend(new);
    let excess = history.len().saturating_sub(cap);
    history.drain(..excess);
    history
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(tick_seq: u64) -> RunRecord {
        RunRecord {
            job: "j".into(),
            scheduled_ms: 0,
            now_ms: 0,
            tick_seq,
            outcome: RunOutcome::ScheduleStarted,
        }
    }

    #[test]
    fn parse_config_keeps_wellformed_jobs_in_order() {
        let bytes = br#"{ "jobs": [
            { "id": "a", "every-ms": 60000, "topic": "cron:a" },
            { "id": "b", "every-ms": 1000, "topic": "cron:b", "payload": {"x": 1} }
        ]}"#;
        let (jobs, faults) = parse_config(bytes).expect("well-formed");
        assert!(faults.is_empty(), "{faults:?}");
        assert_eq!(jobs.len(), 2);
        assert_eq!(jobs[0].id, "a");
        assert_eq!(jobs[1].payload, serde_json::json!({"x": 1}));
    }

    #[test]
    fn parse_config_excludes_faulted_entries_and_reports_each() {
        let bytes = br#"{ "jobs": [
            { "id": "ok", "every-ms": 5, "topic": "t" },
            { "id": "zero", "every-ms": 0, "topic": "t" },
            { "id": "ok", "every-ms": 5, "topic": "t" },
            { "id": "silent", "every-ms": 5, "topic": "" }
        ]}"#;
        let (jobs, faults) = parse_config(bytes).expect("document is valid json");
        assert_eq!(jobs.len(), 1, "only the first well-formed entry survives");
        assert_eq!(faults.len(), 3, "{faults:?}");
        assert!(faults[0].contains("zero"));
        assert!(faults[1].contains("duplicate"));
        assert!(faults[2].contains("topic"));
    }

    #[test]
    fn parse_config_refuses_a_malformed_document() {
        assert!(parse_config(b"not json").is_err());
    }

    #[test]
    fn parse_config_accepts_an_empty_document_as_no_jobs() {
        let (jobs, faults) = parse_config(b"{}").expect("empty config");
        assert!(jobs.is_empty());
        assert!(faults.is_empty());
    }

    #[test]
    fn bounded_history_keeps_the_newest_records() {
        let history: Vec<RunRecord> = (0..4).map(record).collect();
        let bounded = bounded_history(history, [record(4), record(5)], 3);
        let seqs: Vec<u64> = bounded.iter().map(|r| r.tick_seq).collect();
        assert_eq!(seqs, vec![3, 4, 5]);
    }

    #[test]
    fn payloads_round_trip_with_kebab_keys() {
        let fire = FirePayload {
            job: "health".into(),
            scheduled_ms: 60_000,
            now_ms: 61_000,
            missed_before: 2,
            tick_seq: 9,
            payload: serde_json::Value::Null,
        };
        let text = serde_json::to_string(&fire).expect("encodes");
        assert!(text.contains("scheduled-ms"), "{text}");
        assert!(text.contains("missed-before"), "{text}");
        let back: FirePayload = serde_json::from_str(&text).expect("decodes");
        assert_eq!(back, fire);
        let outcome = serde_json::to_string(&RunOutcome::Fired { answers: 1 }).expect("encodes");
        assert!(outcome.contains("fired"), "{outcome}");
    }

    #[test]
    fn state_round_trips_and_defaults_empty() {
        let mut state = SchedulerState::default();
        state.last.insert("a".into(), 120_000);
        let bytes = serde_json::to_vec(&state).expect("encodes");
        let back: SchedulerState = serde_json::from_slice(&bytes).expect("decodes");
        assert_eq!(back, state);
        let empty: SchedulerState = serde_json::from_slice(b"{}").expect("empty");
        assert!(empty.last.is_empty());
    }
}
