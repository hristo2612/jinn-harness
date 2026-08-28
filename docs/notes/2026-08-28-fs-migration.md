# Agent note — phase 1.6: the guests onto `jinn:fs@0.2.0` (pin `41cb2f47`)

The `41cb2f47` pin (jinnd M2-K3) closed FINDINGS.md #3 and #8 and changed
the `write` signature — a breaking guest migration, taken as one, with no
compatibility layer. This note records the choices; the contract law lives
in `plugins/cron/jinn-cron/README.md`, the frictions in `FINDINGS.md`.

## Why the history log is JSONL and the legacy array is a read-once seed

`append` is the operation the finding asked for, and an append lane wants
a line-delimited shape: one record per line, one append per tick sized by
that tick, decoded line by line. The old lane was one JSON array rewritten
whole; an array cannot be appended to. Rather than migrate the soak's root
by hand, the scheduler reads the legacy array once at activation as the
window's seed, before the log, and never writes it — a root that carries
both has every record exactly once, and a fresh root never grows an array.
The composition suite asserts that no `write` of the log ever lands.

The log is unbounded on disk by design. Bounding it would mean a rewrite
(the pattern just retired) or a `remove` + fresh log; both are additive
changes to make when a rotation policy is actually wanted. The `history`
operation still serves the newest 500.

## Why the run record's idempotency key is its path, and nothing else is keyed

Keys are per fiber and answer a repeated delivery from the recorded effect.
The per-fire record has an identity — the boundary — so its key is the
path that names it. A state write or a history append has no such
identity: each is a new effect by construction (a tick never repeats
within a fiber), and a key on them would be a lie the provider would
honor by NOT writing.

## Why the restart proofs moved to the crash path

At this pin every fs mutation is withdrawn with its fiber, and a graceful
shutdown disposes every fiber (FINDINGS.md #14). The suite's restart tests
proved firing law #3 through a clean SIGINT; that path now reverts the
very state the law is about. The honest options were to delete the proofs
or to prove the law through the path that still holds — a process death —
and pin the clean-shutdown withdrawal as its own transcript test so the
kernel change that retires the finding is noticed here, not discovered.
The second was taken. Nothing in `tests/invariants/` is involved — the
suite is the harness's own.

## What the consumer now reports, and why it is not a system probe

`list` and `meta` widen what a guest can honestly observe: its own
directory, the fired job's run records (one file per fire on disk), and
the history log's size. That is still exactly the granted `jinn:fs` scope
— no `df`, no process table (FINDINGS.md #5) — and the report says so by
construction: every number in it is the answer of a ledgered contract
call.
