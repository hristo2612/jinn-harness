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

Phase 1.12 — kernel pin `901d207` (M2-K13). The pin bump is a MIGRATION,
not a version edit: the plugin world moved 0.6.0 → 0.8.0, and against
that kernel every artifact built on the old world is refused with
`artifact is not a loadable component of the plugin world`. Every guest
here is rebuilt by its kit, and the whole distribution boots on the new
world.

**The kernel became a publisher, so the plugins seam stopped being
blind.** `jinn:introspect@0.4.0` publishes every `FiberTransition` the
kernel commits on the reserved topic `jinn:introspect/transitions`,
behind a ledger-ordering barrier, with counted bounded back-pressure and
no-replay ordinals. Both catalog providers now SUBSCRIBE to it under
their own `jinn:introspect` grant, and
`GET /v1/plugins/{catalog}/{id}/transitions` answers what the catalog
WITNESSED — the kernel's own record, delivered, never a diff of two
reads. That closes `FINDINGS.md` #40 and corrects #41: the three
transient readings are unreachable from a SNAPSHOT, not from every
consumer, and `jinn_plugins::UNREACHABLE_AT_PIN` and its canary are
retired on the evidence of a daemon observed delivering all three
(`docs/notes/2026-09-01-a-witness-is-not-a-poller.md`).

### Phase 2.7 — kernel pin `3a8e5c0` (M2-K9), UNCHANGED. The plugins seam
(`plugins/plugins/`) is the SEVENTH and LAST core-port seam, and the one
that makes the malleability contract something an operator does rather
than something a suite asserts.

**A provider is swapped THROUGH THE API.** Two
`PATCH /v1/profile/entries/{id}` calls move `jinn:plugins.main` from
`jinn-plugins-profile` to `jinn-plugins-static`; `list()` reports the new
binding and the new entry set's own qualifier, and the layer above is
untouched as a MEASURED fact — the API's incarnation number is asserted
EQUAL across the swap, not merely "it still answers"
(`tests/composition/tests/plugins.rs::the_catalog_provider_swaps_through_the_api_with_the_layer_above_untouched`).

That is only possible because this seam was designed for it.
`jinn:profile.patch-entry` writes ONE subtree, `config`, so the
package-and-hash swap the other six seams prove by editing the profile
file is not reachable through the operator API at all (`FINDINGS.md` #37).
Both catalog providers therefore read their catalog id from config and are
granted both catalog names up front. The distribution's headline claim —
*swapping a provider is a profile edit* — was true only of an operator
with filesystem access to the document, and now says so.

**A catalog READS; it does not run.** Every value is licensed by the
evidence that produced it, and `active` is reachable from exactly one
input, so every other combination falls to a conservative answer by
construction. A disabled entry reads `no-incarnation` with `reason:
disabled`; an entry whose `jinn:net` grant admits one port while its
config names another reads `failed`; an entry a catalog names and the
machine does not run reads `not-mounted`. There is no `unknown` in the
vocabulary, and no CORRELATED reason either: `jinn:ledger` v0.1 records
no causal parent, so a failure's reason is `no-recorded-cause` carrying
the span that was searched, a COUNT of the reason-bearing lines it
declines to cite, and the qualifier that says why. The lines themselves
are read with `history(id)`, where they are that entry's history and not
a cause (`FINDINGS.md` #38).

**The pin was NOT bumped, on evidence.** jinnd main has moved past
`3a8e5c0` (M2-K10's cycle refusal, M2-K12's Linux CI and keystore fix),
so the question was asked rather than assumed. Every surface this seam
consumes is byte-identical at the pin and at main: `jinn:profile` and
`jinn:ledger` are unchanged verbatim, and `jinn:introspect`'s 0.2.0 →
0.3.0 is additive — a new `waits` operation, with `record entry` (the
only thing read here) untouched, no field added, removed or retyped. The
`jinn:plugin` world's 0.6.0 → 0.7.0 adds a `cycle` case to
`kernel-error`; nothing here emits a reply-expecting dispatch, so no
crossing this seam makes can close a wait cycle. M2-K12's keystore fix is
a `dev`-profile codegen change with zero runtime source delta and no
keystore consumer in this seam. Bumping for tidiness would have moved the
whole distribution onto a new contract surface to buy nothing.

One consequence of NOT bumping is carried in code rather than in this
paragraph: main adds a `CycleRefused` ledger kind, so a catalog that
matched ledger kinds exhaustively would break on a later pin. The reader
treats kinds as an open set with an honest fallthrough — reason-bearing
kinds are a named known set and every other kind is history — which is
the right shape at any pin.

**The reason gap is named rather than papered over.** A guest's own
activation failure — a trap, a panic, a deadline kill — records its STATE
and never its REASON: the kernel puts the `KernelError` in
`FiberRecord.failures` and never drains it to the ledger (`FINDINGS.md`
#38). This seam refuses to correlate such a failure with whatever refusal
happens to precede it, because `jinn:ledger` v0.1 records no causal parent
and a plausible neighbour presented as a cause is the fabrication the seam
exists to kill.

### Phase 2.6 — kernel pin `3a8e5c0` (M2-K9), UNCHANGED. The workflows seam
(`plugins/workflows/`) is the sixth core-port seam and the FOURTH layer of
the stack: a workflow node dispatches work through the `jinn-todo`
DEFINITION, which dispatches to a session through `jinn-session`, which
runs over `jinn-engine`, so

```text
jinn:workflow.<store> -> jinn:todo.<store> -> jinn:session.<store> -> jinn:engine.<id>
```

composes with no layer naming the next one's provider. The layering is
enforced by AUTHORITY: a run store's entry is granted no
`jinn:session.<id>` and no `jinn:engine.<id>` at all.

**A run is pinned to a definition REVISION, and says which.** `start`
resolves "latest" exactly once, records the number, and carries that
revision's WHOLE spec in the run's own `run-started` line; nothing
re-resolves it. A definition edited mid-flight therefore cannot reach a
run already in flight, and `definition-revision` is on every read — the
old gateway does this incidentally, and the invisibility of it cost a
wasted prompt patch on 2026-08-30
(`docs/notes/2026-08-30-workflows-the-pin-and-the-fourth-layer.md`).

**A node is never eternally `running`, and the guarantee is an ORDER
rather than a fold.** The reader reports what the document says; a durable
store replays, plans the recovery, APPENDS the `running -> interrupted`
moves and the run's own ending, and only THEN provides its contract. A
store whose recovery append is refused fails to activate rather than
serving a `running` no durable line justifies. That is the sessions seam's
`running` defect and the todos seam's `executing` defect answered one
layer up, proven red-first rather than assumed inherited.

**A document that reads as absent is not yet a clean slate.** All three
durable stores answer a record-less document — the one a daemon killed
inside its very first append leaves — with three things rather than one:
the typed reading that installs nothing, the BYTES dropped so no later
writer can append onto them, and the ID reserved so the next mint cannot
hand it out again. Recognising the absence and stopping there is what
turned an accepted absence into a journal that refused to replay, one
seam down and in the lines that recognised it
(`docs/notes/2026-08-31-absence-is-three-things.md`, `FINDINGS.md` #36).
The sessions store also HEALS a torn tail now, which it never did, and
each store counts a healed tail apart from a record-less document.

**The pin was NOT bumped, on evidence.** jinnd's M2-K10 refuses a
reply-expecting dispatch that would close a cycle; nothing in this
four-layer stack can close one, because the profile's grant graph — which
BOUNDS the dispatch graph — is acyclic, and that is checked rather than
asserted
(`tests/composition/tests/workflows.rs::the_grant_graph_the_four_layers_compose_through_is_acyclic`).
`FINDINGS.md` #32 therefore stays open and the test it blocked was not
run.

**`FINDINGS.md` #35 is no longer derived.** It predicted that latency
compounds per layer because a composing seam must poll the one below, and
graded itself *derived, not measured*. This seam adds the fourth layer it
was about, so the entry now carries real end-to-end numbers at two, three
and four layers from one daemon, with the poll periods stated.

### Phase 2.5 — kernel pin `3a8e5c0` (M2-K9), UNCHANGED. The todos seam
(`plugins/todos/`) is the fifth core-port seam and the first that is
THREE layers deep: a Todo is dispatched to a SESSION through the
`jinn-session` DEFINITION, and the session drives an engine through
the engines definition, so `jinn:todo.<store>` ->
`jinn:session.<store>` -> `jinn:engine.<id>` composes with no layer
naming the next one's provider. The layering is enforced by
AUTHORITY: a Todo store's entry is granted no `jinn:engine.<id>` at
all.

The ledger honesty this seam owes is by construction, not by a
provider remembering. A status move is legal or REFUSED from an
explicit table that is nowhere near any-status-to-any-status (a
producer does not close their own work; a terminal status has no
exit), the refusal is typed with the attempted `from -> to` as DATA,
and `Todos::plan_update` answers a refusal carrying its own record — so no
code path refuses without recording. A dispatch reads back `done` only
where a terminal record was written; a started dispatch with no ending
replays `interrupted` with a reason, and `running` cannot be produced
from a file at all.

**A status this store reports is a status a durable line justifies.**
Every mutation is three steps in one order: PLAN what would happen
(nothing is touched), APPEND the record, and only then COMMIT it into
the registry. So a `jinn:fs` append that refuses leaves the reported
status exactly where it was, with an unchanged history and a
byte-identical journal, and a restart replays what the live view was
already saying — proven by withdrawing `append` from the durable store's
grant and reading all three
(`tests/composition/tests/todos.rs::a_status_no_durable_line_justifies_is_never_a_status_this_store_reports`).
The definition has no method that advances state and writes nothing.

**A Todo is never eternally `executing`.** The round's two defects were
both found by the real-composition gate, and the first is the one worth
knowing (`docs/notes/2026-08-30-todos-the-fold-is-not-enough.md`): a
DERIVED status — the sessions seam's discipline, correct there — leaves
the ledger unusable the moment the status is an ARGUMENT and not just an
answer. An operator was shown `blocked` and refused every move `blocked`
admits, because the record still stood at `executing`. Adoption now
RECORDS the recovery as a real status-changed line: a new event appended
after the ones already there, never an edit, carrying the dispatch's
reason and no actor. The second defect: a torn tail was tolerated on read
and then appended past, fusing into an unreadable hole — the store now
heals the document and reports `healed-tails`. `FINDINGS.md` #34 and #35
are the kernel side.

### Phase 2.4 — kernel pin `3a8e5c0` (M2-K9). The sessions seam
(`plugins/sessions/`) is the fourth core-port seam and the first that
COMPOSES another: its definition (`jinn-session`) binds a session to the
ENGINES definition, so a store drives `jinn:engine.<id>` and neither seam
knows the other's provider. Landed this round: the definition — the
contract vocabulary, the durable journal's record law and its honest
replay (a turn reads back `done` only where a terminal record was
written; a started turn with no ending replays `interrupted`, and
`running` cannot be produced from a file at all), and `Sessions`, the
registry every store shares. The store providers, the API routes, the kit
and the real-composition proofs are the next round's.

Pin-bump 8 (`3fd7b05` → `3a8e5c0`) closed `FINDINGS.md` #31: a
reply-expecting dispatch to a fiber that owes a change now REFUSES typed
and ledgered, and `jinn:introspect` 0.2.0 answers the same state. The
settings-recovery test it blocked is still `#[ignore]`d — the bump made
the NEXT defect reachable, logged as #32 (entry 4's nested-dispatch
deadlock, with two transcripts at last, and the half entry 4 never named:
the fiber that loses it may never come back).

The distribution's wire law now has one home (`jinn_settings::wire`)
instead of two halves in two seams.

Sessions, the first seam that COMPOSES another. A definition (`jinn-session`) whose contract name
carries the store id, so several stores are live at once; `jinn-session-fs`
keeping one append-only JSONL journal per session over `jinn:fs` and
replaying it honestly on activate; `jinn-session-memory` as the ephemeral
store — a genuine use, and the swap proof. NEITHER spawns an engine:
both inject the engines seam's DEFINITION and drive whatever answers,
which is the layering the phase is for. The operator API gains the
sessions surface, and `session-kit` builds the profile. Restart honesty is
an ORDERING, not a recovery pass: the `turn-started` record lands before
any engine is asked for anything, so a daemon killed mid-turn comes back
with that turn `interrupted` and a reason — proven by SIGKILLing a daemon
with a child-backed run in flight (`tests/composition/tests/sessions.rs`).

Phase 2.3 — kernel pin `3fd7b05` (M2-K8): the keystore, and engines.
The engines seam (`plugins/engines/`) is the third core-port seam: a
definition (`jinn-engine`) whose contract name carries the engine id, so
several providers are live at once; `jinn-engine-claude` and
`jinn-engine-codex` spawning their CLIs through `jinn:process` under an
executable allowlist and an env allowlist; `jinn-engine-echo` as the
no-CLI provider that carries the seam anywhere; a probe consumer on a
schedule and the engines exposed over the operator API. Switching,
coexistence and extension are all profile edits, proven against the real
daemon. Secrets are keystore REFERENCES, resolved by the provider at
spawn time through a read-only prefix grant — the pin's `jinn:keystore`
is the seam's first consumer, and closes the last of `FINDINGS.md` #5.
The same pin retires four more harness workarounds: atomic `jinn:fs`
commits (#22), read-only grant attenuation (#24), the `jinn:profile`
read views that let a consumer read the document of record wherever it
sits (#25), and a `patch-entry` that no longer awaits the restart it
schedules (#26).

Phase 2.2 built the settings seam (`plugins/settings/`) on pin `57360cc`
(M2-K7): a definition (`jinn-settings`) with declared namespaces,
schemas, defaults, layered resolution and typed secret references, a
profile-backed provider writing through `jinn:profile`, the cron
scheduler consuming its job table through it, and the API exposing it.
Phase 2.1 built the operator API (`plugins/api/`) on `1b098be` (M2-K6:
`jinn:process` and `jinn:net`): it answers from the kernel's own
knowledge — the composition through `jinn:introspect`, the ledger
through `jinn:ledger`, edits through `jinn:profile`, the HTTP listener
served from `jinn:net` readiness wakes with no alarm at all. Phase 1.7
put the guests on `jinn:plugin@0.3.0` (a daemon stop SUSPENDS a plugin;
only removal from the profile withdraws it), 1.6 on the `jinn:fs@0.2.0`
bundle, 1.5 on `jinn:clock` alarms; the phase-1.4 soak continues across
all seven bumps (`SOAK.md`). Every seam is proven by real-composition
tests that boot the pinned `jinnd` daemon (`tests/composition`). Kernel
frictions found on the way are logged in `FINDINGS.md` (the two-way
iteration channel — kernel changes are never made here).

## Layout

| Path | What it is |
|---|---|
| `AGENTS.md` | Standing orders for agents working in this repo |
| `KERNEL-PIN.md` | The kernel pin: jinnd commit + contract hashes + bump procedure |
| `kernel-pin/` | Vendored copy of the pinned contract surface (`wit/`, `contracts/`) — integrity-gated against `KERNEL-PIN.md` by `harness-pin` |
| `tools/harness-pin` | The pin gate: computes/verifies contract hashes (`cargo test -p harness-pin`) |
| `tools/cron-kit` | Builds the cron seam's components + pinned profile; its library is the kit machinery every seam kit shares |
| `tools/api-kit` | Builds the operator-API profile: the api trio beside the cron seam |
| `tools/engine-kit` | Builds the engines profile: the engine providers and the probe beside the api trio |
| `tools/session-kit` | Builds the sessions profile: the two store providers beside the engine providers |
| `tools/todo-kit` | Builds the todos profile: the two Todo stores above the two session stores |
| `tools/workflow-kit` | Builds the workflows profile: the two run stores above the two Todo stores |
| `tools/plugin-kit` | Builds the plugins profile: the two catalogs beside the api trio, plus the disabled and misbound entries the honesty proofs need |
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
- **Sibling activation order is unspecified** (#7), which is what keeps
  #30's window open.
- **A vendor CLI is exercised only where an operator names one.** The
  todos and workflows vendor legs self-skip in CI; sessions has no vendor
  test at all.

### 2.1 — Operator API

- **There is no authentication or authorization. (named here)** Loopback
  plus the port the `jinn:net` grant scopes is the ENTIRE boundary. No
  token, no bearer, no per-route authority. Anything on the machine that
  can reach the port is an operator.
- **No outbound HTTP exists at any pin so far.** `jinn:net` v0.1 has no
  `request`, no TLS, no non-loopback listen. **Connectors — Slack,
  Telegram, webhooks, any vendor API — are therefore structurally
  impossible in this repo today. (named here)**
- **The swap proof for this seam swaps a second entry of the SAME
  artifact,** because no second transport shape can exist.
- **`patch-entry`'s `idempotency-key` is accepted and unused.**
- **The API seam recorded no "could not prove" section of its own.
  (named here)** No concurrency proof, no load proof, no auth proof.

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

- **A plugin's swap through the operator API can only ever be a CONFIG
  swap** (#37). Every seam from 2.3 onward proves its malleability by
  changing an entry's `package` and `hash` in the profile FILE; this one
  operated the surface a person or an agent actually has and found that
  `jinn:profile.patch-entry` writes one subtree — `config` — and nothing
  else. The distribution's headline claim, *a product is a profile and
  swapping a provider is a profile edit*, is true today only of an
  operator with filesystem access to the document. This seam's own swap
  works because it was DESIGNED so its binding is decided by config; a
  seam that did not think of that in advance has no API-driven swap at
  all. It has a transcript, and it goes to the M3 parity conversation
  intact.
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
- **There is no lifecycle event surface at all, anywhere** (#40), and
  this seam ships no typed event as a recorded decision rather than an
  oversight. The kernel commits every `FiberTransition` to the ledger but
  is not a publisher on the plugin event bus, and there is no listen topic
  for a lifecycle change, so the only event this seam could emit is one a
  poller synthesised by diffing two snapshots — announcing a transition it
  did not witness and cannot time. The refusal is written at the place a
  person would go to add one (`jinn-plugins`'s module doc), because
  without it the next reader closes the gap with exactly that poller. The
  kernel-side fix is carded as **M2-K13**.
- **An entry the document could not RESOLVE appears in no catalog at
  all** — a list here omits exactly the entries that are most broken.
- **The surface is read-only,** and the join is three reads at three
  instants rather than one atomic view.

### What is not here at all

Beyond the six ported seams and this one, the old gateway's surface has
**no plugin in this repo**: connectors, chats, notes, experiments, the
org and its employees, delegation, approvals, knowledge, the MCP surface,
and the web UI. Cron exists as phase 1's seam and has not been revisited
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
