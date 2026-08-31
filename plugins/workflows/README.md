# The workflows seam

The company's reusable HOW as a capability on the kernel — the sixth
core-port seam under the malleability contract (phase 2.6), and the first
that is FOUR layers deep. A Workflow is the procedure that outlives any
single run of it; a RUN is one execution of one revision of that
procedure. Roles per the seam-triple naming law (AGENTS.md):

| Role | Package | What it is |
|---|---|---|
| Service definition | `jinn-workflow` | The `jinn:workflow.<store-id>` contract: the workflow spec (its nodes, its edges and its typed input schema), the node-state law and its explicit legal-transition table, the run-status value space, the events on `jinn:workflow/event`, the `describe`/`define`/`get`/`list`/`start`/`get-run`/`list-runs`/`node-state`/`cancel`/`events` operations, the append-only journal's record law and its honest replay, the run-to-revision pin, the pure translation to the todos seam, and `Workflows` — the run registry, the graph walk and the ledger discipline every store shares. Owns the `workflows` settings namespace. Pure types + logic. |
| Provider | `jinn-workflow-fs` | Durable: one append-only JSONL document per workflow (its revisions) and one per run (its whole life) over `jinn:fs`, a replay on activate, and the recovery it owes for every run a crash left open. |
| Provider | `jinn-workflow-memory` | Ephemeral: nothing outlives the incarnation. A genuine use (throwaway and test run stores) that doubles as the swap proof and needs no `jinn:fs` grant at all. |
| Consumer | `jinn-api-http` (`plugins/api/`) | Exposes workflows over the operator API: define a workflow, read it, list, start a run, read a run, list runs, move a node's state, cancel, read the event feed. |

## Workflow over Todo over session over engine

**A run store never records a Todo itself, never opens a session, and
never touches an engine.** A `dispatch` node carries a `TodoBinding`; the
store turns that binding's `store` field into the todos seam's contract
name through THAT seam's own definition (`jinn_todo::store_contract`, via
`jinn_workflow::dispatch::todo_contract`) and drives whatever answers.
The Todo store in turn resolves `jinn:session.<store>` from the dispatch
binding it was handed, and the session resolves `jinn:engine.<id>` from
the binding it was created with. So:

```
jinn:workflow.<store>  ->  jinn:todo.<store>  ->  jinn:session.<store>  ->  jinn:engine.<id>
```

Each layer injects the DEFINITION below it and never a provider. Changing
the Todo store a node records in is one field of its `TodoBinding`;
changing the session store is one field of that binding's own
`DispatchSpec`; changing the engine is one field inside that. Swapping
any provider at any layer is a profile edit, and leaves every other layer
untouched.

The layering is enforced by AUTHORITY, not by good behaviour: a run
store's profile entry is granted its own `jinn:workflow.<store-id>`
contract, one `jinn:todo.<store>` per Todo store its nodes may dispatch
to, and — for a durable store — one `jinn:fs` scope. It is granted no
`jinn:session.<id>` and no `jinn:engine.<id>` at all (`tools/workflow-kit`),
so it could not reach a session or an engine if its code tried.

## The pin

**A run executes ONE revision of one definition, for its whole life, and
reports which.** `define` on a workflow that is already here appends
revision `n + 1`; it never replaces `n`. A `start` resolves "latest"
exactly ONCE, writes the revision it resolved into the run's own
`run-started` line, and carries that revision's whole spec on the same
line. Nothing afterwards re-reads the workflow's current revision on
behalf of a live run, and `get-run` reports `definition-revision` on
every read.

So a definition edited mid-flight cannot reach a run already in flight,
and a reader never has to infer which procedure a run is executing. The
reasoning and the cost this stops paying have their one home in
`jinn-workflow/src/revision.rs`.

## The order a store writes in

Every mutation is `plan_*` (touches nothing), then the journal, then
`commit_*` (folds a record that is already durable). There is no method
in the registry that advances state and writes nothing, so the state a
store reports is the state its log holds: an append that refuses leaves
the reported state exactly where it was, and a restart replays what the
live view was already saying.

A refusal is carried by the same discipline rather than escaping it.
`Workflows::plan_node_move` answers `Moved::Refused` — the typed error
AND the `RefusedChange` its provider records, as one value. There is no
code path that produces the refusal without the record, so "typed and
ledgered" is a property of the type rather than of a provider remembering
to do both. An operator reading the ledger sees the attempt even if the
caller dropped the answer on the floor.

## Honesty after a crash

A journal is what a store has after a crash, and a crash is exactly when
a system is tempted to lie. `NodeState::Running` and `RunStatus::Running`
are minted only by the LIVE registry, for a run this incarnation started
and is driving. A replay reports what the document says — `running`
included — because inventing a line nobody wrote is not a reader's job.

What makes "never eternally `running`" true is the ORDER a durable store
activates in. It replays, plans the recovery (`Workflows::plan_recovery`),
APPENDS a real `running -> interrupted` move for every node the document
left open and a `run-ended` for the run itself, and only THEN provides its
contract. **A store whose recovery append is refused fails to activate
rather than serving a `running` no durable line justifies.** Every one of
those is a NEW record appended after the ones already there, never an
edit of one, so an operator can see both that the work was started and
that the daemon died on it.

A run's ending is derived from its nodes and from nothing else:
`RunStatus::Done` — the claim that the procedure was carried out —
requires that every node which RAN reached `done`. A skipped node is the
graph working and does not spoil that claim; an interrupted, failed or
cancelled one does.

## Known limits

Named here rather than left for a reader to discover.

- **The threat model is ACCIDENTAL conditions**: races, crashes, torn
  writes, a daemon that stopped mid-run. It is NOT an adversary with
  write access to the data root. Someone who can write a store's journal
  can forge a run's history or rewrite a definition, and nothing in this
  seam would detect it as forgery; what the reader catches is damage, not
  deceit.
- **`spec-digest` is a change DETECTOR, not a cryptographic hash.** It is
  a 64-bit FNV-1a over the revision's canonical JSON. Two revisions that
  differ read differently, and an operator can compare a run's digest
  with a definition's at a glance — but the AUTHORITY on what a run
  executes is the spec the run itself pinned, which every run carries
  whole in its own `run-started` line. The digest is a label on it. Its
  stability also rests on `serde_json` rendering map keys in insertion
  order, which this workspace's lockfile gives it.
- **A run is not RESUMED across a restart.** A fresh incarnation drives
  nothing it did not start, so a run the daemon stopped mid-flight is
  recorded `interrupted` and is then terminal — including its nodes that
  had not started yet, which end `interrupted` with the run rather than
  waiting for a driver that will never come. That is the conservative
  answer and it is the recorded one; carrying a run across incarnations
  would mean a store claiming to drive work it has no memory of. Running
  the procedure again is a NEW run, which also pins its own revision.
- **There is no retry and no DELETE.** A terminal node state is terminal
  and a terminal run status is terminal, so the honest way to run a step
  again is a NEW run; the ledger's ending is `cancelled`, recorded with a
  reason. An operator who records the wrong thing lives with it beside
  its correction.
- **The event feed is a bounded ring, and a cursor read rather than a
  push.** A run past `EVENT_RING` events loses the oldest from the feed
  and every page says how many were `dropped`, so a reader is never told
  a gap is quiet. The record and the journal are unaffected.
- **Latency compounds per layer, and this seam adds a fourth.** A run's
  answer is visible one workflow-poll after the Todo's, which is already
  one poll behind the session's and two behind the engine's — the reason
  has its home in `plugins/todos/README.md`.
- **A cycle is refused where a workflow is DEFINED, not where it runs.**
  Nothing in this seam dispatches around a loop, so the kernel's own
  cycle refusal does not reach here; the graph is proven acyclic once, at
  `define`.

Guest crates here are NOT workspace members (see the workspace manifest's
note); `tools/workflow-kit` builds them into the workflows profile, and
that is also where the grant list above is written and tested.
Real-composition proof lives in `tests/composition`. The contract surface
is documented in `jinn-workflow/README.md` — one home per fact.
