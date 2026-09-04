# `jinn-ext` 0.1.0 — the extension entry

The service definition of the JS-in-WASM extension tier. This document
is the tier's prose law; the types in `src/lib.rs` are its schema.
Within 0.x every change is strictly additive (the kernel's R12
discipline).

## Names

| Name | Value | What it is |
|---|---|---|
| Seam | `jinn:ext` | Provided by nothing. An extension is a listener; the name is the seam group's, not a contract a caller resolves. |
| Package | `ext/jinn-ext-js-boa` | The first engine provider's artifact. |
| Grants | the topics in `data.topics`, plus `jinn:clock` | Each topic is its own grant name (`plugin.wit`, `events.listen`); the clock is the ONE kernel host provider an engine reads. Nothing else — no seam, no fs, no net. |
| `injects` | absent | An extension injects no service. |

## The entry

```json
{ "id": "ext-green", "package": "ext/jinn-ext-js-boa", "hash": "<the provider's component sha256>",
  "config": { "grants": ["jinn:ui/before-send", "jinn:clock"],
              "data": { "topics": ["jinn:ui/before-send"],
                        "source": "(p) => ({ ...p, text: p.text + ' 🟢' })",
                        "origin": "human" } } }
```

`config.data` is CLOSED (`ExtConfig`, `deny_unknown_fields`): an unknown
field is an activation fault naming it (R3; the settings seam's
closed-surface law). The UI-2 card withheld a `budget` field because
nothing at pin `a53a352` could honor one, and a declared field the guest
cannot enforce is a lie on the record (KG-2, `FINDINGS.md` #48); the
kernel honors one since pin `b1dbe8f` (jinnd M2-K25), and the field is
below.

- `topics` — the topics the extension listens on. Each must ALSO be in
  `config.grants`: the grant is the authority the kernel enforces, the
  list is the listener's statement of intent. A topic listed twice is a
  per-entry fault (one listen per topic; the kit never writes it, the
  guest refuses it).
- `source` — one JS expression evaluating to a function of the payload.
  The operator's code is DATA to a signed plugin (§8 ruling 1): its
  authority is the grant list and nothing else.
- `budget` — optional, `{ "fuel": <u64> }`: the kernel's
  `delivery-budget` record (`plugin.wit` 0.11.0) spelled on the entry.
  When present, every topic is listened on with `events.listen-within`
  and each delivery spends at most that much of the listener store's
  own fuel — deterministic: the same source exceeds the same budget at
  the same instruction on every machine. Exceeding it ends THIS
  extension's instance and fails its own fiber on the record (`guest
  exhausted its delivery fuel budget`, then `Active → Unloading →
  Failed` under `BodyFaulted`); the walk continues past it as one
  contained failure (R9). Absent, a plain `listen`: the guest deadline
  is the bound. Zero is carried as declared and refused by the kernel at
  `listen`, `invalid`, on the record — the provider never clamps. The
  `ui` profile mounts `ext-green` under `ext_kit::GREEN_BUDGET`.
- `origin` — `agent | human`: who wrote the source. Constitution 05's
  `[provenance] origin` restated for data; the operator's declaration on
  the entry, shown by the plugins page as the row's `attestation.origin`
  and read by nothing else. The plugins catalog carries the source's
  digest beside it as `attestation.source` (`sha256:<hex>`,
  `source_digest`; the same digest the guest writes on the ledger as its
  `source sha256:` breadcrumb), so the page's breadcrumb is a STABLE
  reading of the entry and never a sliding history window (plan §9.7
  amendment 8(d)).

## The activation law

An engine provider's `activate`, in this order, each step an
`EffectRegistered` row on the ledger (the activation discipline until
`FINDINGS.md` #38 closes — a fiber that fails between two breadcrumbs
says so by which was written last):

1. `activate entered`
2. `config parsed` — the closed schema above.
3. `js context built` — on the kernel's clock: `jinn:clock` `now`
   resolved and read once (a JS engine needs a clock and a guest has
   none, §5.4 lesson 1).
4. `js evaluated` — the source evaluated ONCE (`self_test`); a syntax
   error or a value that is not a function FAILS THE FIBER, never a
   silent no-op listener (R11).
5. `source sha256:<hex>` — WHAT CODE RAN, on the record (Law 2).
6. One `events.listen` per topic in `data.topics` — `events.listen-within`
   under the entry's `budget` when it declares one. A listen the kernel
   refuses (`GrantRefused`, or `invalid` for a zero budget, on the
   record) fails the activation; the guest never swallows it. The kernel
   labels each listen `listen <topic>`, budgeted or not.

## The delivery

`handle-event(token, topic, payload)` = the `delivery` program in a
FRESH Boa context on the kernel's clock: the payload parsed as JSON, the
source applied, the answer folded with `JSON.stringify`. Three answers:

| The source | The provider answers | The kernel does |
|---|---|---|
| returns an object | its JSON | makes it the payload for the next listener |
| returns `undefined` | EMPTY bytes | leaves the payload unchanged (pass-through) |
| throws, or returns anything else | a contained guest fault | records the failure (`failures + 1` on the walk's `DispatchTrace`), continues the walk (R9) |

A returned object with an ADDED unknown field is accepted: the fold is
the listener's; the schema binds the client's input, not the walk's
output. A context per delivery is the spike's shape, "correct and slow";
its cost is measured in `tests/composition/tests/moments.rs` (proof 2)
and no reuse is designed before that number exists (§9.5).

## What an extension cannot do, by construction

Call any seam (its component imports no `fs`, `net`, `process`,
`keystore`; its one `services.call` targets a kernel host provider, so
the #4/#32 wait cycle has no target); see any moment it is not granted;
change a decision that is not a waterfall; outlive its entry; or, since
pin `b1dbe8f`, spend anyone's clock but its own — a delivery that loops
ends this extension's instance at its budget or its deadline, on its own
row, and the transport that emitted is charged nothing (proof 7;
`FINDINGS.md` #48, answered).
