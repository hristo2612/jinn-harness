# The engines seam: why the contract name carries the engine id

*Agent note, phase 2.3 (kernel pin `3fd7b05`, jinnd M2-K8).*

The malleability contract asks for three things of every core-port seam:
an implementation can be **switched** by a profile edit with consumers
untouched, two implementations can **coexist**, and a third can be added by
**extension** — profile only, no change to the definition. For settings and
for the API, one provider per contract was enough and all three properties
fell out of the entry. Engines are the first seam where the *point* is
several implementations live at once, so the shape had to be decided
rather than inherited.

## The constraint

The kernel holds exactly one provider slot per contract name. `provide`
refuses a second peer for an occupied slot with `DuplicateProvision`, and
it refuses it deliberately — "replacement is never silent" (R9). There is
no shape in the kernel for *instances* of a contract: no qualified
resolve, no provider selection at `services.resolve`, no per-instance
grant. So `jinn:engine` as one name can be served by exactly one engine,
and a composition with claude and codex mounted at once cannot exist.

Three ways out were on the table.

**A router plugin.** One `jinn:engine` provider that resolves the real
providers behind it and dispatches on `request.engine`. Consumers see one
contract; the router owns the routing table. Rejected: the router is a
fourth role the seam-triple naming law does not have, it puts a hop and a
guest-side table between every consumer and every run, and — the decisive
part — the routing table becomes a second home for a fact the profile
already holds. An operator would edit the profile to add an engine and
then edit the router to reach it.

**Distinct contracts per package.** `jinn:engine-claude`, `jinn:engine-codex`.
Rejected outright: it welds the contract name to the implementation, so a
switch is a consumer change — exactly the property the packet asks us to
prove we do not have.

**An instanced contract name.** `jinn:engine.<engine-id>`, where the id
comes from the provider entry's own `config.data.engine` and appears
nowhere else. This is what shipped.

## What the encoding buys

The engine id and the contract name are the same fact, so routing by
engine id *is* resolving a contract, and the profile is the only place
either is written:

- **Switch** — change the entry's `package` and `hash`, keep its `id` and
  its `engine`. A different implementation now serves `jinn:engine.default`.
  No consumer, grant, or definition changes. The kernel restarts exactly
  that fiber and the provision moves with it.
- **Coexistence** — a second entry with `engine: "codex"` provides
  `jinn:engine.codex` beside it. Two live providers, no slot contention,
  and a consumer picks by naming one.
- **Extension** — a third provider is an entry and a grant. `jinn-engine`
  does not change, and neither does any consumer that was not asked to
  reach the new engine.

It also keeps authority where the kernel can see it. A grant names a
contract, so a consumer's grant for `jinn:engine.codex` is per-ENGINE
authority, enforced at the kernel's own choke point rather than by a
router's conscience. A consumer that may run the echo engine and not the
paid one is a profile edit, not a code path.

## What it costs, and the finding

The cost is that a contract name now has structure a reader has to know,
and the kernel does not know it: to the broker, `jinn:engine.codex` is an
opaque string, so nothing checks that two entries do not claim the same
engine id (the slot refusal catches it, but as a duplicate PROVISION, not
as a duplicate engine), and `jinn:introspect` reports provisions the
operator must parse to see engines. `FINDINGS.md` #28 records this as the
kernel friction it is, with the capability shape that would retire the
encoding — instance-qualified provision and resolve, so a contract can be
provided *at* a name.

Everything above is a guest-side emulation of a kernel concept, which is
the same posture the settings seam took for per-entry config layering
(#27). Recording it is the point of the two-way iteration channel: the
harness ships the honest workaround and the kernel gets the card.

## Two rules the providers all follow

**A prompt never rides in argv.** Every provider writes the prompt to the
child's stdin and closes it. `argv` is world-readable in the host's
process table; a prompt is personal data. This is why the definition's
`RunRequest` documents the prompt as stdin-delivered rather than leaving
it to each provider.

**An idle provider costs zero ledger rows.** The `jinn:process` bundle is
poll-shaped in v0.1 — `read` answers `would-block`, `wait` is bounded — so
a provider needs a clock to make progress. It arms a ONE-SHOT
`alarm_at` when a run starts and re-arms it only while a run is still
live, rather than holding an `alarm_every`. That is the discipline
FINDINGS #23's closure bought this repo when the HTTP provider dropped its
poll: a mounted-but-idle plugin adds nothing to the ledger. A repeating
alarm is right for the probe, because the probe *is* a schedule.

## Secrets, and the honest gate

A run request carries `{"$secret": "<key>"}` references — the settings
seam's typed shape, reused rather than restated — and the provider
resolves them through its granted `jinn:keystore` prefix at spawn time.
The profile document, the ledger, and this repo hold key names only. The
vendor CLIs on this box read their own credential files out of `$HOME`
themselves, under the host's uid; the harness never reads, copies, or
names those files, and the env policy passes `HOME` (and `PATH`, for the
node-hosted one) and nothing else.

Where a CLI is absent or unauthenticated, the provider answers
`ErrorCode::Unavailable` and the run is recorded as environment-gated. It
is never faked. The echo provider exists so that the seam's proofs — run,
events in sequence, exit with usage, cancel, refusals — hold on a machine
with no vendor authentication at all, including CI.
