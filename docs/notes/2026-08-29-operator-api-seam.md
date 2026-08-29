# Agent note — phase 2.1: the operator-API seam (pin `1b098be`)

The `1b098be` pin (jinnd M2-K6) brought `jinn:process` and `jinn:net`
(FINDINGS.md #5 closed for both; `jinn:keystore` is still unprovided) on
the `jinn:plugin@0.4.0` world. This note records the non-obvious choices
of the first core-port seam built on it; the contract law lives in
`plugins/api/jinn-api/README.md`, the layout in
`profiles/operator-api/README.md`, the frictions in `FINDINGS.md`.

## Why the consumers PROVIDE and the provider RESOLVES

The seam-triple roles are about the schema: the definition owns it, the
provider owns transport, the consumers inject the schema to implement
operations. On the broker that inverts the call direction — the HTTP
guest resolves `jinn:api-status` / `jinn:api-profile` per request and
calls the consumer that provides it. Making the consumers the broker-side
providers is what makes the provider swap trivial (a transport is an
entry that holds a listener and two grants; the consumers never learn
which transport is asking) and keeps every request exactly one ledgered
contract call. The alternative — the transport providing a contract the
consumers call to register handlers — would put routing state in the
transport and make a swap a consumer restart.

## Why the swap proof uses a second entry of the same artifact

`jinn:net` v0.1 binds loopback TCP only, so no second transport SHAPE can
exist yet (a unix-socket provider needs a bundle edition). The
composition proof therefore swaps the provider ENTRY: `jinn-api-http`
leaves, `jinn-api-http-b` (same artifact, its own id and port) arrives,
by one profile edit, and the proof asserts the consumers' fibers did not
cycle and the API answers on the new port. That is the exact edit an
operator makes to switch transports later; what a real second shape adds
is a different artifact hash in the same edit.

## Why status says "unavailable" instead of reading the daemon log

The daemon knows every fiber's state (`status` on stdin logs it) and the
ledger knows every event, but neither is reachable by a guest: no
introspection contract, no `jinn:ledger` provider. `jinn-status` could
have guessed — "five entries in the profile, so five Active fibers" — and
the brief forbids exactly that. The report names each unanswerable field
with its finding number so a reader knows what the number means and the
field list shrinks, additively, when the kernel grows the contract. The
probes are the honest substitute: a granted `resolve` + `jobs` call proves
the scheduler is live and shows the schedule it holds, through the
broker, on the record.

## Why the operator layout puts the data root at the kit root

The edit lane writes the profile through `jinn:fs`, and `jinn:fs`
resolves under the data root — so the profile must live inside it.
`--data <root>` was chosen over a nested `data/operator/` directory
because the daemon watches the profile's parent for edits: at the root,
the only things beside the profile are the ledger and the artifacts
directory (both classified away by the watcher), while a nested directory
would need `--artifacts`/`--data` re-pointed anyway. The cron proofs keep
their `<root>/data` layout untouched; the api proofs get their own
`boot_operator` and `fresh_api_root`.

## Why the FINDINGS #21 transcript is a passing test

`disposing_the_editor_reverts_the_operators_edit_finding_21` proves a
kernel behavior the harness does NOT want: removing the editor entry
withdraws its profile write, the pre-patch document (editor included)
comes back, and the patched entry restarts on its old config. Pinning it
as a passing proof is the cron seam's precedent (#14's transcript went
red the day the kernel retired it): when the kernel gives the edit lane a
non-revertible shape, this test fails first and is replaced by the proof
of the new law.

## Why the HTTP codec is its own native crate

The provider guest is a wasm-only cdylib and cannot run host tests; the
cron seam's discipline is pure logic in a native crate, thin IO in the
guest. The codec is transport, so it belongs to the provider, not the
definition — hence `jinn-api-http-wire` beside `jinn-api-http` rather
than a module in `jinn-api`.

## Why the answer envelope is a struct, not a bare enum (round 2)

The first edition's `Answer` was the bare externally-tagged enum
`{"ok": …} | {"error": {…}}`. Serde rejects any sibling of the tag on such
an enum, so the verifier's probe `{"ok":{"n":1},"future":true}` decoded
as a `refused` — the one place on the seam where a newer writer's field
was not preserved, and the one answer shape (`error`) that carried no
`api-version`. The envelope is now a struct: `api-version`, the
`Outcome` enum flattened, and a flattened extension map flattened after
it (order matters: the enum consumes its tag first, the map keeps the
rest). `ok` stays a lossless `serde_json::Value` — every nested unknown
survives by construction — and `error` keeps its own map, so the
guarantee holds at the envelope, the variant, and every level beneath.
`api-version` is `Option` on purpose: the seam always writes it, but a
foreign answer that omits it must round-trip without one; inventing a
version for a writer that stated none would be a lie on the wire.
