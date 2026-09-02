//! The door's vocabulary: the kernel's `jinn:auth@0.1.0` contract as the
//! operator-API seam consumes it. The contract's prose law is the vendored
//! bundle (`kernel-pin/contracts/jinn-auth/`); this module is the seam's
//! reading of its WIRE — the answer's tag byte and UTF-8 — and the typed
//! class that reading becomes on the operator surface.
//!
//! What a transport owes (the contract's own paragraph): a plugin serving
//! an inbound connection issues NO dispatch on that connection's behalf
//! before `verify` answers a principal for a credential the connection
//! presented. The HTTP provider is that transport; this module is what it
//! decodes, and `tests/auth_mirror.rs` asserts these names against the
//! vendored file, PARSED.

use crate::{ApiError, ErrorCode};

/// The kernel's authentication contract.
pub const AUTH_CONTRACT: &str = "jinn:auth";
/// Its one operation.
pub const OP_VERIFY: &str = "verify";
/// The one refusal case of the contract, as the seam spells it on the
/// wire (`ErrorCode::Unauthenticated` serializes to exactly this).
pub const UNAUTHENTICATED: &str = "unauthenticated";

/// Answer tag: granted; the principal's name follows as UTF-8.
const TAG_GRANTED: u8 = 0;
/// Answer tag: `unauthenticated`; the reason follows as UTF-8.
const TAG_UNAUTHENTICATED: u8 = 1;

/// Who the presented credential proved: the credential's NAME, never its
/// value (the contract's `principal` record).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Principal {
    pub name: String,
}

/// Why the door did not open. Two classes, deliberately distinct: the
/// kernel's own refusal (present the operator's credential, or stop) and
/// an answer the contract does not spell (a provider defect — fail
/// closed, but never dressed up as the operator's problem).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AuthAnswerError {
    /// The contract's `unauthenticated(reason)`: the reason names which
    /// precondition failed and never carries credential bytes.
    Unauthenticated(String),
    /// The bytes were not the contract's wire (no tag, an unknown tag,
    /// or text that is not UTF-8).
    Malformed(String),
}

/// Decodes one `verify` answer from the contract's wire: tag 0 + name,
/// or tag 1 + reason. Anything else is [`AuthAnswerError::Malformed`] —
/// an unreadable answer grants nothing.
///
/// # Errors
///
/// The refusal, or a malformed answer.
pub fn decode_auth_answer(bytes: &[u8]) -> Result<Principal, AuthAnswerError> {
    let Some((tag, text)) = bytes.split_first() else {
        return Err(AuthAnswerError::Malformed("empty answer".into()));
    };
    let text = std::str::from_utf8(text)
        .map_err(|error| AuthAnswerError::Malformed(format!("answer is not UTF-8: {error}")))?;
    match *tag {
        TAG_GRANTED => Ok(Principal {
            name: text.to_owned(),
        }),
        TAG_UNAUTHENTICATED => Err(AuthAnswerError::Unauthenticated(text.to_owned())),
        other => Err(AuthAnswerError::Malformed(format!(
            "unknown answer tag {other}"
        ))),
    }
}

/// The typed error the seam answers for a door that did not open: the
/// kernel's refusal under its own class, carrying the kernel's reason;
/// a malformed answer under `refused`, naming the defect and nothing the
/// connection sent.
#[must_use]
pub fn auth_api_error(error: &AuthAnswerError) -> ApiError {
    match error {
        AuthAnswerError::Unauthenticated(reason) => ApiError::unauthenticated(reason.clone()),
        AuthAnswerError::Malformed(detail) => ApiError::new(
            ErrorCode::Refused,
            format!("{AUTH_CONTRACT}/{OP_VERIFY} answered off-contract: {detail}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grant_decodes_to_the_principals_name() {
        assert_eq!(
            decode_auth_answer(b"\x00operator"),
            Ok(Principal {
                name: "operator".into()
            })
        );
    }

    #[test]
    fn a_refusal_decodes_to_the_kernels_reason_under_its_own_class() {
        let answer = decode_auth_answer(b"\x01presented credential does not match");
        assert_eq!(
            answer,
            Err(AuthAnswerError::Unauthenticated(
                "presented credential does not match".into()
            ))
        );
        let mapped = auth_api_error(&answer.unwrap_err());
        assert_eq!(mapped.code, ErrorCode::Unauthenticated);
        assert_eq!(mapped.detail, "presented credential does not match");
        assert_eq!(
            serde_json::to_value(mapped.code).expect("encodes"),
            UNAUTHENTICATED,
            "the class is the contract's case name on the wire"
        );
    }

    #[test]
    fn anything_off_the_contracts_wire_grants_nothing_and_is_not_the_operators_problem() {
        for (bytes, what) in [
            (&b""[..], "empty"),
            (&b"\x02anything"[..], "unknown tag"),
            (&b"\x00\xff\xfe"[..], "not UTF-8"),
        ] {
            let answer = decode_auth_answer(bytes);
            assert!(
                matches!(answer, Err(AuthAnswerError::Malformed(_))),
                "{what}: {answer:?}"
            );
            let mapped = auth_api_error(&answer.unwrap_err());
            assert_eq!(mapped.code, ErrorCode::Refused, "{what}");
            assert_ne!(
                mapped.code,
                ErrorCode::Unauthenticated,
                "{what}: a provider defect is never dressed as a refusal"
            );
        }
    }
}
