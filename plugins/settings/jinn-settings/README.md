# `jinn-settings` 0.1.0 — the settings contract

The service definition of the settings seam. This document is the
contract's prose law; the types in `src/` are their schema. Within 0.x
every change is strictly additive, every answer carries `api-version`,
and every wire schema preserves unknown sibling fields across a decode →
encode round trip at every nesting level.

## Names

| Name | Value | What it is |
|---|---|---|
| Contract | `jinn:settings` | Provided by a settings provider; operations `declare`, `get`, `patch`, `namespaces`. |
| Store contract | `jinn:settings-store` | Provided by the overlay store; operation `overlays`. |
| Changed topic | `jinn:settings/changed` | Emitted serial/all after an applied patch, payload `Changed`. An owner holds a listen grant for it. |
| Refused topic | `jinn:settings/refused` | Emitted emit/all after a refused patch, payload `Refused` — its `DispatchTrace` is the refusal's ledger record. |
| Secret reference | `{"$secret": "<keystore key>"}` | The only shape a `secret-ref` field admits. |

All payloads are UTF-8 JSON with kebab-case keys; every answer is the
envelope `{"api-version", "ok": …}` / `{"api-version", "error": {code,
detail}}` with codes `not-found` · `invalid` · `refused` · `unavailable`
— the operator-API seam's envelope, so a transport that speaks it carries
this seam unchanged.

## Namespaces

A namespace is declared by its OWNER — the plugin whose entry config is
the namespace's entry layer: `Declaration { namespace, entry, schema,
defaults, hot-keys }`. `declare` is an idempotent upsert answering the
resolved settings; the owner re-declares on every wake (a provider
restart or swap heals within one wake), and never from `activate`
(§Why the owner never calls from activate).

## The schema language

Closed and side-effect-free (kernel R9): `Schema { properties: { key:
Field { kind, required } }, additional }` with kinds `bool` · `integer`
(non-negative) · `number` · `string` · `array` · `object` ·
`secret-ref`. The validator decides membership of a WHOLE settings
object and nothing else: required keys present, present keys of their
kind, no undeclared key unless `additional`, and a `secret-ref` field
holds a reference — a bare value there is refused ("the settings
document holds no secret"). Resolution of a reference is the keystore
seam's.

## Layers

**Precedence:** `resolve(layers) = defaults ⊕ entry ⊕ overlay` (RFC 7396
merge, bottom to top: a higher layer's key wins, objects merge
recursively, `null` removes) — the overlay outranks the entry, the entry
outranks the defaults, always. The entry layer is the owner's
`config.data` as activated; the overlay lives in the store entry's
`config.data.overlays[namespace]`. Both are in the profile document,
which stays the single source of truth; the provider caches neither.

### The consistency guarantee

A patch is atomic and consistent across layers: **the settings a `patch`
answers and emits in `changed` are exactly the settings the next `get`
resolves.** A patch lands in ONE layer (§The patch law), and the plan's
reported settings are computed from the layers as they stand after that
write — never from "resolved ⊕ patch". If the two would differ — a key
the patch sets would still resolve from a layer above the landing layer
(a mixed hot+cold patch lands in the entry while the overlay holds one
of its hot keys), or a key the patch removes would resolve from a layer
below — the WHOLE patch is refused before anything applies: `invalid`
with a typed `shadowed { key, layer }` naming the first such key and the
layer its value resolves from. There is no partial apply and no event
that says one thing while the document resolves another. The recovery is
the operator's: patch the shadowed key on its own (it then lands in the
layer that resolves it) or clear it there first. Refusal rather than a
two-layer write is deliberate — the kernel patches one entry per call
(FINDINGS.md #28), so two layers could never be written atomically.

## The patch law

`patch(namespace, merge-patch)`:

1. The patch must be an object. The RESULT of laying it over the resolved
   settings is validated against the declared schema BEFORE anything
   applies, and the landing layer must resolve to exactly that result
   (§The consistency guarantee); a refusal is typed (`invalid`, with
   `shadowed { key, layer }` in the consistency case), answered, and
   emitted on the refused topic — nothing was written.
2. A patch whose every top-level key is a hot key lands in the OVERLAY:
   the provider patches the store entry through `jinn:profile`
   (`{ data: { overlays: { ns: <patch> } } }`), the store's trivial
   fiber restarts, then `changed` is emitted with the resolved settings
   and the owner absorbs them in place. `applied: "hot"`.
3. Any other patch lands in the ENTRY: the provider patches the owner
   entry (`{ data: <patch> }`), the loader restarts exactly the owner,
   `changed` is emitted (the owner's notice; it re-declares on its next
   wake with its new entry layer). `applied: "restart"`.
4. A kernel refusal (scope, validation, the loader's retryable conflict)
   is answered `refused` with a `retryable` flag and emitted on the
   refused topic; the revision does not move.
5. `revision` counts applied patches per namespace within a provider
   incarnation.

## Why the owner never calls from `activate`

`jinn:profile` `patch-entry` awaits the patched fiber's restart before
answering. An owner whose `activate` called the settings provider would
be activating inside the provider's own `patch` call — the
nested-dispatch deadlock (FINDINGS.md #4, #26). So an owner resolves its
settings from a one-shot alarm after activation and from every wake,
plans its activation on the entry layer alone, and absorbs `changed`
events without calling back (the payload carries the resolved settings).

## Changes

- **0.1.0 (2026-08-29, kernel pin `57360cc`):** first edition. Same
  day, additive: the consistency guarantee and the typed `shadowed`
  field on an `invalid` refusal (a 0.1.0 reader without it still
  decodes).
