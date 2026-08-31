//! Driving the TODOS seam from a workflow run: the pure translation
//! between a workflow NODE and a Todo, and between a Todo's record and
//! that node's outcome.
//!
//! This module is why no workflow run store creates a Todo store itself,
//! and why no run store knows anything about a session or an engine. A
//! run holds a node's [`TodoBinding`], turns its `store` into the todos
//! seam's contract name through THAT seam's own definition
//! ([`jinn_todo::store_contract`]), and drives whatever answers. The Todo
//! store, in turn, reaches `jinn:session.<store>` from the dispatch
//! binding it was handed, and the session reaches `jinn:engine.<id>` from
//! the engine binding it was created with. Four layers, each reaching the
//! next by DEFINITION:
//!
//! ```text
//!   jinn:workflow.<store>  ->  jinn:todo.<store>  ->  jinn:session.<store>  ->  jinn:engine.<id>
//! ```
//!
//! Changing the Todo store a node records in is one field of a
//! [`TodoBinding`]; changing the session store is one field of the
//! binding's own [`jinn_todo::DispatchSpec`]; changing the engine is one
//! field inside that. Changing any provider is a profile edit. No layer
//! names an implementation of the next.
//!
//! # An outcome is derived, never assumed
//!
//! [`ended`] answers `None` for every state that is not an ending, and it
//! answers from a dispatch record it FOUND — a dispatch this node does
//! not own, or one that is not there at all, ends nothing rather than
//! being read as an absence of trouble. The one dangerous mapping —
//! [`crate::NodeState::Done`], which claims the node's work was carried
//! out — is produced ONLY by a dispatch the todos seam itself recorded
//! `done`. A dispatch that came back `interrupted` maps to an interrupted
//! NODE, so the honesty the todos seam earned after a crash is carried up
//! rather than flattened into a failure.

use jinn_todo::{DispatchStatus, TodoRecord};

use crate::{NodeSpec, NodeState, TodoBinding, LOST_TODO_REASON};

/// The reason a node whose Todo dispatch ended without one carries. A
/// store never reports an ending with no explanation, so a silent failure
/// gets a named reason rather than an empty one.
pub const UNEXPLAINED_REASON: &str = "the Todo's dispatch ended and carried no reason";

/// The Todo contract a node's binding addresses — the todos seam's OWN
/// name for it, never a string built here.
#[must_use]
pub fn todo_contract(binding: &TodoBinding) -> String {
    jinn_todo::store_contract(&binding.store)
}

/// The Todo one node records: the binding's own spec, named. A blank
/// title falls back to the node's title, and a node with no title of its
/// own falls back to a title naming the run and the node — a node is
/// never dispatched as an unnamed Todo, because a Todo nobody can read is
/// not a record of anything.
#[must_use]
pub fn todo_spec(binding: &TodoBinding, node: &NodeSpec, run_id: &str) -> jinn_todo::TodoSpec {
    let mut spec = binding.todo.clone();
    if spec.title.trim().is_empty() {
        spec.title = if node.title.trim().is_empty() {
            format!("run {run_id}: node {}", node.id)
        } else {
            node.title.clone()
        };
    }
    // The run and the node are recorded on the Todo as metadata, so a
    // Todo read on its own says which step of which run it belongs to.
    // Metadata is carried verbatim by the todos seam and interpreted by
    // nobody.
    spec.metadata
        .insert("run-id".to_owned(), serde_json::json!(run_id));
    spec.metadata
        .insert("node-id".to_owned(), serde_json::json!(node.id));
    spec
}

/// The `create` payload for the todos seam.
#[must_use]
pub fn create_request(binding: &TodoBinding, node: &NodeSpec, run_id: &str) -> serde_json::Value {
    serde_json::json!({ "spec": todo_spec(binding, node, run_id) })
}

/// The `dispatch` payload for the todos seam. The binding's session store
/// and engine binding ride through UNTOUCHED — this seam neither reads
/// them nor substitutes anything for them, which is why changing ONE
/// field of a node's dispatch spec runs the whole workflow over another
/// engine.
#[must_use]
pub fn dispatch_request(binding: &TodoBinding, todo_id: &str) -> serde_json::Value {
    serde_json::json!({ "todo-id": todo_id, "dispatch": binding.dispatch })
}

/// How one Todo record ends this seam's node, or `None` while the
/// dispatch has not ended. The dispatch is FOUND by id: one this node
/// does not own, or one the record does not hold, ends nothing. A
/// non-`done` ending always carries a reason, so no reader ever has to
/// invent one.
#[must_use]
pub fn ended(
    record: &TodoRecord,
    dispatch_id: &str,
) -> Option<(NodeState, Option<String>, String)> {
    let dispatch = record
        .dispatches
        .iter()
        .find(|dispatch| dispatch.dispatch_id == dispatch_id)?;
    let answer = dispatch.answer.clone();
    let reason = || {
        dispatch
            .reason
            .clone()
            .unwrap_or_else(|| UNEXPLAINED_REASON.to_owned())
    };
    match dispatch.status {
        // Not an ending. Nothing is claimed and nothing is recorded.
        DispatchStatus::Running => None,
        DispatchStatus::Done => Some((NodeState::Done, None, answer)),
        DispatchStatus::Failed => Some((NodeState::Failed, Some(reason()), answer)),
        DispatchStatus::Cancelled => Some((NodeState::Cancelled, Some(reason()), answer)),
        // The todos seam's own conservative answer after a crash, carried
        // UP rather than flattened into a failure: an interrupted
        // dispatch is an interrupted node.
        DispatchStatus::Interrupted => Some((NodeState::Interrupted, Some(reason()), answer)),
    }
}

/// The reason a node whose Todo became unreadable carries, with what the
/// store saw. One home for the string, so the record, the API and a test
/// all read the same words.
#[must_use]
pub fn lost_todo_reason(detail: &str) -> String {
    format!("{LOST_TODO_REASON}: {detail}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use jinn_todo::Dispatch;

    fn record(status: DispatchStatus, reason: Option<&str>) -> TodoRecord {
        TodoRecord {
            todo_id: "default-1".to_owned(),
            dispatches: vec![Dispatch {
                dispatch_id: "d-1".to_owned(),
                status,
                answer: "the work".to_owned(),
                reason: reason.map(str::to_owned),
                ..Dispatch::default()
            }],
            ..TodoRecord::default()
        }
    }

    fn node(id: &str, title: &str, binding: TodoBinding) -> NodeSpec {
        NodeSpec {
            id: id.to_owned(),
            kind: crate::NodeKind::Dispatch,
            title: title.to_owned(),
            todo: Some(binding),
            ..NodeSpec::default()
        }
    }

    #[test]
    fn a_dispatch_that_has_not_ended_ends_nothing() {
        assert!(ended(&record(DispatchStatus::Running, None), "d-1").is_none());
        // A dispatch this node does not own is not an ending either, and
        // neither is a record that holds no dispatch at all.
        assert!(ended(&record(DispatchStatus::Done, None), "d-9").is_none());
        assert!(ended(&TodoRecord::default(), "d-1").is_none());
    }

    #[test]
    fn done_is_the_one_mapping_that_claims_the_work_was_carried_out() {
        assert_eq!(
            ended(&record(DispatchStatus::Done, None), "d-1"),
            Some((NodeState::Done, None, "the work".to_owned()))
        );
    }

    #[test]
    fn an_interrupted_dispatch_is_an_interrupted_node_not_a_failed_one() {
        let (state, reason, _) = ended(
            &record(DispatchStatus::Interrupted, Some("the daemon stopped")),
            "d-1",
        )
        .expect("an ending");
        assert_eq!(state, NodeState::Interrupted);
        assert_eq!(reason.as_deref(), Some("the daemon stopped"));
    }

    #[test]
    fn no_ending_but_done_is_ever_left_without_a_reason() {
        for status in [
            DispatchStatus::Failed,
            DispatchStatus::Cancelled,
            DispatchStatus::Interrupted,
        ] {
            let (state, reason, _) = ended(&record(status, None), "d-1").expect("an ending");
            assert_ne!(state, NodeState::Done);
            assert!(state.needs_reason());
            assert_eq!(reason.as_deref(), Some(UNEXPLAINED_REASON));
        }
    }

    #[test]
    fn a_node_reaches_the_todos_seam_by_definition_and_names_no_provider() {
        // The engine binding is the SESSIONS seam's type, reached through
        // the todos seam's spec — this seam does not name it.
        let mut dispatch = jinn_todo::DispatchSpec {
            store: "default".to_owned(),
            cwd: Some("work".to_owned()),
            ..jinn_todo::DispatchSpec::default()
        };
        dispatch.engine.engine = "echo".to_owned();
        dispatch.engine.model = Some("m-1".to_owned());
        let binding = TodoBinding {
            store: "default".to_owned(),
            dispatch,
            ..TodoBinding::default()
        };
        assert_eq!(todo_contract(&binding), "jinn:todo.default");
        let node = node("run-it", "port the ledger", binding.clone());
        let created = create_request(&binding, &node, "r-1");
        assert_eq!(created["spec"]["title"], "port the ledger");
        assert_eq!(created["spec"]["metadata"]["run-id"], "r-1");
        assert_eq!(created["spec"]["metadata"]["node-id"], "run-it");
        // The engine binding rides through untouched: this seam names no
        // session provider and no engine provider.
        let dispatched = dispatch_request(&binding, "default-1");
        assert_eq!(dispatched["todo-id"], "default-1");
        assert_eq!(
            dispatched["dispatch"],
            serde_json::to_value(&binding.dispatch).expect("encodes")
        );
        assert_eq!(dispatched["dispatch"]["engine"]["engine"], "echo");
        assert_eq!(dispatched["dispatch"]["engine"]["model"], "m-1");
        assert_eq!(dispatched["dispatch"]["store"], "default");
        assert_eq!(dispatched["dispatch"]["cwd"], "work");
    }

    #[test]
    fn a_node_with_a_blank_todo_title_is_never_dispatched_unnamed() {
        let binding = TodoBinding {
            store: "default".to_owned(),
            todo: jinn_todo::TodoSpec {
                title: "   ".to_owned(),
                ..jinn_todo::TodoSpec::default()
            },
            ..TodoBinding::default()
        };
        // The node's own title names it.
        let titled = node("run-it", "port the ledger", binding.clone());
        assert_eq!(
            todo_spec(&binding, &titled, "r-1").title,
            "port the ledger".to_owned()
        );
        // And a node with no title of its own still names the run and the
        // node rather than recording a blank.
        let bare = node("run-it", "  ", binding.clone());
        let fallback = todo_spec(&binding, &bare, "r-1");
        assert!(fallback.title.contains("r-1"), "{}", fallback.title);
        assert!(fallback.title.contains("run-it"), "{}", fallback.title);
        assert!(fallback.check().is_ok());
        // A title the binding DOES carry is never overwritten.
        let named = TodoBinding {
            todo: jinn_todo::TodoSpec {
                title: "the binding's own title".to_owned(),
                ..jinn_todo::TodoSpec::default()
            },
            ..binding
        };
        assert_eq!(
            todo_spec(&named, &titled, "r-1").title,
            "the binding's own title".to_owned()
        );
    }

    #[test]
    fn a_lost_todo_names_what_the_store_saw() {
        let reason = lost_todo_reason("the store answered not-found");
        assert!(reason.starts_with(LOST_TODO_REASON), "{reason}");
        assert!(reason.contains("the store answered not-found"), "{reason}");
    }
}
