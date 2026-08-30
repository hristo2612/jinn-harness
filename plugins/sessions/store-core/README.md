# `store-core` — the guest-side store, once

Not a crate. One source file, included by BOTH store providers as a
module:

```rust
#[path = "../../store-core/store.rs"]
mod store;
```

## Why it is shared source and not a crate

A guest generates its OWN `wit_bindgen::generate!` bindings — `clock`,
`events`, `services` are per-crate types, so a normal library crate
cannot call them on the guest's behalf. Everything that is not a host
call already lives in the definition (`jinn-session`: the registry, the
journal's record law, the engine translation). What is left is the part
that MAKES those host calls, and it is identical in both stores. Copying
it would be two homes for one fact (AGENTS.md standing order 5) and two
places for a defect to be fixed in one of.

So the two providers differ in exactly what they are supposed to differ
in: where the records live.

| Provider | `JOURNAL` | `DURABLE` |
|---|---|---|
| `jinn-session-memory` | every hook a no-op | `false` |
| `jinn-session-fs` | one append-only JSONL document per session over `jinn:fs` | `true` |

## What an including crate must supply

- `PROVIDER: &str` — the package name `describe` reports.
- `DURABLE: bool` — the store's own declaration.
- `mod journal` with `created`, `turn_started`, `turn_ended`, `closed`,
  and `adopt_all` — the five points where a durable store writes and
  reads. Every one answers `Result<(), SessionError>`; a memory store
  answers `Ok(())` and writes nothing.
