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

Phase 1.7 — kernel pin `4eb4a93` (M2-K4): the guests on the
`jinn:plugin@0.3.0` world, where a daemon stop SUSPENDS a plugin (its
persisted state retained for its profile entry) and only removal from the
profile withdraws its contribution — a clean restart resumes the schedule.
Phase 1.6 put the guests on the `jinn:fs@0.2.0` bundle (append-backed run
history, `list`/`meta` health surface, typed not-found), phase 1.5 on
`jinn:clock` alarms; the phase-1.4 soak continues across all three bumps
(`SOAK.md`). The capability itself is cron as a seam triple
(`plugins/cron/`), proven by real-composition tests that boot the pinned
`jinnd` daemon (`tests/composition`). Kernel frictions found on the way are
logged in `FINDINGS.md` (the two-way iteration channel — kernel changes are
never made here).

## Layout

| Path | What it is |
|---|---|
| `AGENTS.md` | Standing orders for agents working in this repo |
| `KERNEL-PIN.md` | The kernel pin: jinnd commit + contract hashes + bump procedure |
| `kernel-pin/` | Vendored copy of the pinned contract surface (`wit/`, `contracts/`) — integrity-gated against `KERNEL-PIN.md` by `harness-pin` |
| `tools/harness-pin` | The pin gate: computes/verifies contract hashes (`cargo test -p harness-pin`) |
| `tools/cron-kit` | Builds the cron seam's components + pinned profile |
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
