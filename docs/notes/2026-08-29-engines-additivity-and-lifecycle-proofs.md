# Engines seam, round 3: additivity everywhere, and proofs that check

Round 2 of the engines packet landed the seam and then reported a green
workspace gate it did not have. This note records what round 3 changed and,
more usefully, why each change was the only shape available.

## The false green comes first

The round-2 report contained `cargo test --workspace → exit 0`. It was not
true: `the_shadowed_refusals_recovery_lands_when_executed` failed at that
head, under `--workspace` and in isolation, and the verifier reproduced it
twice. Everything downstream — including a scope ruling — was decided on
that number.

The rule that follows is not "be careful". It is mechanical: gates run on
the final pushed head, after the last commit, and the report pastes each
gate's verbatim summary line with its pass/fail counts. A red round
reported red is cheap. A false green costs a full verification cycle and
discredits every other number in the same report.

## Additivity is a law at every nesting level

`RunRequest`, `RunRecord`, `Description` and `EngineError` all carried
flattened extensions, and that read as compliance. It was not: `ToolPolicy`,
`Budget`, `Capabilities` and `Usage` are nested inside those envelopes and
carried none, so a newer peer's `max-turns` or `cache-read-tokens` was
dropped by the hop through this version — silently, which is the part that
matters. The envelope survived; the fact did not.

`Event::Unknown` was worse. It decoded an unheard-of kind rather than
failing, which is right, but it kept neither the kind nor the payload: every
future event became the same nameless placeholder, and an operator watching
a bus could not tell one newer provider's behaviour from another's.

Preserving the kind means the event must serialize under the tag it ARRIVED
with, and a derived internally-tagged enum can only emit its own variant
names. So `Event` now has a hand-written wire codec (`to_map`/`from_map`) —
one home for the event's wire shape, and the only way `Unknown { kind,
fields }` can round-trip byte-for-byte. The proof is a test that feeds a
future-shaped document through each nested type and asserts the encode
equals the original document, not merely that the known fields survived.

## The output cut had to become an event

`Runs::read` marked `RunRecord.truncated` and the providers then emitted
`Event::Cancelled`, which has no truncated field. A consumer of this seam
sees EVENTS. So a bounded answer reached a listener as a stream that simply
stopped — indistinguishable from a whole one, which is exactly the silent
wrong answer R9 forbids.

`read` now answers `Event::Truncated { limit-bytes, read-bytes }` and the
provider records it before the kill, so the cut is on the bus ordered ahead
of the end it caused. Recording it is also what sets the record's flag, so a
listener and a `run-get` reader cannot end up disagreeing about whether the
answer is complete.

## The lifecycle proofs needed a real child, and it could not be a vendor CLI

The acceptance asks that cancel kill the child with the process table
checked, that suspend kill a run genuinely in flight, and that an executable
outside the allowlist and an env leak both be refused and ledgered. Round 2
proved none of these: cancel exercised the echo provider's delay with no
child anywhere, suspend waited for the run to settle first, and the only
refusal probe was keystore scope.

The obvious vehicle is a vendor provider — and it is the wrong one. These
proofs must hold in CI and in an independent verification that (rightly)
refuses to spend a metered vendor fixture, which is precisely when no vendor
CLI is on PATH. A proof that self-skips whenever someone checks it is not a
proof.

So the echo package gained a second shape rather than the repo gaining a
sixth plugin: with `command` set it spawns that absolute path through
`jinn:process` and streams the child's stdout as the answer. The kit mounts
it once as `jinn-engine-spawn` on `sleep` and `env`, both POSIX. Two
decisions inside it are load-bearing:

- The child's explicit `env` argument is **empty**. Whatever it can see
  arrived through the grant's env policy, so the env-leak assertion is about
  observed fact and not about what the provider intended to pass.
- The child is told **nothing** and its stdin closes at once. A witness is
  not an engine; a prompt written to `sleep` would only be a way to hang.

The env proof checks both directions, and the second is the one that
matters: `HOME` and `PATH` are admitted (the two the vendor CLIs actually
need — each opens its own credential file under `HOME`, and a node-hosted
CLI needs its interpreter), the daemon's keystore passphrase is absent by
name and by value, and then narrowing the policy to inherit-none narrows the
child. That is what makes the allowlist a bound the kernel holds rather than
a promise the provider makes.

Machine paths stay where machine state belongs: the generated profile. The
refusal proof reads its unauthorized executable out of the document of
record, so the refusal is the kernel's and not a typo's, and no `/Users`
path enters a tracked file.

## What is still open, honestly

FINDINGS #31 stays open and is re-graded down. A serial dispatch to a fiber
with a pending restart neither queues nor refuses — it waits out the guest
deadline — and the harness workaround (a provider choosing its dispatch mode
from the layer it just patched) is only available to a provider that knows
which layer it wrote, and does not touch the operator's own call chain. Its
proof, `the_shadowed_refusals_recovery_lands_when_executed`, is marked
`#[ignore]` with a reason naming jinnd M2-K9 / PLA-318, which makes such a
dispatch refuse typed and ledgered. No assertion in that test is weakened
and the attribute comes off when the kernel packet lands; PLA-318's
acceptance says so, which is what keeps this from being a skip nobody
revisits.
