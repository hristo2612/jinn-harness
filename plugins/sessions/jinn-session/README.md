# `jinn:session` — contract surface

The sessions seam's service definition. Roles, the composition with the
engines seam, and the reasoning behind the instanced contract name live
one level up (`plugins/sessions/README.md`); this file is the contract.

## Names

| Name | Value |
|---|---|
| Contract | `jinn:session.<store-id>` — one provider slot per store id |
| Event topic | `jinn:session/event` — every provider, one topic |
| Settings namespace | `sessions` |
| Envelope version | `0.1` (additive within `0.x`) |

The store id is read from the provider entry's `config.data.store` and
written nowhere else.

## Operations

Every answer is an `Answer`: `{ "api-version": "0.1", "ok": <value> }` or
`{ "api-version": "0.1", "error": { "code": ..., "message": ... } }`.
Callers classify by the error's CASE — `invalid`, `not-found`, `refused`,
`unavailable`, `failed` — never by folding a message.

| Operation | Request | Answer |
|---|---|---|
| `describe` | — | the store id, the package serving it, and its durability |
| `create` | `{ "spec": SessionSpec }` | `SessionCreated { session-id, store, engine }` |
| `send` | `{ "session-id", "message" }` | `TurnAccepted { session-id, turn-id }` — at once; the turn's progress arrives on the bus |
| `get` | `{ "session-id" }` | `SessionRecord` — status, turns, the whole log |
| `messages` | `{ "session-id", "offset", "limit" }` | `Page` — turns from `offset`, `next-offset` present only when there IS a next page |
| `list` | `{ "owner"?, "engine"? }` | the store's `SessionSummary` list |
| `cancel` | `{ "session-id" }` | the record; the turn in flight ends `cancelled` |
| `close` | `{ "session-id" }` | the record; `send` is refused afterwards |

### `SessionSpec`

```json
{ "api-version": "0.1",
  "engine": { "engine": "echo", "model": null, "effort": null },
  "cwd": null,
  "tools": { "mode": "denied", "allow": [] },
  "attribution": { "owner": "operator" },
  "metadata": {} }
```

`engine` is a binding to the engines seam's DEFINITION, never to a
provider: the store turns `engine` into `jinn:engine.<id>` through
`jinn_engine::engine_contract` and drives whatever answers. `tools` is the
engines seam's own default-deny policy, borrowed rather than copied.
Secret material never appears here; a run's secrets are the engines
seam's `{"$secret": "<key>"}` references, resolved by the ENGINE provider.

## Statuses — and what each one claims

A turn's `status` is a CLOSED value space, and its cases are ranked by how
much they claim:

| `status` | What it claims | What it needs |
|---|---|---|
| `running` | the turn is in flight right now | a live turn in THIS incarnation. Minted only by `Sessions::send`; a journal replay cannot produce it |
| `done` | the engine finished and `answer` is whole | a `turn-ended` record with `status: done` actually written |
| `failed` | the engine ran and failed | a terminal record, and a `reason` |
| `cancelled` | a caller stopped it | a terminal record, and a `reason` |
| `interrupted` | the daemon stopped mid-turn; how far it got is not recorded | nothing — it is the DEFAULT a started turn falls to |

A session's `status` (`idle`, `running`, `closed`, `failed`) is DERIVED
from its turns and never stored, so the two cannot drift.

## The journal

One append-only JSONL document per session; each line a `Record` whose
`kind` is `created`, `turn-started`, `turn-ended` or `closed`. The line is
newline-TERMINATED, and that terminator is what makes a short write
detectable.

`replay` reads it under one rule: **a claim is derived from proof, never
from the absence of a contradiction.**

- Every `turn-started` opens its turn as `interrupted`, carrying
  `INTERRUPTED_REASON`. Only a `turn-ended` record moves it, and only to a
  TERMINAL status: `Record::turn_ended` refuses to write a non-terminal
  one, and `replay` refuses to read one back. Both halves are needed — the
  writer's refusal makes `running` unwritable by this seam, and the
  reader's makes it unreachable from a document this seam did not write
  (a corrupted byte, a half-migrated log). `running` is therefore
  impossible from a replay by CONSTRUCTION rather than by the writer's
  good behaviour alone.
- A terminal ending that carries no `reason` does not erase the one the
  started turn already had: absence of a reason is not proof that there
  was none, so the conservative one stands. `done` is the exception and
  needs none — its whole claim is the answer itself.
- A trailing unterminated line is a torn TAIL and reads as ABSENCE (the
  half-written turn is simply not there), with its byte count reported
  rather than swallowed. An undecodable line ANYWHERE ELSE is a hole, not
  a tear, and is REFUSED — answering the two the same way would let real
  corruption pass for a clean stop.
- A `turn-ended` naming a turn that never started is refused.

The kernel's `jinn:fs` `append` commits whole-document atomically (stage +
fsync + rename — `FINDINGS.md` #22, closed at pin `3fd7b05`), so a tear
should be unreachable through that path. The reader does not rely on it:
the guarantee belongs to a contract this seam does not own.

## Additivity

The distribution's wire law, whose one home is `jinn_settings::wire`:
every wire type carries a rest map (`extra`) and a decode → encode round
trip is lossless for content this version cannot read. This seam proves it
EXHAUSTIVELY — an unknown key is planted at every object node of every
canonical document — rather than by sampling, which its small inventory
allows.

The named non-additive surfaces are the CLOSED value spaces (`status` for
both turns and sessions, the error `code`, and the journal's `kind`) and
the settings seam's `{"$secret": ...}` reference. Closed means REFUSES,
through the one shared `jinn_settings::closed`, so unknown content is a
loud error naming the surface — never a drop, never a guess.

## `Sessions` — the shared registry

Stores do not each implement session semantics. `Sessions` mints session
and turn ids, sequences events, enforces the state machine (one turn in
flight, no `send` to a closed session, a non-`done` ending must carry a
reason), derives status, and pages the log. It is pure — it takes the
kernel's `now` rather than reading a clock — so the seam's semantics are
one implementation with one set of tests, and a store adds only where the
records live.

## Listing stores

`stores_in` turns `(entry-id, provisions)` pairs — as `jinn:introspect`
reports them — into the stores a composition holds: the KERNEL's
knowledge, not a table a consumer keeps.
