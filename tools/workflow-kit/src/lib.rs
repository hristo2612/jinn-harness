//! The workflows seam's profile entries (one home per fact: every profile
//! that mounts the seam mounts these exact entries).
//!
//! Authority is the profile side's, as always. A run store is granted:
//! its own `jinn:workflow.<store-id>` contract (providing IS authority —
//! the kernel checks the grant on `provide` exactly as on a call),
//! `jinn:clock` for the poll wakes that drive its dispatches, and one
//! `jinn:todo.<store>` per TODO STORE its nodes may dispatch to. A
//! DURABLE store is granted a `jinn:fs` scope naming the one directory
//! its journals live in, and nothing outside it. An ephemeral store gets
//! no `jinn:fs` grant at all, which is the authority half of
//! `durable: false`.
//!
//! Note what a run store is NOT granted: no `jinn:session.<id>` and no
//! `jinn:engine.<id>`, at all. It cannot reach a session or an engine
//! even if its code tried, because the session is the Todo's business
//! and the engine is the session's. The four-layer layering is therefore
//! enforced by the KERNEL rather than by this seam's good behaviour.

/// The SWITCHABLE slot: the entry whose store id is `default`. The swap
/// proof moves its PACKAGE from the durable store to the ephemeral one
/// and leaves this id, this store id, the API, the todos seam, the
/// sessions seam and every engine alone.
pub const DEFAULT_ID: &str = "jinn-workflow-default";
/// The store id [`DEFAULT_ID`] serves.
pub const DEFAULT_STORE: &str = "default";
/// The COEXISTENCE half: a second store, live at the same time, on its
/// own contract name, routed per run by the store in the path.
pub const MEMORY_ID: &str = "jinn-workflow-memory";
/// See [`MEMORY_ID`].
pub const MEMORY_STORE: &str = "memory";
/// The EXTENSION proof's entry: NOT in the base profile — the composition
/// suite adds a third store to a live daemon by profile edit alone,
/// against an artifact the kit already built and with no change to the
/// definition.
pub const SCRATCH_ID: &str = "jinn-workflow-scratch";
/// See [`SCRATCH_ID`].
pub const SCRATCH_STORE: &str = "scratch";

/// The durable store's package.
pub const FS_PACKAGE: &str = "workflows/jinn-workflow-fs";
/// The ephemeral store's package.
pub const MEMORY_PACKAGE: &str = "workflows/jinn-workflow-memory";

/// Where the durable store's journals live, under the daemon's data root.
pub const JOURNAL_DIR: &str = "workflows";

/// Every workflow guest the kit builds.
pub const GUESTS: [&str; 2] = ["jinn-workflow-fs", "jinn-workflow-memory"];

/// One run store entry's authority and its own knowledge.
pub struct Store<'a> {
    /// The profile entry id.
    pub id: &'a str,
    /// The package (and artifact) serving it.
    pub package: &'a str,
    /// Its content hash — the profile's pin (kernel Law 5).
    pub hash: &'a str,
    /// The store id, which names the contract it provides.
    pub store: &'a str,
    /// Where its journals go, for a durable store. `None` grants no
    /// `jinn:fs` at all — an ephemeral store cannot write even by
    /// accident.
    pub dir: Option<&'a str>,
    /// The TODO stores its nodes may dispatch to. One grant per Todo
    /// store is per-store authority the KERNEL enforces.
    pub todos: &'a [&'a str],
    /// How often a live node's Todo is polled.
    pub poll_ms: u64,
}

/// One run store entry: grants on the left, its own knowledge on the
/// right.
#[must_use]
pub fn store_entry(store: &Store<'_>) -> serde_json::Value {
    // The topic it EMITS on beside the contract it provides: at pin
    // `138fdce` an emit is covered by the topic's own grant (jinnd M2-K26
    // (e); FINDINGS #49).
    let mut grants = vec![
        serde_json::json!(jinn_workflow::store_contract(store.store)),
        serde_json::json!(jinn_workflow::EVENT_TOPIC),
        serde_json::json!(jinn_cron::CLOCK_CONTRACT),
    ];
    grants.extend(
        store
            .todos
            .iter()
            .map(|todo| serde_json::json!(jinn_todo::store_contract(todo))),
    );
    let mut data = serde_json::json!({
        "store": store.store,
        "poll-ms": store.poll_ms,
    });
    if let Some(dir) = store.dir {
        grants.push(serde_json::json!({ "contract": "jinn:fs", "scope": dir }));
        data["dir"] = serde_json::json!(dir);
    }
    serde_json::json!({ "id": store.id, "package": store.package, "hash": store.hash,
                        "config": { "grants": grants, "data": data } })
}

/// The workflow contracts the operator API may route to. As with the
/// engines, the session stores and the Todo stores, the grant list IS
/// the authority the kernel enforces.
#[must_use]
pub fn api_workflow_grants(stores: &[&str]) -> Vec<serde_json::Value> {
    stores
        .iter()
        .map(|store| serde_json::json!(jinn_workflow::store_contract(store)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_durable_store_may_write_exactly_one_directory() {
        let entry = store_entry(&Store {
            id: DEFAULT_ID,
            package: FS_PACKAGE,
            hash: "abc",
            store: DEFAULT_STORE,
            dir: Some(JOURNAL_DIR),
            todos: &["default"],
            poll_ms: 250,
        });
        let grants = entry["config"]["grants"].as_array().expect("grants");
        assert_eq!(grants[0], "jinn:workflow.default");
        assert!(grants.contains(&serde_json::json!("jinn:todo.default")));
        let fs = grants
            .iter()
            .find(|grant| grant["contract"] == "jinn:fs")
            .expect("an fs grant");
        assert_eq!(fs["scope"], JOURNAL_DIR);
        assert_eq!(entry["config"]["data"]["dir"], JOURNAL_DIR);
    }

    /// The run store EMITS `jinn:workflow/event`; at pin `138fdce` (jinnd
    /// M2-K26 (e); FINDINGS #49) an emit is covered by the topic's own
    /// grant, so every store entry — durable or not — carries it.
    #[test]
    fn a_store_entry_is_granted_the_event_topic_it_emits() {
        for dir in [Some(JOURNAL_DIR), None] {
            let entry = store_entry(&Store {
                id: DEFAULT_ID,
                package: FS_PACKAGE,
                hash: "abc",
                store: DEFAULT_STORE,
                dir,
                todos: &["default"],
                poll_ms: 250,
            });
            let grants = entry["config"]["grants"].as_array().expect("grants");
            assert!(
                grants.contains(&serde_json::json!(jinn_workflow::EVENT_TOPIC)),
                "the emitter is granted its topic (dir {dir:?}): {grants:?}"
            );
        }
    }

    #[test]
    fn an_ephemeral_store_holds_no_write_authority_at_all() {
        let entry = store_entry(&Store {
            id: MEMORY_ID,
            package: MEMORY_PACKAGE,
            hash: "def",
            store: MEMORY_STORE,
            dir: None,
            todos: &["default"],
            poll_ms: 250,
        });
        let grants = entry["config"]["grants"].as_array().expect("grants");
        assert!(
            !grants.iter().any(|grant| grant["contract"] == "jinn:fs"),
            "durable: false is an authority fact, not only a declaration"
        );
        assert!(entry["config"]["data"].get("dir").is_none());
    }

    #[test]
    fn a_run_store_dispatches_to_exactly_the_todo_stores_it_is_granted() {
        let entry = store_entry(&Store {
            id: DEFAULT_ID,
            package: FS_PACKAGE,
            hash: "abc",
            store: DEFAULT_STORE,
            dir: None,
            todos: &["default", "memory"],
            poll_ms: 100,
        });
        let grants = entry["config"]["grants"].as_array().expect("grants");
        assert!(grants.contains(&serde_json::json!("jinn:todo.memory")));
        assert!(!grants.contains(&serde_json::json!("jinn:todo.scratch")));
        assert_eq!(entry["config"]["data"]["poll-ms"], 100);
    }

    #[test]
    fn a_run_store_is_granted_neither_a_session_nor_an_engine() {
        let entry = store_entry(&Store {
            id: DEFAULT_ID,
            package: FS_PACKAGE,
            hash: "abc",
            store: DEFAULT_STORE,
            dir: Some(JOURNAL_DIR),
            todos: &["default", "memory"],
            poll_ms: 250,
        });
        let grants = entry["config"]["grants"].as_array().expect("grants");
        assert!(
            !grants.iter().any(|grant| grant
                .as_str()
                .is_some_and(|name| name.starts_with("jinn:session."))),
            "the session is the Todo's business; the layering is a grant, not a code path"
        );
        assert!(
            !grants.iter().any(|grant| grant
                .as_str()
                .is_some_and(|name| name.starts_with("jinn:engine."))),
            "the engine is the session's business, two layers below this one"
        );
    }
}
