//! Driving one live daemon over one scratch root: kit build (cached),
//! profile edits (the operator lane — ticks ARE config edits, FINDINGS.md
//! #1), ledger reads, and honest process shutdown.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::daemon::workspace_root;

/// How long a composition observation may take before the test fails.
pub const DEADLINE: Duration = Duration::from_secs(30);

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
            .args(["--every-ms", "60000"])
            .current_dir(workspace_root())
            .status()
            .expect("cargo run -p cron-kit");
        assert!(status.success(), "the cron kit builds");
        root
    })
}

/// A fresh scratch root holding a copy of the shared kit.
#[must_use]
pub fn fresh_root(name: &str) -> PathBuf {
    let kit = shared_kit();
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
    stderr: PathBuf,
}

impl Daemon {
    /// Boots the pinned daemon binary over `root`.
    #[must_use]
    pub fn boot(binary: &Path, root: &Path) -> Self {
        let stderr = root.join("daemon.stderr");
        let sink = std::fs::File::create(&stderr).expect("stderr sink");
        let child = Command::new(binary)
            .arg("--profile")
            .arg(root.join("profile.json"))
            .arg("--ledger")
            .arg(root.join("ledger.sqlite"))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(sink)
            .spawn()
            .expect("daemon spawns");
        Self {
            child,
            root: root.to_path_buf(),
            stderr,
        }
    }

    /// The daemon's stderr so far (its operator-facing log), ANSI styling
    /// stripped: the daemon colors unconditionally, and the escape codes
    /// sit between a field's name, `=`, and value — a raw substring match
    /// across them can never hold.
    #[must_use]
    pub fn log(&self) -> String {
        let raw = std::fs::read_to_string(&self.stderr).unwrap_or_default();
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
        self.root.join("data").join(path)
    }

    /// Reads a JSON file under the data root, `None` until it exists and
    /// parses.
    #[must_use]
    pub fn data_json(&self, path: &str) -> Option<serde_json::Value> {
        let bytes = std::fs::read(self.data(path)).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    /// Edits the profile document in place (the operator lane the watcher
    /// serves).
    pub fn edit_profile(&self, edit: impl FnOnce(&mut serde_json::Value)) {
        let path = self.root.join("profile.json");
        let mut document: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).expect("profile readable"))
                .expect("profile parses");
        edit(&mut document);
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&document).expect("profile encodes"),
        )
        .expect("profile writes");
    }

    /// One tick: rewrites the tick entry's config (FINDINGS.md #1 — the
    /// timer stand-in's push path; `cron-kit tick` is the duty-loop twin of
    /// this edit), then waits for the watcher to restart the tick fiber.
    /// An edit that lands before the watcher is armed (the boot window) is
    /// rewritten until it is seen — the duty driver gets the same
    /// resilience from its next interval.
    pub fn tick(&self, seq: u64, now_ms: u64) {
        let restarts = |log: &str| log.matches(r#"restarted=[EntryId("cron-tick")]"#).count();
        let before = restarts(&self.log());
        let deadline = Instant::now() + DEADLINE;
        loop {
            self.edit_profile(|document| {
                entry_config(document, "cron-tick")["data"] =
                    serde_json::json!({ "seq": seq, "now-ms": now_ms });
            });
            let retry = Instant::now() + Duration::from_secs(2);
            while Instant::now() < retry {
                if restarts(&self.log()) > before {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            assert!(
                Instant::now() < deadline,
                "timed out landing tick {seq}\n--- daemon log ---\n{}",
                self.log()
            );
        }
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

    /// Interrupts the daemon (the operator's Ctrl-C) and waits for a clean
    /// exit.
    pub fn interrupt(mut self) {
        let status = Command::new("kill")
            .args(["-INT", &self.child.id().to_string()])
            .status()
            .expect("kill -INT");
        assert!(status.success(), "SIGINT delivered");
        let deadline = Instant::now() + DEADLINE;
        loop {
            match self.child.try_wait().expect("daemon wait") {
                Some(status) => {
                    assert!(status.success(), "clean shutdown\n{}", self.log());
                    return;
                }
                None if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                None => {
                    let _ = self.child.kill();
                    panic!("daemon ignored SIGINT\n{}", self.log());
                }
            }
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
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
