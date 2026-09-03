//! THE BUNDLE (harness packet UI-1, PLA-349): the web UI is one plugin
//! artifact served same-origin by the transport from memory it filled at
//! activation. Every proof boots the `ui` profile through the REAL pinned
//! daemon (AGENTS.md standing order 3) and drives it over loopback as a
//! browser would — a GET with no header for a byte, a bearer for `/v1`.
//! Evidence is the wire (status, headers, bytes) and the ledger (parsed
//! row by row). The proofs are the card's §4.3 items 1–5 as restated at
//! pin `a53a352` (M2-K24: the transport DECLARES the bundle it injects,
//! so the kernel gates its activation and restarts it on a swap —
//! FINDINGS.md #7/#45/#46); item 6 is the repo gate in
//! `tools/ui-kit/tests/verbatim.rs`, item 7 the verifier's.
//!
//! Self-skips LOUDLY when no jinnd checkout holding the pinned commit is
//! reachable (KERNEL-PIN.md Gate 2).

use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use composition::api::{fetch_bytes, get, listening, request_as};
use composition::daemon::{jinnd_source, pinned_commit, pinned_daemon};
use composition::kit::{
    artifact_hash, entry_mut, fresh_api_root, fresh_ui_root, suite_credential, Daemon, LedgerRow,
    UI_CORRUPT, UI_MARKED, UI_MARKER,
};
use jinn_api::{AUTH_CONTRACT, OP_VERIFY};
use jinn_ui::{
    hex_sha256, Manifest, BUNDLE_CONTRACT, CACHE_IMMUTABLE, CACHE_NO_STORE_REVALIDATE, OP_BUNDLE,
    OP_MANIFEST,
};
use ui_kit::{BUNDLE_DIR, BUNDLE_ID, MARKER_META};

const TRANSPORT: &str = "jinn-api-http";
const SETTINGS: &str = "jinn-settings-profile";
const PLUGINS: &str = "jinn-plugins-live";

fn gate() -> Option<&'static PathBuf> {
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

/// Boots a fresh `ui` root and waits for the API to answer.
fn booted(name: &str) -> Option<(Daemon, u16)> {
    let binary = gate()?;
    let (root, port) = fresh_ui_root(name);
    let daemon = Daemon::boot_operator(binary, &root);
    daemon.await_ready();
    let health = get(port, "/v1/health");
    assert_eq!(health.status, 200, "{}", health.raw);
    daemon.eventually("the boot request's close to land", || {
        daemon.ledger_count("NetClosed") >= 1
    });
    Some((daemon, port))
}

/// The kit's manifest of the bundle this root serves.
fn manifest(daemon: &Daemon) -> Manifest {
    let path = daemon.root.join(BUNDLE_DIR).join("manifest.json");
    serde_json::from_slice(&std::fs::read(&path).expect("the kit's manifest")).expect("parses")
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(found, _)| found == name)
        .map(|(_, value)| value.as_str())
}

/// Every `/assets/…` path the document references.
fn asset_refs(document: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut rest = document;
    while let Some(at) = rest.find("/assets/") {
        let tail = &rest[at..];
        let end = tail
            .find(|c: char| c == '"' || c == '\'' || c == ')' || c.is_whitespace())
            .unwrap_or(tail.len());
        refs.push(tail[..end].to_owned());
        rest = &tail[end..];
    }
    refs.sort();
    refs.dedup();
    refs
}

fn kind_of(row: &LedgerRow) -> (String, serde_json::Value) {
    match serde_json::from_str::<serde_json::Value>(&row.kind) {
        Ok(serde_json::Value::Object(object)) if object.len() == 1 => {
            let (name, fields) = object.into_iter().next().expect("one key");
            (name, fields)
        }
        Ok(serde_json::Value::String(unit)) => (unit, serde_json::Value::Null),
        _ => (row.kind.clone(), serde_json::Value::Null),
    }
}

fn is_call(row: &LedgerRow, contract: &str, operation: &str) -> bool {
    let (name, fields) = kind_of(row);
    name == "ContractCall" && fields["contract"] == contract && fields["operation"] == operation
}

/// The connection's own rows: accept, wakes, close, `jinn:net` calls.
fn is_transport(row: &LedgerRow) -> bool {
    let (name, fields) = kind_of(row);
    match name.as_str() {
        "NetAccepted" | "NetReadable" | "NetClosed" => true,
        "ContractCall" => fields["contract"] == "jinn:net",
        _ => false,
    }
}

/// The transport's rows after `seq`, one segment per accepted connection
/// (2.8's discipline, `tests/composition/tests/auth.rs`).
fn segments(daemon: &Daemon, seq: u64) -> Vec<Vec<LedgerRow>> {
    let mut segments: Vec<Vec<LedgerRow>> = Vec::new();
    let mut open: Option<u64> = None;
    for row in daemon
        .ledger_rows()
        .into_iter()
        .filter(|row| row.seq > seq && row.entry.as_deref() == Some(TRANSPORT))
    {
        let (name, fields) = kind_of(&row);
        if name == "NetAccepted" {
            open = fields["handle"].as_u64();
            segments.push(vec![row]);
            continue;
        }
        let Some(handle) = open else { continue };
        let closes = name == "NetClosed" && fields["handle"].as_u64() == Some(handle);
        segments.last_mut().expect("an open segment").push(row);
        if closes {
            open = None;
        }
    }
    segments
}

fn last_seq(daemon: &Daemon) -> u64 {
    daemon.ledger_rows().last().map_or(0, |row| row.seq)
}

/// An entry's fiber `state` and `incarnation` as the plugins seam reads
/// them off the kernel.
fn lifecycle(port: u16, id: &str) -> (String, u64) {
    let read = get(port, &format!("/v1/plugins/main/{id}"));
    assert_eq!(read.status, 200, "{}", read.raw);
    let lifecycle = &read.body["lifecycle"];
    (
        lifecycle["state"]
            .as_str()
            .unwrap_or_else(|| panic!("{id}: a state: {}", read.raw))
            .to_owned(),
        read.body["incarnation"]
            .as_u64()
            .unwrap_or_else(|| panic!("{id}: an incarnation: {}", read.raw)),
    )
}

/// One `GET /` that never panics on a refused or torn connection: `None`
/// when nothing answered whole (the blip of a restarting transport), else
/// the status and body. Proof 4 measures the blip with it.
fn probe_document(port: u16) -> Option<(u16, Vec<u8>)> {
    use std::io::{Read as _, Write as _};
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut stream =
        std::net::TcpStream::connect_timeout(&address, Duration::from_millis(300)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
        .ok()?;
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).ok()?;
    let split = raw.windows(4).position(|window| window == b"\r\n\r\n")?;
    let status = std::str::from_utf8(&raw[..split])
        .ok()?
        .strip_prefix("HTTP/1.1 ")?
        .get(..3)?
        .parse()
        .ok()?;
    Some((status, raw[split + 4..].to_vec()))
}

#[test]
fn the_document_and_every_asset_are_served_from_the_pinned_bundle_by_hash() {
    let Some((daemon, port)) = booted("ui-served") else {
        return;
    };
    let manifest = manifest(&daemon);
    let named = |path: &str| {
        manifest
            .files
            .iter()
            .find(|file| file.path == path)
            .unwrap_or_else(|| panic!("{path} in the manifest"))
    };

    // The document: 200 text/html, no-cache, the manifest's bytes.
    let (status, headers, document) = fetch_bytes(port, "/", None);
    assert_eq!(status, 200);
    assert_eq!(
        header(&headers, "content-type"),
        Some("text/html; charset=utf-8")
    );
    assert_eq!(
        header(&headers, "cache-control"),
        Some(CACHE_NO_STORE_REVALIDATE)
    );
    assert_eq!(hex_sha256(&document), named("index.html").sha256);
    let document = String::from_utf8(document).expect("html");
    assert!(document.contains("<div id=\"root\">"), "the shell's root");

    // Every asset it references: 200, immutable, its own MIME, bytes that
    // hash to the manifest's entry.
    let refs = asset_refs(&document);
    assert!(!refs.is_empty(), "the document references its assets");
    for reference in &refs {
        let (status, headers, bytes) = fetch_bytes(port, reference, None);
        assert_eq!(status, 200, "{reference}");
        assert_eq!(
            header(&headers, "cache-control"),
            Some(CACHE_IMMUTABLE),
            "{reference}"
        );
        let file = named(&reference[1..]);
        assert_eq!(
            header(&headers, "content-type"),
            Some(file.mime.as_str()),
            "{reference}"
        );
        assert_eq!(
            hex_sha256(&bytes),
            file.sha256,
            "{reference}: the bytes are the pinned bytes"
        );
    }
    println!("proof 1: {} referenced assets served by hash", refs.len());

    // The PWA manifest keeps its MIME (inventory §2.16).
    let (status, headers, _) = fetch_bytes(port, "/manifest.webmanifest", None);
    assert_eq!(status, 200);
    assert_eq!(
        header(&headers, "content-type"),
        Some("application/manifest+json")
    );

    // An unknown asset is 404 text/plain — NEVER the SPA fallback.
    let (status, headers, body) = fetch_bytes(port, "/assets/missing.js", None);
    assert_eq!(status, 404);
    assert!(header(&headers, "content-type").is_some_and(|mime| mime.starts_with("text/plain")));
    assert!(
        !String::from_utf8_lossy(&body).contains("<html"),
        "not the document"
    );

    // A client route answers the document (the SPA fallback).
    let (status, _, fallback) = fetch_bytes(port, "/settings", None);
    assert_eq!(status, 200);
    assert_eq!(hex_sha256(&fallback), named("index.html").sha256);
    daemon.interrupt();
}

#[test]
fn a_byte_is_never_a_dispatch_and_a_v1_request_is_always_the_door() {
    let Some((daemon, port)) = booted("ui-door") else {
        return;
    };
    let manifest = manifest(&daemon);
    let asset = manifest
        .files
        .iter()
        .find(|file| file.immutable)
        .expect("an asset");
    let baseline = last_seq(&daemon);

    // Three bytes: the document with nothing, an asset with nothing, and
    // the document WITH the operator's bearer — the mandatory probe: on a
    // static path the credential is IGNORED, not consumed.
    let (status, ..) = fetch_bytes(port, "/", None);
    assert_eq!(status, 200);
    let (status, ..) = fetch_bytes(port, &format!("/{}", asset.path), None);
    assert_eq!(status, 200);
    let (status, ..) = fetch_bytes(port, "/settings", Some(suite_credential()));
    assert_eq!(status, 200);
    // Then the door, unchanged from 2.8: no bearer is 401, the right one 200.
    let refused = request_as(port, "GET", "/v1/status", None, None);
    assert_eq!(refused.status, 401, "{}", refused.raw);
    assert_eq!(refused.header("www-authenticate"), Some("Bearer"));
    let granted = request_as(port, "GET", "/v1/health", None, Some(suite_credential()));
    assert_eq!(granted.status, 200, "{}", granted.raw);

    daemon.eventually("the five connections to close on the ledger", || {
        segments(&daemon, baseline)
            .iter()
            .filter(|segment| kind_of(segment.last().expect("a row")).0 == "NetClosed")
            .count()
            >= 5
    });
    let segments = segments(&daemon, baseline);
    assert_eq!(segments.len(), 5, "one segment per connection");

    // The three static segments: transport rows and NOTHING else — no
    // crossing, no resolve, no decision, no effect.
    for (segment, what) in segments
        .iter()
        .zip(["document", "asset", "document with a bearer"])
    {
        let kinds: Vec<&str> = segment.iter().map(|row| row.kind.as_str()).collect();
        assert!(
            segment.iter().all(is_transport),
            "{what}: a byte is never a dispatch: {kinds:#?}"
        );
    }
    // The two /v1 segments: exactly one verify and one decision each,
    // the refusal with no dispatch after it.
    for (segment, granted) in segments[3..].iter().zip([false, true]) {
        let kinds: Vec<&str> = segment.iter().map(|row| row.kind.as_str()).collect();
        assert_eq!(
            segment
                .iter()
                .filter(|row| is_call(row, AUTH_CONTRACT, OP_VERIFY))
                .count(),
            1,
            "one verify: {kinds:#?}"
        );
        let decided: Vec<bool> = segment
            .iter()
            .filter_map(|row| {
                let (name, fields) = kind_of(row);
                (name == "AuthDecided").then(|| fields["granted"].as_bool().expect("granted"))
            })
            .collect();
        assert_eq!(decided, [granted], "one decision: {kinds:#?}");
    }
    // Across the whole window: exactly two decisions — the bearer on the
    // static path was never put to the kernel.
    let decisions = daemon
        .ledger_rows()
        .into_iter()
        .filter(|row| row.seq > baseline && kind_of(row).0 == "AuthDecided")
        .count();
    assert_eq!(decisions, 2, "the static bearer was ignored, not consumed");
    assert!(
        !daemon
            .ledger_kinds()
            .join("\n")
            .contains(suite_credential()),
        "no credential bytes on the ledger"
    );
    daemon.interrupt();
}

#[test]
fn the_bundle_crosses_once_per_transport_activation_and_its_size_is_recorded() {
    let Some((daemon, port)) = booted("ui-once") else {
        return;
    };
    // Serve a few pages so a per-request read would show.
    for _ in 0..3 {
        let (status, ..) = fetch_bytes(port, "/", None);
        assert_eq!(status, 200);
    }
    let rows = daemon.ledger_rows();
    let transport_rows: Vec<&LedgerRow> = rows
        .iter()
        .filter(|row| row.entry.as_deref() == Some(TRANSPORT))
        .collect();
    let bundle_calls = transport_rows
        .iter()
        .filter(|row| is_call(row, BUNDLE_CONTRACT, OP_BUNDLE))
        .count();
    let manifest_calls = transport_rows
        .iter()
        .filter(|row| is_call(row, BUNDLE_CONTRACT, OP_MANIFEST))
        .count();
    let activations = transport_rows
        .iter()
        .filter(|row| row.kind.contains(r#""to":"Active""#))
        .count();
    assert_eq!(activations, 1, "one activation of the transport");
    assert_eq!(
        bundle_calls, 1,
        "exactly one bundle crossing per activation"
    );
    // The manifest is asked EXACTLY once: the transport declares
    // `injects: ["jinn:ui-bundle"]` and the kernel activates it only once
    // that provider is Active (M2-K24; FINDINGS.md #45 fixed at pin
    // a53a352) — no probe, no subscription, no second read.
    assert_eq!(
        manifest_calls, 1,
        "exactly one manifest crossing per activation"
    );
    let bytes = std::fs::read(daemon.root.join(BUNDLE_DIR).join("bundle.bin"))
        .expect("the kit's bundle")
        .len();
    println!(
        "proof 3: bundle {bytes} bytes crossed once ({manifest_calls} manifest crossings); {} files; ledger {} rows in total, {} on the transport",
        manifest(&daemon).files.len(),
        rows.len(),
        transport_rows.len()
    );
    daemon.interrupt();
}

#[test]
fn swapping_the_ui_is_a_profile_edit_of_one_entry() {
    let Some((daemon, port)) = booted("ui-swap") else {
        return;
    };
    let bundle_reads = |daemon: &Daemon| {
        daemon
            .ledger_rows()
            .iter()
            .filter(|row| row.entry.as_deref() == Some(TRANSPORT))
            .filter(|row| is_call(row, BUNDLE_CONTRACT, OP_BUNDLE))
            .count()
    };
    let (_, transport_before) = lifecycle(port, TRANSPORT);
    let (_, settings_before) = lifecycle(port, SETTINGS);
    let (_, plugins_before) = lifecycle(port, PLUGINS);
    let reads_before = bundle_reads(&daemon);
    let (status, _, before) = fetch_bytes(port, "/", None);
    assert_eq!(status, 200);
    assert!(
        !String::from_utf8_lossy(&before).contains(MARKER_META),
        "the first bundle carries no marker"
    );

    // The edit: ONE entry's package and hash.
    let marked = artifact_hash(&daemon.root, UI_MARKED);
    let edited = Instant::now();
    daemon.edit_profile(|document| {
        let entry = entry_mut(document, BUNDLE_ID);
        entry["package"] = serde_json::json!(format!("ui/{UI_MARKED}"));
        entry["hash"] = serde_json::json!(marked);
    });

    // The swap is a RESTART (M2-K24; FINDINGS.md #46 fixed at pin
    // a53a352): the bundle entry's artifact changed, so the transport —
    // its declared consumer — is unloaded under `DependencyChanged` and
    // reloaded, and the NEW incarnation reads the new bundle at its own
    // activation. Between the two incarnations the port is closed: the
    // blip the card predicted, MEASURED here (refused connects, and the
    // time from the edit to the marker), never asserted away. A page
    // that does answer is whole — the old bytes or the new, never a
    // mixed set — because each incarnation serves only what it verified.
    let mut refused = 0u32;
    let mut first_marked: Option<Instant> = None;
    let deadline = edited + Duration::from_secs(60);
    while first_marked.is_none() {
        assert!(
            Instant::now() < deadline,
            "the swap to land\n{}",
            daemon.log()
        );
        match probe_document(port) {
            None => refused += 1,
            Some((200, body)) => {
                if String::from_utf8_lossy(&body).contains(UI_MARKER) {
                    first_marked = Some(Instant::now());
                }
            }
            Some((status, _)) => panic!("a page during the swap answered {status}"),
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let landed = first_marked.expect("marked") - edited;

    // Incarnation +1 EXACTLY, one bundle crossing per incarnation, and
    // the kernel's own word for why: the one `Unloading` row on the
    // transport names `DependencyChanged` — its declared provider moved.
    // No consumer that declares nothing moved.
    daemon.eventually("the new incarnation's read to land on the ledger", || {
        bundle_reads(&daemon) == reads_before + 1
    });
    let (state, transport_after) = lifecycle(port, TRANSPORT);
    assert_eq!(state, "active");
    assert_eq!(
        transport_after,
        transport_before + 1,
        "the swap is a restart: the transport's incarnation +1 exactly (M2-K24)"
    );
    assert_eq!(
        bundle_reads(&daemon),
        reads_before + 1,
        "one bundle crossing per incarnation"
    );
    let unload_causes: Vec<serde_json::Value> = daemon
        .ledger_rows()
        .iter()
        .filter(|row| {
            row.entry.as_deref() == Some(TRANSPORT) && row.kind.contains(r#""to":"Unloading""#)
        })
        .map(|row| kind_of(row).1["cause"].clone())
        .collect();
    assert_eq!(
        unload_causes,
        [serde_json::json!("DependencyChanged")],
        "the kernel unloaded the transport because its declared provider changed"
    );
    println!(
        "proof 4: swap served {landed:?} after the edit; blip: {refused} refused connects while it landed; transport incarnation {transport_before} -> {transport_after}; bundle crossings {reads_before} -> {}",
        bundle_reads(&daemon)
    );
    assert_eq!(
        lifecycle(port, SETTINGS).1,
        settings_before,
        "the settings consumer untouched"
    );
    assert_eq!(
        lifecycle(port, PLUGINS).1,
        plugins_before,
        "the plugins catalog untouched"
    );
    let (_, headers, document) = fetch_bytes(port, "/", None);
    assert_eq!(
        header(&headers, "cache-control"),
        Some(CACHE_NO_STORE_REVALIDATE)
    );
    assert!(String::from_utf8_lossy(&document).contains(UI_MARKER));
    daemon.interrupt();
}

/// Points the root's bundle entry at the CORRUPTED variant before boot.
fn corrupt_bundle_entry(root: &std::path::Path) {
    let corrupt = artifact_hash(root, UI_CORRUPT);
    let path = root.join("profile.json");
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("profile")).expect("parses");
    {
        let entry = entry_mut(&mut document, BUNDLE_ID);
        entry["package"] = serde_json::json!(format!("ui/{UI_CORRUPT}"));
        entry["hash"] = serde_json::json!(corrupt);
    }
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&document).expect("encodes"),
    )
    .expect("profile");
}

/// Whether the transport's fiber ever read `Failed`.
fn transport_failed(daemon: &Daemon) -> bool {
    daemon
        .ledger_rows()
        .iter()
        .any(|row| row.entry.as_deref() == Some(TRANSPORT) && row.kind.contains(r#""to":"Failed""#))
}

/// Every sibling of the transport reached Active and none failed.
fn siblings_active(daemon: &Daemon) {
    for sibling in [
        SETTINGS,
        PLUGINS,
        BUNDLE_ID,
        "jinn-status",
        "jinn-profile-edit",
    ] {
        daemon.eventually(&format!("{sibling} to be Active"), || {
            daemon.ledger_rows().iter().any(|row| {
                row.entry.as_deref() == Some(sibling) && row.kind.contains(r#""to":"Active""#)
            })
        });
        assert!(
            !daemon
                .ledger_rows()
                .iter()
                .any(|row| row.entry.as_deref() == Some(sibling)
                    && row.kind.contains(r#""to":"Failed""#)),
            "{sibling} did not fail"
        );
    }
}

#[test]
fn a_bundle_that_does_not_match_its_manifest_never_serves_a_byte() {
    let Some(binary) = gate() else {
        return;
    };
    // The ui root, its bundle entry pointed at the CORRUPTED variant
    // before boot. ONE order (M2-K24; FINDINGS.md #45 fixed at pin
    // a53a352): the transport declares the bundle, so the kernel begins
    // its activation only once the provider is Active; the one read
    // happens INSIDE that activation, the verify refuses it there, and
    // the fiber reads `Failed` with the port never opened — while every
    // sibling reaches Active regardless (R11: a bad bundle fails the
    // transport's activation and nothing else). The late-provider order
    // the previous pin had is not reachable any more: a declared consumer
    // whose provider is absent rests `pending`, never `active` without
    // its bundle.
    let (root, port) = fresh_ui_root("ui-corrupt");
    corrupt_bundle_entry(&root);
    let daemon = Daemon::boot_operator(binary, &root);
    daemon.await_ready();
    daemon.eventually(
        "the corrupt bundle to fail the transport's activation",
        || transport_failed(&daemon),
    );
    siblings_active(&daemon);
    assert!(!listening(port), "a failed transport holds no listener");
    // The one read, whole: a manifest and a bundle crossing each exactly
    // once on the record, and nothing served from them.
    let crossings = |operation: &str| {
        daemon
            .ledger_rows()
            .iter()
            .filter(|row| row.entry.as_deref() == Some(TRANSPORT))
            .filter(|row| is_call(row, BUNDLE_CONTRACT, operation))
            .count()
    };
    assert_eq!(crossings(OP_MANIFEST), 1, "one manifest crossing");
    assert_eq!(crossings(OP_BUNDLE), 1, "one bundle crossing");
    // The REASON — the verify mismatch this transport named in its typed
    // fault — is on the ledger only because the transport writes it
    // there itself before failing: a guest's activation failure records
    // its state and never its reason (FINDINGS.md #38, KG-5, open).
    let reason_recorded = daemon
        .ledger_kinds()
        .iter()
        .any(|kind| kind.contains("activation failed") && kind.contains("verify"));
    println!(
        "proof 5: corrupt bundle refused at activation — the transport's fiber failed, the port never opened; the refusal's reason on the record: {reason_recorded} (the transport's own label; #38)"
    );
    daemon.interrupt();

    // The operator-api profile mounts no bundle: /v1 keeps serving and a
    // page is a typed 503, with no door and no crossing.
    let (root, port) = fresh_api_root("ui-absent");
    let daemon = Daemon::boot_operator(binary, &root);
    daemon.await_ready();
    let health = get(port, "/v1/health");
    assert_eq!(health.status, 200, "{}", health.raw);
    let (status, headers, _) = fetch_bytes(port, "/", None);
    assert_eq!(status, 503);
    assert!(header(&headers, "content-type").is_some_and(|mime| mime.starts_with("text/plain")));
    daemon.interrupt();
}

/// 5b (§8 amendment 4): TEN consecutive boots of the `ui` profile, each on
/// a fresh root through the pinned daemon, every one reaching the
/// transport `Active` AND listening with `GET /` answering the document.
/// The verifier reproduced the coin toss by hand at pin `85d36b4`
/// (transport `Failed`, bundle `Active`, port never opened); at `a53a352`
/// the kernel's gate on the declared `injects` is what makes the boot
/// deterministic, with no harness-side subscription or probe left. A boot
/// that fails here prints the transport's rows and the reason it named on
/// the record (#38) before the assertion, so a toss is a transcript and
/// not a rumour.
#[test]
fn a_fresh_boot_is_deterministic() {
    let Some(binary) = gate() else {
        return;
    };
    const BOOTS: usize = 10;
    let mut timings = Vec::with_capacity(BOOTS);
    for boot in 1..=BOOTS {
        let (root, port) = fresh_ui_root(&format!("ui-boot-{boot}"));
        let started = Instant::now();
        let daemon = Daemon::boot_operator(binary, &root);
        daemon.await_ready();
        let settled = |daemon: &Daemon| {
            daemon.ledger_rows().iter().any(|row| {
                row.entry.as_deref() == Some(TRANSPORT)
                    && (row.kind.contains(r#""to":"Active""#)
                        || row.kind.contains(r#""to":"Failed""#))
            })
        };
        daemon.eventually("the transport to settle", || settled(&daemon));
        let transport_rows: Vec<String> = daemon
            .ledger_rows()
            .iter()
            .filter(|row| row.entry.as_deref() == Some(TRANSPORT))
            .map(|row| format!("{:>4} {}", row.seq, row.kind))
            .collect();
        let named: Vec<String> = daemon
            .ledger_kinds()
            .into_iter()
            .filter(|kind| kind.contains("activation failed"))
            .collect();
        let failed = daemon.ledger_rows().iter().any(|row| {
            row.entry.as_deref() == Some(TRANSPORT) && row.kind.contains(r#""to":"Failed""#)
        });
        assert!(
            !failed,
            "boot {boot}/{BOOTS}: the transport FAILED its activation\nreason on the record: {named:#?}\ntransport rows:\n{}\n--- daemon log ---\n{}",
            transport_rows.join("\n"),
            daemon.log()
        );
        daemon.eventually("the transport to listen", || listening(port));
        // The document, within the ready budget: an Active transport that
        // still answers 503 would be a transport activated without its
        // bundle — impossible under the kernel's gate — and the rows would
        // say so.
        let mut served = fetch_bytes(port, "/", None);
        let budget = Instant::now() + Duration::from_secs(10);
        while served.0 != 200 && Instant::now() < budget {
            std::thread::sleep(Duration::from_millis(100));
            served = fetch_bytes(port, "/", None);
        }
        let (status, _, document) = served;
        assert_eq!(
            status,
            200,
            "boot {boot}/{BOOTS}: the document not served by an Active transport\ntransport rows:\n{}\ndeliveries:\n{}",
            daemon
                .ledger_rows()
                .iter()
                .filter(|row| row.entry.as_deref() == Some(TRANSPORT))
                .map(|row| format!("{:>4} {}", row.seq, row.kind))
                .collect::<Vec<_>>()
                .join("\n"),
            daemon
                .ledger_kinds()
                .iter()
                .filter(|kind| kind.contains("DispatchTrace"))
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
        assert_eq!(
            hex_sha256(&document),
            manifest(&daemon)
                .files
                .iter()
                .find(|file| file.path == "index.html")
                .expect("the document")
                .sha256,
            "boot {boot}: the pinned document"
        );
        timings.push(started.elapsed());
        daemon.interrupt();
    }
    println!(
        "proof 5b: {BOOTS}/{BOOTS} fresh boots reached transport active + listening + document served; boot-to-served {:?}",
        timings
    );
}
