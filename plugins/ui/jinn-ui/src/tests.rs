//! The definition's own proofs: the codec round-trips, the manifest is
//! the archive's truth, verification FAILS CLOSED on every mismatch, and
//! the serving law answers each path class exactly as the card states
//! (docs/plans/ui-malleability-arc.md §4.2, inventory §2.15/§2.16/§2.24).

use super::*;

fn sample() -> Vec<(String, Vec<u8>)> {
    vec![
        ("index.html".into(), b"<html>doc</html>".to_vec()),
        ("assets/index-abc123.js".into(), b"console.log(1)".to_vec()),
        ("assets/index-abc123.css".into(), b"body{}".to_vec()),
        ("assets/font-xyz.woff2".into(), vec![0, 1, 2, 3]),
        ("manifest.webmanifest".into(), b"{}".to_vec()),
        ("icons/icon-192.png".into(), vec![137, 80, 78, 71]),
    ]
}

#[test]
fn the_bundle_codec_round_trips_and_is_length_prefixed() {
    let files = sample();
    let bytes = encode_bundle(&files);
    // u32-LE count first.
    assert_eq!(&bytes[..4], &(files.len() as u32).to_le_bytes());
    let decoded = decode_bundle(&bytes).expect("decodes");
    assert_eq!(decoded, files);
    assert_eq!(
        decode_bundle(&bytes[..bytes.len() - 1]),
        Err("bundle: truncated".into())
    );
    assert!(decode_bundle(&[]).is_err());
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert_eq!(
        decode_bundle(&trailing),
        Err("bundle: trailing bytes".into())
    );
}

#[test]
fn the_manifest_names_every_file_with_its_hash_mime_and_cache_class() {
    let files = sample();
    let bundle = encode_bundle(&files);
    let manifest = manifest_for(&files, &bundle);
    assert_eq!(manifest.api_version, API_VERSION);
    assert_eq!(manifest.document, DOCUMENT);
    assert_eq!(manifest.files.len(), files.len());
    let by_path = |path: &str| {
        manifest
            .files
            .iter()
            .find(|file| file.path == path)
            .unwrap_or_else(|| panic!("{path} in the manifest"))
    };
    assert_eq!(by_path("index.html").mime, "text/html; charset=utf-8");
    assert!(
        !by_path("index.html").immutable,
        "the document is never immutable"
    );
    assert_eq!(
        by_path("assets/index-abc123.js").mime,
        "application/javascript"
    );
    assert!(by_path("assets/index-abc123.js").immutable);
    assert_eq!(by_path("assets/index-abc123.css").mime, "text/css");
    assert_eq!(by_path("assets/font-xyz.woff2").mime, "font/woff2");
    assert_eq!(
        by_path("manifest.webmanifest").mime,
        "application/manifest+json"
    );
    assert!(!by_path("manifest.webmanifest").immutable);
    assert_eq!(by_path("icons/icon-192.png").mime, "image/png");
    assert_eq!(
        by_path("index.html").sha256,
        hex_sha256(b"<html>doc</html>")
    );
    assert_eq!(manifest.bundle_sha256, hex_sha256(&bundle));
    // Additive: an unknown sibling rides through a decode → encode.
    let mut json = serde_json::to_value(&manifest).expect("encodes");
    json["files"][0]["later"] = serde_json::json!(true);
    json["later-too"] = serde_json::json!(1);
    let back: Manifest = serde_json::from_value(json.clone()).expect("decodes");
    assert_eq!(serde_json::to_value(&back).expect("encodes"), json);
}

#[test]
fn verification_fails_closed_on_every_mismatch() {
    let files = sample();
    let bundle = encode_bundle(&files);
    let manifest = manifest_for(&files, &bundle);
    let verified = verify(&manifest, &bundle).expect("the honest bundle verifies");
    assert_eq!(verified.len(), files.len());
    assert_eq!(verified["index.html"], b"<html>doc</html>");

    // One byte of one file flipped inside the archive: the manifest's hash
    // for THAT file no longer matches, and the bundle hash does not either.
    let mut corrupt = bundle.clone();
    let position = bundle.len() - 2;
    corrupt[position] ^= 0xff;
    let error = verify(&manifest, &corrupt).expect_err("a flipped byte fails");
    assert!(error.contains("bundle-sha256"), "{error}");

    // The archive is honest but the manifest lies about a file.
    let mut lying = manifest.clone();
    lying.files[1].sha256 = hex_sha256(b"something else");
    lying.bundle_sha256 = hex_sha256(&bundle);
    let error = verify(&lying, &bundle).expect_err("a wrong file hash fails");
    assert!(error.contains("assets/index-abc123.js"), "{error}");

    // A file in the archive the manifest does not name, or vice versa.
    let mut extra = files.clone();
    extra.push(("sneaked.js".into(), b"x".to_vec()));
    let extra_bundle = encode_bundle(&extra);
    let mut manifest_extra = manifest.clone();
    manifest_extra.bundle_sha256 = hex_sha256(&extra_bundle);
    let error = verify(&manifest_extra, &extra_bundle).expect_err("an unnamed file fails");
    assert!(error.contains("sneaked.js"), "{error}");
    let mut missing = manifest.clone();
    missing.files.push(ManifestFile {
        path: "ghost.js".into(),
        sha256: hex_sha256(b""),
        mime: "application/javascript".into(),
        immutable: true,
        extra: Extensions::new(),
    });
    let error = verify(&missing, &bundle).expect_err("a named file absent fails");
    assert!(error.contains("ghost.js"), "{error}");

    // No document: nothing to serve, refused.
    let no_document: Vec<(String, Vec<u8>)> = files[1..].to_vec();
    let no_document_bundle = encode_bundle(&no_document);
    let error = verify(
        &manifest_for(&no_document, &no_document_bundle),
        &no_document_bundle,
    )
    .expect_err("a bundle without its document fails");
    assert!(error.contains(DOCUMENT), "{error}");
}

fn served() -> (Manifest, Files) {
    let files = sample();
    let bundle = encode_bundle(&files);
    let manifest = manifest_for(&files, &bundle);
    let files = verify(&manifest, &bundle).expect("verifies");
    (manifest, files)
}

#[test]
fn the_serving_law_answers_each_path_class_as_the_card_states() {
    let (manifest, files) = served();
    let at = |path: &str| serve(&manifest, &files, path);

    // The document: 200 text/html, no-cache — at `/` and on the SPA fallback.
    for path in [
        "/",
        "/settings",
        "/settings/plugins",
        "/anything/deep",
        "/index.html",
    ] {
        let Static::File {
            mime, cache, body, ..
        } = at(path)
        else {
            panic!("{path}: the document")
        };
        assert_eq!(mime, "text/html; charset=utf-8", "{path}");
        assert_eq!(cache, CACHE_NO_STORE_REVALIDATE, "{path}");
        assert_eq!(body, b"<html>doc</html>", "{path}");
    }
    // A hashed asset: its bytes, its MIME, immutable.
    let Static::File {
        mime, cache, body, ..
    } = at("/assets/index-abc123.js")
    else {
        panic!("an asset")
    };
    assert_eq!(mime, "application/javascript");
    assert_eq!(cache, CACHE_IMMUTABLE);
    assert_eq!(body, b"console.log(1)");
    // An unknown asset is 404 text/plain — NEVER the document.
    assert!(matches!(at("/assets/missing.js"), Static::NotFound));
    assert!(matches!(at("/assets/"), Static::NotFound));
    // The PWA manifest keeps its MIME and is not immutable.
    let Static::File { mime, cache, .. } = at("/manifest.webmanifest") else {
        panic!("the manifest")
    };
    assert_eq!(mime, "application/manifest+json");
    assert_eq!(cache, CACHE_NO_STORE_REVALIDATE);
    // A non-asset file the bundle carries is served as itself.
    assert!(matches!(
        at("/icons/icon-192.png"),
        Static::File {
            mime: "image/png",
            ..
        }
    ));
    // The API namespace in another spelling is NOTHING: neither a page nor
    // a route (404, no dispatch) — and so is any path with a dot-dot segment
    // or an empty segment.
    for path in [
        "/V1/status",
        "/v1",
        "/V1",
        "/../etc",
        "/assets/../index.html",
        "//",
        "/a//b",
    ] {
        assert!(matches!(at(path), Static::NotFound), "{path}");
    }
}

#[test]
fn the_api_namespace_is_exactly_v1_case_sensitive() {
    assert!(is_api_path("/v1"));
    assert!(is_api_path("/v1/status"));
    assert!(!is_api_path("/V1/status"));
    assert!(!is_api_path("/v10/x"));
    assert!(!is_api_path("/"));
    assert!(!is_api_path("/assets/v1.js"));
}

// --- the moment vocabulary (UI-2, §9.2) ---

#[test]
fn the_path_law_names_exactly_the_three_topics_and_nothing_else() {
    assert_eq!(
        moment_topic("/v1/moments/ui/before-send"),
        Some(TOPIC_BEFORE_SEND)
    );
    assert_eq!(
        moment_topic("/v1/moments/ui/before-create-session"),
        Some(TOPIC_BEFORE_CREATE_SESSION)
    );
    assert_eq!(
        moment_topic("/v1/moments/ui/before-patch-settings"),
        Some(TOPIC_BEFORE_PATCH_SETTINGS)
    );
    for miss in [
        "/v1/moments",
        "/v1/moments/",
        "/v1/moments/ui",
        "/v1/moments/ui/",
        "/v1/moments/ui/after-nothing",
        "/v1/moments/introspect/transitions",
        "/v1/moments/ui/../before-send",
        "/v1/moments/UI/before-send",
        "/v1/moments/ui/Before-Send",
        "/v1/moments/ui/before-send/",
        "/v1/moments/ui/before-send/x",
        "/v1/momentsui/before-send",
    ] {
        assert_eq!(moment_topic(miss), None, "{miss} is not a moment");
    }
    for under in ["/v1/moments", "/v1/moments/", "/v1/moments/ui/before-send"] {
        assert!(is_moments_path(under), "{under}");
    }
    assert!(!is_moments_path("/v1/momentsx"));
    assert!(!is_moments_path("/v1/settings"));
}

#[test]
fn every_topic_binds_its_payload_schema_before_the_walk() {
    let send = br#"{ "text": "hello", "session-id": "s-1", "attachments": [] }"#;
    assert_eq!(validate_moment(TOPIC_BEFORE_SEND, send), Ok(()));
    assert_eq!(
        validate_moment(
            TOPIC_BEFORE_SEND,
            br#"{ "text": "hello", "attachments": [] }"#
        )
        .map_err(|e| e.contains("session-id")),
        Err(true),
        "session-id is required"
    );
    assert!(validate_moment(TOPIC_BEFORE_SEND, b"[1,2]").is_err());
    assert!(validate_moment(TOPIC_BEFORE_SEND, b"not json").is_err());
    let session = br#"{ "engine": { "engine": "echo" } }"#;
    assert_eq!(
        validate_moment(TOPIC_BEFORE_CREATE_SESSION, session),
        Ok(())
    );
    assert!(validate_moment(TOPIC_BEFORE_CREATE_SESSION, b"{}").is_err());
    let patch = br#"{ "namespace": "cron", "patch": { "tick-ms": 700 } }"#;
    assert_eq!(validate_moment(TOPIC_BEFORE_PATCH_SETTINGS, patch), Ok(()));
    assert!(
        validate_moment(
            TOPIC_BEFORE_PATCH_SETTINGS,
            br#"{ "namespace": "cron", "patch": 5 }"#
        )
        .is_err(),
        "the patch is an object"
    );
    assert!(validate_moment("jinn:ui/after-nothing", b"{}").is_err());
    let detail = refused_detail("restarting", TOPIC_BEFORE_SEND, "entry ext-green");
    assert!(detail.starts_with("restarting: "), "{detail}");
    assert_eq!(WALK_REFUSALS.len(), 5);
}
