# The cron soak — a week of real duty (phase 1.4, M2 acceptance)

The M2 acceptance run per the kernel roadmap: the cron slice doing its
production job for a week, on the pinned kernel, without touching the old
gateway's data. The seam under soak is `plugins/cron/` booted through the
real pinned `jinnd` daemon; the scheduler wakes itself on one `jinn:clock`
periodic alarm at a 15-minute cadence (`tick-ms`), so the daemon is the only
process on duty.

**Soak started:** 2026-08-28T04:28:59Z · kernel pin `a17df864` (the pin at
soak start; bumped mid-soak to `01133c45`, `41cb2f47`, `4eb4a93` and
`9e61e47`, all on 2026-08-28, and to `1b098be` on 2026-08-29 — see §Pin
bump mid-soak, and `KERNEL-PIN.md` owns the current pin) · harness `5c828c6` · job `health` every 900 000 ms, wake cadence
900 000 ms.

## Layout

Everything at runtime lives OUTSIDE the repo, under one root:

```sh
SOAK=${SOAK:-$HOME/.local/state/jinn-harness-soak}
```

| Path | What |
|---|---|
| `$SOAK/bin/` | `jinnd` (built from the pinned commit by the composition harness's git-archive build) + `jinnd.commit` (its pin marker), `api-kit` (release; `cron-kit` until the sixth bump), `detach.py` and `soak-run.sh` (copies of the `tools/soak/` originals) |
| `$SOAK/data/profile.json`, `$SOAK/artifacts/` | The generated kit (`api-kit kit`: the cron seam plus the operator-API trio since the sixth bump; `cron-kit kit` before it) — never hand-edited. The profile moved INTO the data root at the sixth bump so the api consumers can read it through their scoped `jinn:fs` (FINDINGS.md #25); the wrapper passes `--artifacts`/`--data` explicitly. |
| `$SOAK/ledger.sqlite` | The daemon's append-only ledger (the evidence surface) |
| `$SOAK/data/` | The daemon's data root: `cron/` (state, the append-only history log, per-fire run records), `health/` (the consumer's reports), and since the sixth bump `profile.json` (the daemon's watcher is non-recursive on the profile's directory, so the fibers' subdirectories never wake it) |
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
cargo build --release -p api-kit && cp target/release/api-kit "$SOAK/bin/api-kit"
install -m 0755 tools/soak/detach.py "$SOAK/bin/detach.py"
tools/soak/install-launchd.sh                  # wrapper + LaunchAgent, files only (§Supervisor)
"$SOAK/bin/api-kit" kit "$SOAK" --port 7921 --every-ms 900000 --tick-ms 900000
mv "$SOAK/profile.json" "$SOAK/data/profile.json"   # the document sits in the data root (FINDINGS.md #25)
```

`--port 7921` is the operator API's loopback port (its `jinn:net` grant is
scoped to exactly that port; nothing routes to production — the API
serves this composition alone). Before the sixth bump the kit was
`cron-kit kit "$SOAK" --every-ms 900000 --tick-ms 900000` and the profile
sat at `$SOAK/profile.json`.

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
those are the outages the agent exists to end. `ThrottleInterval` is the minimum 30 s
between STARTS: a daemon that has been up longer relaunches immediately, a
crash loop is throttled to one start per 30 s.

**What the wrapper adds.** It derives `$SOAK` from `$HOME` (the plist cannot
expand it), redirects the daemon's stderr into `logs/jinnd.log`, appends one
`started (launchd; reason=...)` line to `ops.log`, writes `run/jinnd.pid`,
and `exec`s the daemon with absolute `--profile`/`--ledger` paths (its
canonical form; since pin `9e61e47` the daemon resolves relative paths
itself, FINDINGS.md #18 closed). Because it `exec`s, the daemon inherits the wrapper's
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
mark=$(wc -l < "$SOAK/logs/jinnd.log")
launchctl kickstart gui/$(id -u)/run.jinn.harness-soak
until tail -n +"$((mark + 1))" "$SOAK/logs/jinnd.log" | grep -q '"jinnd":"ready"'; do sleep 1; done
launchctl print gui/$(id -u)/run.jinn.harness-soak | grep -E 'state|pid'
```

The start evidence is the daemon's READINESS line (since pin `9e61e47`,
FINDINGS.md #12 minimum): one machine-readable line in `jinnd.log`,
`{"jinnd":"ready","watcher":"armed","profile":"…"}`, emitted only once the
file watcher is armed AND the boot reconcile is done. It is the line to
key on — never `boot.json`: the watcher now arms (or refuses, exit 1)
BEFORE any boot evidence is written (FINDINGS.md #18 closed), so a refused
start leaves no `boot.json` and no readiness line, and a readiness line
means a serving, watched daemon.

**Unsupervised (the fallback, for a daemon deliberately outside the agent).**
`detach.py` puts it in its own session — macOS has no `setsid`, and a plain
background job dies with its process group (the known group-kill hazard).
Do not run this while the agent is loaded; that is the double start.

```sh
SOAK=${SOAK:-$HOME/.local/state/jinn-harness-soak}
mark=$(wc -l < "$SOAK/logs/jinnd.log")
/usr/bin/python3 "$SOAK/bin/detach.py" "$SOAK/logs/jinnd.log" \
  "$SOAK/bin/jinnd" --profile "$SOAK/data/profile.json" --ledger "$SOAK/ledger.sqlite" \
  --artifacts "$SOAK/artifacts" --data "$SOAK/data" \
  > "$SOAK/run/jinnd.pid"
until tail -n +"$((mark + 1))" "$SOAK/logs/jinnd.log" | grep -q '"jinnd":"ready"'; do sleep 1; done
echo "$(date -u +%FT%TZ) started: jinnd $(cat "$SOAK/run/jinnd.pid")" >> "$SOAK/logs/ops.log"
```

Both lanes pass absolute paths as a matter of form; a relative
`--profile` is no longer a hazard (the daemon canonicalizes against its
working directory before arming the watcher — the third-bump start slip
recorded in §Pin bump mid-soak cannot recur at this pin).

## Health check (the one command)

```sh
SOAK=${SOAK:-$HOME/.local/state/jinn-harness-soak}; pid=$(cat "$SOAK/run/jinnd.pid" 2>/dev/null); if kill -0 "$pid" 2>/dev/null; then echo "jinnd alive pid=$pid"; else echo "jinnd DOWN"; fi; last=$(ls -t "$SOAK/data/cron/runs/health" 2>/dev/null | head -1); if [ -n "$last" ]; then echo "last fire: $last ($(( ( $(date +%s) - $(stat -f %m "$SOAK/data/cron/runs/health/$last") ) / 60 )) min ago)"; else echo "no fires yet"; fi; grep -o '"fires": *[0-9]*' "$SOAK/data/health/report.json" 2>/dev/null; echo "ledger rows: $(sqlite3 "$SOAK/ledger.sqlite" 'SELECT COUNT(*) FROM events')"; echo "size: $(du -sh "$SOAK" | cut -f1)"
```

Since the sixth bump the operator API is the second check — one loopback
request, answered from the kernel's own view of the composition:

```sh
curl -s 127.0.0.1:7921/v1/health
```

Healthy: the daemon alive, last fire under 30 min old (two wakes), ledger
rows growing, size growing slowly, `/v1/health` answering `"ok":true` with
five entries. Any `DOWN`, a stale last fire, or a
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

A stop that lands mid-tick (the wake handler in flight) lands the WHOLE
tick: since pin `9e61e47` the kernel drains the in-flight handler under
the guest deadline before sealing the journal (FINDINGS.md #16 closed), so
the state write, the run record and the history line all land and the
suspension retains them — `cron/state.json`'s `last` and the newest
history record agree exactly after every planned stop. (At `4eb4a93` such
a stop tore the tick: state advanced, history line refused; the firing law
absorbed it as one lost record.)

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
uncleanly, launchd relaunches it as soon as `ThrottleInterval` allows, and the
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

**Executed 2026-08-28 (fourth bump):** `4eb4a93` → `9e61e47` (jinnd
M2-K5: drain-before-seal, one-shot own-write recognition,
watch-before-evidence + readiness line; no contract delta), harness PR
#7. The first fully supervised bump: a clean SIGINT stop of the
`4eb4a93` daemon (planned, logged), Setup re-run from the bumped repo
(the composition suite rebuilt the cached daemon at the new pin, the kit
regenerated, the wrapper refreshed), then `printf planned-start` +
`kickstart`. The start evidence was the readiness line, captured verbatim
in `ops.log` beside the `pin bump 4eb4a93 -> 9e61e47` line; the post-bump
`AlarmWake` → `DispatchTrace` → run record → append chain follows it.

**Executed 2026-08-29 (fifth bump):** `9e61e47` → `1b098be` (jinnd
M2-K6: `jinn:process` + `jinn:net` providers, world `jinn:plugin@0.4.0`
with the bundles' typed errors on the wire; harness packet
`packet/2.1-operator-api`). Supervised, per this procedure: a clean
SIGINT stop of the `9e61e47` daemon (planned, logged with before/after
evidence — state, history and report byte-identical across the stop),
Setup re-run from the bumped repo (the composition suite rebuilt the
cached daemon at the new pin, the release kit refreshed, the wrapper
re-rendered, the kit regenerated — the cron artifact hashes were
byte-identical to the previous pin's, the guests regenerate their
bindings on the 0.4.0 world with no logic change), then
`printf planned-start` + `kickstart`. Start evidence: the readiness line,
verbatim in `ops.log` beside the `pin bump 9e61e47 -> 1b098be` line. The
boot's ledger shows FOUR `ServiceProvided` rows (`jinn:fs`, `jinn:clock`,
`jinn:process`, `jinn:net` — FINDINGS.md #5 closed for the latter two)
and the scheduler RESUMING with one catch-up fire for the boundary that
elapsed inside the stop window (the activate plan, FINDINGS.md #13
shape). Duty gap ≈ 20 s. The soak's tree is unchanged — the operator-API
seam is NOT mounted in the soak profile (the M2 acceptance run keeps the
composition it started with; the api trio is proven in the composition
suite and served only there).

**Executed 2026-08-29 (sixth bump):** `1b098be` → `57360cc` (jinnd
M2-K7: `jinn:introspect`, the live `jinn:ledger` reader, `jinn:profile`
`patch-entry` as operator intent, the `jinn:net` readiness wake; harness
packet `packet/2.2-settings`, PLA-314). Supervised, per this procedure:
a clean SIGINT stop of the `1b098be` daemon (planned, logged with
before/after evidence — state, history and report byte-identical across
the stop), Setup re-run from the bumped repo with `api-kit` replacing
`cron-kit` as the kit builder: the composition suite rebuilt the cached
daemon at the new pin, the wrapper was re-rendered (it now passes
`--profile data/profile.json --artifacts --data` explicitly), the kit
regenerated SEVEN entries — the cron pair (byte-identical duty: same
job, same cadence), the api trio on loopback `:7921`, the settings pair
— and the profile was MOVED into the data root (FINDINGS.md #25), then
`printf planned-start` + `kickstart`. Start evidence: the readiness
line, verbatim in `ops.log` beside the `pin bump 1b098be -> 57360cc`
line. The boot's ledger shows SEVEN `ServiceProvided` rows (fs, clock,
process, net, introspect, ledger, profile), the scheduler RESUMING with
one catch-up fire for the boundary that elapsed inside the stop window
(FINDINGS.md #13 shape), its first settings declaration one clock floor
later, and `jinn-api-http` listening with ZERO alarms (FINDINGS.md #23
closed). Duty gap ≈ 2 min (the relocation and the kit regeneration were
inside it). **This is the first bump that changes the soak's tree:** the
M2 acceptance composition (the cron pair) is unchanged and still the
only duty; the api trio and the settings pair are mounted BESIDE it as
the seams that qualified for real duty (the API answers from the
kernel's own view and holds no alarm; the scheduler consumes its job
table through `jinn:settings`). The idle-growth measurement that
qualifies the API is in the next paragraph.

**Idle ledger growth with the API mounted (2026-08-29):** measured over a 971 s window starting right
after the two post-boot operator requests (ledger rows 1926 → 1948, no
operator request made inside it). The window contained exactly one
scheduler wake (the 15-minute cadence): +22 rows, ALL attributed to the
cron duty — `cron-scheduler` 10 (the wake, its settings re-declaration,
state write, fire, run record, history append), `health-snapshot` 10
(its report chain), `jinn-settings-profile` 2 (the declaration's overlay
read) — and ZERO rows attributed to `jinn-api-http`, `jinn-status`,
`jinn-profile-edit` or `jinn-settings-store`. The mounted API's idle cost
is 0 rows/min (the poll shape FINDINGS.md #23 measured was ≈ 8 rows/s);
the settings seam adds 4 rows per scheduler wake (`ContractResolved` +
`declare` on the scheduler, `ContractResolved` + `overlays` on the
provider) — 384 rows/day at this cadence, the price of a layer that is
guest knowledge (FINDINGS.md #27). The +7d audit's per-tick baseline is
therefore ≈ 22 rows/wake from this bump on (≈ 5 before the first bump,
≈ 15 after it). Recorded in `ops.log` beside the bump line.

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
