# The catalog is the swappable part, and the swap had to be re-derived

*Phase 2.7, the plugins seam. Kernel pin `3a8e5c0`, NOT bumped.*

Three things in this round were not obvious, and one of them is a process
failure I am recording rather than smoothing over.

## 1. The swap the packet asked for did not exist

The acceptance was: *a provider is swapped THROUGH THE API — a real
profile patch, not a fixture file edit*. Every seam from 2.3 onward has a
swap proof, so the reasonable expectation was that this one would look
like the others and be driven over HTTP instead of over the filesystem.

It cannot. All six existing swap proofs change an entry's `package` and
`hash`, and `jinn:profile.patch-entry` applies its merge-patch to **the
entry's `config` subtree and nothing else**. `package` and `hash` are
siblings of `config`. The operator API can change what a plugin is
configured with; it can never change what a plugin IS.

That is a defensible confinement and I am not asking for it to be removed
— an editing plugin that could rewrite another entry's artifact hash is a
Law-1 side door. What it means is that the distribution's headline claim,
*a product is a profile and swapping a provider is a profile edit*, has
been true only for an operator with filesystem access to the document.
Through the surface a person or an agent actually uses, a provider swap is
not expressible at all **unless the seam was designed so that its binding
is decided by config**. `FINDINGS.md` #37.

So this seam was designed that way, deliberately and visibly: both catalog
providers read their catalog id from `config.data.catalog`, and both are
granted both catalog names up front. Two patches — park the incumbent,
then claim the name — move `jinn:plugins.main` between packages. The
ordering is not cosmetic: the kernel holds one provider slot per contract
name (#29), so claiming an occupied one refuses at `provide` and the
claimant fails its activation.

The cost is worth naming, because a future seam pays it too: a second
contract name reserved purely to park an incumbent, both providers granted
both names, and a two-call swap where the file edit is one. A seam that
does not think of this in advance simply has no API-driven swap.

**Why the "untouched" assertion is an incarnation number.** "The layer
above is untouched" is easy to fake — the API answering 200 after the swap
proves only that something is listening. The proof asserts that the API
entry's `incarnation` is EQUAL before and after, so it demonstrably did
not restart. It does not restart because it resolves a catalog contract
per request over the string-keyed lane rather than holding it as an
injection, which is the same reason the older seams' swaps left their API
alone.

## 2. The reason for a failed activation mostly does not exist

The acceptance said a failed activation reports **failed with a reason,
never `unknown`, never a default**. Reading the kernel at the pin split
that into two cases that look identical from outside:

- Pre-activation faults and broker refusals DO land prose —
  `ErrorRecorded { error: { code, message } }` and
  `GrantRefused { contract, reason, detail }`, entry-attributed.
- A guest's OWN activation failing — a trap, a panic, a deadline kill —
  lands nothing. The `KernelError` goes into `FiberRecord.failures`, and
  the bridge that feeds the ledger drains `transitions` and only
  `transitions`. `FiberRecord`'s own doc comment claims failures are the
  ledger's feed; half of it is not wired up. `FINDINGS.md` #38.

There was an obvious and wrong move available here: take the last refusal
that precedes the `→ Failed` transition on the same entry and call it the
reason. It would have been right most of the time, it would have made the
proof prettier, and it would have been a fabrication — `jinn:ledger` v0.1
records no causal parent, so nothing justifies the link. The seam answers
`failed` with `not-found-in-window` carrying the span it searched, and the
composition proof uses a failure the kernel really does record (a
`jinn:net` grant that admits one port beside a config that names another)
so that "with a reason" is proven with real prose rather than asserted
over the case where none exists.

> **CORRECTION, round 2 (2026-08-31).** The paragraph above is what round
> 1 INTENDED and not what round 1 shipped. `Catalog::entry` did exactly
> the thing it says it refused: it took the last reason-bearing line in
> the window and called it the reason, with no link at all, so an
> unrelated refusal from an earlier incarnation surfaced as a failed
> activation's cause. The verifier proved it and the reproduction is now
> a test. This paragraph is left standing rather than rewritten, because
> a note is a record of a round and the useful thing about this one is
> that its author believed it. See
> `docs/notes/2026-08-31-a-reason-is-not-a-neighbour.md` for the fix, and
> `FINDINGS.md` #38 for the corrected record.

## 3. Red-first was not satisfied by ordering, and the substitute is mutation

I wrote `lifecycle.rs`, `entry.rs` and `catalog.rs` before `tests.rs`.
That is a deviation from the standing TDD order and I am not going to
present the green run as if it had been red first.

What I did instead is a mutation check, and I think it is the stronger
evidence for this particular packet: I broke the implementation in the two
ways this seam exists to prevent and confirmed the tests catch them.

- Making an entry with no live incarnation read `Active`, and collapsing
  the loading arm so an owed change still reads `activating`, turns
  `active_is_the_only_reading_that_claims_the_plugin_is_serving`,
  `an_entry_with_no_live_fiber_reads_neither_active_nor_activating` and
  `a_loading_fiber_that_already_owes_a_change_is_never_eternally_activating`
  red.
- Dropping the attribution filter, and folding an unreadable document into
  an empty one, turns `a_history_holds_that_plugins_lines_and_only_its_own`,
  `an_unreadable_document_is_never_an_empty_catalog` and
  `describe_says_what_a_plugin_may_do_and_what_it_has_done` red.

Ordering proves a test could fail once. Mutation proves it fails on the
specific defect it is named for. Both would have been better.

## 4. A precondition caught a real mistake, which is the point of them

The disposal proof's first draft used the DISABLED entry as its subject:
remove it from the document, show its history survives. Its precondition —
*this plugin has lines before anything is removed* — failed, because a
plugin that never activated is charged no ledger line at all. Without that
assertion the test would have "passed" by comparing an empty history to an
empty history, and would have proven precisely nothing while looking
thorough.

The subject moved to an entry that ran, and the fact the precondition
found is now asserted in place rather than quietly edited out.
