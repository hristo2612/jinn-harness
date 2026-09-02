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
transport's own memory is not a dispatch. So the bundle is read ONCE per
incarnation — at `activate` when the provider is live, otherwise on the
kernel's witnessed `Active` transition (#45) — as an injected
dependency; nothing a static request does ever crosses into a guest; and
the door is not on that path at all — a
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

- Proof 3: the bundle crossed ONCE per transport activation — 1,464,011
  bytes (35 files; the plan's "~4 MB" was an estimate of a full build,
  this is the two-surface one), in a ledger of 190 rows at rest, 31 of
  them on the transport's account, three of those `manifest` probes
  (the activation-order path of #45).
- Proof 4: the swap served the marked document 1.24 s after the edit,
  with 0 refused connects (no blip: the transport never restarted, #46),
  one more `bundle` crossing on the record, the settings consumer's and
  the catalog's incarnations unchanged — and the transport's unchanged
  too, which is the finding, not the design.
- The web build: initial critical path 181,517 gzip bytes against the
  carried 195,000 budget; 347 files verbatim, 20 adapted, 3 new on the
  port map; 98 test files / 840 tests green.

## What was found

- **#45** — a wasm entry that injects a sibling's contract at activation
  is a coin toss (four boots of five failed the transport), the kernel
  never re-arms it when the sibling lands, and a provider's own "I am
  here" event is the #4/#32 cycle (`CycleRefused` on the record). The
  transport now completes its one read on the kernel's own
  `jinn:introspect/transitions` publish, under a `jinn:introspect` grant
  it has no other use for.
- **#46** — a provider swap does not restart a wasm consumer that
  injected it: the card's "a bundle swap is a restart" (R9, epoch
  gating) is not available on the string lane; the swap is a witnessed
  transition and a re-read, on the record.
- **#38, a transcript added** — the transport's verify fault names the
  mismatched file and the record keeps only `Failed` (KG-5). Round 2
  added the workaround: the transport registers its fault as an effect
  label before failing, so the reason outlives the fiber on the ledger.
- Not a finding: the activation crossing is 1.46 MB and shows nowhere in
  the boot time; KG-1 (#37 / PLA-348) is the reason every write on the
  plugins page is disabled.

## Round 2: the acceptance restated to the pinned kernel

Verify round 1 (four Blockers, §8 amendment 4) moved four things.

- Proof 4 ASSERTS the transport's incarnation unchanged across the swap
  (#46) instead of printing it; it flips to +1 when M2-K24 lands.
- Proof 5 proves BOTH orders of #45, the late one FORCED: the `ui`
  profile boots with the bundle entry absent (the transport keeps its
  `ui-bundle-entry` config and grants and rests active without a bundle,
  every page a typed 503), then the corrupt entry is added by a profile
  edit; the transport witnesses its Active transition, verify refuses
  inside the delivery, the kernel contains it (`failures: 1`), and the
  transport's incarnation is unchanged with no byte served.
- Proof 5b boots ten fresh roots in a row; every one must reach the
  transport active, listening and serving the document (~27 s from
  spawn to served, of which the transport's own activation is ~50 ms on
  the ledger). Its second run caught the verifier's coin toss from the
  other side — the transport ACTIVE and serving 503 for the daemon's
  life, because the bundle entry reached Active between the transport's
  second probe and its activation's commit, and a listen registered
  inside `activate` is not live until the commit. FINDINGS #45's round-2
  addendum carries the transcript; the fix is one post-commit probe on a
  one-shot clock alarm (never a poll), beside the fault label (#38) and
  the two contained provider classes (`provider-failed`,
  `inactive-context`) read as "not yet" rather than death.
- The Settings page renders ONLY what the namespace's schema declares:
  the settings seam's `Resolved` now carries `schema` (additive, R12),
  `api-config.ts` reads it, filters every patch to declared keys and
  never sends a `secret-ref`, and the page hides the config.yaml-shaped
  sections a profile does not declare, naming them in one caption. In
  the `ui` profile the one namespace is `cron` and the declared setting
  proof 7 patches is `tick-ms` (cold: the scheduler restarts and the
  value reads back from `GET /v1/settings/cron`). `routes/settings/page.tsx`
  therefore joins item 1's adaptations — a verbatim page cannot render
  only declared settings.
- Onboarding is item 9: the wizard's mount is gone from `page-layout.tsx`,
  the wizard and its test are not ported, `api.ts` synthesises the
  onboarding state complete with no request, and a repo test in
  `tools/ui-kit/tests/verbatim.rs` asserts no `/api/` string survives in
  any adapted or new file outside the two item-1 adapters. Twenty-seven
  VERBATIM files still carry the string (proof 6 forbids touching them);
  a call on one answers the SPA document and never old-gateway data, and
  the test prints them as the carried inventory.

## Round 3: the transcript is the proof, and the grep was not

Verify round 2 passed everything but one line: after pairing and reloading
`/settings`, the browser's network log showed `GET /api/plugins` and
`GET /api/talk/config`, both answered with the SPA document. The repo test
above had listed both files as carried inventory — a dead string in a
verbatim file is harmless, the reasoning went, because nothing mounted calls
it. The transcript said otherwise, and the ruling (§8 amendment 6) made the
two files adaptations 10 and 11.

One of the two attributions was wrong, and finding out how mattered more
than the fix. `inventory.ts`'s `usePluginInventory` has no caller: the
plugins page reads `/v1/plugins/main` through item 1's adapter since round
1 and imports only the query key and the disk-follows hook. The live
`GET /api/plugins` came from `plugins/disk-plugins.ts` — the old gateway's
client-plugin loader, mounted on EVERY page by `DiskPluginsBridge` in the
shell and re-run on each connection flip, which is why the transcript saw it
repeated. Adapting inventory.ts as ruled would have left the Blocker open.
It is adapted anyway (a read that would go live the day something called
it is not inventory), and the loader is declared item 12 on the Todo with
this evidence: it resolves EMPTY client-side and issues no request, and a
pass still SETTLES, because the contributed-route splat item 5 keeps waits
on that flag and would render nothing forever without it. Unmounting the
bridge from item 8 instead would have been the smaller diff and the wrong
one.

What this taught the repo test: a grep over adapted files proves what the
diff sends, not what the page sends. The test now pins the three mounted
requesters as adapted rows, so a map edit cannot move a live requester back
into the carried list; the carried list itself is 33 verbatim files, and
the claim about them is the narrow one — a call on one answers the SPA
document — with the browser transcript, the verifier's proof 7, as the
proof that no mounted page makes such a call.

## What this packet does NOT do

The live half (UI-3), moments (UI-2), the service worker (plan §8,
question 4), compression, keep-alive, `HEAD`, range requests. The
plugins page cannot enable or disable anything: `jinn:profile.patch-entry`
writes `config` only (`FINDINGS.md` #37) and the capability constitution
04 names is carded as jinnd M2-K23 (PLA-348); the page says so on every
disabled control. A failed plugin shows `failed` and no reason (#38),
on the page that exists to show reasons.
