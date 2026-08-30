//! Driving the SESSIONS seam from a Todo store: the pure translation
//! between a Todo and a session, and between a session's record and a
//! dispatch's outcome.
//!
//! This module is why no Todo store opens a session itself, and why
//! neither store knows anything about an engine. A store holds a Todo's
//! [`DispatchSpec`], turns its `store` into the sessions seam's contract
//! name through THAT seam's own definition
//! ([`jinn_session::store_contract`]), and drives whatever answers. The
//! session, in turn, resolves `jinn:engine.<id>` from the binding it was
//! created with. Three layers, each reaching the next by DEFINITION:
//!
//! ```text
//!   jinn:todo.<store>  ->  jinn:session.<store>  ->  jinn:engine.<id>
//! ```
//!
//! Changing the engine a Todo's work runs on is one field of a
//! [`DispatchSpec`]; changing the session store is another; changing
//! either provider is a profile edit. No layer names an implementation of
//! the next.
//!
//! # An outcome is derived, never assumed
//!
//! [`ended`] answers `None` for every state that is not an ending. The
//! one dangerous mapping — [`DispatchStatus::Done`], which claims the
//! work was carried out and the answer is whole — is produced ONLY by a
//! turn the session itself recorded `done`. A session turn that came back
//! `interrupted` maps to an interrupted DISPATCH, so the honesty the
//! sessions seam earned after a crash is carried up rather than flattened
//! into a failure.

use jinn_session::{SessionRecord, SessionSpec, TurnStatus};

use crate::{DispatchSpec, DispatchStatus, TodoRecord};

/// The reason a dispatch whose session vanished carries.
pub const LOST_SESSION_REASON: &str =
    "the session this dispatch was driving is no longer readable, so how far it got is not \
     recorded";
/// The reason a dispatch whose turn ended without one carries. A store
/// never reports an ending with no explanation, so a silent failure gets
/// a named reason rather than an empty one.
pub const UNEXPLAINED_REASON: &str = "the session's turn ended and carried no reason";

/// The session contract a dispatch spec addresses — the sessions seam's
/// own name for it, never a string built here.
#[must_use]
pub fn session_contract(spec: &DispatchSpec) -> String {
    jinn_session::store_contract(&spec.store)
}

/// The session spec one dispatch opens: the Todo's engine binding, its
/// cwd, and nothing of the Todo store's own configuration. The store
/// serves whatever session store and engine the DISPATCH named.
#[must_use]
pub fn session_spec(spec: &DispatchSpec, todo_id: &str) -> SessionSpec {
    let mut session = SessionSpec {
        engine: spec.engine.clone(),
        cwd: spec.cwd.clone(),
        ..SessionSpec::default()
    };
    // The Todo is recorded on the session as metadata, so a session read
    // on its own says which piece of work it belongs to. Metadata is
    // carried verbatim by the sessions seam and interpreted by nobody.
    session
        .metadata
        .insert("todo-id".to_owned(), serde_json::json!(todo_id));
    session
}

/// The `create` payload for the sessions seam.
#[must_use]
pub fn create_request(spec: &DispatchSpec, todo_id: &str) -> serde_json::Value {
    serde_json::json!({ "spec": session_spec(spec, todo_id) })
}

/// The `send` payload: the dispatch's own message, or the Todo rendered
/// as a brief. Never an empty prompt — a session asked to do nothing
/// would answer nothing and the dispatch would read as done.
#[must_use]
pub fn send_request(spec: &DispatchSpec, session_id: &str, todo: &TodoRecord) -> serde_json::Value {
    let message = match spec.message.as_deref() {
        Some(message) if !message.trim().is_empty() => message.to_owned(),
        _ => brief(todo),
    };
    serde_json::json!({ "session-id": session_id, "message": message })
}

/// The Todo as a brief a session can act on: what was asked, and what
/// "done" means. Only the parts that are there — a blank field is left
/// out rather than rendered as an empty heading.
#[must_use]
pub fn brief(todo: &TodoRecord) -> String {
    let mut brief = format!("Todo {}: {}", todo.todo_id, todo.title);
    if !todo.body.trim().is_empty() {
        brief.push_str("\n\n");
        brief.push_str(todo.body.trim());
    }
    if !todo.acceptance.trim().is_empty() {
        brief.push_str("\n\nAcceptance: ");
        brief.push_str(todo.acceptance.trim());
    }
    brief
}

/// How one session record ends this seam's dispatch, or `None` while the
/// turn has not ended. A non-`done` ending always carries a reason, so no
/// reader ever has to invent one.
#[must_use]
pub fn ended(
    record: &SessionRecord,
    turn_id: &str,
) -> Option<(DispatchStatus, Option<String>, String)> {
    let turn = record.log.iter().find(|turn| turn.turn_id == turn_id)?;
    let answer = turn.answer.clone();
    let reason = || {
        turn.reason
            .clone()
            .unwrap_or_else(|| UNEXPLAINED_REASON.to_owned())
    };
    match turn.status {
        // Not an ending. Nothing is claimed and nothing is recorded.
        TurnStatus::Running => None,
        TurnStatus::Done => Some((DispatchStatus::Done, None, answer)),
        TurnStatus::Failed => Some((DispatchStatus::Failed, Some(reason()), answer)),
        TurnStatus::Cancelled => Some((DispatchStatus::Cancelled, Some(reason()), answer)),
        // The sessions seam's own conservative answer after a crash,
        // carried UP rather than flattened into a failure: an
        // interrupted turn is an interrupted dispatch.
        TurnStatus::Interrupted => Some((DispatchStatus::Interrupted, Some(reason()), answer)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jinn_session::{EngineBinding, Turn};

    fn record(status: TurnStatus, reason: Option<&str>) -> SessionRecord {
        SessionRecord {
            session_id: "s-1".to_owned(),
            log: vec![Turn {
                turn_id: "t-1".to_owned(),
                status,
                answer: "the work".to_owned(),
                reason: reason.map(str::to_owned),
                ..Turn::default()
            }],
            ..SessionRecord::default()
        }
    }

    #[test]
    fn a_turn_that_has_not_ended_ends_nothing() {
        assert!(ended(&record(TurnStatus::Running, None), "t-1").is_none());
        // A turn this dispatch does not own is not an ending either.
        assert!(ended(&record(TurnStatus::Done, None), "t-9").is_none());
    }

    #[test]
    fn done_is_the_one_mapping_that_claims_the_work_was_carried_out() {
        assert_eq!(
            ended(&record(TurnStatus::Done, None), "t-1"),
            Some((DispatchStatus::Done, None, "the work".to_owned()))
        );
    }

    #[test]
    fn an_interrupted_turn_is_an_interrupted_dispatch_not_a_failed_one() {
        let (status, reason, _) = ended(
            &record(TurnStatus::Interrupted, Some("the daemon stopped")),
            "t-1",
        )
        .expect("an ending");
        assert_eq!(status, DispatchStatus::Interrupted);
        assert_eq!(reason.as_deref(), Some("the daemon stopped"));
    }

    #[test]
    fn no_ending_but_done_is_ever_left_without_a_reason() {
        for status in [
            TurnStatus::Failed,
            TurnStatus::Cancelled,
            TurnStatus::Interrupted,
        ] {
            let (mapped, reason, _) = ended(&record(status, None), "t-1").expect("an ending");
            assert_ne!(mapped, DispatchStatus::Done);
            assert_eq!(reason.as_deref(), Some(UNEXPLAINED_REASON));
        }
    }

    #[test]
    fn a_dispatch_reaches_the_sessions_seam_by_definition_and_names_no_provider() {
        let spec = DispatchSpec {
            store: "default".to_owned(),
            engine: EngineBinding {
                engine: "echo".to_owned(),
                model: Some("m-1".to_owned()),
                ..EngineBinding::default()
            },
            cwd: Some("work".to_owned()),
            ..DispatchSpec::default()
        };
        assert_eq!(session_contract(&spec), "jinn:session.default");
        let created = create_request(&spec, "default-1");
        assert_eq!(created["spec"]["engine"]["engine"], "echo");
        assert_eq!(created["spec"]["engine"]["model"], "m-1");
        assert_eq!(created["spec"]["cwd"], "work");
        assert_eq!(created["spec"]["metadata"]["todo-id"], "default-1");
    }

    #[test]
    fn a_dispatch_with_no_message_sends_the_todo_and_never_an_empty_prompt() {
        let todo = TodoRecord {
            todo_id: "default-1".to_owned(),
            title: "port the ledger".to_owned(),
            body: "  fold state from an append-only log  ".to_owned(),
            acceptance: "the suite is green".to_owned(),
            ..TodoRecord::default()
        };
        let spec = DispatchSpec::default();
        let sent = send_request(&spec, "s-1", &todo);
        let message = sent["message"].as_str().expect("a message");
        assert!(message.contains("port the ledger"), "{message}");
        assert!(
            message.contains("fold state from an append-only log"),
            "{message}"
        );
        assert!(
            message.contains("Acceptance: the suite is green"),
            "{message}"
        );
        assert_eq!(sent["session-id"], "s-1");
        // A blank message is not an override.
        let blank = DispatchSpec {
            message: Some("   ".to_owned()),
            ..DispatchSpec::default()
        };
        assert_eq!(
            send_request(&blank, "s-1", &todo)["message"],
            serde_json::json!(message)
        );
        // A real one is.
        let named = DispatchSpec {
            message: Some("do the thing".to_owned()),
            ..DispatchSpec::default()
        };
        assert_eq!(
            send_request(&named, "s-1", &todo)["message"],
            "do the thing"
        );
    }

    #[test]
    fn a_brief_leaves_out_what_the_todo_does_not_say() {
        let bare = TodoRecord {
            todo_id: "t".to_owned(),
            title: "just a title".to_owned(),
            ..TodoRecord::default()
        };
        assert_eq!(brief(&bare), "Todo t: just a title");
    }
}
