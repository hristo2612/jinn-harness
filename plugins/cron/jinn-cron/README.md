# `jinn:cron@0.1.0` — the scheduled-work contract

The service definition of the cron seam. This document is the contract's
prose law; the types in `src/` are its schema. Designed to outlive this
implementation (the kernel's R12 discipline): within 0.x every change here is
strictly additive, and the shapes carry explicit version headroom (schedule
variants, outcome variants).

## Names

| Name | Value | What it is |
|---|---|---|
| Contract | `jinn:cron` | Provided by the scheduler; grant it to resolve and call. |
| Operations | `jobs`, `history` | Read-only introspection calls on the provider (empty request payload, JSON answer). |
| Clock grant | `jinn:clock` | Time enters the seam here. A provider-side requirement: the scheduler holds one periodic alarm whose period is `tick-ms`. Consumers never need it. |

All payloads on this seam are UTF-8 JSON with kebab-case keys.

## Settings namespace (the scheduler's config subtree)

```json
{ "tick-ms": 60000,
  "jobs": [ { "id": "health", "every-ms": 900000, "topic": "cron:health",
              "payload": { "free": "form" } } ] }
```

- `tick-ms` — the period of the single `jinn:clock` periodic alarm the
  scheduler holds; how often the firing law is evaluated. Default `60000`.
  It must be at least the granted clock resolution floor (250 ms with a bare
  `"jinn:clock"` grant, coarser with an explicit scope — kernel R9): the
  kernel refuses a periodic alarm finer than the floor and the scheduler's
  activation fails loudly rather than running blind. A wake cadence is not a
  schedule — a job fires on its own `every-ms` boundaries, at worst one
  `tick-ms` late.
- `id` — unique per scheduler; duplicates beyond the first are config faults.
- `every-ms` — the schedule spec, v0.1: a fixed period anchored at the Unix
  epoch; boundaries are the instants `k * every-ms` (k ≥ 1). `0` is a config
  fault. Calendar/cron-expression schedules are a planned **additive**
  extension (a new field beside `every-ms`, never a reinterpretation).
- `topic` — where this job's fire events go. Empty is a config fault.
- `payload` — opaque JSON handed through to every fire event, `null` default.

Config faults never fail activation: faulted entries are excluded and each
fault is a `config-fault` run record — visible in history, never silent.

## Time (the clock)

Time enters through the kernel's `jinn:clock` capability, granted to the
scheduler (FINDINGS.md #1, closed by the `01133c45` pin). Two entries, and
only two:

- **At `activate`** the scheduler reads `now`, runs one tick plan
  immediately, and requests `alarm-every(tick-ms, token)`.
- **At every wake** the kernel delivers
  `handle-event(token, "jinn:clock/alarm", payload)`, payload = the 8-byte
  little-endian unix-ms wake instant; that instant is the tick's `now-ms`.

Both paths hand the firing law the same internal tick shape:

```json
{ "seq": 12, "now-ms": 1756350000000 }
```

`seq` is a per-activation wake counter (`0` = the activate plan, then 1, 2,
…). `now-ms` is wall-clock milliseconds since the Unix epoch. Ticks whose
`now-ms` does not advance are no-ops by construction (no new boundary can be
due).

**The clock is the kernel's** — the scheduler never has another one, and
`seq` is an operator-facing edition marker, not a guard: it is not validated
for monotonicity, it restarts at `0` on every activation, and a repeated
`seq` carrying a later clock still fires normally. The firing law's boundary
accounting is the only replay/rewind protection — a duplicate payload and a
rewound clock are both no-ops because no new boundary is due.

## The firing law (missed fires, restarts — no silent backfill)

For each job, the scheduler keeps `last`: the newest boundary already
processed. On a tick at time `T`, the due boundaries are those in
`(last, T]`:

1. **At most one fire per job per tick.** Only the NEWEST due boundary
   fires.
2. **Skipped boundaries are recorded, never fired.** If more than one
   boundary is due, the earlier ones become a single `skipped` run record
   (count + range) and the fire event carries `missed-before`.
3. **Restarts follow the same law.** State persists across scheduler
   restarts and daemon restarts (`cron/state.json`, snapshot/restore across
   hot-swaps). Boundaries elapsed while down: newest fires (one catch-up,
   honestly marked by its `missed-before`), the rest are recorded skipped.
   There is no backfill, and no fire is ever silently dropped — every
   boundary ends as exactly one of: fired, skipped-on-record.
   Alarms do NOT survive a kernel restart (`jinn:clock` v0.1), so the
   restart re-entry is the guest's own act: the scheduler re-requests its
   alarm in `activate` and plans there off `now`, which lands the catch-up
   fire immediately at boot instead of one `tick-ms` later (FINDINGS.md
   #13).
   A persisted `last` may predate a config edit that changed `every-ms`:
   all boundary accounting therefore happens on the CURRENT grid, with
   `last` re-floored to it — a period change mid-window never corrupts the
   count (and the arithmetic saturates, so hostile state can at worst
   under-count, never wrap).
4. **Lost state is a recorded event, not a guess.** A job with no state
   (new, or state lost) starts its schedule at the current tick: no fire, a
   `schedule-started` run record, `last = floor(T / every-ms) * every-ms`.
   Elapsed boundaries before a state loss are unobservable and are NOT
   reconstructed.

## Fire events

Emitted on the job's `topic`, dispatch mode `serial`, selector `all`:

```json
{ "job": "health", "scheduled-ms": 1756350000000, "now-ms": 1756350004120,
  "missed-before": 0, "tick-seq": 12, "payload": null }
```

Consumers answer with opaque bytes; the count of settled answers lands in the
job's run record. **A consumer must not call `jinn:cron` (or otherwise call
back into the scheduler) while handling a fire**: the scheduler is awaiting
that very delivery, and the call chain deadlocks until the kernel's guest
deadline kills it (FINDINGS.md #4). Introspection calls belong in `activate`.

Every fire emit lands exactly one `DispatchTrace { topic, mode, listeners,
failures, emitter }` ledger event, with `topic` the job's own topic. **That
line is the first-class audit statement "job X fired"** (FINDINGS.md #2,
closed by the `01133c45` pin) — no inference from surrounding effects.

## Run history

Two lanes, both under the scheduler's `jinn:fs` scope:

- **Per-fire records** — `cron/runs/<job>/<scheduled-ms>.json`, one file
  per fire, written after the emit settles and carrying the full run
  record. This write is one granted-contract effect whose ledger label
  names the job and the boundary: **it is the fire's outcome document** —
  how the fire went. That it happened is recorded first-class by the emit's
  own `DispatchTrace` ledger event (§Fire events); the two lanes answer
  different questions. Job ids are path-safe by construction
  (`[A-Za-z0-9_-]`, enforced at config parse).
- **The history log** — `cron/history.jsonl`: ALL run records (fires,
  skips, starts, faults), one JSON record per line, newest last, grown
  ONLY by `jinn:fs` `append` — one append per recording tick, sized by
  that tick's records, never a rewrite (O(1) per record; the pre-0.2.0
  rewrite-per-fire lane is retired, FINDINGS.md #3). The log is unbounded
  on disk by design (the ledger is the archive; a rotation policy would be
  an additive change here); the `history` operation serves a window of the
  newest 500 records.
- **The legacy document** — `cron/history.json` (one JSON array, the
  pre-0.2.0 lane): read once at activation as the seed of the window,
  before the log's records, and never written again. A root that carries
  one keeps its records without duplicating them.

Record shape (`outcome` is externally tagged, additive):

```json
{ "job": "health", "scheduled-ms": 1756350000000, "now-ms": 1756350004120,
  "tick-seq": 12, "outcome": { "fired": { "answers": 1 } } }
```

Outcomes: `fired { answers }` (0 answers = no live listener answered — a
visible duty gap, not an error), `skipped { boundaries, first-ms, last-ms }`,
`schedule-started`, `config-fault { detail }`, `emit-failed { detail }`,
`state-fault { path, detail }`. A reader encountering an outcome tag it
does not know carries it verbatim (see §Additivity).

State (`cron/state.json`) is written before history on each tick: if the
writes are torn apart by a crash, the failure mode is a missing record, never
a double fire (the write pair is not transactional under `jinn:fs` —
FINDINGS.md #6).

Idempotency keys (`jinn:fs` 0.2.0, keyed exactly-once per fiber): the
per-fire record's key is its own path — the boundary is the fire's
identity — so a repeated delivery within one fiber answers the recorded
effect instead of writing twice; state writes and history appends claim
no key (each is a new effect by construction — a tick never repeats).

**Persistence across a clean daemon stop does not hold at kernel pin
`41cb2f47`** (FINDINGS.md #14): every granted fs mutation is withdrawn
with its fiber, and a graceful shutdown disposes every fiber, so the
persisted state and history are reverted to their content at that
fiber's activation. A process death leaves them intact. Firing law #3 is
stated as the contract's intent and proven through the crash path until
the kernel provides a durable-state lane.

## Persistence honesty

On activation the scheduler classifies each persisted document
(`cron/state.json`, `cron/history.jsonl`, the legacy `cron/history.json`)
honestly:

- **Genuinely absent** (the bundle's typed `not-found`, classified by case,
  never from a message) → a fresh default, silently (a first boot is not
  an error).
- **Present but undecodable** (for the log: any non-blank line that is not
  a record, e.g. a torn tail after a crash mid-append, reported by line
  number) → the original bytes are preserved verbatim under
  `cron/quarantine/<name>`, the loss is recorded as a `state-fault` run
  record naming both paths, and the schedule starts fresh under firing law
  #4. Never a silent default.
- **Unreadable for any other reason** (permissions, provider failure) →
  the activation fails loudly (contained per the kernel's R11). Defaulting
  there could re-fire boundaries the unreadable state already processed.

## Additivity (the R12 promise, mechanically)

- Every wire schema tolerates and **preserves** unknown sibling fields
  across a decode → encode round trip — **at every nesting level**: the
  payload of each known `outcome` variant carries its own flattened
  extension map, so a field a newer writer adds inside e.g.
  `outcome.fired` survives this reader verbatim.
- The schedule position in a job entry is open: this reader knows
  `every-ms`; an entry with a schedule it does not recognize degrades to a
  per-entry `config-fault` — contained and recorded, never a document
  rejection.
- An unrecognized `outcome` tag decodes as an opaque carrier and re-encodes
  identically. (The unit outcome `schedule-started` has no payload to
  extend; a newer version reshaping it into an object lands in the same
  carrier, still verbatim.)

## Provider operations

- `jobs` → `{ "jobs": [ { "id", "every-ms", "topic", "next-ms" } ] }` —
  `next-ms` is the next boundary strictly after `last` (absent until the
  schedule has started).
- `history` → the run-record window (newest 500, oldest first), as a JSON
  array.

## Changes

- **0.1.0 (2026-08-28, kernel pin `01133c45`):** the tick topic
  `jinn:cron/tick` and its payload are withdrawn with the kernel clock's
  arrival — no external emitter ever existed outside this repo's retired
  stand-in; `tick-ms` added (additive).
