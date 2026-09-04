//! The composition's SHAPE on the operator surface (pin `f8b285b`, jinnd
//! M2-K23 `jinn:profile-admin`; FINDINGS #37 closed by harness pin-bump
//! 10): adding, removing, disabling, re-granting and re-pinning an entry
//! is ONE ledgered kernel write from the transport, never a file edit.
//!
//! This module is the pure half: which write a request names, the
//! broker wire each write crosses on, and how the kernel's typed refusal
//! comes back as this seam's `refused`. The contract itself is not
//! restated here — its one home is
//! `kernel-pin/contracts/jinn-profile-admin/README.md`.
//!
//! # One write per call, and the config route beside it
//!
//! `PATCH /v1/profile/entries/{id}` carries EITHER a config patch
//! (`{ config }`, the `jinn:api-profile` route, confined to `config.data`
//! — a `grants` sent through it is answered by the kernel's 0.3.0 typed
//! refusal, K23 (d)) OR exactly one admin write (`{ disabled }`,
//! `{ grants }`, `{ package, hash }`). A body naming two is `invalid`
//! before any kernel call: two writes in one request would be two rows
//! or none, and the answer could name neither honestly.

use serde::{Deserialize, Serialize};

use crate::{ApiError, ErrorCode, Extensions, API_VERSION};

/// The kernel's composition-administration contract.
pub const ADMIN_CONTRACT: &str = "jinn:profile-admin";
/// The entries collection: `POST` adds one; `/{id}` addresses one.
pub const ENTRIES_PATH: &str = "/v1/profile/entries";

/// The five operations, by the bundle's names.
pub const OP_ADD_ENTRY: &str = "add-entry";
/// See [`OP_ADD_ENTRY`].
pub const OP_REMOVE_ENTRY: &str = "remove-entry";
/// See [`OP_ADD_ENTRY`].
pub const OP_SET_DISABLED: &str = "set-disabled";
/// See [`OP_ADD_ENTRY`].
pub const OP_SET_GRANTS: &str = "set-grants";
/// See [`OP_ADD_ENTRY`].
pub const OP_SWAP_PLUGIN: &str = "swap-plugin";

/// The refusal classes, in the wire's byte order (1..=4).
const CLASSES: [&str; 4] = ["unauthorized", "malformed", "conflict", "irreversible"];

/// One admin write, typed.
#[derive(Clone, Debug, PartialEq)]
pub enum AdminWrite {
    /// `POST /v1/profile/entries` — the 0.2.0 `entry` record.
    Add(serde_json::Value),
    /// `DELETE /v1/profile/entries/{id}`.
    Remove,
    /// `PATCH … { disabled }`.
    SetDisabled(bool),
    /// `PATCH … { grants }` — the whole list, replaced.
    SetGrants(serde_json::Value),
    /// `PATCH … { package, hash [, version] }` — the entry's pin.
    SwapPlugin {
        package: String,
        version: String,
        hash: String,
    },
}

impl AdminWrite {
    /// The contract operation this write is.
    #[must_use]
    pub fn operation(&self) -> &'static str {
        match self {
            Self::Add(_) => OP_ADD_ENTRY,
            Self::Remove => OP_REMOVE_ENTRY,
            Self::SetDisabled(_) => OP_SET_DISABLED,
            Self::SetGrants(_) => OP_SET_GRANTS,
            Self::SwapPlugin { .. } => OP_SWAP_PLUGIN,
        }
    }
}

/// One admin route: the entry it administers and the write.
#[derive(Clone, Debug, PartialEq)]
pub struct AdminRoute {
    pub id: String,
    pub write: AdminWrite,
}

/// Whether a path is the entries collection or one entry in it.
#[must_use]
pub fn is_entries_path(path: &str) -> bool {
    path == ENTRIES_PATH || path.starts_with(&format!("{ENTRIES_PATH}/"))
}

fn entry_id(path: &str) -> Option<&str> {
    let id = path.strip_prefix(&format!("{ENTRIES_PATH}/"))?;
    (!id.is_empty() && !id.contains('/')).then_some(id)
}

fn invalid(detail: impl Into<String>) -> ApiError {
    ApiError::new(ErrorCode::Invalid, detail)
}

fn string_field(body: &serde_json::Value, name: &str) -> Result<String, ApiError> {
    body[name]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid(format!("{name} must be a string")))
}

/// The one write a `PATCH` body names, or `None` for a config patch.
fn patch_write(body: &serde_json::Value) -> Result<Option<AdminWrite>, ApiError> {
    let Some(fields) = body.as_object() else {
        return Ok(None);
    };
    let pin = ["package", "hash", "version"]
        .iter()
        .any(|name| fields.contains_key(*name));
    let named = usize::from(fields.contains_key("disabled"))
        + usize::from(fields.contains_key("grants"))
        + usize::from(pin);
    if named == 0 {
        return Ok(None);
    }
    if named > 1 || fields.contains_key("config") {
        return Err(invalid(
            "one write per call: a body names `disabled`, `grants`, `package`+`hash`, or `config` — never two",
        ));
    }
    if let Some(disabled) = fields.get("disabled") {
        return disabled
            .as_bool()
            .map(|flag| Some(AdminWrite::SetDisabled(flag)))
            .ok_or_else(|| invalid("disabled must be true or false"));
    }
    if let Some(grants) = fields.get("grants") {
        return if grants.is_array() {
            Ok(Some(AdminWrite::SetGrants(grants.clone())))
        } else {
            Err(invalid("grants must be an array"))
        };
    }
    Ok(Some(AdminWrite::SwapPlugin {
        package: string_field(body, "package")?,
        hash: string_field(body, "hash")?,
        version: if fields.contains_key("version") {
            string_field(body, "version")?
        } else {
            String::new()
        },
    }))
}

/// Matches a method + path + parsed body against this surface: `None`
/// when the request is not an admin write (the config patch route, a
/// read, a path this surface does not shape); `Some(Err)` for a body
/// that names a write badly — answered `invalid` without a kernel call.
#[must_use]
pub fn admin_route(
    method: &str,
    path: &str,
    body: &serde_json::Value,
) -> Option<Result<AdminRoute, ApiError>> {
    match (method, entry_id(path)) {
        ("POST", None) if path == ENTRIES_PATH => Some(
            ["id", "package", "hash"]
                .iter()
                .try_for_each(|name| string_field(body, name).map(drop))
                .map(|()| AdminRoute {
                    id: body["id"].as_str().unwrap_or_default().to_owned(),
                    write: AdminWrite::Add(body.clone()),
                }),
        ),
        ("DELETE", Some(id)) => Some(Ok(AdminRoute {
            id: id.to_owned(),
            write: AdminWrite::Remove,
        })),
        ("PATCH", Some(id)) => match patch_write(body) {
            Ok(Some(write)) => Some(Ok(AdminRoute {
                id: id.to_owned(),
                write,
            })),
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        },
        _ => None,
    }
}

fn segment(wire: &mut Vec<u8>, text: &str) {
    wire.extend(
        u32::try_from(text.len())
            .expect("a segment fits")
            .to_le_bytes(),
    );
    wire.extend(text.as_bytes());
}

/// The broker wire for one write: the operation and its u32-LE
/// length-prefixed UTF-8 segments, in the bundle's order.
#[must_use]
pub fn admin_payload(id: &str, write: &AdminWrite) -> (&'static str, Vec<u8>) {
    let mut wire = Vec::new();
    match write {
        AdminWrite::Add(record) => segment(&mut wire, &record.to_string()),
        AdminWrite::Remove => segment(&mut wire, id),
        AdminWrite::SetDisabled(disabled) => {
            segment(&mut wire, id);
            segment(&mut wire, if *disabled { "true" } else { "false" });
        }
        AdminWrite::SetGrants(grants) => {
            segment(&mut wire, id);
            segment(&mut wire, &grants.to_string());
        }
        AdminWrite::SwapPlugin {
            package,
            version,
            hash,
        } => {
            for text in [id, package, version, hash] {
                segment(&mut wire, text);
            }
        }
    }
    (write.operation(), wire)
}

/// The kernel's answer: tag `2` + the `ProfileAdministered` row's u64-LE
/// sequence, or tag `1` + one class byte + the reason — surfaced as this
/// seam's `refused` with the class verbatim and `retryable` for the one
/// class the bundle says is (`conflict`).
///
/// # Errors
///
/// The typed refusal, or `refused` for an answer that is neither.
pub fn admin_answer(operation: &str, bytes: &[u8]) -> Result<u64, ApiError> {
    match bytes.split_first() {
        Some((2, seq)) if seq.len() == 8 => Ok(u64::from_le_bytes(seq.try_into().expect("eight"))),
        Some((1, rest)) if !rest.is_empty() => {
            let class = usize::from(rest[0])
                .checked_sub(1)
                .and_then(|index| CLASSES.get(index))
                .copied()
                .unwrap_or("unknown");
            let reason = String::from_utf8_lossy(&rest[1..]);
            let mut error = ApiError::new(
                ErrorCode::Refused,
                format!("{operation} refused ({class}): {reason}"),
            );
            error.extra.insert("class".into(), class.into());
            error
                .extra
                .insert("retryable".into(), (class == "conflict").into());
            Err(error)
        }
        _ => Err(ApiError::new(
            ErrorCode::Refused,
            format!("{operation}: the answer is neither accepted nor a typed refusal"),
        )),
    }
}

/// An accepted write's answer: the entry, the write, and the sequence of
/// its `ProfileAdministered` row — the INTENT, landed before the commit;
/// the restart, spawn or disposal it schedules is followed on the ledger.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct AdministeredAnswer {
    pub api_version: String,
    pub id: String,
    pub write: String,
    pub administered_seq: u64,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// See [`AdministeredAnswer`].
#[must_use]
pub fn administered_answer(id: &str, operation: &str, seq: u64) -> AdministeredAnswer {
    AdministeredAnswer {
        api_version: API_VERSION.to_owned(),
        id: id.to_owned(),
        write: operation.to_owned(),
        administered_seq: seq,
        extra: Extensions::new(),
    }
}
