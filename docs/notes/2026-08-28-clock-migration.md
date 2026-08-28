# Agent note — phase 1.5: cron onto `jinn:clock` (pin `01133c45`)

The `01133c45` pin (jinnd M2-K2) brought the two capabilities the cron seam
had been logging as frictions: `jinn:clock` (FINDINGS.md #1) and the event
bus's `DispatchTrace` tap (FINDINGS.md #2). This note records the choices
made while adopting them; the contract law itself lives in
`plugins/cron/jinn-cron/README.md`.

## Why one periodic alarm, not one `alarm-at` per job boundary

The packet asked for a periodic alarm, and the seam was already shaped for
it: `plan_tick` is a poll — jobs × state × instant → fires, records, state —
so it absorbs a coarse wake honestly, folding every boundary that elapsed
since `last` into one fire plus one `skipped` record. One alarm per
scheduler is also one thing to re-request on activation, one thing to cancel
on dispose, and one `AlarmWake` row per wake regardless of job count.

The refinement worth naming: an `alarm-at` per job's next boundary would
fire *on* the boundary instead of up to `tick-ms` late, and would idle
between boundaries instead of waking on a grid. It costs an alarm per job
(each a live revertible effect) and a re-arm after every fire, and it makes
the firing law's catch-up path the exception rather than the norm. Not done;
recorded here so the next person does not have to rediscover the trade.

## Why the scheduler still calls `now` at `activate`

`alarm-every`'s first wake is one full period out — there is no "fire now,
then every P" shape (FINDINGS.md #13). Combined with the contract's honest
bound that alarms do not survive a kernel restart, a scheduler that only
held the alarm would be blind for a whole `tick-ms` after every activation:
at the soak's 15-minute cadence, a 15-minute hole after every restart.

So restart re-entry is the guest's own act. `activate` reads `now`, runs one
plan immediately, and then requests the alarm — the catch-up fire lands at
boot. One consequence is worth stating plainly: the boot is no longer
"quiet". A scheduler with no persisted state records `schedule-started` at
activation (firing law #4) rather than at the first tick, and a scheduler
with state may fire its catch-up before the daemon's boot log has settled.
That is the honest shape; the composition suite asserts it rather than
working around it.

## Why `tick-ms` is a knob separate from `every-ms`

They answer different questions: `every-ms` is a job's period (when it is
*due*), `tick-ms` is how often the scheduler *looks*. Collapsing them would
mean either one alarm per job or a wake rate set by the shortest job.

Keeping them apart makes the cost explicit. Each wake is one `AlarmWake`
ledger row whether or not anything fires, so an operator trades fire
lateness against ledger growth with a number they choose: a fine `tick-ms`
buys punctuality and pays in rows, a coarse one is cheap and fires up to
`tick-ms` late. The soak sets both to 900000 — one wake per 15 minutes for a
15-minute job — which keeps the cadence and the lateness envelope identical
to the retired tick driver's, so the +7d comparison across the bump stays
apples to apples.

## Why the tick topic was withdrawn rather than kept

The obvious alternative was to keep `jinn:cron/tick` and let something
kernel-side or operator-side emit it. But nothing outside this repo's own
retired stand-in ever emitted it, and a topic with no emitter is a side door
waiting to happen: any plugin granted the topic could inject a `now-ms` the
firing law would trust. With time arriving through a granted capability the
kernel meters (R9 floor, wake attribution in the ledger), an unmetered
alternative path is exactly what should not exist. It is withdrawn in the
contract's `## Changes` section, not quietly deleted.

## Why the composition suite now runs on real time

Deterministic time injection died with the tick entry: there is no config
field to write an instant into any more, and the harness must not fake the
kernel's clock. So the suite boots a kit built with
`--every-ms 2000 --tick-ms 500` and observes real fires, alarm registration
and withdrawal, `AlarmWake` and `DispatchTrace` ledger lines, the
re-request after a restart, and the clock grant gating the scheduler.

Real time means no exact expected transcript, so the assertions are written
on invariants that hold for any interleaving rather than on counts the
scheduler happens to reach: every fired boundary is on the grid
(`scheduled-ms % every-ms == 0`), a catch-up's `scheduled-ms` equals the
preceding `skipped` record's `last-ms + every-ms`, and counts only ever
grow. Those hold whether the machine was fast or the CI runner stalled — and
they are the properties the firing law actually promises.

## Why the soak keeps a 15-minute wake

The soak's job is the +7d comparison, not punctuality. Holding the wake
cadence at the retired driver's 15 minutes keeps fire counts, lateness, and
per-day ledger growth measured against the same baseline the first half of
the soak produced — the only change the audit should see is the row cost per
tick collapsing from ~5 bookkeeping rows of fiber churn to one `AlarmWake`.
Running the soak fine and the comparison coarse would have confounded the
one measurement the phase exists to take (`SOAK.md`, the +7d audit).
