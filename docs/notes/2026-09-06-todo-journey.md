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
