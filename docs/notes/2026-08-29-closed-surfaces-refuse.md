# A closed surface refuses, and a proof measures what its name says

*2026-08-29 — the engines seam (`plugins/engines/`), round 5 of the 2.3
packet.*

Two findings, one theme: a claim that was true in the README and false in
the code, and a test whose name promised a measurement it did not take.
Both are the same failure mode as a silent drop — the reader is told
something has been checked when it has not.

## "Closed" was a description, not a behaviour

The seam declares two non-additive surfaces: the `{"$secret": "<key>"}`
reference and the closed value spaces (`effort`, `tools.mode`, `state`,
`code`). The README said extra keys inside a secret reference made the
request refuse. They did not. `SecretRef` derived `Deserialize`, so serde
did what serde does with a field it was not told about: dropped it,
silently, and answered a well-formed reference. A hostile decode→encode
lost `future-scope` and reported success.

That is exactly the defect rounds 2 and 3 of this packet were spent
closing, with a README entry as its disguise. A surface that quietly
discards what it cannot name tells its peer the document was understood.
The disguise is the dangerous part: additivity's property test skips the
closed surfaces by construction (there is nothing to preserve there), so
the one place the law does not reach was also the one place nobody was
checking anything.

**Refusal, not preservation.** For a secret reference this is not a
schema preference. An unknown key riding along beside a credential name
is a security property, and preserving it would be the wrong answer even
if the shape had room. So the decoder refuses.

**One refusal, not one per surface.** The fix is `jinn_settings::closed`
— a single function every closed surface's refusal goes through, whose
message names the surface, the offending content, and what the surface
admits. `SecretRef`'s hand-written `Deserialize` uses it, and so do the
four value spaces, whose derived refusals were loud but named only the
admitted variants: an operator reading `unknown variant "ultra"` has to
guess which field refused. It lives in `jinn-settings` because that crate
owns the `$secret` shape (one home per fact), and the engines definition
re-exports it.

Hand-writing four enum decoders buys the surface name and costs a
duplicated variant table. That cost is paid by
`every_closed_variant_round_trips_through_its_own_encoding`: every
variant of every closed space is encoded and decoded back, so the two
halves cannot drift.

This is why closed surfaces are documented in the definition README and
NOT in `FINDINGS.md` (COO ruling, PLA-316): `FINDINGS.md` is the
kernel-friction ledger, and a surface we chose to close is our decision,
not a friction the kernel imposed.

## A proof that measured the wrong thing

The output-bound proof asserted that a child writing far past its budget
put at most the budget on the wire — and read the byte total out of the
run's own `run-get` record. The number was real and the implementation
was correct; the claim about what had been measured was not. The seam
was reporting on itself, under a name that said a listener had received
those bytes.

That is the shape of a false green. A bound looks enforced because a
number under it appears in an assertion, and nobody notices the number
came from the wrong side of the boundary. It is worth fixing even when
the underlying behaviour is right, because the next person to break the
emission path will be told the bound still holds.

The proof now takes its number from an actual listener: the probe
consumer, repointed at the witness engine by profile edit (the grant per
engine moves with the id it routes to), folds every text-bearing event
the kernel hands it and writes the total into its own record. The
record's series is still asserted, under a name that says record.

One thing the listener sees that `run-get` obscured: a spent output
budget ENDS the run. The provider kills the child, so the listener
downstream of the cut sees `cancelled` with reason `budget` — the proof
now waits for that terminal state rather than a clean exit, which is also
a small piece of documentation about what the bound does.
