# The UI bundle seam

The web UI as a plugin on the kernel — the first packet of the UI
malleability arc (UI-1, `docs/plans/ui-malleability-arc.md` §4). Roles
per the seam-triple naming law (AGENTS.md):

| Role | Package | What it is |
|---|---|---|
| Service definition | `jinn-ui` | The `jinn:ui-bundle` contract: operations `manifest` (paths, sha256, MIME, cache class, the document's name, the blob's hash) and `bundle` (the whole archive as one length-prefixed blob); the codec; `verify`, which FAILS CLOSED on any mismatch; the SERVING LAW as pure functions (the document `no-cache`, hashed `assets/` `immutable`, an unknown asset `404 text/plain` never the SPA fallback, every other non-`/v1` path the document, `.webmanifest` as `application/manifest+json`); the MIME table. Pure types + logic; compiled into the guests and the kit. |
| Provider | `jinn-ui-bundle-embedded` | Wasm plugin holding the built client COMPILED IN: `include_bytes!` of the archive and manifest `ui-kit` wrote. Its config is empty and it is granted only the contract it provides; its identity is its content hash, which is what makes a UI swap a profile edit of ONE entry's `package` and `hash`. |
| Consumer | `jinn-api-http` (`plugins/api/`) | The transport, when its profile mounts a bundle: resolves `jinn:ui-bundle` at `activate`, reads the manifest and the whole archive as one crossing each, verifies, and holds the files for the incarnation's life; answers `GET` on every non-`/v1` path from that memory BEFORE the door and with NO crossing — a byte is never a dispatch, a bearer on a static path is ignored — while every `/v1/*` request keeps the door as packet 2.8 left it. A bundle that does not verify fails the transport's activation and nothing else (R11). |

The direction of calls: the transport RESOLVES the bundle contract at
activation and never again; the provider PROVIDES. A second provider
shape (a bundle read from a directory, a bundle fetched by hash) is a new
package next to `jinn-ui-bundle-embedded` and a profile edit; the
composition suite proves the swap restarts the transport alone.

The contract surface is documented in `jinn-ui/README.md` — one home per
fact. Guest crates here are NOT workspace members (see the workspace
manifest's note): `cargo run -p ui-kit -- kit <root> --port N` builds the
web client with its pinned toolchain, archives it, builds and encodes the
provider, and writes the `ui` profile (`profiles/ui/README.md`). The
client itself lives at the repo root under `web/`, ported verbatim from
jinn `43e8647` behind the gate `tools/ui-kit/tests/verbatim.rs` over the
pinned map `web/port-map.txt`. Real-composition proof lives in
`tests/composition/tests/ui.rs`, which boots that profile through the
REAL pinned `jinnd` daemon.
