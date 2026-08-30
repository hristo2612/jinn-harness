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
observations whose cards need more evidence, marked as such. Entry **35** is
answered in place by phase 2.6 — the fourth layer it predicted now
exists, so its term is a measured number rather than a structure, and the
prediction held. What HELD is
at the bottom — the kernel earned that section too. Entries 1 and 2 are
**closed** as of the `01133c45` pin bump (jinnd M2-K2) and entries 3 and 8
as of the `41cb2f47` pin bump (jinnd M2-K3), and entries 14 and 15 (hit
adopting `41cb2f47`) as of the `4eb4a93` pin bump (jinnd M2-K4): each
carries a closure note appended in place, and the original text stands as
the record of what the friction was. Entries 16, 17 and 18 were hit
adopting `4eb4a93` and are **closed** as of the `9e61e47` pin bump (jinnd
M2-K5), which also delivers entry 12's stated minimum (a readiness line;
its status surface remains open). Entry 5 is **closed** for `jinn:process`
and `jinn:net` as of the `1b098be` pin bump (jinnd M2-K6). Entries 19–24
were hit building the operator-API seam on that pin (phase 2.1): 19 and
20 are the introspection gaps the status surface names in its answers,
21 is the edit lane's revertibility hazard (transcript-pinned), 22–24 are
shape frictions with source evidence. Entries 24 and 25 are **closed** as
of the `3fd7b053` pin bump (jinnd M2-K8). Entries 19, 20, 21 and 23 are
**closed** as of the `57360cc` pin bump (jinnd M2-K7; phase 2.2), which
also closes 22's profile case; entry 25 was hit adopting that pin (the
document of record is readable by a guest only under the data root) and
entries 26–27 were hit building the settings seam on it, and entry 28
closing its round-1 consistency blocker. Entries 29–30 were hit building
the engines seam on the `3fd7b053` pin (phase 2.3): both are shaped,
reproducible packet cards, and 30 is the sharpest entry in this file — a
live composition can permanently lose a contract with no fault, no
refusal, and no log line. Entry 31 was hit adopting the same pin in the
settings seam and is the entry-26 closure's shadow: the non-blocking
`patch-entry` is right, and it turned a concealed dispatch hazard into a
live one; it is **closed** as of the `3a8e5c0` pin bump (jinnd M2-K9).
Entry 32 was hit adopting that pin (phase 2.4) on the path entry 31 used
to cut short: it gives entry 4's nested-dispatch deadlock its first real
transcripts, shows that `Emit` blocks the emitter exactly as serial does,
and adds the half entry 4 never named — whether the fiber that loses the
deadlock ever comes back is incidental, and when it does not, its alarm
writes two ledger rows per period forever.

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

**Closed for `jinn:process` and `jinn:net` 2026-08-29 — retired by pin
1b098be (jinnd M2-K6).** The daemon registers four base providers
(`ServiceProvided` ×4 at every boot: `jinn:fs`, `jinn:clock`,
`jinn:process`, `jinn:net` — soak ledger seq 1473–1476 after the bump),
each behind its declared bundle with the fs grant discipline: a typed
`process-policy` / `net-policy` scope (fail-closed admission, default
deny), every call a ledgered crossing, spawned children and sockets as
kernel registrations released on suspend and dispose. The world is
`jinn:plugin@0.4.0` and carries the bundles' own `process-error` /
`net-error` on the guest wire. Harness side: the operator-API seam is the
first consumer of `jinn:net` (`plugins/api/jinn-api-http` listens on
loopback under a port-scoped grant; refusals for an out-of-range port and
a non-loopback host proven on the record). What remains of this entry:
`jinn:keystore` is still declared and unprovided.

**Fully closed 2026-08-29 — retired by pin 3fd7b05 (jinnd M2-K8).** The
daemon registers the fourth and last base provider: `jinn:keystore@0.1.0`
(`get`, `put`, `delete`, `list`) behind the same broker choke point, under
a `key-prefix` scope that admits NO key on a bare grant and an `ops`
attenuation, with values sealed at rest under a master key that is never
under the data root and a ledger record that carries the key NAME and the
value's digest, never the value. Harness side: the engines seam is its
first consumer — a run request carries `{"$secret": "<key>"}` references
and each provider resolves them through `jinn:keystore` `get` at spawn
time (`plugins/engines/jinn-engine/src/lib.rs` documents the shape;
`jinn-engine-claude`, `jinn-engine-codex` and `jinn-engine-echo` do the
resolving), under the grant `tools/engine-kit/src/lib.rs` writes:
`{ contract: "jinn:keystore", scope: ["engines/"], ops: ["get"] }` — a
prefix, read-only, so a provider reads secret values and can never write,
delete, or enumerate them. Nothing in this repo, its profiles, or its
ledgers holds secret material; only key names. One operational note the
suite had to absorb: a macOS daemon with no passphrase configured falls to
the platform keychain, whose ACL can put an OS prompt in front of the
first mutation, so every daemon the composition suite boots sets
`JINND_KEYSTORE_PASSPHRASE` (`tests/composition/src/kit.rs`) — the
kernel's own packet record calls the keychain backend compiled-but-untested
and names the passphrase as the headless choice, so this is adoption, not
a friction.

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


## 19. No introspection contract — a guest cannot see the composition it is part of

The daemon knows every entry's fiber id and state (`status` on stdin logs
`entry entry="…" fiber=N state=Some(Active)` to stderr), which services
each fiber provides, which listeners, alarms and sockets it holds, and
whether the boot reconcile is done (the readiness line). None of that is
reachable by a guest: the only granted surfaces are the four base
providers and sibling contracts. An operator-facing status plugin
therefore cannot answer `fiber-state`, `fiber-uid`, `provisions`,
`listeners`, `alarms` or `readiness` honestly — and the brief forbids
guessing.

**Harness-side handling shipped:** `jinn-status` answers `status` from
what a guest CAN reach — the profile document of record through its
scoped `jinn:fs` (the entries with their authority fields) and provider
PROBES through granted contracts (a `resolve` + one read call; for cron,
the job table with `next-ms`) — and names each unanswerable field in
`kernel.unavailable` with this entry's number. Two smaller observations
belong here: the ledger's `entry` column is empty at this pin
(attribution is fiber-only; the composition suite maps fiber → entry via
`ServiceProvided` rows), and a refusal's reason text (`denied("… not
loopback …")`) reaches the guest typed but appears neither on the ledger
(`GrantRefused { contract }` only) nor on the log.

**Packet-card shape:** a read-only introspection contract (e.g.
`jinn:introspect`), granted like any other and ledgered per read:
`entries` → `{ id, fiber, state, incarnation, provisions, registrations
(listeners, alarms, sockets) }`, `readiness`; plus fill the ledger's
`entry` column and put the refusal reason on the `GrantRefused` record.
Entry 12's "honest fix" (a machine-readable status surface) is this card.

*Evidence grade:* source-confirmed (`jinnd-daemon` `daemon.rs` registers
fs/clock/process/net only; `watch.rs` `log_status` is stderr-only);
composition `status_health_and_ledger_tail_answer_through_the_api` pins
the shape of the honest answer.

**Closed 2026-08-29 — retired by pin 57360cc (jinnd M2-K7).** The kernel
provides `jinn:introspect@0.1.0` (`entries` → `{ id, fiber, state,
incarnation, provisions, registrations { listeners, alarms, sockets,
processes } }`, `readiness` → `{ boot-reconciled, watcher-armed }`),
granted like any contract, ledgered per read, answered from a snapshot
under brief kernel locks (R1). The two riders landed too: the ledger's
`entry` column is filled for every attributable event and `GrantRefused`
carries the typed `reason` (not-granted / scope-mismatch / not-loopback
/ unresolvable / foreign-handle) with its `detail`. Harness side:
`jinn-status` lays the kernel's view over each document entry
(additive `fiber`, `state`, `incarnation`, `provisions`, `registrations`
siblings), answers `readiness`, and `kernel.unavailable` is EMPTY —
`UNAVAILABLE_STATUS_FIELDS` is `&[]`, the list vocabulary (`finding:
19`) stays on the wire for 0.1.0 readers. `health` now keys `ok` on the
kernel's word (every entry `active`), not on a guess. The composition
suite attributes every kernel read to the status ENTRY and every net
refusal to the provider ENTRY with its reason.

## 20. `jinn:ledger` is declared, not provided — the ledger is readable only beside the daemon

The contract bundle `kernel-pin/contracts/jinn-ledger` (a `read-range`
reader with consumption receipts, constitution 02) has no live provider:
the daemon registers none, so a guest cannot page the ledger or learn the
last sequence number. The operator API's `ledger-tail` and
`last-ledger-seq` are unanswerable from inside; every reader of the
ledger today is a process beside the daemon opening the SQLite file.

**Harness-side handling shipped:** `ledger-tail` honors the paged request
shape (`after`, `limit` clamped to 1..=500) and answers an empty page with
a TYPED `unavailable { finding: 20 }` — never a guess, never a hang — and
the request is still a ledgered contract call, so the operator's read
intent is on the record.

**Packet-card shape:** register the `jinn:ledger` reader behind its
bundle (`read-range(from-id, limit)`), consumption receipts per 02 and
receipts excluded from the reader's own feed, payloads redacted by
sensitivity class; a `last-seq` read beside it.

*Evidence grade:* source-confirmed (daemon assembly); composition
`status_health_and_ledger_tail_answer_through_the_api` pins the typed
answer.

**Closed 2026-08-29 — retired by pin 57360cc (jinnd M2-K7).** The daemon
registers the `jinn:ledger@0.1.0` reader behind its bundle:
`read-range(from-id, limit)` (clamped 1..=500) answers a JSON page of
typed events (`id`, `wall-ms`, `entry`, `fiber`, `kind`, `payload` as
JSON text, `sensitivity`) with `next-from`; `last-seq` answers the
high-water mark; every read appends a `LedgerConsumed { first, last,
count }` receipt under the reader's attribution, and the reader's own
receipts are excluded from its feed. Harness side: `ledger-tail` serves
a REAL page (`after`/`limit` → `read-range(after + 1, limit)`,
`next-after` when a further page may exist), `status` carries
`last-ledger-seq`; the composition suite reads a 3-event page at
`after=7` and finds the receipt under `jinn-status`.

## 21. An operator edit made through `jinn:fs` is a revertible effect of the editing fiber — disposing the editor rolls the edit back

The edit lane (entry 9's operator surface) writes the profile document
through the editor guest's granted `jinn:fs` `write`. That write is a
revertible effect: the kernel keeps the PRE-PATCH document as its inverse,
retains it across the editor's incarnations (a clean stop suspends the
editor with `FiberSuspended { retained: 1 }`), and withdraws it LIFO when
the editor ENTRY is disposed. Reproduced on the real daemon (composition
`disposing_the_editor_reverts_the_operators_edit_finding_21`): PATCH one
entry's config through the API → the scheduler restarts on the new config
→ the operator removes `jinn-profile-edit` from the profile → the daemon
disposes it, `EffectWithdrawn "fs write profile.json"` lands, the
pre-patch document (editor entry INCLUDED) is restored on disk, the
watcher reconciles THAT: the editor is re-created and the scheduler
restarts on its OLD config. Retiring a plugin silently rolled back
configuration and resurrected the plugin. The class of state is entry 14's
(a record, not a contribution), met at the operator lane; it also forces
the layout coupling that the profile must sit under the data root
(`profiles/operator-api/README.md`).

**Packet-card shape (alternatives):** (i) a kernel-provided profile
contract — `jinn:profile` `patch-entry(id, merge-patch)` applied by the
loader itself, write-back included, ledgered as an operator action with
NO fs inverse (the profile's history is the ledger's, not a fiber's
journal); or (ii) a record-class grant scope on `jinn:fs` whose mutations
are ledgered but not journaled for withdrawal. (i) also removes the
data-root coupling and the torn-write window of entry 22. Severity: high
for the operator lane.

*Evidence grade:* reproducible — pinned by a passing composition
transcript (it goes red when the kernel retires this) and by the
`retained: 1` suspension record of the editor on every clean stop.

**Closed 2026-08-29 — retired by pin 57360cc (jinnd M2-K7, shape (i)).**
`jinn:profile@0.1.0` `patch-entry(id, merge-patch)` is applied BY THE
LOADER: validated against the profile schema (an object whose grants
would admit at activation), written back atomically (stage + fsync +
rename — the profile case of entry 22 closes with it), the patched fiber
restarted exactly (same fiber, new incarnation, `cause: ConfigChanged`),
and recorded as `ProfilePatched { entry, by }` with NO fs inverse and NO
fiber journal entry; refusals answer `refused(reason)` on the wire and
land as `AmendmentRefused` (or `GrantRefused { ScopeMismatch }` for an
entry outside the `entry-ids` scope); an entry cannot patch itself (the
nested-dispatch class). Harness side: `jinn-profile-edit` applies the
entry-patch law locally (unknown id / no-op answered without a call) and
hands the kernel `{ data?, grants? }` as one merge patch; its `jinn:fs`
grant now only READS the document. The pinned transcript went red on
adoption and is replaced by
`disposing_the_editor_leaves_the_operators_edit_in_place_finding_21_closed`:
the editor is removed, its own trail is withdrawn, the document keeps
the patch, the scheduler never restarts on an old config, and the editor
is never resurrected. The `idempotency-key` request field stays on the
wire, accepted and unused.

## 22. `jinn:fs` `write` is in place — a concurrent reader can see a torn document

`HostFs` `write` is `tokio::fs::write(file, data)` (truncate, then write;
`hostfs/ops.rs`). The provider's own retention store writes staged +
fsync + rename ("durable means durable", `retention.rs`) — the discipline
exists and is not applied to the data-plane write. For an ordinary data
file the window is the guest's own; for the profile document it is the
daemon's watcher's and any operator's `cat`: a delivery can read a
truncated or partial document. Not observed torn in the composition runs
(the document is a few KiB and the write lands in one syscall), so the
severity is low today and rises with document size.

**Packet-card shape:** stage + rename for `write` (the retention store's
helper, reused), or a declared `replace` operation with that shape.
Entry 21's shape (i) removes the profile case entirely.

*Evidence grade:* source-confirmed; no torn transcript.

**Profile case closed 2026-08-29 by pin 57360cc** (entry 21 shape (i):
the loader's write-back is stage + fsync + rename). The general case
(`jinn:fs` `write` for data files) stays open.

**Fully closed 2026-08-29 — retired by pin 3fd7b05 (jinnd M2-K8).** The
card's shape shipped for the data plane too: `jinn:fs` `write` AND
`append` now commit whole by stage + fsync + rename
(`contracts/jinn-fs/metadata.toml`, `commit = "stage-fsync-rename"` on
both operations; the 0.2.0 "O(1) per record" note on `append` is retired
in the bundle, an honest trade of per-record cost for a tail that is never
torn). A concurrent reader observes the prior document or the new one,
never a prefix. Harness side, every guest-kept document is now atomic
without a guard of its own: the cron scheduler's `state.json` and the
health job's report (`plugins/cron/`), the settings store's overlays, and
— the reason this closure mattered for this packet — the engine probe's
`last.json` and its `history.log`
(`plugins/engines/jinn-engine-probe/src/lib.rs`), which the composition
suite and an operator read WHILE the daemon is running. The suite reads
those files mid-run with no retry-on-parse-failure, which is only sound
because of this commit shape.

## 23. Sockets have no readiness wake — a server polls at the clock floor, on the record

`jinn:net` v0.1 is non-blocking by design (R1): `accept` and `read` answer
`would-block` and the bundle says "the guest polls, typically from a
`jinn:clock` alarm". The HTTP provider does exactly that at the granted
floor (250 ms). Two costs, measured on the composition ledgers: latency —
a request waits up to one poll before its accept and one more per read
round, so an idle-API request costs 250–500 ms; and ledger growth — every
poll is 2 rows (`AlarmWake` + the `accept` `ContractCall`) whether or not
anything is pending, ≈ 8 rows/s, ≈ 690 000 rows/day for an IDLE operator
API (the cron seam's whole duty cycle is ≈ 100 rows/day at its 15-minute
cadence). A production-shaped API cannot carry that.

**Packet-card shape:** a readiness delivery for kernel-registered sockets
— `lifecycle.handle-event(token, "jinn:net/readable", handle)` when a
listener has a pending connection or a connection has bytes/EOF — so a
server holds no alarm at all; or, smaller, a bounded blocking `accept`
with a timeout (the `process` bundle's `wait` shape, capped at 1000 ms)
so a poll costs one row per second instead of eight. Either keeps R1.

*Evidence grade:* measured per poll on the composition ledgers (2 rows per
wake, attributed to the provider's fiber); the daily figure is arithmetic
at the 250 ms floor.

**Closed 2026-08-29 — retired by pin 57360cc (jinnd M2-K7, the first
shape).** The kernel delivers `lifecycle.handle-event(handle,
"jinn:net/readable", <8-byte LE handle>)` when a listener has a pending
connection or a connection has bytes or EOF — one wake per readiness
transition the guest has not acted on (level-triggered, coalesced; EOF
wakes once; `accept`/`read` consume then re-arm), each a `NetReadable`
ledger event; the bundle stays non-blocking and additive (world
`0.4.0` unchanged). Harness side: `jinn-api-http` holds NO alarm and no
`jinn:clock` grant — the listener's wake accepts (and serves each fresh
connection once, its bytes may already be pending), a connection's wake
reads, answers, closes; the idle-poll close is replaced by a bound on
open connections. The composition suite asserts zero `AlarmWake` rows on
the provider's fiber and one `NetReadable` per readiness transition.
The soak measurement (SOAK.md §Pin bump mid-soak, sixth bump) is the
idle-growth evidence: the API mounted in the soak, a 971 s idle window
added ZERO rows attributed to the api entries (the +22 rows in it were
the cron duty's one wake).

## 24. A `jinn:fs` grant cannot be attenuated to read-only

The `path-prefix` scope is a containment path (`grants.rs`
`Declared::PathPrefix` ↔ `ScopeValue::Path`); there is no operation-class
attenuation. `jinn-status` only ever READS the profile document, but the
grant that lets it read is the same grant that would let it write —
authority wider than use, by construction. Every read-only consumer of a
document (status, health, any viewer) carries the same excess.

**Packet-card shape:** an operation class in the scope — `{ path, ops:
["read"] }` (effect classes are already declared per operation in each
bundle's `metadata.toml`) — with the subset predicate extended to it.
Low severity, structural.

*Evidence grade:* source-confirmed; no transcript (no misuse to record).

**Closed 2026-08-29 — retired by pin 3fd7b05 (jinnd M2-K8).** A grant may
now carry `ops` beside `contract`/`scope` — an operation-class attenuation
validated fail-closed against the vocabulary each bundle declares
(`grants/ops.rs` `declared_ops`, mirroring every `[operations.*]`) and
enforced per call at the broker (`broker/authority.rs` `check_op`: an
operation outside the class is a ledgered `GrantRefused` naming it, never
a default-accept). Authority is now exactly as wide as its use here:
`jinn-status` reads the document of record under `ops = ["entry",
"document"]` and CANNOT patch it, while `jinn-profile-edit` holds all
three — both written in `tools/api-kit/src/main.rs` (`api_entries`) from
`jinn_api::KERNEL_PROFILE_READ_OPS` / `KERNEL_PROFILE_EDIT_OPS`
(`plugins/api/jinn-api/src/kernel.rs`). Proven on the real daemon by
`a_read_only_profile_grant_holds_no_patch_authority_finding_24_closed`
(`tests/composition/tests/api.rs`): the shipped classes read back from the
document of record, and the editor narrowed to the viewer's read-only
class has its `patch-entry` refused on the ledger while its `document`
read keeps working.

## 25. The document of record is reachable by a guest only under the data root — `jinn:introspect` carries no authority fields, `jinn:profile` has no read

Hit adopting `57360cc`. Entry 21's closure moved the WRITE side of the
operator lane onto `jinn:profile`, so nothing about a patch needs the
document inside the `jinn:fs` surface any more — but the READ side
still does: `status` shows each entry's authority fields (`package`,
`hash`, `grants`) and `get` answers the document verbatim, and the only
surface a guest can read a file through is its scoped `jinn:fs`, which
resolves under the daemon's data root. `jinn:introspect` `entries`
carries the kernel's runtime view (fiber, state, incarnation,
provisions, registrations) and none of the document's authority fields;
`jinn:profile` has `patch-entry` and no `get`/`entry` read. So the
operator layout's coupling (`profiles/operator-api/README.md`: the
profile must sit under the data root) survives the closure of 21 for
reads alone, and a composition whose profile sits beside the data root
— the cron soak's layout since day one — cannot show its authority
fields through the API. A guest holding a `jinn:fs` scope on the
document also still holds write authority it never uses (entry 24).

**Harness-side handling shipped:** the read is typed. `jinn-status`
answers `document: { readable: false, unavailable: { finding: 25 } }`
when the document is out of reach and still lists every entry from the
kernel's view (authority fields empty, stated — never guessed);
`jinn-profile-edit`'s `get` answers the same typed `unavailable`. The
soak mounts the api trio with its profile relocated INTO the data root
(`$SOAK/data/profile.json`, `--artifacts`/`--data` passed explicitly;
the watcher is non-recursive so the fibers' subdirectories never wake
it) so the soak's status is complete; the composition suite proves both
answers.

**Packet-card shape:** `jinn:introspect` `entries` gains the document's
authority fields (`package`, `hash`, `grants`), or `jinn:profile` gains
a read (`document()` / `entry(id)`), granted read-only — either retires
the data-root coupling entirely and, with it, the excess write authority
of entry 24 for every viewer.

*Evidence grade:* source-confirmed (`contracts/jinn-introspect`,
`contracts/jinn-profile` at the pin); composition
`the_operator_api_serves_a_profile_beside_the_data_root_finding_25`
pins the typed answer.

**Closed 2026-08-29 — retired by pin 3fd7b05 (jinnd M2-K8).** The card's
second disjunct shipped: `jinn:profile` 0.2.0 gained the reads `entry(id)`
and `document()` (`daemon/profile_read.rs`), answering the document of
record's authority fields — `id`, `package`, `version`, `hash`, `grants`,
`config`, `disabled`, `parent` — for every entry the caller's `entry-ids`
scope admits, each a ledgered call, a read outside the scope a ledgered
grant refusal. The harness reads the document through that contract now,
not through a file: `plugins/api/jinn-status/src/lib.rs`
(`profile_document`, feeding `status`/`health`) and
`plugins/api/jinn-profile-edit/src/lib.rs` (`read_profile`, feeding both
`get` and the local entry-patch law), over the wire in
`plugins/api/jinn-api/src/kernel.rs` (`profile_entry_payload`,
`decode_profile_document`, `decode_profile_entry`, `ProfileEntryRecord`).
Neither consumer holds any `jinn:fs` grant on the document any more
(`tools/api-kit/src/main.rs`), so the data-root coupling is gone from both
the read path and the authority side; the typed `unavailable` answer stays
for the one case left, a viewer mounted without the read grant. Proven on
the real daemon by
`the_operator_api_reads_the_document_beside_the_data_root_finding_25_closed`
(`tests/composition/tests/api.rs`), which asserts in the soak's layout —
the profile BESIDE the data root — exactly the completeness the previous
pin could only answer as unavailable.

## 26. `patch-entry` awaits the patched fiber's restart — an owner that resolves its settings in `activate` cannot be patched by its settings provider

Hit building the settings seam on `57360cc`. The kernel's `jinn:profile`
`patch-entry` answers only after the loader has restarted the patched
fiber (`profile_cap.rs`: `loader.update_entry(..).await`, then
`ProfilePatched`). The kernel guards the direct case (an entry patching
itself is refused: "would await its own restart from inside its own host
call"), but the two-hop case is the seam's normal shape: the settings
provider, inside its `patch` handler, patches the OWNER entry; the owner
restarts; if the owner's `activate` calls `jinn:settings` (`declare`,
`get`) that call lands on a provider instance that is mid-`patch`,
waiting for the restart that is waiting for the call — the
nested-dispatch class of entry 4, held until the guest deadline, and the
owner's activation fails. There is no readiness or injection surface a
guest can declare to order activation (entry 7), and no deferred
amendment shape (the paper's Algorithm 5 stages the desired state and
answers at once — recorded as a post-M1 candidate in the kernel's
decision log 2026-08-25 ruling 1).

**Harness-side handling shipped:** the owner never calls the provider
from `activate`. `cron-scheduler` plans its activation on its entry
layer alone, resolves the settings from a one-shot `alarm-at(now)` one
clock floor later and re-declares on every wake (the `declare` answer
IS the job table; a provider restart or swap heals within one wake), and
absorbs `jinn:settings/changed` from the payload without calling back.
The provider patches the overlay STORE entry synchronously (its fiber
calls nothing in `activate`) and the owner entry synchronously too — the
owner's activation makes no call. The bound this leaves is stated in
entry 27.

**Packet-card shape:** either (a) a non-blocking amendment answer
(`patch-entry` returns `accepted` once the document is committed and the
restart is scheduled — the Algorithm-5 shape the decision log already
names), so a patched owner may resolve anything in `activate`; or (b) a
guest-declarable injection in the profile entry (`requires:
["jinn:settings"]`) so the kernel gates the owner's activation on the
provider's availability (entry 7's card) — (a) removes this deadlock
class, (b) removes the boot-ordering retry. Both keep R1.

*Evidence grade:* source-confirmed (`profile_cap.rs`, the self-patch
refusal names the class); the harness shape is pinned by the settings
composition suite (`declare_resolve_and_patch_on_both_paths_with_the_c5_c6_transcript`:
the restart path lands with the owner making no call from `activate`).

**Closed 2026-08-29 — retired by pin 3fd7b05 (jinnd M2-K8), shape (a).**
`jinn:profile` 0.2.0 answers `accepted(seq)` once the document has
committed and the patched fiber's restart is SCHEDULED; the call never
awaits the restart (`contracts/jinn-profile/metadata.toml`, `settle =
"deferred-restart-scheduled-never-awaited"`). The two-hop deadlock class
is therefore gone: a settings provider may patch the entry that resolves
it from `activate`, and the seam is free of the constraint that shaped it.
Harness side, the new answer is decoded and USED rather than merely
tolerated: `jinn_api::decode_profile_answer`
(`plugins/api/jinn-api/src/kernel.rs`) answers the accepted patch's
`ProfilePatched` ledger sequence, and both callers ride it out to the
operator as `patched-seq` — `plugins/api/jinn-profile-edit/src/lib.rs` on
the API's `patch-entry` answer and
`plugins/settings/jinn-settings-profile/src/lib.rs` on the settings
`patched` answer — so a caller can follow the restart it did not wait for
through `jinn:ledger`, which is the only way a non-blocking amendment is
observable at all. The owner-side shape `cron-scheduler` already had
(resolve on its own alarm wake, absorb `changed` in place) is KEPT, but it
is no longer a workaround: it is what makes a provider restart or a
provider swap heal within one wake, and it is now a resilience property
rather than a deadlock avoidance.

## 27. C5/C6 decision evidence — what a settings patch costs on the restart path and on the hot path, measured on the real daemon

Recorded for SOURCE-OF-TRUTH's open decisions C5 (hot-config acceptance)
and C6 (per-entry config layering / intercept plumbing), from the
settings composition suite
(`declare_resolve_and_patch_on_both_paths_with_the_c5_c6_transcript`,
run root `settings-paths-74740`, pin `57360cc`, suite kit: 2 s job
period, 500 ms tick). Ledger rows are quoted by sequence from that run.

**The kernel's answer to C5 today:** a `jinn:profile` `patch-entry`
ALWAYS restarts the patched fiber (reconcile-by-id; `cause:
ConfigChanged`). There is no kernel path by which a plugin absorbs a
change to its own entry's config in place. Hot-config is therefore
possible only by keeping the changed layer OUTSIDE the owner's entry —
and the only home that keeps the profile the single source of truth is
another entry. The harness built exactly that (the `jinn-settings-store`
entry + the `jinn:settings-store` read; `plugins/settings/README.md`),
which is a guest-side emulation of what C6's intercept chain would give
the kernel natively: a per-entry config LAYER the loader resolves.

**Restart path** (`tick-ms` patch → the owner entry; rows 188–208, 21
rows, answered to the HTTP caller in 28 ms): the provider's
`patch-entry` call (188) → the owner's kernel registrations released and
its fiber suspended with its contribution retained (`listen`, `alarm`,
`ServiceWithdrawn`, `FiberSuspended { retained: 5 }`, 189–192) → the
successor incarnation activates (three `fs` reads of state/history, the
provision, `now`, the activation plan's state write, the on-duty effect,
the new `alarm every 250ms`, the listener and the one-shot settings
alarm, 193–203) → four transitions to `Active` (204–207) →
`ProfilePatched { entry: cron-scheduler, by: jinn-settings-profile }`
(208). Duty gap: the whole suspend → Active lies inside the 28 ms
answer; the schedule RESUMED from the persisted `last` (no
`schedule-started`). State continuity: exact (the retained journal). One
bound: the successor's activation plan runs on the ENTRY layer alone
(the overlay arrives one clock floor later on its settings alarm), so a
job the overlay had removed can fire once on restart and a job the
overlay added waits one floor.

**Hot path** (`jobs` patch → the store entry; rows 116–148, 33 rows,
answered in 56 ms): the HTTP crossing (116) → the provider reads the
overlay (117–118) and patches the STORE through `jinn:profile` (119–120)
→ the store's trivial fiber cycles (`ServiceWithdrawn`, `FiberSuspended
{ retained: 0 }`, re-provision, four transitions, 121–128) →
`ProfilePatched { entry: jinn-settings-store }` (129) → the `changed`
event delivered serially to the scheduler, which re-plans IN PLACE on
the new table: `now`, state write, and — because the halved schedule was
already due — one fire with the consumer's full report chain and the run
record + history append (130–147) → `DispatchTrace { topic:
jinn:settings/changed, listeners: 1, failures: 0 }` (148). The owner's
fiber never transitioned; its alarm, listener, provision and state were
untouched; the store's cycle is the price of keeping the profile the
truth (8 rows). Without the coincident fire the hot path is 15 rows.

**Reading for the decision.** Per patch the two paths cost the same
order of ledger rows and the same order of latency (tens of
milliseconds on this box) — the restart path is not expensive because
suspend ≠ dispose made a restart a state-preserving event (entry 14's
closure). What separates them is CONTINUITY, not cost: the restart path
drops every kernel registration for the duration and re-plans from the
entry layer (the bound above); the hot path keeps them all and applies
exactly the resolved layer. The harness could build hot-config on top of
the kernel with one extra entry and one extra contract, at the price of
(a) two homes for the owner's effective config in the document (its
entry and the store's overlay — a reader of the profile must resolve
them; `jinn:introspect` shows neither), (b) a provider that must never
be called from an owner's `activate` (entry 26), and (c) every owner
re-declaring on every wake because the layer is guest knowledge.
**Recommendation for C6:** a kernel-resolved per-entry layer (the
intercept chain the paper already names) would make the store entry
unnecessary, give the loader one resolved config to hand `activate`,
and let `jinn:introspect` report effective config. **Recommendation for
C5:** accept hot-config as a DECLARED per-key property of a namespace
(the `hot-keys` shape here), never a default — a kernel registration
(an alarm period, a listener topic, a bind port) cannot be changed in
place and must restart.

*Evidence grade:* measured — every row above is on the pinned run's
ledger, the suite re-prints the transcript on every run; the soak
carries the same seam on the 15-minute cadence (SOAK.md, sixth bump).

## 28. `jinn:profile` patches one entry per call — two layers of one namespace cannot be written atomically

Hit closing PLA-314's round-1 blocker. A settings namespace has two
homes in the document (the owner entry's `config.data`, the store
entry's `config.data.overlays[ns]`, `plugins/settings/README.md`). A
mixed hot+cold patch under an existing overlay must either land in both
(the cold keys in the entry, the hot keys in the overlay) or be refused
whole, or its report lies (the round-1 probe: answered `jobs:
[requested]`, resolved `jobs: [overlay]`). `patch-entry` takes ONE entry
id and ONE merge patch and awaits that entry's reconcile before
answering (`kernel-pin/contracts/`, entry 26): landing in both is two
calls with an observable state between them, and a refusal of the second
after the first applied — and restarted the owner — is a partial apply.

**Shape shipped:** refusal. The definition computes a plan's reported
settings from the post-state layers and refuses `invalid` +
`shadowed { key, layer, recovery }` when they differ from the request;
the recovery is the explicit-layer call that clears the shadowing layer,
then a retry — two honest calls, one per entry, which is this finding's
floor (`plugins/settings/jinn-settings/README.md` §The recovery).

**Packet-card shape (only if C6 is ever revisited):** a multi-entry
`patch-entries` — one call, N `(entry, merge)` pairs, applied under one
reconcile or none, one `ProfilePatched` per entry on the ledger. With
C6 decided against kernel-side layering (PLA-314 round-1 steering), the
harness has no present need; logged so the choice of refusal over
apply-both is traceable to the contract and not to taste.

*Evidence grade:* source-confirmed (the `patch-entry` shape in the pinned
contracts); the refusal is pinned by the settings composition suite
(`a_patch_reports_exactly_what_the_next_get_resolves_in_both_orders`).

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

## 29. A contract has one provider slot and no notion of an instance — N engines coexisting means N contract names

Hit building the engines seam on `3fd7b05` (phase 2.3). The broker holds
`providers: contract -> provider`, and `provide` refuses a second peer for
an occupied slot with `DuplicateProvision` — deliberately, "replacement is
never silent" (R9). `services.resolve(contract)` mints a handle against
whoever holds the slot. There is no qualified resolve, no provider
selection, and no per-instance grant. So a capability whose whole point is
that SEVERAL implementations are live at once — engines, and later
connectors, model providers, storage backends — cannot be one contract.

The harness encodes the instance in the NAME: a provider serves
`jinn:engine.<engine-id>`, the id read from its own entry's
`config.data.engine` and written nowhere else
(`plugins/engines/jinn-engine/src/lib.rs`, `engine_contract`). That buys
the malleability properties cleanly — a switch is a `package`/`hash` edit
on one entry, coexistence is a second entry, extension is a third — and it
keeps per-engine authority where the kernel enforces it, since a grant
names a contract and therefore an engine. But the kernel cannot see the
structure: `jinn:engine.codex` is an opaque string to the broker, so
nothing checks that two entries do not claim the same engine id (the slot
refusal catches it, but reports a duplicate PROVISION, not a duplicate
engine), a consumer's `jinn:introspect` view must parse provisions to find
engines, and the whole convention lives in guest code where a typo is a
`missing-dependency` at resolve rather than a profile-load refusal.

**Packet-card shape:** instance-qualified provision and resolve —
`services.provide(contract, instance)` / `resolve(contract, instance)`
with the instance carried in the handle and in the grant scope (a
`instances` scope beside `path-prefix` and `key-prefix`, same fail-closed
admission), so a contract may be provided AT a name. The ledger's
`ServiceProvided` gains the instance; `jinn:introspect` reports it
structurally; the loader can refuse two entries claiming one instance at
LOAD time rather than at first `provide`. This is the same class as entry
27's C6 note: the harness is emulating a kernel concept in guest
convention, and the emulation is what should be retired.

*Evidence grade:* source-confirmed (`crates/jinnd-wasm/src/broker.rs`
`provide`/`resolve` at the pin — the slot map and the `DuplicateProvision`
refusal); the encoding and its three proofs are pinned by the engines
composition suite (`tests/composition/tests/engines.rs`).

## 30. `services.provide` has no staging path — a provision made in `activate` binds the STAGING instance, and one call before the swap commit kills the contract for good

Hit building the engines seam on `3fd7b05` (phase 2.3), and it is the
sharpest entry in this file: a live composition can permanently lose a
contract with no fault, no refusal, and no log line.

`events.listen` knows about staging. Its host surface checks
`self.seat.staging` and RECORDS the registration to be committed at swap
commit "against the new instance's own delivery face", with the comment
"Recorded, not routed (R8)". `broker.provide` has no such path: it takes
`peer` — the STAGING peer, because `activate` runs in staging — checks the
grant, bumps the generation, and inserts that peer into the contract's
single provider slot. When the swap commits, the staging instance is
discarded. The slot still points at it.

Every provider in this repo before now got away with it, because nothing
ever called their contract before the boot reconcile finished. The engines
seam is the first composition with a CONSUMER whose own wake can land
inside the reconcile, and it fails immediately and permanently:

```
38 jinn-engine-default  8  ServiceProvided   jinn:engine.default        <- staging instance
39 jinn-engine-default  8  EffectRegistered  "jinn-engine-echo on duty"
44 jinn-engine-probe   11  EffectRegistered  "alarm at <now>"
51 jinn-engine-probe   11  AlarmWake         alarm 1                    <- inside the reconcile
52 jinn-engine-probe   11  ContractResolved  jinn:engine.default
53 jinn-engine-probe   11  ContractCall      jinn:engine.default/run    <- served by staging
54 jinn-engine-default  8  ContractCall      jinn:clock/now
65 jinn-engine-default  8  FiberTransition   Pending -> Loading  (InitialLoad)
66 jinn-engine-default  8  FiberTransition   Loading -> Active   (InitialLoad)
```

From seq 66 on, every call on that contract answers
`KernelError::ProviderFailed("the instance is gone")` — the operator API's
`describe` at 108 and its `run` at 214 both do, while
`jinn:engine.claude` and `jinn:engine.codex`, which nobody called during
the reconcile, answer normally from the same boot. The entry reports
`state=Active`, the reconcile reports `faults=[]`, and the daemon log says
nothing. The contract is dead until the entry restarts (patching it back
to life is what made the seam's cancel proof pass while its run proof did
not).

Two things make this worse than an ordering nuisance. It is SILENT — Law 2
records the provision and the calls, and nothing records the loss. And it
is not recoverable by the consumer: a retry resolves the same dead slot,
so a probe that "records a missing provider and moves on" never heals.

**Packet-card shape:** give `provide` the staging path `listen` already
has — record the provision on the staging outcome and commit it at swap
against the committed instance's face, as
`crate::handle::Registration::Listen` is committed today. Two smaller
hardenings are worth the same card: a call that lands on a staging
provider should answer a typed retryable refusal rather than be served by
an instance about to be discarded (R9: a refusal is better than a silent
wrong answer); and `ProviderFailed("the instance is gone")` should be a
ledger event, not only a wire error, so a composition that loses a
contract says so.

**Harness-side handling shipped:** `jinn-engine-probe` no longer arms a
one-shot at `now` — its first wake is one period out
(`plugins/engines/jinn-engine-probe/src/lib.rs`), which is what a schedule
means anyway and which keeps the first call out of the boot reconcile.
That narrows the window; it does not close it, because nothing a guest can
declare orders its activation against another entry's swap (entry 7) — on
a loaded host a slow reconcile can still overlap a period.

*Evidence grade:* packet-card-ready. Transcript above is run root
`eng2` at pin `3fd7b05` (ledger seq 38-66, 107-108, 213-214), reproduced
from the engines composition suite's `engines-run` root; source-confirmed
in `crates/jinnd-wasm/src/surfaces.rs` (`listen` stages, `provide` does
not) and `crates/jinnd-wasm/src/broker.rs` (`provide` inserts `peer` into
the slot directly).


## 31. A serial dispatch to a fiber with a pending restart stalls to the guest deadline — no refusal, no diagnostic, no row

Hit adopting `3fd7b05` (jinnd M2-K8) in the settings seam, which the
engines packet inherited. Entry 26's closure made `patch-entry`
non-blocking: the call schedules the patched fiber's restart and returns
at once, which is the right shape and what that entry asked for. But it
moves a hazard from hidden to live. A guest that patches an entry and
then makes a `DispatchMode::Serial` call to that same entry's fiber is
now aiming at an incarnation the loader is in the middle of replacing.
The dispatch does not fail and does not queue: it waits for a peer that
will never answer, until the caller's guest deadline expires. The
blocking `patch-entry` used to conceal this by finishing the restart
before it returned.

Nothing in the system says so. There is no `Restarting` refusal, no
ledger row naming the stall, and no way to ask whether a fiber has a
restart pending — `jinn:introspect` reports the composition, not the
loader's in-flight work. The operator sees only an HTTP request that
never answers, and the plugin author sees a call that works on one code
path and hangs on the other with no signal distinguishing them.

**Workaround attempted, and INSUFFICIENT.** The settings provider chooses
its dispatch mode from the layer it just patched — `Serial` on the hot
path (the owner absorbs the overlay in place, so the answer legitimately
waits for it), `Emit` on the restart path (the owner re-declares on its
own wake, so the notice needs no reply). That change is correct on its
own merits and is kept: a notice aimed at an incarnation being replaced
has no one to answer it, and the successor re-declares regardless. It
does NOT close the gap. A round-2 report claimed it made
`the_shadowed_refusals_recovery_lands_when_executed` pass; that claim was
false, and the test fails at the same head both under `cargo test
--workspace` and in isolation. Correcting that claim is the point of this
re-grading.

Two reasons the workaround cannot be the fix. It is only available to a
provider that KNOWS which layer it wrote — a consumer patching an entry
it does not own has no such knowledge and no way to acquire it. And it
only moves the guest's own notice off the restart path; it does nothing
about the operator's call chain, which still contains a serial dispatch
that lands on a fiber the loader is replacing. The stall is a property of
the kernel's dispatch, not of who sends the notice.

**Evidence (final, at pin `3fd7b05`).** `tests/composition/tests/settings.rs`
`the_shadowed_refusals_recovery_lands_when_executed`: the recovery the
shadowed refusal names is an ENTRY-layer patch, so it takes the restart
path, and its `PATCH /v1/settings/cron` never answers — the request dies
on the suite's 45 s bound (`tests/composition/src/api.rs:68`,
`WouldBlock`). The ledger trace is seq **224** `patch-entry`, **228**
`ProfilePatched`, **230** `guest exceeded its call deadline`: the profile
amendment lands, the restart is scheduled, and the call that follows it
waits out the deadline. Reproduced independently twice by the verifier at
head `7557533` — once under `cargo test --workspace` (`FAILED. 4 passed;
1 failed`) and once isolated (`FAILED. 0 passed; 1 failed`).

**Status: BLOCKED on the kernel, tracked.** The test is marked
`#[ignore]` with a reason string naming this entry and its kernel packet
— not deleted, not skipped silently, and with no assertion weakened. The
kernel fix is **jinnd M2-K9** (`d95cffd`, tracked as **PLA-318**): a
serial dispatch to a fiber with a pending restart REFUSES, typed and
ledgered, with the pending-restart state readable through
`jinn:introspect`. PLA-318's acceptance removes the `#[ignore]` and
closes this entry.

**Packet-card shape:** make the restart visible or make the dispatch
survive it. Either (a) a serial dispatch to a fiber with a pending
restart is QUEUED and delivered to the successor incarnation — the
composability story the paper argues for, and the least surprising
behaviour; or (b) it is refused typed (`Restarting`), so the caller can
choose to retry, drop, or downgrade to an emit; or at minimum (c)
`jinn:introspect` exposes pending-restart state so a guest can pick its
dispatch mode from the kernel's knowledge instead of reconstructing it
from its own. (a) and (b) are contract changes; (c) is additive. M2-K9
takes (b) with (c) beside it.

*Evidence grade:* packet-card-ready, and AUTHORED — the card exists as
jinnd M2-K9 / PLA-318. Reproducible at pin `3fd7b05` on the real daemon
with the exact call, the exact deadline and the exact ledger sequence;
independently reproduced by the verifier. The harness has no remaining
move here: this entry stays open, with its proof ignored and named, until
the kernel packet lands.

**Closed 2026-08-30 — retired by pin `3a8e5c0` (jinnd M2-K9).** A
reply-expecting dispatch aimed at a fiber that already owes a change is
now REFUSED, typed (`restarting` / `gone` / `suspended` / `stalled`,
each with a `refused-target`) and ledgered, before any listener runs; and
`jinn:introspect` 0.2.0 answers the same state as `entry.unserved` from
the same snapshot, so ASKING and BEING REFUSED cannot disagree. The stall
this entry describes is gone on the exact path that produced it:
`the_shadowed_refusals_recovery_lands_when_executed` at pin `3a8e5c0`
runs the ENTRY-layer patch that used to die on the 45 s request bound and
it now ANSWERS — ledger seq **223** `patch-entry`, **224**
`ProfilePatched`, **236/237** the operator's response written and the
socket closed, **244–247** the scheduler's restart, **250** the
successor's `declare` (`target/composition/runs/settings-recovery-26570`,
this round's transcript). At pin `3fd7b05` that same call never reached
236.

The harness workaround this entry called insufficient — the settings
provider choosing its dispatch mode from the layer it just wrote — is
KEPT, and no longer as a workaround: `Emit` on the restart path is the
semantically right notice (the successor re-declares on its own wake and
has nothing to answer with), and it now costs nothing to be wrong about,
because a `Serial` there would be refused typed rather than hang. It is
no longer load-bearing. It does not make the notice SAFE, either — entry
32 shows an `Emit` deadlocking against an owner that is merely busy — but
that is a different gap, and no dispatch mode avoids it.

The test does NOT pass at this pin, and it is honest to say why: it now
gets FURTHER and dies later, on a different kernel gap — the
nested-dispatch deadlock of entry 4, which entry 31's stall used to mask
by killing the test before it could be reached. That is entry **32**, and
the `#[ignore]` on this test now names 32 rather than this entry.

## 32. A settings owner's `declare` and the provider's `changed` notice deadlock each other — and whether the loser comes back is luck

Hit adopting `3a8e5c0` (jinnd M2-K9), on the path entry 31's stall used
to cut short. It is entry **4**'s class — nested dispatch — but entry 4
was graded *structural, not yet a runnable transcript, not independently
reproduced*. It is reproducible now, twice, on the real daemon, with
ledger sequences; and the two transcripts together say two things entry 4
does not.

**The deadlock.** `jinn-settings-profile` serves the operator's `PATCH`
and, having applied it, emits `jinn:settings/changed` to the namespace's
owner. The owner (`cron-scheduler`) re-declares its namespace on EVERY
alarm wake — the healing mechanism entry 26 records the reason for —
which is a `services.call` into that same provider instance. When the two
overlap, the owner is parked on the provider's busy supervisor channel
and the provider is parked awaiting delivery into the owner. Both die on
the 5 s guest deadline, and so does the operator's request behind them.

This is not a rare interleaving: the suite kit wakes the scheduler every
250 ms and it declares on every wake, so any patch is a coin flip against
it. The two transcripts caught it at different patches — one on the
restart path, one on the hot path — which is the point: the collision is
with the owner's cadence, not with a layer.

**`Emit` does not help, and one transcript proves it.** The provider
sends the restart-path notice `DispatchMode::Emit` precisely because the
successor re-declares on its own wake and owes no answer. Run `36182`
deadlocked on exactly that emit. The kernel awaits every listener
delivery end-to-end in every mode — `jinnd-events/src/dispatch.rs:43`
and `jinnd-wasm/src/topics.rs:245` both `await target.deliver(...)` —
so fire-and-forget discards the ANSWER, never the WAIT. M2-K9's refusal
does not apply either: the owner owes no transition, it is merely BUSY.

**Whether the deadlocked fiber recovers is incidental.** In run `36182`
the patch was aimed at the owner's own entry, so the loader already owed
it a restart; the deadline killed the instance and the pending restart
rebuilt it (seq **230** `EffectWithdrawn`, and on to a live successor).
In run `26570` the patch was aimed at the STORE entry and the owner was
collateral — nothing owed it anything, and nothing came for it:

```
311 cron-scheduler {"ErrorRecorded":{"error":{"code":"PluginFailed","message":"guest exceeded its call deadline"}}}
312 cron-scheduler {"AlarmWake":{"alarm":5}}
313 cron-scheduler {"ErrorRecorded":{"error":{"code":"PluginFailed","message":"the instance is gone"}}}
314 cron-scheduler {"AlarmWake":{"alarm":5}}
315 cron-scheduler {"ErrorRecorded":{"error":{"code":"PluginFailed","message":"the instance is gone"}}}
…  the pair repeats every 250 ms to the end of the transcript (seq 627+)
```

No `FiberTransition` to `Failed`, no teardown, no restart, no bound: the
armed alarm outlives the instance it wakes and converts itself into two
ledger rows per period for as long as the daemon lives. R11 says a
failing plugin deactivates itself and its dependents cleanly; this one
deactivates nothing and writes forever. The operator sees a scheduler
that has silently stopped scheduling and a ledger growing at 8 rows per
second. That a config restart happened to rescue the other run is luck,
not a mechanism.

**Evidence.** `tests/composition/tests/settings.rs`
`the_shadowed_refusals_recovery_lands_when_executed` at pin `3a8e5c0`,
twice, in separate daemons.

- `target/composition/runs/settings-recovery-36182` (restart path, the
  test's FIRST patch): **219** `ContractCall jinn:settings patch`, **223**
  `patch-entry`, **224** `AlarmWake` on `cron-scheduler`, **226** its
  `ContractCall jinn:settings declare` into the busy provider, **227**
  `ProfilePatched cron-scheduler`, **228** `jinn-api-http … guest
  exceeded its call deadline`, **229** the same on `cron-scheduler`,
  **230** the pending restart's `EffectWithdrawn`.
- `target/composition/runs/settings-recovery-26570` (hot path, the test's
  overlay patch, and the run that does not recover): **292** `patch`,
  **297** `AlarmWake`, **298–299** `declare`, **300** `ProfilePatched
  jinn-settings-store`, **309** and **311** the two deadlines, and from
  **313** the unbounded `instance is gone` loop.

In both, the operator's `PATCH /v1/settings/cron` dies on the suite's
45 s bound (`tests/composition/src/api.rs:68`, `WouldBlock`).

**What the harness can do: nothing honest.** The provider cannot know
whether a listener is currently calling it, and the guest world has no
deferral primitive to move the notice off the call. Slowing the owner's
declare cadence shrinks the window without closing it, and buys the
shrink by making a provider swap heal slower — trading one correctness
gap for another. The seam is boxed in.

**Packet-card shape.** Two pieces, and the second is the urgent one.
(a) Reentrancy: a dispatch whose target instance is BUSY SERVING THE
EMITTER is a deadlock the kernel can see at the one dispatch point it
already owns — refuse it typed (`busy`, beside M2-K9's four) so the
emitter can drop the notice, or queue it for delivery once the current
call returns. Entry 4's card asked for this; it now has its transcripts.
(b) Recovery: a fiber whose instance died on a deadline must reach a
terminal transition and release its kernel registrations, or be
restarted. An armed alarm outliving its instance is an unbounded
ledger-write loop with no fault recorded anywhere — the same shape as
entry 30, a live composition losing something for good with no fault, no
refusal, and no line saying so.

*Evidence grade:* packet-card-ready. Reproduced twice at pin `3a8e5c0` on
the real daemon, in separate daemons and at different points of the same
test, with exact calls, deadlines and ledger sequences; the kernel-side
mechanism is named in source (`jinnd-events/src/dispatch.rs`,
`jinnd-wasm/src/topics.rs`). The test that produces it stays in the
suite, `#[ignore]`d and naming this entry — nothing in its body is
relaxed.

---

## 33. An append-only log over `jinn:fs` grows the fiber's effect journal without bound — one entry per line, for the life of the incarnation

**Where the harness hit it.** The sessions seam's durable store
(`plugins/sessions/jinn-session-fs`) keeps one append-only JSONL journal
per session and writes one line per event: `created`, `turn-started`,
`turn-ended`, `closed`. Every one of those is a `jinn:fs` `append`.

**The contract's own words.** `kernel-pin/wit/plugin.wit`, `fs.append`:

> Effect class: revertible — inverse = truncate to the prior length, or
> restore prior absence (the shape for guest-kept logs). Keyed and
> journaled as `write`. […] The registered effect joins the calling
> fiber's journal: teardown withdraws it LIFO with the rest of the
> fiber's contribution (R5, I1).

So the append is designed for exactly this shape ("the shape for
guest-kept logs") and each one leaves a journal entry that lives until
the fiber's teardown. A store driving a busy session writes four lines
per turn; a long-lived instance therefore accumulates effect-journal
entries in proportion to TOTAL TURNS SERVED, not to anything bounded —
and the whole point of a durable store is that its incarnation is
long-lived.

**Why this is the kernel's question and not the harness's.** Nothing the
guest can do changes it. Batching lines into fewer, larger appends trades
the growth rate for a worse tear window and a coarser crash boundary —
the ordering that makes restart honesty work (`turn-started` on disk
before any engine is asked for anything) requires the append to happen at
that exact point. Writing whole documents with `write` instead of
`append` is worse on every axis: it registers an effect too, and it
rewrites the entire log per line.

**What a card would decide.** Whether an append to a path a fiber has
already appended to should COALESCE with its existing journal entry (one
truncate-to-prior-length inverse per (fiber, path) is sufficient — the
first one's prior length is the only one an unwind needs), or whether the
`log` effect class should be journaled differently from `write` at all.
Coalescing looks correct and cheap: LIFO unwind of N appends to one path
truncates to N different lengths, and only the earliest matters.

**What the harness does meanwhile.** Nothing — no workaround, no side
door. The sessions suite's journals are short-lived and the growth is
invisible at that scale, which is precisely why this is filed on the
CONTRACT rather than on an observation.

*Evidence grade:* **derived, not measured.** This is read off the pinned
contract's own text and the kernel's stated journaling rule, and it is
consistent with what `FINDINGS.md` #22 recorded about the commit path.
The harness has NOT measured a journal growing, has not observed a
failure, and does not claim a threshold — the sessions suite's sessions
are too short to produce one. Read as a contract question worth a card,
not as a reproduced defect. A card should start by measuring it.

---

## What the TODOS seam could NOT prove, and says so

Recorded here because the honest limit of a proof belongs beside the
frictions, not buried in a test:

- **A vendor engine under a Todo runs only where an operator asks for
  one by name.** `tests/composition/tests/todos.rs::the_same_dispatch_runs_over_a_vendor_engine_when_the_operator_names_one`
  binds a real vendor CLI as the second leg of the three-layer proof,
  changing the engine field and nothing else. It is gated on
  `JINN_HARNESS_TODO_VENDOR_ENGINE` because it spends metered inference
  under the operator's own authentication, so it self-skips in CI (and
  says so loudly) exactly as the pinned-daemon gate does without a jinnd
  checkout. A SKIP proves nothing and is never summarized as a pass; an
  engine that is named and not mounted FAILS rather than skipping. The
  echo and child-backed legs remain, and on their own they prove a
  binding swap between two in-repo providers — not that the stack
  survives contact with a real CLI.
- **The torn tail is manufactured, not observed.** `jinn:fs`'s append is
  whole-document atomic (#22), so the suite writes a short document
  behind the daemon's back to produce one. What is proven is the
  READER's behaviour on a torn document, not that the kernel tears.
  See #34.
- **Concurrency between two writers to one Todo is not proven.** The
  proofs drive one caller at a time. The registry is behind a mutex and
  the journal's append is atomic, so a lost update is not reachable
  through the ops as written — but "not reachable by inspection" is not
  a proof, and no test races two `update`s on one Todo.
- **The event ring's drop count is proven at unit level only.** The
  composition proofs read the feed but never overflow it: a Todo with
  more than `EVENT_RING` events would take longer than a test's
  deadline to produce through the API.
- **`declared-status` and `status` agree in every state the composition
  reaches after the recovery lands.** The one state where they differ —
  a Todo adopted but not yet recovered — exists only inside
  `adopt_all`, and is proven at unit level
  (`the_recovery_is_a_new_event_and_makes_the_ledger_usable_again`),
  not through the daemon.

## What the sessions seam could NOT prove, and says so

Recorded here because the honest limit of a proof belongs beside the
frictions, not buried in a test:

- **A vendor engine under a session was not driven in this round.** The
  "same spec over another engine" proof runs over two genuinely different
  engine PROVIDER SHAPES that exist on every host — the answering echo
  provider and the same package in its child-spawning shape, which drives
  a real process through `jinn:process`. A metered vendor CLI would prove
  the same layering and is absent exactly where the proof must run (CI,
  and an independent verification that declines to spend inference). The
  binding is the same one field either way.
- **The event feed is polled, not pushed.** `GET …/events?after=N` is a
  cursor read, and that is a bound rather than an unfinished stream: an
  open response would have to be pushed into from inside a caller's
  dispatch, which is entry #4's and #32's class. Named in
  `plugins/sessions/jinn-session/README.md`.

---

## 34. `jinn:fs` can append and it can rewrite, but it cannot DROP A SUFFIX — so a store that tolerates a torn tail has to rewrite the whole document to stay readable

**What happened.** The todos seam's journal reader admits an unterminated
last line as ABSENCE (a half-written record must read as "absent or
complete", never as a damaged one) and REFUSES a hole anywhere earlier.
That reader law is right and is unit-proven. What the real-composition
gate then found is that the law is not enough on its own: the next
`append` lands on the END of the partial line, so the tear and the new
record become ONE undecodable line — in the middle of the document. A
Todo that came back fine after one boot refuses to replay at the boot
after, and the store that holds it fails to activate.

**Evidence — the mechanism, reproduced.** A tolerable torn TAIL that is
appended onto stops being a tail: it fuses with the new record into one
undecodable line in the MIDDLE of the document, which the reader refuses
by its own law. Deterministic, and named:

```
$ cargo test -p jinn-todo --lib -- --exact \
    tests::an_append_onto_a_torn_tail_makes_a_hole_the_reader_refuses --nocapture
running 1 test
append-onto-a-tear replays as: journal line 3: a Todo's `status` is a closed surface and REFUSES the value `exec{` rather than dropping or guessing it (it admits backlog | executing | in-review | blocked | done | cancelled) at line 1 column 50
test tests::an_append_onto_a_torn_tail_makes_a_hole_the_reader_refuses ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 40 filtered out; finished in 0.00s
```

The test asserts all three readings in one place: the torn document
replays (absence), the SAME document with one more record appended does
not (a hole), and the same append onto a HEALED document does. Line 3 is
the fused line — the tear's bytes and the next record, read as one.

**Evidence — the store path.** `tests/composition/tests/todos.rs::a_torn_tail_is_absence_and_the_todo_before_it_survives`
drives the same mechanism through the real daemon: it manufactures the
tear the way a torn write would leave it (the daemon is killed, the
document is written short, the daemon is rebooted) and then requires the
next move to be durable. Before the heal existed that proof failed at
"the appended move to be durable" — the appended line was there in bytes
and gone as a record. That pre-fix run is NARRATED from the round it
happened in; the transcript above is the part that is reproducible on
demand.

**What the harness does meanwhile.** The durable store HEALS the document
on adoption: a replay that reports `torn_tail_bytes > 0` is followed by a
full `fs::write` of the whole prefix
(`plugins/todos/jinn-todo-fs/src/journal.rs::heal`), and `describe`
reports `healed-tails` so bytes are never discarded in silence. No record
is lost — by the reader's own law those bytes were never a record — but
this is a REWRITE of an append-only document, and it costs the whole
document's bytes per heal. On a long-lived ledger that is the wrong shape
for the smallest possible repair.

**The capability that would retire it.** A `truncate(path, len,
idempotency-key)` on `jinn:fs`, or an `append` variant that refuses
unless the document ends on a given byte (so a store learns of a tear
without reading and rewriting). Either lets a store drop exactly the
bytes that are not a record, atomically, without touching the ones that
are. The kernel already owns the atomic commit path (#22), so this is a
new operation on an existing mechanism rather than a new mechanism.

*Evidence grade:* **packet-card-ready — the mechanism is reproduced on
demand, the store path is narrated.** The transcript above reproduces the
defect the missing operation would retire, the workaround is in the tree
and cited, and the shape of that operation follows from the contract's
own vocabulary. Two things are NOT established and the grade does not
claim them. First, the pre-fix composition failure is a narrative of the
round it happened in, not a command a reader can re-run — the heal is in
the tree, so that proof is green now. Second, how a tear arises through
the kernel's own commit path: `#22` closed `append` as whole-document
atomic, so the suite has to manufacture the tear by writing behind the
daemon's back. Read the entry as "the reader's tolerance has a hole in
it" rather than as "the kernel tears writes".

---

## 35. Latency compounds per LAYER, because every seam that composes another has to poll it — the three-layer stack pays two poll periods, and a fourth would pay three

**What happened.** Nothing failed. This is the first seam in the
distribution that sits on top of a seam that itself sits on top of one:
`jinn:todo.<store>` drives `jinn:session.<store>`, which drives
`jinn:engine.<id>`. Neither store may LISTEN for the layer below it —
that is entry #4's and #32's nested-dispatch class, and a store that
emitted from inside its callee's delivery is exactly the deadlock this
repo keeps finding. So each layer POLLS the one below on its own clock
wake. The cost is additive by construction: a session's answer is visible
to the store one session-poll after the engine produced it, and to the
TODO store one todo-poll after that.

**Evidence.** `plugins/todos/store-core/store.rs` (`poll_once`, and the
module doc's third discipline) beside
`plugins/sessions/store-core/store.rs`, which does the same thing one
layer down; the suite kits both at 250 ms and the composition proofs wait
accordingly (`DISPATCH_DEADLINE` is 120 s against the sessions suite's
90 s, for the same work).

**Why it is worth a card even though it is not a defect.** The
distribution's whole shape is seams composing seams, and this is the
first place it pays the cost twice. Nothing here is measured: what the
entry establishes is that the term is ADDITIVE and structural, which is
read off the two implementations. Two layers is a bound nobody notices. The company ledger over sessions over engines is already
three; a workflow seam over todos would be four. The additive term is
structural, not incidental, and it is paid on every answer.

**The capability that would retire it.** Whatever closes #4/#32 — a
delivery that does not run inside the caller's dispatch, so a consumer
can be NOTIFIED by the seam it drives rather than asking it. That would
make the cost per layer a wake rather than a period.

*Evidence grade:* **derived, not measured.** The additive structure is
read off the two implementations and is certain; the harness has NOT
measured end-to-end latency at each layer, has not established a
distribution, and does not claim a number. The deadlines the suite uses
are generous margins, not measurements. A card should start by measuring
it at two and three layers.

### CLOSED as a prediction — MEASURED at pin `3a8e5c0`, phase 2.6

The entry above stands as the record of what it claimed and how it was
graded. This is its answer, appended in place rather than filed
elsewhere.

Phase 2.6 added the fourth layer the entry was about
(`jinn:workflow.<store>` over `jinn:todo.<store>` over
`jinn:session.<store>` over `jinn:engine.<id>`), so the prediction became
testable and `tests/composition/tests/workflows.rs::dispatch_latency_at_two_three_and_four_layers`
tested it. All three depths are driven from ONE daemon, on the same
engine, at the same poll period, in the same minutes, so they differ in
the number of layers and in nothing else; each depth's workflow is a
single dispatch node with no edges, so a run is one pass through the stack
with no graph walk mixed in; and the observer polls at 15 ms against
stores that poll at 250 ms, so the measurement is not dominated by the
measuring.

```
FINDINGS #35, measured at poll-ms=250 per store layer (observer polls every 15 ms, 5 samples, one daemon):
  2 layers (session -> engine)              median 513 ms  samples [508, 508, 513, 564, 548]
  3 layers (todo -> session -> engine)      median 755 ms  samples [721, 752, 772, 755, 783]
  4 layers (workflow -> todo -> ...)        median 1084 ms  samples [1041, 1121, 1084, 1087, 1079]
  per-layer term: 3-2 = 242 ms, 4-3 = 329 ms (the additive model predicts one poll period, 250 ms, for each)
```

**The additive model is CONFIRMED.** The third layer costs 242 ms against
a predicted 250 ms — within 3%, which is as close as a prediction of this
shape can come. The fourth costs 329 ms: additive in kind, 32% over the
predicted period. Four layers take rather more than twice what two take,
on a stack whose engine work is a 250 ms delay.

**The 79 ms the fourth layer costs beyond a poll period is NOT explained
by a measurement.** The candidate explanation, read off the two
implementations, is that the layers are not symmetric at their START: a
Todo store OPENS its session synchronously inside the `dispatch` call
(`plugins/todos/store-core/store.rs`, `on_dispatch`), while a run store
starts its node on its own clock wake and only then dispatches the Todo
(`plugins/workflows/store-core/store.rs`, `start_ready_nodes`) — so the
fourth layer pays at both ends where the third pays at one. That is
*derived from reading the two files, not separately measured*, and it is
recorded at that grade rather than promoted to the entry's finding.

*Evidence grade:* **measured.** Five samples per depth from one daemon at
a stated poll period, with the command in the tree and re-runnable. The
entry's original grade was honest and its prediction held; what changes is
that the term is now a number rather than a structure. The capability that
would retire the cost is unchanged — whatever closes #4/#32, so a consumer
is NOTIFIED by the seam it drives rather than asking it, would make the
per-layer term a wake rather than a period.

---

## What the WORKFLOWS seam could NOT prove, and says so

Recorded here because the honest limit of a proof belongs beside the
frictions, not buried in a test:

- **A vendor engine under a workflow run runs only where an operator asks
  for one by name.** `tests/composition/tests/workflows.rs::the_same_run_runs_over_a_vendor_engine_when_the_operator_names_one`
  binds a real vendor CLI as the last leg of the FOUR-layer proof,
  changing the engine field and nothing else. It is gated on
  `JINN_HARNESS_WORKFLOW_VENDOR_ENGINE` for the reason the todos gate
  gives one layer down: it spends metered inference under the operator's
  own authentication. A SKIP proves nothing and is never summarized as a
  pass; an engine that is NAMED and not mounted FAILS rather than
  skipping. The echo and child-backed legs remain, and on their own they
  prove a binding swap between two in-repo providers — not that four
  layers survive contact with a real CLI.
- **The torn tail is manufactured, not observed** — the same limit the
  todos seam recorded, for the same reason (#22, #34). The suite writes an
  unterminated line behind the daemon's back. What is proven is the
  READER's behaviour and the store's heal, not that the kernel tears.
- **Concurrency between two writers to one run is not proven.** The proofs
  drive one caller at a time. The registry is behind a mutex and the
  journal's append is atomic, so a lost update is not reachable through
  the operations as written — but "not reachable by inspection" is not a
  proof, and no test races two node-state moves on one run.
- **A run is not RESUMED across a restart, and that is a decision, not a
  gap.** A fresh incarnation drives nothing it did not start, so a run the
  daemon stopped mid-flight is recorded `interrupted` — including its
  nodes that had not started yet. Running the procedure again is a NEW
  run, which pins its own revision. Nothing here proves a resumable run
  would be safe, because nothing here builds one.
- **`spec-digest` is a change detector under an ACCIDENTAL threat model.**
  The packet's stated model is races, crashes and torn writes, not an
  adversary with write access to the data root. Someone who can write a
  store's journal can forge a run's history or a definition, and this seam
  would not detect it as forgery; what the reader catches is damage. The
  authority on what a run executes is the spec the run carries whole.
- **The graph walk is proven on two shapes.** A single dispatch node, and
  a two-lane graph where one lane is followed and the other is skipped. A
  wide fan-out, a join with several followed inbound edges, and a deep
  chain are unit-proven in `jinn_workflow` and are NOT driven through the
  daemon.
