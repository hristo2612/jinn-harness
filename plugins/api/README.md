# The operator-API seam

The daemon's operator surface as plugins on the kernel — the first
core-port seam under the malleability contract (phase 2.1). Roles per the
seam-triple naming law (AGENTS.md):

| Role | Package | What it is |
|---|---|---|
| Service definition | `jinn-api` | The `jinn:api-status` / `jinn:api-profile` contracts: operation names, versioned additive request/answer schemas, the typed error and answer envelope, the entry-patch law (RFC 7396 merge on ONE entry's config), the status shape built from the profile document, and the route table every transport exposes. Pure types + logic; compiled into both guests and host tools. |
| Provider | `jinn-api-http` | Wasm plugin owning TRANSPORT only: one `jinn:net` loopback listener at the port its grant is scoped to, served from the kernel's readiness wakes (`jinn:net/readable`, pin `57360cc`) — no alarm, no clock grant; minimal HTTP/1.1 + JSON (`jinn-api-http-wire`, the native-tested codec beside it); every request is exactly one granted contract call on a consumer. A bind refusal fails this entry alone. |
| Consumer | `jinn-status` | Provides `jinn:api-status`: `status`, `health`, `ledger-tail` from the kernel's own knowledge — the composition through `jinn:introspect`, the ledger through `jinn:ledger`, the document of record's authority fields through its scoped `jinn:fs` (where the document sits under the data root; elsewhere the report says so by number, FINDINGS.md #25), and provider probes through granted contracts (`jinn:cron` `jobs`). Nothing guessed. |
| Consumer | `jinn-profile-edit` | Provides `jinn:api-profile`: `get`, `patch-entry` — FINDINGS.md #9's operator edit lane. Applies the entry-patch law to ONE entry through the kernel's `jinn:profile` `patch-entry`: the loader validates, writes the document back atomically, restarts exactly the patched fiber, records `ProfilePatched` — operator intent with no fs inverse (#21 closed). The profile is never bypassed as the source of truth. |

The direction of calls: the provider RESOLVES the consumers' contracts and
calls them; the consumers PROVIDE. "Consumer" is the seam-triple role
(the plugin that injects the service definition's schema), not the broker
direction. A second provider shape (a unix socket, a pipe) is a new
package next to `jinn-api-http` and a profile edit — the composition suite
proves the swap leaves both consumers' fibers untouched.

The contract surface (operations, schemas, the patch law, additivity) is
documented in `jinn-api/README.md` — one home per fact. Guest crates here
are NOT workspace members (see the workspace manifest's note):
`cargo run -p api-kit -- kit <root> --port N` builds them for
wasm32-unknown-unknown beside the cron seam's guests, encodes each to a
component, and writes the pinned operator profile
(`profiles/operator-api/README.md`). Real-composition proof lives in
`tests/composition/tests/api.rs`, which boots that profile through the
REAL pinned `jinnd` daemon.
