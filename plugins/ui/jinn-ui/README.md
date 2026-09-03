# `jinn-ui` 0.1.0 — the UI bundle contract

The service definition of the UI bundle seam. This document is the
contract's prose law; the types in `src/lib.rs` are its schema. Within
0.x every change is strictly additive (the kernel's R12 discipline):
the manifest carries `api-version`, and every wire shape preserves
unknown sibling fields across a decode → encode round trip (the
distribution's additivity law, `plugins/settings/jinn-settings/src/wire.rs`).

## Names

| Name | Value | What it is |
|---|---|---|
| Contract | `jinn:ui-bundle` | Provided by a bundle provider; injected by the transport at activation. |
| `manifest` | operation | Answers the [manifest](#the-manifest), UTF-8 JSON, kebab-case keys. Takes no request. |
| `bundle` | operation | Answers the [archive](#the-archive) as one blob. Takes no request. |
| Provider grants | the contract itself | A bundle provider holds no other authority; its config is empty. |
| Consumer grants | the contract itself, plus `ui-bundle: true` in the transport's data | The grant is the authority the kernel enforces; the data flag is that fact told to the transport, the discipline of its `engines` list. |

An unknown operation is answered with the envelope every seam's error
carries — `{"api-version", "error": {"code": "not-found", "detail"}}` —
never a fault (R11).

## The manifest

```json
{ "api-version": "0.1.0",
  "document": "index.html",
  "bundle-sha256": "<lowercase hex sha256 of the whole bundle blob>",
  "files": [ { "path": "assets/index-3f2a.js", "sha256": "<hex>",
               "mime": "application/javascript", "immutable": true }, … ] }
```

`path` is `/`-separated, relative to the bundle root, with no leading
slash. `mime` is the `Content-Type` the file is served with, from the
table in `mime_of` (the old gateway's, inventory §2.16: `.webmanifest`
MUST be `application/manifest+json` or the install prompt never appears).
`immutable` is true for every file under `assets/` — the build hashes
their names — and false for everything else, the document above all.

## The archive

One blob: `u32-LE count`, then per file `u32-LE path length`, the path
bytes, `u32-LE byte length`, the bytes. `encode_bundle` / `decode_bundle`
are the codec; a truncated blob, trailing bytes, or a non-UTF-8 path is
refused typed.

## Verification (fail closed)

`verify(manifest, blob)` answers the files by path only when ALL of:
the blob hashes to `bundle-sha256`; every file the manifest names is in
the archive and hashes to its `sha256`; no file in the archive is
unnamed; the document is among them. The first mismatch is the error,
naming the file. A transport that cannot verify does not serve: its
activation fails, contained to its own entry (R11), and its listener is
never opened.

## The serving law

For a `GET` on a path that is not the operator API (`is_api_path`:
exactly `/v1` or `/v1/…`, case-sensitive), `serve` answers:

| Path | Answer |
|---|---|
| a segment `..`, an empty segment (`//`), or a first segment spelling `v1` in any other case | `404 text/plain` — nothing: neither a page nor a route, and no dispatch |
| `/assets/<x>` the bundle holds | 200, its MIME, `Cache-Control: public, max-age=31536000, immutable` |
| `/assets/<x>` it does not hold | `404 text/plain` — NEVER the document (inventory §2.15: the fallback's `text/html` for a `.js` request looked like an unrecoverable MIME error for a merely superseded chunk) |
| any other path the bundle holds (`/manifest.webmanifest`, `/icons/…`) | 200, its MIME, `Cache-Control: no-cache` |
| everything else (`/`, `/settings`, `/settings/plugins`, a deep client route) | the document, `text/html; charset=utf-8`, `Cache-Control: no-cache` (inventory §2.16: iOS Safari over a tunnel hostname caches HTML indefinitely) |

A method other than `GET` on a static path is `405 text/plain`. A
transport whose profile mounts no bundle answers every static path
`503 text/plain` and keeps serving `/v1`.

None of this consults the door or crosses into a guest: the bytes are
the transport's own memory, filled once at activation. A bearer
presented on a static path is ignored — never read, never put to
`jinn:auth` — which the composition suite proves on the ledger
(`tests/composition/tests/ui.rs`, proof 2).

## Moments (UI-2, plan §9)

A MOMENT is a `waterfall` walk on a `jinn:ui/<topic>` topic that the
transport dispatches when an AUTHENTICATED client calls
`POST /v1/moments/<domain>/<topic>` with the moment's payload, and
answers with the FOLDED payload: listeners in the order the walk deals
(nothing declares it and no reading exposes it, FINDINGS #52), a
non-empty output replacing the payload for the next, the final payload
the one answer, one `DispatchTrace` row per walk. The vocabulary is
CLOSED (R3) — `src/moments.rs` is its schema:

| Topic | Payload | Inventory |
|---|---|---|
| `jinn:ui/before-send` | `{ "text", "attachments": [], "session-id" }` (`BeforeSend`) | §4.3 moment 1 |
| `jinn:ui/before-create-session` | the sessions seam's `SessionSpec` | §4.3 moment 3 |
| `jinn:ui/before-patch-settings` | `{ "namespace", "patch": { … } }` (`BeforePatchSettings`) | §4.3 moment 19 — the one moment the ported shell reaches (the Settings save) |

**The path law** (`moment_topic`): `/v1/moments/<domain>/<topic>` maps
to `jinn:<domain>/<topic>` for exactly the topics above, byte for byte;
anything else — another topic, a `..` segment, a case variant, a
trailing slash, `/v1/moments/introspect/transitions` — is a 404 with NO
dispatch. The vocabulary is closed, not forwarded: a route that relies
on the kernel's refusal is a route that dispatched.

**The answers**, in the order the transport decides them (after the
door, `plugins/api/jinn-api-http/src/moments.rs`):

| Case | Answer |
|---|---|
| not a named topic | `404 not-found`, no dispatch |
| a method other than `POST` | `405`, no dispatch |
| a body off the topic's schema (`validate_moment`) | `422 invalid`, no dispatch — the schema binds the client's INPUT; the walk's output is the listeners' and is not re-checked |
| the walk delivered (zero or more listeners, any number of contained failures) | `200`, the folded bytes — with no listener, the body itself |
| the walk REFUSED WHOLE by the kernel: `restarting`, `gone`, `suspended`, `stalled` (M2-K9), `cycle` (M2-K10) | `503 unavailable`, `detail` opening with the refusal's name, the typed `refusal` field beside it |

**Fail-closed.** A refused walk is never answered with the unmodified
payload: a validator extension ("refuse a send containing an API key")
is defeated by fail-open, so the send waits for the walk or does not
happen. The client's retry-once after a `503` belongs with the composer
(UI-6); the Settings adapter surfaces the refusal as its conflict notice
and does not retry.

The emitter — the transport — is granted the three topic names as the
profile's statement of what it emits (`tools/ui-kit`, `mount_moments_on`).
Listeners are the extension tier (`plugins/ext/README.md`). Every claim
above is proven on the ledger in `tests/composition/tests/moments.rs`.

## What is deliberately NOT here

Compression (the old gateway memoised brotli/gzip per path+mtime+
encoding; this transport answers identity bytes, and the rule that
survives — never one encoding's bytes for another — is moot without a
second encoding). Keep-alive (every response closes; a page load is N
connections). The service worker (dropped in UI-1, plan §8 question 4).
Range requests. `HEAD`.
