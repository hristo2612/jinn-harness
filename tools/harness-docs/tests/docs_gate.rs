//! The documentation gate (`cargo test -p harness-docs`).
//!
//! Both checks are always on and fail closed. They run over the working
//! tree, so they hold for a checkout, a worktree or a tarball alike, and
//! they need no network, no daemon and no kernel.
//!
//! What each enforces is defined once, in `harness_docs`. This file is
//! only the application of those rules to the real files.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn read(root: &Path, relative: &str) -> String {
    std::fs::read_to_string(root.join(relative)).unwrap_or_else(|e| panic!("{relative}: {e}"))
}

/// The README's limitations map is the artifact the M3 parity
/// conversation starts from. A limit it asserts while `FINDINGS.md`
/// grades that same entry ANSWERED or CORRECTED is a claim its own
/// source has withdrawn.
#[test]
fn the_limitations_map_asserts_nothing_findings_has_withdrawn() {
    let root = repo_root();
    let stale =
        harness_docs::stale_limitations(&read(&root, "README.md"), &read(&root, "FINDINGS.md"));
    assert!(
        stale.is_empty(),
        "the limitations map did not move with the thing:\n{}",
        stale
            .iter()
            .map(|s| format!("  - {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// A comment citing a note that was never written is a citation to
/// nothing, and an unverifiable citation is worse than none.
#[test]
fn every_note_cited_anywhere_in_the_tree_exists() {
    let root = repo_root();
    let mut dangling: Vec<String> = Vec::new();
    let mut cited_total = 0usize;
    for file in text_files(&root) {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let relative = file.strip_prefix(&root).unwrap_or(&file).display();
        for citation in harness_docs::note_citations(&text) {
            cited_total += 1;
            if !root.join(&citation).is_file() {
                dangling.push(format!(
                    "  - {relative} cites `{citation}`, which is not a file"
                ));
            }
        }
    }
    assert!(
        cited_total > 0,
        "the walk found no note citation at all — the gate would pass vacuously"
    );
    assert!(
        dangling.is_empty(),
        "citations to nothing:\n{}",
        dangling.join("\n")
    );
}

/// Every file in the tree except build output, git internals and the
/// vendored kernel surface, which is byte-identical to the pin archive
/// and is not ours to edit.
fn text_files(root: &Path) -> Vec<PathBuf> {
    const SKIP: [&str; 4] = [".git", "target", "node_modules", "kernel-pin"];
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    let skip: BTreeSet<&str> = SKIP.into_iter().collect();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                if !skip.contains(name.as_ref()) {
                    stack.push(path);
                }
            } else {
                found.push(path);
            }
        }
    }
    found
}
