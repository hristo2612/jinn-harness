//! Driving the ENGINES seam from a store: the pure translation between a
//! run record and a turn, and the run request a session's spec makes.
//!
//! This module is why neither store provider spawns anything. A store
//! holds a session's spec, turns its [`EngineBinding`] into the engines
//! seam's contract name through that seam's OWN definition
//! (`jinn_engine::engine_contract`), and drives whatever answers. The
//! translation lives here — pure, host-free, unit-tested — so both
//! providers share one reading of a run rather than each inventing one,
//! and a third store gets it for free.
//!
//! # A run's end is derived, never assumed
//!
//! [`ended`] answers `None` for every state that is not an ending. A
//! store polls; a poll that finds nothing terminal reports nothing, and
//! the turn stays exactly as honest as it was. The one dangerous mapping
//! — [`TurnStatus::Done`], which claims the answer is whole — is
//! produced ONLY by `exited` with no error carried, and truncation
//! demotes it: a clipped answer is a `failed` turn with a reason, never
//! a `done` one that quietly lost its tail.

use jinn_engine::{RunRecord, RunState};

use crate::{SessionSpec, TurnStatus};

/// The reason a truncated run's turn carries.
pub const TRUNCATED_REASON: &str = "the engine's output passed this run's budget and was clipped";
/// The reason a run that failed without saying why carries. A store
/// never reports an ending with no explanation (the journal's rule), so
/// a silent failure gets a named one rather than an empty one.
pub const UNEXPLAINED_REASON: &str = "the engine's run failed and carried no reason";

/// The run request one `send` makes: the session's binding, its tool
/// policy and its cwd, with the caller's message as the prompt. The
/// engine id comes from the SPEC, never from the store's own config —
/// the store serves whatever engine the session was bound to.
#[must_use]
pub fn run_request(spec: &SessionSpec, prompt: &str) -> serde_json::Value {
    let mut request = serde_json::json!({
        "api-version": jinn_engine::API_VERSION,
        "engine": spec.engine.engine,
        "prompt": prompt,
        "tools": spec.tools,
    });
    if let Some(model) = &spec.engine.model {
        request["model"] = serde_json::json!(model);
    }
    if let Some(effort) = &spec.engine.effort {
        request["effort"] = serde_json::to_value(effort).expect("an effort encodes");
    }
    if let Some(cwd) = &spec.cwd {
        request["cwd"] = serde_json::json!(cwd);
    }
    request
}

/// How a run record ends this seam's turn, or `None` while it has not
/// ended. A non-`done` ending always carries a reason, so no reader ever
/// has to invent one.
#[must_use]
pub fn ended(record: &RunRecord) -> Option<(TurnStatus, Option<String>)> {
    match record.state {
        // Not an ending. Nothing is claimed and nothing is recorded.
        RunState::Starting | RunState::Running => None,
        RunState::Exited => Some(match (&record.error, record.truncated) {
            (Some(error), _) => (TurnStatus::Failed, Some(error.clone())),
            // A clipped answer is not a whole one. `done` claims the
            // text is complete, so truncation must not reach it.
            (None, true) => (TurnStatus::Failed, Some(TRUNCATED_REASON.to_owned())),
            (None, false) => (TurnStatus::Done, None),
        }),
        RunState::Cancelled => Some((
            TurnStatus::Cancelled,
            Some(
                record
                    .error
                    .clone()
                    .unwrap_or_else(|| "the run was cancelled".to_owned()),
            ),
        )),
        RunState::Failed => Some((
            TurnStatus::Failed,
            Some(
                record
                    .error
                    .clone()
                    .unwrap_or_else(|| UNEXPLAINED_REASON.to_owned()),
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jinn_engine::Usage;

    fn record(state: RunState) -> RunRecord {
        RunRecord {
            api_version: jinn_engine::API_VERSION.to_owned(),
            run_id: "r1".to_owned(),
            engine: "echo".to_owned(),
            model: None,
            state,
            events: Vec::new(),
            status: None,
            usage: Usage::default(),
            text: "hi".to_owned(),
            truncated: false,
            error: None,
            extra: crate::Extensions::new(),
        }
    }

    #[test]
    fn a_run_that_has_not_ended_ends_nothing() {
        assert!(ended(&record(RunState::Starting)).is_none());
        assert!(ended(&record(RunState::Running)).is_none());
    }

    #[test]
    fn done_is_the_one_mapping_that_needs_everything_to_be_right() {
        assert_eq!(
            ended(&record(RunState::Exited)),
            Some((TurnStatus::Done, None))
        );
        // A clipped answer is not a whole answer.
        let mut clipped = record(RunState::Exited);
        clipped.truncated = true;
        assert_eq!(
            ended(&clipped),
            Some((TurnStatus::Failed, Some(TRUNCATED_REASON.to_owned())))
        );
        // An engine that exited cleanly and still reported a failure is
        // a failure: the exit code is not the claim, the error is.
        let mut errored = record(RunState::Exited);
        errored.error = Some("upstream refused".to_owned());
        assert_eq!(
            ended(&errored),
            Some((TurnStatus::Failed, Some("upstream refused".to_owned())))
        );
    }

    #[test]
    fn no_ending_but_done_is_ever_left_without_a_reason() {
        for state in [RunState::Cancelled, RunState::Failed] {
            let (status, reason) = ended(&record(state)).expect("an ending");
            assert_ne!(status, TurnStatus::Done);
            assert!(
                reason.is_some_and(|reason| !reason.is_empty()),
                "{state:?} explains itself"
            );
        }
    }

    #[test]
    fn a_run_request_carries_the_sessions_binding_and_nothing_of_the_stores() {
        let spec: SessionSpec = serde_json::from_value(serde_json::json!({
            "engine": { "engine": "claude", "model": "m-1", "effort": "high" },
            "cwd": "work",
            "tools": { "mode": "allowlist", "allow": ["bash"] },
        }))
        .expect("a spec");
        let request = run_request(&spec, "hello");
        assert_eq!(request["engine"], "claude");
        assert_eq!(request["model"], "m-1");
        assert_eq!(request["effort"], "high");
        assert_eq!(request["cwd"], "work");
        assert_eq!(request["prompt"], "hello");
        assert_eq!(request["tools"]["mode"], "allowlist");
        // An unbound field is ABSENT, never a sentinel a provider would
        // have to tell apart from a real value.
        let bare = run_request(&crate::SessionSpec::default(), "hi");
        assert!(bare.get("model").is_none() && bare.get("cwd").is_none());
    }
}
