//! THE MOMENTS and the JS-in-WASM extension tier (harness packet UI-2,
//! PLA-353; `docs/plans/ui-malleability-arc.md` §9.3, proofs 1–8 and 10;
//! proof 7 flipped and proof 7b added by pin-bump 8, PLA-361).
//! Every proof boots the `ui` profile through the REAL pinned daemon
//! (AGENTS.md standing order 3) — the kit-built profile mounting the Boa
//! engine provider as `ext-green` (§6) — and drives the transport over
//! loopback as the client would: `POST /v1/moments/<domain>/<topic>` with
//! the operator's bearer. Every ledger claim is read from the daemon's
//! own SQLite (`Daemon::ledger_rows`), never from the transport's answer
//! alone. Proof 9 is the repo gate in `tools/ui-kit/tests/verbatim.rs`;
//! proof 11 is the independent verifier's, over `agent-browser`.
//!
//! Self-skips LOUDLY when no jinnd checkout holding the pinned commit is
//! reachable (KERNEL-PIN.md Gate 2).
//!
//! The extension tier's vocabulary is SPELLED below rather than imported
//! from `jinn-ui`, `jinn-ext` and `ext-kit`: these proofs must compile and
//! run RED on the merge-base without the implementation (§9.7 amendment
//! 8(e), red-by-reversion; `docs/notes/ui-2-red-transcript.md`), so what
//! the card names is what the proofs name. The crates stay the one home
//! for the production code; a literal that drifts from its crate fails
//! proof 2 and proof 8 on the ledger, which is the point of spelling it.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use composition::api::{get, post, request, request_as, Response};
use composition::daemon::{jinnd_source, pinned_commit, pinned_daemon};
use composition::kit::{
    artifact_hash, entry_mut, fresh_ui_root, suite_credential, Daemon, LedgerRow,
};
use composition::plugins::{history, state};
use jinn_api::{AUTH_CONTRACT, OP_VERIFY};
use sha2::{Digest, Sha256};

const TRANSPORT: &str = "jinn-api-http";
const CATALOG: &str = "main";
const SEND_PATH: &str = "/v1/moments/ui/before-send";
const CREATE_PATH: &str = "/v1/moments/ui/before-create-session";
/// The §6 body: what the composer sends before the optimistic bubble.
const SEND_BODY: &str = r#"{"text":"hello","session-id":"session-1","attachments":[]}"#;
/// The guest deadline at the pin (`lane::DEADLINE`, `crates/jinnd-wasm/src/lane.rs`).
const GUEST_DEADLINE: Duration = Duration::from_secs(5);
/// Proof 2's line in the sand: above it the cost of one moment is KG-7.
const KG7_BOUND: Duration = Duration::from_millis(250);

/// The three moment topics the card names (§9.2; `jinn-ui`'s vocabulary).
const TOPIC_BEFORE_SEND: &str = "jinn:ui/before-send";
const TOPIC_BEFORE_CREATE_SESSION: &str = "jinn:ui/before-create-session";
/// The first engine provider: its package (the entry's `package`) and its
/// artifact basename (the kit's sidecar) — `jinn-ext` / `ext-kit`.
const BOA_PACKAGE: &str = "ext/jinn-ext-js-boa";
const BOA_GUEST: &str = "jinn-ext-js-boa";
/// The ONE host provider an engine reads (§5.4 lesson 1).
const CLOCK_CONTRACT: &str = "jinn:clock";
/// The activation breadcrumbs, in the order the guest registers them
/// (§5.4; FINDINGS #38's discipline).
const BREADCRUMBS: [&str; 4] = [
    "activate entered",
    "config parsed",
    "js context built",
    "js evaluated",
];
/// The operator's example from §6, mounted by the kit with `origin: human`.
const GREEN_ID: &str = "ext-green";
const GREEN_SOURCE: &str = "(p) => ({ ...p, text: p.text + ' 🟢' })";
/// A second extension appending a different marker (proof 3).
const BLUE_ID: &str = "ext-blue";
const BLUE_SOURCE: &str = "(p) => ({ ...p, text: p.text + ' 🔵' })";
/// A source that throws on every delivery (proof 4).
const THROWING_ID: &str = "ext-throwing";
const THROWING_SOURCE: &str = "(p) => { throw new Error('the throwing extension'); }";
/// A source that returns `undefined`: the pass-through case (proof 4).
const UNDEFINED_SOURCE: &str = "(p) => undefined";
/// A source that does not parse: a failed fiber on the record (proof 8).
const BROKEN_SOURCE: &str = "(p) => { this is not javascript";
/// A source that loops forever on delivery (proof 7).
const LOOPING_SOURCE: &str = "(p) => { while (true) {} }";
const LOOPING_ID: &str = "ext-looping";
/// An entry declaring a zero budget (proof 7b, second half).
const ZERO_ID: &str = "ext-zero";
/// The budgets proof 7b declares, in the store's own fuel (the kernel's
/// `delivery-budget { fuel }` at pin `b1dbe8f`, M2-K25 (b)): generous on
/// the fold, small on the looping listener.
const GREEN_FUEL: u64 = 4_000_000_000;
const LOOPING_FUEL: u64 = 50_000_000;
/// Half the guest deadline: a budgeted death lands far under it.
const BUDGET_BOUND: Duration = Duration::from_millis(2_500);

/// A source whose ACTIVATION is slow by construction: a bounded counting
/// loop under fuel, run when the source expression is evaluated, then
/// the fold — never `while(true)` (proof 5's restart window).
fn slow_source(iterations: u64, marker: &str) -> String {
    format!("(function () {{ var i = 0; while (i < {iterations}) i++; return (p) => ({{ ...p, text: p.text + ' {marker}' }}); }})()")
}

/// `sha256:<hex>` of the source as configured: what the catalog's
/// `attestation.source` carries (proof 8; §9.7 amendment 8(d)).
fn source_digest(source: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(source.as_bytes()))
}

/// The fifth breadcrumb, `source sha256:<hex>`: WHAT CODE RAN, on the
/// record (§8 ruling 1).
fn source_breadcrumb(source: &str) -> String {
    format!("source {}", source_digest(source))
}

/// The extension entry in §6's "Install" shape: `config.data` carries the
/// topics, the source and the origin; `config.grants` is the topics (each
/// its own grant name) plus the one host provider the engine reads;
/// `injects` is absent — an extension injects no service.
fn ext_entry(
    id: &str,
    hash: &str,
    topics: &[&str],
    source: &str,
    origin: &str,
) -> serde_json::Value {
    let mut grants: Vec<serde_json::Value> = topics.iter().map(|t| serde_json::json!(t)).collect();
    grants.push(serde_json::json!(CLOCK_CONTRACT));
    serde_json::json!({ "id": id, "package": BOA_PACKAGE, "hash": hash,
                        "config": { "grants": grants,
                                    "data": { "topics": topics, "source": source,
                                              "origin": origin } } })
}

/// The entry's `budget`: the kernel's `delivery-budget` record spelled on
/// `config.data` (`{ "fuel": <u64> }`), which the provider translates into
/// `events.listen-within` for every topic it listens on (pin-bump 8).
fn budgeted(mut entry: serde_json::Value, fuel: u64) -> serde_json::Value {
    entry["config"]["data"]["budget"] = serde_json::json!({ "fuel": fuel });
    entry
}

/// A NOT-YET assertion (§9.7 amendment 8(b)): asserts the pinned kernel's
/// CURRENT behaviour by the finding's name, so the day a pin answers the
/// finding this proof fails loudly and is flipped — never a print nobody
/// reads.
fn not_yet(finding: &str, still_holds: bool, what_changed: &str) {
    assert!(
        still_holds,
        "NOT-YET {finding} is ANSWERED at this pin — {what_changed}; flip the assertion and close the finding"
    );
}

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

/// The provider's pin in this root (the kit's sidecar).
fn boa_hash(root: &Path) -> String {
    artifact_hash(root, BOA_GUEST)
}

/// An extension entry on the §6 shape, pinned to this root's provider.
fn extension(root: &Path, id: &str, topics: &[&str], source: &str) -> serde_json::Value {
    ext_entry(id, &boa_hash(root), topics, source, "agent")
}

/// Edits the root's profile document BEFORE boot.
fn edit_before_boot(root: &Path, edit: impl FnOnce(&mut serde_json::Value)) {
    let path = root.join("profile.json");
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("profile")).expect("parses");
    edit(&mut document);
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&document).expect("encodes"),
    )
    .expect("profile");
}

fn remove_entry(document: &mut serde_json::Value, id: &str) {
    let entries = document["entries"].as_array_mut().expect("entries");
    let before = entries.len();
    entries.retain(|entry| entry["id"] != id);
    assert_eq!(entries.len(), before - 1, "{id} was mounted");
}

fn push_entry(document: &mut serde_json::Value, entry: serde_json::Value) {
    document["entries"]
        .as_array_mut()
        .expect("entries")
        .push(entry);
}

/// Boots a fresh `ui` root after `edit`, waits for the API and for every
/// extension entry to settle (`Active` or `Failed`).
fn booted(name: &str, edit: impl FnOnce(&Path, &mut serde_json::Value)) -> Option<(Daemon, u16)> {
    let binary = gate()?;
    let (root, port) = fresh_ui_root(name);
    edit_before_boot(&root, |document| edit(&root, document));
    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("profile.json")).expect("profile"))
            .expect("parses");
    let extensions: Vec<String> = document["entries"]
        .as_array()
        .expect("entries")
        .iter()
        .filter(|entry| entry["package"] == BOA_PACKAGE)
        .map(|entry| entry["id"].as_str().expect("id").to_owned())
        .collect();
    let daemon = Daemon::boot_operator(binary, &root);
    daemon.await_ready();
    let health = get(port, "/v1/health");
    assert_eq!(health.status, 200, "{}", health.raw);
    for id in &extensions {
        daemon.eventually(&format!("{id} to settle"), || settled(&daemon, id));
    }
    Some((daemon, port))
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

fn settled(daemon: &Daemon, id: &str) -> bool {
    daemon.ledger_rows().iter().any(|row| {
        row.entry.as_deref() == Some(id)
            && (row.kind.contains(r#""to":"Active""#) || row.kind.contains(r#""to":"Failed""#))
    })
}

/// Every `DispatchTrace` row on a MOMENT topic after `seq`, as its
/// fields (the cron seam emits on its own topics beside these).
fn traces(daemon: &Daemon, seq: u64) -> Vec<(LedgerRow, serde_json::Value)> {
    daemon
        .ledger_rows()
        .into_iter()
        .filter(|row| row.seq > seq)
        .filter_map(|row| {
            let (name, fields) = kind_of(&row);
            (name == "DispatchTrace"
                && fields["topic"]
                    .as_str()
                    .is_some_and(|topic| topic.starts_with("jinn:ui/")))
            .then_some((row, fields))
        })
        .collect()
}

/// The labels an entry registered, in sequence order.
fn labels(daemon: &Daemon, id: &str) -> Vec<String> {
    daemon
        .ledger_rows()
        .iter()
        .filter(|row| row.entry.as_deref() == Some(id))
        .filter_map(|row| {
            let (name, fields) = kind_of(row);
            (name == "EffectRegistered").then(|| fields["label"].as_str().unwrap_or("").to_owned())
        })
        .collect()
}

fn last_seq(daemon: &Daemon) -> u64 {
    daemon.ledger_rows().last().map_or(0, |row| row.seq)
}

/// The ledger's own clock per row (`ts`, unix ms): the walk's cost read
/// off the record rather than off the client's wall.
fn ledger_ts(daemon: &Daemon) -> Vec<(u64, Option<String>, String, i64)> {
    let connection =
        rusqlite::Connection::open(daemon.root.join("ledger.sqlite")).expect("ledger opens");
    let mut select = connection
        .prepare("SELECT seq, entry, kind, ts FROM events ORDER BY seq")
        .expect("ledger schema");
    let rows = select
        .query_map([], |row| {
            Ok((
                row.get::<_, u64>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })
        .expect("ledger reads")
        .collect::<Result<Vec<_>, _>>()
        .expect("ledger rows");
    rows
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

fn is_transport(row: &LedgerRow) -> bool {
    let (name, fields) = kind_of(row);
    match name.as_str() {
        "NetAccepted" | "NetReadable" | "NetClosed" => true,
        "ContractCall" => fields["contract"] == "jinn:net",
        _ => false,
    }
}

fn closed_segments(daemon: &Daemon, seq: u64, count: usize) -> Vec<Vec<LedgerRow>> {
    daemon.eventually(
        &format!("{count} connections to close on the ledger"),
        || {
            segments(daemon, seq)
                .iter()
                .filter(|segment| kind_of(segment.last().expect("a row")).0 == "NetClosed")
                .count()
                >= count
        },
    );
    let segments = segments(daemon, seq);
    assert_eq!(segments.len(), count, "one segment per connection");
    segments
}

fn send(port: u16) -> Response {
    request(port, "POST", SEND_PATH, Some(SEND_BODY))
}

/// The response body EXACTLY as the wire carried it.
fn raw_body(response: &Response) -> &str {
    response
        .raw
        .split_once("\r\n\r\n")
        .map_or("", |(_, body)| body)
}

fn one_trace(daemon: &Daemon, seq: u64, topic: &str) -> serde_json::Value {
    let traces = traces(daemon, seq);
    assert_eq!(traces.len(), 1, "exactly one walk: {traces:#?}");
    let (row, fields) = &traces[0];
    assert_eq!(
        row.entry.as_deref(),
        Some(TRANSPORT),
        "the emitter is the transport"
    );
    assert_eq!(fields["topic"], topic);
    assert!(
        fields["mode"]
            .as_str()
            .is_some_and(|mode| mode.eq_ignore_ascii_case("waterfall")),
        "{fields}"
    );
    fields.clone()
}

/// A request that tolerates the transport dying mid-answer or never
/// answering: the elapsed time, the status line if one came (`None` on a
/// closed socket, a refused connect, or `wait` elapsing), and the raw
/// wire as far as it got.
fn tolerant(
    port: u16,
    method: &str,
    path: &str,
    body: &str,
    wait: Duration,
) -> (Duration, Option<u16>, String) {
    use std::io::{Read as _, Write as _};
    let started = Instant::now();
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = std::net::TcpStream::connect_timeout(&address, Duration::from_secs(2))
    else {
        return (started.elapsed(), None, String::new());
    };
    stream.set_read_timeout(Some(wait)).expect("read timeout");
    let wire = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        suite_credential(),
        body.len()
    );
    if stream.write_all(wire.as_bytes()).is_err() {
        return (started.elapsed(), None, String::new());
    }
    let mut raw = Vec::new();
    let _ = stream.read_to_end(&mut raw);
    let raw = String::from_utf8_lossy(&raw).into_owned();
    let status = raw
        .strip_prefix("HTTP/1.1 ")
        .and_then(|rest| rest.get(..3))
        .and_then(|code| code.parse().ok());
    (started.elapsed(), status, raw)
}

#[test]
fn a_moment_with_no_listener_answers_its_own_payload() {
    let Some((daemon, port)) = booted("moments-none", |_, document| {
        remove_entry(document, GREEN_ID)
    }) else {
        return;
    };
    let baseline = last_seq(&daemon);
    let answer = send(port);
    assert_eq!(answer.status, 200, "{}", answer.raw);
    assert_eq!(raw_body(&answer), SEND_BODY, "the body, byte for byte");
    daemon.eventually("the walk on the ledger", || {
        !traces(&daemon, baseline).is_empty()
    });
    let trace = one_trace(&daemon, baseline, TOPIC_BEFORE_SEND);
    assert_eq!(trace["listeners"], 0);
    assert_eq!(trace["failures"], 0);
    println!("proof 1: no listener — the body answered unmodified; {trace}");
    daemon.interrupt();
}

#[test]
fn one_js_extension_folds_the_payload_and_the_ledger_says_so() {
    let Some((daemon, port)) = booted("moments-green", |_, _| {}) else {
        return;
    };
    assert_eq!(
        state(&get(port, &format!("/v1/plugins/{CATALOG}/{GREEN_ID}")).body),
        "active"
    );
    let baseline = last_seq(&daemon);
    let answer = send(port);
    assert_eq!(answer.status, 200, "{}", answer.raw);
    assert_eq!(answer.body["text"], "hello 🟢", "{}", answer.raw);
    assert_eq!(answer.body["session-id"], "session-1");
    daemon.eventually("the walk on the ledger", || {
        !traces(&daemon, baseline).is_empty()
    });
    let trace = one_trace(&daemon, baseline, TOPIC_BEFORE_SEND);
    assert_eq!(trace["listeners"], 1);
    assert_eq!(trace["failures"], 0);

    // The extension's own rows, in order: the four breadcrumbs, WHAT
    // CODE RAN, the listen (§5.4's good row).
    let mut expected: Vec<String> = BREADCRUMBS.iter().map(|s| (*s).to_owned()).collect();
    expected.push(source_breadcrumb(GREEN_SOURCE));
    expected.push(format!("listen {TOPIC_BEFORE_SEND}"));
    assert_eq!(labels(&daemon, GREEN_ID), expected);

    // Proof 2's MEASUREMENT (§5.5's unmeasured item): twenty walks, the
    // wall per walk from the request to the answer, and the ledger's own
    // clock from the walk's trace row back to the transport's previous
    // row on that connection (the door's decision) — the cost of one
    // moment on the spike's shape, a Boa context per delivery.
    const WALKS: usize = 20;
    let before = last_seq(&daemon);
    let mut walls = Vec::with_capacity(WALKS);
    for _ in 0..WALKS {
        let started = Instant::now();
        let answer = send(port);
        walls.push(started.elapsed());
        assert_eq!(answer.status, 200, "{}", answer.raw);
        assert_eq!(answer.body["text"], "hello 🟢");
    }
    daemon.eventually("twenty walks on the ledger", || {
        traces(&daemon, before).len() >= WALKS
    });
    let rows = ledger_ts(&daemon);
    let mut ledger = Vec::with_capacity(WALKS);
    for (index, (seq, _, _, ts)) in rows
        .iter()
        .enumerate()
        .filter(|(_, (seq, entry, kind, _))| {
            *seq > before && entry.as_deref() == Some(TRANSPORT) && kind.contains("DispatchTrace")
        })
    {
        let previous = rows[..index]
            .iter()
            .rev()
            .find(|(_, entry, _, _)| entry.as_deref() == Some(TRANSPORT))
            .expect("a transport row before the walk");
        ledger.push((*seq, ts - previous.3));
    }
    let wall_avg = walls.iter().sum::<Duration>() / WALKS as u32;
    let wall_max = walls.iter().max().copied().unwrap_or_default();
    let ledger_avg = ledger.iter().map(|(_, ms)| *ms).sum::<i64>() / WALKS as i64;
    let ledger_max = ledger.iter().map(|(_, ms)| *ms).max().unwrap_or_default();
    println!(
        "proof 2: {WALKS} walks — wall per walk avg {wall_avg:?} max {wall_max:?} (all: {walls:?}); ledger clock trace-to-previous-transport-row avg {ledger_avg} ms max {ledger_max} ms; guest memory high-water mark: not exposed (jinn:introspect 0.6.0 carries injects/unmet, no memory reading — KG-7's second half)"
    );
    if wall_avg > KG7_BOUND {
        println!(
            "proof 2: the per-walk cost {wall_avg:?} is above {KG7_BOUND:?} — KG-7 is a finding; no Boa context reuse is designed in this packet"
        );
    }
    daemon.interrupt();
}

#[test]
fn two_extensions_compose_in_registration_order_and_the_order_is_named() {
    let Some((daemon, port)) = booted("moments-two", |root, document| {
        let blue = extension(root, BLUE_ID, &[TOPIC_BEFORE_SEND], BLUE_SOURCE);
        push_entry(document, blue);
    }) else {
        return;
    };
    for id in [GREEN_ID, BLUE_ID] {
        assert_eq!(
            state(&get(port, &format!("/v1/plugins/{CATALOG}/{id}")).body),
            "active"
        );
    }
    let baseline = last_seq(&daemon);
    let answer = send(port);
    assert_eq!(answer.status, 200, "{}", answer.raw);
    let text = answer.body["text"].as_str().expect("text").to_owned();
    assert!(text.contains("🟢") && text.contains("🔵"), "{text}");
    daemon.eventually("the walk on the ledger", || {
        !traces(&daemon, baseline).is_empty()
    });
    let trace = one_trace(&daemon, baseline, TOPIC_BEFORE_SEND);
    assert_eq!(trace["listeners"], 2);
    assert_eq!(trace["failures"], 0);
    // Both listens are on the record, one row each, written by the
    // broker at registration. The ORDER of the fold is what the boot
    // dealt: at this pin the walk's order across two sibling listeners
    // on one topic is NOT reliably the listen rows' order (at head
    // `9468fd0` two local runs folded 🔵 then 🟢 under rows reading
    // green, blue; CI's run folded the other way), and no reading
    // exposes the order the walk took — the `DispatchTrace` row carries
    // counts only (FINDINGS.md #52, KG-3). So the proof NAMES both orders
    // and asserts neither equals the other; the NOT-YET assertion is on
    // the reading's absence: a pin whose trace names its deliveries fails
    // it, and the proof is flipped to assert the fold against that.
    let listens: Vec<String> = daemon
        .ledger_rows()
        .iter()
        .filter(|row| {
            row.kind.contains("EffectRegistered")
                && row.kind.contains(&format!("listen {TOPIC_BEFORE_SEND}"))
        })
        .filter_map(|row| row.entry.clone())
        .collect();
    assert_eq!(listens.len(), 2, "{listens:?}");
    let fold_order = if text.find("🟢") < text.find("🔵") {
        [GREEN_ID, BLUE_ID]
    } else {
        [BLUE_ID, GREEN_ID]
    };
    not_yet(
        "FINDINGS.md #52 (the order a walk takes across sibling listeners is not a reading: the DispatchTrace carries counts only)",
        trace.get("deliveries").is_none() && trace.get("order").is_none(),
        &format!("the walk's trace now names its deliveries: {trace}"),
    );
    println!(
        "proof 3: two extensions folded — answer {text:?}, fold order {fold_order:?}, listen rows {listens:?} (KG-3 / FINDINGS #52: the order across siblings is what the boot dealt, and the listen rows are not its witness)"
    );
    daemon.interrupt();
}

#[test]
fn a_throwing_extension_is_recorded_and_the_walk_continues() {
    let Some(binary) = gate() else { return };
    // First half: a throwing extension beside ext-green.
    let (root, port) = fresh_ui_root("moments-throw");
    let throwing = extension(&root, THROWING_ID, &[TOPIC_BEFORE_SEND], THROWING_SOURCE);
    edit_before_boot(&root, |document| push_entry(document, throwing));
    let daemon = Daemon::boot_operator(binary, &root);
    daemon.await_ready();
    for id in [GREEN_ID, THROWING_ID] {
        daemon.eventually(&format!("{id} to settle"), || settled(&daemon, id));
    }
    let baseline = last_seq(&daemon);
    let answer = send(port);
    assert_eq!(answer.status, 200, "{}", answer.raw);
    assert_eq!(
        answer.body["text"], "hello 🟢",
        "ext-green's fold survives (R9)"
    );
    daemon.eventually("the walk on the ledger", || {
        !traces(&daemon, baseline).is_empty()
    });
    let trace = one_trace(&daemon, baseline, TOPIC_BEFORE_SEND);
    assert_eq!(trace["listeners"], 2);
    assert_eq!(trace["failures"], 1);
    // A failed delivery is not a failed activation: the fiber stays active.
    assert_eq!(
        state(&get(port, &format!("/v1/plugins/{CATALOG}/{THROWING_ID}")).body),
        "active"
    );
    // Where the failure is on the record. At the pin a contained delivery
    // failure is the `failures` count on the EMITTER's trace (asserted
    // above) and NOTHING on the listener's own history: after the walk the
    // throwing extension's rows are its one clock read and no failure row
    // (FINDINGS.md #51; §9.7 amendment 8(b)). The card's clause "in ITS
    // history" is NOT-YET, asserted by name: a pin that writes the row
    // fails this assertion, and the proof is flipped to require it.
    let kinds: Vec<String> = history(port, CATALOG, THROWING_ID)["lines"]
        .as_array()
        .expect("lines")
        .iter()
        .filter(|line| line["seq"].as_u64().is_some_and(|seq| seq > baseline))
        .map(|line| line["kind"].as_str().unwrap_or("").to_owned())
        .collect();
    let own_rows: Vec<String> = daemon
        .ledger_rows()
        .iter()
        .filter(|row| row.seq > baseline && row.entry.as_deref() == Some(THROWING_ID))
        .map(|row| row.kind.clone())
        .collect();
    assert!(
        own_rows.iter().any(|row| row.contains(CLOCK_CONTRACT)),
        "the delivery reached the throwing extension (its clock read is on the record): {own_rows:?}"
    );
    not_yet(
        "FINDINGS.md #51 (a contained delivery failure writes nothing on the listener's history)",
        kinds.iter().all(|kind| kind == "ContractCall")
            && !own_rows.iter().any(|row| row.contains("Fail") || row.contains("fail")),
        &format!("the throwing extension's history after the walk now carries {kinds:?} (raw rows {own_rows:?})"),
    );
    println!(
        "proof 4: throwing beside green — failures 1 on the emitter's trace, the fold survived; NOT-YET #51: the throwing extension's history after the walk is {kinds:?} (raw rows {own_rows:?}), no failure row"
    );
    daemon.interrupt();

    // Second half: a source returning `undefined` yields EMPTY output and
    // the payload passes unchanged; a source returning a string is a
    // contained failure (§9.4 probe), and neither aborts the walk.
    let (root, port) = fresh_ui_root("moments-undefined");
    let undefined = extension(
        &root,
        "ext-undefined",
        &[TOPIC_BEFORE_SEND],
        UNDEFINED_SOURCE,
    );
    let string = extension(&root, "ext-string", &[TOPIC_BEFORE_SEND], "(p) => 'nope'");
    edit_before_boot(&root, |document| {
        remove_entry(document, GREEN_ID);
        push_entry(document, undefined);
        push_entry(document, string);
    });
    let daemon = Daemon::boot_operator(binary, &root);
    daemon.await_ready();
    for id in ["ext-undefined", "ext-string"] {
        daemon.eventually(&format!("{id} to settle"), || settled(&daemon, id));
    }
    let baseline = last_seq(&daemon);
    let answer = send(port);
    assert_eq!(answer.status, 200, "{}", answer.raw);
    assert_eq!(
        raw_body(&answer),
        SEND_BODY,
        "undefined passes the payload unchanged"
    );
    daemon.eventually("the walk on the ledger", || {
        !traces(&daemon, baseline).is_empty()
    });
    let trace = one_trace(&daemon, baseline, TOPIC_BEFORE_SEND);
    assert_eq!(trace["listeners"], 2);
    assert_eq!(
        trace["failures"], 1,
        "the string-returning source is the one failure"
    );
    println!(
        "proof 4 (second half): undefined = pass-through, a string = contained failure; {trace}"
    );
    daemon.interrupt();
}

/// Proof 5: a moment posted inside an extension's restart window is
/// refused typed `restarting` and NOTHING is answered unmodified — the
/// card's intended assertion, answered at pin `138fdce` (jinnd M2-K26:
/// a `listen` registration survives its instance's suspension as a
/// refusing registration until the replacement commits atomically, so a
/// reply-expecting walk in the window selects it and is refused whole;
/// FINDINGS.md #47). Found NOT-YET in UI-2 round 1 at `a53a352`: the
/// kernel withdrew the listen with the old incarnation's suspension
/// BEFORE the replacement committed, so a walk in the window selected
/// nobody and answered its own payload (53 unvalidated sends in one
/// edit). Both halves are asserted now: the transport's (every refusal
/// typed, naming `restarting`) and the kernel's (zero unmodified answers,
/// zero `listeners: 0` walks, one `DispatchRefused` row per refused
/// send); the window is still measured on the record and printed.
#[test]
fn a_restarting_extension_refuses_the_moment_typed_and_nothing_is_sent() {
    let Some((daemon, port)) = booted("moments-restart", |_, _| {}) else {
        return;
    };
    let first = send(port);
    assert_eq!(first.body["text"], "hello 🟢", "{}", first.raw);
    let baseline = last_seq(&daemon);

    // The operator's edit: a new source whose ACTIVATION is slow by
    // construction (a bounded loop under fuel; ~3M iterations/s measured
    // at this pin, so ~1.3 s), through the profile document — the lane
    // the watcher serves, so the transport stays free to take moments
    // while the extension restarts. (Through `PATCH /v1/profile/entries`
    // the transport itself awaits the restart inside the patch's own
    // request, #26, and cannot take a moment until it lands.)
    let slow = slow_source(4_000_000, "v2");
    let edited = Instant::now();
    daemon.edit_profile(|document| {
        entry_mut(document, GREEN_ID)["config"]["data"]["source"] = serde_json::json!(slow);
    });
    // Every answer in the window, bucketed: refused typed (what the card
    // expected), the OLD fold (the edit not yet taken), UNMODIFIED (a walk
    // that selected no listener — fail-open), the NEW fold (landed).
    let mut refused: Vec<(Duration, String)> = Vec::new();
    let mut old = 0u32;
    let mut open: Vec<Duration> = Vec::new();
    let mut landed: Option<(Duration, Response)> = None;
    let deadline = edited + Duration::from_secs(60);
    while landed.is_none() {
        assert!(
            Instant::now() < deadline,
            "the restart to land\n{}",
            daemon.log()
        );
        let answer = send(port);
        let at = edited.elapsed();
        match (answer.status, answer.body["text"].as_str()) {
            (503, _) => refused.push((at, answer.body["error"].to_string())),
            (200, Some("hello v2")) => landed = Some((at, answer)),
            (200, Some("hello 🟢")) => old += 1,
            (200, Some("hello")) => open.push(at),
            (status, text) => panic!(
                "a moment during the restart answered {status} {text:?}: {}",
                answer.raw
            ),
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let (landed_at, landed) = landed.expect("landed");
    assert_eq!(landed.body["text"], "hello v2");
    // The transport's half: every refusal is typed and names its case.
    for (_, error) in &refused {
        assert!(
            error.contains("\"refusal\":\"restarting\"") && error.contains("restarting:"),
            "typed, naming restarting: {error}"
        );
        assert!(error.contains("\"unavailable\""), "{error}");
    }
    // The kernel's half, on the record: the walks in the window and what
    // each selected; the refusal rows, if any; the restart's own window.
    daemon.eventually("the landed walk on the ledger", || {
        traces(&daemon, baseline)
            .iter()
            .any(|(_, f)| f["listeners"] == 1)
    });
    let window_traces = traces(&daemon, baseline);
    let open_walks = window_traces
        .iter()
        .filter(|(_, fields)| fields["listeners"] == 0)
        .count();
    let refusal_rows: Vec<String> = daemon
        .ledger_rows()
        .iter()
        .filter(|row| row.seq > baseline && row.kind.contains("Refused"))
        .map(|row| row.kind.clone())
        .collect();
    let rows = ledger_ts(&daemon);
    let suspended = rows.iter().rev().find(|(_, entry, kind, _)| {
        entry.as_deref() == Some(GREEN_ID) && kind.contains("FiberSuspended")
    });
    let active = rows.iter().rev().find(|(_, entry, kind, _)| {
        entry.as_deref() == Some(GREEN_ID)
            && kind.contains(r#""to":"Active""#)
            && kind.contains("ConfigChanged")
    });
    let window_ms = active.zip(suspended).map(|(a, s)| a.3 - s.3);
    println!(
        "proof 5: after the edit — {old} answers with the OLD fold, {} REFUSED typed `restarting` (first at {:?}), {} answered the payload UNMODIFIED (fail-open; first at {:?}), the new fold landed at {landed_at:?}; walks with listeners=0 on the ledger: {open_walks}; refusal rows: {refusal_rows:?}; the old incarnation's suspension to the new one's Active: {window_ms:?} ms",
        refused.len(),
        refused.first().map(|(at, _)| at),
        open.len(),
        open.first()
    );
    // The kernel's half, asserted (M2-K26 (a)/(b)): the window was hit
    // and every send inside it was refused — none answered unmodified,
    // no walk selected nobody, and each refusal is a `DispatchRefused`
    // row on the record.
    assert!(
        !refused.is_empty(),
        "the window was hit: {old} old answers before it, landed at {landed_at:?}\n{}",
        daemon.log()
    );
    assert!(
        open.is_empty(),
        "zero moments answered UNMODIFIED inside the restart window (FINDINGS #47): {} were, first at {:?}",
        open.len(),
        open.first()
    );
    assert_eq!(
        open_walks, 0,
        "zero walks selected nobody inside the window (FINDINGS #47)"
    );
    let dispatch_refused = refusal_rows
        .iter()
        .filter(|kind| kind.contains("DispatchRefused"))
        .count();
    assert_eq!(
        dispatch_refused,
        refused.len(),
        "one DispatchRefused row per refused send: {refusal_rows:?}"
    );
    assert!(
        window_ms.is_some(),
        "the restart's window is on the record: a FiberSuspended and a ConfigChanged Active for {GREEN_ID}"
    );
    println!(
        "proof 5: every moment inside the window was refused typed `restarting`, none answered unmodified (FINDINGS #47 closed at this pin)"
    );
    daemon.interrupt();
}

#[test]
fn an_extension_is_granted_its_topic_and_nothing_else() {
    let Some(binary) = gate() else { return };
    let (root, port) = fresh_ui_root("moments-grants");
    // An entry whose data names the topic but whose grants do not.
    let mut ungranted = extension(&root, "ext-ungranted", &[TOPIC_BEFORE_SEND], GREEN_SOURCE);
    ungranted["config"]["grants"] = serde_json::json!([CLOCK_CONTRACT]);
    // An entry granted before-send only.
    let blue = extension(&root, BLUE_ID, &[TOPIC_BEFORE_SEND], BLUE_SOURCE);
    edit_before_boot(&root, |document| {
        remove_entry(document, GREEN_ID);
        push_entry(document, ungranted);
        push_entry(document, blue);
    });
    let daemon = Daemon::boot_operator(binary, &root);
    daemon.await_ready();
    for id in ["ext-ungranted", BLUE_ID] {
        daemon.eventually(&format!("{id} to settle"), || settled(&daemon, id));
    }
    assert_eq!(
        state(&get(port, &format!("/v1/plugins/{CATALOG}/ext-ungranted")).body),
        "failed",
        "a listen the kernel refuses fails the activation"
    );
    let kinds: Vec<String> = history(port, CATALOG, "ext-ungranted")["lines"]
        .as_array()
        .expect("lines")
        .iter()
        .map(|line| line["kind"].as_str().unwrap_or("").to_owned())
        .collect();
    assert!(
        kinds.iter().any(|kind| kind.contains("GrantRefused")),
        "GrantRefused on its history: {kinds:?}"
    );
    assert_eq!(
        state(&get(port, &format!("/v1/plugins/{CATALOG}/{BLUE_ID}")).body),
        "active"
    );

    // before-send: the ungranted one is no listener; blue folds.
    let baseline = last_seq(&daemon);
    let answer = send(port);
    assert_eq!(answer.status, 200, "{}", answer.raw);
    assert_eq!(answer.body["text"], "hello 🔵");
    daemon.eventually("the walk on the ledger", || {
        !traces(&daemon, baseline).is_empty()
    });
    let trace = one_trace(&daemon, baseline, TOPIC_BEFORE_SEND);
    assert_eq!(trace["listeners"], 1, "{trace}");

    // Second half: a valid before-create-session moment selects NO
    // listener — the payload selects by topic, and nothing else selects.
    let baseline = last_seq(&daemon);
    let spec = serde_json::json!({ "engine": { "engine": "echo" } });
    let answer = post(port, CREATE_PATH, &spec);
    assert_eq!(answer.status, 200, "{}", answer.raw);
    assert_eq!(answer.body, spec);
    daemon.eventually("the walk on the ledger", || {
        !traces(&daemon, baseline).is_empty()
    });
    let trace = one_trace(&daemon, baseline, TOPIC_BEFORE_CREATE_SESSION);
    assert_eq!(trace["listeners"], 0, "{trace}");
    println!("proof 6: an ungranted topic fails the fiber on the record; a granted one receives only its own topic");
    daemon.interrupt();
}

/// Proof 7, FLIPPED at pin `b1dbe8f` (jinnd M2-K25; harness pin-bump 8):
/// the #48 shape exactly — a `while (true) {}` source on
/// `jinn:ui/before-send`, plain `listen`, no budget — and the intended
/// assertion at last. The walk costs the LISTENER's guest deadline and
/// the transport is charged nothing (M2-K25 (a)): it answers the moment
/// with the payload unmodified and `failures: 1` folded out, its next
/// `GET /v1/health` answers within bound, its fiber has NO transition
/// and its incarnation is what it was. The looping extension is `failed`
/// on the record with its OWN row — the deadline named under its
/// attribution, then `Active → Unloading → Failed` under `BodyFaulted`
/// (M2-K25 (c); the fatal half of #51). R11 in both halves.
#[test]
fn a_looping_extension_costs_its_own_slot_and_not_the_transport() {
    let Some(binary) = gate() else { return };
    let (root, port) = fresh_ui_root("moments-loop");
    let looping = extension(&root, LOOPING_ID, &[TOPIC_BEFORE_SEND], LOOPING_SOURCE);
    edit_before_boot(&root, |document| {
        remove_entry(document, GREEN_ID);
        push_entry(document, looping);
    });
    let daemon = Daemon::boot_operator(binary, &root);
    daemon.await_ready();
    daemon.eventually(&format!("{LOOPING_ID} to settle"), || {
        settled(&daemon, LOOPING_ID)
    });
    let transport_before = get(port, &format!("/v1/plugins/{CATALOG}/{TRANSPORT}")).body;
    assert_eq!(state(&transport_before), "active");
    let baseline = last_seq(&daemon);

    let (elapsed, status, raw) =
        tolerant(port, "POST", SEND_PATH, SEND_BODY, Duration::from_secs(60));
    // The walk costs the listener's deadline — its bound under a plain
    // `listen` — and nothing beyond it: the emitter's clock was parked.
    assert!(
        elapsed >= GUEST_DEADLINE - Duration::from_millis(500)
            && elapsed < GUEST_DEADLINE + Duration::from_secs(5),
        "the walk costs the listener's guest deadline and no more: {elapsed:?}"
    );
    assert_eq!(
        status,
        Some(200),
        "the transport answered the moment: {raw}"
    );
    let body = raw.split_once("\r\n\r\n").map_or("", |(_, body)| body);
    assert_eq!(
        body, SEND_BODY,
        "the only listener failed contained, so the payload is answered unmodified"
    );
    daemon.eventually("the walk on the ledger", || {
        !traces(&daemon, baseline).is_empty()
    });
    let trace = one_trace(&daemon, baseline, TOPIC_BEFORE_SEND);
    assert_eq!(trace["listeners"], 1, "{trace}");
    assert_eq!(
        trace["failures"], 1,
        "one contained failure folded out: {trace}"
    );

    // The transport after the walk: answering within bound, no
    // transition, no deadline row, the same incarnation.
    let (health_elapsed, health_status, _) =
        tolerant(port, "GET", "/v1/health", "", Duration::from_secs(10));
    assert_eq!(health_status, Some(200), "GET /v1/health after the walk");
    assert!(
        health_elapsed < Duration::from_secs(2),
        "the transport answers within bound after the walk: {health_elapsed:?}"
    );
    daemon.eventually(&format!("{LOOPING_ID} to fail on the record"), || {
        state(&get(port, &format!("/v1/plugins/{CATALOG}/{LOOPING_ID}")).body) == "failed"
    });
    let rows = daemon.ledger_rows();
    let after = |id: &str| -> Vec<(u64, String, serde_json::Value)> {
        rows.iter()
            .filter(|row| row.seq > baseline && row.entry.as_deref() == Some(id))
            .map(|row| {
                let (name, fields) = kind_of(row);
                (row.seq, name, fields)
            })
            .collect()
    };
    let transport_rows = after(TRANSPORT);
    assert!(
        !transport_rows
            .iter()
            .any(|(_, name, _)| name == "FiberTransition"),
        "the transport's fiber has no transition: {transport_rows:?}"
    );
    let deadline_rows: Vec<(u64, Option<String>, String)> = rows
        .iter()
        .filter(|row| row.seq > baseline && row.kind.to_ascii_lowercase().contains("deadline"))
        .map(|row| (row.seq, row.entry.clone(), row.kind.clone()))
        .collect();
    assert!(
        !deadline_rows.is_empty()
            && deadline_rows
                .iter()
                .all(|(_, entry, _)| entry.as_deref() == Some(LOOPING_ID)),
        "every deadline row names the listener and never the transport: {deadline_rows:?}"
    );
    let transport_after = get(port, &format!("/v1/plugins/{CATALOG}/{TRANSPORT}")).body;
    assert_eq!(state(&transport_after), "active");
    assert_eq!(
        transport_after["incarnation"], transport_before["incarnation"],
        "the transport's incarnation is what it was"
    );

    // The listener: its own row, then failed under the new cause.
    let looping_rows = after(LOOPING_ID);
    let errors: Vec<String> = looping_rows
        .iter()
        .filter(|(_, name, _)| name == "ErrorRecorded")
        .map(|(_, _, fields)| fields["error"]["message"].as_str().unwrap_or("").to_owned())
        .collect();
    assert!(
        errors
            .iter()
            .any(|message| message == "guest exceeded its call deadline"),
        "the deadline is the listener's own row: {looping_rows:?}"
    );
    let transitions: Vec<(String, String, String)> = looping_rows
        .iter()
        .filter(|(_, name, _)| name == "FiberTransition")
        .map(|(_, _, fields)| {
            (
                fields["from"].as_str().unwrap_or("").to_owned(),
                fields["to"].as_str().unwrap_or("").to_owned(),
                fields["cause"].as_str().unwrap_or("").to_owned(),
            )
        })
        .collect();
    assert_eq!(
        transitions,
        [
            ("Active".to_owned(), "Unloading".to_owned(), "BodyFaulted".to_owned()),
            ("Unloading".to_owned(), "Failed".to_owned(), "BodyFaulted".to_owned()),
        ],
        "the listener's fiber fails its own cell under BodyFaulted, and rests there (R9): {looping_rows:?}"
    );
    println!(
        "proof 7: the looping walk took {elapsed:?} (guest deadline {GUEST_DEADLINE:?}); the moment answered {status:?} unmodified with {trace}\n  the transport after the walk: GET /v1/health {health_status:?} in {health_elapsed:?}, incarnation {:?} → {:?}, its rows: {transport_rows:?}\n  {LOOPING_ID} after the walk: errors {errors:?}, transitions {transitions:?}\n  deadline rows: {deadline_rows:?}\nproof 7: the transport survived the walk — a bad extension costs its own slot (FINDINGS #48 closed at this pin)",
        transport_before["incarnation"], transport_after["incarnation"]
    );
    daemon.interrupt();
}

/// Proof 7b (pin-bump 8, the K25 card's harness consequence 2): the
/// entry's `budget` — `config.data.budget: { fuel }`, the kernel's
/// `delivery-budget` record spelled on the entry — is translated by the
/// Boa provider into `events.listen-within`, so a delivery that burns
/// past it ends the LISTENER's instance deterministically and far under
/// the deadline, on its own row (`guest exhausted its delivery fuel
/// budget`); a budgeted fold under its budget folds exactly as before,
/// and the walk continues past the contained death (R9). Second half:
/// a zero budget is refused at `listen`, `invalid`, on the record,
/// attributed to the declaring entry — the provider translates it
/// faithfully and never clamps — and its siblings are untouched.
#[test]
fn an_extension_s_budget_is_a_listen_within_and_a_looping_delivery_ends_at_its_fuel() {
    let Some(binary) = gate() else { return };
    let (root, port) = fresh_ui_root("moments-budget");
    let green = budgeted(
        extension(&root, GREEN_ID, &[TOPIC_BEFORE_SEND], GREEN_SOURCE),
        GREEN_FUEL,
    );
    let looping = budgeted(
        extension(&root, LOOPING_ID, &[TOPIC_BEFORE_SEND], LOOPING_SOURCE),
        LOOPING_FUEL,
    );
    edit_before_boot(&root, |document| {
        remove_entry(document, GREEN_ID);
        push_entry(document, green);
        push_entry(document, looping);
    });
    let daemon = Daemon::boot_operator(binary, &root);
    daemon.await_ready();
    for id in [GREEN_ID, LOOPING_ID] {
        daemon.eventually(&format!("{id} to settle"), || settled(&daemon, id));
        let read = get(port, &format!("/v1/plugins/{CATALOG}/{id}"));
        assert_eq!(
            state(&read.body),
            "active",
            "a budget is accepted at activation: {}",
            read.raw
        );
    }
    // A budgeted registration is a listen on the record, the same rows.
    let mut expected: Vec<String> = BREADCRUMBS.iter().map(|s| (*s).to_owned()).collect();
    expected.push(source_breadcrumb(GREEN_SOURCE));
    expected.push(format!("listen {TOPIC_BEFORE_SEND}"));
    assert_eq!(labels(&daemon, GREEN_ID), expected);
    let baseline = last_seq(&daemon);

    let (elapsed, status, raw) =
        tolerant(port, "POST", SEND_PATH, SEND_BODY, Duration::from_secs(60));
    assert_eq!(
        status,
        Some(200),
        "the transport answered the moment: {raw}"
    );
    let body: serde_json::Value =
        serde_json::from_str(raw.split_once("\r\n\r\n").map_or("", |(_, body)| body))
            .expect("a JSON answer");
    assert_eq!(body["text"], "hello 🟢", "the budgeted fold folds: {raw}");
    assert!(
        elapsed < BUDGET_BOUND,
        "a budgeted death lands far under the deadline ({GUEST_DEADLINE:?}): {elapsed:?}"
    );
    daemon.eventually("the walk on the ledger", || {
        !traces(&daemon, baseline).is_empty()
    });
    let trace = one_trace(&daemon, baseline, TOPIC_BEFORE_SEND);
    assert_eq!(trace["listeners"], 2, "{trace}");
    assert_eq!(trace["failures"], 1, "{trace}");
    daemon.eventually(&format!("{LOOPING_ID} to fail on the record"), || {
        state(&get(port, &format!("/v1/plugins/{CATALOG}/{LOOPING_ID}")).body) == "failed"
    });
    let rows = daemon.ledger_rows();
    let looping_rows: Vec<(String, serde_json::Value)> = rows
        .iter()
        .filter(|row| row.seq > baseline && row.entry.as_deref() == Some(LOOPING_ID))
        .map(kind_of)
        .collect();
    let errors: Vec<String> = looping_rows
        .iter()
        .filter(|(name, _)| name == "ErrorRecorded")
        .map(|(_, fields)| fields["error"]["message"].as_str().unwrap_or("").to_owned())
        .collect();
    assert_eq!(
        errors,
        ["guest exhausted its delivery fuel budget"],
        "the budget's own message on the listener's own row: {looping_rows:?}"
    );
    assert!(
        looping_rows
            .iter()
            .any(|(name, fields)| name == "FiberTransition"
                && fields["to"] == "Failed"
                && fields["cause"] == "BodyFaulted"),
        "failed under BodyFaulted: {looping_rows:?}"
    );
    assert!(
        !rows.iter().any(|row| row.seq > baseline
            && row.entry.as_deref() == Some(TRANSPORT)
            && row.kind.contains("FiberTransition")),
        "the transport has no transition"
    );
    // The walk continued past the contained death, and the budgeted
    // survivor is unaffected: the next moment folds with one listener.
    let second = last_seq(&daemon);
    let answer = send(port);
    assert_eq!(answer.status, 200, "{}", answer.raw);
    assert_eq!(answer.body["text"], "hello 🟢");
    daemon.eventually("the second walk on the ledger", || {
        !traces(&daemon, second).is_empty()
    });
    let second_trace = one_trace(&daemon, second, TOPIC_BEFORE_SEND);
    assert_eq!(second_trace["listeners"], 1, "{second_trace}");
    assert_eq!(second_trace["failures"], 0, "{second_trace}");
    assert_eq!(
        state(&get(port, &format!("/v1/plugins/{CATALOG}/{GREEN_ID}")).body),
        "active"
    );
    println!(
        "proof 7b: budgeted walk took {elapsed:?} (fuel {LOOPING_FUEL} on the looping listener, {GREEN_FUEL} on the fold); {trace}; {LOOPING_ID} rows {looping_rows:?}; the next walk {second_trace}"
    );
    daemon.interrupt();

    // Second half: zero is refused at `listen`, typed, on the record,
    // attributed to the entry that declared it; ext-green (unbudgeted)
    // is untouched and the moment still folds.
    let (root, port) = fresh_ui_root("moments-budget-zero");
    let zero = budgeted(
        extension(&root, ZERO_ID, &[TOPIC_BEFORE_SEND], GREEN_SOURCE),
        0,
    );
    edit_before_boot(&root, |document| push_entry(document, zero));
    let daemon = Daemon::boot_operator(binary, &root);
    daemon.await_ready();
    for id in [GREEN_ID, ZERO_ID] {
        daemon.eventually(&format!("{id} to settle"), || settled(&daemon, id));
    }
    let zero_read = get(port, &format!("/v1/plugins/{CATALOG}/{ZERO_ID}"));
    assert_eq!(state(&zero_read.body), "failed", "{}", zero_read.raw);
    let zero_errors: Vec<String> = daemon
        .ledger_rows()
        .iter()
        .filter(|row| row.entry.as_deref() == Some(ZERO_ID))
        .map(kind_of)
        .filter(|(name, _)| name == "ErrorRecorded")
        .map(|(_, fields)| fields["error"]["message"].as_str().unwrap_or("").to_owned())
        .collect();
    assert!(
        zero_errors
            .iter()
            .any(|message| message == "delivery fuel budget must be non-zero"),
        "the kernel's refusal, on the declaring entry's own row: {zero_errors:?}"
    );
    let crumbs = labels(&daemon, ZERO_ID);
    assert!(
        crumbs.contains(&"js evaluated".to_owned())
            && !crumbs.iter().any(|label| label.starts_with("listen ")),
        "the refusal is the listen's, after the source evaluated: {crumbs:?}"
    );
    assert_eq!(
        state(&get(port, &format!("/v1/plugins/{CATALOG}/{GREEN_ID}")).body),
        "active"
    );
    let answer = send(port);
    assert_eq!(answer.status, 200, "{}", answer.raw);
    assert_eq!(answer.body["text"], "hello 🟢", "the sibling folds");
    println!(
        "proof 7b (second half): a zero budget is refused at listen — {ZERO_ID} failed with {zero_errors:?}; ext-green folds beside it"
    );
    daemon.interrupt();
}

#[test]
fn an_extension_boots_from_a_profile_and_a_syntax_error_is_a_failed_fiber() {
    let Some(binary) = gate() else { return };
    // Real composition: ext-green reaches Active through the pinned
    // daemon from the kit-written profile, with its breadcrumbs in
    // order and its attestation on the catalog row.
    let (root, port) = fresh_ui_root("moments-boot");
    let broken = extension(&root, "ext-broken", &[TOPIC_BEFORE_SEND], BROKEN_SOURCE);
    edit_before_boot(&root, |document| push_entry(document, broken));
    let booted_at = Instant::now();
    let daemon = Daemon::boot_operator(binary, &root);
    daemon.await_ready();
    for id in [GREEN_ID, "ext-broken"] {
        daemon.eventually(&format!("{id} to settle"), || settled(&daemon, id));
    }
    let green = get(port, &format!("/v1/plugins/{CATALOG}/{GREEN_ID}"));
    assert_eq!(state(&green.body), "active", "{}", green.raw);
    // The attestation is the catalog's STABLE reading of the entry: the
    // origin, and the source's digest (the breadcrumb the page renders,
    // never read off a sliding history window — §9.7 amendment 8(d)).
    assert_eq!(
        green.body["attestation"],
        serde_json::json!({ "origin": "human", "source": source_digest(GREEN_SOURCE) }),
        "{}",
        green.raw
    );
    let transport = get(port, &format!("/v1/plugins/{CATALOG}/{TRANSPORT}"));
    assert!(
        transport.body.get("attestation").is_none(),
        "no origin declared, no attestation field: {}",
        transport.raw
    );
    let mut expected: Vec<String> = BREADCRUMBS.iter().map(|s| (*s).to_owned()).collect();
    expected.push(source_breadcrumb(GREEN_SOURCE));
    expected.push(format!("listen {TOPIC_BEFORE_SEND}"));
    assert_eq!(labels(&daemon, GREEN_ID), expected);
    let loaded: Vec<String> = daemon
        .ledger_rows()
        .iter()
        .filter(|row| row.entry.as_deref() == Some(GREEN_ID) && row.kind.contains("ArtifactLoaded"))
        .map(|row| row.kind.clone())
        .collect();

    // The variant whose source does not parse: `failed`, the breadcrumbs
    // it wrote before failing, their withdrawal LIFO and clean.
    let broken = get(port, &format!("/v1/plugins/{CATALOG}/ext-broken"));
    assert_eq!(state(&broken.body), "failed", "{}", broken.raw);
    let crumbs = labels(&daemon, "ext-broken");
    assert!(
        crumbs.starts_with(&["activate entered".to_owned(), "config parsed".to_owned()]),
        "{crumbs:?}"
    );
    assert!(
        crumbs.contains(&"js context built".to_owned()),
        "the context builds before the source is read: {crumbs:?}"
    );
    assert!(
        !crumbs.iter().any(|label| label == "js evaluated"),
        "a syntax error never reaches `js evaluated`: {crumbs:?}"
    );
    let withdrawn: Vec<(String, bool)> = daemon
        .ledger_rows()
        .iter()
        .filter(|row| row.entry.as_deref() == Some("ext-broken"))
        .filter_map(|row| {
            let (name, fields) = kind_of(row);
            (name == "EffectWithdrawn").then(|| {
                (
                    fields["label"].as_str().unwrap_or("").to_owned(),
                    fields["clean"].as_bool().unwrap_or(false),
                )
            })
        })
        .collect();
    let mut lifo: Vec<String> = crumbs
        .iter()
        .filter(|label| !label.starts_with("jinn-ext-js-boa activation failed"))
        .cloned()
        .collect();
    lifo.reverse();
    lifo.insert(
        0,
        crumbs
            .iter()
            .find(|label| label.starts_with("jinn-ext-js-boa activation failed"))
            .cloned()
            .expect("the fault label"),
    );
    assert_eq!(
        withdrawn
            .iter()
            .map(|(label, _)| label.clone())
            .collect::<Vec<_>>(),
        lifo,
        "withdrawal is LIFO"
    );
    assert!(withdrawn.iter().all(|(_, clean)| *clean), "{withdrawn:?}");
    // The REASON is on the ledger only because the guest writes it there
    // itself before failing (#38's workaround, the transport's, copied):
    // a `Failed` transition with no such label beside it would mean the
    // one class the kernel owns — a trap or the deadline.
    let reason: Vec<String> = daemon
        .ledger_rows()
        .iter()
        .filter(|row| row.entry.as_deref() == Some("ext-broken"))
        .filter(|row| row.kind.contains("ErrorRecorded") || row.kind.contains("activation failed"))
        .map(|row| row.kind.clone())
        .collect();
    assert!(
        reason.iter().any(|kind| kind.contains("source:")),
        "the guest named its fault before failing: {reason:?}"
    );
    println!(
        "proof 8: ext-green active from the profile in {:?} (artifact rows {loaded:?}); ext-broken failed with crumbs {crumbs:?}, withdrawn LIFO clean; its REASON on the record: {reason:?} (#38 / KG-5)",
        booted_at.elapsed()
    );
    daemon.interrupt();
}

#[test]
fn a_moment_is_the_door_then_one_walk_and_nothing_else() {
    let Some((daemon, port)) = booted("moments-door", |_, _| {}) else {
        return;
    };
    let baseline = last_seq(&daemon);
    // 1: a moment with the bearer.
    let granted = send(port);
    assert_eq!(granted.status, 200, "{}", granted.raw);
    // 2: no bearer.
    let refused = request_as(port, "POST", SEND_PATH, Some(SEND_BODY), None);
    assert_eq!(refused.status, 401, "{}", refused.raw);
    // 3-7: paths the vocabulary does not name — 404, no dispatch — and
    // §9.4's probes: upper case, a `..` segment, a trailing slash.
    let misses = [
        "/v1/moments/ui/after-nothing",
        "/v1/moments/introspect/transitions",
        "/v1/moments/ui/../before-send",
        "/v1/moments/UI/before-send",
        "/v1/moments/ui/before-send/",
    ];
    for miss in misses {
        let answer = request(port, "POST", miss, Some(SEND_BODY));
        assert_eq!(answer.status, 404, "{miss}: {}", answer.raw);
    }
    // 8: a bearer that verifies but a body that is not JSON: 422, no
    // dispatch, the verify row present — the door is paid before the
    // schema.
    let invalid = request(port, "POST", SEND_PATH, Some("not json"));
    assert_eq!(invalid.status, 422, "{}", invalid.raw);
    // 9: a 256 KiB+ body is refused by the wire before any dispatch.
    let huge = format!(
        r#"{{"text":"{}","session-id":"s","attachments":[]}}"#,
        "x".repeat(256 * 1024 + 1)
    );
    let too_big = request(port, "POST", SEND_PATH, Some(&huge));
    assert!(
        (400..500).contains(&too_big.status),
        "the wire refuses the body: {}",
        too_big.raw
    );

    let segments = closed_segments(&daemon, baseline, 9);
    let verifies = |segment: &[LedgerRow]| {
        segment
            .iter()
            .filter(|row| is_call(row, AUTH_CONTRACT, OP_VERIFY))
            .count()
    };
    let walks = |segment: &[LedgerRow]| {
        segment
            .iter()
            .filter(|row| kind_of(row).0 == "DispatchTrace")
            .count()
    };
    let kinds = |segment: &[LedgerRow]| -> Vec<String> {
        segment.iter().map(|row| row.kind.clone()).collect()
    };
    // The moment: exactly one verify, then exactly one walk, and every
    // other row a transport row or the door's decision.
    let moment = &segments[0];
    assert_eq!(verifies(moment), 1, "{:#?}", kinds(moment));
    assert_eq!(walks(moment), 1, "{:#?}", kinds(moment));
    let verify_at = moment
        .iter()
        .position(|row| is_call(row, AUTH_CONTRACT, OP_VERIFY))
        .expect("verify");
    let walk_at = moment
        .iter()
        .position(|row| kind_of(row).0 == "DispatchTrace")
        .expect("walk");
    assert!(verify_at < walk_at, "the door, then the walk");
    for row in moment {
        let (name, fields) = kind_of(row);
        let door_resolve = name == "ContractResolved" && fields["contract"] == AUTH_CONTRACT;
        assert!(
            is_transport(row)
                || door_resolve
                || name == "AuthDecided"
                || name == "DispatchTrace"
                || is_call(row, AUTH_CONTRACT, OP_VERIFY),
            "nothing else on the moment's connection: {}",
            row.kind
        );
    }
    // No bearer: one verify, one refusal, no walk.
    assert_eq!(verifies(&segments[1]), 1);
    assert_eq!(walks(&segments[1]), 0, "{:#?}", kinds(&segments[1]));
    // The five misses and the invalid body: the door paid, no walk.
    for segment in &segments[2..8] {
        assert_eq!(verifies(segment), 1, "{:#?}", kinds(segment));
        assert_eq!(walks(segment), 0, "{:#?}", kinds(segment));
    }
    // The oversized body: refused by the wire — no verify, no walk.
    assert_eq!(verifies(&segments[8]), 0, "{:#?}", kinds(&segments[8]));
    assert_eq!(walks(&segments[8]), 0, "{:#?}", kinds(&segments[8]));
    // Across the window: exactly one walk in total.
    assert_eq!(traces(&daemon, baseline).len(), 1);
    println!(
        "proof 10: nine connections — one moment (verify then one walk), one 401, five 404s and one 422 with the door paid and no walk, one oversized body refused by the wire with no crossing at all (status {})",
        too_big.status
    );
    daemon.interrupt();
}

/// KG-6, answered at pin `138fdce` (jinnd M2-K26 (e); FINDINGS.md #49):
/// `events.emit` is covered by the grant of the topic's own name exactly
/// as `listen` is. The transport is the ONE first-party guest that emits
/// on `jinn:ui/*` (an extension is a listener by construction — its Boa
/// provider never calls `emit`), and its three topic grants were written
/// into the `ui` profile by UI-2 so it would READ as the kernel now
/// enforces. This boot STRIPS them and shows the walk is refused ON THE
/// RECORD: a `GrantRefused` row naming the topic, the transport's typed
/// `refused` answer, NO `DispatchTrace`, and the extension never ran.
/// The same emitter WITH its grant is proof 2's fold, unchanged.
#[test]
fn an_entry_emitting_off_its_topic_grant_is_refused_on_the_record() {
    let Some((daemon, port)) = booted("moments-kg6", |_, document| {
        let transport = entry_mut(document, TRANSPORT);
        let grants = transport["config"]["grants"]
            .as_array_mut()
            .expect("grants");
        let before = grants.len();
        grants.retain(|grant| !grant.as_str().is_some_and(|g| g.starts_with("jinn:ui/")));
        assert_eq!(
            grants.len(),
            before - 3,
            "the three topic grants were there"
        );
    }) else {
        return;
    };
    let baseline = last_seq(&daemon);
    let answer = send(port);
    // The ledger is read after its batch lands (the refusal row is the
    // kernel's, appended on the broker's clock, not the answer's).
    let grant_refusals = || -> Vec<(Option<String>, String)> {
        daemon
            .ledger_rows()
            .iter()
            .filter(|row| row.seq > baseline && row.kind.contains("GrantRefused"))
            .map(|row| (row.entry.clone(), row.kind.clone()))
            .collect()
    };
    daemon.eventually("the refusal (or a walk) on the ledger", || {
        !grant_refusals().is_empty() || !traces(&daemon, baseline).is_empty()
    });
    let refusals = grant_refusals();
    let walks = traces(&daemon, baseline).len();
    let extension_rows: Vec<String> = daemon
        .ledger_rows()
        .iter()
        .filter(|row| row.seq > baseline && row.entry.as_deref() == Some(GREEN_ID))
        .map(|row| row.kind.clone())
        .collect();
    println!(
        "KG-6: the transport with NO topic grant posted a moment — status {}, error {}, walks {walks}, GrantRefused rows {refusals:?}, the extension's rows after the send {extension_rows:?}",
        answer.status, answer.body["error"]
    );
    // The transport's half: the kernel's `grant-refused` is the seam's
    // typed `refused` class, never the unmodified payload.
    assert_eq!(answer.status, 502, "{}", answer.raw);
    assert_eq!(answer.body["error"]["code"], "refused", "{}", answer.raw);
    assert!(
        answer.body["error"]["detail"]
            .as_str()
            .is_some_and(|detail| detail.starts_with("emit refused:")),
        "{}",
        answer.raw
    );
    // The kernel's half, on the record (Law 1, Law 2): the broker's own
    // refusal row on the emitter, naming the topic as the grant it lacks;
    // no walk traced; the listener never selected, never run.
    assert_eq!(refusals.len(), 1, "one GrantRefused row: {refusals:?}");
    let (entry, kind) = &refusals[0];
    assert_eq!(
        entry.as_deref(),
        Some(TRANSPORT),
        "on the emitter's row: {kind}"
    );
    let row: serde_json::Value = serde_json::from_str(kind).expect("a JSON row");
    assert_eq!(
        row["GrantRefused"]["contract"], TOPIC_BEFORE_SEND,
        "naming the topic as the grant it lacks: {kind}"
    );
    assert_eq!(walks, 0, "no DispatchTrace for a refused emit");
    assert!(
        extension_rows.is_empty(),
        "the listener never ran: {extension_rows:?}"
    );
    println!(
        "KG-6: an emit without the topic's grant is refused on the record (FINDINGS #49 closed at this pin)"
    );
    daemon.interrupt();
}
