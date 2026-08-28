# The cron soak — a week of real duty (phase 1.4, M2 acceptance)

The M2 acceptance run per the kernel roadmap: the cron slice doing its
production job for a week, on the pinned kernel, without touching the old
gateway's data. The seam under soak is `plugins/cron/` booted through the
real pinned `jinnd` daemon, driven by `cron-kit tick` (the timer stand-in,
FINDINGS.md #1) at a 15-minute cadence.

**Soak started:** 2026-08-28T04:28:59Z · kernel pin `a17df864` (the pin at
soak start; `KERNEL-PIN.md` owns the current pin) · harness `5c828c6` ·
job `health` every 900 000 ms, tick interval 900 s.

## Layout

Everything at runtime lives OUTSIDE the repo, under one root:

```sh
SOAK=${SOAK:-$HOME/.local/state/jinn-harness-soak}
```

| Path | What |
|---|---|
| `$SOAK/bin/` | `jinnd` (built from the pinned commit by the composition harness's git-archive build), `cron-kit` (release), `detach.py` (copy of `tools/soak/detach.py`) |
| `$SOAK/profile.json`, `$SOAK/artifacts/` | The generated kit (`cron-kit kit`) — never hand-edited |
| `$SOAK/ledger.sqlite` | The daemon's append-only ledger (the evidence surface) |
| `$SOAK/data/` | The daemon's data root: `cron/` (state, history, per-fire run records), `health/` (the consumer's reports) |
| `$SOAK/logs/` | `jinnd.log`, `tick.log`, `ops.log` (operator actions, one timestamped line each — restarts count toward the +7d audit) |
| `$SOAK/run/` | `jinnd.pid`, `tick.pid` |
| `$SOAK/meta.json` | Start timestamp + pins, written once at soak start |

## Setup (how the runtime root is stood up)

From the repo root, with a jinnd checkout holding the pinned commit
reachable (`KERNEL-PIN.md` Gate-2 lanes):

```sh
SOAK=${SOAK:-$HOME/.local/state/jinn-harness-soak}
mkdir -p "$SOAK/bin" "$SOAK/logs" "$SOAK/run"
cargo test -p composition          # builds the PINNED daemon via git archive + proves the seam
cp target/composition/pinned-jinnd/target/debug/jinnd "$SOAK/bin/jinnd"
cargo build --release -p cron-kit && cp target/release/cron-kit "$SOAK/bin/cron-kit"
install -m 0755 tools/soak/detach.py "$SOAK/bin/detach.py"
"$SOAK/bin/cron-kit" kit "$SOAK" --every-ms 900000
```

The composition suite is the setup's preflight: a red gate means do not
start the soak. The `.commit` marker beside the cached binary must equal
the pin in `KERNEL-PIN.md`.

## Start

Both processes are detached into their own sessions by `detach.py`
(macOS has no `setsid`; a plain background job dies with its process
group — the known group-kill hazard). Daemon first, tick driver second:

```sh
SOAK=${SOAK:-$HOME/.local/state/jinn-harness-soak}
/usr/bin/python3 "$SOAK/bin/detach.py" "$SOAK/logs/jinnd.log" \
  "$SOAK/bin/jinnd" --profile "$SOAK/profile.json" --ledger "$SOAK/ledger.sqlite" \
  > "$SOAK/run/jinnd.pid"
until [ -f "$SOAK/data/health/boot.json" ]; do sleep 1; done   # boot evidence
/usr/bin/python3 "$SOAK/bin/detach.py" "$SOAK/logs/tick.log" \
  "$SOAK/bin/cron-kit" tick "$SOAK/profile.json" --interval-s 900 \
  > "$SOAK/run/tick.pid"
echo "$(date -u +%FT%TZ) started: jinnd $(cat "$SOAK/run/jinnd.pid"), tick $(cat "$SOAK/run/tick.pid")" >> "$SOAK/logs/ops.log"
```

## Health check (the one command)

```sh
SOAK=${SOAK:-$HOME/.local/state/jinn-harness-soak}; for name in jinnd tick; do pid=$(cat "$SOAK/run/$name.pid" 2>/dev/null); if kill -0 "$pid" 2>/dev/null; then echo "$name alive pid=$pid"; else echo "$name DOWN"; fi; done; last=$(ls -t "$SOAK/data/cron/runs/health" 2>/dev/null | head -1); if [ -n "$last" ]; then echo "last fire: $last ($(( ( $(date +%s) - $(stat -f %m "$SOAK/data/cron/runs/health/$last") ) / 60 )) min ago)"; else echo "no fires yet"; fi; grep -o '"fires": *[0-9]*' "$SOAK/data/health/report.json" 2>/dev/null; echo "ledger rows: $(sqlite3 "$SOAK/ledger.sqlite" 'SELECT COUNT(*) FROM events')"; echo "size: $(du -sh "$SOAK" | cut -f1)"
```

Healthy: both processes alive, last fire under 30 min old (two intervals),
ledger rows growing, size growing slowly. Any `DOWN`, a stale last fire, or
a shrinking/exploding size is a soak event — record it in `ops.log`, keep
the evidence, investigate before restarting. (The ledger count opens the
live database read-write as the composition suite does — SELECT only; a
read-only handle cannot join the live WAL.)

## Stop

Tick driver first (so no edit lands mid-shutdown), then SIGINT to the
daemon — it disposes all fibers, reaches quiescence, and flushes the
ledger before exiting:

```sh
SOAK=${SOAK:-$HOME/.local/state/jinn-harness-soak}
kill -INT "$(cat "$SOAK/run/tick.pid")"
kill -INT "$(cat "$SOAK/run/jinnd.pid")"
echo "$(date -u +%FT%TZ) stopped" >> "$SOAK/logs/ops.log"
```

Wait for the jinnd process to exit before restarting; `quiescent; ledger
flushed; bye` in `jinnd.log` is the clean-shutdown evidence. Never
`kill -9` the daemon while the soak is healthy — a hard kill is itself a
crash-recovery observation and must be logged as one.

## Restart

Stop, then Start, over the SAME root — `ledger.sqlite` and `data/` are
never reset during the soak. The scheduler's firing law absorbs the gap
honestly: at most one catch-up fire, missed boundaries land as one
`skipped` record, no backfill. Restarts are part of what the soak proves;
log each in `ops.log`.

## Pin bump mid-soak (when M2-K2 lands and the harness adopts)

A pin bump during the soak is a planned soak event, observed end to end:

1. Land the pin-bump commit per `KERNEL-PIN.md` (one commit, hashes +
   vendored surface + commit together).
2. Stop the soak (above).
3. Re-run Setup from the bumped repo: the composition suite rebuilds the
   cached daemon at the new pin (the `.commit` marker flips), the kit
   rebuild refreshes `$SOAK/bin` and regenerates `profile.json` +
   `artifacts/` with the new honest pins. **Do not touch `ledger.sqlite`
   or `data/`** — schedule state survives; the regenerated tick entry
   reseeds at `seq 0 / now-ms 0`, which never dispatches.
4. Start, and log `pin bump <old> -> <new>` in `ops.log`.
5. The next fires after the bump are the adoption evidence for the +7d
   audit.

## The +7d audit (closes the soak)

At soak start + 7 days, against `ops.log`, the ledger, and `data/`:
fire count vs expected (4/hour × wall time, minus recorded gaps),
missed/skipped record audit, ledger growth rate (rows and bytes — HostFs
undo retention, FINDINGS.md #8, is what the byte curve is watching; the
evidence feeds M2-K3), memory/disk footprint, restart/crash log, zero
old-gateway interaction (the daemon touches nothing outside `$SOAK`).
The audit report goes on the tracking Todo for the M2 acceptance decision.
