//! The JOIN: a catalog's declared entry set, the kernel's composition
//! snapshot, and the ledger, read into one answer.
//!
//! Both providers share every line of this. What differs between them is
//! only [`Declared`] — where the entry set came from — which is why the
//! catalog is the swappable part and the reading is not.
//!
//! # The join is three reads, and the answer says so
//!
//! [`ReadWindow`] rides on every answer carrying
//! [`crate::JOIN_QUALIFIER`]. The composition, the document (or the
//! declaration) and the ledger page are taken at three instants; an entry
//! may move between them. That is the narrowest true thing about
//! everything here, so it travels in the response rather than living only
//! in a README.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::entry::{Entry, Grant, GrantSource, Grants, Listing, ReadWindow};
use crate::history::{History, Line};
use crate::lifecycle::{Lifecycle, Reason, Snapshot, Window};
use crate::{Extensions, PluginsError, API_VERSION};

/// Where a catalog's ENTRY SET came from. The same two authorities
/// [`GrantSource`] names, because an entry set and its grants always come
/// from the same read — a catalog never mixes a declared entry with a
/// read grant list or the reverse.
pub type Source = GrantSource;

/// One entry as its SOURCE declares it — before any kernel reading.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Declared {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
    #[serde(default)]
    pub grants: Vec<Grant>,
    /// Whether the source says this entry is disabled. Only the document
    /// of record knows this; a declaration that does not say it means
    /// `false`, which is a declaration and is labelled as one by
    /// [`GrantSource`].
    #[serde(default)]
    pub disabled: bool,
    /// `config.data.origin`, when the entry declares one (an extension's
    /// attestation); read verbatim, never defaulted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

/// The `describe` answer: the entry, what it MAY do, and what it HAS
/// done.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Description {
    pub api_version: String,
    pub catalog: String,
    pub served_by: String,
    #[serde(flatten)]
    pub entry: Entry,
    /// The readings that may legally follow this one, from the seam's
    /// transition table — so an operator or an agent reads the possible
    /// next moves instead of inferring them.
    pub legal_next: Vec<String>,
    /// What its authority lets it do: one line per grant. Declared, and
    /// labelled with the same source its grants carry.
    pub declared_effects: Vec<String>,
    /// What it HAS done, within the window: a count per ledger kind.
    pub done: BTreeMap<String, u64>,
    pub read: ReadWindow,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The shared reading. Stateless: every answer is a fresh join.
pub struct Catalog;

impl Catalog {
    /// The `jinn:profile` `document()` answer as an entry set. An element
    /// without a string `id` is not an entry and is dropped; a grant this
    /// reader cannot parse is DROPPED FROM THE LIST AND COUNTED, never
    /// silently turned into no grant at all.
    ///
    /// # Errors
    ///
    /// [`PluginsError`] when the document does not carry an `entries`
    /// array — an unreadable document is never an empty one.
    pub fn parse_document(document: &serde_json::Value) -> Result<Vec<Declared>, PluginsError> {
        let entries = document
            .get("entries")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| {
                PluginsError::new(
                    crate::ErrorCode::Failed,
                    "the profile document carries no `entries` array, so this catalog has no \
                     entry set to report — which is not the same as an empty one",
                )
            })?;
        Ok(entries
            .iter()
            .filter_map(|entry| {
                let id = entry.get("id").and_then(serde_json::Value::as_str)?;
                Some(Declared {
                    id: id.to_owned(),
                    package: entry
                        .get("package")
                        .and_then(serde_json::Value::as_str)
                        .map(ToOwned::to_owned),
                    grants: entry
                        .get("grants")
                        .and_then(serde_json::Value::as_array)
                        .map(|grants| grants.iter().filter_map(Grant::parse).collect())
                        .unwrap_or_default(),
                    disabled: entry
                        .get("disabled")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                    origin: entry
                        .pointer("/config/data/origin")
                        .and_then(serde_json::Value::as_str)
                        .map(ToOwned::to_owned),
                })
            })
            .collect())
    }

    /// One entry's reading, joined from the three reads.
    #[must_use]
    pub fn entry(
        declared: &Declared,
        source: GrantSource,
        snapshot: Option<&Snapshot>,
        history: &History,
        window: Window,
    ) -> Entry {
        // The reasons, resolved BEFORE the reading, so the reading law
        // never has to invent one — and resolved in the ONE order that
        // cannot fabricate: a positive reading of the document first,
        // and the searched-and-no-cause statement for everything else.
        // The ledger is COUNTED, never cited: see the reading law's
        // module doc, and `FINDINGS.md` #38.
        let searched = Reason::NoRecordedCause {
            window,
            candidates: history.reason_bearing(),
            qualifier: crate::lifecycle::NO_CAUSE_QUALIFIER.to_owned(),
        };
        let no_fiber = if declared.disabled {
            Reason::Disabled
        } else {
            searched.clone()
        };
        let failure = searched;
        Entry {
            id: declared.id.clone(),
            package: declared.package.clone(),
            incarnation: snapshot.and_then(|s| s.incarnation),
            owes: snapshot.and_then(|s| s.unserved),
            provides: snapshot.map(|s| s.provisions.clone()).unwrap_or_default(),
            grants: Grants::read(source, declared.grants.clone()),
            lifecycle: Lifecycle::read(snapshot, no_fiber, failure),
            attestation: declared
                .origin
                .clone()
                .map(|origin| crate::entry::Attestation { origin }),
            extra: Extensions::new(),
        }
    }

    /// Every entry this catalog holds.
    #[must_use]
    pub fn list(
        catalog: &str,
        served_by: &str,
        declared: &[Declared],
        source: GrantSource,
        snapshots: &BTreeMap<String, Snapshot>,
        page: &[Line],
        window: Window,
    ) -> Listing {
        let entries = declared
            .iter()
            .map(|entry| {
                let history = History::of(&entry.id, page.to_vec(), window);
                Self::entry(entry, source, snapshots.get(&entry.id), &history, window)
            })
            .collect();
        let mut listing = Listing::new(catalog, served_by, entries, ReadWindow::new(window));
        // Entries the KERNEL reports that this catalog does not declare.
        // Empty by construction for a profile-derived catalog; for a
        // fixed one it is the difference between the appliance it was
        // built for and the machine it is running on, and an operator
        // reading a catalog is owed that difference rather than a list
        // that quietly omits it.
        let unlisted: Vec<&String> = snapshots
            .keys()
            .filter(|id| !declared.iter().any(|entry| &&entry.id == id))
            .collect();
        listing
            .extra
            .insert("unlisted".to_owned(), serde_json::json!(unlisted));
        listing
    }

    /// One entry, in full.
    #[must_use]
    pub fn describe(
        catalog: &str,
        served_by: &str,
        declared: &Declared,
        source: GrantSource,
        snapshot: Option<&Snapshot>,
        history: &History,
        window: Window,
    ) -> Description {
        let entry = Self::entry(declared, source, snapshot, history, window);
        let mut done: BTreeMap<String, u64> = BTreeMap::new();
        for line in &history.lines {
            *done.entry(line.kind.clone()).or_default() += 1;
        }
        Description {
            api_version: API_VERSION.to_owned(),
            catalog: catalog.to_owned(),
            served_by: served_by.to_owned(),
            legal_next: entry
                .lifecycle
                .legal_next()
                .iter()
                .map(|name| (*name).to_owned())
                .collect(),
            declared_effects: entry.grants.values.iter().map(declared_effect).collect(),
            done,
            entry,
            read: ReadWindow::new(window),
            extra: Extensions::new(),
        }
    }
}

/// One grant as a sentence about what the entry MAY do.
fn declared_effect(grant: &Grant) -> String {
    let scope = grant
        .scope
        .as_ref()
        .map(|scope| format!(" scoped to {scope}"))
        .unwrap_or_default();
    let ops = grant
        .ops
        .as_ref()
        .map(|ops| format!(" limited to {}", ops.join(", ")))
        .unwrap_or_default();
    format!("may call {}{scope}{ops}", grant.contract)
}
