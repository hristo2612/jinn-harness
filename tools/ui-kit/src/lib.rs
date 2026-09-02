//! The UI seam's profile entries and the bundle-building half of the kit
//! (one home per fact); `main.rs` runs the web build and writes the profile.

use std::path::{Path, PathBuf};
use std::process::Command;

use jinn_ui::{encode_bundle, manifest_for, Manifest, BUNDLE_CONTRACT};

/// The bundle entry's id — the ONE entry a UI swap edits.
pub const BUNDLE_ID: &str = "jinn-ui-bundle";
/// The embedded provider's package (its artifact basename).
pub const BUNDLE_PACKAGE: &str = "ui/jinn-ui-bundle-embedded";
/// Where under a kit root the archive and manifest are written; the
/// provider compiles them in from `$JINN_UI_BUNDLE_DIR`.
pub const BUNDLE_DIR: &str = "ui-bundle";
/// The environment variable the provider's `include_bytes!` reads.
pub const BUNDLE_DIR_VAR: &str = "JINN_UI_BUNDLE_DIR";
/// The marker a variant bundle's document carries (proof 4 reads it).
pub const MARKER_META: &str = "jinn-ui-marker";

/// The repo root (this crate lives at `tools/ui-kit`).
#[must_use]
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The bundle entry: granted ONLY the contract it provides; its config is
/// empty; its identity is its hash (the UI-1 card, §4.2).
#[must_use]
pub fn bundle_entry(package: &str, hash: &str) -> serde_json::Value {
    serde_json::json!({ "id": BUNDLE_ID, "package": package, "hash": hash,
                        "config": { "grants": [BUNDLE_CONTRACT], "data": {} } })
}

/// Tells the transport's entry a bundle is mounted: the `jinn:ui-bundle`
/// grant and `jinn:introspect` (its transitions publish is what says the
/// bundle entry is Active — the authority the kernel enforces), and the
/// entry's id (that fact told to the provider).
pub fn mount_bundle_on(transport: &mut serde_json::Value) {
    let grants = transport["config"]["grants"]
        .as_array_mut()
        .expect("grants");
    grants.push(serde_json::json!(BUNDLE_CONTRACT));
    grants.push(serde_json::json!(jinn_api::INTROSPECT_CONTRACT));
    transport["config"]["data"]["ui-bundle-entry"] = serde_json::json!(BUNDLE_ID);
}

/// Every regular file under `dir`, as `(relative /-path, bytes)`, sorted.
///
/// # Panics
///
/// If the directory cannot be walked.
#[must_use]
pub fn archive(dir: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(root: &Path, dir: &Path, into: &mut Vec<(String, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).expect("bundle dir readable") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                walk(root, &path, into);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .expect("under the root")
                    .components()
                    .map(|component| component.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                into.push((relative, std::fs::read(&path).expect("file readable")));
            }
        }
    }
    let mut files = Vec::new();
    walk(dir, dir, &mut files);
    files.sort();
    files
}

/// Writes `bundle.bin` and `manifest.json` for `files` under `out` — the
/// ONLY writer of a manifest: its hashes are computed, never typed.
///
/// # Panics
///
/// If the files cannot be written.
pub fn write_bundle(out: &Path, files: &[(String, Vec<u8>)]) -> Manifest {
    std::fs::create_dir_all(out).expect("bundle out dir");
    let blob = encode_bundle(files);
    let manifest = manifest_for(files, &blob);
    std::fs::write(out.join("bundle.bin"), &blob).expect("bundle write");
    std::fs::write(
        out.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).expect("manifest encodes"),
    )
    .expect("manifest write");
    println!(
        "bundle {} files, {} bytes, sha256 {} -> {}",
        files.len(),
        blob.len(),
        manifest.bundle_sha256,
        out.display()
    );
    manifest
}

/// Runs the web client's pinned build (`pnpm install --frozen-lockfile`,
/// then `pnpm build` under `web/`; `pnpm` on `PATH`) and answers `web/out`.
///
/// # Panics
///
/// If either step fails — the kit never archives a stale build.
#[must_use]
pub fn build_web() -> PathBuf {
    let web = repo_root().join("web");
    for args in [vec!["install", "--frozen-lockfile"], vec!["build"]] {
        let status = Command::new("pnpm")
            .args(&args)
            .current_dir(&web)
            .status()
            .unwrap_or_else(|error| panic!("pnpm {}: {error} (is pnpm on PATH?)", args.join(" ")));
        assert!(status.success(), "pnpm {} failed", args.join(" "));
    }
    web.join("out")
}

/// A copy of `files` whose document carries `<meta name="jinn-ui-marker"
/// content="{marker}">` — the second bundle proof 4 swaps to.
///
/// # Panics
///
/// If the document has no `</head>`.
#[must_use]
pub fn marked(files: &[(String, Vec<u8>)], marker: &str) -> Vec<(String, Vec<u8>)> {
    files
        .iter()
        .map(|(path, bytes)| {
            if path == jinn_ui::DOCUMENT {
                let document = String::from_utf8_lossy(bytes);
                let tag = format!("<meta name=\"{MARKER_META}\" content=\"{marker}\">\n  </head>");
                assert!(document.contains("</head>"), "the document has a head");
                (
                    path.clone(),
                    document.replacen("</head>", &tag, 1).into_bytes(),
                )
            } else {
                (path.clone(), bytes.clone())
            }
        })
        .collect()
}
