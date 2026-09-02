//! THE BUNDLE (harness packet UI-1, PLA-349): the web UI is one plugin
//! artifact served same-origin by the transport from memory it filled at
//! activation. Every proof boots the `ui` profile through the REAL pinned
//! daemon (AGENTS.md standing order 3) and drives it over loopback as a
//! browser would — a GET with no header for a byte, a bearer for `/v1`.
//! Evidence is the wire (status, headers, bytes) and the ledger (parsed
//! row by row). The proofs are the card's §4.3 items 1–5; item 6 is the
//! repo gate in `tools/ui-kit/tests/verbatim.rs`, item 7 the verifier's.
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
    // The manifest is asked once when the provider was live at the
    // transport's activation; up to three times otherwise (FINDINGS.md
    // #7): two probes that answered not-yet around the subscription, then
    // the read on the witnessed Active transition.
    assert!(
        (1..=3).contains(&manifest_calls),
        "manifest crossings: {manifest_calls}"
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

    // Until the marker is served: the old bytes, never a refused connect
    // and never a mixed set — the transport keeps its memory until the
    // new entry is witnessed Active and re-read in one step.
    let mut refused = 0u32;
    let mut first_marked: Option<Instant> = None;
    let deadline = edited + Duration::from_secs(60);
    while first_marked.is_none() {
        assert!(
            Instant::now() < deadline,
            "the swap to land\n{}",
            daemon.log()
        );
        if !listening(port) {
            refused += 1;
        } else {
            let (status, _, body) = fetch_bytes(port, "/", None);
            assert_eq!(status, 200, "a page during the swap");
            if String::from_utf8_lossy(&body).contains(UI_MARKER) {
                first_marked = Some(Instant::now());
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let landed = first_marked.expect("marked") - edited;

    // Exactly one more bundle crossing — the re-read on the witnessed
    // transition — and no consumer moved. AT THE PINNED KERNEL the
    // transport's incarnation is ASSERTED UNCHANGED (card §4.3 item 4 as
    // amended, §8 amendment 4): epoch gating does not reach a wasm entry
    // resolving on the string lane (FINDINGS.md #46), so the swap is the
    // re-read on the record, never a restart and never a silent
    // replacement. When jinnd M2-K24 lands through pin-bump 7 this line
    // flips to `transport_before + 1` and the transitions subscription
    // goes.
    daemon.eventually("the re-read to land on the ledger", || {
        bundle_reads(&daemon) == reads_before + 1
    });
    let (state, transport_after) = lifecycle(port, TRANSPORT);
    assert_eq!(state, "active");
    assert_eq!(
        transport_after, transport_before,
        "the transport did not restart on the swap at pin 85d36b4 (#46)"
    );
    println!(
        "proof 4: swap served {landed:?} after the edit; refused connects while it landed: {refused}; transport incarnation {transport_before} -> {transport_after}; bundle crossings {reads_before} -> {}",
        bundle_reads(&daemon)
    );
    assert_eq!(refused, 0, "no blip: the transport never stopped listening");
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

/// Points the root's bundle entry at the CORRUPTED variant; answers the
/// entry as it now stands so a caller can add it back later.
fn corrupt_bundle_entry(root: &std::path::Path) -> serde_json::Value {
    let corrupt = artifact_hash(root, UI_CORRUPT);
    let path = root.join("profile.json");
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("profile")).expect("parses");
    let entry = {
        let entry = entry_mut(&mut document, BUNDLE_ID);
        entry["package"] = serde_json::json!(format!("ui/{UI_CORRUPT}"));
        entry["hash"] = serde_json::json!(corrupt);
        entry.clone()
    };
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&document).expect("encodes"),
    )
    .expect("profile");
    entry
}

/// Whether the transport's fiber ever read `Failed`.
fn transport_failed(daemon: &Daemon) -> bool {
    daemon
        .ledger_rows()
        .iter()
        .any(|row| row.entry.as_deref() == Some(TRANSPORT) && row.kind.contains(r#""to":"Failed""#))
}

/// Whether a transitions delivery to a listener failed on the record
/// (the kernel contains a failing listener, R9: `failures: 1`).
fn delivery_failed(daemon: &Daemon) -> bool {
    daemon.ledger_kinds().iter().any(|kind| {
        kind.contains("DispatchTrace")
            && kind.contains("jinn:introspect/transitions")
            && !kind.contains(r#""failures":0"#)
    })
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
    // before boot: the transport must refuse to serve and fail its own
    // activation, and every sibling must reach Active regardless.
    // ORDER (i): the provider present at boot. At the pinned kernel the
    // transport meets it in one of two orders (FINDINGS.md #45), both
    // fail-closed and both on the record: live at the transport's
    // activation — verify fails the ACTIVATION, the fiber reads `Failed`
    // and the port never opens; landing later — verify fails inside the
    // transitions DELIVERY, which the kernel contains (R9: a failing
    // listener is recorded, `failures: 1`) and the transport stays Active
    // with no bundle, answering every page a typed 503. The proof asserts
    // WHICH occurred, and that no byte was served either way.
    let (root, port) = fresh_ui_root("ui-corrupt");
    corrupt_bundle_entry(&root);
    let daemon = Daemon::boot_operator(binary, &root);
    daemon.await_ready();
    daemon.eventually("the corrupt bundle to be refused on the record", || {
        transport_failed(&daemon) || delivery_failed(&daemon)
    });
    let at_activation = transport_failed(&daemon);
    let order = if at_activation {
        "activation: the transport's fiber failed, the port never opened"
    } else {
        "delivery: the transitions handler's failure recorded, the transport serves a typed 503"
    };
    println!("proof 5 (i): corrupt bundle refused at {order}");
    siblings_active(&daemon);
    if at_activation {
        assert!(!listening(port), "a failed transport holds no listener");
    } else {
        daemon.eventually("the transport to listen", || listening(port));
        let (status, ..) = fetch_bytes(port, "/", None);
        assert_eq!(
            status, 503,
            "a transport holding no verified bundle serves no page"
        );
        assert_eq!(get(port, "/v1/health").status, 200, "/v1 keeps serving");
    }
    // The REASON — the verify mismatch this transport named in its typed
    // fault — is on neither the ledger nor the log at this pin: a guest's
    // activation failure records its state and never its reason
    // (FINDINGS.md #38, KG-5). Recorded here rather than asserted around.
    let reason_recorded = daemon
        .ledger_kinds()
        .iter()
        .any(|kind| kind.contains("activation failed") && kind.contains("verify"));
    println!(
        "proof 5 (i): the refusal's reason on the record: {reason_recorded} (the transport's own label; #38)"
    );
    daemon.interrupt();

    // ORDER (ii), FORCED: the bundle entry lands by a profile edit AFTER
    // the transport is Active. Boot with the entry ABSENT (the transport
    // keeps its `ui-bundle-entry` config and grants: a mounted-but-absent
    // bundle is the no-bundle 503, not a failure), then add the corrupt
    // entry: the transport witnesses its Active transition, re-reads,
    // verify refuses inside the delivery — contained (`failures: 1`) —
    // and the transport stays Active, its incarnation unchanged, serving
    // the typed 503 and `/v1` as before. No byte, no restart.
    let (root, port) = fresh_ui_root("ui-corrupt-late");
    // The corrupt entry, held back: written into the profile, then cut
    // out of it before the boot, added back by the edit.
    let corrupt = corrupt_bundle_entry(&root);
    let path = root.join("profile.json");
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("profile")).expect("parses");
    let entries = document["entries"].as_array_mut().expect("entries");
    let absent = entries
        .iter()
        .position(|entry| entry["id"] == BUNDLE_ID)
        .expect("the bundle entry");
    entries.remove(absent);
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&document).expect("encodes"),
    )
    .expect("profile");
    let daemon = Daemon::boot_operator(binary, &root);
    daemon.await_ready();
    daemon.eventually("the transport to listen without a bundle", || {
        listening(port)
    });
    let (status, ..) = fetch_bytes(port, "/", None);
    assert_eq!(status, 503, "no bundle mounted yet: the typed 503");
    let (state, incarnation_before) = lifecycle(port, TRANSPORT);
    assert_eq!(state, "active");
    let seq_before = last_seq(&daemon);
    daemon.edit_profile(|document| {
        document["entries"]
            .as_array_mut()
            .expect("entries")
            .push(corrupt.clone());
    });
    daemon.eventually("the late corrupt bundle entry to be Active", || {
        daemon.ledger_rows().iter().any(|row| {
            row.seq > seq_before
                && row.entry.as_deref() == Some(BUNDLE_ID)
                && row.kind.contains(r#""to":"Active""#)
        })
    });
    daemon.eventually("the delivery-time refusal on the record", || {
        delivery_failed(&daemon)
    });
    assert!(
        !transport_failed(&daemon),
        "the late order never fails the transport's fiber (#45)"
    );
    let (state, incarnation_after) = lifecycle(port, TRANSPORT);
    assert_eq!(state, "active");
    assert_eq!(incarnation_after, incarnation_before, "no restart");
    let (status, headers, _) = fetch_bytes(port, "/", None);
    assert_eq!(status, 503, "still no verified bundle: no byte served");
    assert!(header(&headers, "content-type").is_some_and(|mime| mime.starts_with("text/plain")));
    assert_eq!(get(port, "/v1/health").status, 200, "/v1 keeps serving");
    siblings_active(&daemon);
    println!("proof 5 (ii): the late corrupt bundle refused at delivery; transport incarnation {incarnation_before} -> {incarnation_after}, no byte served");
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
/// The verifier reproduced the coin toss by hand (transport `Failed`,
/// bundle `Active`, port never opened); a boot that fails here prints the
/// transport's rows and the reason it named on the record (#38) before
/// the assertion, so the toss is a transcript and not a rumour.
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
        let (status, _, document) = fetch_bytes(port, "/", None);
        assert_eq!(status, 200, "boot {boot}: the document");
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
