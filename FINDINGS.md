# Kernel findings — the two-way iteration log

Frictions the distribution hits against the pinned kernel, logged with
enough specificity to become `jinnd` packet cards. **Kernel changes are
never made here** (AGENTS.md standing order 1); this file is the channel.
Each entry: what happened, the evidence, and the capability shape that
would retire the workaround. Numbers are stable — code comments cite them.

Phase 1.3 (cron seam) baseline: every entry below was hit while building
the first real capability. Evidence grades were audited by the independent
round-1 verification and are stated per entry: entries 1, 2, 3, 6, and 8
are packet-card-ready (reproducible, shaped); the rest are honest
observations whose cards need more evidence, marked as such. What HELD is
at the bottom — the kernel earned that section too. Entries 1 and 2 are
**closed** as of the `01133c45` pin bump (jinnd M2-K2) and entries 3 and 8
as of the `41cb2f47` pin bump (jinnd M2-K3): each carries a closure note
appended in place, and the original text stands as the record of what the
friction was. Entries 14 and 15 were hit adopting `41cb2f47`.

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

**Closed 2026-08-28 — retired by pin 01133c45 (jinnd M2-K2).** The kernel
ships `jinn:clock`: `now` (effect = read), plus `alarm-at` / `alarm-every`
whose requests are revertible effects (the undo cancels), every wake a
ledger event `AlarmWake` attributed to the requesting fiber, and the
resolution floor scoped by the grant (250 ms default, R9). Harness side:
`cron-tick-source`, `cron-kit tick`, and the `jinn:cron/tick` topic are
retired outright — `cron-scheduler` holds one periodic alarm at `tick-ms`
and plans once at `activate` off `now` (see finding 13 for why that second
call is needed); `DispatchTrace` is the first-class fire line and the
per-fire run record stays as the outcome document. Measured cost change:
~5 ledger rows of fiber churn per tick → one `AlarmWake` row per wake (the
fs write effects and one `DispatchTrace` per fire emit only when something
actually fires).

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

**Harness-side mitigation shipped (round 2):** every fire now writes one
per-fire run record through granted `jinn:fs`
(`cron/runs/<job>/<scheduled-ms>.json`), whose ledgered effect label names
the job and boundary — fires are ledger-identifiable today. The bus tap
remains the first-class answer; the mitigation does not replace it.

**Closed 2026-08-28 — retired by pin 01133c45 (jinnd M2-K2).** The tap is
wired: every bus emit lands exactly one `DispatchTrace { topic, mode,
listeners, failures, emitter }` ledger event. For cron, the fire emit on a
job's own topic (e.g. `cron:health`) IS the audit statement "job X fired" —
first-class, no inference. The per-fire run record
(`cron/runs/<job>/<scheduled-ms>.json`) stays as the outcome document
rather than as the fire's only ledger identification.

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

**Closed 2026-08-28 — retired by pin 41cb2f47 (jinnd M2-K3).** The
`jinn:plugin` world is 0.2.0 and its `fs` import IS the `jinn:fs@0.2.0`
bundle verbatim: `list`, `meta` (reads, ledgered contract calls), `write`,
`append` (inverse = truncate-to-prior-length), `remove` (now revertible),
`fs-error` with a TYPED `not-found`, and idempotency keys on every
mutation. The write signature changed — a breaking guest migration, taken
deliberately (harness PR #4, no compat shims). Harness side: the
scheduler's run history is `cron/history.jsonl`, grown by ONE `append` per
recording tick (`fs append cron/history.jsonl` on the ledger, never a
`write` of the log — the composition suite asserts it); absence is
classified by case, and `read_error_is_absence` is gone with the folded
message it parsed; the health snapshot enumerates its directory and the
fired job's run records with `list` and stats the history log with `meta`.

## 4. Nested dispatch deadlocks until the guest deadline — by construction

An emit awaits each listener delivery end-to-end; a listener that calls
back into the emitting instance during handling (e.g. a job runner calling
`jinn:cron` `history` while handling that job's fire) parks its call on
the emitter's busy supervisor channel — deadlock until the 5s guest
deadline kills it. Designed around, documented in the contract (`jinn-cron`
README §Fire events: introspection belongs in `activate`), but the
failure mode is silent-slow and will bite every seam that composes
providers with consumers.

*Evidence grade:* structural (one supervisor task exclusively owns each
instance's store and serves calls from its channel — `jinnd-wasm`
`instance.rs`), not yet a runnable transcript; the round-1 verification
did not independently reproduce it. The packet card should start with a
two-plugin repro fixture.

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

*Evidence grade:* source-confirmed (the daemon assembly registers exactly
one provider, `hostfs.register` in `jinnd-daemon` `daemon.rs`); no run
transcript of a refused `jinn:process` call yet.

## 6. No transactional pairing of related effects

The scheduler must persist state + history per tick as two separate
`jinn:fs` writes. The contract documents the chosen tear: state first, so
a crash between them loses a record but never doubles a fire. The
capability gap: no way to declare "these two writes commit together".

**Packet-card shape:** contract-level effect groups (one label, one
inverse, N operations) — also the shape hot-swap state handoff wants.

## 7. No guest-to-guest readiness gating on the dynamic string lane

Sibling activation order is UNSPECIFIED — that is the finding, not any
particular order: one boot ledger showed the consumer Active before the
scheduler; the round-1 verification observed the opposite order on its
boots. Nothing guarantees a consumer's provider is Active when the
consumer activates: a resolve may hit `missing-dependency`; a tick emitted
before the scheduler listens is silently unheard. Known post-M1 kernel
surface (the M1 demo greeter carries the same comment); now it has a
production consumer. Patterns forced on every plugin: opportunistic peeks,
replay absorption (the firing law shrugs off the boot's replayed tick).

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

**Closed 2026-08-28 — retired by pin 41cb2f47 (jinnd M2-K3).** Every
revertible fs effect's inverse is made durable in a retention store beside
the data root (`<data>.inverses/`, keyed by effect id, fsynced, spilled
BEFORE the mutation commits, refused if it cannot be) and provider memory
holds an index of headers only; a completed revert or withdrawal consumes
and reclaims the spill. Retention is event-driven — no compaction daemon.
Soak side: the byte curve the +7d audit watches (SOAK.md) now measures
bounded, durable retention — `$SOAK/data.inverses/` is inside the root the
health check sizes — instead of unbounded provider RAM. What the spill
does NOT bound is the count of live inverses of a long-lived fiber: the
scheduler's state writes (one per wake) stay retained until that fiber is
disposed, which is finding 14's territory, not this one's.

## 9. Profile write-back vs external editors — an uncoordinated shared file

Operator tooling (formerly the tick driver) edits `profile.json` while the
daemon holds a bidirectional write-back lane over the same file. Not
observed clobbering in this phase (no write-back fired in the cron runs),
but the race is structural: a runtime write-back can overwrite a
just-written external edit. Round-1 verification rightly noted the duty
driver was itself part of the problem — a non-atomic read/modify/write
editor; round 2 made both the driver and the test harness atomic
(stage + rename), which the composition suite now exercises against the
real watcher. The cross-process last-writer-wins race with the daemon's
write-back remains, and is the kernel-side part of this finding.

**Packet-card shape:** an operator edit surface on the daemon (patch one
entry's config over the API plugin when it lands), or a documented
last-writer-wins + rewrite-on-loss protocol for external editors.

## 10. The operator log can arrive ANSI-styled on a non-tty

In this harness's runs, the daemon's stderr redirected to a file carried
CSI escape sequences between a log field's name, `=`, and value — naive
substring matching fails, and the composition harness ships an ANSI
stripper because of it. The round-1 verification could NOT reproduce this
(their redirected stderr contained zero CSI sequences), so the behavior is
environment-sensitive; the ask stands regardless: the daemon shell should
pin the answer by honoring `NO_COLOR` / detecting non-tty explicitly.
Low severity.

## 11. WIT ergonomics, small but constant

Hand-bookkept u64 undo/listen tokens per guest; per-contract hand-rolled
byte-wire conventions; `handle-call` operations as bare strings. All
workable; the pinned surface's own v0.2 note (typed per-contract import
worlds) would remove most of it. Ergonomic feedback for the v0.2
refinement, not a packet card — logged so that refinement has a consumer's
voice.

## 12. No machine-readable "booted, watcher armed" signal

(Contributed by the round-1 verification.) The daemon offers no signal
that its boot reconcile is done AND its file watcher is armed; an edit
landing in the window between them is silently unseen. The composition
harness compensates by rewriting the profile until the expected restart
appears in the log; the duty driver relies on its next interval. Distinct
from finding 7 (that is guest-to-guest; this is operator-to-daemon).

**Packet-card shape:** a readiness line on stderr is the minimum; the
honest fix is a machine-readable status surface (the future API plugin's
first duty, alongside finding 9's edit lane).

*Evidence added 2026-08-28 (pin 41cb2f47 adoption):* reproduced under
suite load — a consumer nonce-bump edit landed with the daemon's log
showing exactly one reconcile (`created=[…] restarted=[]`) and never a
restart; the same test passes in isolation. The composition harness now
carries the mitigation as a helper (`edit_profile_until_restart`: rewrite
atomically until the expected restart appears in the log) and every
config-edit proof uses it.

## 13. `alarm-every`'s first wake is one full period out

There is no "fire now, then every P" shape: a periodic alarm's first wake
lands at `now + P`. A scheduler that only held the alarm would therefore be
blind for one whole period after every (re)activation — and since alarms do
not survive a kernel restart, every daemon restart re-opens that hole. At
the soak's 15-minute duty period it is a 15-minute blind window per
restart, which is exactly the window a monitoring job must not have.

**Workaround shipped:** `cron-scheduler`'s `activate` calls `now` and runs
one tick plan immediately, then requests the periodic alarm — the catch-up
fire lands at once instead of one period later. Cost: an extra
`ContractCall` plus that plan's own writes on every activation.

**Packet-card shape:** either an `immediate: bool` on `alarm-every` (first
wake now, then every P), or document `alarm-at(now)` + `alarm-every` as the
idiom together with a guarantee about their relative delivery ordering.

*Evidence grade:* contract-documented — `contracts/jinn-clock/contract.wit`
states "first one period from now"; the harness-side workaround is shipped
in `cron-scheduler`'s `activate`. Low severity.

## 14. Every `jinn:fs` mutation is withdrawn with its fiber — there is no durable-state lane

Since pin `41cb2f47` (M2-K3, ruled LAW-conformant on the kernel side: R5,
I1 — dispose withdraws exactly the fiber's contribution, fs effects
included), every `write`/`append`/`remove` a guest makes joins its fiber's
journal and is undone LIFO when the fiber is disposed. The daemon's
graceful shutdown disposes every fiber. Consequences, all reproduced on the
real daemon:

- A clean SIGINT reverts the scheduler's `cron/state.json` to its content
  at that fiber's activation, truncates the history log, and restores the
  consumer's `health/report.json` — the ledger of one composition run:
  `EffectWithdrawn "fs write cron/state.json"` ×3, `"fs append
  cron/history.jsonl"`, `"fs write health/report.json"`, `"fs write
  health/boot.json"`, all `clean: true`, in the shutdown trail. Firing
  law #3 (state persists across daemon restarts) therefore does NOT hold
  across a clean restart at this pin: the reboot starts the schedule fresh
  (`schedule-started`, no `skipped` record), and the boundaries the
  previous process fired are unobservable to it.
- A reconcile restart of one entry (config edit) does the same to that
  entry's persisted documents; the loader does not hand a snapshot across
  a config-edit restart, so the successor cannot carry the state in
  memory either.
- A process death (SIGKILL) leaves the files as they were — the inverses
  stay spilled as orphans in the retention store, nothing replays them —
  so persistence across a CRASH holds, and is now the path the
  composition suite proves firing law #3 through.

The class of state at stake is a *record* (the newest boundary already
processed; a fire's outcome document; a health report): reverting it does
not un-fire anything, it makes the next process mis-count. The seam has no
lane for such state — every granted mutation is a revertible contribution.

**Harness-side handling shipped:** the composition suite proves restart
persistence through the crash path (`Daemon::kill`) and pins the clean-
shutdown withdrawal as a transcript test
(`a_clean_shutdown_withdraws_the_fibers_persisted_contribution`) that goes
red when the kernel retires this; SOAK.md §Stop carries the operational
consequence.

**Packet-card shape (alternatives, for the COO/verifier to rule on):**
(i) a `jinn:ledger` read import so a guest rebuilds its record-state from
its own `DispatchTrace`/effect trail — the constitutional answer if the
ledger is the record lane; (ii) an effect class for records — `commit` /
"release" an effect out of the fiber journal, still ledgered, or a
declared `record` scope on the grant whose mutations are not withdrawn on
dispose; (iii) shutdown as *suspend* — process exit stops fibers without
withdrawing (only an explicit dispose withdraws), which matches how a
crash already behaves. Severity: high for any plugin that keeps state.

*Evidence grade:* reproducible — pinned by a composition test against the
real daemon; kernel-side confirmed by M2-K3's own runbook (step 5: the
scribe's journal appends are withdrawn on dispose).

## 15. Effects registered while a dispose is in flight escape the withdrawal

Observed once (pin `41cb2f47`, composition run `restart-31313`): a SIGINT
landed mid-tick, and the effects the scheduler registered after the
journal was sealed but before the fiber stopped (the tick's state write,
the fire's run-record write, and the history append; effect ids
`…301`, `…304`, `…305`) appear in the ledger as registered and never
withdrawn, while the earlier effects of the same fiber are withdrawn
`clean: true`. The disk was left in a torn shape: part of a tick undone,
part kept. Not yet reproduced deliberately.

**Packet-card shape:** quiesce the instance (drain its in-flight call
before sealing the journal) or refuse registrations after the seal with a
ledgered error, so a dispose trail is exactly the fiber's contribution
(I1) — never a prefix of it.

*Evidence grade:* single ledger observation; the card should start with a
repro that disposes during a guest's `handle-event`.

---

## What held (evidence the paradigm carries production shape)

- **The 0.2.0 fs bundle does what it says:** `append` is O(1) per record
  and its inverse truncates; `list` answers sorted names; `meta` agrees
  with the bytes written; a missing path is a typed `not-found`; every op
  is a ledgered contract call and every mutation a labeled effect
  (composition: `run_history_is_append_backed_and_the_consumer_sees_the_wider_surface`).

- **Reconcile-by-id is surgical:** a config edit restarts exactly the
  edited entry — the scheduler — and the consumer keeps its fiber
  (composition: `reschedules_on_config_edit_through_reconcile`).
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
  ledger is a complete causal story, emits included since the
  `DispatchTrace` tap landed (finding 2).
