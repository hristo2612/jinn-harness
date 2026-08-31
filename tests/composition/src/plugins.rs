//! The plugins seam's composition fixtures: one home for the gate, the
//! boot and the reads every plugins proof shares.

use std::path::PathBuf;
use std::sync::OnceLock;

use crate::api::{get, Response};
use crate::daemon::{jinnd_source, pinned_commit, pinned_daemon};
use crate::kit::{fresh_plugins_root, Daemon};

/// The catalog id an operator addresses.
pub const MAIN: &str = "main";
/// Where a catalog goes when it gives up the switchable name.
pub const PARKED: &str = "parked";

/// The pinned daemon binary, or a LOUD skip.
#[must_use]
pub fn gate() -> Option<&'static PathBuf> {
    static BINARY: OnceLock<Option<PathBuf>> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            let commit = pinned_commit().expect("KERNEL-PIN.md parses");
            let Some(source) = jinnd_source(&commit) else {
                eprintln!(
                    "SKIPPED (loudly): real-composition gate found no jinnd checkout holding \
                     pinned commit {commit} — set JINND_DIR, add a sibling ../jinnd, or set \
                     JINND_CLONE_URL (KERNEL-PIN.md Gate 2 discipline)"
                );
                return None;
            };
            Some(pinned_daemon(&source, &commit).expect("the pinned daemon builds"))
        })
        .as_ref()
}

/// A booted plugins profile, or `None` behind the gate.
///
/// # Panics
///
/// If the daemon boots and does not answer its own health route.
#[must_use]
pub fn booted(name: &str) -> Option<(Daemon, u16)> {
    let binary = gate()?;
    let (root, port) = fresh_plugins_root(name);
    let daemon = Daemon::boot_operator(binary, &root);
    daemon.await_ready();
    let health = get(port, "/v1/health");
    assert_eq!(health.status, 200, "{}", health.raw);
    Some((daemon, port))
}

/// One catalog's listing.
///
/// # Panics
///
/// If the catalog does not answer.
#[must_use]
pub fn listing(port: u16, catalog: &str) -> serde_json::Value {
    let read = get(port, &format!("/v1/plugins/{catalog}"));
    assert_eq!(read.status, 200, "{}", read.raw);
    read.body
}

/// One entry out of a listing.
///
/// # Panics
///
/// If the listing does not hold it.
#[must_use]
pub fn entry(listing: &serde_json::Value, id: &str) -> serde_json::Value {
    listing["entries"]
        .as_array()
        .unwrap_or_else(|| panic!("a listing: {listing}"))
        .iter()
        .find(|entry| entry["id"] == id)
        .unwrap_or_else(|| panic!("entry {id:?} in the listing: {listing}"))
        .clone()
}

/// One entry, described.
#[must_use]
pub fn described(port: u16, catalog: &str, id: &str) -> Response {
    get(port, &format!("/v1/plugins/{catalog}/{id}"))
}

/// One entry's history.
///
/// # Panics
///
/// If the history does not answer.
#[must_use]
pub fn history(port: u16, catalog: &str, id: &str) -> serde_json::Value {
    let read = get(port, &format!("/v1/plugins/{catalog}/{id}/history"));
    assert_eq!(read.status, 200, "{}", read.raw);
    read.body
}

/// The `state` an entry reads as.
///
/// # Panics
///
/// If the entry carries no lifecycle at all.
#[must_use]
pub fn state(entry: &serde_json::Value) -> String {
    entry["lifecycle"]["state"]
        .as_str()
        .unwrap_or_else(|| panic!("a lifecycle: {entry}"))
        .to_owned()
}
