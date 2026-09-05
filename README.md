# jinn-harness

**The Jinn distribution on the `jinnd` kernel.**

Everything a Jinn instance is — todos, workflows, engines, connectors, profiles —
lives here as plugins behind the kernel's typed capability contracts. The kernel
([`jinnd`](https://github.com/hristo2612/jinnd)) is pinned by exact commit and
contract hash (see `KERNEL-PIN.md`); this repo never contains kernel code and
never imports kernel internals. A product is a profile — a named plugin tree —
not a codebase.

After M4 retires the legacy gateway repo, this repo is renamed to **`jinn`**.

## Status

The core service seams and the Settings/Plugins web surfaces run as plugins
through the pinned daemon. This is not production parity; the limitations
below remain part of the acceptance evidence.

See [KERNEL-PIN.md](KERNEL-PIN.md) for the current kernel and contract surface,
[FINDINGS.md](FINDINGS.md) for measured kernel limits, [Agent Notes](docs/notes/)
for the implementation rationale, and [SOAK.md](SOAK.md) for the duty procedure.
Historical pin and packet transcripts live in those references and Git history.

## Layout

| Path | What it is |
|---|---|
| `AGENTS.md` | Standing orders for agents working in this repo |
| `KERNEL-PIN.md` | The kernel pin: jinnd commit + contract hashes + bump procedure |
| `kernel-pin/` | Vendored copy of the pinned contract surface (`wit/`, `contracts/`) — integrity-gated against `KERNEL-PIN.md` by `harness-pin` |
| `tools/harness-pin` | The pin gate: computes/verifies contract hashes (`cargo test -p harness-pin`); also parses a vendored bundle's WIT for the consumers' mirror checks |
| `tools/harness-docs` | The docs gate: the README limitations map against `FINDINGS.md` grades, and every `docs/notes/` citation against the tree (`cargo test -p harness-docs`) |
| `tools/cron-kit` | Builds the cron seam's components + pinned profile; its library is the kit machinery every seam kit shares |
| `tools/api-kit` | Builds the operator-API profile: the api trio beside the cron seam |
| `tools/engine-kit` | Builds the engines profile: the engine providers and the probe beside the api trio |
| `tools/session-kit` | Builds the sessions profile: the two store providers beside the engine providers |
| `tools/todo-kit` | Builds the todos profile: the two Todo stores above the two session stores |
| `tools/workflow-kit` | Builds the workflows profile: the two run stores above the two Todo stores |
| `tools/plugin-kit` | Builds the plugins profile: the two catalogs beside the api trio, plus the disabled and misbound entries the honesty proofs need |
| `tools/ext-kit` | Builds the JS-in-WASM extension tier's engine provider (Boa) and holds the extension entry shape; `tests/imports.rs` asserts the component imports exactly the four plugin-world interfaces |
| `tools/ui-kit` | Builds the `ui` profile: the web client built by its pinned toolchain, archived into the embedded bundle provider beside the plugins profile; `tests/verbatim.rs` is the byte-for-byte gate over `web/port-map.txt` |
| `web/` | The TypeScript client (not a Cargo member): jinn `43e8647`'s shell, Settings and plugins page, ported verbatim; Node and pnpm pinned by `web/.npmrc` and `packageManager`; the node lane in CI |
| `plugins/` | First-party plugin crates (wasm components) — land per phase, one seam triple at a time |
| `profiles/` | Named plugin trees — a product is a profile |
| `tests/composition` | Real-composition gates: boot generated profiles through the REAL pinned jinnd daemon |
| `FINDINGS.md` | Kernel frictions logged as jinnd packet-card candidates (two-way iteration) |
| `docs/notes/` | Agent notes: rationale for non-obvious decisions, one per non-trivial change |
| `docs/postmortems/` | Defects that hardened into rules (AGENTS.md standing order 5) |

## What the core port did NOT achieve

Seams 2.1 through 2.7 are landed. This section is the honest close: every
named known limit, gathered in one place, so the M3 parity conversation
starts from a list rather than from optimism. Where a limit was previously
IMPLIED rather than named, it is named here for the first time and marked
**(named here)**.

Each seam's own README carries its limits in full; this is the index, not
a second home. `FINDINGS.md` carries the kernel-side frictions and the
per-seam "could NOT prove" sections.

### The whole port

- **No 2.x seam has soak evidence. (named here)** `SOAK.md` is the CRON
  seam, phase 1.4. Nothing from 2.1–2.7 has had a week of real duty.
- **Nothing anywhere races two concurrent writers.** Todos, workflows,
  sessions and the plugins catalog all drive one caller at a time. "Not
  reachable by inspection" is written down in each case, and it is not a
  proof.
- **The threat model is ACCIDENTAL throughout** — races, crashes, torn
  writes, a daemon that stopped. Not an adversary with write access to the
  data root or the profile. A forged journal is not detected as forgery;
  what a reader catches is damage.
- **Six seams hand-roll journal replay** and each got absence wrong
  differently (`FINDINGS.md` #36). The shared typed replay outcome and the
  typed NEGATIVE lookup answer are proposed and **not built**.
- **The nested-dispatch deadlock (#4/#32) is unretired,** and it is the
  root cause of every polling decision in the stack: it is why every event
  feed is a cursor rather than a push, why every composing seam polls the
  one below, and therefore why latency compounds per layer (#35, measured:
  513 ms at two layers, 755 at three, 1084 at four).
- **`jinn:fs` cannot drop a suffix** (#34), so every heal in all three
  durable stores rewrites the whole prefix.
- **Sibling activation order is unspecified for an entry that declares
  nothing** (#7, answered at pin `a53a352` for the string lane: a wasm
  entry's `injects` declaration is a kernel gate, and the `ui` transport
  uses it); an undeclared consumer still meets its provider in whichever
  order the boot deals, which is what keeps #30's window open.
- **A vendor CLI is exercised only where an operator names one.** The
  todos and workflows vendor legs self-skip in CI; sessions has no vendor
  test at all.

### 2.1 — Operator API

- **Authentication exists since packet 2.8; authorization does not.**
  The operator API refuses every request that does not present the one
  operator credential (`jinn:auth`, proven at the door —
  `tests/composition/tests/auth.rs`). What remains, named: ONE principal
  and no per-route authority — the operator may do everything the API
  does; no accounts, roles or sessions-as-identity, by ruling; and no
  defence against a process running as the daemon's own uid, by the
  kernel contract's stated threat model. Before 2.8, loopback plus the
  port was the entire boundary.
- **No plugin here makes an outbound request.** Since pin `85d36b4`
  `jinn:net` 0.3.0 PROVIDES `request`/`send-request` behind the grant's
  `outbound` allowlist, `https://` over verified TLS; before it, no
  outbound existed at any pin. **Connectors — Slack, Telegram, webhooks,
  any vendor API — are therefore no longer structurally impossible, and
  still do not exist in this repo. (named here)** Non-loopback LISTEN
  remains unprovided.
- **The swap proof for this seam swaps a second entry of the SAME
  artifact,** because no second transport shape can exist.
- **`patch-entry`'s `idempotency-key` is accepted and unused.**
- **The API seam recorded no "could not prove" section of its own.
  (named here)** No concurrency proof, no load proof. The auth proof
  exists since 2.8; it proves the door for the HTTP provider and says
  nothing about a transport that does not exist yet.

### 2.2 — Settings

- **The shadowed-refusal recovery is unproven end to end.** The repo's
  only `#[ignore]`d test, blocked on #32.
- **A mixed hot+cold patch across two layers cannot be applied
  atomically** (#28). The shipped shape is a whole refusal carrying an
  executable recovery.
- **Per-entry config layering is a guest-side emulation of a kernel
  concept** (#27, #29).
- **Secret references are names only. (named here)** No rotation, no
  revocation, no lifecycle: nothing addresses what happens when a key
  changes under a running provider.

### 2.3 — Engines

- **`run-get` cannot tell a REAPED run from an id that never existed.** A
  consumer polling a reaped run reads "no run" and takes the conservative
  branch, reporting `failed` for work that SUCCEEDED. Named in
  `docs/postmortems/2026-08-30-the-run-bound-reaped-by-key-order.md` and
  **carrying no FINDINGS number** — the sharpest untracked item in the
  repo, and the same class as #36's typed negative lookup.
- **N engines coexisting is N contract names,** a guest-side encoding the
  kernel cannot see (#29): nothing refuses two entries claiming one engine
  id, and a typo is a resolve-time `missing-dependency` rather than a
  profile-load refusal.
- **A provision made in `activate` binds the STAGING instance** (#30);
  one call before the swap commit kills the contract permanently, with no
  fault, no refusal and no log line. The harness narrowed the window and
  did not close it.
- **The echo provider's token counts and `cost-micro-usd: 0` are
  stand-ins.** Every CI-runnable usage-path proof runs on fabricated
  numbers.
- **No concurrency or load proof, and no record that there is none.
  (named here)**

### 2.4 — Sessions

- **One unreplayable journal takes the WHOLE durable store down.** A
  per-document quarantine is the better shape and is not built.
- **A store POLLS its engine;** listening would deadlock (#4/#32).
- **The event ring is bounded** and a session past it loses its oldest.
- **An append-only journal grows the fiber's effect journal without
  bound** (#33) — one entry per line for the life of the incarnation.
  Graded *derived, not measured*; the harness does nothing about it.
- **No vendor engine was ever driven under a session.**

### 2.5 — Todos

- **A comment cannot be edited or removed, and neither can a Todo. There
  is no DELETE.**
- **The torn tail is manufactured, not observed** — the suite writes a
  short document behind the daemon's back. What is proven is the reader,
  not that the kernel tears.
- **The event ring's drop count and the `declared-status` divergence are
  unit-proven only,** never driven through the daemon.

### 2.6 — Workflows

- **`spec-digest` is a 64-bit FNV-1a change detector, not a cryptographic
  hash,** and its stability rests on this workspace's lockfile.
- **A run is NOT resumed across a restart** — a decision, not a gap. And
  nothing here proves a resumable run would be safe, because nothing here
  builds one.
- **There is no retry and no delete.**
- **The graph walk is proven on TWO shapes through the daemon.** A wide
  fan-out, a join with several followed inbound edges and a deep chain are
  unit-proven only — the biggest single parity exposure in the seam.
- **A record-less document is proven for the RUN family only.**

### 2.7 — Plugins

Its own README carries these in full; the load-bearing ones:

- **A plugin identity swap disposes the old incarnation before spawning
  the replacement.** The operator API can change composition shape since
  pin `f8b285b` (#37, answered by `jinn:profile-admin`), but the swap
  window remains open: a reply-expecting walk can select no listener
  between disposal and replacement. This is the contract's stated 0.1.0
  limit, carded as jinnd M2-K27; see
  [the shape-write note](docs/notes/2026-09-04-a-shape-is-a-write.md).
- **A guest's own activation failure has no reason at this pin** (#38),
  and this seam refuses to invent one from a neighbouring line. Round 1
  of this packet DID invent one — an unrelated refusal from an earlier
  incarnation, reported as the cause — and the fix removed the variant
  that could carry it, so the fabrication is unrepresentable rather than
  unreached.
- **`state: null` is four situations and this seam separates two** (#39).
- **Three of the eleven readings are unreachable from a SNAPSHOT** (#41,
  corrected at pin `901d207`). `mounted`, `activating` and `interrupted`
  each name a fiber between two rests, and a pull answered at rest cannot
  carry one: a real restart, measured through this seam, completed inside
  one HTTP read while 190 consecutive reads all returned `active` and the
  kernel's own ledger recorded the whole path. That measurement stands.
  What did not stand is the generalisation built on it — that no consumer
  could ever be handed one — and `jinn_plugins::UNREACHABLE_AT_PIN` with
  its `no-transient-reading-at-this-pin` canary are retired on evidence:
  the subscription witnesses all three, and the canary's predicate,
  fed the daemon's own delivered readings, refuses every one. The
  narrower law survives as `NOT_FROM_A_SNAPSHOT` and
  `no-transient-reading-from-a-snapshot` — an ENTRY's lifecycle is still
  a join over a pull, so it still may not carry one.
- **The lifecycle event surface now EXISTS, and what this seam answers
  from it is a bounded history** (#40, answered at pin `901d207`). Until
  that kernel the claim above was literal: nothing published, so the only
  event this seam could have emitted was one a poller synthesised by
  diffing two snapshots — a transition announced without being witnessed
  and without a time. The refusal was written at the place a person would
  come to add one, and it outlived its own premise by exactly one pin:
  `jinn:introspect@0.4.0` publishes every committed `FiberTransition`,
  both catalogs SUBSCRIBE under their own grant, and
  `GET /v1/plugins/{catalog}/{id}/transitions` answers what was
  WITNESSED. What is still limited is narrower and named: this seam emits
  no event of its own, so a consumer of IT still pulls (#4/#32); the
  witnessed log is bounded at 256 sightings and is per incarnation, so a
  catalog restart starts a new one; and the kernel withholds `cause` on
  this contract, so a sighting names `jinn:ledger` as where the reason
  lives — which #20 says is readable only beside the daemon.
- **An entry the document could not RESOLVE appears in no catalog at
  all** — a list here omits exactly the entries that are most broken.
- **The surface is read-only,** and the join is three reads at three
  instants rather than one atomic view.

### UI-1 — The bundle

- **Reveal and rescan have no catalog operation.** Installation, removal,
  disable, topic widening and engine swap are live shape writes (#37,
  answered at pin `f8b285b`); a catalog entry is not a local folder. See
  [the shape-write note](docs/notes/2026-09-04-a-shape-is-a-write.md).
- **A failed plugin shows `failed` and no reason** (#38), on the page
  that exists to show reasons — the transcript of the page trying is
  this packet's addition to that finding.
- **Activation-time injection was a coin toss the kernel did not
  re-arm** (#45), and **a provider swap did not restart its wasm
  consumer** (#46) — both answered at pin `a53a352` (M2-K24, pin-bump
  7): the transport declares `injects: ["jinn:ui-bundle"]`, the kernel
  gates its activation and restarts it on a swap, and the transitions
  subscription, the introspect and clock grants, the post-commit probe
  and the "not yet" classification are gone. What stays: the
  transport's own activation fault is still named on the ledger by the
  transport itself before it fails (#38's workaround), because the
  kernel still records a state and never a reason.
- **The Settings page shows only what a profile's settings seam
  declares** — in the `ui` profile that is `cron` (`jobs`, `tick-ms`,
  `entry-id`; the secret reference is read-only); the config.yaml-shaped
  sections are hidden and named in one caption, never sent.
- **The bundle is one 1.46 MB crossing per transport activation. (named
  here)** Measured in proof 3 and recorded in the note; a page load is N
  connections because responses close.
- **Only Settings and Plugins are ported.** The rail no longer pretends
  otherwise: since PLA-356 (adaptation 15, plan §9.7 amendment 10) it
  derives what is live from the route table, Plugins is a rail item of
  its own, and an absent destination is disabled with the reason
  `not in this profile` rather than landing on the plugin splat.
- **The service worker is dropped** (plan §8, question 4), so the UI is
  not installable and has no offline shell in this packet.

### UI-2 — Moments and the extension tier

- **A moment inside an extension's restart window WAS answered UNMODIFIED,
  not refused** (#47, answered at pin `cb08683`, jinnd M2-K26, pin-bump 9):
  at `a53a352` the kernel withdrew a listener's `listen` with the old
  incarnation's suspension BEFORE the replacement committed, so a walk in
  the window (~500 ms per source edit) selected nobody and M2-K9's
  `restarting` never fired. Now the registration survives the suspension
  as a refusing registration until the replacement's atomic commit: every
  send in the window is `503` naming `restarting`, none unmodified, and
  proof 5 asserts both halves of fail-closed. What stays open is the
  card's named limit: an `emit`-mode notification inside the window is
  still lost and traced `listeners: 0`.
- **A bad extension KILLED the transport, not its own slot** (#48,
  answered at pin `b1dbe8f`, jinnd M2-K25, pin-bump 8): at `a53a352`
  every guest call was one `settle(deadline)` and `emit` awaited each
  delivery inside the emitter's call, so a listener that looped spent the
  transport's 5 s deadline — proof 7 measured the transport's instance
  dying on it. Now the walk spends the LISTENER's bound, the transport is
  charged nothing, and the extension that looped is `failed` on its own
  row; the entry's `budget` is honored as `listen-within`. What stays
  open is #51's non-fatal half: a throwing extension's contained failure
  is still only a count on the emitter's trace.
- **`emit` WAS not gated by a topic grant** (#49, answered at pin
  `cb08683`, jinnd M2-K26 (e), pin-bump 9): at `a53a352` any guest could
  emit any unreserved topic and the transport's three topic grants were a
  statement, not an authority. Now a walk is covered by the grant of the
  topic's own name exactly as a subscription is — the stripped transport
  is refused on its own row — and every first-party emitter in the kits
  carries its topic (the audit found only the `ui` transport did). A
  transport in a profile WITHOUT the UI is not granted the moment topics:
  a moment there is refused `refused` on the record.
- **A contained delivery failure is a count on the emitter's trace and
  nothing on the listener's history** (#51): the plugins page shows a
  throwing extension `active` with a clock read and no failure. Proof 4
  asserts `failures: 1` on the emitter's trace and the ABSENCE on the
  listener by name, so the pin that writes the row flips the proof.
- **The guest's memory is not a reading the kernel exposes** (#50): the
  cost of one moment is measured (3.3 ms per walk, a fresh Boa context
  each) and no context reuse is designed; the memory high-water mark is
  not on `jinn:introspect` 0.6.0.
- **Listener order across siblings is what the boot dealt. (named here)**
  Two extensions on one topic fold in an order nothing declares and no
  reading exposes: the walk's order and the `listen` rows' order
  disagreed in two runs of three at one head (FINDINGS #52; KG-3, the
  kernel's answer to sibling order covers declared injections only).
  Proof 3 names both orders and asserts neither is the other's witness.
- **Installing, removing, disabling an extension, widening its topics or
  swapping its engine are CLICKS since pin `f8b285b`** (#37 closed; the
  K23 split, plan §9.5; pin-bump 10): one `jinn:profile-admin` write each
  from the transport, the refusal rendered on the row. Before it they
  were profile edits rendered DISABLED with the finding (#48 answered at
  `b1dbe8f`, pin-bump 8; #47 at `cb08683`, pin-bump 9; #37 at `f8b285b`)
  — never silently absent; the NOT-YET mechanism stays, empty.
  Editing an installed extension's `source`, `origin` or
  already-granted `topics` IS `PATCH /v1/profile/entries/{id}` today.
- **The two chat topics are dispatchable and proven, and reached by no
  ported surface. (named here)** The ported shell has no composer (UI-6);
  `before-patch-settings` is the one moment an operator can click.

### What is not here at all

Beyond the six ported seams and this one, the old gateway's surface has
**no plugin in this repo**: connectors, chats, notes, experiments, the
org and its employees, delegation, approvals, knowledge, the MCP surface,
and the remaining web surfaces. Cron exists as phase 1's seam and has not been revisited
as a product surface. Parity is a long way from here, and the cutover rule
below is what keeps that honest.

## The cutover rule

The old gateway keeps ALL production until parity. Nothing in this repo touches
production data before the parity gate passes, instance by instance.

## Building

```
cargo test --workspace
```

Plugin crates build to `wasm32` components against the vendored contract
surface in `kernel-pin/` — never against a live kernel checkout.

### Environment gates on the test suites

Every gate below self-skips LOUDLY when its condition is absent, and a
skip is never reported, returned, or summarized as a pass.

| Variable | What it turns on |
|---|---|
| `JINND_DIR` / `JINND_CLONE_URL` | Where the real-composition suites find a jinnd checkout holding the pinned commit (`KERNEL-PIN.md` Gate 2). Without one, every composition proof skips. |
| `JINND_READ_TOKEN` | CI's credential for the same checkout; jinnd is private, so CI runs the composition leg only where it is configured. |
| `JINN_HARNESS_WORKFLOW_VENDOR_ENGINE` | The engine id the workflows seam's vendor leg binds as the last of the FOUR-layer composition proof. Same discipline as the todos gate below, one layer up: it spends metered inference under the operator's own authentication, so it runs where a person names it and skips loudly everywhere else, and an engine that is NAMED and not mounted fails the proof rather than skipping it. |
| `JINN_HARNESS_TODO_VENDOR_ENGINE` | The engine id (`claude` or `codex`) the todos seam's vendor leg binds as the second half of the three-layer composition proof. It spends metered inference under the operator's own authentication, so it runs where a person names it and skips everywhere else. An engine that is NAMED and not mounted fails the proof rather than skipping it. |
