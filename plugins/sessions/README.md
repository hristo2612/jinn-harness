# The sessions seam

Durable, resumable conversations as a capability on the kernel — the
fourth core-port seam under the malleability contract (phase 2.4), and the
first one that COMPOSES another. Roles per the seam-triple naming law
(AGENTS.md):

| Role | Package | What it is |
|---|---|---|
| Service definition | `jinn-session` | The `jinn:session.<store-id>` contract: the session spec (engine binding, cwd, tool policy, attribution, metadata), the typed session/turn statuses, the events on `jinn:session/event`, the `create`/`send`/`get`/`messages`/`list`/`cancel`/`close` operations, the durable JOURNAL's record law and its honest replay, and `Sessions` — the registry, turn state machine and status derivation every store shares. Owns the `sessions` settings namespace. Pure types + logic. |
| Provider | `jinn-session-fs` | Durable: one append-only JSONL journal per session over `jinn:fs`, paginated reads served from it, and a replay on activate that recovers what the daemon left behind. |
| Provider | `jinn-session-memory` | Ephemeral: nothing outlives the incarnation. A genuine use (throwaway and test sessions) that doubles as the swap proof and needs no new kernel capability. |
| Consumer | `jinn-api-http` (`plugins/api/`) | Exposes sessions over the operator API: create, send, read a session, read its messages paginated, list, cancel, close. |

## Session over engine — neither knows the other's provider

**A store never spawns an engine.** Both providers INJECT the engines
seam's DEFINITION — they resolve `jinn:engine.<id>` from the session's own
`EngineBinding` (`jinn_engine::engine_contract`) and drive whatever
answers. So:

- Changing a session's `engine` field runs the SAME session spec on a
  different engine provider. The store is untouched and does not know
  which provider answered.
- Swapping the store provider by a profile edit leaves the engines
  untouched, and the engine providers never learn that sessions exist.

That is the composition the phase is for: two seams, each swappable
without the other knowing.

## Why the contract name carries the store id

For the same reason the engines seam's does, and the reasoning has one
home there (`plugins/engines/README.md`, `FINDINGS.md` #29): the kernel
holds ONE provider slot per contract name, so N stores coexisting means N
contract names. The store id is read from the provider entry's own
`config.data.store` and written nowhere else, which makes switch,
coexistence and extension all profile edits.

## Honesty after a crash

A journal is what a store has after a crash, and a crash is exactly when a
system is tempted to lie. The reader is therefore built so the DANGEROUS
answer needs proof: a turn reads back `done` only where a terminal record
was written, and a started turn with no ending replays `interrupted` with
a reason. `running` is minted only by the live registry for a turn this
incarnation started, so a replay cannot produce it at all. A torn TAIL
(the last line, short) reads as absence; a hole anywhere earlier is
corruption and is REFUSED. The contract surface and the law are documented
in `jinn-session/README.md` — one home per fact.

Guest crates here are NOT workspace members (see the workspace manifest's
note); `session-kit` builds them into the sessions profile. Real-composition
proof lives in `tests/composition/tests/sessions.rs`.
