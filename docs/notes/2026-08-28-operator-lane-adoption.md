# Agent note — phase 1.9: adopting operator-lane honesty (pin `9e61e47`)

The `9e61e47` pin (jinnd M2-K5) closed FINDINGS.md #16, #17 and #18 and
delivered #12's stated minimum, with no contract delta (the `wit/` and
`contracts/` trees hash identically to `4eb4a93`). This note records the
harness's choices; the closures live in `FINDINGS.md`, the operating
procedure in `SOAK.md`.

## Why the mitigations were deleted, not kept as fallbacks

`edit_profile_until` (rewrite with different bytes until the observation
holds) and `edit_profile_until_restart` existed because the daemon could
lose an edit under a success line. A helper that keeps rewriting would now
hide a regression of exactly the kernel property the pin bump adopts: if
the daemon ever swallowed an edit again, a retry loop would paper over it
and the suite would stay green. One atomic edit, gated on the readiness
line and followed by its observation, goes red instead. The proofs keep
their transcripts and flip them: `the_cron_grant_gates_the_consumer_peek`
asserts zero all-empty `reconciled` lines (the #17 signature) after two
single edits; the clean-stop proof asserts `state.last` EQUALS the newest
history record (the #16 shape, no longer tolerated).

## Why `booted()` waits for the readiness line, not `boot.json`

`boot.json` is boot evidence; the readiness line is the daemon's own
statement that the watcher is armed and the boot reconcile is done. The
operator lane keys on the statement (FINDINGS.md #12's minimum) and the
proof `readiness_is_announced_once_after_the_boot_reconcile` pins its
shape: exactly one line, after the `reconciled` line, `"watcher":"armed"`,
naming the canonical profile path, with `boot.json` already on disk.
`SOAK.md` §Start makes the same switch for both launch lanes.

## Why the mid-tick proof aims at the probe write

The scheduler writes `cron/state.json` on EVERY wake (500 ms in the suite),
firing or not; only one wake in four is a firing tick. Aiming the SIGINT
at a state write therefore mostly hits a wake whose work is already over
— the first draft of the proof did, and saw 0/3 drains with the kernel
property still holding. The consumer's `health/probe.txt` write is the
earliest log mark that a FIRE is in flight with related effects (report,
run record, history append) still to come; aimed there, all three cycles
landed inside the tick and drained (the append logged after the `SIGINT`
line). The test requires at least one drained landing so it cannot pass
vacuously, and asserts exact state/history agreement on every cycle so it
holds for any interleaving.

## Why an edit lands "before readiness" rather than "during the reconcile"

The window between the daemon's first log line and its readiness line is
~40 ms on the reference machine — too narrow to target from outside with
a poll. The operator-facing property is that an edit landing anywhere
between spawn and readiness is applied (read by the boot itself, or a
watched delivery reconciled right after it) and never swallowed as an
echo; `an_edit_landing_before_readiness_is_applied` proves that shape
through the real daemon. The deterministic edit-during-a-slow-reconcile
case is the kernel's own (`jinnd-daemon` `tests/operator.rs`), where the
reconcile can be made slow on purpose.
