# The cron soak — a week of real duty (phase 1.4, M2 acceptance)

The M2 acceptance run per the kernel roadmap: the cron slice doing its
production job for a week, on the pinned kernel, without touching the old
gateway's data. The seam under soak is `plugins/cron/` booted through the
real pinned `jinnd` daemon; the scheduler wakes itself on one `jinn:clock`
periodic alarm at a 15-minute cadence (`tick-ms`), so the daemon is the only
process on duty.

**Soak started:** 2026-08-28T04:28:59Z · kernel pin `a17df864` (the pin at
soak start; bumped mid-soak to `01133c45`, `41cb2f47`, and `4eb4a93`, all
on 2026-08-28 — see §Pin bump mid-soak, and `KERNEL-PIN.md` owns the
current pin) · harness `5c828c6` · job `health` every 900 000 ms, wake cadence
900 000 ms.

## Layout

Everything at runtime lives OUTSIDE the repo, under one root:

```sh
SOAK=${SOAK:-$HOME/.local/state/jinn-harness-soak}
```

| Path | What |
|---|---|
| `$SOAK/bin/` | `jinnd` (built from the pinned commit by the composition harness's git-archive build) + `jinnd.commit` (its pin marker), `cron-kit` (release), `detach.py` and `soak-run.sh` (copies of the `tools/soak/` originals) |
| `$SOAK/profile.json`, `$SOAK/artifacts/` | The generated kit (`cron-kit kit`) — never hand-edited |
| `$SOAK/ledger.sqlite` | The daemon's append-only ledger (the evidence surface) |
| `$SOAK/data/` | The daemon's data root: `cron/` (state, the append-only history log, per-fire run records), `health/` (the consumer's reports) |
| `$SOAK/data.inverses/` | The kernel's `jinn:fs` effect-retention store (since pin `41cb2f47`): one durable inverse per live revertible effect, keyed by effect id — the byte curve FINDINGS.md #8 asked for is measured here |
| `$SOAK/logs/` | `jinnd.log`, `ops.log` (operator actions, one timestamped line each — restarts and the pin bump count toward the +7d audit) |
| `$SOAK/run/` | `jinnd.pid`; the supervisor's two scratch files, `launchd.hostboot` (the host boot stamp the wrapper compares to tell a reboot from a crash restart) and `launchd.reason` (one word an operator drops to name a planned start; the wrapper consumes it) |
| `$SOAK/meta.json` | Start timestamp + pins, written once at soak start |

## Setup (how the runtime root is stood up)

From the repo root, with a jinnd checkout holding the pinned commit
reachable (`KERNEL-PIN.md` Gate-2 lanes):

```sh
SOAK=${SOAK:-$HOME/.local/state/jinn-harness-soak}
mkdir -p "$SOAK/bin" "$SOAK/logs" "$SOAK/run"
cargo test -p composition          # builds the PINNED daemon via git archive + proves the seam
cp target/composition/pinned-jinnd/target/debug/jinnd "$SOAK/bin/jinnd"
cp target/composition/pinned-jinnd/.commit "$SOAK/bin/jinnd.commit"   # the wrapper logs this pin
cargo build --release -p cron-kit && cp target/release/cron-kit "$SOAK/bin/cron-kit"
install -m 0755 tools/soak/detach.py "$SOAK/bin/detach.py"
tools/soak/install-launchd.sh                  # wrapper + LaunchAgent, files only (§Supervisor)
"$SOAK/bin/cron-kit" kit "$SOAK" --every-ms 900000 --tick-ms 900000
```

`--tick-ms 900000` is the scheduler's alarm period: one wake per 15 minutes,
the same cadence and the same fire-lateness envelope the retired timer
stand-in had, so the +7d comparison across the pin bump stays apples to
apples.

The composition suite is the setup's preflight: a red gate means do not
start the soak. The `.commit` marker beside the cached binary must equal
the pin in `KERNEL-PIN.md`.

## Supervisor (the LaunchAgent)

The soak's daemon runs under a user LaunchAgent, `run.jinn.harness-soak`.
The 2026-08-28 13:36Z host reboot is why: `detach.py` only detaches, so the
reboot killed the daemon silently and duty stayed down until a session
happened to look. Under the agent a reboot is a counted soak event — the
daemon comes back at login and says so in `ops.log`.

The tracked originals are `tools/soak/`: `run.jinn.harness-soak.plist.template`
(the plist; `__SOAK__` is a placeholder, no machine path is ever tracked),
`soak-run.sh` (the wrapper launchd actually runs) and `install-launchd.sh`
(renders + installs both, and deliberately does NOT load the agent — see
Adoption). `tools/harness-pin`'s `soak_supervisor` gate holds those bounds.

**What the plist declares.** `RunAtLoad` (start at login, i.e. after a
reboot) and `KeepAlive = { SuccessfulExit: false }` — restart only after an
UNCLEAN exit. That condition is the whole reason planned stops still work:
the daemon exits 0 after a clean SIGINT suspend-and-flush, so §Stop stays
stopped and the supervisor never fights the operator; a `kill -9`, a failed
flush barrier (exit 1) and a host that dies underneath are all unclean, and
those are the outages the agent exists to end. `ThrottleInterval` 30 s
bounds a crash loop.

**What the wrapper adds.** It derives `$SOAK` from `$HOME` (the plist cannot
expand it), redirects the daemon's stderr into `logs/jinnd.log`, appends one
`started (launchd; reason=...)` line to `ops.log`, writes `run/jinnd.pid`,
and `exec`s the daemon with ABSOLUTE `--profile`/`--ledger` paths
(FINDINGS.md #18). Because it `exec`s, the daemon inherits the wrapper's
pid: `run/jinnd.pid` is the daemon's own pid, the §Health check and §Stop
are unchanged, and launchd keys `SuccessfulExit` on the daemon's own status.

The reason vocabulary is what the +7d audit counts:

| reason | means |
|---|---|
| `adopt` / `planned-start` | an operator start — the reason file was dropped first |
| `boot` | first supervised start since this host booted (a reboot) |
| `keepalive-restart` | same host boot, nobody asked: launchd replaced a daemon that exited uncleanly |

**Operator verbs** (`bootstrap`/`bootout`/`kickstart`, the modern `launchctl`
lane — pick one lane and stay in it; never mix in `load`/`unload`):

```sh
label=run.jinn.harness-soak; plist="$HOME/Library/LaunchAgents/$label.plist"
launchctl bootstrap gui/$(id -u) "$plist"    # install the job (RunAtLoad starts it)
launchctl kickstart  gui/$(id -u)/$label     # start it again after a planned stop
launchctl print      gui/$(id -u)/$label     # state, last exit status, pid
launchctl bootout    gui/$(id -u)/$label     # remove the job (SIGTERMs a running daemon — see below)
```

`bootout` is for RETIRING the supervisor, not for stopping the soak: launchd
SIGTERMs, and the daemon's clean path is SIGINT only, so a `bootout` over a
live daemon is a hard kill. Always stop cleanly first (§Stop), then boot out.

**Adoption** (how a running, unsupervised daemon moves under the agent
without a double start): `install-launchd.sh`, then a clean §Stop of the
running daemon, then `printf adopt > "$SOAK/run/launchd.reason"`, then
`bootstrap`. Every step is an `ops.log` line.

## Start

**Supervised (the normal path).** The agent is loaded; the operator names the
start so it is not mistaken for a crash restart, and `kickstart` runs the
wrapper, which does everything §Supervisor describes — including the
`started (launchd; reason=planned-start)` line, so no manual `ops.log` echo
is needed:

```sh
SOAK=${SOAK:-$HOME/.local/state/jinn-harness-soak}
printf planned-start > "$SOAK/run/launchd.reason"
launchctl kickstart gui/$(id -u)/run.jinn.harness-soak
until [ -f "$SOAK/data/health/boot.json" ]; do sleep 1; done   # boot evidence
launchctl print gui/$(id -u)/run.jinn.harness-soak | grep -E 'state|pid'
```

`boot.json` alone is not proof of a healthy start: the boot reconcile writes
it BEFORE the file watcher is attempted (FINDINGS.md #18), so confirm the
job is `running` with a pid, and that `jinnd.log` carries no `file watcher
unavailable`.

**Unsupervised (the fallback, for a daemon deliberately outside the agent).**
`detach.py` puts it in its own session — macOS has no `setsid`, and a plain
background job dies with its process group (the known group-kill hazard).
Do not run this while the agent is loaded; that is the double start.

```sh
SOAK=${SOAK:-$HOME/.local/state/jinn-harness-soak}
/usr/bin/python3 "$SOAK/bin/detach.py" "$SOAK/logs/jinnd.log" \
  "$SOAK/bin/jinnd" --profile "$SOAK/profile.json" --ledger "$SOAK/ledger.sqlite" \
  > "$SOAK/run/jinnd.pid"
until [ -f "$SOAK/data/health/boot.json" ]; do sleep 1; done   # boot evidence
echo "$(date -u +%FT%TZ) started: jinnd $(cat "$SOAK/run/jinnd.pid")" >> "$SOAK/logs/ops.log"
```

Absolute `--profile`/`--ledger` paths in both lanes, always (FINDINGS.md #18).

## Health check (the one command)

```sh
SOAK=${SOAK:-$HOME/.local/state/jinn-harness-soak}; pid=$(cat "$SOAK/run/jinnd.pid" 2>/dev/null); if kill -0 "$pid" 2>/dev/null; then echo "jinnd alive pid=$pid"; else echo "jinnd DOWN"; fi; last=$(ls -t "$SOAK/data/cron/runs/health" 2>/dev/null | head -1); if [ -n "$last" ]; then echo "last fire: $last ($(( ( $(date +%s) - $(stat -f %m "$SOAK/data/cron/runs/health/$last") ) / 60 )) min ago)"; else echo "no fires yet"; fi; grep -o '"fires": *[0-9]*' "$SOAK/data/health/report.json" 2>/dev/null; echo "ledger rows: $(sqlite3 "$SOAK/ledger.sqlite" 'SELECT COUNT(*) FROM events')"; echo "size: $(du -sh "$SOAK" | cut -f1)"
```

Healthy: the daemon alive, last fire under 30 min old (two wakes), ledger
rows growing, size growing slowly. Any `DOWN`, a stale last fire, or a
shrinking/exploding size is a soak event — record it in `ops.log`, keep the
evidence, investigate before restarting. (The ledger count opens the live
database read-write as the composition suite does — SELECT only; a
read-only handle cannot join the live WAL.)

## Stop

SIGINT to the daemon — the planned-stop path. Since pin `4eb4a93` (jinnd
M2-K4: suspend ≠ dispose) a clean stop SUSPENDS every fiber: kernel
registrations release (the scheduler's alarm, its `jinn:cron` provision,
the consumer's listener — their inverses run, on the record), every
`jinn:fs` mutation the fibers made is RETAINED for its profile entry (the
schedule state, the history log, the run records, the consumer's report
stay exactly as the fibers left them; their durable inverses stay under
`data.inverses/`), one typed `FiberSuspended { retained }` event lands per
fiber, and the daemon reaches quiescence and flushes the ledger before
exiting. A clean stop is therefore a duty-continuity event, not a state
event: the next start resumes the schedule from the persisted `last`.

```sh
SOAK=${SOAK:-$HOME/.local/state/jinn-harness-soak}
kill -INT "$(cat "$SOAK/run/jinnd.pid")"
echo "$(date -u +%FT%TZ) stopped (clean; fibers suspended, state retained)" >> "$SOAK/logs/ops.log"
```

The supervisor needs no `bootout` here and must not get one: the clean exit
is status 0, and `KeepAlive = { SuccessfulExit: false }` leaves a clean exit
alone (§Supervisor). Confirm with `launchctl print gui/$(id -u)/run.jinn.harness-soak`
— the job goes to `not running` with `last exit code = 0`, and stays there
until a §Start `kickstart`. If the job DOES come back on its own, the exit
was not clean: read the tail of `jinnd.log` (a failed flush barrier exits 1)
and log it as a crash, not a stop.

Wait for the jinnd process to exit before restarting; `quiescent; ledger
flushed; bye` in `jinnd.log` and the two `FiberSuspended` rows at the
ledger's tail are the clean-shutdown evidence. Never `kill -9` the daemon
while the soak is healthy — a hard kill is itself a crash-recovery
observation and must be logged as one (the files survive it too: crash and
clean stop agree on the disk outcome, only the clean path flushes).

A stop that lands mid-tick (the wake handler in flight) can leave that
tick torn — state advanced, its history line refused after the journal
sealed (FINDINGS.md #16); the firing law absorbs it (no double fire, one
lost record), and the ledger still carries the fire.

**History of this section:** at pin `41cb2f47` a clean stop WITHDREW the
fibers' fs contribution (FINDINGS.md #14), and the COO ruled on
2026-08-28 that planned stops use the hard path (`kill -9`, logged
`stopped (hard, planned; files kept per FINDINGS #14 + COO ruling)`)
until the kernel retired the finding. **That ruling is retired as of pin
`4eb4a93` (same day):** the finding is closed kernel-side, the clean path
preserves the audit evidence, and SIGINT is the planned-stop path again.
The soak's one `41cb2f47`-era stop ended up being neither: the host
rebooted (see §Pin bump mid-soak, third bump).

## Restart

Stop, then Start (the supervised lane: `kickstart` after the reason file), over the SAME root — `ledger.sqlite` and `data/` are
never reset during the soak. The schedule RESUMES from the persisted
`cron/state.json` (no second `schedule-started`). Alarms do not survive a
kernel restart, so the scheduler re-requests its alarm and runs one plan
immediately at `activate`:
the catch-up fire lands at boot rather than one period later. The firing law
absorbs the gap honestly: at most one catch-up fire, missed boundaries land
as one `skipped` record, no backfill. Restarts are part of what the soak
proves; each lands in `ops.log` — the wrapper writes the start line itself
with the reason that tells a planned restart from a reboot from a
KeepAlive restart, and the operator adds the before/after evidence line.

An UNPLANNED restart needs no operator at all now: the daemon dies
uncleanly, launchd relaunches it within `ThrottleInterval`, and the
wrapper's `reason=keepalive-restart` (or `reason=boot` after a host
reboot) is the outage's own record. Annotate it when you next look —
what died, and what the ledger shows on the other side.

## Pin bump mid-soak

A pin bump during the soak is a planned soak event, observed end to end.
The procedure, for this bump and any future one:

1. Land the pin-bump commit per `KERNEL-PIN.md` (one commit, hashes +
   vendored surface + commit together).
2. Stop the soak (above).
3. Re-run Setup from the bumped repo (the daemon is down, so refreshing
   `$SOAK/bin` is safe; `jinnd.commit` flips with the binary, and
   `install-launchd.sh` refreshes the wrapper if it changed — the plist
   itself is pin-independent, so no `bootout`/`bootstrap` is needed): the composition suite rebuilds the
   cached daemon at the new pin (the `.commit` marker flips), the kit
   rebuild refreshes `$SOAK/bin` and regenerates `profile.json` +
   `artifacts/` with the new honest pins. **Do not touch `ledger.sqlite`
   or `data/`** — schedule state survives the regeneration.
4. Start per §Start (`printf planned-start`, then `kickstart`), and log
   `pin bump <old> -> <new>` in `ops.log` beside the wrapper's start line.
5. The next fires after the bump are the adoption evidence for the +7d
   audit.

**Executed 2026-08-28:** old pin `a17df864` → new pin `01133c45` (jinnd
M2-K2: `jinn:clock` + the `DispatchTrace` bus tap), harness PR #3. The bump is what retired the timer stand-in and the process
that drove it: from here the soak runs one process, and `ops.log` carries
the `pin bump a17df864 -> 01133c45` line.

**Executed 2026-08-28 (second bump):** `01133c45` → `41cb2f47` (jinnd
M2-K3: the `jinn:fs@0.2.0` bundle + HostFs effect retention), harness PR
#4. The stop was a clean SIGINT of the `01133c45` daemon (whose fs
effects were not journaled, so the files survived it — the last stop for
which that is true, see §Stop). Over the same root the `41cb2f47`
scheduler read the legacy `cron/history.json` once as its window seed and
opened `cron/history.jsonl` as the append lane; `ops.log` carries the
`pin bump 01133c45 -> 41cb2f47` line and the before/after evidence.

**Executed 2026-08-28 (third bump):** `41cb2f47` → `4eb4a93` (jinnd
M2-K4: suspend ≠ dispose, `jinn:plugin@0.3.0`), harness PR #5. The
planned stop never happened: the host rebooted at 13:36:01Z and the
`41cb2f47` daemon died with it — logged in `ops.log` as an UNPLANNED
crash-recovery observation (the disk outcome is a hard kill's: files
intact, the WAL's last rows committed), which pre-empted the standing
hard-stop ruling's last use. The first `4eb4a93` start was launched
with relative paths and exited after its boot reconcile (the watcher
refused — FINDINGS.md #18; an operator slip, corrected in `ops.log`); the
start per §Start over the same root resumed the schedule from the
persisted `last` (one catch-up fire for the newest missed boundary, the
rest one `skipped` record), and then performed the first CLEAN stop/start
cycle of the soak as the proof that the ruling could be retired:
`ops.log` carries the reboot line, the `pin bump 41cb2f47 -> 4eb4a93`
line, the correction, and the clean cycle's before/after evidence.

## The +7d audit (closes the soak)

At soak start + 7 days, against `ops.log`, the ledger, and `data/`:
fire count vs expected (4/hour × wall time, minus recorded gaps),
missed/skipped record audit, ledger growth rate (rows and bytes — the byte curve watched HostFs
undo retention in RAM until pin `41cb2f47`, and from there the durable
retention store `$SOAK/data.inverses/` plus the append-only history log,
FINDINGS.md #8 closed), memory/disk footprint, restart/crash log — read straight off the
`started (launchd; reason=...)` lines from the supervisor's adoption
onward, `boot` and `keepalive-restart` counted as outages and
`adopt`/`planned-start` as planned (the 2026-08-28 host reboot counts as
a crash, the clean stop/start cycle after the third bump and the
supervisor adoption as planned restarts) — zero
old-gateway interaction (the daemon touches nothing outside `$SOAK`), and
post-bump fire evidence: `AlarmWake` + `DispatchTrace` lines in the ledger
after the bump timestamp, with the per-tick ledger row cost compared before
(≈5 rows/tick of fiber churn) and after (1 `AlarmWake` per wake).
The audit report goes on the tracking Todo for the M2 acceptance decision.
