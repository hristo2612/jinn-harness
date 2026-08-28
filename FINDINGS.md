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
as of the `41cb2f47` pin bump (jinnd M2-K3), and entries 14 and 15 (hit
adopting `41cb2f47`) as of the `4eb4a93` pin bump (jinnd M2-K4): each
carries a closure note appended in place, and the original text stands as
the record of what the friction was. Entries 16, 17 and 18 were hit
adopting `4eb4a93` and are **closed** as of the `9e61e47` pin bump (jinnd
M2-K5), which also delivers entry 12's stated minimum (a readiness line;
its status surface remains open).

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

**Minimum delivered 2026-08-28 — pin 9e61e47 (jinnd M2-K5 #18/#12).** The
daemon arms its watcher BEFORE the boot reconcile and, once that reconcile
is done, emits exactly one machine-readable line on stderr:
`{"jinnd":"ready","watcher":"armed","profile":"<canonical path>"}`. The
window this entry named no longer exists (an edit landing during the boot
reconcile is a watched delivery, applied after it — see entry 17's
closure), and the line is the operator lane's one gate: the composition
kit's `booted()` waits for it and every proof edits after it
(`readiness_is_announced_once_after_the_boot_reconcile`,
`an_edit_landing_before_readiness_is_applied`); SOAK.md §Start keys on it
instead of `boot.json`. `edit_profile_until_restart` and the
rewrite-until-restart loop are deleted. What stays OPEN of this entry is
the honest fix it named: a machine-readable status surface (the API
plugin's first duty, with entry 9's edit lane).

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

**Closed 2026-08-28 — retired by pin 4eb4a93 (jinnd M2-K4, shape (iii)
adjudicated in the kernel's decision log: suspend ≠ dispose).** A
contribution belongs to the profile ENTRY, not to a fiber incarnation or a
process. Daemon shutdown now SUSPENDS every fiber: kernel registrations
(the alarm, the provision, the listener) release with their inverses on
the record, world mutations (`jinn:fs` writes/appends) are RETAINED in the
entry's durable journal, and one typed `FiberSuspended { retained }` event
lands per fiber; the transitions carry `cause: Suspend`. A reconcile
restart hands the successor incarnation the entry's journal (a config
edit no longer reverts the entry's documents); only the entry's removal
from the composition withdraws its trail, LIFO across incarnations and
across process restarts. Crash and clean shutdown agree on the disk
outcome; only the clean path reaches quiescence and flushes the ledger.
Harness side: the pinned transcript went red by design and is replaced by
`a_clean_shutdown_suspends_and_a_restart_resumes_the_schedule` (files
retained, `FiberSuspended` ×2, no `fs` withdrawal, the next boot resumes
the schedule with no second `schedule-started`); the restart proof is back
on the clean path; SOAK.md's hard-stop ruling is retired. `jinn:plugin` is
`0.3.0` (R12): no signature change, the lifecycle semantics are contract.

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


**Closed 2026-08-28 — retired by pin 4eb4a93 (jinnd M2-K4 ruling 5).**
Reproduced kernel-side first (a dispose during a guest's `handle-event`),
then closed fail-closed: once an instance's journal seals for withdrawal
or suspension, every further registration REFUSES with a ledgered
`InactiveContext` error, so a dispose trail is exactly the fiber's
contribution (I1), never a prefix of it. Observed on this harness at the
new pin (composition run `clean-stop-6425`): a SIGINT landing mid-tick
left `fs append refused: the seat's journal is sealed for withdrawal` on
the ledger (`ErrorRecorded` ×2, `InactiveContext` then the guest's
`PluginFailed`), the tick's earlier effects retained, nothing torn between
withdrawn and kept. What remains of the shape — the OTHER half of the
card's disjunction, draining an in-flight handler before the seal — is
finding 16.

## 16. Suspend seals the journal under an in-flight handler — a planned stop can tear a tick

Pin `4eb4a93` closes finding 15 by refusal: a registration after the seal
fails loudly. It does not drain the guest call in flight first. Observed on
the real daemon (composition run `clean-stop-6425`, pin `4eb4a93`): a
SIGINT landed while the scheduler's wake handler was mid-tick — the tick's
state write had landed (`last` advanced to the boundary), the fire had
emitted (`DispatchTrace`, the consumer's report written), the per-fire run
record registered, and then the history append was refused
(`InactiveContext: fs append refused: the seat's journal is sealed for
withdrawal`), the handler failed (`PluginFailed`), and the daemon
suspended. On disk: state says the boundary was processed, its run record
exists, the history log has no line for it. The seam's own contract
absorbs this honestly (§Run history: state before history, so a torn tick
loses a record and never doubles a fire), and the composition proof
tolerates `last` being one boundary past the newest history record. But
this was a PLANNED stop — the operator's Ctrl-C — and the kernel had every
opportunity to let a sub-second handler finish.

**Packet-card shape:** on suspend (and on dispose), quiesce the instance
before sealing its journal — await the in-flight guest call under the
existing guest deadline, then seal; refusal stays as the backstop for a
handler that outlives the deadline. Severity: low for cron (a lost history
line, ledger still complete); medium for any guest whose handler makes
several related effects (the transactional gap of finding 6, met at
shutdown).

*Evidence grade:* reproducible under suite load (the wake cadence is 500 ms
and a tick takes ~50 ms, so roughly one clean stop in ten lands mid-tick);
the ledger of the named run is the transcript.

**Closed 2026-08-28 — retired by pin 9e61e47 (jinnd M2-K5 #16).** On
suspend AND on dispose the kernel drains the in-flight guest call under
the existing guest deadline, THEN seals the journal; the seal refusal
stays as the backstop only for a handler that outlives the deadline.
Observed on this harness at the new pin (composition
`a_stop_landing_mid_tick_lands_the_whole_tick`): three planned SIGINTs
aimed inside a firing tick (delivered ~2 ms after the consumer's probe
write) each drained — the report write, the run record and the history
append all logged AFTER the `SIGINT: suspending` line, then `quiescent;
ledger flushed; bye`; `state.last` equals the newest history record after
every stop, `FiberSuspended` per fiber, no `InactiveContext`, no
`PluginFailed`. The clean-stop proof
(`a_clean_shutdown_suspends_and_a_restart_resumes_the_schedule`) no longer
tolerates `last` one boundary past the history log — it asserts exact
agreement. SOAK.md §Stop's torn-tick paragraph is retired.

## 17. The daemon's own-write-back check swallows an external edit that lands during a reconcile

`Daemon::reload` skips a delivery whose profile text equals the text it
"committed" — its guard against reconciling the echo of its own write-back.
But the committed text is captured by RE-READING `profile.json` from disk
at the end of every apply (boot included), not from the bytes the daemon
itself wrote. An external edit that lands while a reconcile is still
applying is therefore captured as the daemon's own echo: the watcher's
delivery of that edit reads back the same bytes, the guard fires, and the
reconcile returns an empty report — logged as
`reconciled created=[] restarted=[] disposed=[] unchanged=[]`, every list
empty (the diff never ran). Any identical rewrite afterwards is skipped
the same way, so finding 12's mitigation (rewrite the same bytes until the
restart shows) cannot escape it: the composition log of run `grants-6778`
shows the boot reconcile at `13:55:00.28Z`, the consumer nonce-bump edit
landing before it finished, and then one all-empty reconcile every ~600 ms
for 30 s. Deterministic given the interleaving; the interleaving is common
under load, where the boot reconcile is slow and the test's edit is fast.
In isolation the same tests pass.

**Harness-side mitigation shipped:** `edit_profile_until` rewrites the
document atomically with DIFFERENT bytes on each attempt (a trailing
newline toggled) until the expected observation holds; every config-edit
proof uses it.

**Packet-card shape:** recognize the daemon's own write-back by the bytes
it wrote (remember the rendered text at the save, or a write generation),
never by re-reading a file another writer may have replaced — or drop the
echo guard and let the loader's own diff answer `unchanged`. This is the
kernel-side half of finding 9 made concrete: with the guard as it is, an
operator edit can be lost with a success line in the log. Severity:
medium — an operator lane that silently drops edits.

*Evidence grade:* reproducible — the mechanism is read from the pinned
source (`reload` + the apply tail) and the named run's log is the
transcript; three consecutive suite runs hit it before the mitigation.

**Closed 2026-08-28 — retired by pin 9e61e47 (jinnd M2-K5 #17).** The
daemon recognizes its own write-back by the exact bytes the loader WROTE,
remembered at the save and consumed ONE-SHOT by the delivery that matches
them: an echo logs no `reconciled` line at all (the log never claims a
reconcile that did not run), an external edit landing during a reconcile
is applied by the next delivery, and an identical operator rewrite
reconciles `unchanged=[…]`, never skipped. Harness side:
`edit_profile_until` (byte-varying rewrites) is deleted; every config-edit
proof makes ONE atomic edit and waits for its observation
(`edit_profile_restarting`), and the `grants-6778` transcript is flipped
(`the_cron_grant_gates_the_consumer_peek` asserts zero all-empty
`reconciled` lines — the entry's signature — after two single edits).

## 18. A relative `--profile` path fails the watcher AFTER the boot reconcile has written evidence

Started from its runtime root with `--profile profile.json --ledger
ledger.sqlite` (relative paths), the `4eb4a93` daemon booted and
reconciled — the guests activated, the consumer's `boot.json` and the
scheduler's catch-up fire landed on disk and on the ledger — and THEN its
file watcher refused to start (`ERROR jinnd: file watcher unavailable
refused=KernelError { code: EffectFailed, message: "No path was found.
about [\"\"]" }`: the watched directory is the profile's parent, empty
for a bare file name) and the daemon exited 1. Refusing to serve unwatched
is the right call. The friction is the ORDER: the boot evidence the
operator waits for (`boot.json`) is written before the watcher is even
attempted, so a launcher that keys on it reads a refused start as a
running daemon — exactly what happened on the soak's third-bump start
(operator slip; `ops.log` carries the correction, the duty gap, and the
restart with absolute paths per SOAK.md §Start).

**Packet-card shape:** canonicalize `DaemonPaths` at open (an absolute
profile path is what the watcher needs — resolve against the working
directory), and validate the watcher before the boot reconcile so a
refused start writes nothing; a machine-readable readiness signal
(finding 12) would close the operator side.

*Evidence grade:* reproducible from the log line; one observation.

**Closed 2026-08-28 — retired by pin 9e61e47 (jinnd M2-K5 #18).** The
daemon canonicalizes its paths against the working directory at startup
and arms the file watcher — or refuses, with the error — BEFORE the kernel
assembles and the boot reconcile writes anything; the readiness line
(entry 12) follows the boot reconcile. Proven through the real daemon:
`a_relative_profile_path_boots_watched` (relative `--profile`/`--ledger`
from the root: readiness, watcher armed, an edit served) and
`a_refused_watcher_writes_no_evidence` (a profile in a missing directory:
exit 1, no `reconciled`, no readiness line, no ledger, no data). SOAK.md's
absolute-path caveat is retired; the supervisor wrapper keeps absolute
paths as its canonical form, not as a workaround, and §Start's evidence
is the readiness line rather than `boot.json`.

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
  reboot (`restart_rerequests_the_alarm_fires_once_and_records_the_gap`),
  and since pin `4eb4a93` the clean stop is a SUSPENSION on the record —
  files retained, `FiberSuspended` per fiber, the next boot resuming the
  schedule (`a_clean_shutdown_suspends_and_a_restart_resumes_the_schedule`).
- **Guest provisions and every broker crossing recorded:** the fire-run
  ledger is a complete causal story, emits included since the
  `DispatchTrace` tap landed (finding 2).
