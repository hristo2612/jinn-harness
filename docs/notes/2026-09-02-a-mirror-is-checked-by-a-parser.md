# A mirror is checked by a parser, or it is not checked

*Harness pin-bump 6 — adopting M2-K18 (`jinnd` `85d36b4`), plugin world
0.8.0 → 0.10.0; `jinn:net` 0.3.0, `jinn:auth` 0.1.0, `jinn:introspect`
0.5.0.*

## The migration shape held, and this time it was planned as one from the start

Pin-bump 5 learned that a world move is a migration: the moment the
vendored `wit/` changes, every guest in this repo is refused by the new
kernel as *not a loadable component of the plugin world*, so the rebuild
is the first task and the pin edit is the last. This bump was planned
that way and the shape held exactly. The guests generate their bindings
from `kernel-pin/wit`; the kits rebuild on the changed input; the one
guest that imports `net` (`jinn-api-http`) never matches a `net-error`
case exhaustively, so the additive `untrusted` case cost no source line.
The blast radius of a two-minor world move was, again, a rebuild.

What is worth writing down is not the move. It is the thing the move made
possible for the first time.

## `jinn:introspect@0.4.0` never parsed, and nothing could have noticed

The kernel's own README for the bundle says it plainly: `from` is a WIT
keyword, and `record readiness` shared one interface namespace with the
`readiness` operation. Nothing had noticed because no consumer bindgens
the bundle file — guests bind `wit/plugin.wit`, and the daemon mirrors
the introspect shapes by hand. The first parser to read the file was the
kernel's M2-K16 contract lens.

The harness had the identical hazard, three times over. `jinn-api`
spells `Readiness`, `Registrations` and `IntrospectEntry` as `serde`
structs; `jinn-plugins` spells `Transition` and `Unserved`, and reads the
`entry` record by string key in `Snapshot::parse_entries`. Each is a
second copy of a shape whose first copy is a file, and the only thing
comparing the copies was whoever last read both. That is the same
disease as the soak's hand-maintained pin and the README's stale
limitation (`docs/notes/2026-09-01-a-record-that-nothing-makes-move.md`):
a record of a live thing that nothing makes move.

A world file has a mechanical reader, so a drift in it breaks a build.
A bundle file had none, so a drift in it broke nothing — which is worse.

## The check, and exactly what it enforces

`harness-pin` — the crate that already owns the vendored surface and
proves it is the pinned one — gains `ContractWit`: one vendored bundle's
`contract.wit`, parsed with `wit-parser`, answering three questions a
mirror needs. A record's field names as they appear on the wire (`%from`
is the field `from`; the parser strips the escape). An enum's case names
in order. The named type an operation answers.

Each consuming crate then asserts its own copies against the parsed
file, in a test named for what it is: `tests/introspect_mirror.rs`.

- `Readiness` writes exactly the fields of `readiness-report`, and the
  operation `readiness` answers that record. Both halves of "the record
  was renamed, the wire was not" are asserted, not stated.
- `Registrations` writes exactly the fields of `registrations`.
- `IntrospectEntry` writes only keys `entry` declares, and the fields it
  leaves to `extra` are named — exactly `unserved`, which the plugins
  seam reads from there by key. A widening of that gap is now a decision
  a test makes visible, not drift.
- `Transition` writes exactly the fields of `transition`, `from`
  included. `Unserved` serializes to the `unserved` cases, case for
  case, in order, and parses each back.
- `Snapshot::parse_entries`, fed an object whose keys are exactly the
  record's fields, recovers every value — a snapshot reading a key the
  contract does not spell would come back empty.

Enforced: key sets and case names. Not enforced, and said so in each
file: types, and the answer the daemon actually sends. The second is the
real-composition suite's job and stays there.

The test was written first and was red against the old pin for the
right reason — the parser refused 0.4.0 at `contract.wit:141`, `found
keyword from` — and went green on the bump. That red is also the
demonstration: had this check existed at pin-bump 5, the unparseable
edition would have been a harness finding, not a kernel self-discovery
two packets later.

## Why the parser lives in the pin gate

The pin gate's Gate 1 already guarantees that the file under
`kernel-pin/contracts/jinn-introspect/` hashes to the pinned value. A
check that parses that file therefore inherits the guarantee: what it
compared against IS the pinned contract. Putting the parser anywhere else
would have meant a second path to the vendored surface and a second
claim about which file is authoritative. One home per fact.

## What the pin brings that this packet deliberately does not use

`jinn:net` 0.3.0 provides outbound — `request` unchanged byte for byte
and `send-request` beside it, `https://` over a vendored root bundle with
verification that has no off switch, and `untrusted` as its own refusal
because it is its own next move. No plugin here begins using it. The
allowlist, the irreversible effect class, and the first consumer belong
to the connector packets, designed as such rather than reached for
because the door is now open.

`jinn:auth` 0.1.0 is vendored and consumed by nothing. The operator API's
boundary in this repo is still loopback plus the granted port, and the
README's limitations map now says so in those words: the kernel supplies
the authority, the distribution has not yet held it to the door. That is
PLA-343.

## Findings

`FINDINGS.md` #43 is corrected at this pin: the world's title line and
package declaration agree again at 0.10.0. The gate it asked for is not
built, and #44 — hit on the same adoption — is the same class one file
over: the contract index names `jinn-net` at 0.1.0 and `jinn-introspect`
at 0.1.0 beside the newly added `jinn-auth`, and the net bundle's header
cites a world version that has moved. The lens that now parses every
bundle makes those three string comparisons cheap, and a reader should
never be the gate.

## The soak

Untouched. The M2 duty soak stays on `3a8e5c03` until the supervised bump
decided after its audit; this packet moved the repo's pin and nothing the
soak runs.
