# The sessions seam's stores: what was decided, and what was falsified

Phase 2.4, round 2. The definition landed in round 1
(`2026-08-30-sessions-seam-definition.md`); this note is about the two
providers, the API surface, and the proofs — and about the two places the
round changed its mind.

## The verifier was right about the reader

Round 1 claimed, in the definition's own docs, that a replay could not
produce `TurnStatus::Running`. The WRITER refused it
(`Record::turn_ended`), and that was real. The READER had no such check:
a complete journal line holding `turn-ended { "status": "running" }`
decoded and replayed as a live turn. So the claim held only for documents
this seam wrote — a corrupted byte, a half-migrated log, or a future
version meaning something else by that tag was enough to hand back a
session eternally in flight that nothing would ever finish.

Both halves refuse it now. The general shape is worth stating: **a
writer's refusal is not a reader's guarantee.** Any invariant this
codebase enforces on the way out and relies on on the way in is only as
strong as the weaker end, and the weaker end is always the reader,
because the reader is the one holding bytes it did not write.

The same pass fixed a smaller version of the same thing: a terminal
record carrying no `reason` used to erase the conservative reason the
started turn already had. Absence of a reason is not proof that there was
none.

## Polling, not listening

The obvious design is for a store to LISTEN on `jinn:engine/event` and
update its turns as the deltas arrive. It is also the design that
deadlocks. A delivery on that topic arrives inside the ENGINE fiber's
dispatch; a store that then emitted its own session events would be
emitting from inside a call the engine is parked in — `FINDINGS.md` #4,
and #32 at this pin, which has the transcripts. Round 1's standing
expectation (b) says it directly: `Emit` is not an escape, because the
kernel awaits every listener delivery end-to-end in every mode.

So a store polls `run-get` on its own clock wake, and every bus record
minted while a caller is inside the guest is held until that wake. The
cost is one poll period of latency. That is a bound, and a bound is not a
defect; the alternative is a deadlock, which is not a bound.

## Restart honesty is an ordering, not a recovery pass

There is no crash-recovery sweep in this seam, and there should not be
one. The `turn-started` record is appended BEFORE any engine is asked for
anything, and a turn with no terminal record replays `interrupted` with a
reason — so the conservative answer is what an unfinished record MEANS,
not something a later pass repairs. A sweep would be a second opinion
about the same bytes, free to disagree with the reader.

Driving first and recording after would leave a window where a crash
loses the turn entirely. An absent turn is a WORSE lie than an
interrupted one: it says nothing happened.

## The proof was falsified before it was believed

All seven composition proofs passed on the first drive, which is weak
evidence — a test that has never been red does not discriminate. So the
`turn_started` journal write was replaced with a no-op and the restart
proof re-run. It went red, and the red was informative: the store refused
to ACTIVATE at all, because the surviving journal then held a
`turn-ended` for a turn that never started, and `adopt_all` refuses a
document it cannot replay. The recovery read failed with
`jinn:session.default has no live provider`.

That is the designed behaviour and it fails closed — a store that quietly
skipped a corrupt session would answer `list` short and nobody would
know. But it also names a real limit, now recorded in the seam README:
**one unreplayable document takes the whole store down**, not just the
damaged session, and the only recovery is an operator moving the file. A
per-document quarantine is the better shape and is not built.

## `store-core` is shared source on purpose

Each guest generates its own `wit_bindgen` bindings, so a normal library
crate cannot make host calls on a guest's behalf. Everything that is not
a host call already lives in the definition. What was left was identical
in both stores, and copying it would have been two homes for one fact
(AGENTS.md standing order 5) and two places for a defect to be fixed in
one of. One file, included as a module by both, with a README saying so.

The two providers now differ in exactly what they are supposed to differ
in: where the records live. `durable: false` is an authority fact as well
as a declaration — the ephemeral store is granted no `jinn:fs` at all.
