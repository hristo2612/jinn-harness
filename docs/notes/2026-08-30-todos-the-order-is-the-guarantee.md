# The order is the guarantee: a reported status needs a durable line under it

*Phase 2.5, the todos seam, round 2. Kernel pin `3a8e5c0`, unchanged.*

## What the verify round found

Withdraw one authority from the durable Todo store — `jinn:fs append`,
nothing else — and ask it to move a status. Round 1 answered:

```
UPDATE_HTTP=502
LIVE_STATUS="executing"          LIVE_HISTORY_LEN=1
JOURNAL_BYTE_IDENTICAL=true
REPLAYED_STATUS="backlog"        REPLAYED_HISTORY_LEN=0
```

Three answers to one question. The caller was told the move failed. The
store said it had happened. The journal said it had not, and a restart
agreed with the journal. Two views of one Todo, disagreeing about
whether work had started.

## Why the code did that

`Todos::update` applied the move and then answered it; the store
appended after. Every mutation in the seam had that shape, and two of
them tried to buy it back with compensations — `create` called `forget`
when its line did not land, `dispatch` ended a dispatch it had just
opened. Both worked. Neither was the fix, because a compensation is a
second chance to be wrong: it runs after the state has already moved,
and it can only run if the process is still alive to run it.

The seam's own README said the state is FOLDED from the log. It was
folded from the log on adoption, and kept beside it afterwards.

## The shape that fixes it

Every mutation is now two methods and one rule.

- `plan_*` computes what would happen and takes `&self`. It validates,
  mints ids, applies the status table, and touches nothing.
- `commit_*` folds a record that is *already durable* into the registry.

The store calls its journal between them, and commits only on `Ok`. The
ordering is not a convention a provider remembers — there is no method
on `Todos` that advances state and writes nothing, so the only way to
move the reported status is to have a record in hand.

What that buys is stated as an assertion rather than as a promise:

> The live view and the post-restart view of one Todo can never
> disagree, because the live one is derived by the same fold, from the
> same records, in the same order.

The composition proof reads all three answers the probe read —
the POST fails typed and names the write that refused, the store reports
the OLD status with an unchanged history over a byte-identical journal,
and a restart replays exactly what the live view was already saying. The
third is the one that binds; the first two can both be true while the
views still diverge.

`forget` and the compensating `end_dispatch` are deleted, along with the
windows they were patching. Deleting them was the point: a compensation
in the tree is evidence that the order is wrong somewhere.

## The transferable rule

This seam has now paid for the same mistake twice in two rounds, from
opposite directions.

Round 1: a status DERIVED and never written down is unusable the moment
someone acts on it (`2026-08-30-todos-the-fold-is-not-enough.md`).
Round 2: a status WRITTEN DOWN before the record that justifies it is
a claim nothing backs.

Both are the same law seen from two sides:

> The reported state and the durable record are one thing. Write the
> record, then derive the state from it. Never derive without writing,
> and never write the state before the record.

## The vendor leg

The card said the three-layer stack is proven "over echo and then a
vendor engine by changing only the binding". Round 1 ran it over two
in-repo providers and recorded the gap honestly in `FINDINGS.md`.
Recording it was right; shipping past it was not — two stand-ins prove a
binding swap, and never that the stack survives contact with a metered
CLI under its own authentication.

It now runs against a real one, gated by name on
`JINN_HARNESS_TODO_VENDOR_ENGINE` so it executes where a person asks for
it and self-skips in CI, the way the pinned-daemon gate self-skips
without a jinnd checkout. Two honesty conditions on the gate, both
mechanical:

- A skipped leg prints that it skipped and asserts nothing. There is no
  sentinel value that could be read as a run.
- An engine that is NAMED and not mounted FAILS. Absence answers a
  question the operator actually asked, so it is never quietly a skip.

The leg ran on this round's host and printed what it did:

```
VENDOR LEG RAN: engine "claude" answered a Todo dispatched through session store "default"; answer "OK"
```
