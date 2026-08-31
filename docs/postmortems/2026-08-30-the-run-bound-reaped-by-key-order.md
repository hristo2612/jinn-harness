# A bounded registry reaped by key order, and a consumer read it as failure

*Found by the workflows seam's real-composition gate (phase 2.6). Fixed in
the same round. The first entry in this tier, so it also sets its shape:
what happened, why it survived, and the rule it becomes.*

## What happened

`tests/composition/tests/workflows.rs::a_definition_edited_mid_run_does_not_change_the_run_and_the_run_reports_its_revision`
went red on an assertion that had nothing to do with the pin. The run it
started after the edit came back `failed`, and its node's reason was:

```
node "work" ended failed: no run "default-16"
```

The pin assertions had all passed. What failed was four layers down.

## The mechanism

`jinn_engine::Runs` holds runs in a `BTreeMap<String, Live>` keyed by run
id, and a run id is `<engine>-<n>`. `retain_recent(keep)` — the bound that
keeps a provider's memory finite (jinnd R9) — collected the finished runs
by walking that map and dropped the first `excess` of them.

Walking a `BTreeMap<String, _>` is LEXICOGRAPHIC order. Past nine runs,
lexicographic and chronological order stop agreeing: `"echo-10"` sorts
before `"echo-9"`. So the bound reaped recent runs and kept much older
ones. Its own doc comment said "drops the oldest finished records"; the
code did not do that.

## Why it mattered, and why it is not cosmetic

The consequence is not a lost record. It is a **false `failed`**.

A consumer polls the engine's `run-get` for a run it started. If the
record has been reaped, the engine answers `no run <id>` — and there is
nothing in that answer to distinguish a record that was reaped from an id
that never existed. So the consumer takes the conservative branch and
records the work as `failed`. The work had SUCCEEDED.

That is a dangerous claim derived from the absence of a record rather than
from any evidence of failure — the exact class this program keeps paying
for, and this is the sixth instance.

## Why it survived three seams

Two reasons, and both are worth keeping.

**The test that guarded the bound used five runs.** `echo-1` through
`echo-5` are all single-digit, so lexicographic and chronological order
agree and the defect is invisible. A test that exercises an ordering has
to cross the boundary where the orderings diverge; five did not, twelve
does.

**The deeper the stack, the later the read.** A consumer one layer above
the engine reads a finished run within one poll period. The fourth layer
reads it three poll periods later, plus whatever the machine is doing —
and the measured cost of that stack is now on the record (`FINDINGS.md`
#35: 1084 ms at four layers against 513 ms at two). The reaping window is
fixed; the read moved. Adding a layer is what made a latent defect
reachable.

## The fix

Order the finished candidates by `Live::started_ms` — already recorded,
and simply unused — with the id only as a tie-break so the order is total
and a repeated call reaps the same records.

Red-first: `plugins/engines/jinn-engine/src/tests.rs::the_bound_drops_the_oldest_and_not_the_one_whose_id_sorts_first`,
twelve runs, asserting that the three most recent survive and the nine
oldest do not.

## The rules

1. **A collection keyed by a formatted id is not ordered by that id.**
   Whenever a bound, a page, a "latest" or an "oldest" reads from a map
   keyed by `<name>-<n>`, it must order by a recorded value, never by the
   key. A test for such an ordering must cross the digit boundary where
   the two orderings diverge — nine is not enough.
2. **An absent record is not evidence of failure.** A consumer that cannot
   tell "reaped" from "never existed" will manufacture the dangerous
   answer out of a gap. The open half of this defect is that
   `jinn:engine`'s `run-get` still cannot tell a consumer which of the two
   it is looking at; the fix above makes the reaping honest but does not
   give the consumer that word. That is a card for the engines seam, not
   something the workflows seam could close.
