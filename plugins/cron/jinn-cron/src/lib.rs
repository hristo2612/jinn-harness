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
/// The kernel's clock capability (kernel-pin/contracts/jinn-clock): time
/// enters the seam through it — a `now` read at activation and the wakes
/// of one periodic alarm.
pub const CLOCK_CONTRACT: &str = "jinn:clock";
/// The topic the kernel delivers alarm wakes on (`handle-event`), payload
/// = 8-byte little-endian unix milliseconds.
pub const WAKE_TOPIC: &str = "jinn:clock/alarm";
/// The scheduler's default alarm period (settings `tick-ms`): how often
/// the firing law is consulted, hence how late a fire may land.
pub const DEFAULT_TICK_MS: u64 = 60_000;
/// Introspection operation: the live job table.
pub const OP_JOBS: &str = "jobs";
/// Introspection operation: the bounded run history.
pub const OP_HISTORY: &str = "history";
/// The scheduler's persisted state, under its `jinn:fs` scope.
pub const STATE_PATH: &str = "cron/state.json";
/// The bounded run history, under its `jinn:fs` scope.
pub const HISTORY_PATH: &str = "cron/history.json";
/// Where a corrupt persisted document is preserved before the scheduler
/// starts fresh (contract §Persistence honesty).
pub const QUARANTINE_DIR: &str = "cron/quarantine";
/// History keeps the newest this-many records (an operational window; the
/// ledger is the archive).
pub const HISTORY_CAP: usize = 500;

/// Unknown sibling fields, preserved across a decode → encode round trip
/// (R12 additivity: this reader carries what a newer writer said, it never
/// strips it).
pub type Extensions = serde_json::Map<String, serde_json::Value>;

/// One validated, schedulable job — what [`parse_config`] produces and the
/// firing law consumes. Internal normal form, not the wire schema.
#[derive(Clone, Debug, PartialEq)]
pub struct JobSpec {
    pub id: String,
    /// Schedule spec v0.1: a fixed period anchored at the Unix epoch;
    /// boundaries are `k * every-ms`, k >= 1.
    pub every_ms: u64,
    /// Where this job's fire events go.
    pub topic: String,
    /// Opaque JSON handed through to every fire event.
    pub payload: serde_json::Value,
}

/// One job entry as written in config (the wire schema). The schedule is
/// an OPEN position: v0.1 recognizes `every-ms`; future variants (e.g. a
/// calendar expression) are additive sibling fields. An entry whose
/// schedule this reader does not recognize degrades to a per-entry
/// `config-fault` — contained, recorded, never a document rejection.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct JobConfig {
    #[serde(default)]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub every_ms: Option<u64>,
    #[serde(default)]
    pub topic: String,
    #[serde(default)]
    pub payload: serde_json::Value,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The scheduler's whole settings subtree.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct CronConfig {
    #[serde(default)]
    pub jobs: Vec<JobConfig>,
    /// The periodic alarm's period (additive, 0.1.0); absent =
    /// [`DEFAULT_TICK_MS`]. Must be no finer than the granted `jinn:clock`
    /// floor — the kernel refuses a finer period and activation fails loudly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tick_ms: Option<u64>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One consultation of the firing law: the kernel clock's instant plus an
/// edition. `seq` is an edition marker for operators, not a guard — the
/// firing law's boundary accounting is the only replay/rewind protection
/// (contract §Time).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TickPayload {
    /// The per-activation edition: `0` is the activate-time plan (the
    /// clock's `now` read), then one per alarm wake.
    pub seq: u64,
    /// Milliseconds since the Unix epoch, as the kernel clock read it.
    pub now_ms: u64,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// One fire event, emitted on the job's topic (mode `serial`, selector
/// `all`).
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct FirePayload {
    pub job: String,
    /// The boundary that fired.
    pub scheduled_ms: u64,
    /// The kernel clock when it fired.
    pub now_ms: u64,
    /// Boundaries skipped (recorded, never fired) before this one.
    pub missed_before: u64,
    pub tick_seq: u64,
    pub payload: serde_json::Value,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// How one run record settled. Externally tagged, kebab-case; variants are
/// additive within 0.x at EVERY nesting level: an outcome tag this reader
/// does not recognize decodes as [`RunOutcome::Unrecognized`] carrying the
/// raw value, and unknown fields INSIDE a known variant's payload ride its
/// flattened extensions — never rejected, never stripped. (The one unit
/// variant, `schedule-started`, has no payload to extend; a newer version
/// reshaping it into an object lands in the carrier, still verbatim.)
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunOutcome {
    /// The boundary fired; `answers` counts settled listener answers.
    /// Zero answers is a visible duty gap, not an error.
    Fired {
        answers: u64,
        #[serde(flatten)]
        extra: Extensions,
    },
    /// Boundaries skipped under the firing law: recorded, never fired.
    #[serde(rename_all = "kebab-case")]
    Skipped {
        boundaries: u64,
        first_ms: u64,
        last_ms: u64,
        #[serde(flatten)]
        extra: Extensions,
    },
    /// A job with no state started its schedule at this tick (law #4).
    ScheduleStarted,
    /// A config entry was excluded; the schedule never saw it.
    ConfigFault {
        detail: String,
        #[serde(flatten)]
        extra: Extensions,
    },
    /// The fire's emit crossing was refused by the kernel.
    EmitFailed {
        detail: String,
        #[serde(flatten)]
        extra: Extensions,
    },
    /// A persisted document was present but undecodable; the original is
    /// preserved under the quarantine path named in `detail` (contract
    /// §Persistence honesty).
    StateFault {
        path: String,
        detail: String,
        #[serde(flatten)]
        extra: Extensions,
    },
    /// An outcome written by a newer contract version: carried verbatim.
    #[serde(untagged)]
    Unrecognized(serde_json::Value),
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
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The scheduler's persisted state: per job, the newest boundary already
/// processed. Serialized to [`STATE_PATH`] and carried across hot-swaps as
/// the snapshot blob.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SchedulerState {
    #[serde(default)]
    pub last: BTreeMap<String, u64>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// What [`parse_config`] makes of a settings subtree.
#[derive(Clone, Debug, PartialEq)]
pub struct ParsedConfig {
    /// Schedulable jobs, in config order.
    pub jobs: Vec<JobSpec>,
    /// One fault per excluded entry (contract: recorded, never silent).
    pub faults: Vec<String>,
    /// The alarm period to request (`tick-ms`, defaulted).
    pub tick_ms: u64,
}

/// Parses the scheduler's settings subtree: schedulable jobs in config
/// order, plus one fault string per excluded entry (missing/unrecognized
/// schedule, zero period, empty topic, empty or duplicate id), plus the
/// alarm period. A malformed document is an `Err` — that is an activation
/// failure, not a config fault.
///
/// # Errors
///
/// The document is not valid JSON for [`CronConfig`].
pub fn parse_config(bytes: &[u8]) -> Result<ParsedConfig, String> {
    let config: CronConfig =
        serde_json::from_slice(bytes).map_err(|error| format!("malformed cron config: {error}"))?;
    let tick_ms = config.tick_ms.unwrap_or(DEFAULT_TICK_MS);
    let mut jobs: Vec<JobSpec> = Vec::new();
    let mut faults = Vec::new();
    for job in config.jobs {
        if job.id.is_empty() {
            faults.push("job with no id".to_owned());
            continue;
        }
        // Ids name run-record paths: keep them path-safe by construction.
        if !job
            .id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            faults.push(format!("job {:?}: id must be [A-Za-z0-9_-]", job.id));
            continue;
        }
        let Some(every_ms) = job.every_ms else {
            faults.push(format!(
                "job {:?}: no schedule this reader recognizes (v0.1 knows every-ms)",
                job.id
            ));
            continue;
        };
        if every_ms == 0 {
            faults.push(format!("job {:?}: every-ms 0 is not a schedule", job.id));
        } else if job.topic.is_empty() {
            faults.push(format!("job {:?}: empty topic", job.id));
        } else if jobs.iter().any(|kept| kept.id == job.id) {
            faults.push(format!("job {:?}: duplicate id", job.id));
        } else {
            jobs.push(JobSpec {
                id: job.id,
                every_ms,
                topic: job.topic,
                payload: job.payload,
            });
        }
    }
    Ok(ParsedConfig {
        jobs,
        faults,
        tick_ms,
    })
}

/// The per-fire run-record path: one identifiable granted-write per fire,
/// labeled by job and boundary — the fire's outcome document beside the
/// kernel's `DispatchTrace` audit line (contract §Run history).
#[must_use]
pub fn run_record_path(job: &str, scheduled_ms: u64) -> String {
    format!("cron/runs/{job}/{scheduled_ms}.json")
}

/// Whether a `jinn:fs` read refusal reports genuine absence (the world's
/// fs interface has no typed not-found — FINDINGS.md #3 — so absence is
/// classified from the provider's message; anything else is NOT absence
/// and must not silently default).
#[must_use]
pub fn read_error_is_absence(message: &str) -> bool {
    message.contains("os error 2") || message.contains("No such file")
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
            extra: Extensions::new(),
        }
    }

    #[test]
    fn parse_config_keeps_wellformed_jobs_in_order() {
        let bytes = br#"{ "jobs": [
            { "id": "a", "every-ms": 60000, "topic": "cron:a" },
            { "id": "b", "every-ms": 1000, "topic": "cron:b", "payload": {"x": 1} }
        ]}"#;
        let ParsedConfig { jobs, faults, .. } = parse_config(bytes).expect("well-formed");
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
        let ParsedConfig { jobs, faults, .. } =
            parse_config(bytes).expect("document is valid json");
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
        let parsed = parse_config(b"{}").expect("empty config");
        assert!(parsed.jobs.is_empty());
        assert!(parsed.faults.is_empty());
        assert_eq!(parsed.tick_ms, DEFAULT_TICK_MS, "the alarm period defaults");
    }

    #[test]
    fn tick_ms_is_an_additive_setting() {
        let parsed = parse_config(br#"{ "tick-ms": 500, "jobs": [] }"#).expect("parses");
        assert_eq!(parsed.tick_ms, 500);
        let config: CronConfig = serde_json::from_slice(br#"{ "jobs": [] }"#).expect("parses");
        assert!(
            !serde_json::to_string(&config)
                .expect("encodes")
                .contains("tick-ms"),
            "an absent period stays absent on the wire"
        );
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
            extra: Extensions::new(),
        };
        let text = serde_json::to_string(&fire).expect("encodes");
        assert!(text.contains("scheduled-ms"), "{text}");
        assert!(text.contains("missed-before"), "{text}");
        let back: FirePayload = serde_json::from_str(&text).expect("decodes");
        assert_eq!(back, fire);
        let outcome = serde_json::to_string(&RunOutcome::Fired {
            answers: 1,
            extra: Extensions::new(),
        })
        .expect("encodes");
        assert!(outcome.contains("fired"), "{outcome}");
    }

    #[test]
    fn an_extended_payload_round_trips_with_unknown_fields_preserved() {
        // A newer writer added sibling fields; this reader carries them
        // (R12 additivity), it never strips or rejects them.
        let text = r#"{ "seq": 4, "now-ms": 9000, "zone": "UTC", "grid": { "v": 2 } }"#;
        let tick: TickPayload = serde_json::from_str(text).expect("decodes");
        assert_eq!(tick.extra["zone"], "UTC");
        let back = serde_json::to_value(&tick).expect("encodes");
        assert_eq!(back["zone"], "UTC");
        assert_eq!(back["grid"]["v"], 2);
    }

    #[test]
    fn unknown_fields_inside_a_known_outcome_variant_are_preserved() {
        // The round-2 verifier's exact probe: a newer writer added
        // `duration-ms` INSIDE `outcome.fired`. The variant must still
        // decode as Fired (answers stays typed) AND the extension must
        // survive the round trip.
        let text = r#"{"job":"health","scheduled-ms":80000,"now-ms":90000,"tick-seq":7,
                      "outcome":{"fired":{"answers":1,"duration-ms":12}}}"#;
        let record: RunRecord = serde_json::from_str(text).expect("decodes");
        let RunOutcome::Fired { answers, .. } = &record.outcome else {
            panic!("still a recognized fired outcome: {:?}", record.outcome);
        };
        assert_eq!(*answers, 1);
        let back = serde_json::to_value(&record).expect("encodes");
        assert_eq!(back["outcome"]["fired"]["answers"], 1);
        assert_eq!(back["outcome"]["fired"]["duration-ms"], 12, "{back}");
    }

    #[test]
    fn deeply_nested_extensions_inside_a_variant_are_preserved() {
        let text = r#"{"job":"j","scheduled-ms":1,"now-ms":2,"tick-seq":3,
                      "outcome":{"skipped":{"boundaries":2,"first-ms":10,"last-ms":20,
                                            "zone":{"tz":"UTC","deep":{"x":1}}}}}"#;
        let record: RunRecord = serde_json::from_str(text).expect("decodes");
        let RunOutcome::Skipped { boundaries, .. } = &record.outcome else {
            panic!("still a recognized skipped outcome: {:?}", record.outcome);
        };
        assert_eq!(*boundaries, 2);
        let back = serde_json::to_value(&record).expect("encodes");
        assert_eq!(back["outcome"]["skipped"]["zone"]["deep"]["x"], 1, "{back}");
        assert_eq!(back["outcome"]["skipped"]["first-ms"], 10);
    }

    #[test]
    fn an_unrecognized_outcome_round_trips_verbatim() {
        let text = r#"{ "job": "j", "scheduled-ms": 1, "now-ms": 2, "tick-seq": 3,
                       "outcome": { "paused": { "until-ms": 5 } } }"#;
        let record: RunRecord = serde_json::from_str(text).expect("a newer outcome decodes");
        let RunOutcome::Unrecognized(raw) = &record.outcome else {
            panic!("carried as unrecognized: {:?}", record.outcome);
        };
        assert_eq!(raw["paused"]["until-ms"], 5);
        let back = serde_json::to_value(&record).expect("encodes");
        assert_eq!(back["outcome"]["paused"]["until-ms"], 5);
    }

    #[test]
    fn a_schedule_this_reader_does_not_know_degrades_to_a_fault() {
        // A newer writer's calendar job beside a v0.1 job: the known job
        // schedules, the unknown one is one contained config fault.
        let bytes = br#"{ "jobs": [
            { "id": "cal", "cron": "0 * * * *", "topic": "t" },
            { "id": "ok", "every-ms": 5, "topic": "t" }
        ]}"#;
        let ParsedConfig { jobs, faults, .. } = parse_config(bytes).expect("document accepted");
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, "ok");
        assert_eq!(faults.len(), 1);
        assert!(
            faults[0].contains("cal") && faults[0].contains("schedule"),
            "{faults:?}"
        );
    }

    #[test]
    fn history_lines_round_trip_one_record_per_line() {
        // The append lane: one JSON record per line, newline-terminated,
        // so every fire is one O(1) append and the log decodes line by
        // line (contract §Run history).
        let mut log = Vec::new();
        log.extend(history_line(&record(1)));
        log.extend(history_line(&record(2)));
        assert_eq!(log.iter().filter(|byte| **byte == b'\n').count(), 2);
        let decoded = parse_history_lines(&log).expect("decodes");
        assert_eq!(decoded, vec![record(1), record(2)]);
        assert!(parse_history_lines(b"").expect("empty log").is_empty());
        assert!(
            parse_history_lines(b"\n\n").expect("blank lines are not records").is_empty()
        );
    }

    #[test]
    fn a_torn_history_line_is_reported_by_number_not_swallowed() {
        let mut log = history_line(&record(1));
        log.extend(b"{\"job\":\"j\",\"sched");
        let refused = parse_history_lines(&log).expect_err("a torn tail is not a record");
        assert!(refused.contains("line 2"), "{refused}");
    }

    #[test]
    fn a_legacy_history_array_decodes_as_the_window_seed() {
        // The pre-0.2.0 document (`cron/history.json`, one JSON array,
        // rewritten per fire) is read once as the seed of the window and
        // never written again.
        let legacy = serde_json::to_vec(&vec![record(1), record(2)]).expect("encodes");
        assert_eq!(parse_legacy_history(&legacy).expect("decodes").len(), 2);
        assert!(parse_legacy_history(b"nope").is_err());
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
