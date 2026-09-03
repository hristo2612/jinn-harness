//! What a catalog SAYS about one plugin, and the qualifier every answer
//! carries.
//!
//! # Two authorities, never mixed
//!
//! An entry's grants come from one of exactly two places, and the answer
//! says which: the DOCUMENT OF RECORD (`jinn:profile`), which is what the
//! kernel actually enforces, or the catalog's own DECLARATION, which is
//! what a fixed catalog was built with. They are not interchangeable and
//! they are not merged. A declared grant list is a claim about a profile
//! this catalog has not read; a read one is the authority itself. A
//! reader that could not tell them apart would let an appliance catalog's
//! build-time opinion pass for the kernel's enforcement, which is the
//! whole reason [`GrantSource`] is on the wire beside the values.
//!
//! # A plugin granted nothing reports nothing
//!
//! An empty [`Grants::values`] is a POSITIVE reading: the source was
//! consulted and it listed no grant. A source that could not be consulted
//! never produces an entry with empty grants — it produces no entry at
//! all, or a typed refusal. There is no path from "I could not read" to
//! "it has no authority".

use serde::{Deserialize, Serialize};

use crate::lifecycle::{Lifecycle, Unserved, Window};
use crate::{Extensions, API_VERSION};

/// Where a grant list came from. On the wire beside the values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GrantSource {
    /// Read from the document of record through `jinn:profile` — the
    /// authority the kernel enforces.
    ProfileDocument,
    /// Declared in this catalog's own configuration. A claim about a
    /// profile this catalog did not read, and labelled as one.
    CatalogDeclaration,
}

impl GrantSource {
    /// Whether a list from this source is the authority the kernel
    /// enforces, or a claim about it. The narrower guarantee travels
    /// with the values rather than living only in a README.
    #[must_use]
    pub fn qualifier(self) -> &'static str {
        match self {
            Self::ProfileDocument => "read from the document of record; this is the authority the kernel enforces",
            Self::CatalogDeclaration => "declared in this catalog's configuration; NOT read from the document of record, so it is a claim about the authority and not the authority",
        }
    }
}

/// One authority an entry holds, as the document or the declaration
/// spells it: a bare contract name, or a contract attenuated by scope
/// and/or operation class.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Grant {
    pub contract: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ops: Option<Vec<String>>,
}

impl Grant {
    /// Reads one grant as the profile spells it: `"jinn:ledger"` or
    /// `{ "contract": ..., "scope": ..., "ops": [...] }`. Anything else
    /// is not a grant and is answered `None` rather than coerced — an
    /// unreadable grant must never become an empty one.
    #[must_use]
    pub fn parse(value: &serde_json::Value) -> Option<Self> {
        if let Some(contract) = value.as_str() {
            return Some(Self {
                contract: contract.to_owned(),
                scope: None,
                ops: None,
            });
        }
        let object = value.as_object()?;
        Some(Self {
            contract: object.get("contract")?.as_str()?.to_owned(),
            scope: object.get("scope").cloned(),
            ops: object.get("ops").and_then(|ops| {
                ops.as_array().map(|ops| {
                    ops.iter()
                        .filter_map(|op| op.as_str().map(ToOwned::to_owned))
                        .collect()
                })
            }),
        })
    }
}

/// An entry's authority, and where the reading came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Grants {
    pub source: GrantSource,
    /// The grants themselves. EMPTY means "consulted, and none" — never
    /// "could not consult".
    pub values: Vec<Grant>,
    /// How far this source's word actually goes ([`GrantSource::qualifier`]).
    pub qualifier: String,
}

impl Grants {
    /// A grant list read from a source that WAS consulted.
    #[must_use]
    pub fn read(source: GrantSource, values: Vec<Grant>) -> Self {
        Self {
            source,
            values,
            qualifier: source.qualifier().to_owned(),
        }
    }
}

/// One plugin as a catalog reports it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Entry {
    /// The profile entry id.
    pub id: String,
    /// The package the entry names, when the source carries one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    /// The incarnation the kernel reports for this entry, when it has
    /// one. The EVIDENCE behind `active`: the reading law reaches
    /// `Lifecycle::Active` only with an installed incarnation and
    /// nothing owed, so carrying the number is what lets a consumer
    /// check the claim instead of trusting it. Absent for an entry with
    /// no installed incarnation, which is a reading and not a gap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub incarnation: Option<u64>,
    /// What the entry's LIVE incarnation already owes, as the kernel
    /// reports it. The THIRD fact behind `active`: the reading law
    /// reaches [`Lifecycle::Active`] only with an incarnation installed
    /// AND nothing owed, and until this rode beside the claim a consumer
    /// could check two of those three facts and had to TRUST the reader
    /// on the last. Absent means "the incarnation owes nothing", which
    /// is a positive reading and the only shape `active` may take.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owes: Option<Unserved>,
    /// The contracts this entry PROVIDES, as the kernel reports them —
    /// the binding an operator swaps. Empty for an entry with no live
    /// incarnation, which is a reading and not an absence of one.
    #[serde(default)]
    pub provides: Vec<String>,
    /// Its authority, and where the reading came from.
    pub grants: Grants,
    /// Its life, as the reading law licenses it.
    pub lifecycle: Lifecycle,
    /// The operator's attestation on the entry, when it declares one:
    /// `config.data.origin` read verbatim (the extension tier's
    /// `origin: agent | human`, UI-2 §9.2; constitution 05's provenance
    /// restated for data). ABSENT for every entry that declares none,
    /// never defaulted: a reading, not a state machine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attestation: Option<Attestation>,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// See [`Entry::attestation`]. `source` is the digest of the entry's
/// `config.data.source` (`sha256:<hex>`, the guest's own breadcrumb): the
/// page's source breadcrumb comes from HERE, a stable reading, never from
/// a sliding history window (§9.7 amendment 8(d)).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Attestation {
    pub origin: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// The read a catalog actually performed to answer. It travels with
/// EVERY answer, because a `no-recorded-cause` reason and an entry's
/// lifecycle are both only as strong as the reads behind them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ReadWindow {
    /// The ledger span consulted for reasons and history.
    pub ledger: Window,
    /// Stated on the wire because it is the narrowest thing about every
    /// answer here: the composition, the document and the ledger are
    /// THREE reads at three instants, never one atomic view.
    pub qualifier: String,
}

/// The qualifier every answer carries. Its one home.
pub const JOIN_QUALIFIER: &str =
    "joined from three separate reads (jinn:introspect entries, the profile document \
     or this catalog's declaration, and a jinn:ledger page) taken at three instants; \
     it is not one atomic view, so an entry may have moved between them";

impl ReadWindow {
    #[must_use]
    pub fn new(ledger: Window) -> Self {
        Self {
            ledger,
            qualifier: JOIN_QUALIFIER.to_owned(),
        }
    }
}

/// The `list` answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Listing {
    pub api_version: String,
    /// The catalog id — the half of `jinn:plugins.<catalog>` an operator
    /// addresses.
    pub catalog: String,
    /// The PACKAGE answering this catalog id: the binding a profile edit
    /// swaps, reported so a swap is observable in the answer itself.
    pub served_by: String,
    pub entries: Vec<Entry>,
    pub read: ReadWindow,
    #[serde(flatten)]
    pub extra: Extensions,
}

impl Listing {
    #[must_use]
    pub fn new(catalog: &str, served_by: &str, entries: Vec<Entry>, read: ReadWindow) -> Self {
        Self {
            api_version: API_VERSION.to_owned(),
            catalog: catalog.to_owned(),
            served_by: served_by.to_owned(),
            entries,
            read,
            extra: Extensions::new(),
        }
    }
}
