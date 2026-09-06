# A bounded Todo result is reviewed explicitly

UI-4a, from `f957b425bc6b27f009119ff240bc33fb9c7dee5d`; kernel pin unchanged.

## Red evidence

Before implementation, `cargo test -p jinn-todo --lib`:

```text
failures:

---- dispatch::tests::todo_context_is_included_in_sequence_order stdout ----

thread 'dispatch::tests::todo_context_is_included_in_sequence_order' (81082827) panicked at plugins/todos/jinn-todo/src/dispatch.rs:163:9:
context omitted: Todo : Draft an update

Acceptance: Include the deadline
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace

---- tests::ui4a_oversized_prompt_is_refused_before_any_dispatch_change stdout ----

thread 'tests::ui4a_oversized_prompt_is_refused_before_any_dispatch_change' (81082864) panicked at plugins/todos/jinn-todo/src/tests.rs:860:5:
oversized UTF-8 prompt must be refused before dispatch


failures:
    dispatch::tests::todo_context_is_included_in_sequence_order
    tests::ui4a_oversized_prompt_is_refused_before_any_dispatch_change

test result: FAILED. 44 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

error: test failed, to rerun pass `-p jinn-todo --lib`
```

## Shape and compatibility

The first useful Todo journey uses existing store/session/engine boundaries.
Only the small status table is generated; no board or wire-package framework is
introduced. Contract details have one home in the Todo definition README, and
profile mounts in `profiles/ui/README.md`. The UI uses one operator principal.
Pending writes are retained in tab storage and matched to durable audit markers,
not automatically repeated. The record revision follows durable journal order;
active session links are deliberately not claimed durable before a dispatch ends.

Saved text editing is deferred. New comments and recorded needs-work notes enter
the next explicit task. Comment order is the durable sequence, not arrival time.
No kernel, pin, guest lockfile, toolchain or production change is part of this
packet. The workspace lockfile adds only ui-kit's local todo-kit dependency.

## Additional red evidence

The needs-work prompt assertion failed before the feedback addition:

```text

---- dispatch::tests::the_next_task_includes_recorded_needs_work_feedback stdout ----

thread 'dispatch::tests::the_next_task_includes_recorded_needs_work_feedback' (81229604) panicked at plugins/todos/jinn-todo/src/dispatch.rs:172:9:
assertion failed: brief(&todo).contains("Use a shorter opening")
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    dispatch::tests::the_next_task_includes_recorded_needs_work_feedback

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 48 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p jinn-todo --lib`
```

With only the server revision guard reverted, the current test failed:

```text

---- tests::ui4a_revision_survives_legacy_journal_and_rejects_stale_context_and_review stdout ----

thread 'tests::ui4a_revision_survives_legacy_journal_and_rejects_stale_context_and_review' (81286564) panicked at plugins/todos/jinn-todo/src/tests.rs:890:5:
assertion failed: todos.check_revision(id, Some(initial)).is_err()
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    tests::ui4a_revision_survives_legacy_journal_and_rejects_stale_context_and_review

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 49 filtered out; finished in 0.00s

error: test failed, to rerun pass `-p jinn-todo --lib`
```

With only the UI revision comparison reverted, both inspection tests failed;
with only the connection/mutation generation fence reverted, the late-read test
failed. Each implementation file was restored before the green run.

```text
⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯[2/2]⎯


 Test Files  2 failed (2)
      Tests  2 failed | 4 passed (6)
   Start at  17:53:38
   Duration  1.23s (transform 271ms, setup 432ms, import 174ms, tests 108ms, environment 1.42s)

⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯⎯[1/1]⎯


 Test Files  1 failed (1)
      Tests  1 failed | 4 passed (5)
   Start at  17:53:40
   Duration  404ms (transform 61ms, setup 50ms, import 62ms, tests 7ms, environment 229ms)

```

## First actual vendor checkpoint

From this branch's real ui-kit composition, with the daemon freshly built from
`git archive f8b285b5aaffddeeb4939a0035d6c18a03487999`, the actual new Todo vendor
test passed within the dispatch's first 45-minute checkpoint. Exact command,
with isolated fixture paths supplied by environment:

```sh
JINND_DIR="$KERNEL_SOURCE" JINN_TODO_JOURNEY_ROOT="$FIXTURE" \
  cargo test -p composition --test todo_journey -- --nocapture
```

The saved task-session journal attested `codex/gpt-6-astra/high` and denied tools.
The Todo remained executing after model completion. Conditional operator review
and close, followed by completed-record restart equality, passed:

```text
ACTUAL submitted prompt (220 UTF-8 bytes):
Todo work-2: Draft a project update

Write one short sentence using all the acceptance and context facts.

Acceptance: Include the exact word SUNFLOWER.

Context 1: The deadline is Friday. Include Friday in the sentence.
ACTUAL Astra/high answer: The SUNFLOWER project deadline is Friday.
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 164.32s
```

The first fixture correction was the expected HTTP status: the existing API
carries `store-code: refused` as HTTP 502. No kernel defect was evidenced.
Final-head full gates and rebuilt-artifact browser evidence follow in the delivery
receipt; this early checkpoint is not substituted for them.

A terminal live session is not mislabeled as a running worker while the Todo
waits for its durable ending. The component proof first failed with
`1 failed | 3 passed (4)` and the missing text
`The session has ended. Waiting for the Todo’s saved outcome.`; the label now
uses the observed session status and leaves acceptance disabled until the saved
dispatch reports success.
