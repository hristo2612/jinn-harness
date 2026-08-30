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

Phase 2.5 — kernel pin `3a8e5c0` (M2-K9), UNCHANGED. The todos seam
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
| `plugins/` | First-party plugin crates (wasm components) — land per phase, one seam triple at a time |
| `profiles/` | Named plugin trees — a product is a profile |
| `tests/composition` | Real-composition gates: boot generated profiles through the REAL pinned jinnd daemon |
| `FINDINGS.md` | Kernel frictions logged as jinnd packet-card candidates (two-way iteration) |
| `docs/notes/` | Agent notes: rationale for non-obvious decisions, one per non-trivial change |

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
| `JINN_HARNESS_TODO_VENDOR_ENGINE` | The engine id (`claude` or `codex`) the todos seam's vendor leg binds as the second half of the three-layer composition proof. It spends metered inference under the operator's own authentication, so it runs where a person names it and skips everywhere else. An engine that is NAMED and not mounted fails the proof rather than skipping it. |
