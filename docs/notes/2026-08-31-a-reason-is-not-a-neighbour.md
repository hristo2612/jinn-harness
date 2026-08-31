# A reason is not a neighbour, and a mutation harness is not a note

*Round 2 of the plugins seam (PLA-329). What the verifier found, what it
cost to fix properly, and the two things this round learned that the next
seam should not have to learn again.*

## The defect, and why it was the worst kind

Round 1's catalog answered a failed activation's `reason` with the last
reason-bearing ledger line in its window. There was no link of any kind
between the two — not an incarnation, not a span, not a causal column,
because `jinn:ledger` v0.1 has none. The verifier's reproduction is now a
test: an unrelated `GrantRefused` written by an EARLIER incarnation of the
same entry came back as this activation's cause, complete with the
kernel's own prose.

Every other instance of the absence class this port has met misreported an
ABSENCE — a missing record read as an empty one, a sentinel standing in
for a reading that never happened. This one is different in kind. It
attached a real, plausible, false cause. An operator reading it would have
had no reason to doubt it, and a wrong answer that looks like evidence is
harder to catch than a missing one. The entry in `FINDINGS.md` #38 had
even written down the rule it broke — *do not correlate the failure with
whatever refusal happens to precede the `→ Failed` transition* — and then
described the seam as though it followed it. The record was false, not
just the code, which is why round 2 corrected the entry as well.

## Why the fix is a deleted variant and not a better filter

The tempting fix is a narrower filter: cite the line only if it falls
inside the failing incarnation's span, delimited by the kernel's own
committed transitions. It is a much better heuristic. It is still a
heuristic — a refusal a guest recovered from sits inside that span too —
and, worse, it is the kind of thing a later edit loosens by one condition
without anyone noticing.

So `Reason::Ledgered` is gone. `Reason::NotFoundInWindow` went with it,
because "searched and not there" is a lie when the window holds three
refusals the answer chose not to cite. What is left is
`Reason::NoRecordedCause`: the window that was read, a COUNT of the
reason-bearing lines the answer declines to cite, and a qualifier saying
why, in the answer the consumer reads. `History::last_reason` was replaced
by `History::reason_bearing`, a count with no accessor for the line at
all. The fabrication is now unrepresentable rather than unreached, and the
prose an operator wants is exactly one call away in `history(id)`, where
it is that entry's history and not a cause.

## Ordering was missed; mutation is what stands in its place

Round 1 wrote the types before the tests and said so. That cannot be
un-missed. Round 2's substitute is `checks.rs` + `mutants.rs`: each
honesty property is a named predicate over the SERIALIZED answer, and six
named defects — the round-1 fabrication restored verbatim, reading a real
ledger page — are run through them, with the sweep failing on a mutant
nothing catches AND on a check no mutant reaches. Ordering proves a test
could fail; this proves each test fails on the defect it is named after.

The point that made it worth building rather than describing: it is the
SAME predicates that run against the real daemon's answers in
`plugins_lifecycle.rs`. A mutation check in a document is a claim about
tests; one that runs is a property of them. And it found a real gap while
being written — `active` was a conclusion on the wire with its evidence
left behind, so `Entry.incarnation` now rides beside it and a consumer can
check the claim instead of trusting it.

## What the daemon could not be made to say

The packet asked for `mounted-never-activated` and `killed-mid-effect →
interrupted` proven through the real pinned daemon, and warned against
assuming this layer inherits anything. It does not, and the reason turned
out to be a kernel gap rather than a seam shortcut.

Those two readings describe a fiber BETWEEN two rests. The kernel passes
through them — its ledger records `Active → Unloading → Pending → Loading
→ Active` for a real config-driven restart — and no consumer can see them,
because `jinn:introspect` is a pull surface and nothing pushes. 190
consecutive catalog reads across that exact restart returned `active`,
every one. A WASM unload-and-reload finishes well inside a single HTTP
read; there is no polling rate that wins.

So the round did three things instead of one. It measured the gap and
filed it (#41), with its cause (#40: there is no lifecycle event surface
at all). It proved the two readings on the kernel's OWN recorded state
words from that run's ledger, with the join exercised in the test process,
and said plainly that this is what it is rather than dressing it as a
composition proof. And it added the durable shape of never-activated that
IS reachable — an entry whose artifact hash the machine refuses, mounted
in the document and never once activated — which also exercises the
non-disabled dark path, the second place the round-1 fabrication lived.

## The typed events the seam does not ship

The acceptance asked the definition for typed events. Building them
established that there is nothing truthful to emit: a catalog knows the
composition at the instant it asks and nothing between two asks, so a
`PluginLifecycleChanged` could only be produced by a poller comparing two
snapshots — announcing, as an event, a transition it did not witness and
cannot time. That is this seam's own fabrication class one layer up. The
seam ships no event type and #40 is the recorded reason, so the absence is
a decision with evidence rather than an omission.
