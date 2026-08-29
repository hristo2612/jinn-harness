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
`budget` bounds wall clock and ANSWER BYTES, and both are enforced in one
place — `Runs::record_all`, the single path from a provider's events to
the bus. A text event is charged against the run's remaining allowance and
**clipped to it before it is emitted**, so `output-bytes` is a bound on
what reaches the bus and the consumer, not a count taken afterwards. Past
it the answer is a prefix, the cut is a `truncated` event, and the
provider stops reading and kills the child (R9). `secrets` maps a child environment variable to a keystore KEY
NAME — the settings seam's typed `{"$secret": ...}` shape, reused, so
secret references have one home. Secret material never appears in a
request, a profile document, a ledger payload, or this repo.

The prompt is delivered to the child on **stdin** by every provider in this
seam, never in argv: argv is world-readable in the host's process table.

### Run events

One `RunEvent` per bus message: `{ "api-version", "engine", "run-id",
"seq", "kind", ... }`, where `seq` counts from 0 per run — a listener
orders and de-duplicates on it, never on arrival. The envelope declares no
rest map of its own because the flattened event already is one (see
[Additivity](#additivity)); a `kind` this version does not know decodes as
`unknown`, keeps its tag and its whole payload, and is counted — never
dropped and never guessed.

| `kind` | Fields |
|---|---|
| `started` | `model` |
| `delta` | `text` — a chunk of the answer |
| `tool-call` | `name`, `input` |
| `tool-result` | `name`, `ok` |
| `turn-end` | `text` — the whole answer, for engines that report one instead of deltas |
| `exited` | `status` (negated signal number for a signal death, the `jinn:process` convention), `usage`, `truncated`, `error` |
| `cancelled` | `reason` — `cancel`, `budget`, or the provider's own |
| `truncated` | `limit-bytes`, `read-bytes` — the output budget is spent and the answer is a prefix |

`exited` comes from the process's real `wait` status, never from the
stream: a codec reports what an engine SAID, the kernel reports what the
child DID.

## Additivity

**The law.** For every type and every variant here, known or unknown, at
every nesting depth, decode-then-encode is lossless for content this
schema does not know. A field a newer peer sends rides through an older
hop untouched.

**The mechanism, implemented once.** Every wire type carries a rest map
named `extra` holding verbatim whatever its version did not read. Derived
types get it from serde's `flatten`; `Event` and `Answer`, whose tags
forbid a derive, get it from the shared `decode_with_rest` /
`encode_with_rest` pair — the same law written out, not a second
algorithm. `Event` is therefore a struct of an `EventKind` and ONE rest
map rather than a rest map per variant, so a kind added later cannot be
the next place that forgets. The `Additive` trait reaches every type's
rest map uniformly, and the property test plants unknown keys at random
depths through the whole inventory rather than checking a table of
examples.

**The named non-additive surfaces.** Two, deliberately, and nothing else.
A closed surface **REFUSES** what it cannot name: it never drops it and
never guesses. A surface that quietly discarded an unknown field would be
the same silent-wrong-answer defect additivity exists to prevent, only
with a README entry as its disguise — the sender is told the document was
understood. Both refusals go through the ONE shared
`jinn_settings::closed`, so the error always names the surface that
refused (`every_closed_surface_names_itself_when_it_refuses`).

| Surface | Why it is closed | What happens instead |
|---|---|---|
| `{"$secret": "<key>"}` | The settings seam's own shape — a reference carrying extras is not a reference. One home per fact: it is not this seam's to widen. And preservation would be the WRONG answer here even if it were: an unknown key must never ride along beside a credential name, which makes refusal a security property rather than a schema preference. | The decoder itself refuses, naming the surface and the sibling key. The request never half-reads. |
| The closed value spaces — `effort`, `tools.mode`, `state`, `code` | An enum has nowhere to put a value it cannot name, and guessing which known value a future `effort: "ultra"` meant is the silent-wrong-answer shape this seam forbids. | A LOUD decode error naming the surface, the value, and what the surface admits. Never a default, never a drop. |

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
