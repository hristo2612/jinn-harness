# Agent note — phase 1.3 cron seam: the non-obvious decisions

## Why time enters as config edits

> **Superseded 2026-08-28** by the `01133c45` pin bump (jinnd M2-K2): time
> now enters through the kernel's `jinn:clock` capability, and the config-edit
> tick with its stand-in plugin and driver is retired. See
> `docs/notes/2026-08-28-clock-migration.md`. The section below stands as
> history — it is what the seam was built against.

The pinned kernel has no clock/timer capability and guests are purely
reactive (FINDINGS.md #1). The only recurring external input the daemon
serves is a profile edit through its file watcher. So the tick IS a config
edit: `cron-tick-source`'s config carries `{seq, now-ms}`, each edit
restarts exactly that fiber (reconcile-by-id), and the fresh activation
emits the tick. This is deliberately an *honest* workaround — every tick
is ledger-visible as a config-caused fiber cycle — and deliberately
disposable: a `jinn:clock` capability retires the plugin and the driver
without touching the contract (`jinn:cron`'s tick topic and payload stay;
only the emitter changes).

## Why the firing law lives in `jinn-cron`, not the guest

`plan_tick` is a pure function (jobs × state × tick → fires, records,
state). That keeps the semantics natively unit-testable (15 cases run on
the host in milliseconds) while the guest stays a thin IO shell — the
same discipline that made the kernel's own temporal semantics testable.
The composition suite then only has to prove the WIRING through the real
daemon, not re-prove arithmetic.

## Why one catch-up fire (and not zero, and not backfill)

On a gap (coarse tick or downtime), the newest due boundary fires and the
earlier ones become one `skipped` record. Zero-catch-up would mean a
health snapshot silently absent until the next boundary after recovery —
the wrong default for monitoring duty. Backfill would fire N stale jobs
into the present. One catch-up, `missed-before` on the event, gap on the
record: recover promptly, never lie about the past. Anchoring at the Unix
epoch (boundaries `k * every-ms`) keeps boundaries deterministic across
restarts with no stored phase.

## Why the tests boot a subprocess, not the daemon crate

The kernel's own headless demo drives `jinnd_daemon::Daemon` in-proc; the
harness must not — this repo builds only against the pinned contract
surface, and importing kernel crates would silently couple us to kernel
internals. The composition support builds the daemon binary FROM THE
PINNED COMMIT (`git archive`, no worktree metadata, no working-tree reads
— the working tree of a shared checkout is somebody else's branch) and
drives it exactly as an operator does: profile edits, stderr, the SQLite
ledger, SIGINT. Discovery mirrors the pin gate's Gate-2 lanes and
self-skips loudly without a jinnd checkout.

## Why guest crates are not workspace members

The scaffold's manifest note said plugins join `members` as they land;
that was wrong for cdylib guests of the plugin world — a host-target
`cargo test --workspace` cannot link their extern imports. The kernel's
demo plugins established the pattern: guests are standalone
wasm32-only crates (`[workspace]` cap in their manifests), compiled and
component-encoded by the kit tool. The definition crate (`jinn-cron`)
IS a member — it is pure types + logic and carries the unit suite.

## Test-harness resilience choices

Two real races surfaced and are handled in the harness, not papered over:
a tick edit can land before the daemon's watcher is armed (the boot
window), so `tick()` rewrites until the restart is observed — the duty
driver gets the same resilience from its next interval; and the daemon's
ANSI-styled stderr defeats naive log matching, so `log()` strips CSI
sequences (FINDINGS.md #10).
