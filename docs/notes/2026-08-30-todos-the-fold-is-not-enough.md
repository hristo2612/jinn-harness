# The fold was not enough, and the composition gate is what said so

*Phase 2.5, the todos seam. Kernel pin `3a8e5c0`, unchanged.*

## What was built, and what was assumed

The card asked for a Todo whose interrupted dispatch "comes back recorded
as interrupted WITH A REASON, never eternally `executing`". The sessions
seam had already solved the shape of that one layer down: a status is
DERIVED from the records rather than stored beside them, so it cannot
drift from what it describes. `SessionStatus` is never written; it is
computed from the turns every time it is asked for.

So the todos seam did the same thing. `reported_status` folds an
`executing` Todo whose last dispatch replayed `interrupted` onto
`blocked`, carrying the dispatch's reason, and leaves the journal
completely untouched. Two named fields on the record — `declared-status`
for what history says, `status` for what the store reports — and neither
one lies. The unit proofs were green. It looked finished.

## What the daemon said

The real-composition proof killed a daemon mid-dispatch, rebooted it, and
then did the thing an operator would do next: it read the Todo (which
said `blocked`), and asked to move it `blocked -> executing` so the work
could resume.

The ledger refused. `502`.

The reason is obvious in hindsight and was invisible from inside the unit
tests: `update` applies the table to the DECLARED status, because that is
what the journal will have to agree with on the next replay. The declared
status was still `executing`. So the operator was shown a status, offered
the moves that status admits, and refused all of them. The Todo was
readable, honest, and completely stuck.

## Why the sessions seam does not have this problem

Because a session status is not something anyone MOVES. It is a summary of
turns, and the only way to change it is to take a turn. There is no
operation whose argument is "the status I think this is in", so there is
nothing for a derived status to disagree with.

A Todo's status is the opposite: it is the thing operators act on
directly, and every action names both ends of the move. The moment a
status is an ARGUMENT and not just an answer, deriving it creates a
second version of it — and the caller is holding the wrong one.

That is the transferable lesson, and it is narrower than "never derive":

> A derived status is safe exactly while nothing takes it as input.
> The instant an operation is parameterised by the status, the
> derivation has to be written down before anyone can act on it.

## What was done instead

`Todos::recover` applies the fold as a REAL move, through the same table
as every other move, and the durable store journals it during adoption.
The recovery is an ordinary `status-changed` line — a new event appended
after the ones already there, never an edit of one — carrying the
dispatch's reason as its note and no actor, because nobody asked for it.

The append-only claim is not weakened by this; it is what makes it work.
The move that started the work is still on the record, exactly as it was
written. An operator reading the history sees both facts in order: the
work was started, and then the daemon died on it. Rewriting the first
line to say `blocked` would have destroyed the evidence that the work was
ever underway, which is the failure mode the fold existed to avoid.

The fold stays, because it is the by-construction guarantee: if the
recovery line cannot be written the activation fails closed, and until
the line lands the record still refuses to claim `executing`. Belt and
braces, and each one says what it is for.

## The other thing the gate found

The same proof exposed a defect in the tear-tolerance the definition had
already unit-proven. The reader admits an unterminated last line as
absence — correct, and tested. But the next `append` lands on the END of
that partial line, so the tear and the new record fuse into one
undecodable line in the MIDDLE of the document, and the next boot refuses
to replay it at all. Tolerating a tear on read and then writing past it
turns a recoverable state into an unrecoverable one.

The store now heals the document on adoption: a replay reporting
`torn_tail_bytes > 0` is followed by a rewrite of the whole prefix, and
`describe` reports `healed-tails` so bytes are never dropped in silence.
No record is lost — by the reader's own law, those bytes were never a
record. It is a full rewrite of an append-only document, which is the
wrong shape for the smallest possible repair, and `FINDINGS.md` #34 names
the `jinn:fs` operation that would fix that.

## Why this is a note and not a postmortem

Nothing shipped broken. Both defects were found by the gate that exists to
find them, in the round that introduced them, before anything was landed —
which is the system working. What is worth keeping is the *shape* of the
first one, because the distribution is going to keep composing seams and
the temptation to reuse a lower seam's pattern wholesale will keep
recurring. The sessions seam's derivation discipline was right for
sessions and wrong here, and the difference is one sentence long.
