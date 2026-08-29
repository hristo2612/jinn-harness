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
pub const DEADLINE: Duration = Duration::from_secs(30);

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
    root
}

/// One live daemon over one root.
pub struct Daemon {
    child: Child,
    pub root: PathBuf,
    /// The daemon's data root (`jinn:fs`'s root): `<root>/data` for the
    /// cron layout, `<root>` itself for the operator layout.
    pub data_root: PathBuf,
    stderr: PathBuf,
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
        let mut daemon = Self::spawn(
            binary,
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
        );
        daemon.data_root = root.to_path_buf();
        daemon
    }

    /// Spawns the daemon with `args` from `cwd`; its stderr lands under
    /// `root`.
    #[must_use]
    pub fn spawn<'a>(
        binary: &Path,
        root: &Path,
        cwd: &Path,
        args: impl IntoIterator<Item = &'a OsStr>,
    ) -> Self {
        let stderr = root.join("daemon.stderr");
        let sink = std::fs::File::create(&stderr).expect("stderr sink");
        let child = Command::new(binary)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(sink)
            .spawn()
            .expect("daemon spawns");
        Self {
            child,
            root: root.to_path_buf(),
            data_root: root.join("data"),
            stderr,
        }
    }

    /// Whether the readiness line has been emitted.
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.log().contains(READY)
    }

    /// Waits for the readiness line — the ONLY gate the operator lane
    /// needs before its first edit (the watcher is armed, the boot
    /// reconcile is done).
    pub fn await_ready(&self) {
        self.eventually("the readiness line", || self.is_ready());
    }

    /// The daemon's stderr so far (its operator-facing log), ANSI styling
    /// stripped (see [`strip_ansi`]).
    #[must_use]
    pub fn log(&self) -> String {
        strip_ansi(&std::fs::read_to_string(&self.stderr).unwrap_or_default())
    }

    /// Polls until `check` holds; panics with `what` (and the daemon log)
    /// after [`DEADLINE`].
    pub fn eventually(&self, what: &str, mut check: impl FnMut() -> bool) {
        let deadline = Instant::now() + DEADLINE;
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

    /// Every ledger row as `(seq, fiber, kind)` — the fiber column is the
    /// event's attribution (the `entry` column is empty at this pin).
    #[must_use]
    pub fn ledger_rows(&self) -> Vec<LedgerRow> {
        ledger_rows_at(&self.root)
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

/// One ledger row: sequence, attributed fiber (if any), and the `kind`
/// JSON text.
#[derive(Clone, Debug)]
pub struct LedgerRow {
    pub seq: u64,
    pub fiber: Option<u64>,
    pub kind: String,
}

/// Every ledger row of a root (live or stopped) in sequence order — a
/// plain WAL reader running SELECTs only (see [`Daemon::ledger_kinds`]).
#[must_use]
pub fn ledger_rows_at(root: &Path) -> Vec<LedgerRow> {
    let connection = rusqlite::Connection::open(root.join("ledger.sqlite")).expect("ledger opens");
    let mut select = connection
        .prepare("SELECT seq, fiber, kind FROM events ORDER BY seq")
        .expect("ledger schema");
    let rows = select
        .query_map([], |row| {
            Ok(LedgerRow {
                seq: row.get(0)?,
                fiber: row.get(1)?,
                kind: row.get(2)?,
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
