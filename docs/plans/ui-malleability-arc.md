# The UI malleability arc - plan

**Status:** PLAN, docs only (PLA-347), APPROVED by the COO on 2026-09-02 with the rulings recorded in §8. Nothing in this document is implemented.
The first packet (§4) is carded to dispatch the day the 2026-09-04 §7(b) soak
audit closes; the later phases (§3) are cards with one decision each and are
re-priced when their turn comes. Every number marked *estimate* is one.

**Written against.** jinn-harness `main` at `2149d82` (seams 2.1 through 2.8
landed); kernel pin `85d36b4` (`jinn:plugin@0.10.0`, `jinn:net` 0.3.0 with
outbound and TLS, `jinn:auth` 0.1.0, `jinn:introspect` 0.5.0 -
`KERNEL-PIN.md`); the existing web UI at `43e8647` exactly as surveyed by
`docs/notes/web-ui-port-inventory.md`, cited below as **inventory §N** and
nowhere re-derived. The kernel facts cited by file are read at the pin.

**What it obeys.** The direction is recorded on PLA-347 and restated in the
inventory's preamble: port the existing UI and its quirks close to verbatim
("we don't reinvent it and have to fix all of the bugs we banged our heads
against the wall for. we just make it malleable"); the UI is a RENDERER of a
plugin tree, never an extension API; a user extension is a waterfall listener
on the kernel's one event bus (SOURCE-OF-TRUTH §3, "Events"); the user-facing
tier is JS inside a WASM plugin so Laws 1 and 5 stand unamended; port the
pixels and the fixes, not the spine (inventory, "The line it makes
actionable"). And the cutover rule, verbatim from `AGENTS.md`: **the old
gateway keeps ALL production until parity.**

**How to read this.** §1 is the ground the plan stands on, every claim cited.
§2 is the shape. §3 is the arc, one card per phase. §4 is the first packet,
dispatch-ready. §5 is the extension tier's feasibility with the measured
spike. §6 traces the operator's own example through the seams end to end.
§7 is what this arc will not do. §8 is what the COO has to decide.

---

## 1. Ground truth

Facts this plan rests on, with where each one lives. Nothing below is a
design choice.

**About the existing UI (inventory).**

- The production source is 85,653 lines across 614 files; the irreducible
  shell is roughly 15,600 lines; surfaces are roughly 70,000, of which Chat is
  19,960, Todos 12,700, Workflows 5,400, with Talk's 10,810 outside the route
  system entirely (inventory §1.4, §1.5).
- `components/chat` cannot move as one surface: ten of its modules are
  imported by seven other surfaces and by core hooks, so the chat port begins
  with an extraction (inventory §1.5, last paragraph). `globals.css` (1,557
  lines) moves as one unit or not at all (same place).
- The client is origin-agnostic by construction, but `/api/*` and `/ws` must
  be on the SAME origin as the app; a split origin needs a real CORS and
  credentials design, not a config change (inventory §3.5, §3.6 item 5). The
  old gateway's route order - CORS, OPTIONS, auth gate, `/api/` dispatch,
  static - is itself a stated security property (inventory §2.15, §2.24).
- The old gateway serves the document `no-cache`, hashed `/assets/*`
  `immutable`, a missing asset as `404 text/plain` and never the SPA
  fallback, and `.webmanifest` as `application/manifest+json`; every one of
  those was an individually hunted bug (inventory §2.15, §2.16, §2.24).
- The client holds no bearer token anywhere; auth is HttpOnly cookie pairing
  gated by `AuthGate` over a four-field `GET /api/auth/state` contract
  (inventory §3.3). The door built by packet 2.8 forbids exactly a cookie as
  identity and is bearer-only (`docs/notes/2026-09-02-the-door-presents-what-it-was-given.md`).
- The realtime channel is one WebSocket carrying a 33-name event map;
  frames failing their payload guard are dropped silently, so a near-miss
  shows as a dead UI, not an error; `status: 'running'` must land
  synchronously at enqueue or three separate client mechanisms are left
  patching over its absence (inventory §3.2, §3.6 items 1 and 2).
- The Todo status vocabulary is eight statuses with `done -> backlog`
  reopening on the web side, six with terminal-is-terminal on the harness
  side, and the web mirrors the edge table in a checked-in JSON held by a
  parity test (inventory §2.19, §3.4 Tier 3, §4.2).
- The workflow editor compiles against the old daemon's source tree through
  a Vite path alias - the single hardest coupling, and not an endpoint
  (inventory §3.6 item 9, §6.6).
- The disk-plugin system's own loader says it is error isolation, not a
  capability boundary; its events are `void`, inbound only, with no return
  channel (inventory §1.3, §4.4).
- The existing UI's own optimistic-send reconciliation matches a server row
  to the optimistic bubble by a content-identity key (role, content, media
  fingerprint) as well as by id; a content change between the two shows the
  message twice (inventory §2.7 G1, G2).

**About the harness at `2149d82` (its own README and notes).**

- `jinn-api-http` is one `jinn:net` loopback listener served from readiness
  wakes; no keep-alive (every response closes), no chunked encoding, no
  server push, no static-file serving of any kind; request head cap 16 KiB,
  body cap 256 KiB (`plugins/api/jinn-api-http-wire/src/lib.rs`; inventory
  §4.1 "api").
- The door: every parsed request is exactly one `jinn:auth` `verify` before
  any dispatch on the connection's behalf, no grant cached, 401 with the
  `WWW-Authenticate: Bearer` challenge; the contract's own words are that a
  transport "issues NO dispatch on that connection's behalf before this call
  answers `principal`" (README §2.8; the 2.8 note).
- `jinn:profile.patch-entry` writes ONE subtree, `config`. A package-and-hash
  swap, an added entry, a removed entry, or a `disabled` toggle is not
  reachable through the operator API at all (`FINDINGS.md` #37; the 2.7 note
  `docs/notes/2026-08-31-the-catalog-is-the-swappable-part.md`).
- Every feed on the harness side is a cursor read, because a listener that
  calls back into the seam that is delivering to it deadlocks to the guest
  deadline (`FINDINGS.md` #4/#32, unretired; latency compounds per layer,
  #35, measured 513/755/1084 ms at two/three/four layers).
- The plugins seam is read-only and answers a guest's activation failure with
  its state and never its reason (`FINDINGS.md` #38).

**About the kernel at `85d36b4` (read at the pin).**

- The bus has five dispatch modes; `waterfall` is in the pinned WIT and the
  return channel exists: `events.emit(topic, mode, target, payload) ->
  result<list<list<u8>>>` and `lifecycle.handle-event -> result<list<u8>>`
  (`kernel-pin/wit/plugin.wit`; inventory §4.0). The harness has never used
  `waterfall` (inventory §4.0, fact 1).
- Waterfall semantics, exactly: the payload is handed to listeners in
  REGISTRATION order; a NON-EMPTY output replaces the payload for the next
  listener; an empty output leaves it unchanged; a failing listener is
  recorded and the walk continues (R9); the final payload is the one output;
  every dispatch lands a `DispatchTrace { topic, mode, listeners, failures,
  emitter }` ledger row (`crates/jinnd-wasm/src/topics.rs`,
  `crates/jinnd-events/src/dispatch.rs`, `crates/jinnd-events/src/table.rs`).
- A reply-expecting walk is refused whole - `restarting`, `gone`,
  `suspended`, `stalled` - if any selected listener's incarnation owes a
  transition; nothing is delivered (`plugin.wit`, `events.emit`; M2-K9). A
  walk that would close a wait cycle is refused in every mode (M2-K10).
- A subscription is covered by the grant of the topic's own name
  (`plugin.wit`, `events.listen`). Grants are written on the entry under
  `config.grants`; a topic name is a grant like a contract name is
  (`tools/cron-kit/src/lib.rs`, `cron_entries`).
- The guest deadline is 5 s per crossing (`crates/jinnd-wasm/src/lane.rs`,
  `DEADLINE`); fuel metering is on with a 10,000-fuel yield interval and no
  per-delivery cap (`crates/jinnd-wasm/src/instance.rs`).
- Sibling activation order is unspecified (`FINDINGS.md` #7), and listener
  order is registration order - so two extensions on one topic compose in an
  order nothing declares.
- A profile entry's `package` plus `hash` pins one artifact at
  `<artifacts>/<basename>.wasm` with a `.sha256` sidecar; that is the
  content-addressed unit the kernel loads and hot-swaps
  (`crates/jinnd-daemon/src/paths.rs`, `watch.rs`; constitution 04, 05).
- Constitution 04 names `jinn:profile-admin` as the separate
  operator-authorized capability for grants, identity, nesting and
  `disabled`; no such contract exists at the pin.

---

## 2. The shape

One repo, the Rust workspace, the TypeScript client inside it under `web/`
(not a Cargo member - guests are not either, `Cargo.toml`), built by a kit,
its output embedded in ONE plugin artifact, served by the transport the
distribution already has, and extended through the bus.

```text
browser ──HTTP/1.1 loopback, one origin──▶ jinn-api-http  (the transport; holds jinn:net, jinn:auth)
   │  GET /, /assets/*, /manifest.webmanifest │  answered from memory the transport filled at
   │  (public bytes: no door, no crossing)    │  activation from ONE crossing on ▼
   │                                          │  jinn:ui-bundle ─▶ jinn-ui-bundle-<name>  (provider: the bundle
   │  GET|POST|PATCH /v1/*  + Bearer          │                    IS its artifact; swapping the UI is swapping
   │  (the door: one verify per request)      │                    this entry's package+hash, and nothing else)
   │                                          │
   │  POST /v1/moments/<topic> + Bearer  ─────┼─▶ events.emit("jinn:ui/<topic>", waterfall, all, body)
   │                                          │        │ registration order
   ▼                                          │        ▼
 the view layer, ported verbatim,             │   jinn-ext-js-<engine>  (Tier A guest; JS engine inside;
 its data layer re-seated on /v1              │   config = the user's source; grant = the topic name)
```

Three seam triples, named per `AGENTS.md` standing order 4:

| Seam | Definition | Providers | Consumers |
|---|---|---|---|
| The bundle | `jinn-ui` - contract `jinn:ui-bundle`: `manifest` (paths, sha256, mime, immutable?) and `bundle` (the whole archive as one blob) | `jinn-ui-bundle-embedded` (the built `web/out` embedded at kit time; the artifact hash pins the UI) | `jinn-api-http` (injects the definition, serves the bytes) |
| Moments | `jinn-ui` again - the `jinn:ui/*` topic vocabulary, each topic's payload schema, and the fail-closed law | none: a moment is a walk, not a service | `jinn-api-http` (emits), every extension (listens) |
| The JS-in-WASM extension tier | `jinn-ext` - the extension entry's config schema (`topics`, `source`, `origin: agent \| human`, `budget`) and the activation law: the guest registers the source's sha256 as a breadcrumb (Law 2) | `jinn-ext-js-boa` first; `jinn-ext-js-quickjs` if and when §5's gap closes | none: an extension is a listener |

Why the transport serves the bundle rather than the bundle plugin holding its
own `jinn:net` grant: the client requires `/api/*` and the app on one origin
(inventory §3.6 item 5), one port is one listener, and the alternative -
two ports and a CORS layer on the door - adds a preflight surface the wire
does not have and the 2.8 proofs never covered. The Todo's "a plugin holding
a `jinn:net` grant serves the pinned bundle" is satisfied literally, by the
plugin that already holds it. The bundle stays a separate entry because that
is what makes the UI a profile edit and the transport a transport.

Why "from memory filled at activation" and not per request: the door's
contract forbids a dispatch on an unauthenticated connection's behalf, and a
browser's top-level navigation cannot carry a bearer header. Reading the
bundle ONCE, at `activate`, as an injected dependency, keeps both true: no
request for a byte causes a crossing, and a bundle swap forces the transport
through the kernel's own epoch gating (SOURCE-OF-TRUTH §3) - it restarts,
re-reads, and serves the new hash. The cost is a ~30 ms blip of the API port
on a UI swap (`FINDINGS.md` #27's measured reconcile) and one ~4 MB payload
across the broker per transport activation. Both are stated in §4 as
measured acceptance, not assumed.

---

## 3. The arc

Seven phases. Each is one packet card with ONE design decision; acceptance is
composition proofs against the pinned daemon built from `git archive` of the
pin (`tests/composition/src/daemon.rs`), never a hand-mounted test
(`AGENTS.md` standing order 3); kernel gaps are candidate jinnd cards, never
harness workarounds (standing order 1). Ordering follows inventory §4.5 with
one deliberate inversion, stated at UI-2.

| Phase | Packet | The one decision | Depends on | Estimate |
|---|---|---|---|---|
| UI-1 | UI-as-profile | The bundle is a plugin's artifact; the transport serves it same-origin from memory filled at activation | 2.8 | 2 rounds, ~4 agent-days *(estimate)* |
| UI-2 | Moments and the JS-in-WASM extension tier | A moment is a `jinn:ui/<topic>` waterfall the transport dispatches for `POST /v1/moments/<topic>`, fail-closed | UI-1 | 2 rounds, ~4 agent-days *(estimate)* |
| UI-3 | The live half | Server-sent events over held loopback connections, fed by cursor reads on one alarm; the client's vocabulary becomes the harness's | UI-1 | 2 rounds, ~5 agent-days *(estimate)* |
| UI-4 | Todos | One status vocabulary: the harness's six-status terminal law wins; the client's edge table is GENERATED from the definition | UI-3 | 3 rounds, ~6 agent-days *(estimate)* |
| UI-5 | Workflows | The editor compiles against a PUBLISHED wire package generated from `jinn-workflow`, never a path alias into a source tree | UI-4 | 2 rounds, ~5 agent-days *(estimate)* |
| UI-6 | Chat | The transcript reducer keeps its shape; `session:delta` is derived from `jinn:session/event` by ONE adapter with a parity suite | UI-3, UI-2 | 3 rounds, ~8 agent-days *(estimate)* |
| UI-7 | The plugin tree | The seven fixed areas become a tree read from `jinn:plugins`; nav is the `nav:after-build-tree` waterfall | UI-2, UI-1 | 2 rounds, ~4 agent-days *(estimate)* |

### UI-1 - UI-as-profile

Fully carded in §4. In one line: the shell (inventory §1.4) and two surfaces
with a direct seam fit - Settings (inventory §4.2, the only `direct` row) and
the plugins page (read-only, three ops) - ported verbatim, served from a
profile, behind the door.

### UI-2 - Moments and the JS-in-WASM extension tier

Fully carded in §9 (PLA-353). In one line:

**Decision.** A moment is a waterfall on a `jinn:ui/<topic>` topic that the
transport dispatches when an authenticated client calls
`POST /v1/moments/<topic>` with the moment's payload, and answers with the
folded payload. A refused walk (`restarting`, `gone`, `suspended`,
`stalled`, `cycle`) is answered `503 unavailable` naming the refusal, never
silently the unmodified payload: a validator extension ("refuse a send
containing an API key", inventory §4.3 moment 1) is defeated by fail-open,
and the client retries once after the ~30 ms a restart takes. The extension
tier is `jinn-ext-js-boa` (§5): a Tier A guest whose config carries the
operator's JS source and the topics it listens on, whose only authority is
the topic names in its grants, and whose JS has NO host calls - so it cannot
re-enter a seam and #4/#32 cannot reach it.

**Why before the live half (the inversion of inventory §4.5).** The arc
exists for this phase; UI-1 already carries a working data path for the
moment endpoint; and the operator's example (§6) needs no push transport to
be demonstrated end to end.

**Scope.** `jinn:ui/before-send` and `jinn:ui/before-create-session`
(inventory §4.3 moments 1 and 3, both with a gateway twin) as the first two
topics; the client's `sendText` choke point (inventory §4.3 moment 1) calls
the moment from the re-seated data layer BEFORE the optimistic bubble is
built, so inventory §2.7 G1's identity key matches the server twin (§6).
`jinn-ext` definition, `jinn-ext-js-boa` provider, `ext-kit`. Two rules from
the §8 ruling on source-as-config: the entry's data carries `origin: agent |
human` (constitution 05's attestation, restated for data) and the plugins
page shows it; and the guest records the source's sha256 as an activation
breadcrumb (`effects.register("source sha256:<hex>")`) so WHAT CODE RAN is on
the record (Law 2).

**Acceptance (composition proofs, `tests/composition/tests/moments.rs`).**

1. `a_moment_with_no_listener_answers_its_own_payload` - one `DispatchTrace`
   row with `listeners: 0`, the body echoed.
2. `one_js_extension_folds_the_payload_and_the_ledger_says_so` - the
   operator's `(p) => ({...p, text: p.text + " 🟢"})` from a profile entry;
   the answer carries the emoji; `DispatchTrace { listeners: 1, failures: 0 }`.
3. `two_extensions_compose_in_registration_order_and_the_order_is_named` -
   both fold; the answer shows both, in the order the ledger's listen rows
   record; the proof asserts the order it OBSERVED and the card records it as
   unspecified across siblings (#7) - KG-3 below.
4. `a_throwing_extension_is_recorded_and_the_walk_continues` - `failures: 1`,
   the other extension's fold present (R9).
5. `a_restarting_extension_refuses_the_moment_typed_and_nothing_is_sent` -
   patch the extension's config mid-walk; `503` naming `restarting`; zero
   session crossings on the ledger.
6. `an_extension_is_granted_its_topic_and_nothing_else` - an extension whose
   entry lacks the topic grant fails to listen, on the record.
7. `a_looping_extension_costs_the_send_the_guest_deadline` - `while(true){}`
   answers after the 5 s deadline, MEASURED and recorded, which is the
   evidence for KG-2.
8. Real-composition: the extension boots from a profile through the pinned
   daemon; the JS self-test at activation (§5.4) is the guest's own
   fail-closed check.

**Kernel gaps this phase will surface (candidate jinnd cards).**

- **KG-1 `jinn:profile-admin` - CARDED as jinnd M2-K23 (PLA-348, backlog,
  sequenced after the 2026-09-04 audit on the kernel lane; UI-1 does not
  wait for it).** Installing an extension is adding an ENTRY
  with GRANTS; `patch-entry` writes `config` only (#37). Until the capability
  constitution 04 names exists, "install an extension from the UI" is a file
  edit, and the card says so. Scope: add, remove, `disabled` toggle, grants,
  under an operator-authorized contract, every write a ledger row.
- **KG-2 per-delivery budget.** A `handle-event` has the whole 5 s guest
  deadline; a looping extension costs every send that much. Candidate: a
  per-listen fuel or deadline cap declared at `listen`, refused typed when
  exceeded, so a bad extension costs its own slot and not the walk.
- **KG-3 listener order is a declaration nowhere.** Registration order is
  activation order, which is unspecified across siblings (#7). Candidate: an
  ordinal on `listen`, or profile order honored and stated.
- **KG-4 WASI-lite for Tier A** - only if QuickJS is chosen; §5.

**Dependencies.** UI-1 (the data path and the door). Nothing in the live
half.

### UI-3 - The live half

**Decision.** The transport holds an authenticated `GET /v1/events?after=N`
connection open and writes `text/event-stream` frames to it from cursor
reads it takes on ONE `jinn:clock` alarm; the client's `lib/ws.ts` becomes
an `EventSource` client with the same backoff (inventory §2.19, equal
jitter), and the client's event vocabulary becomes the harness's:
`@jinn/gateway-events` is NOT ported; the 35 importing files (inventory §6.6)
move to a generated `web/src/lib/harness-events.ts` whose guards are derived
from the seams' own Rust types.

**Why this and not a WebSocket or a push from the seams.** The wire has no
upgrade path and the door is per-request; SSE is a plain authenticated GET
that never closes, which the transport can already hold (a connection is a
kernel registration; readiness wakes are level-triggered). A push from a
seam is #4/#32: a store cannot emit from inside a delivery, so the transport
polls the cursors it already exposes. The cost is one poll period of
latency and it is the honest bound (#35).

**Scope.** `/v1/events` on the transport; `harness-events.ts` and its
generator; `use-gateway`, `use-query-invalidation` (inventory §2.19) re-seated
verbatim on it; the status bar's connection indicator.

**Acceptance.** `a_held_connection_receives_every_event_after_its_cursor_in_order`;
`a_closed_client_releases_its_registration_on_the_record`; `latency_from_seam_row_to_frame_is_measured`
(the number joins #35); `a_frame_failing_its_guard_is_dropped_and_counted`
(inventory §3.2's silent drop made visible); `reconnect_resumes_from_the_cursor_with_no_gap`.

**Kernel gaps.** #35 (measured again, now with the client attached); #4/#32
(the reason for the poll); the bounded event rings' drop counts per seam
(README, 2.4 and 2.5) - a candidate for a kernel-level bounded, cursored
publish path rather than six guest rings.

**Dependencies.** UI-1.

### UI-4 - Todos

**Decision.** One status vocabulary. The harness's six-status law with
terminal-is-terminal wins (inventory §4.2, §4.1 "todos"); the web's
`transition-edges.json` mirror (inventory §2.19, §3.4 Tier 3) is GENERATED
from `jinn-todo`'s table by the kit, and the parity test that held the old
mirror (inventory §2.23) is re-pointed at the generator. `assigned` and
`escalated` and `done -> backlog` are not ported; the board's columns are
the definition's statuses.

**Scope.** Board (deliberately NOT virtualised - inventory §2.21), list,
task page (wire types to the leaves, 20 files - inventory §3.4 Tier 1),
quick-add; the `*-wire.ts` layer extended (inventory §3.6 item 10).

**Acceptance.** `a_drag_the_client_pre_checks_legal_is_legal_on_the_daemon`
(generated mirror vs definition, every edge); `a_refused_move_is_shown_from_its_recorded_refusal`;
`an_optimistic_status_is_version_fenced_against_the_definition_s_version`;
board and list render-cost budgets carried (inventory §2.23).

**Kernel gaps.** None expected in the kernel; the harness side owes edit and
delete on a Todo and a comment (README 2.5), approvals, labels and
attachments - each a harness seam packet, listed on the card as
prerequisites, not workarounds.

**Dependencies.** UI-3 (the board is live-updated), UI-1.

### UI-5 - Workflows

**Decision.** The editor compiles against a published wire package generated
from `jinn-workflow`'s Rust types, never a path alias into a daemon source
tree (inventory §3.6 item 9 is the coupling this retires). The canvas's
hand-maintained `edgeTaken` mirror (inventory §2.21) is replaced by the run
store's own `edge-activated` reading.

**Scope.** List, editor (three canvas surfaces each import the React Flow
stylesheet - inventory §2.21, carry it), run canvas, run inspector.

**Acceptance.** `a_definition_saved_by_the_editor_round_trips_the_definition_byte_for_byte`;
`a_run_is_pinned_to_its_revision_and_the_canvas_shows_which`;
`the_canvas_paints_the_edge_the_run_store_recorded`.

**Kernel gaps.** None in the kernel. Harness prerequisites: triggers,
enable/disable/retire/duplicate, node retry, run approvals, the
`expectedRevision` 409 contract (inventory §4.2).

**Dependencies.** UI-4.

### UI-6 - Chat

**Decision.** The transcript reducer (`use-live-session.ts`, 1,428 lines,
plus `blocks.ts` - inventory §3.4 Tier 1, §3.6 item 8) keeps its shape;
`session:delta`'s eight sub-types and the five session events are DERIVED
from `jinn:session/event` by one adapter module with a parity suite that
replays recorded harness event pages and asserts the exact frames the old
gateway would have produced. `status: 'running'` at enqueue (inventory §3.6
item 2) is answered by the session definition's own law: `send` answers
`TurnAccepted` at once and the `turn-started` record lands before any engine
is asked (`docs/notes/2026-08-30-sessions-seam-stores.md`).

**Scope.** The extraction first (inventory §1.5: ten cross-imported chat
modules become core), then the chat surface; every quirk in inventory §2.1
through §2.13 carried verbatim, the unexplained carries (§2.13) byte for
byte.

**Acceptance.** The render-cost budget (a streaming token executes zero row
bodies, inventory §2.23); `the_optimistic_bubble_settles_on_the_first_frame_not_the_response`
(inventory §3.1, send); `a_dropped_completion_is_recovered_by_the_watchdog`
(inventory §2.8 H2, driven through the daemon); `a_send_from_cron_passes_the_same_before_send_moment`
(the gateway-side half of §6).

**Kernel gaps.** Attachments need a file seam and there is none (inventory
§4.2, §4.3 moment 7); interrupt; the block envelope. All harness seams, not
kernel.

**Dependencies.** UI-3, UI-2.

### UI-7 - The plugin tree

**Decision.** The seven fixed contribution areas (inventory §1.3) become a
tree whose nodes are read from `jinn:plugins` `list`/`describe`; the nav is
the `nav:after-build-tree` waterfall (inventory §4.3 moment 8) so an
extension reorders, hides or renames without a plugin API. The contrib
registry's provenance stamping, boundary and namespacing survive unchanged
(inventory §1.3, "Could a plugin tree subsume it?"); the no-build ESM door
(inventory §2.20) is NOT ported - the extension tier is the WASM one.

**Acceptance.** `a_contributed_route_cannot_shadow_a_core_route` (inventory
§1.3 route semantics, carried); `hiding_a_surface_is_a_profile_edit_and_the_nav_says_so`;
`a_failed_plugin_shows_failed_and_names_where_its_reason_would_be` (#38).

**Kernel gaps.** KG-1 (enable/disable from the page; M2-K23 / PLA-348); #38 (a reason).

**Dependencies.** UI-2, UI-1.

---

## 4. The first packet, carded: UI-1 - UI-as-profile

**Milestone:** M3 preparation (the arc's first packet; the web UI running
against the kernel API is M3's own acceptance line, SOURCE-OF-TRUTH §7) ·
**Owner:** kernel-dev - ONE build node; sub-agents allowed for the
verbatim TypeScript port because the diff gate (proof 6) is what makes a
mechanical port safe to parallelize; the seam and the proofs stay with the
card owner (§8 ruling 6) · **Status:** ready to dispatch after the 2026-09-04
§7(b) audit closes · **Binding rules:** `AGENTS.md` standing orders 1
through 5; jinnd R1 (no blocking in a guest), R3 (typed wire), R9 (no
silent replacement: a bundle swap is a restart), R11 (a bad bundle fails the
transport's activation, nothing else), R12 (additive contract, 0.x minor),
Laws 1, 2, 5 · **LOC ceiling (card-authoritative, binding):** production Rust net delta
**≤ 800** across `plugins/ui/`, `plugins/api/jinn-api-http/src`,
`plugins/api/jinn-api-http-wire/src` (added by §8 amendment 3 so the
framing rows the packet adds are billed, not excluded), `tools/ui-kit`. The
harness has no loc-meter, so the meter is declared here:
`git diff --numstat main -- 'plugins/ui/**/*.rs' 'plugins/api/jinn-api-http/src/*.rs' 'plugins/api/jinn-api-http-wire/src/*.rs' 'tools/ui-kit/**/*.rs'`,
added minus deleted, summed over every file that is not under a `tests/`
directory and not named `tests.rs`; a `#[cfg(test)]` module inside a
production file is a declared category - the PR lists each such module with
its line count and that count is subtracted, so the ceiling binds
production code and never incentivizes golfing a test. The composition
suite is excluded. The TypeScript tree carries NO line ceiling because its
acceptance is a DIFF against the pinned sha, not a size ·
**Standing gates:** `cargo fmt --check && cargo clippy --workspace
--all-targets -- -D warnings && cargo test --workspace`, plus the node lane
of §4.4, plus `cargo test -p composition`.

### 4.1 The one decision

The UI bundle is one plugin artifact, content-addressed by the kernel's own
`package` + `hash`, embedded at kit time; the transport injects the bundle
definition, reads the whole bundle ONCE at activation as a single crossing,
verifies every file's sha256 against the manifest (fail closed), and serves
the document and its assets from memory to any loopback peer with no door
and no crossing. Every `/v1/*` request keeps the door exactly as 2.8 left it.
A UI swap is a profile edit of the bundle entry's `package` and `hash`; the
kernel's epoch gating restarts the transport, which re-reads.

### 4.2 Scope

**The bundle seam (Rust).**

- `plugins/ui/jinn-ui` (definition, workspace member): contract name
  `jinn:ui-bundle`; operations `manifest` (answers `{ files: [{ path,
  sha256, mime, immutable }], document: "index.html", bundle-sha256 }`) and
  `bundle` (answers one blob: u32-LE count, then per file u32-LE path
  length, path, u32-LE byte length, bytes); the serving law in prose: the
  document `no-cache`, `immutable` files `public, max-age=31536000,
  immutable`, unknown `/assets/*` answered `404 text/plain`, every other
  non-`/v1` path answered the document (inventory §2.15), `.webmanifest` as
  `application/manifest+json` (inventory §2.16). Additivity and the envelope
  as every seam (`plugins/api/jinn-api/README.md`).
- `plugins/ui/jinn-ui-bundle-embedded` (provider guest): `include_bytes!`
  of the kit's archive, `manifest` and `bundle` answered verbatim, no grant
  but its own contract. Its config is empty; its identity is its hash.
- `plugins/api/jinn-api-http`: at `activate`, resolve `jinn:ui-bundle`
  (when granted; the operator-api profile without a UI keeps serving `/v1`
  and answers `503 unavailable` on `/`), read `bundle`, verify, hold; the
  route family `GET /`, `GET /assets/*`, `GET /manifest.webmanifest`, and
  the SPA fallback, answered before the door is consulted and with no
  crossing; response framing gains the two `Cache-Control` values and
  `text/html`, `text/css`, `application/javascript`, `image/*`, `font/woff2`,
  `application/manifest+json` MIME rows. Responses still close (no
  keep-alive); a page load is N connections, and the proof counts them.
- `tools/ui-kit`: runs `pnpm --filter @jinn/web build`, archives `web/out`
  into the provider's `include_bytes!` input (`JINN_UI_BUNDLE_DIR`), builds
  and encodes the provider by the shared kit machinery, writes the `ui`
  profile (api trio + settings seam + plugins catalogs + the bundle).

**The client (TypeScript, `web/`).** Ported VERBATIM from `43e8647`
`packages/web/`, restricted to: the shell (inventory §1.4: bootstrap and
routing, the provider stack, the auth gate, layout and nav, theming,
transport, platform adapters, the contrib registry), the Settings surface,
the plugin settings page, and the `components/ui` family. Not one quirk from
inventory §2.16 through §2.19 and §2.22 that touches these files is tidied.
The ENUMERATED adaptations, and only these, are the diff:

1. `lib/api.ts`, `lib/api-config.ts`: the calls the two surfaces make,
   re-pointed to `/v1/settings[/{ns}]`, `/v1/plugins[/{catalog}[/{id}[/history]]]`,
   `/v1/status`, `/v1/health`, `/v1/engines`; each mapped in ONE adapter
   function per endpoint, never in a component (inventory §3.6 item 7's
   shape). The config editor's `X-Jinn-Config-Revision` token has no
   counterpart; the settings page's YAML editor is replaced by the seam's
   namespace patches, and the conflict notice (inventory §2.22) reads the
   seam's typed `refused`.
2. `lib/auth.ts`, `routes/auth-provider.tsx`, `components/auth/*`: the
   four-field state is SYNTHESISED client-side (`authRequired: true`,
   `authenticated` = a `GET /v1/health` with the held bearer answered 200);
   the pairing screen gains a "paste the operator credential" mode (the
   value read from `<data>.operator-token` - the 2.8 note says where); the
   credential lives in `sessionStorage`, never a cookie; `authFetch` adds
   `Authorization: Bearer`; a 401 clears it and shows the pairing screen
   (inventory §3.3's transparent retry becomes a transparent sign-out).
3. `hooks/use-gateway.tsx`: the socket is not opened; gateway status is a
   5 s `GET /v1/health` poll until UI-3. `use-query-invalidation.ts` is
   carried whole and receives no frames.
4. `routes/settings/plugins/*`: enable, disable, rescan and reveal are
   rendered disabled with the finding (#37 class, KG-1 / PLA-348); the row's
   lifecycle reading and `history` come from the catalog.
5. `src/main.tsx`, `lib/app-routes.ts`: the route table lists Settings,
   Plugins, the plugin splat and the two redirects; every other route is
   absent, and `nav.ts`'s feature-disabled snapshot (inventory §2.18) hides
   what is absent.
6. `vite.config.ts`: the PWA plugin and the service worker are REMOVED for
   this packet (§8, question 4); `preloadUiFont` stays; `manualChunks` stays;
   the three `@jinn/*-wire` aliases into the old daemon's source are gone
   (their consumers are not in scope); `@jinn/plugin-sdk` stays an alias.
7. `index.html`: unchanged, including the `crypto.randomUUID` polyfill and
   the blocking theme script (inventory §2.13 item 1, §2.17).
8. `routes/client-providers.tsx` (shell): the two Talk mounts
   (`TalkOrbOverlay`, `TalkContextBridge`) and their imports are REMOVED and
   nothing else changes - §7 forbids porting Talk and inventory §1.2 puts
   `components/talk` out of scope, so a verbatim port of this file would
   pull ~10.5k lines the card excludes. Ruled an adaptation on 2026-09-02
   (§8 amendment 3); proof 6 asserts a NON-EMPTY diff for it. The two
   zero-import leaves `@jinn/model-id` and `@jinn/fallback-map-wire` that
   `settings/engines/*` imports are ported VERBATIM from
   `packages/jinn/src/shared/` through the pinned map; they are not
   adaptations and the gate asserts an EMPTY diff against their source paths.
9. `routes/onboarding*`, `components/onboarding-wizard.tsx` and their mount
   in the shell: the onboarding flow is NOT on the port list (it is neither
   shell, Settings, Plugins nor `components/ui`) and its `/api/onboarding`
   calls are an old-gateway route the new daemon answers with the SPA
   document. Ruled on 2026-09-02 (§8 amendment 4): the mount is removed and
   the onboarding state is synthesised complete client-side; the files
   themselves are not ported. A repo test asserts that no string `/api/`
   remains in `web/src` outside the adapter files item 1 names.
10. `routes/settings/plugins/inventory.ts`: its `/api/plugins` read is
   re-pointed to the `/v1/plugins` catalog through item 1's adapter (one
   function, the same shape), because the mounted Settings surface issued
   it live (§8 amendment 6; the verifier's network transcript, round 2).
11. `lib/talk-capability.ts`: Talk is out of scope (§7); the capability
   resolves ABSENT client-side and issues no request (its `/api/talk/config`
   GET was live on the mounted Settings surface - amendment 6).
12. `plugins/disk-plugins.ts` (and its test): the old gateway's
   client-plugin loader, mounted on EVERY page by the `DiskPluginsBridge` in
   `routes/client-providers.tsx` and re-run on each connection flip - the
   actual source of the live `GET /api/plugins` the round-2 transcript
   recorded (amendment 6 attributed it to `inventory.ts` by grep; the call
   graph says otherwise). The disk door resolves EMPTY client-side (zero
   plugins, `settled` still flips so `ContributedRoute` renders) and issues
   no request; `/api/plugins/<id>/client` has no counterpart (#37 / KG-1).
   Ruled in §8 amendment 7.

Everything else is `git diff 43e8647 -- packages/web/<path>` empty, file by
file, and the acceptance below asserts it.

**Toolchain (inventory §6, decided here).**

- `web/` at the repo root; `pnpm@10.6.4` via `packageManager`, Node
  `24.13.0` via `.npmrc` `use-node-version` and `engine-strict=true`
  (inventory §6.2 - the pin records an ABI incident); Vite 7, Tailwind 4,
  TypeScript 5.8, Vitest 4, ESLint 10 as pinned there. No Turbo.
- `.gitignore` gains `node_modules/`, `web/out/`, `dist/`, `coverage/`,
  `*.tsbuildinfo` (inventory §6.3) BEFORE the first TypeScript file lands.
- The CI privacy firewall's tracked-tree check gains `node_modules/`, `out/`
  and `dist/` (inventory §6.4 item 1); the home-path pattern is unchanged
  and the node lane's snippets are written around it (item 2).
- The ratchet and its 153-entry web baseline come across (inventory §6.5,
  the recommendation) so the tree arrives green with its debt visible; the
  footgun gate does NOT come across in this packet (named in §7).
- The bundle is built in CI and by the kit and never vendored (inventory
  §6.10 item 2): its hash is the component's, printed by the kit, never
  hand-written.
- The node lane, gated as the Rust lanes are (inventory §6.9): install
  frozen, typecheck (this is where the gateway-events types test fires,
  inventory §2.23), lint, test, build, then `perf-budget` with its
  native-marker scan (inventory §2.24, "worth carrying").

### 4.3 Acceptance

Composition proofs in `tests/composition/tests/ui.rs`, each booting the `ui`
profile through the pinned daemon; every one runs red first against a
transport that does not serve:

1. `the_document_and_every_asset_are_served_from_the_pinned_bundle_by_hash` -
   `GET /` is 200 `text/html` with `Cache-Control: no-cache`; every
   `/assets/<hashed>` the document references is 200 with the `immutable`
   value and its bytes hash to the manifest's; `/manifest.webmanifest` is
   `application/manifest+json`; `/assets/missing.js` is `404 text/plain`;
   `/settings` answers the document (inventory §2.15, §2.16, §2.24).
2. `a_byte_is_never_a_dispatch_and_a_v1_request_is_always_the_door` - on the
   ledger, every connection segment that requested a document or an asset
   carries transport rows and nothing else (2.8's `provider_segments`
   discipline, reused); every `/v1` segment carries exactly one `verify`
   before its dispatch; a `/v1` request with no bearer is 401 (the 2.8 suite
   unchanged, now also green on the `ui` profile).
3. `the_bundle_crosses_once_per_transport_activation_and_its_size_is_recorded` -
   exactly one `jinn:ui-bundle` `bundle` crossing after the transport's
   activation; the ledger row count and the bundle byte count are printed
   into the proof's output and copied to the card's report.
4. `swapping_the_ui_is_a_profile_edit_of_one_entry` - edit the bundle
   entry's `package` and `hash` to a second kit-built bundle whose document
   carries a marker. AT PIN `a53a352` (jinnd M2-K24, adopted by pin-bump 7;
   flipped here per §8 amendment 4's own text): the swap IS a restart - the
   transport declares `injects: ["jinn:ui-bundle"]`, so the kernel unloads
   it under `DependencyChanged` and reloads it; its `incarnation` is
   ASSERTED +1 EXACTLY in the kernel's own vocabulary — one more LOAD of
   its fiber on its own rows, the introspect `incarnation` (an identity,
   never reused in a process) asserted to have moved and printed
   (harness FINDINGS #46, the transcript) — never merely omitted, exactly one more
   `jinn:ui-bundle` crossing lands on the ledger (one per incarnation), the
   one `Unloading` row names `DependencyChanged`, `GET /` answers the marker,
   the settings and plugins consumers' incarnations are unchanged, and the
   blip of the port between the two incarnations is MEASURED (refused
   connects, edit-to-marker), never asserted away. The transitions
   subscription, the `jinn:introspect` grant and the post-commit probe that
   stood in for this at `85d36b4` are removed (harness FINDINGS #45/#46,
   fixed at pin `a53a352`). Before the pin-bump this item read: the swap is
   a witnessed transition and a re-read, incarnation asserted UNCHANGED,
   refused connects 0.
5. `a_bundle_that_does_not_match_its_manifest_never_serves_a_byte` - boot
   the `ui` profile with a deliberately corrupted archive. AT PIN `a53a352`
   this is ONE order: the kernel activates the declared transport only once
   the bundle entry is `active`, the read and its verify run INSIDE that
   activation, the transport's fiber reads `failed` and the port never
   opens; one `manifest` and one `bundle` crossing on the record, no byte
   served; the settings and plugins consumers stay `active`, and the
   operator-api profile without a bundle entry still answers `/v1/health`
   200 and `/` a typed 503. The two orders `85d36b4` had (#45), and the
   forced late-provider order that item covered, are no longer reachable: a
   declared consumer without its provider rests `pending`, never `active`
   without its bundle.
5b. `a_fresh_boot_is_deterministic` (added by §8 amendment 4) - ten
   consecutive boots of the `ui` profile, each on a FRESH root, through the
   pinned daemon from git archive: every boot reaches the transport `active`
   AND listening, `GET /` answering the document, within the suite's ready
   budget. The verifier reproduced the coin toss by hand (transport
   `failed`, bundle `active`, port never opened, second boot fine); a UI the
   operator is to test cannot boot on a coin toss. If the pinned kernel
   cannot be made deterministic from the harness side within Law, the
   packet lands as NOT-YET on this item with M2-K24 named as the unblock.
   At pin `a53a352` the item stays and passes with NO harness-side
   workaround: the kernel's gate on the declaration is the determinism.
6. `the_view_layer_is_verbatim` - not a daemon proof: a repo test that runs
   `git diff --stat 43e8647 -- packages/web/<f>` for every ported file
   against `web/<f>` through a pinned mapping and asserts an EMPTY diff for
   every file not on the §4.2 adaptation list (items 1-12), and a non-empty one
   for every file on it (the gate has to be able to fail in both
   directions).
7. Browser-level, driven by the INDEPENDENT VERIFIER with `agent-browser`
   against a throwaway root (§8 amendment): open `/`, be shown the pairing
   screen, paste the credential, see Settings and Plugins, patch a DECLARED
   setting of a namespace and read it back (§8 amendment 4: the Settings
   page renders only the settings the namespace's schema declares, mapped
   one adapter per endpoint; an undeclared field is hidden with the
   inventory row that names it, never sent). The verifier posts the transcript on the Todo; no
   person is in the acceptance loop.

Plus: node lane green; `cargo test -p harness-docs` green; the privacy
firewall green with the widened check; every quirk carried is named on the
PR by its inventory row.

### 4.4 Round protocol

Standard harness packet rounds; the verifier owns the composition
additions. Hostile probes to expect: an asset request while the transport is
restarting (must refuse, never serve a mixed set - inventory §2.24, "404 not
SPA fallback"); a 16 KiB+ request head on a static path (413, close); a
`/v1` path spelled `/V1` or with a `..` segment (404, no dispatch); a bearer
on a static path (ignored, no verify - and the proof that it is IGNORED,
not consumed, since the door is not on that path; MANDATORY by the §8
ruling, not a probe the verifier may skip); a second bundle entry
claiming `jinn:ui-bundle` (`DuplicateProvision`, the second fails, the first
keeps serving).

### 4.5 Out of scope

Every surface not named; the live half; moments; the service worker; the
Tauri shell; Talk; the footgun gate; connectors; production data of any
instance (the cutover rule).

### 4.6 Kernel findings this packet is likely to file

- **KG-1** (`jinn:profile-admin`, carded M2-K23 / PLA-348) - the plugins
  page cannot enable or disable, and a UI swap is a file edit. The card
  exists; this packet adds the transcript of the surface trying.
- **KG-5** (#38) - a broken bundle shows `failed` with no reason on the
  page it was meant to show reasons on.
- A payload-size observation: one ~4 MB crossing per activation is measured
  in proof 3; if the broker's copy shows in the activation time, that number
  becomes a finding rather than a workaround.

---

## 5. The JS-in-WASM extension tier, measured

The question the Todo asks: does a QuickJS component build for plugin world
0.10.0 today, how big is it, and what host imports does it need. A throwaway
spike answered it on 2026-09-02 with the pinned toolchain; nothing from it is
committed. `$SPIKE` below is a scratch directory outside every checkout;
`$HARNESS` is a checkout of this repo at `2149d82`; the Rust toolchain is
rustup `stable-aarch64-apple-darwin` 1.96.0 with `wasm32-unknown-unknown`
installed (the Homebrew `cargo` on the same machine has no wasm32 std - the
kits' `rustup which rustc` fallback exists for exactly that, and the first
attempt failed on it before anything was measured).

### 5.1 Spike A - QuickJS through `rquickjs`, for the plugin world's target

```text
$SPIKE/a-rquickjs/Cargo.toml   rquickjs = { version = "0.12.2", default-features = false }
                               [lib] crate-type = ["cdylib"]; release: opt-level "s", lto, codegen-units 1, panic abort
$SPIKE/a-rquickjs/src/lib.rs   Runtime::new -> Context::full -> ctx.eval::<i32,_>("1+2")
cargo build --release --target wasm32-unknown-unknown
```

Result, 2 s: **does not build.** `rquickjs-sys` compiles QuickJS's C sources
with the system `clang --target=wasm32-unknown-unknown` and fails at the
first include:

```text
libregexp.c:24:10: fatal error: 'stdlib.h' file not found
```

The plugin world's target is freestanding: there is no libc. QuickJS is C
and needs one. This is the whole answer to "does it build for plugin world
0.10.0 today": no, and not for a reason a Rust flag fixes.

### 5.2 Spike A2 - the same crate against a libc, to measure what QuickJS needs

A self-contained `wasi-sdk-34.0` (unpacked under `$SPIKE`, nothing installed
on the machine) supplies a libc for `wasm32-wasip2`:

```text
CC_wasm32_wasip2=$SPIKE/wasi-sdk/bin/clang AR_wasm32_wasip2=$SPIKE/wasi-sdk/bin/ar \
  cargo build --release --target wasm32-wasip2
```

Result, 12.65 s: **builds.** Artifact `spike_a_rquickjs.wasm`, a component:

| Measure | Value |
|---|---|
| Component size | 863,575 bytes |
| gzip -9 | 330,472 bytes |
| Top-level imports | 15, all `wasi:*@0.2.6`: `io/poll`, `io/error`, `io/streams`, `clocks/monotonic-clock`, `clocks/wall-clock`, `cli/environment`, `cli/exit`, `cli/stdin`, `cli/stdout`, `cli/stderr`, `cli/terminal-input`, `cli/terminal-output`, `cli/terminal-stdin`, `cli/terminal-stdout`, `cli/terminal-stderr` |
| Of those present in `jinn:plugin@0.10.0` | 0 |

The libc surface QuickJS itself needs, read off the built static library
(`llvm-nm -u libquickjs.a`, internal cross-object symbols removed): 96
undefined names, of which the libc and libm ones are - allocation (`malloc`,
`calloc`, `realloc`, `free`), memory and string (`memchr`, `memcmp`,
`strchr`, `strcmp`, `strlen`, `strncmp`, `strrchr`, `strstr`), the printf
family (`printf`, `fprintf`, `snprintf`, `vfprintf`, `vsnprintf`, `fputc`,
`fwrite`, `putchar`, `puts`, `stdout`), number parsing (`strtod`), time
(`clock_gettime`, `gettimeofday`, `localtime_r`), termination (`abort`,
`__assert_fail`), and about thirty libm functions (`acos` through `tanh`,
`pow`, `fmod`, `frexp`, `scalbn`, `lrint`, `round`, `hypot`, `cbrt`,
`expm1`, `log1p`, `log2`, `log10`).

So a QuickJS Tier A guest needs ONE of: a libc shim of roughly sixty symbols
written against the plugin world (the allocator and libm are mechanical;
`printf`/`strtod`/`localtime_r` are not), or a kernel import - **KG-4, a
WASI-lite subset for Tier A** (`clocks`, `cli/exit`, stdio as ledger lines,
empty `environment`, no filesystem, no sockets). Neither exists at the pin.

### 5.3 Spike B - a JS engine that IS a plugin-world guest today

`boa_engine` is pure Rust. The spike builds it as a real `jinn:plugin@0.10.0`
guest in the shape every harness guest has (`wit_bindgen::generate!` on
`kernel-pin/wit`, `export!`), with a waterfall listener that runs the
operator's JS over the payload:

```text
$SPIKE/b-boa-guest/Cargo.toml   boa_engine = { version = "0.22", default-features = false }
                                serde_json = "1.0"; wit-bindgen = "0.43.0"; getrandom = "0.4"
                                [lib] crate-type = ["cdylib"]; release: opt-level "s", lto, codegen-units 1, panic abort, strip
$SPIKE/b-boa-guest/src/lib.rs   activate: events.listen("jinn:ui/before-send", token); config.source held
                                handle-event: JSON.stringify((SOURCE)(JSON.parse(payload))) -> the bytes
                                __getrandom_v03_custom: a deterministic backend (the world imports no entropy)
RUSTFLAGS='--cfg getrandom_backend="custom"' cargo build --release --target wasm32-unknown-unknown
encode: wit_component::ComponentEncoder::default().module(core).validate(true).encode()   # the kits' own call
```

Result: **builds** (dependencies cold in about 30 s; 11.9 s incremental) and
**encodes as a validated component**:

| Measure | Value |
|---|---|
| Component, as booted in §5.4 (breadcrumbs and the clock fix included) | 3,944,216 bytes |
| gzip -9 | 1,160,114 bytes |
| Component, the bare listener before either | 3,942,572 bytes |
| Component imports | exactly `jinn:plugin/types@0.10.0`, `jinn:plugin/effects@0.10.0`, `jinn:plugin/events@0.10.0` |
| Core imports beneath them | `effects.register`, `events.listen` |
| WASI or any other host import | none |

Three toolchain facts the packet card must carry: `getrandom` (pulled by
Boa) refuses `wasm32-unknown-unknown` unless a backend is chosen by cfg, and
the custom backend's symbol is `__getrandom_v03_custom` even in 0.4 (read
from the crate's `backends/custom.rs`); Boa 0.22's value API is not the enum
older examples show (`JsValue::to_string(&mut ctx)` is the stable read); and
a context must be built with a guest-supplied `Clock` (§5.4, lesson 1).

### 5.4 Spike B, booted through the pinned daemon

The guest was mounted from a profile through the pinned daemon built by
`git archive` of `85d36b4` and `cargo build -p jinnd-daemon`, exactly as
`tests/composition/src/daemon.rs` builds it. Its `activate` evaluates the
operator's example ONCE and fails the fiber unless the engine's answer equals
`config.data.expect`, so the pair below discriminates: the same artifact,
two expectations, two fates.

```json
{ "id": "ext-green", "package": "ext/jinn-ext-js-boa", "hash": "20304da648c5…",
  "config": { "grants": ["jinn:ui/before-send"],
              "data": { "topics": ["jinn:ui/before-send"],
                        "source": "(p) => ({ ...p, text: p.text + ' 🟢' })",
                        "expect": "<see the table>" } } }
```

| Boot | `expect` | Fiber | The entry's own ledger rows, in order |
|---|---|---|---|
| good | `{"text":"hello 🟢"}` | **`Active`** | `EffectRegistered` for each of four breadcrumbs (`activate entered`, `config parsed`, `js context built`, `js evaluated`), `EffectRegistered "listen jinn:ui/before-send"`, `Pending → Loading → Active`; on SIGINT `EffectWithdrawn` for the listen, `FiberSuspended { retained: 0 }`, `Active → Unloading → Disposed` |
| bad | `{"text":"hello"}` | **`Failed`** | the same four breadcrumbs, their withdrawal LIFO and `clean: true`, then `Pending → Loading → Unloading → Failed` |

Read together: the component loads at its hash (`ArtifactLoaded`), a Boa
context builds inside the kernel's wasmtime, the operator's source evaluates
under fuel metering with the default 1 MB guest stack, its RESULT decides the
fiber's fate, the listen lands as an effect under the topic's grant, and a
daemon stop suspends it cleanly. That is the extension tier's shape, running
on the pinned kernel, with zero kernel change.

Two things the first boots taught, recorded because they cost most of an
hour and would cost the packet the same:

1. **A JS engine needs a clock, and a guest has none.** `Context::default()`
   builds Boa's `StdClock` from `Instant::now()`, which has no implementation
   on `wasm32-unknown-unknown` and aborts before `activate` can say a word.
   The provider supplies its own `Clock` through `ContextBuilder::clock`
   (the spike: Boa's `FixedClock`; the real provider: `jinn:clock`'s `now`,
   read once per delivery).
2. **When it aborted, nothing said why.** The ledger carried
   `Loading → Unloading → Failed` and no reason, and the daemon's log at
   `RUST_LOG=trace` carried nothing either: `FINDINGS.md` #38 observed from
   the outside, on the first day a machine-written guest was mounted. The
   debugger that worked was the ledger itself - one `effects.register` with a
   label per activation step, read back in order. UI-2's card carries that
   as its activation discipline until #38 closes, and KG-5 carries this
   transcript.

### 5.5 The verdict, and what is not measured

- **The tier is buildable today with zero kernel change**, with Boa as the
  engine. Its authority is exactly the plugin world's; its only imports are
  the two host calls a listener needs. Laws 1 and 5 stand.
- **QuickJS is not buildable for the plugin world today.** It becomes
  buildable with a libc shim (harness work, roughly sixty symbols, estimate
  2 to 4 agent-days *(estimate)*) or with KG-4 (kernel work). The size case
  for it is real - 864 KB against 3.9 MB, 330 KB against 1.16 MB gzipped -
  and the engine is a PROVIDER swap in this design (`jinn-ext-js-boa` to
  `jinn-ext-js-quickjs`), so choosing Boa first forecloses nothing.
- **Not measured, and named as UI-2 round 1's first job:** the cost of one
  moment - `Context::default()` plus one eval - under the kernel's fuel
  metering (10,000-fuel yield interval), on the real daemon, per delivery;
  the guest's memory high-water mark; whether a `Context` can be kept across
  deliveries in a single-threaded guest without the operator's source
  leaking state between moments (the spike creates one per delivery, which
  is correct and slow). Inventory §4.6's first uncertainty stands until
  those numbers exist.

---

## 6. A user extension is a waterfall listener - the operator's example, traced

The example: "i want my chat to be extended to parse my input before
sending and append emoji 🟢". Through the seams as UI-1 and UI-2 leave them.

**Install (a profile edit; KG-1, carded as M2-K23 `jinn:profile-admin`, PLA-348, is why it is not a click yet).**

```json
{ "id": "ext-green", "package": "ext/jinn-ext-js-boa", "hash": "<the provider's component sha256>",
  "config": { "grants": ["jinn:ui/before-send"],
              "data": { "topics": ["jinn:ui/before-send"],
                        "source": "(p) => ({ ...p, text: p.text + ' 🟢' })",
                        "origin": "human" } } }
```

The grant is the topic's own name (`plugin.wit`, `events.listen`). The
source is data to a signed plugin; it has no host calls, so its authority is
the grant list and nothing else (Law 5's structural containment; §8
question 1 asks whether that is enough).

**Activate.** The loader mounts `ext-green`; the guest's `activate` calls
`events.listen("jinn:ui/before-send", token)`; the kernel records the
listen effect and the listener's registration position. The spike's
self-test at activation evaluates the source once and fails the fiber if it
does not round-trip, so a syntax error is a `failed` fiber on the record,
never a silent no-op listener (R11).

**The moment, client side.** The operator types `hello` and presses Enter.
`chat-input.tsx`'s `sendText` (inventory §4.3 moment 1, the single choke
point for Enter and for STT auto-send) calls the re-seated data layer's
`sendMessage`, which FIRST calls
`POST /v1/moments/ui/before-send` with `Authorization: Bearer <credential>`
and body `{ "text": "hello", "session-id": "…", "attachments": [] }`.

**The door.** `jinn-api-http` puts the bearer to `jinn:auth` `verify`: one
crossing, one `AuthDecided { granted: true }` row, before anything else
(2.8, unchanged).

**The walk.** The transport calls
`events.emit("jinn:ui/before-send", waterfall, all, <body bytes>)`. The
kernel selects every listener of the topic in registration order and, for a
reply-expecting mode, first checks that none owes a transition
(`plugin.wit`, `events.emit`). It delivers `handle-event(token,
"jinn:ui/before-send", <body>)` to `ext-green`; the guest runs
`JSON.stringify((source)(JSON.parse(body)))` inside Boa inside wasmtime
under fuel metering and returns `{"text":"hello 🟢",…}`; the kernel, seeing
a non-empty output, makes it the payload for the next listener
(`topics.rs`, `Waterfall` arm); with no next listener the walk ends with
that payload as its one output, and a `DispatchTrace { topic:
"jinn:ui/before-send", mode: waterfall, listeners: 1, failures: 0, emitter:
jinn-api-http }` row lands (Law 2).

**The answer.** The transport answers `200 { "text": "hello 🟢", … }`. The
client now builds the optimistic bubble from the FOLDED text and calls the
session store's `send` with it - so the bubble's content-identity key
(inventory §2.7 G1) matches the server twin exactly and the message renders
once, and the optimistic id survives (G2). Ordering the moment before the
bubble is the one place the view layer's contract had to be understood to be
left alone.

**The transcript.** The turn's user message reads `hello 🟢`. The engine
receives `hello 🟢`. Nothing between the composer and the engine had to know
an extension exists.

**What happens when it goes wrong, each on the record.**

- The source throws: the listener's failure is recorded, `failures: 1`,
  the payload is unchanged, the send proceeds with `hello` (R9: a failing
  listener never aborts the walk). The operator sees the failure on the
  plugins page's history for `ext-green`.
- The extension is mid-restart (the operator just edited its source):
  the walk is refused whole with `restarting`, the transport answers `503`
  naming it, the client retries once after the restart lands (about 30 ms,
  #27) - the send is never silently unextended (UI-2's decision).
- Two extensions on the topic: both fold, in registration order; the order
  is what the ledger's listen rows say and nothing else (KG-3).
- The extension loops: the walk costs the guest deadline, 5 s, and the send
  waits for it (KG-2).
- A send from cron or a workflow: it does not pass through the transport's
  moment endpoint in UI-2. UI-6 makes the session definition's `send` emit
  the same topic gateway-side, so every sender pays the same moment - and
  that emitter must itself never be a listener's target (#4/#32).

**What an extension cannot do, by construction.** It cannot call any seam
(no `services` import in the extension guest's world usage; the spike's
component imports prove the shape); it cannot see any moment it is not
granted; it cannot change a decision that is not a waterfall (approvals are
`on-`, not `before-`, inventory §4.3 moment 20); it cannot outlive its
entry.

---

## 7. Non-goals, and the cutover rule

**The cutover rule, verbatim (`AGENTS.md`): the old gateway keeps ALL
production until parity. No plugin here reads or writes production data
before the parity gate passes for its instance.** Every packet in this arc
runs against kit-built profiles and the composition rig, and the one
browser-level acceptance in UI-1 runs against a throwaway root. The old
gateway's web UI is untouched and keeps serving every instance.

This arc will NOT:

- Rewrite the view layer, re-derive a tuned constant, or tidy an unexplained
  carry (inventory §2.13, §2.25 are ported byte for byte).
- Port the no-build ESM plugin door (inventory §2.20) or any same-realm
  extension mechanism; the extension tier is WASM (Law 1, Law 5).
- Adopt a second UI framework (SOURCE-OF-TRUTH §8).
- Port Talk (10,810 lines, an always-mounted overlay with a server-declared
  control manifest - inventory §1.2, §3.4 Tier 1) until a Talk seam exists.
- Port the Tauri shell path, its CSP or the base64 native transport
  (inventory §2.17, §2.24 "Unknown") in any phase here.
- Port Org, Notes, Skills, Files, Experiments, Global search, Limits, or
  Logs beyond the ledger tail, each of which waits on a seam that does not
  exist (inventory §4.2, §4.5 item 9); the cards that build those seams are
  not UI cards.
- Carry the service worker in UI-1 (inventory §6.10 item 3: the artifact
  pin and a client cache need one owner; decided in §8 question 4).
- Carry `scripts/check-footguns.mjs` in UI-1 (inventory §6.5); its
  mechanism is worth having and its rule set is gateway-shaped; a later
  card re-derives the rules for a view layer.
- Change any kernel contract, vendor a kernel crate, or work around a
  kernel gap (standing order 1): every gap above is a candidate card.
- Amend any Law. Everything here runs under Laws 1 through 5 as ratified.

---

## 8. Decisions taken (COO, 2026-09-02)

The seven questions this plan put to the COO, each with its ruling in one
line; the reasoning that was put with each question follows, kept as the
record of why.

| # | Question | Ruling |
|---|---|---|
| 1 | Is the operator's JS allowed to be config? | ACCEPTED for UI-2; entry data carries `origin: agent \| human` (shown on the plugins page); the guest records the source's sha256 as an activation breadcrumb; revisit with KG-1 |
| 2 | Public bytes with no door? | ACCEPTED as carded; proof 2 is the boundary; the "bearer on a static path is IGNORED" probe is a mandatory acceptance line |
| 3 | UI-2 before UI-3? | ACCEPTED |
| 4 | Service worker dropped in UI-1? | ACCEPTED; returns only in a card owning both the artifact pin and the client cache |
| 5 | Boa first? | ACCEPTED; the tier is "the JS-in-WASM extension tier"; QuickJS is a later provider packet (libc shim) or KG-4, prerequisite of nothing |
| 6 | Who implements the TypeScript half? | One card, ONE build node (kernel-dev); sub-agents allowed for the verbatim port; proof 6 makes that safe |
| 7 | Card KG-1 now? | Carded as jinnd M2-K23 `jinn:profile-admin` (PLA-348); UI-1 does not wait for it |

Two amendments beyond the questions: §4.3 proof 7 is the independent
verifier's over `agent-browser`; §4's LOC ceiling binds, with its meter
declared. Dispatch of UI-1 waits for the 2026-09-04 audit.

### The questions as put, with their reasoning

1. **Is the operator's JS allowed to be config?** In §6 the source is data
   inside a signed first-party plugin whose only authority is topic grants.
   Law 5 says plugins are signed; an extension's SOURCE is not, under this
   design. The alternative - every extension a signed envelope of its own
   (constitution 05, with the local development key) - is stricter and makes
   "type it in Settings" a build step. Recommendation: config for UI-2, with
   the origin attested as `agent | human` in the entry's data and shown on
   the plugins page; revisit with KG-1.
   **Ruled: ACCEPTED for UI-2**, with two additions now in UI-2's scope: the
   `origin` field on the entry and on the plugins page, and the source's
   sha256 recorded as an activation breadcrumb (Law 2).
2. **Public bytes with no door.** §4.1 serves the document and assets to any
   loopback peer without `verify`, on the reading that the door's contract
   forbids a DISPATCH on an unauthenticated connection's behalf and a byte
   from memory is not one. The 2.8 note's "every parsed request is exactly
   one verify" was true of a transport that served only `/v1`. If the COO
   reads the contract as every REQUEST, the alternative is a bearer in the
   URL fragment consumed by the pairing screen - which the 2.8 note rejected
   for the API. Recommendation: as carded, with proof 2 as the boundary.
   **Ruled: ACCEPTED as carded.** Proof 2 is the boundary; the §4.4 probe
   "a bearer on a static path is IGNORED, not consumed" is mandatory.
3. **The order of UI-2 and UI-3.** This plan puts moments before the live
   half, against inventory §4.5, because the arc exists for malleability.
   The cost is that chat is two packets further out.
   **Ruled: ACCEPTED.**
4. **The service worker.** Dropped in UI-1. It returns, if at all, in a
   card that owns both the artifact pin and the client cache.
   **Ruled: ACCEPTED.**
5. **Boa as the first engine of the JS-in-WASM extension tier.** §5 is
   the evidence; the name describes the shape (JS inside WASM), the engine
   is a provider. If the COO wants QuickJS first, KG-4 or the shim is a
   prerequisite packet and UI-2 slips by its estimate.
   **Ruled: ACCEPTED - Boa first.** The tier is named "the JS-in-WASM
   extension tier" throughout; the engine is a provider (`jinn-ext-js-boa`,
   later `jinn-ext-js-quickjs` via a libc-shim packet or KG-4). Neither is a
   prerequisite of anything.
6. **Who implements the TypeScript half of UI-1.** The card is kernel-dev's;
   the verbatim port and the node lane are `jinn-dev`'s craft. Recommendation:
   one card, two sessions, kernel-dev owns the seam and the proofs.
   **Ruled: one card, ONE build node (kernel-dev), sub-agents allowed for
   the verbatim port** - the diff gate (proof 6) is what makes the
   mechanical port safe to parallelize; the seam and the proofs stay with the
   card owner. §4's owner line says so.
7. **Whether a PLA card for KG-1 is opened now.** Every phase from UI-2 on
   names it; UI-7 is blocked on it for its headline feature.
   **Ruled: carded NOW as jinnd M2-K23 `jinn:profile-admin` (PLA-348,
   backlog)**, sequenced after the 2026-09-04 audit on the kernel lane; UI-1
   does not wait for it.

### Amendments after dispatch

- **Amendment 3 (COO, 2026-09-02, UI-1 build round 1).** Raised by the build
  before any edit: `routes/client-providers.tsx` is on the shell's port
  closure but mounts Talk, which §7 excludes. Ruled: the file is the EIGHTH
  adaptation (§4.2 item 8) - the two Talk mounts removed, nothing else -
  and proof 6 asserts a non-empty diff for it. The `@jinn/model-id` and
  `@jinn/fallback-map-wire` leaves port verbatim through the map (empty
  diff). The meter's path list gains `plugins/api/jinn-api-http-wire/src`
  so the packet's framing rows are billed against the same ≤ 800 ceiling
  rather than declared beside it; the ceiling does not move. The build's
  ~650 (+~40 wire) is an ESTIMATE, not a ceiling.
- **Amendment 4 (COO, 2026-09-02, UI-1 verify round 1: ESCALATE, 4
  Blockers).** (1) Acceptance 4 assumed the kernel's epoch gating restarts a
  wasm consumer on a provider swap; at pin `85d36b4` it does not (FINDINGS
  #46). Ruled: acceptance 4 is restated to the pinned kernel's behaviour and
  the incarnation is ASSERTED unchanged, never omitted; it flips to +1 when
  M2-K24 (PLA-350, carded and approved on the kernel lane) lands through
  pin-bump 7. (2) Acceptance 5 assumed one activation order; the pinned
  kernel has two (#45). Ruled: both orders proven, the late-provider order
  forced by a profile edit; and a new 5b requires ten deterministic fresh
  boots - the verifier reproduced the coin toss by hand and an operator
  cannot test a UI that boots half the time. (3) The Settings page sent an
  undeclared field (`defaultDelivery` on `cron`) and the daemon refused it
  422: a build defect under adaptation 1 - the page renders only declared
  settings; proof 7 patches a declared one. (4) The onboarding wizard still
  calls `/api/onboarding`, an old-gateway route: a build defect; ruled the
  ninth adaptation (mount removed, state synthesised, a repo test that no
  `/api/` string survives outside the adapters). The behaviour-free Major
  (divider ASCII) is fixed before land. Round 2 of 2 under the STOP RULE;
  a third round only by ruling.
- **Amendment 5 (COO, 2026-09-02, UI-1 build round 2 declarations).**
  (1) The item-2 lines in `jinn-api-http/src` (the activation names its
  fault on the record; a provider's contained failure classified "not
  yet") are MANDATED by amendment 4 and an overrun of the ≤ 800 ceiling
  consisting solely of them is reported as such, never a Blocker. (2) One
  delta outside §4.2's exhaustive scope is authorized: the settings seam's
  `Resolved` gains a `schema` field (additive, R12; `plugins/settings/*`,
  declared beside the meter) so the page can render only declared settings;
  `routes/settings/page.tsx` is adapted under item 1. The declared setting
  proof 7 patches is `cron` / `tick-ms`. (3) The `/api/` repo test is scoped
  to adapted and new files; verbatim files carrying dead `/api/` strings are
  listed as inventory, and proof 7's network transcript proves no such
  request is issued by the mounted pages.
- **Amendment 6 (COO, 2026-09-03, UI-1 verify round 2: one Blocker).**
  The verifier's browser transcript caught two live old-gateway requests
  from the mounted Settings surface, both from files the diff gate held
  verbatim: `routes/settings/plugins/inventory.ts` (`/api/plugins`) and
  `lib/talk-capability.ts` (`/api/talk/config`). Ruled: both become
  adaptations (items 10 and 11); proof 6's list is items 1-11. A THIRD
  round is authorized by this ruling under the STOP RULE, scope-locked to
  this one Blocker plus regressions in the lines it changes; the verifier
  re-runs proof 7's network transcript and nothing else it already passed at
  `94e028a` unless those lines touch it.
- **Amendment 7 (COO, 2026-09-03, UI-1 build round 3 declaration).** The
  build showed with evidence that amendment 6's attribution of the live
  `GET /api/plugins` to `inventory.ts:53` was wrong: that hook has no
  callers; the request comes from `plugins/disk-plugins.ts`, mounted on
  every page. Ruled: `disk-plugins.ts` is adaptation 12 (resolves empty,
  no request; the bridge stays mounted because `ContributedRoute` waits on
  its `settled`); items 10 and 11 stand as built; the `/api/` repo test's
  adapted scope is items 10-12; proof 6's list is items 1-12. Rust delta 0;
  the meter stays 843/800 MANDATED. Round 3's scope lock covers all three.



## 9. The second packet, carded: UI-2 - Moments and the JS-in-WASM extension tier

**Milestone:** M3 preparation (the arc's second packet; the phase the arc
exists for, §3 "UI-2", ruling 3) · **Owner:** kernel-dev - ONE build node;
sub-agents allowed for the provider guest's toolchain legwork (§5.3's three
facts) and for the two client adaptations, because proof 9 (the diff gate)
and proof 8 (the guest's own self-test) are what make that safe; the seam,
the transport route and the proofs stay with the card owner (§8 ruling 6) ·
**Status:** carded 2026-09-03 (PLA-353 phase 1); dispatch after card review
AND after pin-bump 7 lands, one heavy cargo lane at a time · **Pin the card
assumes:** harness `main` after pin-bump 7 (PLA-352, PR #23, branch
`packet/pin-bump-7-k24`, head `1f676c0`) lands - kernel `a53a352` (jinnd
M2-K24), `jinn:plugin@0.10.0` UNCHANGED between `85d36b4` and `a53a352`
(`kernel-pin/wit/plugin.wit` is byte-identical across the bump; the bump's
diff touches `kernel-pin/wit/README.md` only), `jinn:introspect` 0.6.0
(`kernel-pin/contracts/jinn-introspect/contract.wit`, `entry` gains
`injects` and `unmet`), FINDINGS #7, #45 and #46 closed "fixed at pin
`a53a352`", #38 open. **At the time of writing pin-bump 7 has NOT landed**
(PR #23 open, build/verify live); nothing in this card depends on what it
changes - the extension entry declares no `injects` (it injects no service)
and the transport's declaration of its bundle is UI-1's - but the card is
written against the post-bump tree and the packet does not dispatch before
it · **Binding rules:** `AGENTS.md` standing orders 1 through 5; jinnd R1
(no blocking in a guest), R3 (typed wire; the moment vocabulary is closed),
R9 (no silent replacement: a refused walk is a typed 503, never the
unmodified payload; a failing listener never aborts the walk), R11 (a bad
extension fails its own fiber and nothing else - the one place the pinned
kernel cannot keep that promise is named in proof 7 and KG-2), R12 (every
seam delta additive, 0.x minor), Laws 1, 2, 5 (§8 ruling 1's two additions
are in scope: `origin` on the entry and on the page, the source's sha256 on
the ledger) · **LOC ceiling (card-authoritative, binding):** production Rust
net delta **≤ 1,100** - the card author's PRE-DESIGN ESTIMATE, priced before
design contact, which the COO re-prices ONCE on the first meter reading
(the M1-P2 / K24 amendment 1 precedent; the UI-1 card's 800 landed at 843
MANDATED and reads 765 after pin-bump 7 - PR #23's agent note, section
"Meter"). The meter is UI-1's, its path list extended by the new plugin
directories and the one existing seam this packet touches:
`git diff --numstat main -- 'plugins/ext/**/*.rs' 'plugins/ui/**/*.rs' 'plugins/api/jinn-api-http/src/*.rs' 'plugins/api/jinn-api-http-wire/src/*.rs' 'plugins/plugins/jinn-plugins/src/*.rs' 'tools/ui-kit/**/*.rs' 'tools/ext-kit/**/*.rs'`,
added minus deleted, summed over every file that is not under a `tests/`
directory and not named `tests.rs`; a `#[cfg(test)]` module inside a
production file is a declared category - the PR lists each such module
with its line count and that count is subtracted; `tests/composition` is
excluded. The plugins seam path is IN the list (not declared beside it)
because the card knows in advance that the catalog row grows one additive
field; a delta on any path outside the list is declared beside the meter
with its reason (amendment 5's shape). The TypeScript tree carries NO line
ceiling because its acceptance is a DIFF against the pinned sha ·
**Standing gates:** `cargo fmt --check && cargo clippy --workspace
--all-targets -- -D warnings && cargo test --workspace`, the node lane
(`.github/workflows/ci.yml`, job `web`), `cargo test -p composition`, and
the privacy firewall (same file, "privacy firewall").

### 9.1 The one decision

A moment is a `waterfall` walk on a `jinn:ui/<topic>` topic that the
transport dispatches when an AUTHENTICATED client calls
`POST /v1/moments/<domain>/<topic>` with the moment's payload, and answers
with the FOLDED payload - `events.emit(topic, waterfall, all, body)`,
listeners in registration order, a non-empty output replacing the payload
for the next listener, the final payload the one answer, one
`DispatchTrace` row per walk (`kernel-pin/wit/plugin.wit` `events.emit`;
§1 "About the kernel", waterfall semantics). **Fail-closed:** a walk the
kernel refuses whole - `restarting`, `gone`, `suspended`, `stalled`
(M2-K9) or `cycle` (M2-K10), each a typed `kernel-error` in `plugin.wit` -
is answered `503` with the envelope's `unavailable` code
(`plugins/api/jinn-api/README.md`, "HTTP status mapping") and the refusal's
name in `detail`, never the unmodified payload: a validator extension
(inventory §4.3 moment 1, "refuse a send containing an API key") is
defeated by fail-open, so the send waits for the walk or does not happen.
The extension tier is `jinn-ext-js-boa` (§5.3, §5.4; ruling 5): a Tier A
guest whose entry's `config.data` carries the operator's JS source, the
topics it listens on and the `origin` attestation, whose authority is the
topic names in its `config.grants` (`plugin.wit` `events.listen`: "a
subscription is covered by the grant of the topic's own name") plus ONE
kernel host provider read (`jinn:clock` `now`, §9.2), and whose JS has NO
host calls - so it cannot re-enter a seam and #4/#32 has no target.

### 9.2 Scope

**The moments seam (Rust, in `jinn-ui` - "the seam is `jinn-ui` again",
§2 table).**

- `plugins/ui/jinn-ui` (definition, workspace member) gains the moment
  vocabulary as pure types: the topic names, each topic's payload schema,
  and the fail-closed law in prose. Three topics, closed (R3): `jinn:ui/before-send`
  with `{ text, attachments, session-id }` (inventory §4.3 moment 1, the
  operator's own example; §6 traces it); `jinn:ui/before-create-session`
  with the `SessionSpec` shape (moment 3; `plugins/sessions/jinn-session/src/spec.rs`
  per inventory §4.1 "sessions"); and `jinn:ui/before-patch-settings` with
  `{ namespace, patch }` (moment 19 - "the one moment where a waterfall
  already has a native shape"). The third is this card's ONE addition to
  §3's scope and the reason is stated: the ported shell has no composer
  (chat is UI-6, inventory §1.5's extraction first), so the two chat topics
  can be dispatched and proven through the daemon but reached by no ported
  surface; the Settings page's save IS a ported write (UI-1 proof 7 patches
  `cron` / `tick-ms`, amendment 5), so `before-patch-settings` is the one
  moment an operator can reach from the UI this packet ships, and it is
  what proof 11 drives. The COO may strike it; the card then loses proof 11's
  click path and keeps its data path. The path law: `/v1/moments/<domain>/<topic>`
  maps to `jinn:<domain>/<topic>` for exactly the topics this crate names;
  anything else is a 404 with no dispatch (the vocabulary is closed, not
  forwarded - `/v1/moments/introspect/transitions` must never reach
  `emit`, which the kernel would refuse anyway as reserved, `plugin.wit`
  `events.emit` M2-K13, but a route that relies on the kernel's refusal is
  a route that dispatched).
- `plugins/api/jinn-api-http`: the route family `POST /v1/moments/<domain>/<topic>`,
  behind the door exactly as every `/v1` request (one `verify`, then the
  walk, `door.rs` unchanged); the body (capped at the wire's 256 KiB,
  `plugins/api/jinn-api-http-wire/src/lib.rs`) is the payload bytes,
  validated against the topic's schema BEFORE the walk (422 `invalid` on a
  miss, no dispatch); the answer is `200` with the folded bytes; a refused
  walk is `503 unavailable` naming the refusal; a walk with zero listeners
  answers the body. The transport's entry is granted the three topic names
  as the profile's statement of what it emits - noting for the record that
  at pin `a53a352` `events.emit` checks only the reserved-topic refusal and
  no topic grant (`crates/jinnd-wasm/src/surfaces.rs`, `emit`, read at the
  pin: `listen` calls `check_grant(grant_for(topic))`, `emit` does not) -
  KG-6 in §9.6, verified on the ledger in round 1 rather than asserted from
  the read.
- `plugins/api/jinn-api-http-wire`: no new status rows (`503` and `422`
  exist, `status_for`); one MIME row is not needed (JSON both ways). Any
  framing delta is billed on the meter.

**The JS-in-WASM extension tier (Rust, `plugins/ext/` - a new seam group
with its role-table README per standing order 4).**

- `plugins/ext/jinn-ext` (definition, workspace member): contract name
  `jinn:ext` - not a service anyone calls (an extension is a listener;
  "Providers: `jinn-ext-js-boa`; Consumers: none", §2 table) but the home
  of the entry's config schema and the activation law, compiled into the
  guest and the kit. The schema: `config.data = { topics: [<topic>...],
  source: <JS>, origin: "agent" | "human" }`, serde-typed, closed - an
  unknown field is an activation fault (R3; the settings seam's
  closed-surface law, `docs/notes/2026-08-29-closed-surfaces-refuse.md`).
  NO `budget` field: §2's table named one, and at this pin nothing can
  honor it (KG-2; a declared field the guest cannot enforce is a lie on
  the record). `origin` is constitution 05's attestation restated for data
  (the kernel's `docs/constitution/05-manifest-signing.md`, `[provenance] origin = "human | agent"`,
  immutable there; here it is the operator's declaration on the entry, and
  the plugins page shows it - ruling 1). The activation law, in prose and
  as the guest's own checks: (1) register the four breadcrumbs of §5.4
  (`activate entered`, `config parsed`, `js context built`, `js evaluated`)
  as effects in that order, the activation discipline until #38 closes;
  (2) register `source sha256:<hex>` so WHAT CODE RAN is on the record
  (Law 2, ruling 1); (3) evaluate the source ONCE and fail the fiber if it
  is not a function - a syntax error is a `failed` fiber on the record,
  never a silent no-op listener (R11; §6 "Activate"); (4) `events.listen`
  on each topic in `data.topics`, each of which must also be in
  `config.grants` (a listen the kernel refuses is a `GrantRefused` row and
  the activation fails - the guest does not swallow it).
- `plugins/ext/jinn-ext-js-boa` (provider guest, NOT a workspace member
  like every guest, `Cargo.toml`'s note): §5.3's shape -
  `wit_bindgen::generate!` on `kernel-pin/wit`, `export!`, `boa_engine`
  0.22 with `default-features = false`, `getrandom` 0.4 with the custom
  backend symbol `__getrandom_v03_custom` and the cfg
  `getrandom_backend="custom"` carried in the crate's OWN
  `.cargo/config.toml` so the flag travels with the crate and not with a
  shell; a `Clock` supplied through `ContextBuilder::clock` (§5.4 lesson 1)
  - the real provider reads `jinn:clock` `now` ONCE per delivery under a
  `jinn:clock` grant, the guest's one `services.call`, whose target is a
  kernel host provider and never a guest, so the #4/#32 wait cycle has no
  target; the component's imports are therefore exactly `types`, `effects`,
  `events`, `services` of `jinn:plugin@0.10.0` and nothing else, asserted
  by the kit (§5.3's `imports` program, committed this time as a kit
  test). `handle-event(token, topic, payload)` =
  `JSON.stringify((SOURCE)(JSON.parse(payload)))` in a Boa context; the
  context is built per delivery in round 1 (the spike's shape - "correct
  and slow", §5.5) and the cost is MEASURED in proof 2 before any reuse is
  designed; a source that throws or returns a non-object answers the
  guest-fault so the kernel records the failure and the walk continues
  (R9); a source that returns `undefined` answers EMPTY bytes, which the
  kernel treats as "leave the payload unchanged" (§1 waterfall semantics) -
  the pass-through case, and proof 4's second half. Default 1 MB guest
  stack (§5.4).
- `tools/ext-kit` (workspace member): builds and encodes the provider by
  the shared kit machinery (`plugin-kit`'s build + `wit_component` encode
  with `validate(true)`, §5.3), prints size and sha256 (a component of
  ~3.9 MB, §5.3 - the packet records the exact number), and writes the
  extension entries the composition suite and the `ui` profile mount:
  `ext_entry(id, topics, source, origin)` in the §6 "Install" shape,
  grants = the topics (+ `jinn:clock`), `injects` absent. Every entry is
  GENERATED with the artifact's honest hash (Law 5; `profiles/ui/README.md`
  "never hand-maintained").
- `tools/ui-kit`: the `ui` profile gains ONE extension entry, `ext-green`,
  the operator's example from §6 (`origin: "human"`), and the transport
  entry gains the three topic grants (`mount_bundle_on`'s sibling,
  `mount_moments_on`). A variant with a second extension and a variant with
  a throwing one exist for the suite, like `UI_MARKED` and `UI_CORRUPT`
  today (`tests/composition/src/kit.rs`).

**The plugins seam (Rust, one additive field).** `plugins/plugins/jinn-plugins`:
the catalog row (`PluginCatalogEntryWire`, `web/src/lib/api-v1-wire.ts`;
the seam's own `entry.rs`) gains an OPTIONAL `attestation: { origin }`
read from the entry's `config.data.origin` when present - additive (R12),
absent for every entry that declares none, never defaulted (the seam's
"a reading, not a state machine" law, `plugins/plugins/README.md`). Both
introspect mirror gates are untouched (the field is the profile's, not the
kernel's).

**The client (TypeScript, `web/`).** Two adaptations, and only these,
join §4.2's list of twelve; the verbatim gate's map gains their rows:

13. `lib/api-config.ts` (already adaptation 1's file): the save path calls
    `POST /v1/moments/ui/before-patch-settings` with `{ namespace, patch }`
    BEFORE its `PATCH /v1/settings/{ns}` and sends the FOLDED patch - in
    the adapter, never in a component (§4.2 item 1's shape; §6's "before
    the optimistic bubble" is the same rule one surface earlier). A `503`
    from the moment is surfaced as the page's existing conflict notice
    reading the typed refusal (inventory §2.22's notice, re-pointed in
    item 1); the client does NOT retry on its own in this packet (the
    retry-once of §3's decision belongs with the composer in UI-6 and is
    stated there). `lib/api.ts` gains `moment(domain, topic, payload)` as
    the one adapter function every later surface calls; `sendText`'s call
    (inventory §4.3 moment 1) is UI-6's, by file ownership, and UI-6's
    acceptance already carries the gateway half.
14. `routes/settings/plugins/plugin-row.tsx` (already under adaptation 4,
    `routes/settings/plugins/*`): renders the `attestation.origin` badge
    (`human` / `agent`) when the row carries one, and nothing else changes.
    The install, remove, enable and disable controls stay rendered disabled
    with the finding exactly as item 4 left them (#37 / KG-1).

No other TypeScript file changes; proof 9 asserts it both ways.

**Toolchain.** Nothing new on the node side. On the Rust side: `boa_engine`
and `getrandom` join the guest's manifest (guests are not workspace
members, so the workspace's dependency set is unchanged; the PR states
this against R10 anyway). The wasm target and `rustup which rustc`
fallback are the kits' existing ones (§5, preamble).

### 9.3 Acceptance

Composition proofs in `tests/composition/tests/moments.rs` (proofs 1-8
and 10) and `tests/composition/tests/ui.rs` (proof 9's sibling gate lives
in `tools/ui-kit/tests/verbatim.rs`), each booting the `ui` profile
through the pinned daemon built by `git archive` of the pin
(`tests/composition/src/daemon.rs`); every one runs RED FIRST against a
transport that has no moment route and a profile that mounts no
extension. Every ledger claim is read from `Daemon::ledger_rows`
(`tests/composition/src/kit.rs`), never from the transport's answer alone.

1. `a_moment_with_no_listener_answers_its_own_payload` - the `ui` profile
   with the extension entry REMOVED; `POST /v1/moments/ui/before-send`
   with the §6 body answers `200` and the body byte-for-byte; exactly one
   `DispatchTrace { topic: "jinn:ui/before-send", mode: waterfall,
   listeners: 0, failures: 0, emitter: jinn-api-http }` row.
2. `one_js_extension_folds_the_payload_and_the_ledger_says_so` - `ext-green`
   mounted from the profile; the answer's `text` is `hello 🟢`;
   `DispatchTrace { listeners: 1, failures: 0 }`; the extension's own rows
   carry the four breadcrumbs, `source sha256:<the source's hex>` and
   `listen jinn:ui/before-send` in that order (§5.4 table, good row). The
   proof then sends the same moment twenty times and PRINTS the wall time
   per walk (from the request to the answer, and from the walk's
   `DispatchTrace` row to its predecessor on the transport) and the guest's
   memory high-water mark if `jinn:introspect` 0.6.0 exposes one (it does
   not at this pin - the proof prints "not exposed"), which closes §5.5's
   "not measured" list on the first two items and is the packet's report
   line. If the per-walk cost is above 250 ms, the number is a finding
   (KG-7) and no reuse of the Boa context is designed inside this packet.
3. `two_extensions_compose_in_registration_order_and_the_order_is_named` -
   the two-extension variant (`ext-green` and `ext-blue`, the second
   appending a different marker); the answer shows both markers; the
   order in the answer equals the order of the two `EffectRegistered
   "listen jinn:ui/before-send"` rows on the ledger; the proof asserts the
   order it OBSERVED and prints it; the card records that the order across
   siblings is what the boot dealt (#7 is answered for DECLARED injections
   only; an extension declares none, so "an entry that declares nothing is
   unchanged", FINDINGS #7 at `a53a352`) - KG-3.
4. `a_throwing_extension_is_recorded_and_the_walk_continues` - the
   throwing variant beside `ext-green`; `DispatchTrace { listeners: 2,
   failures: 1 }`; the answer carries `ext-green`'s fold (R9); the
   throwing extension's failure is in ITS history (`GET
   /v1/plugins/main/<id>/history`); its fiber stays `active` (a failed
   delivery is not a failed activation). Second half: a source returning
   `undefined` yields EMPTY output and the payload passes unchanged,
   `failures: 0`.
5. `a_restarting_extension_refuses_the_moment_typed_and_nothing_is_sent` -
   `PATCH /v1/profile/entries/ext-green` with a new `source` whose
   ACTIVATION is slow by construction (a bounded counting loop of about
   one second under fuel, so the restart window is wide enough to hit
   deterministically, never `while(true)`); a moment posted inside the
   window answers `503` with `detail` naming `restarting`; on the ledger
   the walk's refusal row and NO `DispatchTrace` with a delivery; after
   the restart lands the same moment answers `200` with the NEW source's
   fold. The client's retry is not proven here (UI-6).
6. `an_extension_is_granted_its_topic_and_nothing_else` - an entry whose
   `data.topics` names `jinn:ui/before-send` but whose `config.grants`
   does not; `GrantRefused` on its history, the fiber `failed`, and a
   moment answers `listeners: 0`. Second half: an entry granted
   `jinn:ui/before-send` only, whose source is registered on the topic,
   receives NO delivery for `before-create-session` (the payload selects
   listeners by topic; nothing else selects).
7. `a_looping_extension_costs_the_walk_the_guest_deadline_and_the_transport_s_fate_is_recorded`
   - a `while(true){}` source; the moment answers after the deadline,
   MEASURED and printed (`lane::DEADLINE` 5 s at `a53a352`,
   `crates/jinnd-wasm/src/lane.rs`). Then the honest half: the kernel
   wraps EACH guest call in one `settle(deadline, ...)` (`crates/jinnd-wasm/src/instance.rs`)
   and `emit` awaits every delivery end to end (`plugin.wit` `events.emit`;
   #4/#32), so the transport's own `handle-event` - inside which it emits -
   is on the same clock as the walk it waits for. The proof RECORDS what
   happens to the transport (its next `/v1/health`; its fiber's state and
   incarnation on `jinn:introspect`; whether the deadline row names it)
   and asserts nothing about it in advance: if the transport's instance
   dies on its own deadline (`settle.rs`, "guest exceeded its call
   deadline") the fact is the packet's KG-2 transcript and the packet lands
   NOT-YET on "a bad extension costs its own slot and not the transport",
   with the kernel card named. R11 is kept by the kernel for the fiber
   that looped; whether it is kept for the fiber that WAITED is what this
   proof finds out.
8. `an_extension_boots_from_a_profile_and_a_syntax_error_is_a_failed_fiber` -
   real-composition (standing order 3): `ext-green` reaches `active`
   through the pinned daemon from the kit-written profile with its
   breadcrumbs in order; a variant whose `source` does not parse reads
   `failed` with the breadcrumbs up to `config parsed` and the withdrawal
   LIFO `clean: true` (§5.4 bad row); the catalog row for `ext-green`
   carries `attestation: { origin: "human" }` and a row with no `origin`
   carries no `attestation` field at all.
9. `the_view_layer_is_verbatim` - the existing gate extended: EMPTY diff
   for every file not on the list (items 1-14), NON-EMPTY for every file on
   it, both directions; `no_old_gateway_route_survives_in_the_adapted_client`
   re-run over the adapted scope (items 10-14).
10. `a_moment_is_the_door_then_one_walk_and_nothing_else` - on the ledger
    every connection segment that posted a moment carries exactly one
    `verify`, then exactly one `DispatchTrace`, and no other crossing (the
    2.8 `provider_segments` discipline, reused from proof 2 of §4.3); a
    moment with no bearer is `401` with no dispatch; `/v1/moments/ui/after-nothing`,
    `/v1/moments/introspect/transitions` and `/v1/moments/ui/../before-send`
    are `404` with no dispatch; a 256 KiB+ body is refused by the wire
    before any dispatch.
11. Browser-level, driven by the INDEPENDENT VERIFIER with `agent-browser`
    against a throwaway root, transcript posted on the Todo, no person in
    the loop (§8's amendment): open `/`, paste the credential, open
    Plugins and see `ext-green` `active` with its `human` badge and, in its
    history, the `source sha256:` breadcrumb; open Settings, patch the
    declared `cron` / `tick-ms` (amendment 5) with `ext-green`'s profile
    entry re-pointed by the verifier's own file edit to
    `jinn:ui/before-patch-settings` with a source that rewrites the patch
    to a different declared value, save, and read back the FOLDED value
    from `GET /v1/settings/cron` and from the page; then the network
    transcript proving exactly two requests left the page for the save
    (the moment, then the patch) and no request to any `/api/` path. If
    the COO strikes the third topic, this proof reduces to the badge, the
    breadcrumb, and one `fetch` of `/v1/moments/ui/before-send` issued from
    the app's origin with the held credential (the same `authFetch`), whose
    answer carries the emoji - and the card says so on the Todo before
    dispatch.

Plus: node lane green; `cargo test -p harness-docs` green (the new seam
group README and the ext note are cited, `docs_gate.rs`); the privacy
firewall green - the Boa guest's `target/` and the kit's outputs are
untracked, and no path in the packet names a machine or the external ops
volume; `cargo test -p harness-pin` green (no pin change); every quirk
carried is named on the PR by its inventory row; the meter reading pasted
with its `--numstat` and the `cfg(test)` list.

### 9.4 Round protocol

Standard harness packet rounds: 2 rounds, a third only by ruling (the STOP
RULE, amendment 4's shape); the verifier owns the composition additions and
proof 11. Round 1's first job, before any seam code, is proof 2's
measurement on the spike's shape (§5.5 names it) so the ceiling is
re-priced on a real number. Hostile probes to expect: a moment posted while
the transport is restarting under a bundle swap (must refuse or 503, never
a walk on a torn transport - proof 4 of §4.3 is the harness for it); a
topic in upper case, with a `..` segment, or with a trailing slash (404,
no dispatch); a bearer on `/v1/moments/...` that verifies but a body that
is not JSON (422, no dispatch, the verify row present - the door is paid
before the schema); a source that returns a string instead of an object
(a contained failure, `failures: 1`, payload unchanged); a source that
returns the payload with an ADDED unknown field (accepted - the fold is the
listener's, the schema binds the client's input, not the walk's output;
stated so the verifier does not file it); an extension whose `topics`
lists a topic twice (one listen, the second a per-entry fault - the kit
never writes it, the guest refuses it); two extensions with identical
sources (both fold, twice); `while(true)` at ACTIVATION rather than in a
delivery (the fiber fails at its own deadline, the transport is untouched -
the contrast with proof 7 that makes KG-2 precise); a second entry claiming
to PROVIDE `jinn:ext` (nothing provides it; the definition is types, not a
service - a stray `services.provide` is a defect in that guest and fails
it).

### 9.5 Out of scope

UI-3's live half (no push, no `/v1/events`; the moment's answer is the
request's response and nothing else moves). The client-side call sites of
the two chat topics (`sendText`, `buildNewSessionParams` - inventory §4.3
moments 1 and 3) and the retry-once after a `503`: UI-6. The gateway-side
twin of `before-send` (the session definition emitting the same topic so a
send from cron pays the same moment): UI-6, stated there. Every moment not
named in §9.2 (moment 2's node waterfall, 8's nav tree, 13's tool gate -
each waits on its own surface or seam). A second engine
(`jinn-ext-js-quickjs`; the libc shim or KG-4): ruling 5, prerequisite of
nothing. Reusing a Boa `Context` across deliveries (measured first, proof
2; designed later). A per-delivery budget of any kind (KG-2; no field, no
harness timer). Talk, the Tauri shell, the service worker, the footgun
gate, connectors, production data of any instance (the cutover rule, §7).

**The K23 / PLA-348 split, exactly.** `jinn:profile-admin` (KG-1; jinnd
M2-K23, PLA-348, `backlog`, unassigned at the time of writing, sequenced
after M2-K24 on the kernel lane per SOURCE-OF-TRUTH §7's M3 entry) is NOT
a dependency of this packet, and UI-2 never blocks on it. What this
packet does WITHOUT it, because `jinn:profile.patch-entry` writes an
entry's `config` subtree (`kernel-pin/contracts/jinn-profile/contract.wit`,
`patch-entry`; #37): editing an INSTALLED extension's `source`, its
`origin`, and the SUBSET of `topics` it is already granted - all of them
`config.data`, all of them reachable through `PATCH /v1/profile/entries/{id}`
today, and proof 5 uses exactly that. What waits on PLA-348 and lands
NOT-YET in this packet, each rendered disabled on the plugins page with
the finding as item 4 already renders enable/disable: (a) INSTALL - adding
the extension ENTRY with its GRANTS ("install an extension is adding an
entry with grants", §3 KG-1); in this packet an install is the kit writing
the profile or the operator editing the file, and the card says so; (b)
REMOVE; (c) the `disabled` toggle; (d) widening `topics` to one the entry
is not granted (a grants change); (e) swapping the ENGINE - the entry's
`package` and `hash` from `ext/jinn-ext-js-boa` to a later provider (#37's
"the one swap every seam proves"). The page shows `origin` (ruling 1)
without K23 because it is a read. When PLA-348 lands and a pin bump adopts
it, items (a) through (e) are ONE later card - not a re-open of this one -
and the disabled controls of item 4 and item 14 become that card's
enumerated adaptations.

### 9.6 Kernel findings this packet is likely to file

- **KG-2, sharpened (per-delivery budget) - Blocker-class if proof 7 lands
  as read.** At `a53a352` a guest call is one `settle(deadline, ...)`
  (`crates/jinnd-wasm/src/instance.rs`; `lane::DEADLINE` 5 s) and `emit`
  awaits each delivery inside the emitter's call; a listener that spends
  the deadline spends the EMITTER's too. Candidate: a per-listen fuel or
  deadline cap declared at `listen`, refused typed when exceeded, charged
  to the listener's slot; and a stated rule for the emitter's clock during
  a walk. Proof 7 is the transcript either way.
- **KG-3 (listener order is a declaration nowhere)** - unchanged by K24
  for entries that declare no injection (#7 at `a53a352`, "an entry that
  declares nothing is unchanged"); proof 3 records the dealt order.
  Candidate: an ordinal on `listen`, or profile order honored and stated.
- **KG-5 (#38, open)** - an extension whose source does not parse reads
  `failed` and the page can show the breadcrumbs it wrote before failing
  (the harness-side answer, #38's UI-1 round 2) and nothing the kernel
  said; the packet adds the transcript of a machine-written guest failing
  on purpose, the second such transcript after §5.4's.
- **KG-6 (emit is ungated by topic grant).** `surfaces.rs` at the pin
  checks `grant_for(topic)` on `listen` and only the reserved-topic
  refusal on `emit`; if the ledger confirms it in round 1, any guest can
  emit any unreserved topic, and a moment's emitter is bounded by nothing
  the profile states. Candidate: `emit` covered by the topic's grant like
  `listen` (constitution 01 §Grants, "every topic is its own grant name").
  The card grants the transport its topics NOW so the profile already reads
  as the kernel will one day enforce it.
- **KG-7 (the cost of one moment)** - only if proof 2's number is a
  problem: a Boa context per delivery under fuel metering, on the record,
  with the memory high-water mark the kernel does not yet expose (0.6.0
  has `injects` and `unmet`, no memory reading) - the second half is a
  `jinn:introspect` candidate regardless of the number.
- **The ~4 MB artifact.** UI-1's ~1.4 MB bundle crosses once per
  activation (§4.3 proof 3, measured 1,375,153 bytes at pin-bump 7); the
  extension's component is ~3.9 MB (§5.3) and is LOADED, not crossed - the
  activation-time cost is the kernel's `ArtifactLoaded` and Boa's context
  build (§5.4), printed by proof 8; if either shows in the ten-boot
  determinism budget (§4.3 proof 5b's bound), it is a finding and not a
  workaround.

---

## Appendix - the spike, reproducibly

Throwaway; run under a scratch directory, delete after. Paths relative to
`$SPIKE`; `$HARNESS` is a checkout of this repo at `2149d82`; `$JINND` a
checkout of the kernel holding `85d36b4`.

```text
# toolchain (the kits' fallback, spelled out)
RUSTC=$(rustup which rustc); CARGO=$(dirname "$RUSTC")/cargo
rustup target list --installed | grep wasm32-unknown-unknown

# A: QuickJS for the plugin world's target - fails at 'stdlib.h'
cd $SPIKE/a-rquickjs && RUSTC=$RUSTC $CARGO build --release --target wasm32-unknown-unknown

# A2: the same against wasi-libc (wasi-sdk-34.0 unpacked under $SPIKE/wasi-sdk)
CC_wasm32_wasip2=$SPIKE/wasi-sdk/bin/clang AR_wasm32_wasip2=$SPIKE/wasi-sdk/bin/ar \
  RUSTC=$RUSTC $CARGO build --release --target wasm32-wasip2
$SPIKE/imports/target/release/imports target/wasm32-wasip2/release/spike_a_rquickjs.wasm
$SPIKE/wasi-sdk/bin/llvm-nm -u target/wasm32-wasip2/release/build/rquickjs-sys-*/out/libquickjs.a

# B: Boa as a jinn:plugin@0.10.0 guest (wit path: $HARNESS/kernel-pin/wit)
cd $SPIKE/b-boa-guest && RUSTFLAGS='--cfg getrandom_backend="custom"' \
  RUSTC=$RUSTC $CARGO build --release --target wasm32-unknown-unknown
$SPIKE/encode/target/release/encode target/wasm32-unknown-unknown/release/spike_b_boa_guest.wasm ext.wasm
$SPIKE/imports/target/release/imports ext.wasm

# B, booted: the pinned daemon from git archive, exactly as tests/composition/src/daemon.rs does
git -C $JINND archive 85d36b4d846a54857a9e4d96d2039a298918375a | tar -x -C $SPIKE/pinned-jinnd
(cd $SPIKE/pinned-jinnd && $CARGO build -p jinnd-daemon)
# one root per boot: artifacts/jinn-ext-js-boa.wasm + .sha256 sidecar (the component's sha256),
# profile.json holding the §5.4 entry with "expect" set per row; then
$SPIKE/pinned-jinnd/target/debug/jinnd --profile $ROOT/profile.json --ledger $ROOT/ledger.sqlite \
  --artifacts $ROOT/artifacts --data $ROOT/data      # wait for {"jinnd":"ready"}, SIGINT
sqlite3 $ROOT/ledger.sqlite "select seq, kind from events where entry='ext-green' order by seq"
```

`imports` is a 30-line `wasmparser` program printing a component's top-level
imports and each nested core module's; `encode` is the kits' own
`wit_component::ComponentEncoder` call with `validate(true)`, printing size
and sha256. Neither is committed; both are trivially rewritten.
