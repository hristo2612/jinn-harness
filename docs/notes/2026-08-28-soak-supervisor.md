# Note: why the soak's LaunchAgent uses a conditioned KeepAlive

*2026-08-28 · packet PLA-306 · rationale for a non-obvious choice.*

## The event

At 13:36:01Z the host rebooted. The soak daemon (pin `41cb2f47`) died with
it and stayed dead until a working session happened to look — `detach.py`
only detaches, nothing supervises. The outage was real duty lost, and worse,
it was SILENT: the +7d audit would have seen a gap with no record of what
made it. The fix is a supervisor; the design question is which one, and how
it coexists with the soak's own stop/start discipline.

## Why not a bare `KeepAlive`

`KeepAlive = true` relaunches the job on ANY exit. That is correct for the
reboot case and wrong for every planned one: SOAK.md §Stop is a SIGINT that
suspends the fibers, retains their fs state and flushes the ledger — a
first-class soak procedure used by every pin bump. Under a bare KeepAlive
each planned stop becomes a fight (stop → launchd restarts → stop again), or
forces the operator into `bootout` before every stop and `bootstrap` after,
which turns a two-line procedure into a four-line one and adds a new way to
get it wrong.

`KeepAlive = { SuccessfulExit: false }` — relaunch only after an UNCLEAN
exit — makes the daemon's own exit status the arbitration, and the daemon
already draws the line in exactly the right place:

- clean SIGINT → suspend, quiesce, flush, `exit(0)` → launchd leaves it down,
- failed flush barrier → `exit(1)` → relaunched (duty resumes; the ledger
  gap is on the record either way),
- `kill -9`, or a host that dies underneath → not a clean exit → relaunched.

So the planned lane needs no supervisor verbs at all: SIGINT stops,
`launchctl kickstart` starts. `bootout`/`bootstrap` are reserved for
retiring or installing the agent, which is what they mean. One lane
(`bootstrap`/`kickstart`/`bootout`), never mixed with `load`/`unload`.

Note the coupling this creates: the supervision contract now depends on the
daemon exiting 0 on a clean stop. If the kernel ever changes that, the
LaunchAgent silently stops honoring planned stops. That is a cheap price for
losing the fight-the-operator failure mode, but it belongs written down.

## Why a wrapper script and not `jinnd` directly

Three jobs the plist cannot do:

1. **No machine paths in the repo.** A plist does not expand `$HOME`, so a
   direct `ProgramArguments` would hard-code an absolute runtime root. The
   tracked plist is a template with one `__SOAK__` placeholder; the wrapper
   derives the root from `$HOME` and passes the absolute `--profile` and
   `--ledger` paths the watcher requires (FINDINGS.md #18).
2. **An honest `ops.log` line.** A supervisor start has to be as visible as
   an operator start, and it has to be DISTINGUISHABLE: the wrapper resolves
   a reason — `adopt`/`planned-start` (an operator dropped a reason file),
   `boot`, else `keepalive-restart` — so the audit counts outages apart
   from planned restarts without any human annotation. *(As shipped here,
   `boot` was decided from a stamp file, not from `kern.boottime`; that
   cost a false `reason=boot` on 2026-08-29 and was replaced — see
   `2026-08-29-soak-honest-relaunch.md`, which owns the decision from
   then on.)*
3. **Pid continuity.** The wrapper `exec`s the daemon, so jinnd inherits the
   wrapper's pid. `run/jinnd.pid` stays the daemon's own pid (the health
   check and §Stop are untouched), and launchd's `SuccessfulExit` keys on
   the daemon's status, not a shell's.

The reason heuristic is deliberately observational, not clever: it cannot
know why launchd relaunched, only whether the host boot changed and whether
an operator claimed the start. The claim made here — that both failure
directions are safe, an unclaimed planned start over-counting an outage
and never the reverse — was WRONG, and 2026-08-29 is the counterexample:
a crash restart was written as `boot`. The safe direction is only safe if
the boot test cannot silently fail open; see
`2026-08-29-soak-honest-relaunch.md`.

## Adoption without a double start

Installing the agent must not start a second daemon beside the live one, so
`install-launchd.sh` writes files and stops. The sequence is: install, clean
§Stop of the running daemon, drop the reason file, `bootstrap`. Then one
deliberate `kill -9` proves the KeepAlive leg — and it proves it on the real
soak ledger, which is the only place the claim is worth anything.
