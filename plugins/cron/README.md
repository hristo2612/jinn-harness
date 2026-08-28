# The cron seam

Scheduled work as plugins on the kernel — the first real capability of the
distribution (phase 1.3). Roles per the seam-triple naming law (AGENTS.md):

| Role | Package | What it is |
|---|---|---|
| Service definition | `jinn-cron` | The `jinn:cron` contract: settings namespace, schedule spec, tick/fire payloads, run-record shape, and the firing law. Pure types + logic; compiled into both guests and host tools. |
| Provider | `cron-scheduler` | Wasm plugin holding the schedule: reads job entries from its config subtree, computes due boundaries per tick, emits typed fire events, keeps run history and state through its granted `jinn:fs`. |
| Provider (stand-in) | `cron-tick-source` | The timer stand-in: the kernel has no clock/timer capability yet (FINDINGS.md #1), so time enters through the one honest push path — a config reconcile. An operator-lane driver (`cron-kit tick`) rewrites this entry's config; the restarted fiber emits the `jinn:cron/tick` event. Retired the day a kernel timer capability ships. |
| Consumer | `health-snapshot` | A real scheduled job: on each fire it probes data-root writability (write, read back, compare) and writes a health report through its granted `jinn:fs` — every fire and every write ledger-visible. |

The contract surface (topics, operations, payload schemas, the firing law) is
documented in `jinn-cron/README.md` — one home per fact.

Guest crates here are NOT workspace members (see the workspace manifest's
note): `cargo run -p cron-kit -- kit <root>` builds them for
wasm32-unknown-unknown, encodes each to a component, and writes the pinned
profile. Real-composition proof lives in `tests/composition`, which boots the
generated profile through the REAL pinned `jinnd` daemon.
