# Agent note — phase 1.7: adopting suspend ≠ dispose (pin `4eb4a93`)

The `4eb4a93` pin (jinnd M2-K4) closed FINDINGS.md #14 and #15 with no
signature change: `jinn:plugin@0.3.0` versions the lifecycle semantics. This
note records the harness's choices; the contract law lives in
`plugins/cron/jinn-cron/README.md`, the frictions in `FINDINGS.md`.

## Why the transcript test was replaced, not deleted

`a_clean_shutdown_withdraws_the_fibers_persisted_contribution` existed to go
red when the kernel retired the finding — it did, first run, exactly as
designed. Its successor asserts the new law from both sides: the files the
fibers left (state on the grid, the history log a byte-prefix extension of
its pre-stop bytes, the newest run record present, the report's count kept)
and the ledger (one `FiberSuspended` per fiber, `cause: Suspend`
transitions, the alarm released, not one `fs` withdrawal), then boots over
the same root and proves the schedule RESUMED (one `schedule-started` in the
whole history, boundaries strictly increasing across the stop, the alarm
re-requested). The restart proof went back to the clean path; the crash
path stays in the quarantine proof as the SIGKILL half of the equivalence.

## Why a torn final tick is tolerated, not asserted away

*(Superseded 2026-08-28 by pin `9e61e47`: the kernel drains the handler
and the proof asserts exact agreement — see
`2026-08-28-operator-lane-adoption.md`.)*

A SIGINT can land while the wake handler is mid-tick; the kernel seals the
journal and refuses the tick's later registrations (finding 15's closure).
The seam's contract already orders a tick state-first so a torn tick loses
a record and never doubles a fire; the proof therefore allows `last` to be
one boundary past the newest history record and logs the drain the kernel
could do instead as FINDINGS.md #16. Hiding the window by waiting for a
quiet moment would have proven less.

## Why config-edit proofs vary their bytes

*(Superseded 2026-08-28 by pin `9e61e47`: `edit_profile_until` is deleted;
one atomic edit after the readiness line — see
`2026-08-28-operator-lane-adoption.md`.)*

Three consecutive suite runs lost an edit each: the daemon remembers its
own write-back by re-reading the file after an apply, so an edit landing
during the boot reconcile is remembered as the daemon's echo and every
identical rewrite is skipped (FINDINGS.md #17). `edit_profile_until`
toggles a trailing newline per attempt — same document, different bytes —
and every config-edit proof goes through it. The mitigation is the
operator lane's, the defect is the kernel's; neither is hidden.

## Why "migrating the guests" changed no guest logic

The 0.3.0 world's shape is 0.2.0's; the guests regenerate their bindings
from the vendored `kernel-pin/wit`. What changed is what their persisted
documents MEAN: the entry's continuing record, not the incarnation's
revertible scratch. The scheduler already treated them that way (firing
law #3), which is why the kernel change landed as a retirement here and
not as a rewrite.
