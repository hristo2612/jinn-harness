# The `ui` profile

The plugins profile plus ONE entry: `jinn-ui-bundle`
(`ui/jinn-ui-bundle-embedded`, granted only `jinn:ui-bundle`, empty
config), and the transport `jinn-api-http` additionally granted
`jinn:ui-bundle` and `jinn:introspect` (the transitions publish that
completes its read when the bundle entry activates after it, #45) with
`ui-bundle-entry: "jinn-ui-bundle"` in its data. Everything else is
exactly what `plugin-kit` mounts — the cron seam, the api trio, the
settings pair, the live and fixed catalogs, the shelved entry and the
deliberately failing one — so the ported plugins page has a tree with a
failure to show. Entry shapes live in the kit builders (`tools/ui-kit`,
`bundle_entry`, `mount_bundle_on`; the rest in `api-kit`, `plugin-kit`,
`cron-kit`); the document is GENERATED with honest artifact pins (kernel
Law 5), never hand-maintained:

```
cargo run -p ui-kit -- kit <root> --port N [--every-ms N] [--tick-ms N]
```

The kit runs the web client's pinned build first (`pnpm install
--frozen-lockfile && pnpm build` under `web/`; `pnpm` on `PATH`, Node
pinned by `web/.npmrc`), archives `web/out` into `<root>/ui-bundle/`
(`bundle.bin` + `manifest.json`), and compiles the archive into the
provider through `$JINN_UI_BUNDLE_DIR`. The bundle is built by the kit
and by CI and never vendored: its hash is the component's, printed by the
kit.

Boot it in the operator layout (`profiles/operator-api/README.md`), then
open `http://127.0.0.1:<port>/` in a browser: the pairing screen asks for
the operator credential — the contents of `<root>.operator-token`, the
launcher-written file beside the data root (packet 2.8) — and holds it in
`sessionStorage` for the tab's life. Settings and Plugins are the two
surfaces UI-1 ports; every `/v1` call carries the credential as a
bearer; every byte of the UI itself is served with no door and no
crossing.

Swapping the UI is a profile edit of the ONE bundle entry's `package`
and `hash` (a second kit-built provider under another name); the
transport witnesses the entry reach `Active` on the kernel's
`jinn:introspect/transitions` publish and re-reads — 1.24 s, no refused
connect, its own incarnation unchanged (`FINDINGS.md` #46: the epoch
gating the card assumed stops at the string lane) — measured in
`tests/composition/tests/ui.rs` (proof 4).
A bundle whose bytes do not match its manifest fails the transport's
activation and nothing else (proof 5).

The real-composition proofs for this tree live in
`tests/composition/tests/ui.rs`; the door's (`tests/composition/tests/auth.rs`)
hold on it unchanged.
