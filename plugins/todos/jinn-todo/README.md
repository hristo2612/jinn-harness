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

## Conditional operator writes (UI-4a)

A current record carries `revision`: one-based durable mutation order. Each
created/status/refusal/comment/dispatch-start/dispatch-end record advances it;
replay counts the same complete records, including recovery status writes. It is
neither `api-version` nor the bounded event cursor. Old JSONL remains readable;
unknown fields on known records remain carried through their history. An old
wire record without a revision round-trips without a fabricated zero field.

`update`, `comment` and `dispatch` accept optional `expected-revision`. A mismatch
returns typed `refused` before persistence or dispatch side effects. Old callers
that omit it retain their previous unconditional semantics. The UI always sends
it. Conditional comments cannot append to terminal Todos. Ordinary illegal status
moves still append their refused-attempt record; stale writes append nothing.
Conditional blocked/needs-work status decisions require a nonblank note.

Conditional `update` to `in-review` or `done` also requires `reviewed-dispatch`,
matching the latest dispatch and its successful `done` ending. A running or
failed latest attempt cannot be accepted using an older success. The status law
still applies. The resulting status history saves `reviewed-dispatch` and
`inspected-revision`; a new status write invalidates the previous inspection.
This is an operator's review of model text, not authentication of separate roles.

Optional `client-request` is an audit marker carried into the resulting comment,
status change or dispatch. It is **not** an idempotency key. A dropped response is
reconciled by reading the record; an unresolved write must not be resent
automatically. Create uses the existing `spec.metadata` for its marker. HTTP
callers use the wrapped `{dispatch, expected-revision, client-request}` request
shape so dispatch controls remain on the outer request.

The default task brief includes captured text, comments in sequence order and
nonblank recorded decision notes. The **complete UTF-8 prompt** is limited to
32 KiB, checked before any new dispatch/status/session side effect; nothing is
silently truncated. Legacy explicit `dispatch.message` still overrides the brief,
subject to the same complete-prompt bound. Tools remain denied by the existing
session default. The UI fixes the task binding; the general service remains
provider-neutral.

Session/turn links are live until the terminal dispatch is journaled. On active
restart a Todo may have no live link or partial answer: it reports interrupted
and records its blocked recovery. Completed answers and their links survive.
The UI reads a known live session for partial text and Stop, and the durable
terminal dispatch for saved results. Model completion never advances Todo status.

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
