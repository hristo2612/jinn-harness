# A run is a positive reading, and a heal never writes a record

*Phase 2.6, round 2. Why `journal::replay` answers a type instead of a
struct, and why the fix landed in three seams rather than one.*

## What the defect actually was

The workflows run journal replayed a document holding **one torn byte**
into a default `Replayed`: `workflow-id: ""`, `revision: 0`, no nodes,
`status: running`. The store adopted it. The recovery then asked
`run_ending` how it should end, `run_ending` looked at an empty node set,
found that every node had reached `done` — vacuously — and answered
`Done`. Boot appended a `run-ended` line, and the API served
**HTTP 200, `status: "done"`** for a run that had never been started.

Two things went wrong and they are worth keeping apart, because a single
green test would have covered both and taught us the wrong lesson:

1. **The replay concluded from the absence of a contradiction.** Nothing
   in the document said this was not a run, so it was read as one.
2. **The heal synthesized.** A heal exists to drop bytes that were never a
   record. This one caused a record to be written into a document that
   held none.

## Why a type, and not a check

A guard clause (`if !opened { return Err(...) }`) fixes this document. It
does not fix the shape that produced it: a function whose success type
can represent something it never proved. `Replayed::default()` is a
sentinel — a value that is indistinguishable from a real reading and
therefore passes for one.

So `replay` answers `RunDocument::{Absent, Run}`, and `Absent` carries no
`Replayed` at all. There is nothing there to read a status off. The
compiler then went and found every caller for us: each one had to say, in
its own words, whether it was holding a proven run. That is the property
worth buying — not the guard, the *impossibility*.

`run_ending` got the same treatment one level down: over an empty set of
nodes it now answers `None`. A spec with no nodes is refused at `define`,
so this is the second lock on a door that is already shut — and it is the
lock that holds if a run is ever assembled from something other than a
definition, which is exactly what happened here.

## Why the fix reaches into 2.4 and 2.5

While writing the finding, the same replay in `jinn-todo` and
`jinn-session` turned out to have the identical shape: `Ok` with a
default `Replayed` for a record-less document. Todos would have installed
a Todo nobody created and then *recovered* it, writing a record into an
empty document — the same fabrication, one layer down.

Reporting that as a named limit and leaving it live would have been the
exact failure this program keeps paying for. Both now answer
`Option<Replayed>` and neither store adopts an absence. `Option` rather
than a third bespoke enum is deliberate: at those call sites there is one
absence reason, and inventing a third private vocabulary for it is the
duplication `FINDINGS.md` #36 is about.

## What is counted, and why

A store that declines to make a record out of a document says so:
`describe` reports `documents-without-a-record`. A `404` alone is the
absence of evidence; the counter is evidence of the absence. The
composition proof asserts both, in two separate tests — the API proof
never reads the disk, and the heal proof never asks the API — so neither
fault can hide behind the other's green.

## The part deliberately not built

Six seams hand-roll a durable replay and each has got a different part of
absence wrong. The shared typed replay outcome, and the typed negative
lookup answer that goes with it, are proposed in `FINDINGS.md` #36 and
are not built here. They touch all six seams and are their own card.
