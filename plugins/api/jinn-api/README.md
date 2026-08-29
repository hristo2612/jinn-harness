# `jinn-api` 0.2.0 — the operator-API contracts

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
| Provider grants | `jinn:net` (scoped to one loopback port), both contracts above | The HTTP provider's authority: transport + the right to call the consumers. No clock: it serves from the kernel's readiness wakes. |
| Consumer grants | the contract each provides; `jinn:fs` scoped to the profile document (read); `jinn:introspect`, `jinn:ledger`, `jinn:cron` (status); `jinn:profile` scoped `["*"]` (editor) | Authority is the profile side's — requests are not grants. |

All payloads on this seam are UTF-8 JSON with kebab-case keys. Every
broker answer is the envelope `{"api-version", "ok": …}` or
`{"api-version", "error": {code, detail, finding?}}` — a consumer never
fails its fiber to say no (R11: a refusal is an answer, not a fault).
The HTTP provider answers `ok` with the value as the body and `error`
with the envelope verbatim under the mapped status.

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
| `GET` | `/v1/settings` | `jinn:settings` / `namespaces` | — |
| `GET` | `/v1/settings/{ns}` | `jinn:settings` / `get` | `{namespace}` from the path |
| `PATCH` | `/v1/settings/{ns}` | `jinn:settings` / `patch` | the JSON body (`{patch}`) + `namespace` from the path |

The settings rows (0.2.0) route to the settings seam's provider directly:
its envelope is this seam's envelope (`plugins/settings/jinn-settings/README.md`),
so the transport maps its typed errors to the same status codes. Each
route names the request field its path parameter lands in (`param`) and
whether the payload is the body or the query (`body`).

HTTP status mapping of the typed error codes is the provider's
(`jinn-api-http-wire`): `not-found` 404, `invalid` 422, `unavailable`
503, `refused` 502; a route miss is 404/405.

## Schemas

- **`status`** → `StatusReport { api-version, entries: [EntryStatus],
  probes: [ProbeReport], kernel: KernelIntrospection, readiness?,
  last-ledger-seq?, document: DocumentStatus }`. `entries` are the
  document's entries with their authority fields (`id`, `package`,
  `hash`, `grants` — read through the consumer's scoped `jinn:fs`) with
  the kernel's own view of each laid over by id through
  `jinn:introspect` (0.2.0, additive): `fiber`, `state` (`pending` |
  `loading` | `active` | `failed` | `unloading` | `disposed`),
  `incarnation`, `provisions`, `registrations { listeners, alarms,
  sockets, processes }` — absent for an entry with no live fiber.
  `readiness` is `{ boot-reconciled, watcher-armed }`; `last-ledger-seq`
  the ledger's high-water mark through `jinn:ledger`; both absent only
  when the kernel read was refused. `document` says whether the
  document of record was readable and, when not, the typed reason
  (FINDINGS.md #25: a guest reads it only under the data root) — the
  entries are then the kernel's list with empty authority fields, never
  guessed. `probes` are observations through the broker: for each
  configured `{contract, operation?}`, a granted `resolve` (and one read
  call), `live` iff both succeed, the answer or the kernel's refusal
  verbatim. `kernel` is the 0.1.0 list of fields no guest could answer,
  now EMPTY (`unavailable: []`, `finding: 19` kept as its vocabulary).
- **`health`** → `HealthReport { api-version, ok, profile-readable,
  entries, probes-live, probes-total }`; `ok` iff the kernel lists every
  entry `active` and every probe is live (`profile-readable` reports the
  document, and no longer gates `ok`: a document beside the data root is
  a layout fact, not an outage).
- **`ledger-tail`** ← `{after?: u64, limit?: u32}` (defaults 0, 100;
  `limit` clamped to `1..=500`) → `LedgerTail { api-version, after,
  limit, events: [LedgerEvent], next-after?, unavailable? }`: the
  kernel's events with `id > after`, at most `limit`, each `{ id,
  wall-ms, entry?, fiber?, kind, payload (the kind's fields as JSON
  text), sensitivity }`; `next-after` is set when a further page may
  exist; `unavailable` carries the typed reason when the `jinn:ledger`
  read was refused. Every page is receipted on the ledger under the
  status entry (`LedgerConsumed`).
- **`get`** → `ProfileDocument { api-version, profile }` — the document
  verbatim; typed `unavailable { finding: 25 }` beside the data root.
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
4. A patch that changes nothing answers `changed: false` and makes no
   kernel call (an identical rewrite would still reconcile `unchanged`
   on the daemon side — the seam does not spend a call to say so).
5. A changed entry is handed to the kernel's `jinn:profile`
   `patch-entry(id, { data?, grants? })` — one RFC 7396 merge patch on
   the entry's `config` (arrays replace whole, so rule 2 holds by
   construction). The LOADER validates it (an object whose grants would
   admit at activation), writes the whole document back atomically,
   commits the runtime view, restarts exactly the patched fiber (`cause:
   ConfigChanged`), and records `ProfilePatched { entry, by }` — operator
   intent with no fs inverse and no fiber journal entry, so disposing the
   editor never touches the document (FINDINGS.md #21 closed at pin
   `57360cc`). A kernel refusal — an entry outside the grant scope, a
   patch failing validation, the caller's own entry, the loader's
   retryable conflict (an operation in flight on the entry or the
   document) — is a typed `refused` answer carrying the reason and a
   `retryable` flag, and lands on the ledger (`AmendmentRefused` /
   `GrantRefused`). The API never bypasses the profile as the source of
   truth. The request's `idempotency-key` is accepted and unused.

## Additivity (the R12 promise, mechanically)

- **Every nesting level.** Every schema carries a flattened extension
  map (`Extensions`) at every level — the `Answer` envelope, each of
  its outcomes (`ok` is a lossless JSON value; `error` carries its own
  map), and every report and request object beneath them. A field a
  newer writer adds survives this reader verbatim across a decode →
  encode round trip: `{"ok":{"n":1},"future":true}` decodes as a
  recognized `ok` and re-encodes with `future` intact; the same holds
  for an unknown object nested inside `error`, `status.kernel`, or a
  patch's `config`.
- **Version on every answer.** `api-version` rides on every answer this
  seam produces — `ok` and `error` alike, including the transport's own
  refusals (a route miss, a malformed body, a malformed provider
  answer) — and on every report inside an `ok`. A foreign answer that
  omits it decodes as unversioned and re-encodes without one: the reader
  never invents a version on the writer's behalf.
- `kernel.unavailable` is a list of names: fields are removed from it as
  the kernel grows, never renamed — empty since 0.2.0, the object stays.
- The route table is append-only within v1; a breaking change is `/v2`.

## Changes

- **0.2.0 (2026-08-29, kernel pin `57360cc`):** additive. Entries carry
  the kernel's view (`fiber`, `state`, `incarnation`, `provisions`,
  `registrations`); `status` gains `readiness`, `last-ledger-seq`,
  `document`; `kernel.unavailable` empties; `ledger-tail` serves real
  pages; `patch-entry` applies through `jinn:profile` (no fs write;
  `idempotency-key` accepted, unused; refusals carry `retryable`);
  `health.ok` keys on the kernel's word. The `kernel` module carries the
  kernel contracts' wire shapes. FINDINGS.md #25 is the one new typed
  `unavailable`.
- **0.1.0 (2026-08-29, kernel pin `1b098be`):** first edition. Round 2:
  the `Answer` envelope gained `api-version` and its extension map (the
  first edition's envelope preserved nothing beside `ok`/`error` and
  versioned only `ok`); additive, no wire shape removed.
