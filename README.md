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

Phase 2.2 — kernel pin `57360cc` (M2-K7): the operator contracts.
The operator API (`plugins/api/`, phase 2.1) now answers from the
kernel's own knowledge — the composition through `jinn:introspect`, the
ledger through `jinn:ledger`, edits through `jinn:profile` (operator
intent the loader applies; disposing the editor leaves the document
alone), the HTTP listener served from `jinn:net` readiness wakes with no
alarm — and is mounted in the soak. The settings seam (`plugins/settings/`)
is the second core-port seam: a definition (`jinn-settings`) with
declared namespaces, schemas, defaults, layered resolution and typed
secret references, a profile-backed provider writing through
`jinn:profile`, the cron scheduler consuming its job table through it,
and the API exposing it. Phase 2.1 built the api trio on `1b098be`
(M2-K6: `jinn:process` and `jinn:net`). Phase 1.7 put the
guests on `jinn:plugin@0.3.0` (a daemon stop SUSPENDS a plugin; only
removal from the profile withdraws it), 1.6 on the `jinn:fs@0.2.0` bundle,
1.5 on `jinn:clock` alarms; the phase-1.4 soak continues across all five
bumps (`SOAK.md`). Every seam is proven by real-composition tests that
boot the pinned `jinnd` daemon (`tests/composition`). Kernel frictions
found on the way are logged in `FINDINGS.md` (the two-way iteration
channel — kernel changes are never made here).

## Layout

| Path | What it is |
|---|---|
| `AGENTS.md` | Standing orders for agents working in this repo |
| `KERNEL-PIN.md` | The kernel pin: jinnd commit + contract hashes + bump procedure |
| `kernel-pin/` | Vendored copy of the pinned contract surface (`wit/`, `contracts/`) — integrity-gated against `KERNEL-PIN.md` by `harness-pin` |
| `tools/harness-pin` | The pin gate: computes/verifies contract hashes (`cargo test -p harness-pin`) |
| `tools/cron-kit` | Builds the cron seam's components + pinned profile; its library is the kit machinery every seam kit shares |
| `tools/api-kit` | Builds the operator-API profile: the api trio beside the cron seam |
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
