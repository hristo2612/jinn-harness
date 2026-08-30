# `store-core` — the guest-side Todo store, once

Not a crate. One source file, included by BOTH store providers as a
module:

```rust
#[path = "../../store-core/store.rs"]
mod store;
```

## Why it is shared source and not a crate

For the reason the sessions seam's `store-core` is, and the reasoning has
one home there (`plugins/sessions/store-core/README.md`): a guest
generates its OWN `wit_bindgen::generate!` bindings, so a library crate
cannot make host calls on the guest's behalf. Everything that is not a
host call already lives in the definition (`jinn-todo`: the registry, the
status table, the journal's record law, the session translation). What is
left is the part that MAKES those host calls, and it is identical in both
stores.

So the two providers differ in exactly what they are supposed to differ
in: where the records live.

| Provider | `journal` | `DURABLE` |
|---|---|---|
| `jinn-todo-memory` | every hook a no-op | `false` |
| `jinn-todo-fs` | one append-only JSONL document per Todo over `jinn:fs` | `true` |

## What an including crate must supply

- `PROVIDER: &str` — the package name `describe` reports.
- `DURABLE: bool` — the store's own declaration.
- `mod journal` with `created`, `status_changed`, `transition_refused`,
  `commented`, `dispatch_started`, `dispatch_ended`, and `adopt_all` —
  the seven points where a durable store writes and reads. Every one
  answers `Result<(), TodoError>`; a memory store answers `Ok(())` and
  writes nothing.

## The order every hook is called in

A store calls its journal BETWEEN the definition's two halves, and never
in any other order:

```rust
let planned = with_todos(|todos| todos.plan_update(..))?;   // touches nothing
journal::status_changed(todo_id, &change, now)?;            // durable, or the call ends here
with_todos(|todos| todos.commit_change(todo_id, &change));  // now the store reports it
```

That is what makes the state this store reports the state its log holds.
A refused append answers the caller typed and moves nothing a reader can
see; a restart then replays exactly what the live view was already
saying. The definition has no method that advances state and writes
nothing, so there is no shorter path to get this wrong
(`jinn_todo::Todos`, module doc).

Note `transition_refused`: a REFUSED move is one of the seven, because
the attempt is a fact this seam records. A store that only wrote the
moves it allowed would leave an operator unable to see that something
tried to close work by a path the table forbids.
