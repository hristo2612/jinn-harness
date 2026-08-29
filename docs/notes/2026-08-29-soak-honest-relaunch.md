# Note: the supervisor's account of a death has to be checkable

*2026-08-29 · packet PLA-297 (harness 1.10) · rationale for a non-obvious choice.*

## The event

`ops.log` recorded `2026-08-29T14:26:41Z started (launchd; reason=boot): jinnd
11459`. There was no boot. `kern.boottime` had been 2026-08-28T13:36:00Z the
whole time; jinnd 75738 stopped after a normal fire at 14:18:19Z, said
nothing, and launchd's KeepAlive replaced it. The supervisor did its job
perfectly. The *record* it wrote was false, and false in the direction that
costs the most: on 2026-09-04 the +7d audit would have counted a killed
daemon as a routine restart and reported an unbroken week of duty.

## Why the old decision could not be trusted

The wrapper decided `boot` by comparing `sysctl kern.boottime` against a
stamp it had written itself at `run/launchd.hostboot`. Absence of the stamp
was read as "the host booted". Something removed the stamp between 03:32:59Z
and 14:26Z — who is unproven and stays unproven — so the test failed OPEN, and
failing open on this test manufactures a boot out of nothing.

The defect is not the missing file. It is that the evidence for a claim about
the *host* was a scratch file in the daemon's own runtime root, which anything
with write access can remove and which no reader can distinguish from "never
written". A supervisor's claims must key on state it does not own.

## What replaces it

- **Boot is decided from uptime.** `boot` iff `kern.boottime` is later than
  the previous start, and the previous start is the mtime of
  `run/jinnd.pid` — a file the wrapper writes at every start for its own
  reasons, so no new bookkeeping exists to go missing. No prior pid at all is
  `first-supervised-start`: provenance unknown, never dressed up as a boot.
- **When the evidence itself is unreadable, the record says so.** If
  `kern.boottime` cannot be parsed, `host_boot=unknown` and no boot is
  claimed. The obvious fallback — treat the boot time as the zero epoch — is
  worse than useless, because `date -r 0` prints `1970-01-01T00:00:00Z` and
  the death line then asserts a boot time nobody measured. That is the same
  defect one level down, and the gate now holds the line against it.
- **The death is recorded, not only the recovery.** The wrapper `exec`s the
  daemon, so nobody is standing beside it when it dies; the next start is the
  earliest honest moment. Before the start line it writes what launchd
  retained (`LastExitStatus`, decoded as a wait status — signal in the low
  bits, exit code in the high byte) and what the daemon last said (the final
  timestamp in `jinnd.log`, which bounds the duty gap). An audit now reads the
  death, then the recovery, in that order.
- **`SOAK_DRY_RUN=1` prints the decision and touches nothing** — including the
  operator's reason file, which a dry run must not consume. That promise was
  at first only mostly true: the wrapper created `logs/` and `run/` under the
  supplied root before deciding, which on a root that did not exist was both a
  write and the manufacture of the empty directory it then reasoned from. The
  `mkdir` now happens only on a real start.

## Three of the same defect, and the inversion (2026-08-30)

Patching the degradation paths one at a time failed three times:

1. the vanished stamp file above;
2. an unreadable `kern.boottime` falling back to a zero epoch, which would
   have written `host up since 1970-01-01T00:00:00Z` — found in the
   builder's own review;
3. a torn previous-start record — the pid reads, then the file is gone
   before the `stat` — leaving `prev_start=0`, which makes
   `boottime > prev_start` trivially true. The verifier proved it with a
   FIFO probe that returned `reason=boot prev_pid=75738` at rc 0.

Same shape every time: **a missing or unreadable input degrading into a
value that makes a positive claim true.** Three instances is not three
bugs, it is one design; a fourth patch would have been the same mistake
again. So the default inverts, exactly as jinnd M2-K9 inverted serial
dispatch — there, a fiber that owes a change refuses everything it cannot
prove safe rather than enumerating what is unsafe.

Each input now reads into either a value the wrapper can prove it read or
the literal `unknown`. There is no sentinel a comparison could mistake for
a measurement: no `0`, no empty string, no zero epoch. `boot` needs proof
from BOTH sides — a readable host boot time and a coherent previous-start
record, boot strictly later — and `keepalive-restart`, which claims the
host did *not* reboot, needs exactly the same two readings. Everything
else falls through to `reason=unknown`, whose `ops.log` line says
`PROVENANCE UNKNOWN` and names the input it could not read.

Two details worth keeping:

- **The pid and its mtime are one record, proven as one.** Reading them as
  two independent facts is what let a pid from a file that no longer
  existed pair with a default mtime of `0`. The record is looked at twice
  with the read between; a tear — gone by the second look, or replaced
  between them — leaves both unknown, because a pid from one record and an
  mtime from another was never a previous start.
- **`first-supervised-start` is earned by a proven absence.** `run/` is
  enumerated; what is not in a directory the wrapper could list is
  provably not there. It is never inferred from a read failure, because
  "I could not look" and "there is nothing there" are different facts and
  only one of them is evidence.

The gate is one red test per input: make the host boot time unreadable,
make the previous pid unreadable, tear the previous-start record (and the
mirror case, replace it between the two looks) — and in each case assert
the wrapper claims nothing. The verifier's FIFO probe, re-run 20 times
against the new wrapper, answers `reason=unknown
unproven=previous-start-record` every time; against the reviewed head it
answered `reason=boot` 5 times out of 5.

The law this leaves behind, for every oracle we write after it: **a claim
is derived from proof, never from the absence of a contradiction.**

## What is deliberately NOT claimed

The sender of the SIGTERM is unknown. jetsam is ruled out by the signal
itself (memory-pressure kills are SIGKILL/9, this was 15). No `pkill` or
`killall` of jinnd exists in any transcript on the box, and the unified log
retained nothing for the window. No document, comment or commit in this
packet names a cause, and none should until there is evidence: the packet is
about making the record honest, and a plausible attribution written down is
the very thing it exists to prevent. If a second unplanned end appears before
the +7d audit, the new death line is the first thing to read.

## Why the soak was not restarted to adopt this

The wrapper is installed at the runtime root and takes effect at the next
natural start; jinnd 11459 was left alone. A fix for a discontinuity-recording
bug must not create a discontinuity to install itself.

## The gate

`tools/harness-pin/tests/soak_supervisor.rs` drives the wrapper in dry-run
mode over scratch roots with stub `launchctl`, `sysctl` and `stat` binaries:
every reason branch, the decoded previous end, the last-seen bound, the
reason file surviving a dry run, one degradation case per input, and the
construction itself (no read may fall back to a value that looks measured).
It earned its keep twice before shipping — catching a greedy `sec =` parse
that matched `usec`, and holding the `boot` and `keepalive-restart` lanes
green through the inversion, so honesty did not cost the wrapper its
answers.
