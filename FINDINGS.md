# Kernel findings — the two-way iteration log

Frictions the distribution hits against the pinned kernel, logged with
enough specificity to become `jinnd` packet cards. **Kernel changes are
never made here** (AGENTS.md standing order 1); this file is the channel.
Each entry: what happened, the evidence, and the capability shape that
would retire the workaround. Numbers are stable — code comments cite them.

Phase 1.3 (cron seam) baseline: every entry below was hit while building
the first real capability. What HELD is at the bottom — the kernel earned
that section too.

## 1. No clock or timer capability — time cannot enter the system

A Tier A guest is purely reactive (entered only via `lifecycle.*`), cannot
read time, and cannot ask to be woken. The daemon's only external inputs
are profile edits, artifact swaps, and stdin `revert`/`status`. A cron
seam therefore has NO honest way to fire on a schedule from inside.

**Workaround shipped:** `cron-tick-source` — a fiber whose config IS the
current time; the operator-lane driver (`cron-kit tick`) rewrites its
config every interval and the reconcile-by-id restart emits the tick.
**Cost, measured:** every tick is a full fiber cycle — 4 `FiberTransition`
ledger events + effect withdraw/register churn per tick (fire-run ledger
seq 28–33), ~5 ledger rows before any work happens. At a 15-minute duty
cadence that is ~480 bookkeeping rows/day for the tick alone.

**Packet-card shape:** a `jinn:clock` capability bundle — `now` (effect =
read) plus an alarm surface (request a wake at T / every P; delivery as a
`handle-event` with a typed payload; every wake a ledger event; grants
scope how fine a timer a plugin may hold). Retires `cron-tick-source` and
`cron-kit tick` outright; `jinn:cron`'s tick topic then has a kernel-side
emitter.

## 2. The event bus has no ledger tap — emits are invisible to Law 2

`DispatchTrace` exists in the ledger schema but is reserved/unwired (known
M1-P7 debt). Consequence with a production capability on top: neither the
tick emit nor a job's FIRE emit lands any direct ledger event — a fire-run
ledger shows the fs writes and contract calls around a fire, but the fire
itself is reconstructable only by inference. "Every fire ledger-visible"
currently holds via the scheduler's history-write effect, not via the bus.

**Packet-card shape:** wire the bus tap: one `DispatchTrace` per emit
(topic, mode, listener count, contained-failure count). For cron that line
IS the audit statement "job X fired at T".

## 3. `jinn:fs` world surface is read/write only — thinner than its bundle

The contract bundle (`kernel-pin/contracts/jinn-fs`) declares `list`,
`read`, `write`, `remove` (+ `file-meta` with `modified-ms`); the plugin
world's `fs` import and the daemon's HostFs answer only `read` and
`write`. No listing, no metadata, no append, no remove.

**Consequences hit:** the health snapshot cannot enumerate or stat
anything — its honest observable surface shrank to a write/read-back
probe; the scheduler's run history is a full-file rewrite per fire (O(n)
per append; bounded at 500 records to compensate).

**Packet-card shape:** finalize `jinn:fs` to its bundle (list/meta/remove
with the declared effect classes), and consider an `append` operation
(effect = revertible, inverse = truncate-to-prior-length) — the natural
shape for guest-kept logs.

## 4. Nested dispatch deadlocks until the guest deadline — by construction

An emit awaits each listener delivery end-to-end; a listener that calls
back into the emitting instance during handling (e.g. a job runner calling
`jinn:cron` `history` while handling that job's fire) parks its call on
the emitter's busy supervisor channel — deadlock until the 5s guest
deadline kills it. Designed around, documented in the contract (`jinn-cron`
README §Fire events: introspection belongs in `activate`), but the
failure mode is silent-slow and will bite every seam that composes
providers with consumers.

**Packet-card shape:** kernel-side reentrancy detection at the broker /
event port — an immediate `invalid("reentrant call to a busy instance")`
refusal (ledgered) beats a 5s hang; a queued-delivery lane is the bigger
alternative.

## 5. Three of four base host-provider contracts have no live provider

The world imports `process`, `net`, `keystore`; the daemon registers only
HostFs. Any call on the other three answers `missing-dependency` — the
packet card's own "system/disk health snapshot" (e.g. `df` via
`jinn:process`) is unbuildable today; the shipped job probes what `jinn:fs`
can honestly reach instead.

**Packet-card shape:** register the remaining base providers behind their
declared bundles (process first — it unlocks real host probes), each with
the same grant + ledger discipline HostFs has.

## 6. No transactional pairing of related effects

The scheduler must persist state + history per tick as two separate
`jinn:fs` writes. The contract documents the chosen tear: state first, so
a crash between them loses a record but never doubles a fire. The
capability gap: no way to declare "these two writes commit together".

**Packet-card shape:** contract-level effect groups (one label, one
inverse, N operations) — also the shape hot-swap state handoff wants.

## 7. No guest-to-guest readiness gating on the dynamic string lane

Sibling activation order is arbitrary (evidence: boot ledger shows the
consumer Active before the scheduler). A consumer resolving a sibling's
contract at activate may hit `missing-dependency`; a tick emitted before
the scheduler listens is silently unheard. Known post-M1 kernel surface
(the M1 demo greeter carries the same comment); now it has a production
consumer. Patterns forced on every plugin: opportunistic peeks, replay
absorption (the firing law shrugs off the boot's replayed tick).

**Packet-card shape:** per-entry dependency declaration for wasm entries
(activate only after named contracts are provided), i.e. the typed lane's
epoch gating extended to the string lane.

## 8. HostFs undo retention is unbounded in-memory

Every write effect's prior content lives in the provider's RAM forever
(`hostfs.rs` undos map; no compaction, no spill). The cron duty cycle
writes ~4 effects per fire; a week of 15-minute duty ≈ 2,700 retained
prior-contents, growing monotonically. Fine for a demo, wrong for a
daemon.

**Packet-card shape:** an effect-retention policy at the provider seam —
spill inverses to disk keyed by effect id, or a declared bounded window
whose expiry is itself a ledger event (an honest "no longer revertible").

## 9. Profile write-back vs external editors — an uncoordinated shared file

The tick driver (and any operator tooling) edits `profile.json` while the
daemon holds a bidirectional write-back lane over the same file. Not
observed clobbering in this phase (no write-back fired in the cron runs),
but the race is structural: a runtime write-back can overwrite a
just-written external edit. The driver re-reads every interval so its
worst case is one delayed tick.

**Packet-card shape:** an operator edit surface on the daemon (patch one
entry's config over the API plugin when it lands), or a documented
last-writer-wins + rewrite-on-loss protocol for external editors.

## 10. The operator log is unconditionally ANSI-styled

The daemon colors stderr even when it is a file/pipe; field name, `=`, and
value are separated by escape codes, so naive log matching fails (the
composition harness ships an ANSI stripper). Honor `NO_COLOR` / detect
non-tty in the daemon shell.

## 11. WIT ergonomics, small but constant

Hand-bookkept u64 undo/listen tokens per guest; per-contract hand-rolled
byte-wire conventions; `handle-call` operations as bare strings. All
workable; the pinned surface's own v0.2 note (typed per-contract import
worlds) would remove most of it. Logged so the v0.2 refinement has a
consumer's voice.

---

## What held (evidence the paradigm carries production shape)

- **Reconcile-by-id is surgical:** every tick restarts exactly the tick
  fiber; scheduler and consumer keep their fibers (composition:
  `reschedules_on_config_edit_through_reconcile`).
- **Pin-by-hash admission, grants, and refusals all ledger-visible:**
  `GrantRefused` lands when the profile withdraws `jinn:cron` from the
  consumer, and the consumer's honest `unavailable` marker follows
  (`the_cron_grant_gates_the_consumer_peek`).
- **LIFO withdrawal with clean flags:** disposing the scheduler leaves
  `EffectWithdrawn`/`ServiceWithdrawn` trails, `clean: true`
  (`disposing_the_scheduler_leaves_a_clean_ledger_trail`).
- **State through granted contracts survives restarts:** the firing law's
  no-backfill/catch-up semantics proved through a real daemon SIGINT +
  reboot (`restart_fires_once_and_records_the_gap_without_backfill`).
- **Guest provisions and every broker crossing recorded:** the fire-run
  ledger is a complete causal story minus the emits (finding 2).
