# The door presents what it was given, and asks even when given nothing

*Harness packet 2.8 (PLA-343) — `jinn-api-http` consumes `jinn:auth@0.1.0`
(jinnd M2-K21, pin `85d36b4`). Together with that kernel packet this
closes the second M3 blocker SOURCE-OF-TRUTH §7 names: there is now an
authentication boundary, and it is proven where the door is.*

## Where the check lives, and why it is the transport's

The kernel holds no inbound listener. A transport plugin holds one
through `jinn:net`, and the kernel sees bytes. So M2-K21 supplies the
AUTHORITY — one credential of record beside the data root, read on every
call, deny by default, every decision an `AuthDecided` row — and says in
its own contract that the check is the transport's obligation: "a plugin
that serves an inbound connection issues NO dispatch on that connection's
behalf before this call answers `principal`". The kernel cannot gate a
transport's granted calls behind `verify` without doing delegation
between plugins, which the contract rules out.

This packet is that obligation, met. `jinn-api-http` has one door
(`src/door.rs`, `admit`), and `serve` walks every parsed request through
it before `dispatch` — the function that turns a request into a granted
crossing on a consumer, the settings seam, or an engine. The proof is the
real-composition suite (`tests/composition/tests/auth.rs`), and the
proof's teeth are the mutation demonstration recorded at the bottom.

## The bearer header, and the one paragraph the packet asked for

The presented credential is the token of an `Authorization: Bearer`
header and nothing else on the request. Three reasons, each of which
rules something out:

- It is what an operator's tools already carry. `curl -H` and a
  browser's `fetch` send it without a wrapper, so the web UI's port needs
  one header, not a protocol.
- It never lands in a URL. A query parameter lands in shell history,
  proxy logs and referrers; the whole point of the ledger redacting the
  presented value to its digest would be undone by the transport writing
  it somewhere else in clear.
- One header carries one value, which is exactly the contract's shape:
  one operator, one credential. A cookie would make a session an
  identity, and Basic auth would put a username on the wire that implies
  an account. The packet forbids both, and the carrier should not invite
  either.

## Nothing presented is still put to the kernel

A request with no bearer header presents nothing. The door does NOT
answer that itself. It calls `verify` with the empty string, and the
kernel refuses it the way it refuses any value that is not the credential.

That is the design, not an economy. The contract's whole promise is ONE
decision point with EVERY decision on the record. A transport that
short-circuited "no credential" locally would be a second decision point
— a tiny one, but the exact shape the contract exists to prevent, and the
one a future transport author copies without noticing. Putting nothing
to the kernel costs a crossing and buys an invariant the suite can state
simply: every parsed request is exactly one `verify`, and exactly one
`AuthDecided` row. The row's `presented` digest is then the SHA-256 of
the empty string, a constant an auditor recognizes on sight.

It also keeps the reason honest. The kernel's reason on the wire is the
kernel's, never a phrase the transport invented, so an operator's log
reads the same whether the peer sent nothing or sent the wrong thing:
what was presented did not prove the operator.

## A refusal is its own class

The kernel's `unauthenticated(reason)` reaches the wire as
`ErrorCode::Unauthenticated` — HTTP 401 with the `WWW-Authenticate:
Bearer` challenge RFC 7235 requires — carrying the kernel's reason and
never the presented bytes. It is distinct from `refused` (502: a grant or
provider said no; the caller's profile to widen) and from `unavailable`
(503: the transport, worth retrying), because its next move is distinct:
present the operator's credential, or stop.

A door that cannot ASK is `refused`, not `unauthenticated`: the contract
unresolvable (the provider's entry lacks the `jinn:auth` grant), the
crossing refused at the broker, or an answer off the contract's wire (no
tag, an unknown tag, non-UTF-8). Each of those is closed — nothing
dispatches — but each is the composition's defect, and dressing it up as
the operator's problem would send an operator hunting for a credential
that was never the issue.

## One call per request, and what that buys

The door never caches a grant. The kernel re-reads the credential file on
every `verify`, so rotation and revocation take effect on the very next
request with no restart, and the suite proves it by rotating and deleting
the file under a running daemon and reading the ledger for the five
decisions in order. A transport that cached "this connection is the
operator" would silently extend a revoked credential's life by one
connection, which is exactly the window the contract's per-call read was
designed to close.

## Provisioning is part of the door

The daemon only reads the credential; the launcher owns it. Both
launchers in this repo now provision it if absent, and both leave an
existing file exactly as it is (a restart keeps its credential; a
rotation is the operator's deliberate overwrite):

- The composition rig writes it in `Daemon::spawn`, before the process
  exists, at `<data>.operator-token` (the kernel's own rule,
  `data.with_extension("operator-token")`, spelled the same way in
  `composition::kit::credential_path`). The value is 32 random bytes
  drawn once per test process, so nothing in this repo looks like a
  secret and no call site threads a token; the suite's client presents
  it by default and the door's proofs present other things on purpose.
- The soak's supervised start runs `tools/soak/provision-token.sh` before
  `exec jinnd`, and SOAK.md's unsupervised lane calls the same script, so
  the rule has one home. It draws 32 bytes from `/dev/urandom` as 64 hex
  characters, writes under `umask 077` to a staging file and renames it
  into place, never prints the value, and REFUSES to keep a file whose
  mode is group- or world-accessible — the kernel would refuse every
  call against it, and a refusal at the start beats a ledger of refusals
  to explain. Where the operator reads it is written in SOAK.md
  §Credential: `cat "$SOAK/data.operator-token"`.

## What the kernel does not do, restated so it is not re-litigated

Same-uid is out of the threat model, by the contract's own words: a
process running as the daemon's uid can read the credential file and
nothing here holds against it. The door does not change that and does
not claim to. What it adds is the boundary the kernel could not add —
that a foreign process, a mistaken second instance, or a future transport
written without a check cannot dispatch on this daemon's behalf.

## The mutation, recorded

Acceptance (4) asks that removing or reordering the `verify` call FAILS
the suite. Both were driven, in the worktree, after the suite was green
at the packet's head; the exact commands and the verbatim failing tails
are on PLA-343. What each showed, in one line each:

- **Removed** (`admit` answers `Ok(())` without calling the kernel): the
  no-credential and wrong-credential requests answer 200, and the suite
  fails first on the status, then — had the status matched — on the
  missing `AuthDecided` rows.
- **Reordered** (`dispatch` before `admit`): the refused requests carry a
  consumer crossing in their segment, and the suite fails on "exactly one
  crossing: … and it is the verify".

Neither mutation is a permanent test. A guest is rebuilt by its kit at the
start of every composition run, so a mutation test would have to compile
a second, deliberately broken guest per run; the demonstration is cheap
and the record is here.
