//! PROOF 6 of the UI-1 card (`docs/plans/ui-malleability-arc.md` §4.3):
//! the view layer is VERBATIM. Not a daemon proof — a repo gate that reads
//! the pinned mapping `web/port-map.txt` and, for every row, compares the
//! bytes under `web/` with the bytes at jinn `43e8647` (`web::SOURCE_SHA`):
//! an EMPTY diff for every file the card did not enumerate as an
//! adaptation, a NON-EMPTY one for every file it did, and no source at
//! all for a file declared new. The gate fails in both directions — a
//! tidied quirk and an un-adapted adaptation are the same defect class —
//! and it fails on completeness: every tracked file under `web/` must be
//! on the map, and every row on the map must be in the tree.
//!
//! The source is a jinn checkout holding the pinned sha: `JINN_WEB_SOURCE_DIR`
//! or a sibling `../jinn`. The pinned sha itself is a local merge that was
//! never pushed (`43e8647` = `origin/main` at `b2dd57c1` merged with two
//! commits touching nothing under the ported trees), so a fresh machine
//! cannot fetch it; it fetches the PUBLIC TWIN `b2dd57c1` instead, whose
//! `packages/web` and `packages/gateway-events` trees are the same tree
//! objects — asserted here whenever both commits are reachable, so the
//! twin can never drift from the pin unnoticed. jinn is public, so unlike
//! the kernel pin's Gate 2 this gate never self-skips: no source is a
//! failure, not a skip.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The jinn commit the view layer is ported from (the card's pinned sha).
const SOURCE_SHA: &str = "43e864750168e163b55855a79f955e471da0bcc1";
/// The public commit carrying the identical ported trees (see the module
/// doc); the one a fresh machine fetches. Reachable from `main` after
/// 2026-08-30, which is what the shallow fetch asks for.
const PUBLIC_TWIN: &str = "b2dd57c15d7f93fe30e0d50f0cf502327318f908";
const TWIN_SINCE: &str = "2026-08-29";
const SOURCE_URL: &str = "https://github.com/hristo2612/jinn.git";
/// The trees that must be the same object at the pin and at its twin.
const IDENTICAL_TREES: [&str; 2] = ["packages/web", "packages/gateway-events"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("tools/ui-kit sits two levels under the root")
        .to_path_buf()
}

fn git_ok(dir: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .is_ok_and(|output| output.status.success())
}

fn has_commit(dir: &Path, commit: &str) -> bool {
    dir.is_dir() && git_ok(dir, &["cat-file", "-e", &format!("{commit}^{{commit}}")])
}

/// A jinn repo and the commit in it to read from: the pin where a local
/// checkout holds it, the public twin otherwise (fetched if needed).
fn source() -> (PathBuf, &'static str) {
    if let Ok(dir) = std::env::var("JINN_WEB_SOURCE_DIR") {
        let dir = PathBuf::from(dir);
        for commit in [SOURCE_SHA, PUBLIC_TWIN] {
            if has_commit(&dir, commit) {
                return (dir, commit);
            }
        }
        panic!("JINN_WEB_SOURCE_DIR holds neither {SOURCE_SHA} nor {PUBLIC_TWIN}");
    }
    let sibling = repo_root().join("../jinn");
    for commit in [SOURCE_SHA, PUBLIC_TWIN] {
        if has_commit(&sibling, commit) {
            return (sibling, commit);
        }
    }
    let cache = repo_root().join("target/ui-kit/jinn-source");
    if has_commit(&cache, PUBLIC_TWIN) {
        return (cache, PUBLIC_TWIN);
    }
    let _ = std::fs::remove_dir_all(&cache);
    std::fs::create_dir_all(&cache).expect("source cache");
    assert!(git_ok(&cache, &["init", "-q"]), "git init");
    eprintln!("fetching jinn main since {TWIN_SINCE} (shallow) for the verbatim gate…");
    assert!(
        git_ok(
            &cache,
            &[
                "fetch",
                "-q",
                &format!("--shallow-since={TWIN_SINCE}"),
                SOURCE_URL,
                "main"
            ]
        ),
        "shallow fetch of main from {SOURCE_URL} failed"
    );
    assert!(
        has_commit(&cache, PUBLIC_TWIN),
        "the fetch did not land the public twin {PUBLIC_TWIN}"
    );
    (cache, PUBLIC_TWIN)
}

fn tree_id(source: &Path, commit: &str, path: &str) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(source)
        .args(["rev-parse", &format!("{commit}:{path}")])
        .output()
        .expect("git rev-parse");
    assert!(output.status.success(), "{commit}:{path} resolves");
    String::from_utf8(output.stdout)
        .expect("a hex id")
        .trim()
        .to_owned()
}

/// The bytes of `path` at `commit`, `None` when nothing is there.
fn at_sha(source: &Path, commit: &str, path: &str) -> Option<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(source)
        .args(["show", &format!("{commit}:{path}")])
        .output()
        .expect("git show");
    output.status.success().then_some(output.stdout)
}

/// Whenever a checkout holds BOTH commits, the twin's ported trees are the
/// pin's own tree objects — the fact that makes reading from the twin the
/// same as reading from the pin.
#[test]
fn the_public_twin_carries_the_pinned_trees() {
    let (source, _) = source();
    if !(has_commit(&source, SOURCE_SHA) && has_commit(&source, PUBLIC_TWIN)) {
        eprintln!(
            "twin identity: only one of the two commits is reachable here; asserted where both are"
        );
        return;
    }
    for tree in IDENTICAL_TREES {
        assert_eq!(
            tree_id(&source, SOURCE_SHA, tree),
            tree_id(&source, PUBLIC_TWIN, tree),
            "{tree} at {SOURCE_SHA} and at {PUBLIC_TWIN} are one tree object"
        );
    }
}

#[derive(Debug)]
struct Row {
    line: usize,
    status: String,
    source: String,
    dest: String,
}

fn map() -> Vec<Row> {
    let path = repo_root().join("web/port-map.txt");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty() && !line.starts_with('#'))
        .map(|(index, line)| {
            let fields: Vec<&str> = line.split('\t').collect();
            assert!(
                fields.len() >= 3,
                "port-map.txt:{}: three tab-separated fields",
                index + 1
            );
            Row {
                line: index + 1,
                status: fields[0].to_owned(),
                source: fields[1].to_owned(),
                dest: fields[2].to_owned(),
            }
        })
        .collect()
}

fn tracked_under_web() -> BTreeSet<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root())
        .args(["ls-files", "--", "web"])
        .output()
        .expect("git ls-files");
    assert!(output.status.success(), "git ls-files -- web");
    String::from_utf8(output.stdout)
        .expect("utf-8 paths")
        .lines()
        .filter_map(|line| line.strip_prefix("web/"))
        .filter(|path| *path != "port-map.txt")
        .map(str::to_owned)
        .collect()
}

#[test]
fn the_view_layer_is_verbatim() {
    let rows = map();
    let (source, commit) = source();
    let web = repo_root().join("web");
    let mut failures = Vec::new();
    let mut counts = (0usize, 0usize, 0usize);

    let mut dests = BTreeSet::new();
    for row in &rows {
        assert!(
            dests.insert(row.dest.clone()),
            "port-map.txt:{}: {} is mapped twice",
            row.line,
            row.dest
        );
        let here = std::fs::read(web.join(&row.dest)).ok();
        match row.status.as_str() {
            "verbatim" => {
                counts.0 += 1;
                match (at_sha(&source, commit, &row.source), here) {
                    (None, _) => {
                        failures.push(format!("{}: no {} at the sha", row.dest, row.source))
                    }
                    (_, None) => failures.push(format!("{}: not in the tree", row.dest)),
                    (Some(theirs), Some(ours)) if theirs != ours => failures.push(format!(
                        "{}: DIFFERS from {} at the sha (verbatim row)",
                        row.dest, row.source
                    )),
                    _ => {}
                }
            }
            "adapted" => {
                counts.1 += 1;
                match (at_sha(&source, commit, &row.source), here) {
                    (None, _) => {
                        failures.push(format!("{}: no {} at the sha", row.dest, row.source))
                    }
                    (_, None) => failures.push(format!("{}: not in the tree", row.dest)),
                    (Some(theirs), Some(ours)) if theirs == ours => failures.push(format!(
                        "{}: IDENTICAL to {} at the sha (adapted row — the adaptation is missing)",
                        row.dest, row.source
                    )),
                    _ => {}
                }
            }
            "new" => {
                counts.2 += 1;
                assert_eq!(
                    row.source, "-",
                    "port-map.txt:{}: a new row's source is `-`",
                    row.line
                );
                if here.is_none() {
                    failures.push(format!("{}: not in the tree", row.dest));
                }
                if at_sha(&source, commit, &format!("packages/web/{}", row.dest)).is_some() {
                    failures.push(format!(
                        "{}: declared new but packages/web/{} exists at the sha",
                        row.dest, row.dest
                    ));
                }
            }
            other => panic!("port-map.txt:{}: unknown status {other:?}", row.line),
        }
    }

    // Completeness, both directions.
    let tracked = tracked_under_web();
    for unmapped in tracked.difference(&dests) {
        failures.push(format!("{unmapped}: tracked under web/ but not on the map"));
    }
    for untracked in dests.difference(&tracked) {
        failures.push(format!(
            "{untracked}: on the map but not tracked under web/"
        ));
    }

    eprintln!(
        "verbatim gate: {} verbatim, {} adapted, {} new against jinn {SOURCE_SHA} (read at {commit})",
        counts.0, counts.1, counts.2
    );
    assert!(
        failures.is_empty(),
        "the view layer is not verbatim ({} failures):\n  {}",
        failures.len(),
        failures.join("\n  ")
    );
}
