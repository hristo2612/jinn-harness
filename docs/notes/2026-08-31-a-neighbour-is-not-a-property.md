# A neighbour is not a property

*PLA-297, packet 1.11. Why the soak's pin is now a digest join, and why the
record that failed could not have been fixed by being more careful with it.*

## What failed

Three sources disagreed about which kernel the M2 duty soak was running:

| source | said |
|---|---|
| `meta.json` — the artifact the +7d audit reads | `41cb2f47` |
| the installed binary and `ops.log` | `57360cc` |
| `KERNEL-PIN.md` — what M2 ships | `3a8e5c03` |

A third pin bump happened around 08-29 and was never written to the file
whose only job is to record exactly that. The audit on 09-04 would have
reported the week's duty against a kernel that stopped running two days
earlier, and it would have been believed, because `meta.json` is the
artifact you would read to answer the question.

No gate caught it. More usefully: no gate *could* have. Nothing was
damaged, nothing was inconsistent, nothing was unparseable. The record was
STALE, and staleness has no local signature. Every reading was internally
coherent; they only disagreed with each other, and nothing was reading two
of them at once.

## The shape of the defect, stated generally

The pin reached the soak as a NEIGHBOUR of the binary — `jinnd.commit`
copied into the same directory — and as a hand-kept field in a second file
entirely. Two files sitting in one directory make no claim about each
other. Neither does a person's memory of what they last installed.

So the failure mode is not "someone forgot". It is that forgetting was
undetectable. A record that is adjacent to its subject can be replaced,
skipped, or left behind, and the result is a directory that looks exactly
like a correct one.

This is the same class the wrapper spent six rounds closing one level in —
`reason=boot` derived from a scratch stamp that could vanish, a zero epoch
standing in for an unread `sysctl`, a torn record whose missing mtime made
a comparison trivially true. Each time, the fix was the same inversion: a
claim is derived from proof, never from the absence of a contradiction.
Round 7 is that law applied to the wrapper's account of ITSELF. It had
learned to tell the truth about how it started while nothing was telling
the truth about what it was running.

## What replaced it

The record is bound to its subject by content. `record-build.sh` digests
the bytes it installs and writes that digest into the record beside the
pin; `soak-run.sh` re-digests the binary it is about to exec and accepts
the pin only where the two agree.

The stale case now has a signature. A record left by an earlier install
describes a different binary, the digests differ, and the wrapper answers
`running_pin=unknown` with `build-record-mismatch` named — an answer an
auditor can see, rather than a plausible wrong one they cannot.

Note what did NOT change: the record can still go unwritten. What changed
is that an unwritten record is now visible as one. The goal was never to
make the mistake impossible; it was to make it loud.

## The two pins

`running_pin` and `harness_pin` are two fields because they are two
questions. What the soak IS running and what `KERNEL-PIN.md` says it
SHOULD be are not degraded versions of one another — the distance between
them is precisely what the drift audit measured, and it was measurable
only because someone held all three readings side by side by hand.

So the harness pin never fills the running one, including in the case
where that would be most tempting: the running pin unprovable and the
harness pin sitting right there, readable, in the same file. It is an
answer to a different question, and a confident answer to the wrong
question is what this packet exists to delete.

## What the join can and cannot prove

It proves *this binary is the one some install recorded as built from
commit C*. It cannot prove *this binary was built from commit C*, because
nothing in the artifact says so: `jinnd` has no `--version`, its stdin
protocol is `revert`/`status`, and the sole 40-hex literal in 62 MB of
binary belongs to a dependency. That is `FINDINGS.md` #42, and the shape
that retires it is the kernel answering for itself — the commit and the
two contract hashes compiled in, reported on demand and on the readiness
line.

Until then every consumer that wants this answer re-implements the same
bookkeeping and can get it wrong differently, which is #36's generator one
repo out. The harness owns the bookkeeping for exactly one deployment; a
daemon installed any other way still has no answer at all, and the record
says `unknown` rather than pretending otherwise.

## Duty per pin, and why an end is a bound

The +7d audit reports duty PER PIN, so the segments are their own
append-only record (`logs/pin-duty.log`) instead of something re-derived
from prose at audit time.

Each start opens a segment and closes the previous one. The close carries
`bound=last-log-line`, because the wrapper `exec`s the daemon: nobody is
standing beside it when it stops, and the latest moment it is PROVEN alive
is its last log line. The real end is at or after that. The alternative —
closing the segment at the next start's timestamp — would silently credit
the outage to the pin that was already dead. Same discipline as the death
line above it: the derivation is labelled, and the reading it rests on is
printed beside it.
