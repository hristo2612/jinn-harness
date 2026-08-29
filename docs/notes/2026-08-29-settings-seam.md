# Agent note — phase 2.2: pin-bump 6 and the settings seam (pin `57360cc`)

The `57360cc` pin (jinnd M2-K7) brought the operator contracts —
`jinn:introspect`, the live `jinn:ledger` reader, `jinn:profile`
`patch-entry` as operator intent, `jinn:net` readiness wakes — and this
packet adopted them (FINDINGS.md #19/#20/#21/#23 closed) and built the
second core-port seam on them. This note records the non-obvious
choices; the contract law lives in `plugins/settings/jinn-settings/README.md`
and `plugins/api/jinn-api/README.md`, the layouts in the profile READMEs,
the frictions in `FINDINGS.md`.

## Why the adoption keeps the profile read on `jinn:fs`

The edit lane no longer touches the document (the kernel's loader applies
`patch-entry`), but `status` still shows each entry's authority fields
and `get` still answers the document, and the only read surface a guest
has is its scoped `jinn:fs`. `jinn:introspect` carries the kernel's
runtime view, not `package`/`hash`/`grants`, and `jinn:profile` has no
read. So the operator layout's data-root coupling survives for reads,
typed (#25): beside the data root the API still serves, `document.readable`
is `false` with the finding, entries come from the kernel's list with
empty authority fields. The soak relocated its profile INTO the data root
rather than run half an API — the watcher is non-recursive on the
profile's directory, so the fibers' subdirectories never wake it.

## Why `kernel.unavailable` stays on the wire, empty

The 0.1.0 report promised the list would empty as the kernel grew, never
rename. It emptied. A 0.1.0 reader still decodes `{ unavailable: [],
finding: 19 }`; removing the object would be the first breaking change on
the seam for no reader's benefit.

## Why the HTTP provider serves each fresh connection once on accept

A readiness wake is per handle: the listener's wake says "a connection is
pending", the connection's wake says "bytes or EOF". Between the accept
and the kernel arming the new connection's wake, its request bytes may
already be there — `accept` then one non-blocking read pass costs one
`read` row and never misses a request; the connection's own wake (when
it comes) finds either nothing or the rest.

## Why the settings overlay is a profile ENTRY

C5 asks whether a plugin can absorb a settings change in place. At this
pin the kernel restarts the patched entry on EVERY `patch-entry`, so the
only way to change an owner's effective settings without restarting it is
to keep the changed layer outside the owner's entry — and the only place
the profile stays the single source of truth is another entry. The
`jinn-settings-store` entry is that place: its `config.data.overlays` is
the hot layer, a hot patch is a `jinn:profile` patch of the store (its
trivial fiber restarts, never the owner), and the provider reads it
through `jinn:settings-store` on every resolution, caching nothing, so an
operator's direct edit of the store shows on the next read. What a
kernel-side intercept chain (C6) would do natively, the harness does with
one extra entry and one extra contract — that is the evidence FINDINGS
#27 records, with the measured cost of both paths.

## Why the owner declares from a wake and never from `activate`

`patch-entry` awaits the patched fiber's restart. A scheduler that called
`declare` from `activate` would be activating inside the provider's own
`patch` — the provider is mid-call, the scheduler's call into it waits for
the provider, the provider waits for the scheduler's activation: the
nested-dispatch deadlock of #4, met on the settings seam (#26). So the
scheduler plans its activation on its entry layer, resolves the settings
one clock floor later from a one-shot alarm, re-declares on every wake
(which is also how a provider restart or swap heals, with no `ready`
event to gamble on), and absorbs `changed` events from their payload
without calling back. The one bound this leaves — a hot-removed job can
fire once on the activation plan of a restarted scheduler — is stated in
#27.

## Why the settings envelope IS the api envelope

`jinn-api-http` decodes every consumer answer through `jinn_api::Answer`.
Giving the settings seam the same `{api-version, ok | error {code,
detail}}` shape with the same four codes means the transport carries the
seam with a route-table row and no adapter, and a second transport would
too. The definitions stay independent (neither depends on the other; the
shape is documented in both).

## Why `jobs` and `notify-token` are hot and `tick-ms` is not

The job table is state the scheduler can replace between wakes; the alarm
period is a kernel registration the fiber cannot re-request in place (no
alarm cancel at this pin — the kernel cancels it with the fiber). A
`tick-ms` patch therefore takes the restart path by declaration, and the
proof measures both paths on the one consumer. `notify-token` exists to
prove the typed secret reference on a real namespace (a reserved key for
the outbound-request edition the `jinn:net` bundle already admits); the
scheduler carries it and ignores it.

## Why a shadowed patch is refused whole, not applied to two layers

Round 1's verifier probed: an overlay holds `jobs`; a mixed `{jobs,
tick-ms}` patch landed whole in the entry, the answer and the `changed`
event carried the requested `jobs`, and the next `get` resolved the
overlay's. The COO ruled the law (a patch's report equals the next
resolve; apply both layers or refuse whole, never a partial apply that
lies) and left the choice. Refusal won because the kernel writes one
entry per `patch-entry` (FINDINGS.md #28): the entry and the overlay are
two entries, so "apply both" is two calls, and a second call refused
after a first applied is exactly the partial state the law forbids —
with the extra cost that the first call has already restarted the
owner. A refusal costs nothing, is typed to the key and the layer, and
leaves the operator a one-step recovery. The plan now computes its
reported settings FROM the post-state layers (`resolve(after)`) and
refuses when that differs from what was asked, which also catches the
removal case (a hot `null` that a lower layer still defines) the probe
did not reach.
