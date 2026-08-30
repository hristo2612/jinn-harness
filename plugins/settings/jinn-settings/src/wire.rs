//! The ADDITIVITY LAW of every wire surface in this distribution, written
//! once (AGENTS.md §One home per fact).
//!
//! **The law.** For every wire type and every variant, known or unknown,
//! at every nesting depth, decode-then-encode is lossless for content the
//! schema does not know. A field a newer peer sends rides through an older
//! hop untouched; it is never dropped, and never silently.
//!
//! **The mechanism.** Every wire type carries a REST MAP — an
//! [`Extensions`](crate::Extensions) named `extra` — holding verbatim
//! whatever its version did not read. Types whose shape serde can derive
//! get it from `#[serde(flatten)]`; a type whose tag forbids a derive gets
//! it from [`decode_with_rest`] / [`encode_with_rest`], which ARE the same
//! law written out rather than a second algorithm. [`Additive`] is how a
//! rest map is reached uniformly, so a seam proves the law by property
//! over its whole inventory instead of by a table of examples.
//!
//! **The named exception.** A CLOSED surface has nowhere to put content it
//! cannot name, so its law is REFUSAL — see [`closed`](crate::closed) and
//! [`closed_value_space`].
//!
//! This module is the home; it is re-exported by the seam definitions that
//! borrow it (`jinn-engine`, `jinn-session`) rather than copied into them.

use crate::Extensions;

/// The rest map, reachable uniformly. Every wire type in the distribution
/// implements it, which is what lets a seam's additivity suite walk its
/// whole inventory through ONE property instead of one example per type.
pub trait Additive {
    /// What this value carried that its version could not read.
    fn rest(&self) -> &Extensions;
}

/// Decoding half of the additivity law, written once.
///
/// `known` reads the fields this version understands, REMOVING them from
/// the map; whatever is left is the rest, kept verbatim. Because the known
/// fields are removed, a key can never be in both halves — the two can
/// never disagree, and neither can clobber the other on the way out.
///
/// # Errors
///
/// Whatever `known` refuses: a required field absent or ill-typed.
pub fn decode_with_rest<T>(
    mut map: Extensions,
    known: impl FnOnce(&mut Extensions) -> Result<T, String>,
) -> Result<(T, Extensions), String> {
    let value = known(&mut map)?;
    Ok((value, map))
}

/// Encoding half of the same law: the fields this version knows, then the
/// rest re-emitted unchanged. `or_insert` rather than `insert` so a known
/// field always wins — decoding makes the overlap impossible, and this
/// keeps a hand-built value from lying about its own shape.
#[must_use]
pub fn encode_with_rest(mut known: Extensions, rest: &Extensions) -> Extensions {
    for (name, value) in rest {
        known.entry(name.clone()).or_insert_with(|| value.clone());
    }
    known
}

/// A field the shape REQUIRES; absent is a decode error naming it.
///
/// # Errors
///
/// The field is absent, or present and ill-typed.
pub fn required<T: serde::de::DeserializeOwned>(
    map: &mut Extensions,
    name: &str,
) -> Result<T, String> {
    let value = map
        .remove(name)
        .ok_or_else(|| format!("this shape carries {name:?}"))?;
    serde_json::from_value(value).map_err(|error| format!("field {name:?}: {error}"))
}

/// A field whose ABSENCE has a meaning of its own (`None`, `false`, an
/// empty value) rather than being an error.
///
/// # Errors
///
/// The field is present and ill-typed.
pub fn optional<T: serde::de::DeserializeOwned + Default>(
    map: &mut Extensions,
    name: &str,
) -> Result<T, String> {
    match map.remove(name) {
        None | Some(serde_json::Value::Null) => Ok(T::default()),
        Some(value) => {
            serde_json::from_value(value).map_err(|error| format!("field {name:?}: {error}"))
        }
    }
}

/// Writes one known field into a hand-built map.
///
/// # Panics
///
/// Never in practice: a seam's own types all encode.
pub fn put<T: serde::Serialize>(map: &mut Extensions, name: &str, value: T) {
    map.insert(
        name.to_owned(),
        serde_json::to_value(value).expect("a field encodes"),
    );
}

/// `impl Additive` for types whose rest map is a derived `extra` field.
#[macro_export]
macro_rules! additive {
    ($($type:ty),* $(,)?) => {
        $(impl $crate::Additive for $type {
            fn rest(&self) -> &$crate::Extensions {
                &self.extra
            }
        })*
    };
}

/// A CLOSED VALUE SPACE, decoded through the ONE shared refusal
/// ([`closed`](crate::closed)). An enum has nowhere to put a value it
/// cannot name, so it refuses — never a default, never the nearest known
/// variant, and never a drop.
///
/// It is hand-written rather than derived for exactly one reason: serde's
/// own refusal names the admitted variants but not the SURFACE that
/// refused, and an operator reading `effort` refused wants to know it was
/// `effort`. The hazard a hand-written table carries — a name here
/// disagreeing with what `Serialize` emits — is closed in each seam by a
/// round-trip test over every variant.
#[macro_export]
macro_rules! closed_value_space {
    ($type:ty, $surface:literal, { $($name:literal => $variant:expr),+ $(,)? }) => {
        impl<'de> ::serde::Deserialize<'de> for $type {
            fn deserialize<D: ::serde::Deserializer<'de>>(
                deserializer: D,
            ) -> ::core::result::Result<Self, D::Error> {
                let named = <::std::string::String as ::serde::Deserialize>::deserialize(
                    deserializer,
                )?;
                match named.as_str() {
                    $($name => ::core::result::Result::Ok($variant),)+
                    _ => ::core::result::Result::Err($crate::closed(
                        $surface,
                        &format!("the value `{named}`"),
                        &[$($name),+].join(" | "),
                    )),
                }
            }
        }
    };
}
