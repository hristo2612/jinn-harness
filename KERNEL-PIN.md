# Kernel Pin

The harness builds against exactly one kernel: the `jinnd` commit below. The
vendored copy of its contract surface lives in `kernel-pin/` and is what
plugin crates compile against; `cargo test -p harness-pin` is the gate that
keeps the pin, the vendored surface, and (when reachable) the kernel repo
itself in agreement.

```
repo: https://github.com/hristo2612/jinnd
commit: a17df864c99a72693839f31801e4fc3627bd3e15
wit-hash: sha256:7cd9d003aefdaf4d8e064d40013f97599161e2b8dd2429afdbb4b90279f03824
contracts-hash: sha256:3133cce002ce5e89ec53c8d4de3e89dcea43b07b8130962ff7e345ce215cfab3
```

`wit-hash` covers `wit/` (the `jinn:plugin` world). `contracts-hash` covers
`contracts/` (the capability contract bundles — `jinn:fs`, `jinn:ledger`, …).
Together they are the contract surface the kernel publishes (jinnd R12).

## Contract hash algorithm (normative)

Implemented exactly once, in `tools/harness-pin` (`harness_pin::contract_hash`);
the computing CLI and the verifying gate share that one implementation so they
cannot drift.

1. Collect every regular file under the directory, recursively.
2. Form each file's path relative to that directory, `/`-separated.
3. Sort the paths bytewise.
4. Feed `"<path>\n<sha256-hex-of-content>\n"` per file, in order, into SHA-256.
5. The hash is `sha256:` + lowercase hex of the digest. An empty directory
   therefore hashes to SHA-256 of empty input.

## The gate

- **Gate 1 — vendored surface (always on, fail-closed):**
  `kernel-pin/wit` and `kernel-pin/contracts` must hash to the pinned values.
  Runs everywhere, including CI, with no network and no credentials.
- **Gate 2 — kernel checkout (self-skipping):** when a jinnd repo is reachable
  (`JINND_DIR`, a sibling `../jinnd` checkout, or `JINND_CLONE_URL`), the
  pinned commit's `wit/` and `contracts/` trees must hash to the pinned
  values — the working tree is never consulted. jinnd is currently private, so
  CI runs this leg only when the `JINND_READ_TOKEN` secret is configured; the
  skip is loud, and Gate 1 still holds fail-closed.

## Pin-bump procedure

One commit, never implicit. To move the pin to a new jinnd commit `<C>`:

1. In a jinnd checkout, compute both hashes at `<C>`:
   `cargo run -p harness-pin -- compute-git <jinnd-dir> <C> wit` and
   `... compute-git <jinnd-dir> <C> contracts`.
2. Replace `kernel-pin/wit` and `kernel-pin/contracts` with the trees at `<C>`
   (`git -C <jinnd-dir> archive <C> wit contracts | tar -x -C kernel-pin`).
3. Update `commit`, `wit-hash`, and `contracts-hash` above.
4. `cargo test -p harness-pin` — both gates must pass.
5. Commit all of it together (`chore: bump kernel pin to <C>`), citing the
   jinnd change that motivated the bump.

A pin bump PR that touches only some of these pieces is wrong by construction —
Gate 1 fails it.
