# A record that nothing makes move

*Harness pin-bump 5, round 2 — the two prose findings, and the two gates
that make them unrepeatable.*

## The finding was prose, and prose was the only thing that failed

Round 1 shipped a breaking world migration — plugin world 0.6.0 → 0.8.0,
every guest rebuilt, the kernel's new publish path subscribed to, a
canary observed going red and a claim retired on that evidence. Every
executable check passed: real composition against the pinned daemon,
every Rust gate, pin and archive integrity, the privacy firewall, CI.

Two things failed, and both were sentences.

- `README.md`'s limitations map still said *there is no lifecycle event
  surface at all, anywhere*, that the kernel is not a publisher, and that
  the fix is merely carded as M2-K13 — in the same commit that shipped
  the subscription, and forty lines below a Status section describing it.
  The README contradicted itself.
- A new evidence comment in `snapshot.rs` sent the reader to
  `docs/notes/witnessed-transitions.md` for the transcript. No such file
  was ever written; the note is
  `docs/notes/2026-09-01-a-witness-is-not-a-poller.md`.

## Both are one disease, and this is its second outbreak

A hand-maintained record of a live property is believed because it looks
authoritative, and goes wrong because nothing makes it move. The soak's
`meta.json` was the first instance — a pin recorded by hand, drifting
from the pin actually running, caught by an audit and costing a packet.
This is the same shape one layer up: the thing moved, the record did not.

The packet's own subject makes it worse than an ordinary staleness. This
branch exists to retire a claim ON OBSERVED EVIDENCE rather than on
belief. Shipping it beside a README still asserting the retired claim
would have been the packet contradicting itself in the same commit.

So the correction is not the deliverable. The gate is.

## What is mechanically checkable here, and what is not

No gate can read prose for truth. But two claim-shapes in this repo carry
their own verifier, and those are exactly the two that failed:

1. **A limitation that cites a `FINDINGS.md` number.** `FINDINGS.md`
   grades its own entries. An entry graded `ANSWERED` or `CORRECTED` is
   one this distribution has withdrawn — so a limitations bullet citing
   it while reading as if nothing changed is contradicted by its own
   source. The rule enforced is exactly that: such a bullet must name the
   retirement in words (`answered` or `corrected`). It is not a check
   that the bullet is otherwise accurate, and it does not pretend to be.

2. **A citation to a note.** `docs/notes/...` either names a file or
   names nothing. One walk of the tree settles it.

The second rule caught a third instance immediately, in the gate's own
source: a unit-test fixture built on a plausible-looking
`docs/notes/x.md`. The fixture was rewritten to cite a note that exists
rather than the check taught to ignore fixtures. A gate with an exemption
for the file it lives in is not a gate.

## Why the #40 bullet was rewritten rather than deleted

`FINDINGS.md` #40 is graded ANSWERED, so the old bullet had to go — but
the seam is not without limits here, and deleting the entry outright
would have traded a false limitation for a missing one. What survives is
narrower and each part is named: the seam emits no event of its own, so a
consumer of IT still pulls; the witnessed log is bounded at 256 sightings
and is per incarnation, so a catalog restart starts a new one; and the
kernel withholds `cause` on this contract, so a sighting names
`jinn:ledger` as where the reason lives — which #20 says is readable only
beside the daemon.

Same move as #41's correction one round earlier: strike the claim that
stopped holding, keep the half that still does, and say which is which.

## What this did not touch

The M2 duty soak, running `3a8e5c03` with an audit due. No kernel change,
no production routing, no old-gateway data. The round is two sentences
and two tests.
