# `jinn:engine` — contract surface

The engines seam's service definition. Roles and the reasoning behind the
instanced contract name live one level up (`plugins/engines/README.md`) and
in `docs/notes/2026-08-29-engines-seam.md`; this file is the contract.

## Names

| Name | Value |
|---|---|
| Contract | `jinn:engine.<engine-id>` — one provider slot per engine id |
| Event topic | `jinn:engine/event` — every provider, one topic |
| Settings namespace | `engines` |
| Envelope version | `0.1` (additive within `0.x`) |

The engine id is read from the provider entry's `config.data.engine` and
written nowhere else.

## Operations

Every answer is an `Answer`: `{ "api-version": "0.1", "ok": <value> }` or
`{ "api-version": "0.1", "error": { "code": ..., "message": ... } }`.
Callers classify by the error's CASE — `invalid`, `not-found`, `refused`,
`unavailable`, `failed` — never by folding a message. `unavailable` is the
honest environment gate: the provider is mounted and correct, this host
cannot carry the run (no CLI, no authentication). A run is never faked.

| Operation | Request | Answer |
|---|---|---|
| `describe` | — | `Description`: the engine id, the package serving it, the models, and declared `capabilities` (`streaming`, `tool-calls`, `cancel`, `usage`, `external-cli`) |
| `run` | `RunRequest` | `RunAccepted { run-id, engine, model }` — at once; the run's progress arrives on the bus |
| `run-get` | `{ "run-id": ... }` | `RunRecord` — state, status, usage, the assembled answer, every event so far |
| `cancel` | `CancelRequest { run-id }` | the run's record; the child is killed |

### `RunRequest`

```json
{ "api-version": "0.1", "engine": "default", "model": null, "effort": null,
  "prompt": "one line", "cwd": null,
  "tools": { "mode": "denied", "allow": [] },
  "budget": { "wall-ms": 120000, "output-bytes": 1048576 },
  "secrets": { "SOME_API_KEY": { "$secret": "engines/some-key" } } }
```

`engine` is the ROUTE: it names the contract. `tools` is **default-deny** —
an absent policy admits no tool, never "whatever the CLI defaults to".
`budget` bounds both wall clock and read bytes, and the provider enforces
both (R9). `secrets` maps a child environment variable to a keystore KEY
NAME — the settings seam's typed `{"$secret": ...}` shape, reused, so
secret references have one home. Secret material never appears in a
request, a profile document, a ledger payload, or this repo.

The prompt is delivered to the child on **stdin** by every provider in this
seam, never in argv: argv is world-readable in the host's process table.

### Run events

One `RunEvent` per bus message: `{ "api-version", "engine", "run-id",
"seq", "kind", ... }`, where `seq` counts from 0 per run — a listener
orders and de-duplicates on it, never on arrival. The envelope carries no
extension map, because `kind` is an internally tagged enum and a second
flattened map would swallow its own fields on the way back in. Forward
compatibility lives in the kinds instead: a `kind` this version does not
know decodes as `unknown` and is counted, never dropped and never guessed.

| `kind` | Fields |
|---|---|
| `started` | `model` |
| `delta` | `text` — a chunk of the answer |
| `tool-call` | `name`, `input` |
| `tool-result` | `name`, `ok` |
| `turn-end` | `text` — the whole answer, for engines that report one instead of deltas |
| `exited` | `status` (negated signal number for a signal death, the `jinn:process` convention), `usage`, `truncated` |
| `cancelled` | `reason` — `cancel`, `budget`, or the provider's own |

`exited` comes from the process's real `wait` status, never from the
stream: a codec reports what an engine SAID, the kernel reports what the
child DID.

## `Runs` — the shared registry

Providers do not each implement run semantics. `Runs` mints run ids
(`<engine>-<n>`, monotone within an incarnation), sequences events,
assembles the answer from deltas or from a lone `turn-end`, accounts both
budgets, tracks terminal state, and bounds how many finished records are
kept. It is pure — it takes the kernel's `now` rather than reading a clock
— so the seam's run semantics are one implementation with one set of
tests, and a provider adds only its argv and its stream codec.

## Listing engines

`engines_in` turns `(entry-id, provisions)` pairs — as `jinn:introspect`
reports them — into the engines a composition holds. The list is the
KERNEL's knowledge, not a table a consumer keeps: an engine appears
because an entry provides its contract, and disappears when the entry
does.
