//! The `jinn:ui-bundle` service definition: the contract's names, the
//! manifest and bundle wire shapes, the bundle codec, verification, and
//! the SERVING LAW as pure functions. The prose law lives in this crate's
//! README; this code is its schema. Compiled natively (unit tests, the
//! kit) and into the guests, so it depends on nothing but serde and sha2.
//!
//! The one decision it encodes (the UI-1 card, §4.1): the UI bundle is ONE
//! plugin artifact, content-addressed by the kernel's own `package` +
//! `hash`; a transport injects this contract, reads the whole bundle ONCE
//! at activation, verifies every file against the manifest FAIL CLOSED,
//! and serves the document and its assets from memory to any loopback
//! peer with no door and no crossing — a byte is never a dispatch.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[cfg(test)]
mod tests;

/// The schema version every manifest carries.
pub const API_VERSION: &str = "0.1.0";
/// The contract a bundle provider provides.
pub const BUNDLE_CONTRACT: &str = "jinn:ui-bundle";
/// Operation: the manifest (paths, sha256, mime, cache class), JSON.
pub const OP_MANIFEST: &str = "manifest";
/// Operation: the whole archive as one blob ([`encode_bundle`]'s shape).
pub const OP_BUNDLE: &str = "bundle";
/// The document every non-asset, non-API path answers (inventory §2.15).
pub const DOCUMENT: &str = "index.html";
/// Files under this prefix are hashed by the build and served immutable.
pub const ASSETS_PREFIX: &str = "assets/";
/// `Cache-Control` for a hashed asset (inventory §2.16).
pub const CACHE_IMMUTABLE: &str = "public, max-age=31536000, immutable";
/// `Cache-Control` for everything else, the document above all: iOS
/// Safari over a tunnel hostname caches HTML indefinitely (inventory
/// §2.16, `90e37113`).
pub const CACHE_NO_STORE_REVALIDATE: &str = "no-cache";
/// The MIME of a typed refusal on a static path (inventory §2.15: a
/// missing asset is `404 text/plain`, never the SPA fallback).
pub const MIME_TEXT: &str = "text/plain; charset=utf-8";

/// Unknown sibling fields, preserved across a decode → encode round trip
/// (the distribution's additivity law, `plugins/settings/jinn-settings/src/wire.rs`).
pub type Extensions = serde_json::Map<String, serde_json::Value>;

/// One file of the bundle as the manifest names it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ManifestFile {
    /// `/`-separated, relative to the bundle root, no leading slash.
    pub path: String,
    /// Lowercase hex SHA-256 of the file's bytes.
    pub sha256: String,
    /// The `Content-Type` it is served with ([`mime_of`]).
    pub mime: String,
    /// Served [`CACHE_IMMUTABLE`] when true, [`CACHE_NO_STORE_REVALIDATE`] otherwise.
    pub immutable: bool,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The `manifest` answer: what the archive holds and what each file is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Manifest {
    pub api_version: String,
    pub files: Vec<ManifestFile>,
    /// The path answered on `/` and on the SPA fallback.
    pub document: String,
    /// Lowercase hex SHA-256 of the whole `bundle` blob.
    pub bundle_sha256: String,
    #[serde(flatten)]
    pub extra: Extensions,
}

/// The verified files, by path.
pub type Files = BTreeMap<String, Vec<u8>>;

/// Lowercase hex SHA-256.
#[must_use]
pub fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// The `Content-Type` a path is served with, by extension. The table is
/// the old gateway's (inventory §2.16): `.webmanifest` is
/// `application/manifest+json` or the install prompt never appears.
#[must_use]
pub fn mime_of(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, extension)| extension) {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "application/javascript",
        Some("css") => "text/css",
        Some("json") => "application/json",
        Some("webmanifest") => "application/manifest+json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        Some("txt") => MIME_TEXT,
        _ => "application/octet-stream",
    }
}

/// The `bundle` blob: u32-LE count, then per file u32-LE path length,
/// the path, u32-LE byte length, the bytes.
#[must_use]
pub fn encode_bundle(files: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut blob = Vec::new();
    blob.extend_from_slice(&(files.len() as u32).to_le_bytes());
    for (path, bytes) in files {
        blob.extend_from_slice(&(path.len() as u32).to_le_bytes());
        blob.extend_from_slice(path.as_bytes());
        blob.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        blob.extend_from_slice(bytes);
    }
    blob
}

/// The inverse of [`encode_bundle`], refused typed on any malformed shape.
///
/// # Errors
///
/// A truncated blob, trailing bytes, or a path that is not UTF-8.
pub fn decode_bundle(blob: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut at = 0usize;
    let mut take = |length: usize| -> Result<&[u8], String> {
        let end = at.checked_add(length).ok_or("bundle: truncated")?;
        let slice = blob.get(at..end).ok_or("bundle: truncated")?;
        at = end;
        Ok(slice)
    };
    let u32_at = |slice: &[u8]| u32::from_le_bytes(slice.try_into().expect("four bytes")) as usize;
    let count = u32_at(take(4)?);
    let mut files = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let length = u32_at(take(4)?);
        let path = std::str::from_utf8(take(length)?)
            .map_err(|_| "bundle: a path is not UTF-8".to_owned())?
            .to_owned();
        let length = u32_at(take(4)?);
        files.push((path, take(length)?.to_vec()));
    }
    if at != blob.len() {
        return Err("bundle: trailing bytes".into());
    }
    Ok(files)
}

/// The manifest of an archive: every file with its hash, MIME and cache
/// class, and the blob's own hash. The kit's half of the truth.
#[must_use]
pub fn manifest_for(files: &[(String, Vec<u8>)], bundle: &[u8]) -> Manifest {
    Manifest {
        api_version: API_VERSION.to_owned(),
        files: files
            .iter()
            .map(|(path, bytes)| ManifestFile {
                path: path.clone(),
                sha256: hex_sha256(bytes),
                mime: mime_of(path).to_owned(),
                immutable: path.starts_with(ASSETS_PREFIX),
                extra: Extensions::new(),
            })
            .collect(),
        document: DOCUMENT.to_owned(),
        bundle_sha256: hex_sha256(bundle),
        extra: Extensions::new(),
    }
}

/// Verifies a `bundle` blob against its manifest, FAIL CLOSED: the blob
/// hashes to `bundle-sha256`, every named file is present and hashes to
/// its `sha256`, no unnamed file is present, and the document is among
/// them. Answers the files by path. A transport that cannot verify does
/// not serve (R11: the transport's activation fails, nothing else).
///
/// # Errors
///
/// The first mismatch, naming the file.
pub fn verify(manifest: &Manifest, bundle: &[u8]) -> Result<Files, String> {
    if hex_sha256(bundle) != manifest.bundle_sha256 {
        return Err("bundle-sha256 does not match the blob".into());
    }
    let decoded = decode_bundle(bundle)?;
    let mut files = Files::new();
    for (path, bytes) in decoded {
        let named = manifest
            .files
            .iter()
            .find(|file| file.path == path)
            .ok_or_else(|| format!("{path}: in the archive, not in the manifest"))?;
        if hex_sha256(&bytes) != named.sha256 {
            return Err(format!("{path}: sha256 does not match the manifest"));
        }
        files.insert(path, bytes);
    }
    if let Some(absent) = manifest
        .files
        .iter()
        .find(|file| !files.contains_key(&file.path))
    {
        return Err(format!(
            "{}: in the manifest, not in the archive",
            absent.path
        ));
    }
    if !files.contains_key(&manifest.document) {
        return Err(format!(
            "{}: the document is not in the archive",
            manifest.document
        ));
    }
    Ok(files)
}

/// Whether a request path belongs to the operator API — exactly `/v1`
/// or `/v1/…`, case-sensitive. Everything else is the serving law's.
#[must_use]
pub fn is_api_path(path: &str) -> bool {
    path == "/v1" || path.starts_with("/v1/")
}

/// What the serving law answers for one path.
#[derive(Debug, PartialEq, Eq)]
pub enum Static<'a> {
    /// 200 with these bytes, this `Content-Type`, this `Cache-Control`.
    File {
        path: &'a str,
        mime: &'a str,
        cache: &'static str,
        body: &'a [u8],
    },
    /// 404 `text/plain`: an unknown asset, a malformed path, or the API
    /// namespace spelled in another case — never the document.
    NotFound,
}

/// THE SERVING LAW (inventory §2.15, §2.16, §2.24), for a GET on a
/// non-API path: a path with a dot-dot or empty segment, or whose first
/// segment spells the API namespace in another case, is nothing (404);
/// `/assets/<x>` is that file immutable or 404, never the document; any
/// other path the bundle holds is that file; everything else is the
/// document, `no-cache`.
#[must_use]
pub fn serve<'a>(manifest: &'a Manifest, files: &'a Files, path: &str) -> Static<'a> {
    let Some(relative) = path.strip_prefix('/') else {
        return Static::NotFound;
    };
    let malformed = relative
        .split('/')
        .any(|segment| segment == ".." || (segment.is_empty() && !relative.is_empty()));
    let api_lookalike = relative
        .split('/')
        .next()
        .is_some_and(|first| first.eq_ignore_ascii_case("v1"));
    if malformed || api_lookalike {
        return Static::NotFound;
    }
    let file = |path: &str| {
        let (named, bytes) = manifest
            .files
            .iter()
            .find(|file| file.path == path)
            .zip(files.get(path))?;
        Some(Static::File {
            path: &named.path,
            mime: &named.mime,
            cache: if named.immutable {
                CACHE_IMMUTABLE
            } else {
                CACHE_NO_STORE_REVALIDATE
            },
            body: bytes,
        })
    };
    if relative.starts_with(ASSETS_PREFIX) {
        return file(relative).unwrap_or(Static::NotFound);
    }
    file(relative)
        .or_else(|| file(&manifest.document))
        .unwrap_or(Static::NotFound)
}
