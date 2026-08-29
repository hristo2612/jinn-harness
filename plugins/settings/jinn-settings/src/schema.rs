//! The closed schema language (kernel R9: no expression evaluation, no
//! ambient authority — a schema is data describing shapes). A namespace's
//! settings are an object of typed fields; the validator decides
//! membership and nothing else.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{is_secret_ref, Extensions};

/// The shape of one field's value.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Bool,
    /// A non-negative integer (settings periods, ports, counts).
    Integer,
    Number,
    String,
    Array,
    Object,
    /// A typed secret reference `{ "$secret": "<key>" }` — a plain string
    /// here is REFUSED: the settings document never holds a secret.
    SecretRef,
}

/// One field.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Field {
    pub kind: Kind,
    #[serde(default)]
    pub required: bool,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl Field {
    #[must_use]
    pub fn new(kind: Kind) -> Self {
        Self {
            kind,
            required: false,
            extra: Extensions::new(),
        }
    }

    #[must_use]
    pub fn required(kind: Kind) -> Self {
        Self {
            required: true,
            ..Self::new(kind)
        }
    }
}

/// A namespace's schema: its fields by key, and whether keys outside them
/// are admitted.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Schema {
    #[serde(default)]
    pub properties: BTreeMap<String, Field>,
    #[serde(default)]
    pub additional: bool,
    #[serde(flatten)]
    pub extra: Extensions,
}

fn matches(kind: Kind, value: &serde_json::Value) -> bool {
    match kind {
        Kind::Bool => value.is_boolean(),
        Kind::Integer => value.is_u64(),
        Kind::Number => value.is_number(),
        Kind::String => value.is_string(),
        Kind::Array => value.is_array(),
        Kind::Object => value.is_object(),
        Kind::SecretRef => is_secret_ref(value),
    }
}

/// Validates a whole settings object against a schema: every required
/// key present, every present key of its declared kind, no key outside
/// the schema unless `additional`, and a `secret-ref` field never a bare
/// value.
///
/// # Errors
///
/// The first violation, named.
pub fn validate(schema: &Schema, value: &serde_json::Value) -> Result<(), String> {
    let Some(object) = value.as_object() else {
        return Err("settings are a JSON object".to_owned());
    };
    for (key, field) in &schema.properties {
        match object.get(key) {
            None if field.required => return Err(format!("{key:?} is required")),
            None => {}
            Some(present) if matches(field.kind, present) => {}
            Some(present) if field.kind == Kind::SecretRef => {
                return Err(format!(
                    "{key:?} is a secret reference ({{\"$secret\": \"<key>\"}}), never a value \
                     ({present} refused: the settings document holds no secret)"
                ))
            }
            Some(present) => {
                return Err(format!("{key:?} must be {:?}, got {present}", field.kind))
            }
        }
    }
    if !schema.additional {
        if let Some(stray) = object
            .keys()
            .find(|key| !schema.properties.contains_key(*key))
        {
            return Err(format!("{stray:?} is not a declared setting"));
        }
    }
    Ok(())
}
