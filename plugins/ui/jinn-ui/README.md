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

## What is deliberately NOT here

Compression (the old gateway memoised brotli/gzip per path+mtime+
encoding; this transport answers identity bytes, and the rule that
survives — never one encoding's bytes for another — is moot without a
second encoding). Keep-alive (every response closes; a page load is N
connections). The service worker (dropped in UI-1, plan §8 question 4).
Range requests. `HEAD`.
