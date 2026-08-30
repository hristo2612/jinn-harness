//! The seam's answer envelope and its typed refusal. Hand-coded for the
//! same reason [`crate::Event`] is — its outcome is a tag, and serde will
//! not derive a flattened rest map beside one — but the law itself is the
//! shared `decode_with_rest` / `encode_with_rest`, never a second
//! algorithm.

use serde::{Deserialize, Serialize};

use crate::{decode_with_rest, encode_with_rest, optional, put, Additive, Extensions, API_VERSION};

/// Why a workflow call was refused. Callers classify by CASE, never by
/// folding a message. A CLOSED value space.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    /// Malformed request, or a value the seam will not record (a blank
    /// name, a cyclic graph, an input the schema does not admit).
    Invalid,
    /// No such workflow, revision, run, node, or store.
    NotFound,
    /// The kernel refused a grant, or the LEDGER refuses the move — an
    /// illegal node-state transition is this code, and its message names
    /// the attempted `from -> to`.
    Refused,
    /// The store is mounted and correct; this host cannot carry the call
    /// — its Todo store is absent, or its records are unreachable.
    Unavailable,
    /// The store tried and it failed.
    Failed,
}

/// A typed refusal.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct WorkflowError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl WorkflowError {
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            extra: Extensions::new(),
        }
    }

    /// The refusal of an illegal node-state move: the typed error, with
    /// the attempt carried as DATA beside the message so a caller
    /// classifies on `from`/`to` rather than parsing prose, and the node
    /// it was attempted on so an operator knows WHERE.
    #[must_use]
    pub fn refused_transition(node: &str, refusal: crate::Refusal) -> Self {
        let mut error = Self::new(
            ErrorCode::Refused,
            format!("node {node:?}: {}", refusal.message()),
        );
        error
            .extra
            .insert("node".to_owned(), serde_json::json!(node));
        error
            .extra
            .insert("from".to_owned(), serde_json::json!(refusal.from.tag()));
        error
            .extra
            .insert("to".to_owned(), serde_json::json!(refusal.to.tag()));
        error
    }
}

/// One answer on the wire.
#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    pub api_version: String,
    pub outcome: Outcome,
    pub extra: Extensions,
}

/// An answer's two shapes.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Ok(serde_json::Value),
    Error(WorkflowError),
}

impl Additive for Answer {
    fn rest(&self) -> &Extensions {
        &self.extra
    }
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
            extra: Extensions::new(),
        }
    }

    #[must_use]
    pub fn error(error: WorkflowError) -> Self {
        Self {
            api_version: API_VERSION.to_owned(),
            outcome: Outcome::Error(error),
            extra: Extensions::new(),
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
    /// The store's [`WorkflowError`].
    pub fn into_result(self) -> Result<serde_json::Value, WorkflowError> {
        match self.outcome {
            Outcome::Ok(value) => Ok(value),
            Outcome::Error(error) => Err(error),
        }
    }

    fn to_map(&self) -> Extensions {
        let mut known = Extensions::new();
        put(&mut known, "api-version", &self.api_version);
        match &self.outcome {
            Outcome::Ok(value) => {
                known.insert("ok".to_owned(), value.clone());
            }
            Outcome::Error(error) => put(&mut known, "error", error),
        }
        encode_with_rest(known, &self.extra)
    }

    fn from_map(map: Extensions) -> Result<Self, String> {
        let ((api_version, outcome), extra) = decode_with_rest(map, |map| {
            let api_version: String = optional(map, "api-version")?;
            let outcome = match (map.remove("ok"), map.remove("error")) {
                (Some(value), None) => Outcome::Ok(value),
                (None, Some(error)) => Outcome::Error(
                    serde_json::from_value(error)
                        .map_err(|error| format!("an answer's error: {error}"))?,
                ),
                (Some(_), Some(_)) => return Err("an answer is ok OR error, never both".to_owned()),
                (None, None) => return Err("an answer carries ok or error".to_owned()),
            };
            Ok((api_version, outcome))
        })?;
        Ok(Self {
            api_version,
            outcome,
            extra,
        })
    }
}

impl Serialize for Answer {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.to_map().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Answer {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_map(Extensions::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

jinn_settings::closed_value_space!(ErrorCode, "a workflow error's `code`", {
    "invalid" => Self::Invalid,
    "not-found" => Self::NotFound,
    "refused" => Self::Refused,
    "unavailable" => Self::Unavailable,
    "failed" => Self::Failed,
});

jinn_settings::additive!(WorkflowError);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NodeState, Refusal};

    #[test]
    fn a_refused_transition_carries_the_attempt_and_the_node_as_data() {
        let error = WorkflowError::refused_transition(
            "review",
            Refusal {
                from: NodeState::Pending,
                to: NodeState::Done,
            },
        );
        assert_eq!(error.code, ErrorCode::Refused);
        assert_eq!(error.extra["node"], "review");
        assert_eq!(error.extra["from"], "pending");
        assert_eq!(error.extra["to"], "done");
        assert!(
            error.message.contains("pending -> done"),
            "{}",
            error.message
        );
    }

    #[test]
    fn an_answer_round_trips_through_the_shared_wire_law() {
        let answer = Answer::ok(serde_json::json!({ "run-id": "default-1" }));
        let bytes = answer.encode();
        let back: Answer = serde_json::from_slice(&bytes).expect("decodes");
        assert_eq!(back, answer);
        let refused = Answer::error(WorkflowError::new(ErrorCode::NotFound, "no such run"));
        let back: Answer = serde_json::from_slice(&refused.encode()).expect("decodes");
        assert_eq!(back, refused);
    }
}
