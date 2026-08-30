# `jinn-workflow` — the workflows seam's service definition

The `jinn:workflow.<store-id>` contract: its vocabulary, its laws, and
the logic every run-store provider shares. Pure types and pure functions
— no host call, and no clock of its own — so the seam's semantics have
ONE implementation with one set of tests, and a provider adds only where
the records live.

The seam's shape, its four-layer composition, its ledger order and its
known limits have one home in `plugins/workflows/README.md`. This file is
the CONTRACT SURFACE.

## The contract name

`jinn:workflow.<store-id>`, from `store_contract`. The store id is read
from the provider entry's own `config.data.store` and written nowhere
else, which makes switch, coexistence and extension all profile edits.
Why the id is encoded in the contract NAME rather than carried beside it
has its one home in `plugins/engines/README.md`.

## Operations

Every payload and answer is UTF-8 JSON with kebab-case keys. An answer is
`{ "api-version", "ok" }` or `{ "api-version", "error" }` — never both.

| Operation | Takes | Answers |
|---|---|---|
| `describe` | nothing | what this store is and what it can do |
| `define` | `{ spec, workflow-id? }` | `{ workflow-id, store, revision, spec-digest }` — an absent `workflow-id` records a new workflow, a present one appends the next revision of it |
| `get` | `{ workflow-id, revision? }` | the workflow's record: its `latest-revision` and its revisions in order, each with its spec, digest, actor and time |
| `list` | nothing | one summary per workflow: `workflow-id`, `name`, `latest-revision`, `spec-digest`, `nodes` |
| `start` | `{ workflow-id, revision?, input, actor? }` | the run's record, carrying the revision it PINNED and that revision's whole spec |
| `get-run` | `{ run-id }` | the run's record |
| `list-runs` | `{ workflow-id?, status? }` | one summary per run, with `nodes-ended` out of `nodes-total`, both counted rather than stored |
| `node-state` | a run, a node, the `state` to move it to, and an optional `note` and `actor` | the run's record, or a typed refusal naming the node and the attempted `from`/`to` |
| `cancel` | `{ run-id, reason, actor? }` | the run's record, ended `cancelled` — a blank reason is refused, because a cancellation nobody can explain is an ending nobody can explain |
| `events` | `{ run-id, after?, limit? }` | one page of the run's feed, plus `dropped` |

The request documents are typed in `src/spec.rs` (`DefineRequest`,
`WorkflowRequest`, `StartRequest`, `RunRequest`, `CancelRequest`,
`ListRunsRequest`, `EventsRequest`); `describe` and `list` take nothing,
and `node-state`'s arguments are `Workflows::plan_node_move`'s, decoded
by the provider that serves the operation. `RunRecord` is the one
run-shaped answer, so every operation that opens, moves or ends a run
answers the same document a read of it would give.

Errors are classified by CASE and never by folding a message:
`invalid | not-found | refused | unavailable | failed`. An illegal
node-state move is `refused`, and carries `node`, `from` and `to` as DATA
beside the message so a caller classifies without parsing prose.

## The node-state value space and its table

`pending | running | done | failed | interrupted | cancelled | skipped`.
CLOSED: a state this version cannot name is refused, never folded onto a
neighbour — the neighbour of `interrupted` might be `done`. The legal
moves, exhaustively (`src/node.rs`):

| From | May move to |
|---|---|
| `pending` | `running`, `skipped`, `cancelled` |
| `running` | `done`, `failed`, `interrupted`, `cancelled` |
| `done` | — |
| `failed` | — |
| `interrupted` | — |
| `cancelled` | — |
| `skipped` | — |

Four laws are encoded there. `pending -> done` and `pending -> failed`
are absent on purpose: a node that never started cannot report how its
work went, and the only endings a pending node has are the ones that say
it never ran. The terminal rows are empty on purpose: a run whose
finished nodes could still change would make every past reading of it
provisional. No row contains its own status, because a state change that
changes nothing would append an event recording that nothing happened.
And `running -> interrupted` is in the table precisely so a node a crash
left running can be RECORDED as ended with a reason.

Every ending but `done` must carry a reason (`NodeState::needs_reason`),
so no reader ever has to invent one. `is_terminal` and the table cannot
disagree: a terminal state is exactly one whose row is empty.

## The run-status value space

`running | done | failed | cancelled | interrupted`. CLOSED, on the same
law. `running` is minted only by the live registry, so a replay cannot
produce it. `done` — the claim that the procedure was carried out — is
derived by `run_ending` and requires that every node which RAN reached
`done`; a `skipped` node is an edge routing past it, which is the graph
working. Every ending but `done` carries a reason.

## Node kinds, edge kinds, and the graph walk

Both are closed value spaces, because a free-string kind is a dispatch
table nobody can enumerate: a reader cannot tell which kinds exist, a
store cannot refuse one it does not implement, and a typo becomes a node
that silently does nothing.

- **`checkpoint`** — the node has no work of its own and ends `done` the
  moment it starts. What makes an entry, a join or an exit expressible
  without pretending they dispatch anything.
- **`dispatch`** — the node dispatches work through the todos seam's
  DEFINITION, carrying a `TodoBinding` of a Todo `store`, the `todo` it
  records there, and the `dispatch` that Todo is sent with. `define`
  refuses a `dispatch` node with no binding, a `dispatch` node whose Todo
  store is blank, and a `checkpoint` node carrying a binding nothing
  would ever dispatch.

Edges say WHEN they are followed: **`always`** whenever the source node
ends however it ended, **`on-done`** only when it ended `done`, and
**`on-not-done`** only when it ended in a state that is not `done` — the
failure lane, named positively rather than as "not the other one".

A node is READY when it is `pending`, every inbound edge is decided, and
at least one of them was followed; a node with no inbound edge at all is
an entry and is ready immediately. It is SKIPPED when every inbound edge
is decided and NONE was followed — skipping is a positive reading of a
decided graph, never a timeout. `define` refuses a graph with a cycle
(Kahn's algorithm, naming the nodes that are in it), a self-edge, an edge
naming a node that is not here, a duplicate node id, and a graph in which
every node has an inbound edge and so a run would have nothing to start
with.

## The typed input schema

A workflow declares its input as fields, each with a `name`, a `kind`
(`string | number | bool`, closed) and whether it is `required`. `start`
checks the run's input against the PINNED revision's schema and refuses a
required field that is absent, a field of the wrong kind, and a field the
schema does not declare — an input a workflow cannot read is a caller
believing something is being used that is not.

That last refusal is not a violation of additivity: additivity is a law
about a WIRE type's unknown fields, and these are a caller's arguments.
The schema document itself carries a rest map like every other; the
arguments it declares stay closed.

## Events

One topic for the whole seam, `jinn:workflow/event`. Each event carries
`store`, `run-id` and a `seq` that counts from 0 per run, so a listener
orders and de-duplicates on the sequence rather than on arrival. The
kinds: `defined`, `run-started`, `node-started`, `node-ended`,
`node-transition-refused`, `run-ended`.

`defined` and `run-started` both carry the revision, because a listener
given only the workflow could not tell WHICH definition it was told
about. `node-ended` carries the node's own outcome state and its reason.
`node-transition-refused` is on the bus for the same reason it is in the
record: an attempt to claim a step was carried out by a path the table
forbids is something an operator should be able to see.

A kind this version cannot name rides through as `unknown` with its whole
payload kept, rather than being a decode error. That is the opposite of
the journal's answer below, and deliberately so: replaying a different
run than the document holds would be a lie, but a listener that skipped
an event it could not read is simply told less than happened.

## The journal's record law

One append-only JSONL document per workflow (its revisions) and one per
run (its whole life). Line kinds: `defined`, `run-started`,
`node-state-changed`, `node-transition-refused`, `run-ended` — a CLOSED
space, because a journal whose unknown lines were skipped would replay a
different run than it holds.

What a replay may conclude:

- A WORKFLOW document holds `defined` lines and nothing else. Revisions
  are consecutive from 1, and each must match its own digest — a
  disagreement means one of the two was not written by this seam.
- A RUN document opens with `run-started`, which carries the pinned
  revision and its whole spec. The run's nodes ARE that spec's nodes;
  nothing else could be, since the definition may have been edited since.
- A `node-state-changed` must begin where the node actually stands and
  must be in the table. Either violated is corruption, REFUSED — the
  writer refuses illegal moves, so a document holding one did not come
  from this seam, and believing it would let `done` be reached by a path
  the law forbids. A line naming a node the pinned spec does not contain
  is refused for the same reason.
- A `run-ended` whose status is not terminal is REFUSED: a run that reads
  as live after a restart is a run nothing will ever finish. A second
  ending after one has landed is refused, and so is a node move after the
  run ended.
- A torn TAIL — the last line, written short — is ABSENCE, because a
  half-written record must read as "absent or complete" and never as a
  damaged one. A hole anywhere EARLIER is corruption and is refused. The
  two are never answered the same way, and the count of tail bytes read
  as absence is reported rather than swallowed.

`Replayed::open_nodes` and `Replayed::run_open` are what a store's
recovery keys on, so the obligation is a value the caller holds rather
than a rule it has to remember.

## The pin, as a reader sees it

`RunRecord::definition_revision` is which revision the run executes,
`RunRecord::spec_digest` is that revision's label, and `RunRecord::spec`
is the revision's spec carried WHOLE — a run therefore executes correctly
even if every stored revision were dropped. The reasoning, and what the
digest is and is not, have their one home in `src/revision.rs`.

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
closed surfaces are the value spaces — `NodeState`, `RunStatus`,
`NodeKind`, `EdgeKind`, `FieldKind`, `ErrorCode`, `journal::Kind` —
which refuse what they cannot name.
