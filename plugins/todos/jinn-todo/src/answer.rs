//! The seam's answer envelope and its typed refusal. Hand-coded for the
//! same reason [`crate::Event`] is — its outcome is a tag, and serde will
//! not derive a flattened rest map beside one — but the law itself is the
//! shared `decode_with_rest` / `encode_with_rest`, never a second
//! algorithm.

use serde::{Deserialize, Serialize};

use crate::{decode_with_rest, encode_with_rest, optional, put, Additive, Extensions, API_VERSION};

/// Why a Todo call was refused. Callers classify by CASE, never by
/// folding a message. A CLOSED value space.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    /// Malformed request, or a value the seam will not record (a blank
    /// title, a blank actor).
    Invalid,
    /// No such Todo, comment, or store.
    NotFound,
    /// The kernel refused a grant, or the LEDGER refuses the move — an
    /// illegal status transition is this code, and its message names the
    /// attempted `from -> to`.
    Refused,
    /// The store is mounted and correct; this host cannot carry the call
    /// — its session store is absent, or its records are unreachable.
    Unavailable,
    /// The store tried and it failed.
    Failed,
}

/// A typed refusal.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TodoError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl TodoError {
    #[must_use]
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            extra: Extensions::new(),
        }
    }

    /// The refusal of an illegal status move: the typed error, with the
    /// attempt carried as DATA beside the message so a caller classifies
    /// on `from`/`to` rather than parsing prose.
    #[must_use]
    pub fn refused_transition(refusal: crate::Refusal) -> Self {
        let mut error = Self::new(ErrorCode::Refused, refusal.message());
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
    Error(TodoError),
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
    pub fn error(error: TodoError) -> Self {
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
    /// The store's [`TodoError`].
    pub fn into_result(self) -> Result<serde_json::Value, TodoError> {
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

jinn_settings::closed_value_space!(ErrorCode, "a Todo error's `code`", {
    "invalid" => Self::Invalid,
    "not-found" => Self::NotFound,
    "refused" => Self::Refused,
    "unavailable" => Self::Unavailable,
    "failed" => Self::Failed,
});

jinn_settings::additive!(TodoError);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Refusal, Status};

    #[test]
    fn a_refused_transition_carries_the_attempt_as_data_not_only_as_prose() {
        let error = TodoError::refused_transition(Refusal {
            from: Status::Executing,
            to: Status::Done,
        });
        assert_eq!(error.code, ErrorCode::Refused);
        assert_eq!(error.extra["from"], "executing");
        assert_eq!(error.extra["to"], "done");
        assert!(
            error.message.contains("executing -> done"),
            "{}",
            error.message
        );
    }
}
