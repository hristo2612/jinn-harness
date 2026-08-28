//! The duty-loop tick driver: the operator-lane process that stands in for
//! the missing kernel timer capability (FINDINGS.md #1). Every interval it
//! rewrites the tick entry's config with the wall clock; the daemon's
//! watcher reconciles, the tick fiber restarts, and the fresh activation
//! emits the tick. Retired the day a kernel timer capability ships.

use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TICK_ENTRY: &str = "cron-tick";

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("wall clock after the epoch")
        .as_millis() as u64
}

/// One tick: read-modify-write of the profile document. The daemon may
/// also write the document back (bidirectional persistence); the driver
/// re-reads it every tick so an interleaved write-back at worst delays one
/// tick, never wedges the loop.
fn advance(profile: &Path) -> Result<u64, String> {
    let text = std::fs::read(profile).map_err(|error| format!("profile read: {error}"))?;
    let mut document: serde_json::Value =
        serde_json::from_slice(&text).map_err(|error| format!("profile parse: {error}"))?;
    let entries = document["entries"]
        .as_array_mut()
        .ok_or("profile has no entries array")?;
    let entry = entries
        .iter_mut()
        .find(|entry| entry["id"] == TICK_ENTRY)
        .ok_or_else(|| format!("profile has no {TICK_ENTRY:?} entry"))?;
    let seq = entry["config"]["data"]["seq"].as_u64().unwrap_or(0) + 1;
    entry["config"]["data"] = serde_json::json!({ "seq": seq, "now-ms": now_ms() });
    let rendered = serde_json::to_vec_pretty(&document).expect("profile encodes");
    std::fs::write(profile, rendered).map_err(|error| format!("profile write: {error}"))?;
    Ok(seq)
}

/// Drives ticks until `count` runs out (forever when `None`).
pub fn drive(profile: &Path, interval_s: u64, count: Option<u64>) {
    let mut remaining = count;
    loop {
        match advance(profile) {
            Ok(seq) => println!("tick {seq}"),
            // Transient (a torn read against a concurrent writer): the next
            // interval retries; the scheduler's firing law absorbs the gap.
            Err(error) => eprintln!("tick skipped: {error}"),
        }
        if let Some(left) = remaining.as_mut() {
            *left -= 1;
            if *left == 0 {
                return;
            }
        }
        std::thread::sleep(Duration::from_secs(interval_s));
    }
}
