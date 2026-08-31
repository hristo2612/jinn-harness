# The cron soak — a week of real duty (phase 1.4, M2 acceptance)

The M2 acceptance run per the kernel roadmap: the cron slice doing its
production job for a week, on the pinned kernel, without touching the old
gateway's data. The seam under soak is `plugins/cron/` booted through the
real pinned `jinnd` daemon; the scheduler wakes itself on one `jinn:clock`
periodic alarm at a 15-minute cadence (`tick-ms`), so the daemon is the only
process on duty.

**Soak started:** 2026-08-28T04:28:59Z · kernel pin `a17df864` (the pin at
soak start; bumped mid-soak to `01133c45`, `41cb2f47`, `4eb4a93` and
`9e61e47`, all on 2026-08-28, to `1b098be` and `57360cc` on 2026-08-29, and
to `3a8e5c03` on 2026-08-31 — seven bumps, see §Pin bump mid-soak, and
`KERNEL-PIN.md` owns the current pin) · harness `5c828c6` · job `health`
every 900 000 ms, wake cadence 900 000 ms.

## Layout

Everything at runtime lives OUTSIDE the repo, under one root:

```sh
SOAK=${SOAK:-$HOME/.local/state/jinn-harness-soak}
```

| Path | What |
|---|---|
| `$SOAK/bin/` | `jinnd` (built from the pinned commit by the composition harness's git-archive build) + `jinnd.build` (the install record that BINDS a pin to that binary's digest — since the seventh bump; the unbound `jinnd.commit` marker it replaced is deleted), `api-kit` (release; `cron-kit` until the sixth bump), `detach.py` and `soak-run.sh` (copies of the `tools/soak/` originals; a change here reaches the soak only when `install-launchd.sh` re-copies it, so the running copy can legitimately predate the repo's) |
| `$SOAK/data/profile.json`, `$SOAK/artifacts/` | The generated kit (`api-kit kit`: the cron seam plus the operator-API trio since the sixth bump; `cron-kit kit` before it) — never hand-edited. The profile moved INTO the data root at the sixth bump so the api consumers can read it through their scoped `jinn:fs` (FINDINGS.md #25); the wrapper passes `--artifacts`/`--data` explicitly. |
| `$SOAK/ledger.sqlite` | The daemon's append-only ledger (the evidence surface) |
| `$SOAK/data/` | The daemon's data root: `cron/` (state, the append-only history log, per-fire run records), `health/` (the consumer's reports), and since the sixth bump `profile.json` (the daemon's watcher is non-recursive on the profile's directory, so the fibers' subdirectories never wake it) |
| `$SOAK/data.inverses/` | The kernel's `jinn:fs` effect-retention store (since pin `41cb2f47`): one durable inverse per live revertible effect, keyed by effect id — the byte curve FINDINGS.md #8 asked for is measured here |
| `$SOAK/logs/` | `jinnd.log`, `ops.log` (operator actions, one timestamped line each — restarts and the pin bump count toward the +7d audit) |
| `$SOAK/run/` | `jinnd.pid` (pid and mtime are ONE previous-start record: the wrapper proves both or neither, then compares the mtime to `kern.boottime` to derive whether the readings are consistent with a reboot or with a crash restart); `launchd.reason` (one word an operator drops to name a planned start; the wrapper consumes it) |
| `$SOAK/meta.json` | Start timestamp, written once at soak start. It records NO pin: a hand-maintained pin is what drifted two bumps behind (§What the record is), and `bin/jinnd.build` is now the one home for that fact |

## Setup (how the runtime root is stood up)

From the repo root, with a jinnd checkout holding the pinned commit
reachable (`KERNEL-PIN.md` Gate-2 lanes):

```sh
SOAK=${SOAK:-$HOME/.local/state/jinn-harness-soak}
mkdir -p "$SOAK/bin" "$SOAK/logs" "$SOAK/run"
cargo test -p composition          # builds the PINNED daemon via git archive + proves the seam
tools/soak/record-build.sh target/composition/pinned-jinnd   # installs the daemon AND records what it is
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
start the soak. `record-build.sh` installs the daemon and writes the
record of what it is; before the seventh bump this step was two `cp`s and
a `jinnd.commit` marker, which is the shape §What the record is describes.

## What the record is (and why it is not typed)

The soak's account of its own kernel is DERIVED at every start, never
written by hand.

`record-build.sh` installs `$SOAK/bin/jinnd` and writes
`$SOAK/bin/jinnd.build` beside it, with every field read from something
rather than supplied by someone:

| field | derived from |
|---|---|
| `binary-sha256` | the installed bytes, digested here |
| `running-pin` | the composition build's `.commit` marker, which the git-archive build took from the commit it checked out |
| `harness-pin` | `KERNEL-PIN.md`'s `commit:` line |
| `recorded-utc` | the clock |

Any of those unreadable is a refusal: a record with a hole in it is what
an auditor cannot tell from a whole one.

`soak-run.sh` then re-computes the digest of the binary it is about to
exec and accepts a pin ONLY from a record describing that digest. So the
evidence on every `ops.log` line carries three new readings:

- `binary_sha256=` — what is running, as an identity. Always reported
  where the file could be read, pin or no pin.
- `running_pin=` — the commit, licensed by the digest match. `unknown`
  otherwise, with `unproven=` naming which way it failed:
  `running-binary` (the daemon could not be read), `build-record` (none
  present), `build-record-unreadable` (present, its own fields do not
  parse), `build-record-mismatch` (present, and about a DIFFERENT
  binary).
- `harness_pin=` — what `KERNEL-PIN.md` said the harness ships when the
  record was written. A SEPARATE reading, in a separate field, which
  never fills `running_pin`.
- `running_pin_checked=` / `harness_pin_checked=` — WHICH CHECK licensed
  each pin, because looking like a commit and being one are two readings.
  A value is a pin only if it is exactly 40 lowercase hex characters
  (`a` is not a commit, and the round-1 wrapper took it); that shape
  check alone reads `well-formed`. Where a kernel checkout is reachable
  (`JINND_DIR`) the wrapper asks the repo and reads
  `resolves-in-kernel-repo`; a well-formed sha the repo does NOT hold
  reads `absent-from-kernel-repo` and takes the value down to `unknown`
  with `running-pin-absent-from-kernel-repo` among the unproven inputs.
  A checkout that is present but unreadable answers nothing rather than no:
  `well-formed-kernel-repo-unreadable`, with the value kept. Under launchd
  no checkout is in sight at all, so a live start reads `well-formed` and
  claims exactly that much. `record-build.sh` refuses
  at install time on both readings rather than writing a record it cannot
  stand behind.

**Why the two pins never share a field.** On 2026-08-31 a COO drift audit
found three sources disagreeing: `meta.json` said `41cb2f47`, the binary
and `ops.log` said `57360cc`, `KERNEL-PIN.md` said `3a8e5c03`. A third
bump had happened and the audit's own artifact still named the pin from
two bumps earlier. Nothing detected it because nothing could — every
reading was internally consistent, and a stale record is
indistinguishable from a current one when nothing binds it to the thing
it describes. The distance between *what is running* and *what ships* is
exactly what that audit measured; a field that can hold either cannot
show it. The kernel side is `FINDINGS.md` #42: the daemon has no
`--version` and embeds no commit, so this join is the strongest reading
available, and it proves "this binary is the one some install recorded as
built from commit C" rather than "this binary was built from commit C".

**`meta.json` no longer records the pin.** It carried a hand-maintained
`kernel-pin` and a `pin-bumps` list; those are the fields that went
stale, so they are gone and the file points here instead. The duty record
is `logs/pin-duty.log`, written by the wrapper:

```
<ts> segment-opened pin=<p> at=<ts> binary_sha256=<sha> harness_pin=<h> reason=<r>
<ts> segment-closed pin=<p> from=<ts> to_bound=<ts> bound=last-log-line
```

One `segment-opened` per start; the next start closes the previous one.
The close is a BOUND and says so: the wrapper `exec`s the daemon, so
nobody is standing beside it when it stops, and the latest moment it is
PROVEN alive is its last log line. The real end is at or after that.

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

| reason | means | the proof it rests on |
|---|---|---|
| `adopt` / `planned-start` | an operator start | the reason file, read — a report, not an inference |
| `boot-consistent` | the readings are CONSISTENT with the daemon having died with the host | a readable `kern.boottime` AND a coherent `run/jinnd.pid` record, boot strictly later |
| `keepalive-restart-consistent` | the readings are consistent with the opposite: the previous start belongs to THIS host boot, so the host did not reboot under the daemon | the same two readings, boot NOT later |
| `first-supervised-start` | no record of a previous start | `run/` was enumerated and held no `jinnd.pid` — a proven absence |
| `unknown` | the wrapper cannot prove why this start happened | — it derives nothing, and names the input it could not read |

**Two of those say `-consistent`, and none says `boot`.** A reboot is a fact
about the host; the wrapper's inputs are a sysctl reading and a file's mtime.
From those it can derive that the readings line up with a reboot, never that
one happened — and the gap stopped being pedantic when a record replaced
between the two looks with its mtime preserved produced `reason=boot` at rc 0
in ten runs out of ten (§Known limits). No care at the read site closes that:
`stat`-after-read proves *I read a pid and an mtime together*, never *this
mtime belongs to that pid*. The primitive cannot carry the claim, so the claim
was retired and the derivation is labelled as one.

**Every line carries the readings it rests on.** A label an auditor cannot see
through is still an oracle, so the inference is followed by `evidence:` and the
raw inputs — built once in the wrapper and printed by all three writers (the
dry run, the death line, the start line), so they cannot drift apart:

| field | what it is |
|---|---|
| `host_boot_sec` / `host_boot` | `kern.boottime` as read, and its UTC rendering |
| `prev_record` | `present` / `absent` (enumerated) / `unknown` (`run/` unenumerable) |
| `prev_pid` | the previous pid as read |
| `prev_start_sec` / `prev_start` | the record's mtime as read, and its UTC rendering |
| `prev_end_raw` / `prev_end` / `prev_end_clean` | launchd's `LastExitStatus` verbatim, and the one decode of it |
| `last_seen` | the daemon's final log timestamp — the duty gap's start |
| `unproven` | the inputs that could not be READ, or `none` |

`unproven` is computed before the decision, so it reads `none` on a proven
lane rather than appearing only once the answer is already `unknown`. A
provably absent record is not an unread one: absence lands in
`prev_record=absent`, never in `unproven`. So a wrong input on 2026-09-04 is
visible as a wrong input on the line beside the word, instead of hiding
inside it.

**A claim is derived from proof, never from the absence of a contradiction.**
Three times running, a missing or unreadable input degraded into a value
that made a positive claim true: a vanished stamp file wrote `boot` for a
SIGTERM on a host that had not rebooted; an unreadable `kern.boottime`
fell back to a zero epoch and would have written `host up since
1970-01-01T00:00:00Z`; a torn record — the pid read, then the file gone
before the `stat` — left the previous start at `0`, which makes
`boottime > prev_start` trivially true, and answered `boot` at rc 0. So
the default inverted (as jinnd M2-K9 inverted serial dispatch): each
input reads into either a value the wrapper can prove it read or the
literal `unknown` — no `0`, no empty string, no zero epoch, nothing a
comparison would mistake for a measurement. Both derivations rest on the
same two readings, so both need both sides proven; everything else,
imagined or not, is `unknown` by construction rather than by a guard
someone remembered to write. The pid
and the previous start's mtime are ONE record, proven as one: the record
is looked at twice with the read between, and a tear leaves both unknown.
`first-supervised-start` is earned by a proven absence — an enumerated
`run/` with nothing in it — and is never inferred from a read failure.

The wrapper `exec`s the daemon, so nobody is standing beside it when it
dies; for every unplanned reason it therefore writes the DEATH before the
start line, from what launchd retained and what the daemon last said:

```
<ts> previous jinnd <pid> <how it ended>; DERIVED <reason>: <the inference>. evidence: <the table above>
```

— or, when the provenance is unproven, `<how it ended>, PROVENANCE UNKNOWN
(could not read: <inputs>)`. The duty gap is `last_seen` → the new readiness
line.

**How it ended and `prev_end=` are one decode, so they cannot disagree.**
`launchctl list`'s `LastExitStatus` is a wait status (a signal in the low
seven bits, an exit code in the high byte); the wrapper decodes it ONCE into
a kind, and the field, `prev_end_clean`, and the phrase the line opens with
are all rendered from that — the phrase containing the field verbatim
(`ended UNCLEAN: killed by signal 15 (SIGTERM)`, `ended CLEANLY: exit 0`,
`ended, HOW UNKNOWN: end status unknown (launchd retained none)`).
Round 3 wrote that phrase as a literal beside a separately-decoded field, and
a real start with `LastExitStatus = 15` printed *"a daemon that ended on its
own"* beside `prev_end="killed by signal 15 (SIGTERM)"` on one line at rc 0 —
the same defect class as the three above, reached from the printing side: a
statement made without its proof beside it drifts from the proof. The raw
reading stays on the line, so a reader can re-decode it themselves. A signal
has no sender here, in either place — nothing retains one (§Known limits).

**The phrase says how the daemon ended, never whether it was asked to.** A
wait status carries no agency: `exit 0` is precisely what a planned §Stop
produces — an external SIGINT, handled, status 0 — so round 4's
`ended CLEANLY, on its own: exit 0` was false on the daemon's most ordinary
path, denying a signal that path always carries. The clause is deleted, not
softened: the wrapper reads no sender and no evidence that there was none,
and an unreadable fact gets no wording at all. Read agency, when you need it,
from the §Stop entry in this log beside the death line — an operator stop is
recorded there; nothing else is.

To see the decision without starting anything:
`SOAK_DRY_RUN=1 sh "$SOAK/bin/soak-run.sh"`. It prints `reason=<r>` and the
same evidence record the `ops.log` lines carry, and touches nothing at all —
against a runtime root that does not exist yet it therefore answers
`reason=unknown … unproven=run-directory previous-start-record` rather than
manufacturing the empty `run/` it would then have reasoned from.

### Known limits (what the supervisor does NOT defend against)

The threat model is an honest wrapper under ACCIDENTAL conditions: races,
missing files, unreadable sysctls, torn records. It is not a forger.

- **A previous-start record replaced between the wrapper's two looks with its
  mtime preserved reads as coherent**, and yields `boot-consistent` from a pid
  that is no longer readable — reproducible 10/10. `stat`-after-read cannot
  establish that an mtime belongs to a pid, so no version of this wrapper
  closes it. Reaching it requires a writer inside `$SOAK/run/` deliberately
  forging records, and that writer can edit `ops.log` directly, so defending
  the label against them buys nothing. What the evidence record buys instead
  is visibility: the forged reading is printed (`prev_start=2000-01-01…`),
  so the audit can see through the inference rather than inherit it.
- **The sender of a SIGTERM is not recorded anywhere**, because nothing
  retains it. `prev_end_raw`/`prev_end` say what the status was, never who
  caused it. The 2026-08-29T14:18Z end stays unattributed (§The +7d audit).
- **`LastExitStatus` is launchd's, not the record's.** It is the last status
  launchd retained FOR THE LABEL, and nothing ties it to the pid in
  `run/jinnd.pid`. In the ordinary supervised lane they are the same instance;
  across an operator `bootout`/`bootstrap`, or a status launchd has since
  dropped, they need not be. The wrapper prints both readings side by side
  (`prev_pid=` and `prev_end_raw=`) and never asserts they describe one
  process — the reason vocabulary rests on the boot time and the record's
  mtime, never on the status.
- **`last_seen` is the last timestamp in `jinnd.log`, not the moment of
  death.** It is a lower bound on when the daemon was alive; the interval
  between it and the death is unobserved by construction, since the wrapper
  is not running while the daemon is.

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
`entries` equal to the number of entries in the BOOTED PROFILE — seven at
the current composition, measured 2026-08-31. That count moves whenever the
profile does, so check it against the profile rather than against a number
written down here. Any `DOWN`, a stale last fire, or a
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
wrapper's `reason=keepalive-restart-consistent` (or `reason=boot-consistent`
when the readings line up with a host reboot) is the outage's own record —
opening with how the previous instance ended, decoded from launchd's retained
status. Annotate it when you next look — what died, and what the ledger shows
on the other side.

## Pin bump mid-soak

A pin bump during the soak is a planned soak event, observed end to end.
The procedure, for this bump and any future one:

1. Land the pin-bump commit per `KERNEL-PIN.md` (one commit, hashes +
   vendored surface + commit together).
2. Stop the soak (above).
3. Re-run Setup from the bumped repo (the daemon is down, so refreshing
   `$SOAK/bin` is safe; `record-build.sh` installs the new binary and
   rewrites `jinnd.build` as one step, so the digest and the pin cannot
   flip apart, and `install-launchd.sh` refreshes the wrapper if it changed — the plist
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

**Executed 2026-08-31 (seventh bump):** `57360cc` → `3a8e5c03` (jinnd
M2-K9 — the pin the harness SHIPS; harness packet
`packet/1.11-soak-pin-derived`, PLA-297). Motivated by a COO drift audit
rather than by a kernel change: the soak was accruing M2 §7(b) duty on a
kernel the milestone does not deliver, while three sources disagreed
about which kernel that even was (§What the record is). This bump is also
the FIRST EXERCISE of the derived record, and the two halves are in that
order deliberately — the record had to stop being hand-written before a
bump could be trusted to say what it had done.

Supervised, per this procedure: a clean SIGINT of daemon 11459 at
16:05:06.392Z reaching `quiescent; ledger flushed; bye` at 16:05:06.417Z
(exit 0, so `KeepAlive` left it stopped); `record-build.sh` installing the
pinned daemon and writing `bin/jinnd.build` as ONE step — the digest and
the pin cannot flip apart, and the hand-copied `bin/jinnd.commit` is gone;
`api-kit` rebuilt and the seven-entry profile regenerated with the
composition UNCHANGED (the cron pair + api trio + settings pair, the same
duty at the same cadence); `printf planned-start` + `kickstart`; readiness
line at 16:05:52.323Z. **Duty gap 45.9 s**, counted against the week per
the standing ruling, not against the restart that ended it.

The start line is the proof the derived record works, and it is worth
reading in full for the two fields that could not both appear before:

```
2026-08-31T16:05:32Z started (launchd; reason=planned-start): jinnd 21597
(pin 3a8e5c03fdbe2f21144faee8daba73beeb75d8b4) evidence: … prev_end="exit 0"
prev_end_clean=yes last_seen=2026-08-31T16:05:06.417671Z
binary_sha256=302881e61f1f647edf9cc4b27c3e4a172dea1f872625519536defbc5eb0d3d21
running_pin=3a8e5c03fdbe2f21144faee8daba73beeb75d8b4
harness_pin=3a8e5c03fdbe2f21144faee8daba73beeb75d8b4 unproven=none
```

The pin is there because the wrapper digested the binary it was about to
exec and `bin/jinnd.build` described that digest — not because a file
happened to hold the value. Before this bump the two pins were `57360cc`
and `3a8e5c03`, and no line anywhere printed both.

Post-bump evidence: the boot reconcile fired the 16:00:00Z boundary as a
catch-up at 16:05:52.4Z (`tick-seq 0`, `answers 1`) with its run record
and history append, all seven fibers `Active`, ledger 7179 → 7397 rows.
Duty was declared live only on the first UNASSISTED wake, at 16:20:52.5Z:
boundary 1788192900000, `tick-seq 1`, `answers 1`, ledger → 7419. The
alarm is period-from-wake, so that first post-boot wake lands one period
after the start rather than on the boundary.

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
onward, `boot-consistent`, `keepalive-restart-consistent` and `unknown`
counted as outages (and the pre-2026-08-30 `boot` / `keepalive-restart`
lines, which are the same lanes before the derivations were labelled as
such — each start line now carries `evidence:`, so a doubted reason can be
re-derived from the readings printed beside it)
(an end nobody could prove the cause of is still an end — its line names
the input that was unreadable, and that is the cue to go looking) and
`adopt`/`planned-start` as planned (the 2026-08-28 host reboot counts as
a crash, the clean stop/start cycle after the third bump and the
supervisor adoption as planned restarts) — zero
old-gateway interaction (the daemon touches nothing outside `$SOAK`), and
post-bump fire evidence: `AlarmWake` + `DispatchTrace` lines in the ledger
after the bump timestamp, with the per-tick ledger row cost compared before
(≈5 rows/tick of fiber churn) and after (1 `AlarmWake` per wake).
**Duty is reported PER PIN**, off `logs/pin-duty.log` rather than
re-derived from prose. The standing ruling (PLA-297, 2026-08-31): a
supervised pin bump does NOT reset the week — real production takes
upgrades, and a slice that cannot survive one is not doing a production
job; a GAP in duty counts against the week, not the restart that ends it;
and no single pin may carry the whole week and be reported as though the
current one did. The audit states which pins carried how much, and says
plainly that no single kernel carried seven days.

The audit report goes on the tracking Todo for the M2 acceptance decision.
