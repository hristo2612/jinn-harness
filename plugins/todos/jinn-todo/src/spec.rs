//! What a caller asks for: the Todo spec, the dispatch binding, and the
//! request documents of every operation that takes one.
//!
//! # An actor is declared, never inferred
//!
//! Every write carries an OPTIONAL actor, and the option is the point.
//! Absence records that nobody was declared — it is never filled in with
//! the caller's transport, a default principal, or the last actor seen.
//! And an actor that is present must be a NAME: the empty string is
//! refused ([`Attribution::check`]), because a blank that renders like a
//! principal is exactly the sentinel that can pass for a real reading.

use jinn_session::EngineBinding;
use serde::{Deserialize, Serialize};

use crate::{ErrorCode, Extensions, Status, TodoError, API_VERSION};

/// Who asked for a write. See the module doc: `None` is "nobody was
/// declared", and it stays that way all the way to the record.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Attribution {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor: Option<String>,
}

impl Attribution {
    /// The declared actor, checked.
    ///
    /// # Errors
    ///
    /// The actor is present and blank — a sentinel, not a principal.
    pub fn check(&self) -> Result<Option<String>, TodoError> {
        match self.actor.as_deref() {
            None => Ok(None),
            Some(name) if name.trim().is_empty() => Err(TodoError::new(
                ErrorCode::Invalid,
                "an `actor` is a principal's name; omit it to record that none was declared, \
                 rather than sending a blank that would read like one",
            )),
            Some(name) => Ok(Some(name.to_owned())),
        }
    }
}

/// One Todo as it is asked for. The fields are the company's own
/// vocabulary (`docs/company-doctrine.md`): what was asked, who owns it,
/// and what "done" means.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TodoSpec {
    #[serde(default)]
    pub api_version: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// The parent Todo, for a child of a larger piece of work. A parent
    /// must already exist, which is what makes the tree ACYCLIC by
    /// construction rather than by a cycle check.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub department: Option<String>,
    /// Lower is more urgent, as the company's ledger reads it. Absent is
    /// absent — never a middle value standing in for "not said".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
    /// What "done" means for this Todo, in the asker's words.
    #[serde(default)]
    pub acceptance: String,
    #[serde(default, flatten)]
    pub attribution: Attribution,
    /// Operator metadata, carried verbatim and never interpreted.
    #[serde(default)]
    pub metadata: Extensions,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl TodoSpec {
    /// The spec as a peer receives it back, with this version stamped on.
    #[must_use]
    pub fn versioned(mut self) -> Self {
        self.api_version = API_VERSION.to_owned();
        self
    }

    /// A spec a store will accept.
    ///
    /// # Errors
    ///
    /// A blank title (a Todo nobody can read is not a record of anything)
    /// or a blank actor.
    pub fn check(&self) -> Result<(), TodoError> {
        if self.title.trim().is_empty() {
            return Err(TodoError::new(
                ErrorCode::Invalid,
                "a Todo's `title` is what the ledger is read by; it cannot be blank",
            ));
        }
        self.attribution.check().map(|_| ())
    }
}

/// `create`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct CreateRequest {
    pub spec: TodoSpec,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `create` answer.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TodoCreated {
    #[serde(default)]
    pub api_version: String,
    pub todo_id: String,
    pub store: String,
    pub status: Status,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `{ "todo-id": ... }` document. ONE shape names a Todo, and every
/// operation that takes one reads it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct GetRequest {
    pub todo_id: String,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// See [`GetRequest`] — the same document, named for `tree`.
pub type TreeRequest = GetRequest;

/// `update`: one status move, with the actor who asked for it and an
/// optional note. The move is checked against the table
/// ([`Status::transition`]); an illegal one is refused NAMING the
/// attempt, and the refusal is recorded.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct UpdateRequest {
    pub todo_id: String,
    pub status: Status,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, flatten)]
    pub attribution: Attribution,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// `comment`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct CommentRequest {
    pub todo_id: String,
    pub body: String,
    #[serde(default, flatten)]
    pub attribution: Attribution,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// `list`. Every filter absent lists every Todo this store holds.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ListRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<Status>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub department: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// Only the Todos with no parent — the objective view the company's
    /// doctrine asks for.
    #[serde(default)]
    pub roots_only: bool,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// Where a Todo's work is to be done: a SESSION store and the engine
/// binding the session is opened with. Both are DEFINITIONS —
/// `jinn:session.<store>` and, inside it, `jinn:engine.<id>` — so a Todo
/// names neither a session provider nor an engine provider, and the
/// three-layer stack composes without any layer knowing the next one's
/// implementation.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct DispatchSpec {
    /// The session store id — the second half of `jinn:session.<store>`.
    pub store: String,
    /// The engine binding the session is created with. Changing ONE
    /// field here runs the same Todo over another engine.
    pub engine: EngineBinding,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// What is sent as the turn. Absent means the Todo itself, rendered
    /// by [`crate::dispatch::brief`] — never an empty prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// `dispatch`: send this Todo to a session.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct DispatchRequest {
    pub todo_id: String,
    pub dispatch: DispatchSpec,
    #[serde(default, flatten)]
    pub attribution: Attribution,
    #[serde(flatten)]
    pub extra: Extensions,
}

jinn_settings::additive!(
    TodoSpec,
    CreateRequest,
    TodoCreated,
    GetRequest,
    UpdateRequest,
    CommentRequest,
    ListRequest,
    DispatchSpec,
    DispatchRequest,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_actor_is_refused_because_it_would_read_like_a_principal() {
        for blank in ["", "   "] {
            let attribution = Attribution {
                actor: Some(blank.to_owned()),
            };
            let refused = attribution.check().expect_err("a blank is not a name");
            assert_eq!(refused.code, ErrorCode::Invalid);
        }
    }

    #[test]
    fn an_absent_actor_stays_absent() {
        assert_eq!(
            Attribution::default().check().expect("absent is fine"),
            None
        );
        let declared = Attribution {
            actor: Some("planner".to_owned()),
        };
        assert_eq!(
            declared.check().expect("a name"),
            Some("planner".to_owned())
        );
    }

    #[test]
    fn a_todo_with_no_title_records_nothing_and_is_refused() {
        let refused = TodoSpec::default().check().expect_err("a blank title");
        assert_eq!(refused.code, ErrorCode::Invalid);
        let named = TodoSpec {
            title: "port the ledger".to_owned(),
            ..TodoSpec::default()
        };
        assert!(named.check().is_ok());
    }
}
