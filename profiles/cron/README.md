# The cron profile

The cron seam's plugin tree: `cron-tick-source` (the timer stand-in),
`cron-scheduler` (the `jinn:cron` provider), `health-snapshot` (the first
real job). Entry shapes, grants, and the default job table live in the kit
builder (`tools/cron-kit`, `profile()`), which writes the document with the
honest artifact pins — a profile pins plugins by content hash (kernel
Law 5), so the document is GENERATED, never hand-maintained:

```
cargo run -p cron-kit -- kit <root> [--every-ms N]
```

writes `<root>/artifacts/*.wasm` (+ `.sha256` sidecars) and
`<root>/profile.json`. Boot it with the pinned kernel's daemon:

```
jinnd --profile <root>/profile.json --ledger <root>/ledger.sqlite
```

and put it on duty (the timer stand-in's driver, FINDINGS.md #1):

```
cargo run -p cron-kit -- tick <root>/profile.json --interval-s 900
```

The real-composition proofs for this tree live in `tests/composition`.
