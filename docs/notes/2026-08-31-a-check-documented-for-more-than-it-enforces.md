# A check documented for more than it enforces

*Round 3 of PLA-329. Three pieces of work, and one of them is the sharpest
finding the packet produced.*

## The defect, and why the harness did not see it

`checks::active_needs_positive_proof` documented two exclusions: an entry
reading `active` **without an incarnation**, or **while its live
incarnation already owes a change**. The predicate returned `Ok` after
the incarnation test. The second exclusion was documented and not
enforced — inside `mutants.rs`, the artifact built to stop exactly that.

The mutation sweep already had the rule that should have caught it: a
mutant no check catches is a hole, and *a check no mutant reaches is
unproven law*. It did not catch this, because the check WAS reached — by
the other half. `active_without_proof` strips the incarnation, the check
goes red, the sweep is satisfied, and a documented exclusion nobody
enforces sits underneath a green matrix.

So the sweep gained a third rule: a mutant must go red **for its own
reason**. Every `Mutant` now names the `evidence` its red message has to
carry, and a mutant caught by a neighbouring reason no longer counts as
caught. That is what makes "the check enforces its doc" checkable rather
than a thing a reviewer has to notice by reading.

## Enforcing the doc needed the evidence on the wire

The checks run over the SERIALIZED entry — the JSON an operator gets —
deliberately, so nothing passes by reaching into a private field. The
reading law says `active` needs three positive facts: the kernel said
`active`, an incarnation is installed, and the live incarnation owes
nothing. Two rode on the wire. The third did not, so the predicate
*could not* enforce its own doc: a defective reader answering `active`
while a change was owed produced bytes identical to an honest one.

The card allowed narrowing the doc instead. Narrowing would have made the
check honest and left the deeper thing in place: an answer whose central
claim rests on a fact its reader cannot check. The seam's own law is that
the evidence rides beside the claim — it is why `incarnation` is on the
wire at all. So `owes` joined it: `Option<Unserved>`, absent meaning "owes
nothing", which is a positive reading and the only shape `active` may
take. The predicate now enforces both halves and a consumer can verify
all three facts without trusting the reader on any of them.

Two mutants prove it, both on a new fixture — a `loading` fiber whose
incarnation already owes a restart — whose honest reading is `restarting`,
a rest. That matters: if the fixture's own honest reading were transient,
both defects would be the fixture's shape rather than an injected one.

## The three words nothing can produce

`mounted`, `activating` and `interrupted` name a fiber between two rests.
FINDINGS #41 measured that no consumer at this pin can be handed one: 189
reads across a real restart, all `active`, while the kernel's ledger
recorded the whole path. The acceptance line that asked for `interrupted`
through the real daemon was amended for that reason, not waived.

A README note would have left the seam shipping a vocabulary three of
whose words nothing produces — a claim that is right for a reason nothing
enforces. So the limit travels with the DEFINITION (`crate::pin`:
`UNREACHABLE_AT_PIN`, its qualifier, and the marking on each variant),
and `no-transient-reading-at-this-pin` is the canary: at this pin, an
answer that DELIVERS one of the three is itself a defect. A mutant
produces one, so the canary is proven non-vacuous in the harness rather
than passing because nothing ever supplies the input. The composition
proof reads the marking from the definition instead of a literal, so the
vocabulary's limit and the measured limit cannot drift apart.

The day the kernel gains a publish path, that check goes red and the
reading law gets re-read. Today there was no answer at all to "would I
notice the day this stops holding?"

The canary asks exactly one question — is this word on the unreachable
list — and not "is this a reading at all", which is a different defect
with its own check. Folding the two together would have let either pass
for the other, and it would have made the canary look reached when only
the sentinel half was.

## Why the canary is not a check on the kernel

It is a check on OUR answer. The kernel is free to pass through all three
states; it does. What the canary refuses is this seam DELIVERING one,
which at this pin can only happen if the reading law was mis-implemented
or the kernel's read surface changed under it. Both are things we want to
hear about immediately, and neither is a defect in the plugin being read.

## Files

`mutants.rs` crossed the 300-line `src/` cap once it held eight mutants,
so the defect catalogue split into `defects.rs` — the defective readings
and the fixtures they are reachable on — leaving `mutants.rs` the harness
that measures them. Two files, two questions: what went wrong once, and
how we prove a check would catch it.
