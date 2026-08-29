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

`resolve(layers) = defaults ⊕ entry ⊕ overlay` (RFC 7396 merge, bottom
to top: a higher layer's key wins, objects merge recursively, `null`
removes). The entry layer is the owner's `config.data` as activated; the
overlay lives in the store entry's `config.data.overlays[namespace]`.
Both are in the profile document, which stays the single source of
truth; the provider caches neither.

## The patch law

`patch(namespace, merge-patch)`:

1. The patch must be an object. The RESULT of laying it over the resolved
   settings is validated against the declared schema BEFORE anything
   applies; a refusal is typed (`invalid`), answered, and emitted on the
   refused topic — nothing was written.
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

- **0.1.0 (2026-08-29, kernel pin `57360cc`):** first edition.
