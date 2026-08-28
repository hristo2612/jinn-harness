# `jinn-api` 0.1.0 — the operator-API contracts

The service definition of the operator-API seam. This document is the
contracts' prose law; the types in `src/` are their schema. Designed to
outlive this implementation (the kernel's R12 discipline): within 0.x
every change is strictly additive, every answer carries `api-version`,
and every wire schema preserves unknown sibling fields across a decode →
encode round trip at every nesting level (the cron seam's additivity
precedent).

## Names

| Name | Value | What it is |
|---|---|---|
| Status contract | `jinn:api-status` | Provided by `jinn-status`; operations `status`, `health`, `ledger-tail`. |
| Profile contract | `jinn:api-profile` | Provided by `jinn-profile-edit`; operations `get`, `patch-entry`. |
| Provider grants | `jinn:net` (scoped to one loopback port), `jinn:clock`, both contracts above | The HTTP provider's authority: transport + the right to call the consumers. |
| Consumer grants | the contract each provides; `jinn:fs` scoped to the profile document; `jinn:cron` (status probe) | Authority is the profile side's — requests are not grants. |

All payloads on this seam are UTF-8 JSON with kebab-case keys. Every
broker answer is the envelope `{"ok": …}` or `{"error": {code, detail,
finding?}}` — a consumer never fails its fiber to say no (R11: a refusal
is an answer, not a fault).

## Route table (v1)

The definition names each operation's transport shape so that every
provider exposes ONE surface:

| Method | Path | Contract / operation | Request payload |
|---|---|---|---|
| `GET` | `/v1/status` | `jinn:api-status` / `status` | the query object (empty) |
| `GET` | `/v1/health` | `jinn:api-status` / `health` | — |
| `GET` | `/v1/ledger/tail?after=N&limit=M` | `jinn:api-status` / `ledger-tail` | `{after, limit}` |
| `GET` | `/v1/profile` | `jinn:api-profile` / `get` | — |
| `PATCH` | `/v1/profile/entries/{id}` | `jinn:api-profile` / `patch-entry` | the JSON body + `id` from the path |

HTTP status mapping of the typed error codes is the provider's
(`jinn-api-http-wire`): `not-found` 404, `invalid` 422, `unavailable`
503, `refused` 502; a route miss is 404/405.

## Schemas

- **`status`** → `StatusReport { api-version, entries: [EntryStatus],
  probes: [ProbeReport], kernel: KernelIntrospection }`. `entries` are the
  profile document's entries with their authority fields (`id`,
  `package`, `hash`, `grants`) — the document of record, read through the
  consumer's scoped `jinn:fs`. `probes` are observations through the
  broker: for each configured `{contract, operation?}`, a granted
  `resolve` (and one read call), `live` iff both succeed, the answer or
  the kernel's refusal verbatim. `kernel` NAMES the fields no guest can
  answer at this kernel pin — `fiber-state`, `fiber-uid`, `provisions`,
  `listeners`, `alarms`, `last-ledger-seq`, `readiness` — with the
  FINDINGS.md number (#19). When the kernel grows an introspection
  contract, those fields land as additive siblings and the list empties.
- **`health`** → `HealthReport { api-version, ok, profile-readable,
  entries, probes-live, probes-total }`; `ok` iff the profile is readable
  and every probe is live.
- **`ledger-tail`** ← `{after?: u64, limit?: u32}` (defaults 0, 100;
  `limit` clamped to `1..=500`) → `LedgerTail { api-version, after,
  limit, events: [], next-after?, unavailable? }`. At this pin no
  `jinn:ledger` reader is provided (FINDINGS.md #20): `events` is empty
  and `unavailable` carries the typed reason and finding. The request is
  still a ledgered contract call — the operator's read intent is on the
  record.
- **`get`** → `ProfileDocument { api-version, profile }` — the document
  verbatim.
- **`patch-entry`** ← `PatchEntryRequest { id, config: { data?,
  grants? }, idempotency-key? }` → `PatchEntryAnswer { api-version, id,
  entry, changed }`.

## The entry-patch law

`patch-entry` changes exactly ONE entry of the document and nothing else:

1. `config.data` is an **RFC 7396 merge patch** on the entry's settings
   subtree: objects merge recursively, `null` removes a key, any other
   value replaces (an array is replaced whole).
2. `config.grants`, when present, **replaces** the grant list — authority
   is never merged; the profile side decides it whole.
3. An unknown `id` is `not-found`; a document without an entries array is
   `invalid`. Neither writes.
4. A patch that changes nothing answers `changed: false` and writes
   nothing (an identical rewrite would still reconcile `unchanged` on the
   daemon side — the seam does not spend a write to say so).
5. A changed document is written back in ONE `jinn:fs` `write` of the
   whole rendered document (pretty, newline-terminated), keyed by the
   request's `idempotency-key` (empty claims none). The daemon's file
   watcher then reconciles the edit exactly as an operator's: the patched
   entry's fiber cycles (`cause: ConfigChanged`), the others are
   `unchanged`. The API never bypasses the profile as the source of truth.

The write is a revertible effect of the editor's fiber at this pin — the
kernel keeps the pre-patch document as its inverse, retained across the
editor's incarnations and withdrawn when the editor entry is disposed.
FINDINGS.md #21 records what that means for an operator's edits and the
capability that retires it; #22 records the write's non-atomic shape.

## Additivity (the R12 promise, mechanically)

- Every schema carries a flattened extension map at every level
  (`Extensions`): a field a newer writer adds survives this reader
  verbatim.
- `kernel.unavailable` is a list of names: fields are removed from it as
  the kernel grows, never renamed.
- The route table is append-only within v1; a breaking change is `/v2`.

## Changes

- **0.1.0 (2026-08-29, kernel pin `1b098be`):** first edition.
