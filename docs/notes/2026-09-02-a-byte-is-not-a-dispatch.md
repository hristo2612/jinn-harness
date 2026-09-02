# A byte is not a dispatch

**Packet:** UI-1 — UI-as-profile (PLA-349), the first packet of the UI
malleability arc (`docs/plans/ui-malleability-arc.md` §4).
**Kernel pin:** `85d36b4` (M2-K18), UNCHANGED.

## The decision, and the sentence it rests on

Packet 2.8 built the door: every parsed request is put to `jinn:auth`
`verify` before the transport dispatches anything on the connection's
behalf. UI-1 serves the web UI from the same transport, on the same
port, to a browser whose top-level navigation cannot carry a bearer
header. Two sentences of the 2.8 note collide unless one of them is
read exactly: "every parsed request is exactly one verify" was true of
a transport that served only `/v1`; the contract's own obligation is
narrower — a transport "issues NO dispatch on that connection's behalf
before this call answers `principal`". A byte answered from the
transport's own memory is not a dispatch. So the bundle is read ONCE, at
`activate`, as an injected dependency; nothing a static request does
ever crosses into a guest; and the door is not on that path at all — a
bearer presented there is IGNORED, not consumed. The COO ruled on it
(plan §8, question 2) and made the "ignored, not consumed" probe a
mandatory acceptance line; proof 2 is that line on the ledger: three
static connections carry transport rows and nothing else, two `/v1`
connections carry exactly one `verify` and one `AuthDecided` each, and
the window holds exactly two decisions.

## What the seam is

`jinn:ui-bundle` (`plugins/ui/jinn-ui/README.md`) has two operations
and no request shapes: `manifest` and `bundle`. The provider is the
built client compiled in (`include_bytes!` of the kit's archive), with
an empty config and only its own contract as a grant, so its identity IS
its hash and a UI swap is a profile edit of one entry's `package` and
`hash`. The transport verifies every file against the manifest before
it opens its listener; a bundle that does not verify fails the
transport's activation and nothing else (proof 5: the transport's fiber
reads `Failed`, every sibling reaches `Active`, the port never opens;
the operator-api profile with no bundle keeps answering `/v1/health`
200 and answers `/` a typed 503).

## The serving law, and why each row is there

Every row was an individually hunted bug in the old gateway (inventory
§2.15, §2.16, §2.24) and none was tidied: the document `no-cache` (iOS
Safari over a tunnel hostname caches HTML indefinitely), hashed assets
`immutable`, an unknown `/assets/*` answered `404 text/plain` and never
the SPA fallback (the fallback's `text/html` for a `.js` request read as
an unrecoverable MIME error for a merely superseded chunk), every other
non-`/v1` path the document, `.webmanifest` as
`application/manifest+json` (the install prompt never appears
otherwise). One row is new and is a decision: a first segment spelling
the API namespace in another case (`/V1/…`) is NOTHING — 404, no
dispatch — rather than a page, so a case variant of the API can never be
reached by a route the door does not sit on.

## Measured

- Proof 3: the bundle crossed ONCE per transport activation —
  {{BUNDLE_BYTES}} bytes, {{BUNDLE_FILES}} files, in {{LEDGER_ROWS}}
  ledger rows at rest, {{TRANSPORT_ROWS}} of them on the transport's
  account.
- Proof 4: the swap landed {{SWAP_LANDED}} after the edit; the blip
  (first refused connect to first marked 200) was {{BLIP}}; the
  transport's incarnation +1, the settings consumer's and the catalog's
  unchanged.

## What was found

{{FINDINGS}}

## What this packet does NOT do

The live half (UI-3), moments (UI-2), the service worker (plan §8,
question 4), compression, keep-alive, `HEAD`, range requests. The
plugins page cannot enable or disable anything: `jinn:profile.patch-entry`
writes `config` only (`FINDINGS.md` #37) and the capability constitution
04 names is carded as jinnd M2-K23 (PLA-348); the page says so on every
disabled control. A failed plugin shows `failed` and no reason (#38),
on the page that exists to show reasons.
