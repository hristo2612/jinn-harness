# The pin, and what the fourth layer costs

*Phase 2.6, the workflows seam. Kernel pin `3a8e5c0` (M2-K9), UNCHANGED —
see "Why this seam did not bump the pin" below.*

Three decisions in this packet were not obvious, and one prediction the
repo had been carrying got tested. This note is the reasoning; the facts
themselves live in `plugins/workflows/README.md` and in the code.

## 1. A run is pinned to a revision, and the pin is carried WHOLE

A workflow is the reusable HOW. A run is one execution of it. The moment
those two can be edited independently — and they must be, or a workflow
could never be improved — the question is what a run in flight executes.

The failure mode is not a crash and that is what makes it expensive. An
operator patches a node's prompt, retries the node, and watches the OLD
prompt run, because the run had silently pinned itself and nothing said
so. That is a real cost, paid against the old gateway on 2026-08-30. The
old gateway does the right thing and does it invisibly, so the first time
anyone learns the rule is by losing an hour to it.

So the pin here is three things a reader can see:

1. **Resolved once.** `start` turns an absent `revision` into the latest
   AT THAT MOMENT and writes the number into the run's own `run-started`
   line. Nothing re-resolves it afterwards, and nothing in
   `jinn_workflow::Workflows` consults a workflow's current revision on
   behalf of a live run.
2. **Carried whole.** The `run-started` line holds the revision's entire
   `WorkflowSpec`, not a reference to it. A run's nodes ARE the pinned
   spec's nodes; a state line naming a node outside it is refused by the
   reader as damage. The run would execute correctly if every definition
   document were deleted.
3. **Reported.** `definition-revision` and `spec-digest` are on every read
   of a run. A reader never has to infer which procedure is running, and
   an operator comparing a run against a definition can see at a glance
   whether they are the same one.

`define` on an existing workflow appends revision `n + 1`. It never
replaces `n`, and the reader refuses a document whose revision numbers
skip or repeat — a revision that could be rewritten would make every past
run's pin provisional.

### On the digest

`spec-digest` is a 64-bit FNV-1a over the revision's canonical JSON,
rendered `fnv1a64:<hex>`. It is a CHANGE DETECTOR and this seam never
treats it as more than one. The packet's threat model is honest behaviour
under accidental conditions — races, crashes, torn writes, a daemon killed
mid-node — and explicitly not an adversary with write access to the data
root. The authority on what a run executes is the spec the run carries;
the digest is a label on it, and a revision whose digest disagrees with
its own spec is REPORTED rather than quietly used.

## 2. The recovery is an ORDER, not a fold

The sessions seam shipped a false `running`. The todos seam shipped a
status no durable write justified, and then gave three answers to one
question when its append grant was withdrawn. This is the third layer in a
row to owe ledger honesty, and the standing law is that the third one does
not get to assume it inherited the fix.

The todos seam's answer was a FOLD: a Todo whose dispatch replayed
`interrupted` READS `blocked`, derived at answer time. That was right
there, and it was not enough — `docs/notes/2026-08-30-todos-the-fold-is-not-enough.md`
records why. The declared status still said `executing`, so an operator
shown `blocked` was refused every move `blocked` admits. The fix was to
RECORD the recovery as a real status-changed line.

This seam takes that one step further, and the step is worth naming.

The obvious port would have been for the reader to open a replayed
`node-started` at `interrupted` — the conservative answer, baked into the
replay, no recovery pass needed. That is what an early draft of this seam
did, and it is wrong for a reason that only shows up on the second boot:
the recovery line and the replay would then disagree about where the node
had been, and appending a `running -> interrupted` move onto a document
the reader already reads as `interrupted` refuses on the next replay.
Deriving and recording cannot both own the same fact.

So the reader here reports what the document SAYS, `running` included —
inventing a line nobody wrote is not a reader's job — and the honesty is
moved into the ORDER a durable store activates in:

> replay -> heal -> adopt -> plan the recovery -> APPEND the
> `running -> interrupted` moves and the run's own ending -> **only then**
> `services::provide`.

A store whose recovery append is REFUSED fails to activate. It does not
serve a `running` that no durable line justifies. That is a stronger
statement than the fold, and it is positively provable rather than being
an argument from the absence of a contradiction: `running` exists in this
crate's memory for the length of one `activate`, before the contract is
provided, and never afterwards. `tests/composition/tests/workflows.rs::a_node_in_flight_when_the_daemon_dies_comes_back_interrupted_with_a_reason`
SIGKILLs a daemon with a node in flight and reads all of it back — the
node's state, its reason, the run's ending, and the two lines appended
after the one that started the work.

The by-product is that this seam needs no second, folded status beside its
declared one. The todos seam needed one because the interrupted fact lived
in the DISPATCH's vocabulary while the ledger's claim lived in the Todo's
`status`. Here the interruption is a value of the node's own state space,
so the recorded state and the reported state are the same field, and the
recovery makes the record say it rather than a derivation saying it on the
record's behalf.

## 3. Why this seam did not bump the kernel pin

jinnd landed M2-K10 while this packet was in flight: a reply-expecting
dispatch that would close a cycle now REFUSES, typed and ledgered,
distinguishable from `Restarting`. The tidy move would have been to bump
to it. The packet's own instruction was to decide on evidence, and the
evidence says no.

A call in this composition is possible only where a GRANT allows it, so
the grant graph BOUNDS the dispatch graph. The generated profile's grant
graph is a directed acyclic graph: a run store is granted its own
`jinn:workflow.<id>`, `jinn:clock`, and one `jinn:todo.<id>` per Todo
store — and **no** `jinn:session.<id>` and **no** `jinn:engine.<id>` at
all. Each layer holds authority over exactly the layer below it and
nothing above. No layer listens on the topic of the layer below either;
each POLLS, which is the whole of `FINDINGS.md` #4 and #32 avoidance.

That is checkable rather than assertable, so it is checked:
`tests/composition/tests/workflows.rs::the_grant_graph_the_four_layers_compose_through_is_acyclic`
reads the profile the kit generates, builds the call graph from the grants
and the providers, and runs Kahn's algorithm over it. It reads the
profile rather than the daemon, so it holds without the pinned-daemon gate
and cannot be skipped quietly. If a future seam ever grants a lower layer
authority over a higher one, that test goes red and the pin decision is
re-opened with a reason.

`FINDINGS.md` #32 therefore stays OPEN and its `#[ignore]`d
settings-recovery test was not run. The bump would have made no claim this
packet could have proved.

## 4. FINDINGS #35, measured

`FINDINGS.md` #35 recorded that latency compounds per LAYER, because a
composing seam must POLL the one below rather than listen to it, and it
graded itself honestly: *derived, not measured*. It predicted a fourth
layer would pay three poll periods.

This packet adds exactly that fourth layer, so the prediction is now
testable and `tests/composition/tests/workflows.rs::dispatch_latency_at_two_three_and_four_layers`
tests it. All three depths are measured from ONE daemon, on the same
engine, at the same poll period, in the same moment, so the depths differ
in the number of layers and in nothing else; each depth's workflow is one
dispatch node with no edges, so a run is one pass through the stack with
no graph walk mixed in; and the observer polls far faster than the stores
do, so the measurement is not dominated by the measuring.

The numbers and the verdict are in `FINDINGS.md` #35's own entry, appended
in place — the entry is the record of what the friction was, and a
prediction that was honestly graded deserves its answer written where the
prediction is, not somewhere else.

The test's assertion is deliberately weak and structural: that each layer
costs something, never nothing. A wall-time threshold across a loaded
machine would be a flaky red that says nothing about the seam. The numbers
are PRINTED, and reading them is what the findings entry does.
