//! Driving one live daemon over one scratch root: kit build (cached),
//! profile edits (the operator lane), ledger reads, and honest process
//! shutdown. Time is the kernel's own (`jinn:clock`): the suite observes
//! real fires on a fast kit rather than injecting instants.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::daemon::workspace_root;

/// How long a composition observation may take before the test fails.
pub const DEADLINE: Duration = Duration::from_secs(60);

/// How long a BOOT may take. Separate from [`DEADLINE`] because it scales
/// with the composition, not with the observation: a profile's every
/// component is compiled and instantiated before the readiness line, and
/// the suite runs its roots in parallel — the engines profile is eleven
/// components per daemon, eight daemons at once, against a debug-built
/// runtime. A boot that is merely SLOW is not a defect, and a deadline
/// that calls it one turns a loaded machine into a red suite.
pub const BOOT_DEADLINE: Duration = Duration::from_secs(240);

/// The suite kit's job period: boundaries every 2 s, so a fire is never
/// more than one period away and a restart gap of a few seconds spans
/// several boundaries.
pub const JOB_PERIOD_MS: u64 = 2_000;

/// The suite kit's alarm period (`tick-ms`): coarser than the kernel's
/// default 250 ms floor, fine enough that a fire lands within half a
/// second of its boundary.
pub const TICK_MS: u64 = 500;

/// Builds the cron kit once per process into a shared cache; tests copy it
/// into per-test roots. Panics on build failure — the kit building is part
/// of what the gate proves.
pub fn shared_kit() -> &'static Path {
    static KIT: OnceLock<PathBuf> = OnceLock::new();
    KIT.get_or_init(|| {
        let root = workspace_root().join("target/composition/kit");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("kit cache dir");
        let status = Command::new("cargo")
            .args(["run", "-p", "cron-kit", "--", "kit"])
            .arg(&root)
            .args(["--every-ms", &JOB_PERIOD_MS.to_string()])
            .args(["--tick-ms", &TICK_MS.to_string()])
            .current_dir(workspace_root())
            .status()
            .expect("cargo run -p cron-kit");
        assert!(status.success(), "the cron kit builds");
        root
    })
}

/// The port the shared api kit is generated with; each test root rewrites
/// it to a port of its own (see [`fresh_api_root`]).
const KIT_PORT: u16 = 7911;

/// Builds the operator-API kit (the api trio beside the cron seam) once
/// per process into a shared cache.
pub fn shared_api_kit() -> &'static Path {
    static KIT: OnceLock<PathBuf> = OnceLock::new();
    KIT.get_or_init(|| {
        let root = workspace_root().join("target/composition/api-kit");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("kit cache dir");
        let status = Command::new("cargo")
            .args(["run", "-p", "api-kit", "--", "kit"])
            .arg(&root)
            .args(["--port", &KIT_PORT.to_string()])
            .args(["--every-ms", &JOB_PERIOD_MS.to_string()])
            .args(["--tick-ms", &TICK_MS.to_string()])
            .current_dir(workspace_root())
            .status()
            .expect("cargo run -p api-kit");
        assert!(status.success(), "the api kit builds");
        root
    })
}

/// Builds the ENGINES kit (the engine providers and the probe beside the
/// api trio, the settings pair and the cron seam) once per process. The
/// probe's cadence is the suite's job period, so a probe run lands inside
/// a test's deadline.
pub fn shared_engine_kit() -> &'static Path {
    static KIT: OnceLock<PathBuf> = OnceLock::new();
    KIT.get_or_init(|| {
        let root = workspace_root().join("target/composition/engine-kit");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("kit cache dir");
        let status = Command::new("cargo")
            .args(["run", "-p", "engine-kit", "--", "kit"])
            .arg(&root)
            .args(["--port", &KIT_PORT.to_string()])
            .args(["--probe-every-ms", &JOB_PERIOD_MS.to_string()])
            .args(["--every-ms", &JOB_PERIOD_MS.to_string()])
            .args(["--tick-ms", &TICK_MS.to_string()])
            .current_dir(workspace_root())
            .status()
            .expect("cargo run -p engine-kit");
        assert!(status.success(), "the engine kit builds");
        root
    })
}

/// Builds the SESSIONS kit (the two store providers beside the engine
/// providers, the api trio, the settings pair and the cron seam) once per
/// process. The poll period is the store's, and it is short so a turn
/// settles inside a test's deadline.
pub fn shared_session_kit() -> &'static Path {
    static KIT: OnceLock<PathBuf> = OnceLock::new();
    KIT.get_or_init(|| {
        let root = workspace_root().join("target/composition/session-kit");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("kit cache dir");
        let status = Command::new("cargo")
            .args(["run", "-p", "session-kit", "--", "kit"])
            .arg(&root)
            .args(["--port", &KIT_PORT.to_string()])
            .args(["--poll-ms", &SESSION_POLL_MS.to_string()])
            .args(["--probe-every-ms", &JOB_PERIOD_MS.to_string()])
            .args(["--every-ms", &JOB_PERIOD_MS.to_string()])
            .args(["--tick-ms", &TICK_MS.to_string()])
            .current_dir(workspace_root())
            .status()
            .expect("cargo run -p session-kit");
        assert!(status.success(), "the session kit builds");
        root
    })
}

/// Builds the TODOS kit (the two Todo stores over the two session stores,
/// over the engine providers, with the api trio, the settings pair and
/// the cron seam) once per process. One poll period serves both store
/// seams: a Todo's dispatch polls a session, which polls its engine.
pub fn shared_todo_kit() -> &'static Path {
    static KIT: OnceLock<PathBuf> = OnceLock::new();
    KIT.get_or_init(|| {
        let root = workspace_root().join("target/composition/todo-kit");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("kit cache dir");
        let status = Command::new("cargo")
            .args(["run", "-p", "todo-kit", "--", "kit"])
            .arg(&root)
            .args(["--port", &KIT_PORT.to_string()])
            .args(["--poll-ms", &SESSION_POLL_MS.to_string()])
            .args(["--probe-every-ms", &JOB_PERIOD_MS.to_string()])
            .args(["--every-ms", &JOB_PERIOD_MS.to_string()])
            .args(["--tick-ms", &TICK_MS.to_string()])
            .current_dir(workspace_root())
            .status()
            .expect("cargo run -p todo-kit");
        assert!(status.success(), "the todo kit builds");
        root
    })
}

/// Builds the WORKFLOWS kit (the two run stores over the two Todo stores,
/// over the two session stores, over the engine providers, with the api
/// trio, the settings pair and the cron seam) once per process. One poll
/// period serves all three store seams: a run's node polls a Todo, whose
/// dispatch polls a session, which polls its engine.
pub fn shared_workflow_kit() -> &'static Path {
    static KIT: OnceLock<PathBuf> = OnceLock::new();
    KIT.get_or_init(|| {
        let root = workspace_root().join("target/composition/workflow-kit");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("kit cache dir");
        let status = Command::new("cargo")
            .args(["run", "-p", "workflow-kit", "--", "kit"])
            .arg(&root)
            .args(["--port", &KIT_PORT.to_string()])
            .args(["--poll-ms", &SESSION_POLL_MS.to_string()])
            .args(["--probe-every-ms", &JOB_PERIOD_MS.to_string()])
            .args(["--every-ms", &JOB_PERIOD_MS.to_string()])
            .args(["--tick-ms", &TICK_MS.to_string()])
            .current_dir(workspace_root())
            .status()
            .expect("cargo run -p workflow-kit");
        assert!(status.success(), "the workflow kit builds");
        root
    })
}

/// A fresh scratch root holding a copy of the shared WORKFLOW kit, its
/// HTTP provider re-pointed at a free loopback port. A restart proof
/// re-boots over the SAME root, so the port stays with it.
/// Builds the plugins kit (the two catalog providers beside the api trio)
/// once per process into a shared cache.
///
/// # Panics
///
/// If the kit builder fails.
pub fn shared_plugins_kit() -> &'static Path {
    static KIT: OnceLock<PathBuf> = OnceLock::new();
    KIT.get_or_init(|| {
        let root = workspace_root().join("target/composition/plugin-kit");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("kit cache dir");
        let status = Command::new("cargo")
            .args(["run", "-p", "plugin-kit", "--", "kit"])
            .arg(&root)
            .args(["--port", &KIT_PORT.to_string()])
            .args(["--every-ms", &JOB_PERIOD_MS.to_string()])
            .args(["--tick-ms", &TICK_MS.to_string()])
            .current_dir(workspace_root())
            .status()
            .expect("cargo run -p plugin-kit");
        assert!(status.success(), "the plugin kit builds");
        root
    })
}

/// Builds the UI kit (the web client archived into the embedded bundle
/// provider, mounted beside the api trio, the settings pair, the plugins
/// catalogs and the cron seam) once per process, plus the two variants
/// the swap and fail-closed proofs mount: a MARKED document and a
/// CORRUPTED blob. Runs the web build, so `pnpm` must be on `PATH`.
///
/// # Panics
///
/// If the kit builder fails.
pub fn shared_ui_kit() -> &'static Path {
    static KIT: OnceLock<PathBuf> = OnceLock::new();
    KIT.get_or_init(|| {
        let root = workspace_root().join("target/composition/ui-kit");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("kit cache dir");
        let status = Command::new("cargo")
            .args(["run", "-p", "ui-kit", "--", "kit"])
            .arg(&root)
            .args(["--port", &KIT_PORT.to_string()])
            .args(["--every-ms", &JOB_PERIOD_MS.to_string()])
            .args(["--tick-ms", &TICK_MS.to_string()])
            .current_dir(workspace_root())
            .status()
            .expect("cargo run -p ui-kit");
        assert!(status.success(), "the ui kit builds");
        for extra in [
            vec!["--name", UI_MARKED, "--marker", UI_MARKER],
            vec!["--name", UI_CORRUPT, "--corrupt"],
        ] {
            let status = Command::new("cargo")
                .args(["run", "-p", "ui-kit", "--", "variant"])
                .arg(&root)
                .args(&extra)
                .current_dir(workspace_root())
                .status()
                .expect("cargo run -p ui-kit -- variant");
            assert!(status.success(), "the ui kit variant {extra:?} builds");
        }
        root
    })
}

/// The marked variant's artifact name (proof 4) and its marker text.
pub const UI_MARKED: &str = "jinn-ui-bundle-marked";
/// See [`UI_MARKED`].
pub const UI_MARKER: &str = "second-bundle-of-the-swap-proof";
/// The corrupted variant's artifact name (proof 5).
pub const UI_CORRUPT: &str = "jinn-ui-bundle-corrupt";

/// A fresh copy of the UI kit on a free port.
///
/// # Panics
///
/// If the copy or the profile rewrite fails.
#[must_use]
pub fn fresh_ui_root(name: &str) -> (PathBuf, u16) {
    let root = copy_kit(shared_ui_kit(), name);
    let port = free_port();
    let path = root.join("profile.json");
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("profile")).expect("profile parses");
    set_provider_port(&mut document, "jinn-api-http", port);
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&document).expect("encodes"),
    )
    .expect("profile");
    (root, port)
}

/// A fresh copy of the plugins kit on a free port.
///
/// # Panics
///
/// If the copy or the profile rewrite fails.
#[must_use]
pub fn fresh_plugins_root(name: &str) -> (PathBuf, u16) {
    let root = copy_kit(shared_plugins_kit(), name);
    let port = free_port();
    let path = root.join("profile.json");
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("profile")).expect("profile parses");
    set_provider_port(&mut document, "jinn-api-http", port);
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&document).expect("encodes"),
    )
    .expect("profile");
    (root, port)
}

#[must_use]
pub fn fresh_workflow_root(name: &str) -> (PathBuf, u16) {
    let root = copy_kit(shared_workflow_kit(), name);
    let port = free_port();
    let path = root.join("profile.json");
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("profile")).expect("profile parses");
    set_provider_port(&mut document, "jinn-api-http", port);
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&document).expect("encodes"),
    )
    .expect("profile");
    (root, port)
}

/// A fresh scratch root holding a copy of the shared TODO kit, its HTTP
/// provider re-pointed at a free loopback port. A restart proof re-boots
/// over the SAME root, so the port stays with it.
#[must_use]
pub fn fresh_todo_root(name: &str) -> (PathBuf, u16) {
    let root = copy_kit(shared_todo_kit(), name);
    let port = free_port();
    let path = root.join("profile.json");
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("profile")).expect("profile parses");
    set_provider_port(&mut document, "jinn-api-http", port);
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&document).expect("encodes"),
    )
    .expect("profile");
    (root, port)
}

/// The store poll period the suite kits with: fine enough that a turn
/// settles well inside a test's deadline, no finer than the kernel's own
/// 250 ms alarm-resolution floor for a bare `jinn:clock` grant — asking
/// for less would not make the answer arrive sooner.
pub const SESSION_POLL_MS: u64 = 250;

/// A fresh scratch root holding a copy of the shared SESSION kit, its
/// HTTP provider re-pointed at a free loopback port. Answers the root and
/// that port, as [`fresh_api_root`] does. A restart proof re-boots over
/// the SAME root, so the port stays with it.
#[must_use]
pub fn fresh_session_root(name: &str) -> (PathBuf, u16) {
    let root = copy_kit(shared_session_kit(), name);
    let port = free_port();
    let path = root.join("profile.json");
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("profile")).expect("profile parses");
    set_provider_port(&mut document, "jinn-api-http", port);
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&document).expect("encodes"),
    )
    .expect("profile");
    (root, port)
}

/// A fresh scratch root holding a copy of the shared engine kit, its HTTP
/// provider re-pointed at a free loopback port. Answers the root and that
/// port, as [`fresh_api_root`] does.
#[must_use]
pub fn fresh_engine_root(name: &str) -> (PathBuf, u16) {
    let root = copy_kit(shared_engine_kit(), name);
    let port = free_port();
    let path = root.join("profile.json");
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("profile")).expect("profile parses");
    set_provider_port(&mut document, "jinn-api-http", port);
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&document).expect("encodes"),
    )
    .expect("profile");
    (root, port)
}

/// The entry with `id` in a profile document, whole (not just its config)
/// — the switch proof edits an entry's `package` and `hash`, which sit
/// outside `config`.
pub fn entry_mut<'doc>(
    document: &'doc mut serde_json::Value,
    id: &str,
) -> &'doc mut serde_json::Value {
    document["entries"]
        .as_array_mut()
        .expect("entries array")
        .iter_mut()
        .find(|entry| entry["id"] == id)
        .unwrap_or_else(|| panic!("profile has entry {id:?}"))
}

/// One artifact's content hash, from the sidecar the kit wrote beside it
/// — the pin a profile edit must carry when it swaps an entry's package
/// (kernel Law 5: a profile pins plugins by content hash).
#[must_use]
pub fn artifact_hash(root: &Path, package: &str) -> String {
    let sidecar = root.join(format!("artifacts/{package}.wasm.sha256"));
    std::fs::read_to_string(&sidecar)
        .unwrap_or_else(|error| panic!("{}: {error}", sidecar.display()))
        .trim()
        .to_owned()
}

/// A fresh scratch root holding a copy of the shared cron kit.
#[must_use]
pub fn fresh_root(name: &str) -> PathBuf {
    copy_kit(shared_kit(), name)
}

/// A fresh scratch root holding a copy of the shared api kit, its HTTP
/// provider re-pointed at a free loopback port (the grant's bind range
/// and the provider's `port` setting move together — one authority
/// decision). Answers the root and that port.
#[must_use]
pub fn fresh_api_root(name: &str) -> (PathBuf, u16) {
    let root = copy_kit(shared_api_kit(), name);
    let port = free_port();
    let path = root.join("profile.json");
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("profile")).expect("profile parses");
    set_provider_port(&mut document, "jinn-api-http", port);
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&document).expect("encodes"),
    )
    .expect("profile");
    (root, port)
}

/// Points the named HTTP provider entry at `port`: its `jinn:net` grant
/// scope and its `port` setting.
pub fn set_provider_port(document: &mut serde_json::Value, id: &str, port: u16) {
    let config = entry_config(document, id);
    config["data"]["port"] = serde_json::json!(port);
    for grant in config["grants"].as_array_mut().expect("grants") {
        if grant["contract"] == "jinn:net" {
            grant["scope"]["bind"] = serde_json::json!([port, port]);
        }
    }
}

/// A loopback port nobody holds right now.
#[must_use]
pub fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|addr| addr.port())
        .expect("a free loopback port")
}

fn copy_kit(kit: &Path, name: &str) -> PathBuf {
    let root = workspace_root()
        .join("target/composition/runs")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let artifacts = root.join("artifacts");
    std::fs::create_dir_all(&artifacts).expect("run root");
    for entry in std::fs::read_dir(kit.join("artifacts")).expect("kit artifacts") {
        let entry = entry.expect("kit artifact entry");
        std::fs::copy(entry.path(), artifacts.join(entry.file_name())).expect("artifact copy");
    }
    std::fs::copy(kit.join("profile.json"), root.join("profile.json")).expect("profile copy");
    // The UI kit's archive and manifest ride along: the bundle proofs
    // read the manifest's hashes and the blob's size from them.
    for entry in std::fs::read_dir(kit).expect("kit root") {
        let entry = entry.expect("kit entry");
        let name = entry.file_name();
        if name.to_string_lossy().starts_with("ui-bundle") && entry.path().is_dir() {
            let target = root.join(&name);
            std::fs::create_dir_all(&target).expect("bundle dir");
            for file in std::fs::read_dir(entry.path()).expect("bundle dir") {
                let file = file.expect("bundle file");
                std::fs::copy(file.path(), target.join(file.file_name())).expect("bundle copy");
            }
        }
    }
    root
}

/// One live daemon over one root.
pub struct Daemon {
    /// Held for the daemon's life; see [`daemon_budget`].
    _permit: DaemonPermit,
    child: Child,
    pub root: PathBuf,
    /// The daemon's data root (`jinn:fs`'s root): `<root>/data` for the
    /// cron layout, `<root>` itself for the operator layout.
    pub data_root: PathBuf,
    stderr: PathBuf,
}

/// The keystore's master-key source for every daemon this suite boots
/// (pin `3fd7b05`, jinnd M2-K8). A macOS daemon with NO passphrase
/// configured falls to the platform keychain, whose ACL can put an OS
/// authorization PROMPT in front of the first `put` — an automated suite
/// would hang on it with no operator present. The kernel's own packet
/// record says the same: the keychain backend is compiled but untested,
/// and a headless daemon sets the passphrase. So the suite sets one, and
/// it is a test constant with no meaning outside these scratch roots —
/// every run's store is a fresh directory that the run deletes.
pub const KEYSTORE_PASSPHRASE_VAR: &str = "JINND_KEYSTORE_PASSPHRASE";
/// See [`KEYSTORE_PASSPHRASE_VAR`]. A neutral placeholder, never a secret.
pub const KEYSTORE_PASSPHRASE: &str = "composition-suite-scratch-passphrase";

/// How many daemons this suite lets run AT ONCE, across every test
/// binary. `cargo test` sizes its thread pool for CPU-bound unit tests
/// AND runs each test binary as its own process in parallel; a
/// composition test is neither — each holds a live daemon that compiles
/// and instantiates every component of its profile and then keeps a duty
/// cycle running, writing state through `jinn:fs`, which since pin
/// `3fd7b05` fsyncs on every commit (FINDINGS.md #22). A dozen of those
/// on one box do not fail faster, they fail *falsely*: requests miss
/// their bounds and boots miss theirs, and a green suite becomes a
/// question about the machine's mood rather than about the kernel.
///
/// So the suite bounds itself, and it does it ACROSS PROCESSES — an
/// in-process semaphore would be multiplied by the number of test
/// binaries, which is exactly the case that bites. Tests still run in
/// parallel; only the live daemons are rationed.
fn daemon_budget() -> usize {
    std::thread::available_parallelism()
        .map(|cores| (cores.get() / 3).max(2))
        .unwrap_or(2)
}

/// Where the slots live. One file per slot, created exclusively.
fn slot_dir() -> PathBuf {
    workspace_root().join("target/composition/daemon-slots")
}

/// A slot a test binary that died without cleaning up may still hold; any
/// slot older than this is reclaimed, so a crashed run cannot wedge the
/// gate for the next one.
const SLOT_STALE_AFTER: Duration = Duration::from_secs(900);

/// The permit a live daemon holds. Released on drop, whether the test
/// passed, failed, or panicked.
struct DaemonPermit {
    slot: PathBuf,
}

impl DaemonPermit {
    fn acquire() -> Self {
        let dir = slot_dir();
        let _ = std::fs::create_dir_all(&dir);
        let budget = daemon_budget();
        loop {
            for index in 0..budget {
                let slot = dir.join(format!("slot-{index}"));
                if std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&slot)
                    .is_ok()
                {
                    return Self { slot };
                }
                let stale = std::fs::metadata(&slot)
                    .and_then(|meta| meta.modified())
                    .ok()
                    .and_then(|held| held.elapsed().ok())
                    .is_some_and(|age| age > SLOT_STALE_AFTER);
                if stale {
                    let _ = std::fs::remove_file(&slot);
                }
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    }
}

impl Drop for DaemonPermit {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.slot);
    }
}

/// Extra daemon permits, held for the duration of a test whose cost the
/// permit model does not capture. [`daemon_budget`] rations DAEMONS,
/// which is the right unit for every proof but one: the vendor-engine
/// proof also spawns two external LLM CLIs, and their cost lands on the
/// same box — on this suite's other daemons, and on the request bounds of
/// every OTHER test binary running beside it. Rationing them as if they
/// were free is what turns an unrelated suite's 45 s request bound into a
/// question about the machine's mood.
///
/// So that one test takes the REST of the budget while it runs. Acquired
/// before its own daemon boots, so it never waits on a slot it is itself
/// holding, and released on drop however the test ends.
pub struct ExtraDaemonLoad(
    /// Never read — held so the permits release on drop.
    #[allow(dead_code)]
    Vec<DaemonPermit>,
);

impl ExtraDaemonLoad {
    /// Takes every permit but one — the one the caller's own daemon is
    /// about to take.
    #[must_use]
    pub fn all_but_one() -> Self {
        Self(
            (1..daemon_budget())
                .map(|_| DaemonPermit::acquire())
                .collect(),
        )
    }
}

/// The `jinn:auth` credential of record for a data root: the kernel's own
/// rule, `<data>.operator-token` — a SIBLING of the data root, never
/// inside a guest's `jinn:fs` reach (the vendored bundle's metadata
/// §credential; `DaemonPaths::credential` in the pinned kernel spells it
/// as `data.with_extension("operator-token")`, and so does this).
#[must_use]
pub fn credential_path(data_root: &Path) -> PathBuf {
    data_root.with_extension("operator-token")
}

/// The credential every daemon THIS PROCESS boots is provisioned with:
/// 32 random bytes, hex-encoded, drawn once per test process. Random
/// rather than a constant so no value in this repo ever looks like a
/// secret; per process rather than per root so the suite's client can
/// present it without every call site threading a token through. Each
/// root is fresh and deleted with its run, and a test that needs another
/// value (wrong, rotated, revoked) writes its own.
#[must_use]
pub fn suite_credential() -> &'static str {
    static CREDENTIAL: OnceLock<String> = OnceLock::new();
    CREDENTIAL.get_or_init(|| {
        let mut bytes = [0u8; 32];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut source| {
                use std::io::Read as _;
                source.read_exact(&mut bytes)
            })
            .expect("32 random bytes from /dev/urandom");
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    })
}

/// The launcher's half of the door, for the daemons this suite boots:
/// writes the credential of record beside `data_root` IF ABSENT — the
/// suite's per-process value, mode 0600 — and leaves an existing file
/// alone (a restart over the same root keeps its credential, as the
/// soak's does). Returns the path. Mirrors what `tools/soak/
/// provision-token.sh` does for the soak launcher; the rig writes its own
/// because its roots are scratch (the packet's carve-out).
///
/// # Panics
///
/// If the file cannot be created with the required mode.
pub fn provision_credential(data_root: &Path) -> PathBuf {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let path = credential_path(data_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("credential parent");
    }
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)
    {
        Ok(mut file) => {
            file.write_all(suite_credential().as_bytes())
                .expect("credential written");
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => panic!("{}: {error}", path.display()),
    }
    path
}

/// Overwrites the credential of record beside `data_root` with `value`
/// (mode 0600, atomically: stage + rename) — the operator's ROTATION.
/// The kernel reads the file on every call, so the next request sees it.
///
/// # Panics
///
/// If the write fails.
pub fn rotate_credential(data_root: &Path, value: &str) {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;
    let path = credential_path(data_root);
    let staging = path.with_extension("operator-token.rotate-tmp");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&staging)
        .expect("credential staged");
    file.write_all(value.as_bytes())
        .expect("credential written");
    drop(file);
    std::fs::rename(&staging, &path).expect("credential replaced");
}

/// Deletes the credential of record beside `data_root` — the operator's
/// REVOCATION. Everything refuses from the next call on, no restart.
///
/// # Panics
///
/// If the file exists and cannot be removed.
pub fn revoke_credential(data_root: &Path) {
    let path = credential_path(data_root);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("{}: {error}", path.display()),
    }
}

/// The daemon's machine-readable readiness line (FINDINGS.md #12 minimum,
/// pin `9e61e47`): emitted on stderr once the watcher is armed AND the boot
/// reconcile is done — the operator lane keys on this, never on boot
/// evidence.
pub const READY: &str = r#""jinnd":"ready""#;

impl Daemon {
    /// Boots the pinned daemon binary over `root` with absolute paths.
    #[must_use]
    pub fn boot(binary: &Path, root: &Path) -> Self {
        let profile = root.join("profile.json");
        let ledger = root.join("ledger.sqlite");
        Self::spawn(
            binary,
            root,
            root,
            &root.join("data"),
            [
                OsStr::new("--profile"),
                profile.as_os_str(),
                OsStr::new("--ledger"),
                ledger.as_os_str(),
            ],
        )
    }

    /// Boots the daemon from `root` as its working directory with RELATIVE
    /// `--profile`/`--ledger` paths — the shape FINDINGS.md #18 was hit on.
    #[must_use]
    pub fn boot_relative(binary: &Path, root: &Path) -> Self {
        Self::spawn(
            binary,
            root,
            root,
            &root.join("data"),
            ["--profile", "profile.json", "--ledger", "ledger.sqlite"].map(OsStr::new),
        )
    }

    /// Boots the pinned daemon over `root` in the OPERATOR layout: the data
    /// root IS `root`, so the profile document sits inside the `jinn:fs`
    /// surface and the api seam's consumers reach it through their scoped
    /// grants (profiles/operator-api/README.md).
    #[must_use]
    pub fn boot_operator(binary: &Path, root: &Path) -> Self {
        let profile = root.join("profile.json");
        let ledger = root.join("ledger.sqlite");
        let artifacts = root.join("artifacts");
        Self::spawn(
            binary,
            root,
            root,
            root,
            [
                OsStr::new("--profile"),
                profile.as_os_str(),
                OsStr::new("--ledger"),
                ledger.as_os_str(),
                OsStr::new("--artifacts"),
                artifacts.as_os_str(),
                OsStr::new("--data"),
                root.as_os_str(),
            ],
        )
    }

    /// Spawns the daemon with `args` from `cwd`; its stderr lands under
    /// `root`. `data_root` is the daemon's data root as the args name it
    /// (or as the daemon will default it): the launcher's half of the door
    /// — the credential of record beside it — is provisioned HERE, before
    /// the process exists, if absent ([`provision_credential`]).
    #[must_use]
    pub fn spawn<'a>(
        binary: &Path,
        root: &Path,
        cwd: &Path,
        data_root: &Path,
        args: impl IntoIterator<Item = &'a OsStr>,
    ) -> Self {
        // Taken BEFORE the process exists, so the bound counts daemons
        // that are booting as well as daemons that are serving.
        let permit = DaemonPermit::acquire();
        provision_credential(data_root);
        let stderr = root.join("daemon.stderr");
        let sink = std::fs::File::create(&stderr).expect("stderr sink");
        let child = Command::new(binary)
            .args(args)
            .current_dir(cwd)
            .env(KEYSTORE_PASSPHRASE_VAR, KEYSTORE_PASSPHRASE)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(sink)
            .spawn()
            .expect("daemon spawns");
        Self {
            _permit: permit,
            child,
            root: root.to_path_buf(),
            data_root: data_root.to_path_buf(),
            stderr,
        }
    }

    /// Where this daemon's credential of record lives
    /// ([`credential_path`] of its data root).
    #[must_use]
    pub fn credential(&self) -> PathBuf {
        credential_path(&self.data_root)
    }

    /// Whether the readiness line has been emitted.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.log().contains(READY)
    }

    /// Waits for the readiness line — the ONLY gate the operator lane
    /// needs before its first edit (the watcher is armed, the boot
    /// reconcile is done). Bounded by [`BOOT_DEADLINE`], not
    /// [`DEADLINE`]: see that constant.
    pub fn await_ready(&self) {
        self.eventually_within("the readiness line", BOOT_DEADLINE, || self.is_ready());
    }

    /// The daemon's stderr so far (its operator-facing log), ANSI styling
    /// stripped (see [`strip_ansi`]).
    #[must_use]
    pub fn log(&self) -> String {
        strip_ansi(&std::fs::read_to_string(&self.stderr).unwrap_or_default())
    }

    /// Polls until `check` holds; panics with `what` (and the daemon log)
    /// after [`DEADLINE`].
    pub fn eventually(&self, what: &str, check: impl FnMut() -> bool) {
        self.eventually_within(what, DEADLINE, check);
    }

    /// [`Daemon::eventually`] with an explicit bound.
    pub fn eventually_within(&self, what: &str, bound: Duration, mut check: impl FnMut() -> bool) {
        let deadline = Instant::now() + bound;
        while !check() {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}\n--- daemon log ---\n{}",
                self.log()
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// A file under the daemon's data root.
    #[must_use]
    pub fn data(&self, path: &str) -> PathBuf {
        self.data_root.join(path)
    }

    /// Reads a JSON file under the data root, `None` until it exists and
    /// parses.
    #[must_use]
    pub fn data_json(&self, path: &str) -> Option<serde_json::Value> {
        let bytes = std::fs::read(self.data(path)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Edits the profile document (the operator lane the watcher serves)
    /// by ONE atomic replace — stage + rename, the duty driver's own write
    /// shape, so the suite also proves the watcher follows renames. One
    /// edit is enough since pin `9e61e47`: the daemon recognizes its own
    /// write-back by the bytes it wrote (FINDINGS.md #17 closed) and arms
    /// its watcher before the boot reconcile (#18, #12 minimum), so an
    /// edit is never swallowed and never unseen. The pre-`9e61e47`
    /// mitigation (`edit_profile_until`: rewrite with different bytes until
    /// the observation holds) is retired.
    pub fn edit_profile(&self, edit: impl FnOnce(&mut serde_json::Value)) {
        let path = self.root.join("profile.json");
        let mut document: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).expect("profile readable"))
                .expect("profile parses");
        edit(&mut document);
        let bytes = serde_json::to_vec_pretty(&document).expect("profile encodes");
        let staging = self.root.join("profile.json.edit-tmp");
        std::fs::write(&staging, bytes).expect("profile stages");
        std::fs::rename(&staging, &path).expect("profile replaces");
    }

    /// One edit, then waits for the daemon to report `id` restarted.
    pub fn edit_profile_restarting(&self, id: &str, edit: impl FnOnce(&mut serde_json::Value)) {
        let restarts_before = self.restart_count(id);
        self.edit_profile(edit);
        self.eventually(&format!("{id} to restart on its config edit"), || {
            self.restart_count(id) > restarts_before
        });
    }

    /// How many reconcile reports restarted `id`.
    #[must_use]
    pub fn restart_count(&self, id: &str) -> usize {
        self.log()
            .matches(&format!(r#"restarted=[EntryId("{id}")]"#))
            .count()
    }

    /// How many log lines carry `needle`.
    #[must_use]
    pub fn log_count(&self, needle: &str) -> usize {
        self.log().matches(needle).count()
    }

    /// The FINDINGS.md #17 signature: a `reconciled` line with EVERY list
    /// empty — the diff never ran because a delivery was mistaken for the
    /// daemon's own write-back echo. Since pin `9e61e47` an echo logs no
    /// `reconciled` line at all and an identical operator rewrite reports
    /// `unchanged=[…]`, so this count must stay zero.
    #[must_use]
    pub fn swallowed_reconciles(&self) -> usize {
        self.log_count("reconciled created=[] restarted=[] disposed=[] unchanged=[]")
    }

    /// Every ledger event's `kind` text, in sequence order (Law 2 — the
    /// ledger is the evidence). The connection is a plain WAL reader — a
    /// read-only handle cannot join the live daemon's WAL and would see a
    /// stale prefix of the ledger; this reader runs SELECTs only.
    #[must_use]
    pub fn ledger_kinds(&self) -> Vec<String> {
        let connection =
            rusqlite::Connection::open(self.root.join("ledger.sqlite")).expect("ledger opens");
        let mut select = connection
            .prepare("SELECT kind FROM events ORDER BY seq")
            .expect("ledger schema");
        let kinds = select
            .query_map([], |row| row.get::<_, String>(0))
            .expect("ledger reads")
            .collect::<Result<Vec<_>, _>>()
            .expect("ledger rows");
        kinds
    }

    /// Every ledger row as `(seq, entry, fiber, kind)` — the event's
    /// attribution (the `entry` column is filled since pin `57360cc`,
    /// FINDINGS.md #19 closed).
    #[must_use]
    pub fn ledger_rows(&self) -> Vec<LedgerRow> {
        ledger_rows_at(&self.root)
    }

    /// How many times `fiber` re-activated on a config change (the
    /// ledger's `FiberTransition … to Active, cause ConfigChanged`): the
    /// restart evidence for both the watcher lane and the kernel's own
    /// `jinn:profile` amendment (which logs no `reconciled` line).
    #[must_use]
    pub fn config_restarts(&self, fiber: u64) -> usize {
        self.ledger_rows()
            .iter()
            .filter(|row| row.fiber == Some(fiber))
            .filter(|row| {
                row.kind
                    .contains(r#""to":"Active","cause":"ConfigChanged""#)
            })
            .count()
    }

    /// How many ledger events carry `needle` in their `kind` text.
    #[must_use]
    pub fn ledger_count(&self, needle: &str) -> usize {
        self.ledger_kinds()
            .iter()
            .filter(|kind| kind.contains(needle))
            .count()
    }

    /// Interrupts the daemon (the operator's Ctrl-C) and waits for a clean
    /// exit — the planned-stop path: every fiber suspends (an in-flight
    /// handler is drained first, pin `9e61e47`), the kernel reaches
    /// quiescence and flushes the ledger.
    pub fn interrupt(self) {
        self.interrupt_when("now", |_| true);
    }

    /// Spins (1 ms) until `trigger` holds, then interrupts — for landing
    /// the SIGINT inside a window the daemon's own log announces.
    pub fn interrupt_when(mut self, what: &str, mut trigger: impl FnMut(&Self) -> bool) {
        let deadline = Instant::now() + DEADLINE;
        while !trigger(&self) {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what} before the SIGINT\n--- daemon log ---\n{}",
                self.log()
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let status = Command::new("kill")
            .args(["-INT", &self.child.id().to_string()])
            .status()
            .expect("kill -INT");
        assert!(status.success(), "SIGINT delivered");
        let status = self.wait_exit();
        assert!(status.success(), "clean shutdown\n{}", self.log());
    }

    /// Waits for the daemon to exit on its own (a refused start) and
    /// returns its status; a daemon still serving at [`DEADLINE`] is killed
    /// and the test fails.
    pub fn wait_exit(&mut self) -> std::process::ExitStatus {
        let deadline = Instant::now() + DEADLINE;
        loop {
            match self.child.try_wait().expect("daemon wait") {
                Some(status) => return status,
                None if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                None => {
                    let _ = self.child.kill();
                    panic!("daemon still serving\n{}", self.log());
                }
            }
        }
    }
}

impl Daemon {
    /// Kills the daemon outright (SIGKILL) and waits: the crash path.
    /// Since pin `4eb4a93` a clean stop SUSPENDS (FINDINGS.md #14 closed)
    /// and the two paths agree on the disk outcome; the crash path stays
    /// in the suite as that equivalence's other half.
    pub fn kill(mut self) {
        self.child.kill().expect("SIGKILL delivered");
        self.child.wait().expect("daemon reaped");
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// One ledger row: sequence, attributed entry and fiber (if any), and
/// the `kind` JSON text.
#[derive(Clone, Debug)]
pub struct LedgerRow {
    pub seq: u64,
    pub entry: Option<String>,
    pub fiber: Option<u64>,
    pub kind: String,
}

impl LedgerRow {
    /// The row's kind as `(name, fields)`: the one-key object the ledger
    /// writes for a struct variant, the bare string for a unit one.
    #[must_use]
    pub fn kind_of(&self) -> (String, serde_json::Value) {
        match serde_json::from_str::<serde_json::Value>(&self.kind) {
            Ok(serde_json::Value::Object(object)) if object.len() == 1 => {
                let (name, fields) = object.into_iter().next().expect("one key");
                (name, fields)
            }
            Ok(serde_json::Value::String(unit)) => (unit, serde_json::Value::Null),
            _ => (self.kind.clone(), serde_json::Value::Null),
        }
    }
}

/// Every ledger row of a root (live or stopped) in sequence order — a
/// plain WAL reader running SELECTs only (see [`Daemon::ledger_kinds`]).
#[must_use]
pub fn ledger_rows_at(root: &Path) -> Vec<LedgerRow> {
    let connection = rusqlite::Connection::open(root.join("ledger.sqlite")).expect("ledger opens");
    let mut select = connection
        .prepare("SELECT seq, entry, fiber, kind FROM events ORDER BY seq")
        .expect("ledger schema");
    let rows = select
        .query_map([], |row| {
            Ok(LedgerRow {
                seq: row.get(0)?,
                entry: row.get(1)?,
                fiber: row.get(2)?,
                kind: row.get(3)?,
            })
        })
        .expect("ledger reads")
        .collect::<Result<Vec<_>, _>>()
        .expect("ledger rows");
    rows
}

/// Strips ANSI styling from the daemon's log: the daemon colors
/// unconditionally, and the escape codes sit between a field's name, `=`,
/// and value — a raw substring match across them can never hold.
#[must_use]
pub fn strip_ansi(raw: &str) -> String {
    let mut text = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(next) = chars.next() {
        if next != '\u{1b}' {
            text.push(next);
            continue;
        }
        if chars.next() == Some('[') {
            // A CSI sequence: parameters, then one final byte in @..=~.
            for terminator in chars.by_ref() {
                if ('@'..='~').contains(&terminator) {
                    break;
                }
            }
        }
    }
    text
}

/// The `config` object of the named profile entry.
pub fn entry_config<'doc>(
    document: &'doc mut serde_json::Value,
    id: &str,
) -> &'doc mut serde_json::Value {
    let entries = document["entries"].as_array_mut().expect("entries array");
    let entry = entries
        .iter_mut()
        .find(|entry| entry["id"] == id)
        .unwrap_or_else(|| panic!("profile has entry {id:?}"));
    &mut entry["config"]
}
