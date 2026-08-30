# The todos seam

The company's work ledger as a capability on the kernel — the fifth
core-port seam under the malleability contract (phase 2.5), and the first
that is THREE layers deep. Roles per the seam-triple naming law
(AGENTS.md):

| Role | Package | What it is |
|---|---|---|
| Service definition | `jinn-todo` | The `jinn:todo.<store-id>` contract: the Todo spec, the typed status and its explicit legal-transition table, the events on `jinn:todo/event`, the `create`/`update`/`comment`/`get`/`list`/`tree`/`dispatch`/`events` operations, the append-only journal's record law and its honest replay, the pure translation to the sessions seam, and `Todos` — the registry, the status law and the fold every store shares. Owns the `todos` settings namespace. Pure types + logic. |
| Provider | `jinn-todo-fs` | Durable: one append-only JSONL journal per Todo over `jinn:fs`, a replay on activate that recovers what the daemon left behind, and the two repairs adoption owes (a healed tail, a recorded recovery). |
| Provider | `jinn-todo-memory` | Ephemeral: nothing outlives the incarnation. A genuine use (throwaway and test ledgers) that doubles as the swap proof and needs no `jinn:fs` grant at all. |
| (shared source) | `store-core/store.rs` | Not a crate — ONE source file both providers include as a module. Everything that is a host call and is identical in both. See its README for why a library crate cannot hold it. |
| Consumer | `jinn-api-http` (`plugins/api/`) | Exposes Todos over the operator API: record, read, move a status, comment, dispatch, read the tree, read the event feed, list. |

## Todo over session over engine — no layer names the next one's provider

**A Todo store never opens a session, and never touches an engine.** Both
providers INJECT the sessions seam's DEFINITION — they resolve
`jinn:session.<store>` from the dispatch's own spec
(`jinn_todo::dispatch::session_contract`) and drive whatever answers. The
session, in turn, resolves `jinn:engine.<id>` from the binding it was
created with. So:

```
jinn:todo.<store>  ->  jinn:session.<store>  ->  jinn:engine.<id>
```

- Changing a dispatch's `engine` field runs the SAME Todo on a different
  engine provider. Neither store is touched and neither knows which
  provider answered.
- Changing its `store` field sends the same Todo to a different session
  store.
- Swapping either store provider is a profile edit, and leaves every
  other layer untouched.

The layering is enforced by AUTHORITY, not by good behaviour: a Todo
store's profile entry is granted no `jinn:engine.<id>` at all
(`tools/todo-kit`), so it could not reach an engine if its code tried.

## The status law

A Todo's status is the company's claim about a piece of work, so the
moves are ENUMERATED — in one place, exhaustively — and everything not
enumerated is refused naming the attempt. Three laws are encoded in that
table (`jinn-todo/src/status.rs`):

- **A producer does not close their own work.** `executing -> done` is
  not a move; the route is `executing -> in-review -> done`.
- **A terminal status is terminal.** `done` and `cancelled` have no
  exits. The honest way back is a new Todo linked to the old one.
- **A status change is a change.** `x -> x` is in no row.

A refused move is typed (`refused`, with `from` and `to` as DATA beside
the message), and it is RECORDED — journal line, bus event, and a row on
the Todo — before the caller is told. `Todos::update` answers
`Moved::Refused` carrying the record, so there is no code path that
refuses without recording.

## Honesty after a crash

A journal is what a store has after a crash, and a crash is exactly when
a system is tempted to lie. The reader is built so the DANGEROUS answer
needs proof: a dispatch reads back `done` only where a terminal record was
written, a started dispatch with no ending replays `interrupted` with a
reason, and `running` is minted only by the live registry — a replay
cannot produce it at all. A `status-changed` line the table does not
admit, or one starting from a status the Todo was never in, is REFUSED as
corruption.

**A Todo is therefore never eternally `executing`.** Adoption folds an
interrupted dispatch onto `blocked` and then RECORDS that fold as a real
status-changed line, so the status a reader is shown and the status the
ledger will act on are the same one. The recovery is a NEW event appended
after the ones already there — never an edit — and it carries the
dispatch's reason as its note and no actor, because nobody asked for it.
The whole history stays readable: an operator can see both that the work
was started and that the daemon died on it.

## Known limits

Named here rather than left for a reader to discover:

- **One unreplayable journal takes the whole durable store down**, for
  the reason and with the shape the sessions seam names
  (`plugins/sessions/README.md`): it fails CLOSED, and a per-document
  quarantine is the better shape and is not built.
- **A healed tail is a full rewrite.** Dropping a torn tail costs the
  whole document's bytes, because `jinn:fs` cannot drop a suffix —
  `FINDINGS.md` #34.
- **Latency compounds per layer.** A Todo's answer is visible one
  todo-poll after the session's, which is one session-poll after the
  engine's. `FINDINGS.md` #35, and the reason is #4 and #32.
- **The event feed is a cursor read, not a push.** Same reason.
- **The ring is bounded and reports what it dropped.** A Todo past
  `EVENT_RING` events loses the oldest from the feed; the record and the
  journal are unaffected.
- **A comment cannot be edited or removed, and neither can a Todo.**
  There is no DELETE on the surface: the ledger's ending is `cancelled`,
  recorded. That is deliberate, and it means an operator who records the
  wrong thing lives with it beside its correction.
- **Two concurrent writers to one Todo are not proven.** See
  `FINDINGS.md`'s "What the todos seam could NOT prove".

Guest crates here are NOT workspace members (see the workspace manifest's
note); `todo-kit` builds them into the todos profile. Real-composition
proof lives in `tests/composition/tests/todos.rs`.
