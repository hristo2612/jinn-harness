# The settings seam

Per-plugin settings as a capability on the kernel — the second core-port
seam under the malleability contract (phase 2.2). Roles per the
seam-triple naming law (AGENTS.md):

| Role | Package | What it is |
|---|---|---|
| Service definition | `jinn-settings` | The `jinn:settings` contract: namespace declarations (schema, defaults, hot keys), the closed schema language and its validator, typed secret references (`{"$secret": "<key>"}` — never a value), layered resolution (defaults < owner entry < overlay), the patch plan (which layer a patch lands in, hence whether the owner restarts), the `changed`/`refused` payloads, the answer envelope. Pure types + logic; compiled into both guests and host tools. |
| Provider | `jinn-settings-profile` | Wasm plugin providing `jinn:settings` over the profile document as the ONE source of truth: the entry layer is the owner's `config.data` as declared, the overlay is the `jinn-settings-store` entry's `config.data.overlays` (read on every resolution through `jinn:settings-store`), and every applied patch is a kernel `jinn:profile` `patch-entry` — of the owner (restart path) or of the store (hot path). A second provider shape is a package beside it and a profile edit. |
| Provider (store) | `jinn-settings-store` | The hot layer's home: provides `jinn:settings-store`, answers its own `config.data.overlays`. Its trivial fiber is what a hot patch restarts, never the owner. |
| Consumer | `cron-scheduler` (`plugins/cron/`) | Declares the `cron` namespace and consumes its job table through `jinn:settings` — on every alarm wake (`declare` answers the resolved settings) and in place from a `changed` event. The definition did not change for the migration. |
| Consumer | `jinn-api-http` (`plugins/api/`) | Exposes `GET /v1/settings`, `GET /v1/settings/{ns}`, `PATCH /v1/settings/{ns}` — the settings envelope is the operator-API envelope, so the transport carries the seam unchanged. |

The contract surface is documented in `jinn-settings/README.md` — one
home per fact. Guest crates here are NOT workspace members (see the
workspace manifest's note); `api-kit` builds them beside the cron and api
guests into the operator profile (`profiles/operator-api/README.md`).
Real-composition proof lives in `tests/composition/tests/settings.rs`.
