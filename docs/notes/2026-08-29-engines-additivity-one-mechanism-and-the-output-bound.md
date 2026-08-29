# Additivity became a mechanism, and the output budget became a bound

*2026-08-29 — the engines seam (`plugins/engines/`), round 4 of the 2.3
packet.*

Two review rounds in a row found the same class of defect in this seam's
wire types, and one found a budget that was counted after the bytes it
was supposed to bound had already moved. Both fixes are structural: the
point of this note is why neither was fixed where it was found.

## Additivity: two instances mean the algorithm was under-defined

Round 2 found nested records (`ToolPolicy`, `Budget`, `Capabilities`,
`Usage`) dropping the fields a newer peer sends. That was fixed by giving
each of them a flattened extension map. Round 3 then found KNOWN event
variants dropping them: `Event::from_map` read the fields its version
understood and discarded the remainder, so a `delta` carrying
`reasoning-tokens: 7` lost it on the hop.

The second finding is the interesting one. The first fix was correct and
insufficient in the same way: it fixed the places someone had thought to
look. A seam whose additivity is a per-type decision has as many chances
to forget as it has types, and it had already forgotten twice.

So the law is now written in exactly one place (the definition's module
doc, restated in its README), and implemented in exactly one:

- Every wire type carries a rest map named `extra`.
- Derived types get it from serde's `flatten`.
- `Event` and `Answer` — whose tag serde will not derive a flattened map
  beside — get it from `decode_with_rest` / `encode_with_rest`, a shared
  pair that IS the law: the decoder removes the fields it knows and hands
  back the remainder; the encoder writes the known fields and re-emits
  the remainder unchanged. Because decoding removes what it reads, the
  two halves can never hold the same key, so neither can clobber the
  other.

The shape change that matters: `Event` stopped being an enum of variants
and became a struct of an `EventKind` and one rest map. A rest map per
variant would have been a third hand-rolling — and the next variant would
have been the third place to forget it. `RunEvent` needs none of its own,
because the flattened event already is one.

### Proven by property, not by example

`additivity_tests` is a generator, not a table. It plants unknown keys —
scalars, arrays and whole records — at random depths inside a canonical
document of every wire type in the seam, round-trips it, and asserts the
result is byte-for-byte the input. The `reasoning-tokens` probe the
reviewer wrote by hand is one sample the generator produces on its own;
it is kept beside the property only because a regression there deserves a
name in a failure list.

The generator is seeded and reproducible: a property failure that cannot
be replayed from its seed is a rumour.

### The two surfaces that are closed, and are said to be

Not everything can carry unknown content, and the honest thing is to name
what cannot rather than let it look preserved:

- A `{"$secret": "<key>"}` reference is the settings seam's closed shape;
  its own gate refuses an object with a second key. It is not this seam's
  to widen, and the generator does not inject into it.
- The closed value spaces (`effort`, `tools.mode`, `state`, `code`) have
  nowhere to put a value they cannot name. They REFUSE, loudly, with the
  value in the message — never a default, which would be a guess about
  what a future peer meant. A test holds them to that.

Both are named in the definition README. Neither is a kernel friction, so
neither is a FINDINGS entry.

## The output budget is a bound, not a report

A run declaring `output-bytes: 32` returned 746 bytes of answer: every
provider emitted its delta whole and then asked the registry to account
for it. The registry dutifully answered "truncated" — after the bytes were
on the bus and in the consumer.

That is a resource bound that bounds nothing. An unbounded or hostile
child floods the bus and every listener before anyone counts, and the
whole reason a run declares a budget through the process bundle is that
the bound holds BEFORE the bytes move.

`Runs::record_all` is now the only path from a provider's events to the
bus, and it does the arithmetic first: a text-bearing event is charged
against the run's remaining allowance and clipped to it, the typed cut
follows the prefix that fit, and once the budget is spent later text is
dropped whole rather than trickling out. Clipping lands on a character
boundary, because a prefix cut mid-character would either be invalid or —
after a three-byte replacement character — be back over the bound it was
supposed to respect.

Two things follow, and both were deliberate:

- **`output-bytes` now means answer bytes on the wire**, not raw stdout
  bytes read. The wire is what the bound is protecting, and it is the
  unit a consumer can verify. The provider still stops reading and kills
  the child the moment the budget is spent.
- **Providers lost their own copies of the arithmetic.** The echo
  provider had a private `budget_cut`; the claude and codex providers each
  charged bytes after their own drain. All three are gone. A provider now
  hands over what its codec produced and receives back what may be
  emitted, which is why the fix reaches providers that have not been
  written yet.

The proof is a real-composition test against the pinned daemon, driving a
child whose output is orders of magnitude past its declared allowance, and
asserting on the bytes a LISTENER received — not on the run record a
consumer may never fetch.
