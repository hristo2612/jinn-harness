//! Kernel pin machinery.
//!
//! The harness pins its kernel (`jinnd`) by exact commit plus a contract hash
//! over each pinned contract-surface directory. The hash algorithm is defined
//! in `KERNEL-PIN.md` and implemented exactly once, here, so the computing
//! tool and the verifying gate cannot drift.

use std::io;
use std::path::Path;

/// One parsed `KERNEL-PIN.md`.
#[derive(Debug, PartialEq, Eq)]
pub struct KernelPin {
    pub repo: String,
    pub commit: String,
    pub wit_hash: String,
    pub contracts_hash: String,
}

/// Parse the pin fields out of `KERNEL-PIN.md` text.
///
/// Recognizes `key: value` lines (anywhere in the document) for the keys
/// `repo`, `commit`, `wit-hash`, `contracts-hash`. Every key is required.
pub fn parse_pin(text: &str) -> Result<KernelPin, String> {
    let field = |key: &str| -> Result<String, String> {
        let prefix = format!("{key}:");
        text.lines()
            .filter_map(|line| line.trim().strip_prefix(&prefix))
            .map(|rest| rest.trim().to_string())
            .find(|v| !v.is_empty())
            .ok_or_else(|| format!("KERNEL-PIN.md is missing the `{key}` field"))
    };
    Ok(KernelPin {
        repo: field("repo")?,
        commit: field("commit")?,
        wit_hash: field("wit-hash")?,
        contracts_hash: field("contracts-hash")?,
    })
}

/// Contract hash of a directory tree on disk.
///
/// Algorithm (normative; also stated in `KERNEL-PIN.md`): collect every
/// regular file under `dir` recursively; for each, form its path relative to
/// `dir` with `/` separators; sort the paths bytewise; feed
/// `"<path>\n<sha256-hex-of-content>\n"` per file, in order, into SHA-256;
/// the result is `"sha256:" + lowercase hex`.
pub fn contract_hash(dir: &Path) -> io::Result<String> {
    let mut files = Vec::new();
    collect_files(dir, dir, &mut files)?;
    files.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let entries = files
        .into_iter()
        .map(|(rel, abs)| Ok((rel, std::fs::read(abs)?)))
        .collect::<io::Result<Vec<_>>>()?;
    Ok(hash_entries(
        entries.iter().map(|(r, c)| (r.as_str(), c.as_slice())),
    ))
}

/// Recursively collect `(relative-path, absolute-path)` for regular files.
fn collect_files(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(String, std::path::PathBuf)>,
) -> io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_files(root, &path, out)?;
        } else if entry.file_type()?.is_file() {
            let rel = path
                .strip_prefix(root)
                .expect("child of root")
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            out.push((rel, path));
        }
    }
    Ok(())
}

/// The normative hash over sorted `(relative-path, content)` entries.
fn hash_entries<'a>(entries: impl Iterator<Item = (&'a str, &'a [u8])>) -> String {
    use sha2::{Digest, Sha256};
    let mut outer = Sha256::new();
    for (rel, content) in entries {
        let inner = format!("{:x}", Sha256::digest(content));
        outer.update(rel.as_bytes());
        outer.update(b"\n");
        outer.update(inner.as_bytes());
        outer.update(b"\n");
    }
    format!("sha256:{:x}", outer.finalize())
}

/// Contract hash of `subdir` as recorded at `commit` in the git repo at
/// `repo`. Same algorithm as [`contract_hash`], with file contents read from
/// the commit's tree (the working tree is never consulted).
pub fn contract_hash_of_git_tree(
    repo: &Path,
    commit: &str,
    subdir: &str,
) -> Result<String, String> {
    let git = |args: &[&str]| -> Result<Vec<u8>, String> {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .map_err(|e| format!("failed to run git: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "git {args:?} failed: {}",
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        Ok(out.stdout)
    };
    let listing = String::from_utf8(git(&[
        "ls-tree",
        "-r",
        "--name-only",
        commit,
        "--",
        subdir,
    ])?)
    .map_err(|e| e.to_string())?;
    let prefix = format!("{subdir}/");
    let mut paths: Vec<&str> = listing.lines().filter(|l| !l.is_empty()).collect();
    paths.sort_by(|a, b| {
        let ra = a.strip_prefix(&prefix).unwrap_or(a);
        let rb = b.strip_prefix(&prefix).unwrap_or(b);
        ra.as_bytes().cmp(rb.as_bytes())
    });
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for path in paths {
        let rel = path
            .strip_prefix(&prefix)
            .ok_or_else(|| format!("unexpected path outside {subdir}/: {path}"))?
            .to_string();
        let content = git(&["show", &format!("{commit}:{path}")])?;
        entries.push((rel, content));
    }
    Ok(hash_entries(
        entries.iter().map(|(r, c)| (r.as_str(), c.as_slice())),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// SHA-256 of empty input — the normative hash of an empty directory.
    const EMPTY: &str = "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    #[test]
    fn empty_directory_hashes_to_sha256_of_empty_input() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(contract_hash(dir.path()).unwrap(), EMPTY);
    }

    #[test]
    fn hash_depends_on_content_not_creation_order() {
        let a = tempfile::tempdir().unwrap();
        fs::create_dir(a.path().join("sub")).unwrap();
        fs::write(a.path().join("z.wit"), "world z").unwrap();
        fs::write(a.path().join("sub/a.wit"), "world a").unwrap();

        let b = tempfile::tempdir().unwrap();
        fs::create_dir(b.path().join("sub")).unwrap();
        fs::write(b.path().join("sub/a.wit"), "world a").unwrap();
        fs::write(b.path().join("z.wit"), "world z").unwrap();

        assert_eq!(
            contract_hash(a.path()).unwrap(),
            contract_hash(b.path()).unwrap()
        );
        assert_ne!(contract_hash(a.path()).unwrap(), EMPTY);
    }

    #[test]
    fn one_byte_of_content_changes_the_hash() {
        let a = tempfile::tempdir().unwrap();
        fs::write(a.path().join("p.wit"), "interface x").unwrap();
        let before = contract_hash(a.path()).unwrap();
        fs::write(a.path().join("p.wit"), "interface y").unwrap();
        assert_ne!(contract_hash(a.path()).unwrap(), before);
    }

    #[test]
    fn renaming_a_file_changes_the_hash() {
        let a = tempfile::tempdir().unwrap();
        fs::write(a.path().join("p.wit"), "interface x").unwrap();
        let before = contract_hash(a.path()).unwrap();
        fs::rename(a.path().join("p.wit"), a.path().join("q.wit")).unwrap();
        assert_ne!(contract_hash(a.path()).unwrap(), before);
    }

    #[test]
    fn parse_pin_reads_all_four_fields() {
        let text = "\
# Kernel Pin\n\nprose here\n\n```\nrepo: https://example.invalid/jinnd\ncommit: 0123abc\nwit-hash: sha256:aa\ncontracts-hash: sha256:bb\n```\n";
        let pin = parse_pin(text).unwrap();
        assert_eq!(
            pin,
            KernelPin {
                repo: "https://example.invalid/jinnd".into(),
                commit: "0123abc".into(),
                wit_hash: "sha256:aa".into(),
                contracts_hash: "sha256:bb".into(),
            }
        );
    }

    #[test]
    fn parse_pin_rejects_a_missing_field() {
        let text = "repo: r\ncommit: c\nwit-hash: sha256:aa\n";
        let err = parse_pin(text).unwrap_err();
        assert!(err.contains("contracts-hash"), "error was: {err}");
    }

    #[test]
    fn git_tree_hash_matches_disk_hash_for_identical_content() {
        // Build a tiny git repo, commit a wit/ tree, and require the
        // at-commit hash to equal the on-disk hash of the same content.
        let repo = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .arg("-C")
                .arg(repo.path())
                .args(args)
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@example.invalid")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@example.invalid")
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}: {out:?}");
            String::from_utf8(out.stdout).unwrap()
        };
        run(&["init", "-q"]);
        fs::create_dir_all(repo.path().join("wit/deep")).unwrap();
        fs::write(repo.path().join("wit/p.wit"), "interface x").unwrap();
        fs::write(repo.path().join("wit/deep/q.wit"), "interface y").unwrap();
        run(&["add", "."]);
        run(&["commit", "-q", "-m", "c"]);
        let commit = run(&["rev-parse", "HEAD"]).trim().to_string();

        let disk = contract_hash(&repo.path().join("wit")).unwrap();
        let tree = contract_hash_of_git_tree(repo.path(), &commit, "wit").unwrap();
        assert_eq!(tree, disk);

        // And the working tree is never consulted: dirty the file, hash of
        // the commit stays put.
        fs::write(repo.path().join("wit/p.wit"), "interface CHANGED").unwrap();
        assert_eq!(
            contract_hash_of_git_tree(repo.path(), &commit, "wit").unwrap(),
            tree
        );
    }
}
