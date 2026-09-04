# The JS-in-WASM extension tier

An operator's extension as a plugin on the kernel — the second packet of
the UI malleability arc (UI-2, `docs/plans/ui-malleability-arc.md` §9).
An extension is a WATERFALL LISTENER: it folds the payload of every
moment it is granted (`plugins/ui/jinn-ui/README.md`, "Moments") and
does nothing else. Roles per the seam-triple naming law (AGENTS.md):

| Role | Package | What it is |
|---|---|---|
| Service definition | `jinn-ext` | The `jinn:ext` entry's config schema (`topics`, `source`, `origin`, and since pin `b1dbe8f` the optional per-delivery `budget` — CLOSED, an unknown field is an activation fault), the activation law's names (the four breadcrumbs, `source sha256:<hex>`), and the two JS programs an engine evaluates (the activation self-test, the per-delivery fold). Not a service anyone calls: nothing provides `jinn:ext`; the definition is types, compiled into the engine guests and the kit. |
| Provider | `jinn-ext-js-boa` | The first engine: a Boa guest (`boa_engine` 0.22, pure Rust — §5 measured that it builds for the plugin world and QuickJS does not without a libc). Holds the operator's source from its entry's `config.data`, listens on `data.topics` under the grants of those topic names — with `events.listen-within` under the entry's `budget` when it declares one, so a runaway delivery ends this guest's own instance and nothing else (M2-K25) — and folds each delivery in a FRESH Boa context on the kernel's clock (`jinn:clock` `now`, one crossing per delivery — its only host call). Its imports are exactly `types`, `effects`, `events`, `services` of `jinn:plugin@0.12.0`, asserted by `tools/ext-kit/tests/imports.rs`; the JS inside has no host calls, so it cannot re-enter a seam. |
| Consumers | none | An extension is reached only by the walk of a topic it is granted. The EMITTER is the transport (`plugins/api/jinn-api-http`, `moments.rs`), which is granted the three topics it emits. |

The direction of calls: the kernel DELIVERS to an extension; an
extension calls nothing but the clock. A second engine
(`jinn-ext-js-quickjs`, behind a libc shim or the kernel's KG-4) is a
new package next to `jinn-ext-js-boa` and a profile edit of the entry's
`package` and `hash` — the swap every seam proves, unreachable through
the operator API at this pin (`FINDINGS.md` #37; the K23 split in the
card, §9.5).

The activation law and the config schema are documented in
`jinn-ext/README.md` — one home per fact. Guest crates here are NOT
workspace members (see the workspace manifest's note):
`cargo run -p ext-kit -- build <artifacts-dir>` builds the provider for
wasm32-unknown-unknown, encodes it, and prints its size and sha256; the
`ui` profile (`tools/ui-kit`) mounts it as `ext-green`, the operator's
example from §6. Real-composition proof lives in
`tests/composition/tests/moments.rs`, which boots that profile through
the REAL pinned `jinnd` daemon.
