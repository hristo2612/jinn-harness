# `store-core` — the guest-side run store, once

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
host call already lives in the definition (`jinn-workflow`: the run
registry, the node-state table, the graph walk, the journal's record law,
the revision pin, the Todo translation). What is left is the part that
MAKES those host calls, and it is identical in both stores.

So the two providers differ in exactly what they are supposed to differ
in: where the records live.

| Provider | `journal` | `DURABLE` |
|---|---|---|
| `jinn-workflow-memory` | every hook a no-op | `false` |
| `jinn-workflow-fs` | one append-only JSONL document per workflow and per run over `jinn:fs` | `true` |

## What an including crate must supply

- `PROVIDER: &str` — the package name `describe` reports.
- `DURABLE: bool` — the store's own declaration.
- `mod journal` with `defined`, `run_started`, `node_state_changed`,
  `node_transition_refused`, `run_ended`, and `adopt_all` — the six
  points where a durable store writes and reads. Every one answers
  `Result<(), WorkflowError>`; a memory store answers `Ok(())` and
  writes nothing.

## The order every hook is called in

A store calls its journal BETWEEN the definition's two halves, and never
in any other order:

```rust
let moved = with_workflows(|w| w.plan_node_move(..))?;      // touches nothing
journal::node_state_changed(run_id, &change, &node)?;       // durable, or the call ends here
with_workflows(|w| w.commit_node_change(run_id, &change));  // now the store reports it
```

That is what makes the state this store reports the state its log holds.
A refused append answers the caller typed and moves nothing a reader can
see; a restart then replays exactly what the live view was already
saying. The definition has no method that advances state and writes
nothing, so there is no shorter path to get this wrong
(`jinn_workflow::Workflows`, module doc).

Note `node_transition_refused`: a REFUSED move is one of the six, because
the attempt is a fact this seam records. A store that only wrote the
moves it allowed would leave an operator unable to see that something
tried to claim a step was carried out by a path the table forbids.

## The activation order is part of the contract

`store::activate` reads the config, builds the registry, calls
`journal::adopt_all`, and then records what every adopted run owes
(`Workflows::plan_recovery`) — a `node-state-changed` line per node the
document left declared `running`, and a `run-ended` line for the run.
Only after all of that may the including crate call `services::provide`.

A recovery append that FAILS fails the activation. A store that cannot
record an ending must not serve a `running` no durable line justifies:
the alternative is a run that reads as live to every caller and can never
finish. `running` therefore exists in a store's memory for the length of
one `activate`, before a single caller can reach it, and never
afterwards.
