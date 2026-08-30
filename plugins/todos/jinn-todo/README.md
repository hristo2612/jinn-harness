# `jinn-todo` — the todos seam's service definition

The `jinn:todo.<store-id>` contract: its vocabulary, its laws, and the
logic every store provider shares. Pure types and pure functions — no
host call — so the seam's semantics have ONE implementation with one set
of tests, and a provider adds only where the records live.

The seam's shape, its layering and its known limits have one home in
`plugins/todos/README.md`. This file is the CONTRACT SURFACE.

## The contract name

`jinn:todo.<store-id>`. The kernel holds one provider slot per contract
name, so N stores coexisting means N names (`FINDINGS.md` #29). The store
id is read from the provider entry's own `config.data.store` and written
nowhere else, which makes switch, coexistence and extension all profile
edits.

## Operations

Every payload and answer is UTF-8 JSON with kebab-case keys. An answer is
`{ "api-version", "ok" }` or `{ "api-version", "error" }` — never both.

| Operation | Takes | Answers |
|---|---|---|
| `describe` | nothing | what this store is, and whether it is `durable` |
| `create` | `{ spec }` | `{ todo-id, store, status }` |
| `get` | `{ todo-id }` | the Todo's record |
| `update` | `{ todo-id, status, note?, actor? }` | the record, or a typed refusal naming `from`/`to` |
| `comment` | `{ todo-id, body, actor? }` | the record |
| `dispatch` | `{ todo-id, dispatch, actor? }` | the record, with the dispatch running |
| `list` | `{ status?, department?, parent?, roots-only? }` | summaries, plus `total` before the filter |
| `tree` | `{ todo-id }` | the Todo and everything parented beneath it |
| `events` | `{ todo-id, after?, limit? }` | one page of the feed, plus `dropped` |

Events go on `jinn:todo/event`, one topic for the whole seam: `created`,
`status-changed`, `transition-refused`, `commented`, `dispatched`,
`dispatch-ended`, `closed`.

## The status value space and its table

`backlog | executing | in-review | blocked | done | cancelled`. CLOSED: a
status this version cannot name is refused, never folded onto a
neighbour. The legal moves, exhaustively:

| From | May move to |
|---|---|
| `backlog` | `executing`, `blocked`, `cancelled` |
| `executing` | `in-review`, `blocked`, `cancelled` |
| `in-review` | `done`, `executing`, `blocked`, `cancelled` |
| `blocked` | `executing`, `backlog`, `cancelled` |
| `done` | — |
| `cancelled` | — |

`executing -> done` is absent on purpose (a producer does not close their
own work); the terminal rows are empty on purpose; and no row contains
its own status.

## The journal's record law

One append-only JSONL document per Todo. Line kinds: `created`,
`status-changed`, `commented`, `dispatch-started`, `dispatch-ended`,
`transition-refused` — a CLOSED space, because a journal whose unknown
lines were skipped would replay a different Todo than it holds.

What a replay may conclude:

- The first record is `created`, or the document is corrupt.
- A `status-changed` must begin where the Todo actually stands and must
  be in the table. Either violated is corruption, REFUSED — the writer
  cannot produce such a line, so a document holding one did not come from
  this seam.
- A `dispatch-started` with no matching ending is `interrupted` with
  `journal::INTERRUPTED_REASON`. A `dispatch-ended` whose status is not
  terminal is REFUSED, so `running` is unreachable from a document by
  construction.
- A torn TAIL — the last line, unterminated — is ABSENCE. A hole anywhere
  earlier is corruption. The two are never answered the same way.

## Two statuses, both named

A record carries `declared-status` (the `to` of the last status line —
history, verbatim) and `status` (what the store reports now). They differ
only while a Todo's dispatch replayed `interrupted` and its recovery has
not yet been recorded; `reported_status` is that fold, in one place, and
`Todos::plan_recovery` is what turns it into a real line — journalled
first, committed after, like every other move. See
`plugins/todos/README.md` for why the fold alone is not enough.

## Attribution

An actor is DECLARED or absent. Absence is recorded as absence and is
never filled in with a transport, a default principal, or the last actor
seen; a present actor that is blank is REFUSED rather than recorded,
because a blank that renders like a principal is exactly the sentinel
this seam's honesty law forbids.

## Additivity

Every wire type carries a rest map and round-trips unknown content
losslessly at every nesting level, proven exhaustively in
`additivity_tests.rs`. The law's one home is `jinn_settings::wire`. The
closed surfaces are the value spaces — `Status`, `DispatchStatus`,
`ErrorCode`, `journal::Kind` — which refuse what they cannot name.
