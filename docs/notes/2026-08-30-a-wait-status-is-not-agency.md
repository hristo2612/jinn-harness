# A wait status is not a claim about agency

*PLA-297 round 5, 2026-08-30 — `tools/soak/soak-run.sh`.*

Round 4 collapsed the previous-end narrative and the `prev_end=` field into
one decode, so the prose could not contradict the field it stood beside. That
was the right fix for the defect it addressed, and it left one clause behind
that the collapse could not catch, because nothing in the wrapper disagreed
with it:

```
previous jinnd <pid> ended CLEANLY, on its own: exit 0
```

Every part of that line is rendered from a single decode of launchd's
retained wait status. The line is internally consistent. It is also false.

## Why it is false, not merely unprovable

The pinned daemon's planned stop (SOAK.md §Stop) is `kill -INT`: the daemon
installs a SIGINT handler, drains, flushes the ledger, and exits 0. So the
daemon's most ordinary clean end is *an externally signalled process that
exits 0*. The clause asserted the daemon was not signalled at precisely the
moment it always is.

The general shape is worth naming, because this packet has now found it four
times from four directions: **a wait status answers one question, and the
wrapper was reading a second answer out of it.** `wait` rc 0 proves the exit
*status*. Whether a signal prompted the exit is a different reading, taken
from a different input — and there is no such input here. launchd retains a
status, not a sender.

The verifier established the premise twice without consulting any of our
prose: the real-composition test `a_clean_shutdown_suspends_and_a_restart_
resumes_the_schedule` passes against the pinned daemon, and an independent
probe signalled a process externally and observed
`EXTERNALLY_SIGNALED_PROCESS_WAIT_RC=0`.

## The fix is a deletion

Not a hedge, not a guard, not a softer verb. `ended CLEANLY: exit 0`. The
same sweep removed `ended UNCLEAN, on its own: exit 3` — the identical
unearned clause on the non-zero-exit branch, where a signalled process that
exits non-zero is just as reachable.

An unreadable fact gets **no wording**, not a cautious one. A hedge
("probably", "apparently", "consistent with") still puts a claim on the line
that no input supports, and an auditor has to decide how much to discount it.
Silence about agency costs the reader nothing: the fields are all there, and
an operator stop is recorded by its own §Stop line in the same log. If the
agency reading is ever wanted, it arrives with its own evidence and earns its
own clause then.

## The proof carries its premise

`an_externally_signalled_exit_zero_is_never_narrated_as_agency` does not cite
this note or the daemon's source for its premise. It spawns a real process,
signals it externally, asserts it observed exit code 0, and only then requires
the wrapper to narrate that status without a claim about agency — so the test
fails if the premise ever stops being true, instead of passing on a stale
belief. `no_branch_of_the_exit_status_space_claims_agency` sweeps the whole
space (signal, exit 0, exit 3, SIGINT, no retained status) against a
vocabulary of agency words, so a future branch cannot reintroduce the class
under a new phrasing.

## What round 4 built and this round kept

One decode rendering the field, the phrase and the `prev_end_clean` token;
the narrative embedding the field verbatim, so a paraphrase fails the gate
even when it happens to be true; per-input inversion with `unknown` by
construction; the one-record read; the derivation labels
(`boot-consistent` / `keepalive-restart-consistent`); the verbatim evidence
record on every line. None of it moved.

## Stop rule

This is the last round of PLA-297's honesty work by COO ruling. Anything the
verifier finds after it becomes a named known limit in SOAK.md §Known limits
rather than another round — five rounds on the truthfulness of one log label
is already more than the label is worth against a milestone queue that is
waiting, and the wrapper as it stands is enormously more truthful than the
`reason=boot` it replaced.
