# `jinn-api` 0.4.0 — the operator-API contracts

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
| Engine contracts | `jinn:engine.<engine-id>` | One provider slot per engine; routed to directly, like the settings seam. The contract, its operations and its types have ONE home: `plugins/engines/jinn-engine/README.md`. |
| Provider grants | `jinn:net` (scoped to one loopback port), both contracts above, `jinn:auth` (bare — the bundle declares no scope) | The HTTP provider's authority: transport + the right to call the consumers + the right to ASK the kernel who a connection is. No clock: it serves from the kernel's readiness wakes. |
| The door (0.4.0) | `jinn:auth` / `verify`, before every dispatch | The transport puts the request's `Authorization: Bearer` token (or NOTHING, when there is none) to the kernel's one decision point, one call per request, and dispatches only on a `principal`. The vocabulary is `auth.rs`; the names are asserted against the vendored `contract.wit` by parsing (`tests/auth_mirror.rs`). |
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
| `GET` | `/v1/engines` | one `jinn:engine.<id>` / `describe` per routable engine | — |
| `GET` | `/v1/engines/{engine}` | `jinn:engine.{engine}` / `describe` | — |
| `POST` | `/v1/engines/{engine}/runs` | `jinn:engine.{engine}` / `run` | the JSON body (a `RunRequest` minus `engine`) + `engine` from the path |
| `GET` | `/v1/engines/{engine}/runs/{run-id}` | `jinn:engine.{engine}` / `run-get` | `{run-id}` from the path |
| `DELETE` | `/v1/engines/{engine}/runs/{run-id}` | `jinn:engine.{engine}` / `cancel` | `{run-id}` from the path |

The settings rows (0.2.0) route to the settings seam's provider directly:
its envelope is this seam's envelope (`plugins/settings/jinn-settings/README.md`),
so the transport maps its typed errors to the same status codes. Each
route names the request field its path parameter lands in (`param`) and
whether the payload is the body or the query (`body`).

HTTP status mapping of the typed error codes is the provider's
(`jinn-api-http-wire`): `not-found` 404, `invalid` 422, `unavailable`
503, `refused` 502, `unauthenticated` 401 (with the `WWW-Authenticate:
Bearer` challenge); a route miss is 404/405.

`unauthenticated` (0.4.0) is its own class because its next move is its
own: present the operator's credential, or stop. It is neither `refused`
(a grant or provider said no — the caller's profile to widen) nor
`unavailable` (the transport — worth retrying). Its `detail` is the
kernel's reason and never carries credential bytes. A door that cannot
ASK — `jinn:auth` unresolvable, the crossing refused, an answer off the
contract's wire — is `refused`: closed, and named as the composition's
defect rather than the operator's.

## The engines routes (0.3.0)

The engines rows carry TWO path parameters and name a DIFFERENT contract
per engine, so they are their own small table (`engines.rs`) rather than
rows of the static one. `GET /v1/engines/{engine}` is one engine's own
`describe`, verbatim — the row `GET /v1/engines` carries for it. Three rules, all of them answers:

1. **Which engines.** The engines an API may route to are its entry's
   `config.data.engines`, written by the profile from the same source as
   its `jinn:engine.<id>` grants. The GRANT is the authority the kernel
   enforces; the setting is that same fact told to the provider, so an
   engine id outside it is `not-found` **without a kernel call**.
2. **An unmounted engine is an ordinary answer.** A `resolve` that fails
   because no entry provides that contract is typed `unavailable` naming
   the engine — never a fault, never a 500. A composition may simply not
   hold that engine.
3. **The engines seam's error class survives.** `EngineError.code` maps
   onto this seam's: `invalid` → `invalid` (422), `not-found` →
   `not-found` (404), `refused` → `refused` (502), `unavailable` →
   `unavailable` (**503**), `failed` → `refused` (502). The seam's own
   code rides along additively as `error.engine-code`, so `failed` is
   never read as `refused`, and `unavailable` — the honest environment
   gate: the provider is mounted and correct, this host cannot carry the
   run — keeps a status of its own.

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
  `engines` (0.3.0) is every engine the composition holds — `{engine,
  contract, entry}` per `jinn:engine.<id>` an entry PROVIDES, read off
  the kernel's own `provisions` (`jinn_engine::engines_in`), sorted by
  engine id. It is not a table the status plugin keeps: an engine is
  there because its entry is mounted, and gone when the entry is.
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
- **`GET /v1/engines`** → `EngineList { api-version, engines:
  [EngineEntry] }`, one row per routable engine, sorted by engine id:
  `{ engine, contract, describe?, error? }` — `describe` is that
  provider's own `Description` verbatim (the engines seam's schema), and
  `error` the typed reason it could not answer. Exactly one of the two.
- **`GET /v1/engines/{engine}`** → that provider's `Description`
  verbatim, or the typed refusal — the one row of the list, unwrapped.
- **`POST /v1/engines/{engine}/runs`** ← a `RunRequest` minus `engine`
  (the path supplies it; a body naming another engine is not a second
  opinion) → the provider's `RunAccepted`. **`GET …/runs/{run-id}`** →
  its `RunRecord`; **`DELETE …/runs/{run-id}`** cancels it and answers
  the record. All three schemas belong to the engines seam and are
  documented there — this seam passes them through verbatim.
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

- **0.4.0 (2026-09-02, kernel pin `85d36b4`):** additive. The door:
  `unauthenticated` joins the error classes (401 on the HTTP provider,
  with its challenge), `auth.rs` carries the `jinn:auth` names and wire
  decode, and every route now owes one `verify` before its dispatch.
  Existing routes, schemas and status codes are unchanged; a request
  that presents no credential is now answered 401 where it was answered
  before, which is the boundary this edition exists to add.
- **0.3.0 (2026-08-29):** additive. The engines surface: five routes onto
  `jinn:engine.<id>` (list, describe, run, run-get, cancel), the list's
  schema, the engines-seam error mapping, and `engines` on the status
  report. No existing route, schema or status code changed.
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
