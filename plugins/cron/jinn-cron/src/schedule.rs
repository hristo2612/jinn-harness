//! The firing law as one pure function (README §The firing law): given the
//! job table, the persisted state, and one tick, decide what fires, what is
//! recorded, and what the state becomes. No IO, no clock — the tick is the
//! only time there is.

use crate::{FirePayload, JobSpec, RunOutcome, RunRecord, SchedulerState, TickPayload};

/// One tick's decision: fires to emit (their run records are appended by
/// the caller once the emit settles), records already settled (started /
/// skipped), and the state to persist BEFORE emitting (crash law: a torn
/// tick loses a record, never doubles a fire).
#[derive(Debug, Default, PartialEq)]
pub struct TickPlan {
    pub fires: Vec<FirePayload>,
    pub records: Vec<RunRecord>,
    pub state: SchedulerState,
}

/// Applies the firing law for one tick. Dropped jobs leave the state; new
/// jobs start their schedule (law #4).
#[must_use]
pub fn plan_tick(jobs: &[JobSpec], state: &SchedulerState, tick: &TickPayload) -> TickPlan {
    let mut plan = TickPlan::default();
    for job in jobs {
        let period = job.every_ms;
        let newest_due = (tick.now_ms / period) * period;
        let record = |outcome| RunRecord {
            job: job.id.clone(),
            scheduled_ms: newest_due,
            now_ms: tick.now_ms,
            tick_seq: tick.seq,
            outcome,
        };
        let Some(&last) = state.last.get(&job.id) else {
            // Law #4: no state — the schedule starts here, recorded.
            plan.state.last.insert(job.id.clone(), newest_due);
            plan.records.push(record(RunOutcome::ScheduleStarted));
            continue;
        };
        if newest_due <= last {
            plan.state.last.insert(job.id.clone(), last);
            continue;
        }
        // Law #1/#2: the newest due boundary fires; earlier due boundaries
        // are one skipped record.
        let skipped = (newest_due - last) / period - 1;
        if skipped > 0 {
            plan.records.push(RunRecord {
                scheduled_ms: last + period,
                outcome: RunOutcome::Skipped {
                    boundaries: skipped,
                    first_ms: last + period,
                    last_ms: newest_due - period,
                },
                ..record(RunOutcome::ScheduleStarted)
            });
        }
        plan.fires.push(FirePayload {
            job: job.id.clone(),
            scheduled_ms: newest_due,
            now_ms: tick.now_ms,
            missed_before: skipped,
            tick_seq: tick.seq,
            payload: job.payload.clone(),
        });
        plan.state.last.insert(job.id.clone(), newest_due);
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(id: &str, every_ms: u64) -> JobSpec {
        JobSpec {
            id: id.into(),
            every_ms,
            topic: format!("cron:{id}"),
            payload: serde_json::Value::Null,
        }
    }

    fn tick(seq: u64, now_ms: u64) -> TickPayload {
        TickPayload { seq, now_ms }
    }

    fn seen(state: &[(&str, u64)]) -> SchedulerState {
        let mut built = SchedulerState::default();
        for (id, last) in state {
            built.last.insert((*id).into(), *last);
        }
        built
    }

    #[test]
    fn a_new_job_starts_its_schedule_without_firing() {
        let plan = plan_tick(
            &[job("a", 60_000)],
            &SchedulerState::default(),
            &tick(1, 90_000),
        );
        assert!(plan.fires.is_empty(), "law #4: no fire on a fresh schedule");
        assert_eq!(plan.records.len(), 1);
        assert_eq!(plan.records[0].outcome, RunOutcome::ScheduleStarted);
        assert_eq!(
            plan.state.last["a"], 60_000,
            "anchored at the elapsed boundary"
        );
    }

    #[test]
    fn no_new_boundary_means_no_fire_and_stable_state() {
        let state = seen(&[("a", 60_000)]);
        let plan = plan_tick(&[job("a", 60_000)], &state, &tick(2, 110_000));
        assert!(plan.fires.is_empty());
        assert!(plan.records.is_empty());
        assert_eq!(plan.state, state);
    }

    #[test]
    fn one_elapsed_boundary_fires_exactly_once() {
        let plan = plan_tick(
            &[job("a", 60_000)],
            &seen(&[("a", 60_000)]),
            &tick(3, 125_000),
        );
        assert_eq!(plan.records, vec![], "nothing skipped");
        assert_eq!(plan.fires.len(), 1);
        let fire = &plan.fires[0];
        assert_eq!(
            (
                fire.scheduled_ms,
                fire.now_ms,
                fire.missed_before,
                fire.tick_seq
            ),
            (120_000, 125_000, 0, 3)
        );
        assert_eq!(plan.state.last["a"], 120_000);
    }

    #[test]
    fn a_gap_fires_the_newest_boundary_and_records_the_skipped_ones() {
        // Boundaries 120k, 180k, 240k, 300k elapsed: 300k fires; 120k-240k
        // are one skipped record of 3 (law #1-#3 — same rule for coarse
        // ticks and restarts).
        let plan = plan_tick(
            &[job("a", 60_000)],
            &seen(&[("a", 60_000)]),
            &tick(9, 310_000),
        );
        assert_eq!(plan.fires.len(), 1);
        assert_eq!(plan.fires[0].scheduled_ms, 300_000);
        assert_eq!(plan.fires[0].missed_before, 3);
        assert_eq!(plan.records.len(), 1);
        assert_eq!(
            plan.records[0].outcome,
            RunOutcome::Skipped {
                boundaries: 3,
                first_ms: 120_000,
                last_ms: 240_000
            }
        );
        assert_eq!(plan.state.last["a"], 300_000);
    }

    #[test]
    fn jobs_are_independent() {
        let jobs = [job("fast", 10_000), job("slow", 100_000)];
        let state = seen(&[("fast", 90_000), ("slow", 0)]);
        let plan = plan_tick(&jobs, &state, &tick(4, 110_000));
        assert_eq!(plan.fires.len(), 2);
        assert_eq!(plan.fires[0].job, "fast");
        assert_eq!(plan.fires[0].scheduled_ms, 110_000);
        assert_eq!(plan.fires[0].missed_before, 1, "100k skipped");
        assert_eq!(plan.fires[1].job, "slow");
        assert_eq!(plan.fires[1].scheduled_ms, 100_000);
        assert_eq!(plan.fires[1].missed_before, 0);
    }

    #[test]
    fn a_dropped_job_leaves_the_state() {
        let plan = plan_tick(
            &[job("kept", 60_000)],
            &seen(&[("kept", 60_000), ("gone", 60_000)]),
            &tick(5, 60_500),
        );
        assert!(!plan.state.last.contains_key("gone"));
        assert!(plan.state.last.contains_key("kept"));
    }

    #[test]
    fn before_the_first_boundary_the_anchor_is_zero() {
        let plan = plan_tick(
            &[job("a", 60_000)],
            &SchedulerState::default(),
            &tick(1, 5_000),
        );
        assert!(plan.fires.is_empty());
        assert_eq!(plan.state.last["a"], 0);
        // ...and the first real boundary then fires normally.
        let next = plan_tick(&[job("a", 60_000)], &plan.state, &tick(2, 61_000));
        assert_eq!(next.fires.len(), 1);
        assert_eq!(next.fires[0].scheduled_ms, 60_000);
    }

    #[test]
    fn a_rewound_clock_is_a_noop() {
        let state = seen(&[("a", 300_000)]);
        let plan = plan_tick(&[job("a", 60_000)], &state, &tick(6, 200_000));
        assert!(plan.fires.is_empty(), "time went backwards: nothing is due");
        assert!(plan.records.is_empty());
        assert_eq!(plan.state, state);
    }
}
