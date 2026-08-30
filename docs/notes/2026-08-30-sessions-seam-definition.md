# The sessions seam's definition, and what pin-bump 8 actually bought

*Phase 2.4, packet `2.4-sessions`, PLA-317. Pin `3fd7b05` → `3a8e5c0`
(jinnd M2-K9).*

## Pin-bump 8: #31 is closed, and the test is still red

The card said: adopt M2-K9, retire the `#[ignore]` the 2.3 packet carried
for `the_shadowed_refusals_recovery_lands_when_executed`, close
`FINDINGS.md` #31, and remove the settings provider's layer-knowledge
workaround if the kernel made it unnecessary.

Two of those are done and one is not, and the difference is worth writing
down because it is the shape this program keeps paying for.

**#31 is genuinely closed**, and not by inference. The bump makes the
exact call that used to die answer: at `3fd7b05` the entry-layer patch's
`PATCH /v1/settings/cron` never reached a response; at `3a8e5c0` the same
call writes its response and closes its socket (ledger seq 223 →
236/237), the scheduler restarts, and the successor re-declares. That is
positive evidence at the exact site, not "the suite went green".

**The test still fails.** It fails LATER, at a different call, for a
different reason: `FINDINGS.md` #32 — the namespace's owner re-declares
on every alarm wake, which is a call into the provider that is at that
moment dispatching a `changed` notice into the owner; each parks on the
other and both die on the guest deadline. #31's stall used to kill the
test before that collision could happen, so closing #31 did not make the
test pass, it made the NEXT defect reachable.

It would have been easy, and wrong, to read the persistent red as "#31 is
not fixed" and re-file it. It would have been easier and worse to read
the ledger's early success as "the bump works" and mark the test green.
The claim came from the transcript at the call site, and it splits into
two claims because the evidence does.

## The card asked whether the workaround could go. It cannot, and the reason changed

The settings provider picks its dispatch mode from the layer it just
wrote: `Serial` on the hot path, `Emit` on the restart path. #31 called
that a workaround and insufficient. At the new pin it is neither
load-bearing nor removable:

- It is no longer a shield. A `Serial` aimed at a fiber that owes a
  restart is now REFUSED typed, so being wrong about the mode costs a
  refusal rather than a hang.
- It is still the right code. `Emit` on the restart path is the correct
  notice on its own merits — the successor re-declares on its own wake
  and has nothing to answer with.
- And it never protected against #32 anyway. Run `36182` deadlocked on
  that very `Emit`: the kernel awaits every listener delivery end-to-end
  in every mode, so fire-and-forget discards the ANSWER, never the WAIT.

So the code stays and the comment above it changes: it is a semantic
choice now, not a hazard shield. Deleting it to satisfy the card's
"remove the workaround if you can" would have removed correct code for a
reason that had stopped being true.

## Two transcripts, and why the second one mattered

The first reproduction caught the deadlock on the hot path and showed
something alarming: the deadlocked fiber never recovered — `AlarmWake`
and `the instance is gone`, every 250 ms, forever. The natural finding to
write was "a fiber that loses this deadlock is dead and writes to the
ledger until the daemon stops".

The second reproduction caught the same deadlock on the RESTART path, and
the fiber recovered. Not because the kernel healed it: because the patch
happened to be aimed at that fiber's own entry, so the loader already
owed it a restart and rebuilt it on the way past. The honest finding is
therefore weaker in one place and sharper in another — whether the loser
comes back is INCIDENTAL, and depends on whether something unrelated was
already restarting it. One transcript would have shipped a claim that is
true in one case and false in the other.

## The definition: where the honesty lives

`jinn-session` is the seam's definition, and the interesting part is the
journal, because a journal is what a store has after a crash and a crash
is when a system is most tempted to lie.

The design rule is that the DANGEROUS answer needs proof:

- `Record::turn_ended` REFUSES a non-terminal status, so `running` is
  never written to disk at all.
- `replay` opens every `turn-started` as `interrupted` with a reason;
  only a terminal record can move it. There is no branch that decides an
  unfinished turn's fate — the conservative answer is the initial value,
  and the dangerous one requires a record.
- The result: a replayed session cannot report a live turn, whatever the
  file says, because the type that would say so cannot be produced from
  the file.

The tear rule is the same idea at the byte level. A trailing unterminated
line is a torn TAIL and reads as ABSENCE — the half-written turn is
simply not there, which is what "absent or complete" means. An
undecodable line anywhere EARLIER is a hole, not a tear, and is REFUSED:
answering the two the same way would let real corruption pass for a clean
stop, which is exactly the sentinel-that-passes-for-a-reading failure the
program has already paid for twice.

The kernel's `jinn:fs` `append` commits whole-document atomically, so a
tear should be unreachable through that path. The reader does not rely on
that. The guarantee belongs to a contract this seam does not own, and a
reader that trusts it has no answer the day it changes.

## One home for the wire law

`jinn-session` is the third seam to need the additivity law, and the law
had two homes: `closed` (the refusal a CLOSED surface owes) in
`jinn-settings`, and `Additive` + the decode/encode halves + the macros in
`jinn-engine`, which happened to declare them first. A third borrower
would have had to pick a half or copy one.

Both halves now live in `jinn_settings::wire` and `jinn-engine`
re-exports them, so nothing else in the tree moved and the engines seam's
additivity property suite is the unchanged guard on the refactor.

The sessions seam proves the law differently on purpose: its inventory is
small enough to plant an unknown key at EVERY object node of every
canonical document, deterministically, rather than sample placements with
a seeded generator. The law has one home; a proof of it may be sharper
where the shape allows.

## What this round did not do

The seam's providers (`jinn-session-fs`, `jinn-session-memory`), the API
routes, the session kit and the real-composition proofs — including the
mid-turn daemon kill that is this packet's restart-honesty acceptance —
are NOT in this round. The definition is the shape they all build on and
it is landed, tested and documented; the rest is the next round's work,
and the packet report says so rather than implying otherwise.
