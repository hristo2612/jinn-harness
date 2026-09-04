# A registration is replaced, never absent — and an emit is covered by its topic's grant

*Harness pin-bump 9 (PLA-364) — adopting jinnd `cb08683` (M2-K26 at
`138fdce`, the restart-window continuity and #49 riding on it, plus its
amendment 2 for FINDINGS #53 — the first candidate `138fdce` was held;
`jinn:plugin` 0.11.0 → 0.12.0, prose only, additive). Kernel changes never here; this note is
about what the harness could finally assert, and the nine emitters the
adopting audit found ungranted.*

## What UI-2 had to leave open, and why the pin is the fix

UI-2 proof 5 edited an extension's `source` through the profile
document and posted a moment every ~5 ms across the edit
(`2026-09-03-a-moment-is-one-walk.md`, "What proof 5 records"). At
`a53a352` the record was FINDINGS #47: the kernel UNLISTENED the old
incarnation's registration at its suspension, the replacement's `listen`
landed only at its commit, and for the ~1.5 s between them every walk
selected nobody — 53 sends answered UNMODIFIED in one edit, `listeners:
0` on each trace, no `DispatchRefused` anywhere. M2-K9's `restarting`
refusal is keyed on a SELECTED listener, so a withdrawn registration was
never asked. The transport kept its half of fail-closed (a refusal it is
handed is typed); it was never handed one. Beside it, the KG-6 probe had
confirmed #49 on the ledger: `events.emit` checked the reserved-topic
refusal and no topic grant, so the transport's three `jinn:ui/*` grants
were a statement, not an authority.

M2-K26 answers both from the kernel's side (`docs/packets/M2-K26.md` in
the kernel repo): (a) a `listen` registration outlives its instance's
suspension as a refusing registration — the same row, no delivery
target — for exactly as long as the fiber owes a transition; (b) the
replacement activates as a staging seat and commits under ONE
topic-table lock, the Mode-1 swap's `rebind`, so no walk ever sees
"neither"; (c) a failed replacement withdraws the tombstones on the
record; (d) the oracle answers `restarting` mid-`Loading` on introspect
too; (e) `emit` is covered by the grant of the topic's own name exactly
as `listen` is, the refusal the broker's `GrantRefused` row. What the
harness owes is the flipped assertions, the grants every emitter now
needs, and the closures.

## The bump, in the order it happened

1. **The proofs, red first, before the pin** (`c11b505`). Proof 5 flipped
   to its intended assertion; the KG-6 probe flipped and renamed to
   `an_entry_emitting_off_its_topic_grant_is_refused_on_the_record`; six
   kit tests, one per builder, name the topic each first-party emitter
   needs; the plugins page test expects five pills. Run on the merge-base
   against the `b1dbe8f` daemon:

   ```text
   KG-6: the transport with NO topic grant posted a moment — status 200, error null, walks 1, GrantRefused rows [], the extension's rows after the send ["{\"ContractCall\":{\"contract\":\"jinn:clock\",\"operation\":\"now\"}}"]
   thread 'an_entry_emitting_off_its_topic_grant_is_refused_on_the_record' panicked at tests/composition/tests/moments.rs:1498:5:
     left: 200
    right: 502
   proof 5: after the edit — 9 answers with the OLD fold, 0 REFUSED typed `restarting` (first at None), 50 answered the payload UNMODIFIED (fail-open; first at Some(356.923834ms)), the new fold landed at 3.529190084s; walks with listeners=0 on the ledger: 50; refusal rows: []; the old incarnation's suspension to the new one's Active: Some(1603) ms
   thread 'a_restarting_extension_refuses_the_moment_typed_and_nothing_is_sent' panicked at tests/composition/tests/moments.rs:810:5:
   the window was hit: 9 old answers before it, landed at 3.529190084s
   test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 9 filtered out; finished in 106.30s
   ```

   (The failing assertion is `!refused.is_empty()` — zero refusals in a
   window the client hit; its message names the old answers before the
   window and the landing after it.)

   the kits (`cargo test --no-fail-fast -p engine-kit -p session-kit -p todo-kit -p workflow-kit -p cron-kit -p api-kit --lib`):

   ```text
   thread 'tests::the_settings_provider_is_granted_the_two_topics_it_emits' panicked at tools/api-kit/src/lib.rs:92:13:
   test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   thread 'tests::the_scheduler_is_granted_every_job_topic_it_fires' panicked at tools/cron-kit/src/lib.rs:189:13:
   test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   thread 'tests::a_provider_entry_is_granted_the_event_topic_it_emits' panicked at tools/engine-kit/src/lib.rs:230:9:
   test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   thread 'tests::a_store_entry_is_granted_the_event_topic_it_emits' panicked at tools/session-kit/src/lib.rs:139:13:
   test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   thread 'tests::a_store_entry_is_granted_the_event_topic_it_emits' panicked at tools/todo-kit/src/lib.rs:150:13:
   test result: FAILED. 3 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   thread 'tests::a_store_entry_is_granted_the_event_topic_it_emits' panicked at tools/workflow-kit/src/lib.rs:151:13:
   test result: FAILED. 4 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   ```

   and the web (`vitest run … plugin-row.test.tsx` with the page at `main`):

   ```text
   × renders every NOT-YET item disabled, each with its finding number 42ms
   ⎯⎯⎯⎯⎯⎯⎯ Failed Tests 1 ⎯⎯⎯⎯⎯⎯⎯
    FAIL  src/routes/settings/plugins/__tests__/plugin-row.test.tsx > an extension row > renders every NOT-YET item disabled, each with its finding number
   AssertionError: expected [ <button …(5)></button>, …(5) ] to have a length of 5 but got 6
    Test Files  1 failed (1)
         Tests  1 failed | 2 passed (3)
   ```

2. **The pin, one commit** (`c943ec2`). `KERNEL-PIN.md`'s procedure:
   commit, both hashes and the vendored `kernel-pin/` trees together;
   `cargo test -p harness-pin` green on both gates:

   ```text
   test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
   test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.31s
   test result: ok. 39 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.29s
   test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   ```

   The plugin world moved 0.11.0 → 0.12.0 (prose only), so
   `tools/ext-kit/tests/imports.rs` moved with it in the same commit
   (the harness's mirror of the version the Boa guest imports). At this
   commit proof 5 and the KG-6 probe are GREEN with no harness change
   but the pin — the flip is the kernel's, which is what a pin-bump
   proof is for:

   ```text
   KG-6: the transport with NO topic grant posted a moment — status 502, error {"code":"refused","detail":"emit refused: grant refused: jinn:ui/before-send"}, walks 0, GrantRefused rows [], the extension's rows after the send []
   proof 5: after the edit — 9 answers with the OLD fold, 63 REFUSED typed `restarting` (first at Some(346.760792ms)), 0 answered the payload UNMODIFIED (fail-open; first at None), the new fold landed at 3.554579833s; walks with listeners=0 on the ledger: 0; refusal rows: ["{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"GrantRefused":{"contract":"cron:health","reason":"NotGranted","detail":null}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"DispatchRefused":{"topic":"jinn:ui/before-send","mode":"Waterfall","target":"ext-green","incarnation":13,"owed":"Reload"}}", "{"GrantRefused":{"contract":"cron:health","reason":"NotGranted","detail":null}}"]; the old incarnation's suspension to the new one's Active: Some(1614) ms
   proof 5: every moment inside the window was refused typed `restarting`, none answered unmodified (FINDINGS #47 closed at this pin)
   test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 9 filtered out; finished in 135.99s
   ```

   And at this same commit the composition boot is RED where the audit
   said it would be: every first-party emitter but the `ui` transport
   emits under no topic grant, and the kernel now refuses it. The cron
   suite (the scheduler firing `cron:health`) and the settings suite
   (the provider's `jinn:settings/changed` and `/refused`):

   ```text
   $ cargo test -p composition --test cron
   test a_stop_landing_mid_tick_lands_the_whole_tick ... FAILED
   test restart_rerequests_the_alarm_fires_once_and_records_the_gap ... FAILED
   test reschedules_on_config_edit_through_reconcile ... FAILED
   test fires_on_schedule_from_kernel_wakes_and_records_the_run ... FAILED
   test an_edit_landing_before_readiness_is_applied ... FAILED
   test a_clean_shutdown_suspends_and_a_restart_resumes_the_schedule ... FAILED
   test corrupt_persisted_state_is_quarantined_and_recorded ... FAILED
   test run_history_is_append_backed_and_the_consumer_sees_the_wider_surface ... FAILED
   timed out waiting for a settled fire
   timed out waiting for the first fire
   timed out waiting for a fire on the halved schedule
   timed out waiting for the first fire to land in the consumer report
   timed out waiting for the first fire to settle in the history log
   timed out waiting for two settled fires
   $ cargo test -p composition --test settings
   test declare_resolve_and_patch_on_both_paths_with_the_c5_c6_transcript ... FAILED
   test swapping_the_settings_provider_by_profile_edit_leaves_the_consumers_untouched ... FAILED
   test a_patch_the_schema_refuses_is_typed_and_on_the_record ... FAILED
   test a_patch_reports_exactly_what_the_next_get_resolves_in_both_orders ... FAILED
   assertion `left == right` failed: the scheduler holds the patched table: HTTP/1.1 200 OK
   timed out waiting for the refusals to land on the record
   timed out waiting for the refusal to land on the record
   test result: FAILED. 0 passed; 4 failed; 1 ignored; 0 measured; 0 filtered out; finished in 119.52s
   ```

3. **The grants** (`bfa0cfc`). The audit (every `events::emit`
   call site in the workspace, resolved to its topic, matched against
   the entry each kit writes): twelve call sites, ten emitters, ONE
   granted. The FINDINGS #49 sentence "every first-party emitter in this
   distribution already carries the grant it would need" was true of the
   `ui` transport and false of the engine providers
   (`jinn:engine/event`), the session, todo and workflow stores
   (`jinn:session/event`, `jinn:todo/event`, `jinn:workflow/event`), the
   settings provider (`jinn:settings/changed`, `jinn:settings/refused`)
   and the cron scheduler (its job topics, `cron:health` in the shipped
   table). Each kit builder now writes the topic the entry EMITS beside
   the contract it provides; `tools/cron-kit` derives the scheduler's
   topic grants from its job table so the two cannot drift; the four
   composition fixtures that hand-build an emitting entry
   (`sessions.rs`, `todos.rs`, `workflows.rs`, `engines.rs`) carry
   theirs. NOT granted: the moment topics on a transport in a profile
   without the UI — `mount_moments_on` stays the one path, so a
   `POST /v1/moments/…` in the operator-api profile is refused `refused`
   on the record, which is what that profile says. Green:

   ```text
   $ cargo test --no-fail-fast -p engine-kit -p session-kit -p todo-kit -p workflow-kit -p cron-kit -p api-kit --lib
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
   ```

   ```text
   $ cargo test -p composition --test cron
   test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 61.21s
   $ cargo test -p composition --test settings
   test result: ok. 4 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 56.61s
   ```

   Ten consecutive fresh boots of the `ui` profile (UI-1 proof 5b) at
   the pin, with the grants:

   ```text
   proof 5b: 10/10 fresh boots reached transport active + listening + document served; boot-to-served [60.448884666s, 64.396083417s, 64.402474667s, 60.5249385s, 60.6062165s, 60.193415209s, 60.519599833s, 60.53093525s, 60.308801541s, 60.289520958s]
   test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 5 filtered out; finished in 652.45s
   ```

4. **The page and the record** (`b5ddf32`, this note's commit). The
   plugins page drops the #47 pill and keeps #37
   (`web/src/routes/settings/plugins/not-yet.tsx`; its test pins five
   pills and neither `#47` nor `#48`). FINDINGS #47 and #49 are graded
   ANSWERED at the pin with the transcripts appended in place; #52 is
   re-measured at the pin and stays open (M2-K26 names sibling order out
   of scope and adds no field to the trace). The README's status and
   limitations name the retirements; the plan's §9.3 item 5 and §9.6
   KG-6 are restated to the pin with their previous text kept, the
   pin-bump 8 shape.

## What the proofs say now

| proof | red first (`b1dbe8f`) | green at `138fdce` (first candidate) | green at `cb08683` (adopted) |
|---|---|---|---|
| 5 the restart window is closed | `0 REFUSED, 50 answered UNMODIFIED, walks with listeners=0: 50, refusal rows: []` |    `63 REFUSED typed restarting (first at 347 ms), 0 answered UNMODIFIED, walks with listeners=0: 0, one DispatchRefused { owed: Reload } per refused send, the new fold at 3.55 s, window 1.6 s` | `49 REFUSED typed restarting (first at 340 ms), 0 answered UNMODIFIED, walks with listeners=0: 0, 49 DispatchRefused { owed: Reload } rows, the new fold at 3.46 s, window 1.55 s` |
| KG-6 an off-grant emit is refused | `status 200, walks 1, GrantRefused rows []` |    `status 502 refused "emit refused: grant refused: jinn:ui/before-send", walks 0, GrantRefused { contract: jinn:ui/before-send, reason: NotGranted } on jinn-api-http, the extension’s rows []` | the same: `status 502 refused, walks 0, GrantRefused { jinn:ui/before-send, NotGranted } on jinn-api-http, the extension's rows []` |
| six kit tests, the emitters' grants | six panics, one per builder | six green, the grants written | six green (unchanged code) |
| 5b ten boots deterministic | (unchanged proof) | `10/10 fresh boots reached transport active + listening + document served (652 s in all)` | see the ui suite tail below |

## #52, re-measured

Three solo runs of proof 3 at the pin (`138fdce`), the grants in place:
in all three the walk folded `ext-blue` first (`"hello 🔵 🟢"`); the
`listen` rows read green-then-blue in runs 1 and 2 and blue-then-green
in run 3. So the fold order was stable across these three boots while
the ROW order was not — the rows are not the walk's witness in either
direction, which is what proof 3 asserts, and no field on the trace
names the order taken. M2-K26 names sibling order out of scope and adds
nothing to `DispatchTrace`; #52 stays open, its NOT-YET assertion as
written. At the adopted pin `cb08683` one more boot: fold
`["ext-blue", "ext-green"]`, rows `["ext-blue", "ext-green"]` —
agreeing this time, as the boot dealt; still open.

## What the first candidate pin broke — FINDINGS #53 (closed at `cb08683`)

The final gate at the first submission's head (`d2557ba`, pin
`138fdce`) was RED on two engines proofs
(`a_child_sees_only_the_environment_its_grant_admits`,
`a_child_writing_far_past_its_budget_puts_at_most_the_budget_on_the_wire`):
each restarts the spawn provider by a config edit and then runs a
child, and the run never settles. The ledger shows the child exit
(`ProcessExited { code: 0 }`) and NOT ONE row for the provider's
`alarm-at` after the spawn — no registration, no refusal, no wake. By
code at `138fdce`: a replacement activates as a staging seat (M2-K26
(b), `lane.rs:182 staging: replacing`) and nothing clears the flag at
the commit, so every registration surface (`hostcaps.rs:192` alarms,
`surfaces.rs:75` listen, `:135` provide) records-not-routes for the
rest of the incarnation's life. Providers that register everything in
`activate` are unaffected (cron, settings, the transport — their suites
are green); a provider that arms an alarm per run loses it after any
config edit. Filed as FINDINGS #53, Blocker-class, with the transcript
and the citations; not patched, not vendored, not worked around
(standing order 1). The two proofs stay red at this head — turning a
proof of a working seam into a NOT-YET is a ruling, not an
implementer's call — so the packet lands or holds on the COO's word.
The COO held it; the fix landed; the closure is the next section.

## The hold, and the re-targeted pin

The COO held the bump on #53 (PLA-364 ruling: a pin that breaks cron
after any config restart goes under neither the operator's instance
nor the soak); the branch and its PR stayed intact, a kernel fix packet
(M2-K26 amendment 2, jinnd PLA-366) ran on the kernel lane, and the
bump resumed as a resubmission with the pin re-targeted to the fix's
landed sha. jinnd `cb08683` = the fix `89f48fa` (a staged seat's
`staging` flag is cleared at `commit_staged`, so a registration made
after `activate` routes as on a first activation; the red daemon test
`ea7ee82` precedes the fix `7bd8ea7`) plus the verifier's invariant
(`cb6477f`, `ed1d0bc`). `wit/` and `contracts/` are byte-identical
between `138fdce` and `cb08683` (`git diff --stat 138fdce cb08683 --
wit contracts` is empty), so the pin commit (`b7960ae`, one commit on
top — pushed history is never rewritten, so `c943ec2` stays in the
branch's history as the superseded candidate) moves the commit line
only: both hashes and the vendored trees are unchanged, checked
byte-identical against `git archive cb08683 wit contracts`, and
`jinn:plugin` stays 0.12.0.

```text
$ cargo test -p harness-pin
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.24s
test result: ok. 39 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.21s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

(Gate 2 ran against the kernel checkout at `cb08683`; no skip line.)
At the re-targeted pin the two engines proofs that were red by the
kernel are green with no harness change but the pin, and the whole
engines suite with them:

```text
$ cargo test -p composition --test engines
test a_child_writing_far_past_its_budget_puts_at_most_the_budget_on_the_wire ... ok
test a_child_sees_only_the_environment_its_grant_admits ... ok
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 305.93s
```

The run's ledger shows the post-restart `alarm-at` WAKING
(`AlarmWake { alarm: 4 }` after the `ProcessExited`, then the drain and
the `jinn:engine/event` trace; a second restart's run wakes on alarm 5)
— the rows FINDINGS #53's closure carries. One correction the closure
records: a post-activation `alarm-at` has no `EffectRegistered` row on
ANY seat at this pin (a fresh seat's per-run alarm is an `AlarmWake`
alone in every engines ledger), so the missing wake was the defect,
never the missing row; noted, not filed. Everything else from the
first submission — the grants audit, proof 5's flip, the KG-6 probe,
the #47 pill, the kit tests — stands as committed; the whole
composition suite is re-run at `cb08683` below.

## The composition suite at the adopted pin

Every suite, in sequence on one cargo lane, against the daemon built
from `git archive cb08683` (the kit's cache marker reads `cb08683`),
from a tree whose only difference to the pin commit `b7960ae` is
documentation (`git diff --stat b7960ae -- . ':!*.md'` is empty):

```text
$ cargo test -p composition --test engines
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 305.93s
$ cargo test -p composition --test moments -- --test-threads=1 --nocapture
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 799.30s
$ cargo test -p composition --test cron
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 60.77s
$ cargo test -p composition --test settings
test result: ok. 4 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 68.18s
$ cargo test -p composition --test api
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 94.54s
$ cargo test -p composition --test auth
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 34.89s
$ cargo test -p composition --test plugins
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 156.87s
$ cargo test -p composition --test plugins_lifecycle
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 49.75s
$ cargo test -p composition --test sessions
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 369.16s
$ cargo test -p composition --test todos
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 487.18s
$ cargo test -p composition --test ui
test a_fresh_boot_is_deterministic ... ok
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 688.41s
$ cargo test -p composition --test workflows
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 718.55s
```

The ui suite's `a_fresh_boot_is_deterministic` is UI-1 proof 5b, the
ten consecutive fresh boots (its per-boot timings are printed only
under `--nocapture`; the suite ran captured, 688 s for the six cases).
The settings suite's one `ignored` case is the standing ignore it
carried at `b1dbe8f`.

## Meter, re-read at the resubmission

The resubmission changes one line of `KERNEL-PIN.md` and documentation:
`git diff --numstat main -- '*.rs'` is the first submission's diff
exactly, so the UI-2 meter's reading is unchanged — **39 production
Rust net** (2 on the meter's paths, 37 declared beside it), the delta
this round **0**, the card's ≤ 150 met. The meter section below is the
first submission's and still describes the head.

## What did not move

- **An `emit`-mode notification inside the window is still lost** and
  traced `listeners: 0` — the card's named limit (§Out: counting it is a
  `DispatchTrace` field, a facade delta the card did not authorize).
- **A transport's policy for a `failed` validator** — the harness's
  question (UI-2), not decided here.
- **#51's non-fatal half** and **#52's reading** — later cards.
- **The soak** — pin `3a8e5c03`, pid untouched; the 2026-09-04 audit
  decides its bump.
- **The `ErrorCode::Refused` → `502` mapping** for a grant-refused emit
  is the transport's existing class (`jinn-api-http-wire::status_for`);
  the probe asserts it as it is. Whether a profile misconfiguration
  should be a `502` or a `503` is a seam question, not this bump's.

## Meter

The UI-2 meter (`docs/plans/ui-malleability-arc.md` §9: UI-1's paths
plus `plugins/ext/**`, `tools/ext-kit/**`,
`plugins/plugins/jinn-plugins/src`; `cfg(test)` a declared category;
the composition suite excluded), `git diff --numstat main` on a clean
tree. Estimate before the first edit: about 30 production Rust net
(topic grants in the kits), under the card's ≤ 150; re-priced once on
the first reading: **39 production Rust net in all** (2 on the meter's
paths, 37 declared beside it), so the estimate is re-priced to 39 and
no ceiling is approached. The card's number (≤ 150) is a production
total across the harness, not a per-path meter, and it is met.

On the UI-2 meter's paths (`git diff --numstat main -- 'plugins/ext/**/*.rs'
'plugins/ui/**/*.rs' 'plugins/api/jinn-api-http/src/*.rs'
'plugins/api/jinn-api-http-wire/src/*.rs' 'plugins/plugins/jinn-plugins/src/*.rs'
'tools/ui-kit/**/*.rs' 'tools/ext-kit/**/*.rs'`):

| file | + | − | note |
|---|---|---|---|
| `tools/ui-kit/src/lib.rs` | 6 | 4 | `mount_moments_on`'s doc, restated to the pin |
| `tools/ext-kit/src/lib.rs` | 1 | 1 | the world version in a doc line |
| `plugins/ext/jinn-ext-js-boa/src/lib.rs` | 1 | 1 | the world version in a doc line |
| `tools/ext-kit/tests/imports.rs` | 4 | 4 | under `tests/`, excluded |

**Production net on the meter's paths: +2**, so the UI-2 meter reads
**773** from 771. `cfg(test)` modules touched on those paths: none.

Declared beside the meter (amendment 5's shape) — production Rust
OUTSIDE the listed paths, the reason being the K26 (e) audit: every
first-party emitter in the distribution must carry the topic it emits,
and those emitters are mounted by the seam kits, not the UI's:

| file | + | − | of which `cfg(test)` | production net |
|---|---|---|---|---|
| `tools/engine-kit/src/lib.rs` | 29 | 0 | 25 (`mod tests`, one case) | +4 |
| `tools/session-kit/src/lib.rs` | 27 | 0 | 23 | +4 |
| `tools/todo-kit/src/lib.rs` | 27 | 0 | 23 | +4 |
| `tools/workflow-kit/src/lib.rs` | 27 | 0 | 23 | +4 |
| `tools/cron-kit/src/lib.rs` | 58 | 10 | 31 (a new `mod tests`, one case) | +17 |
| `tools/api-kit/src/lib.rs` | 30 | 3 | 23 (a new `mod tests`, one case) | +4 |

**Production net outside the paths: +37** (the cron scheduler's grants
are now derived from its job table, which is the one builder that grew
by more than a grant line). The composition fixtures
(`tests/composition/tests/{sessions,todos,workflows,engines,moments}.rs`)
are excluded by the meter's rule.
