# The cron profile

The cron seam's plugin tree, two entries: `cron-scheduler` (the `jinn:cron`
provider, granted `jinn:cron`, `jinn:fs`, `jinn:clock`, and every job
topic it fires — `cron:health` in the shipped table; since pin `138fdce`
an emit is covered by the topic's own grant, and the kit derives these
from the job table) and
`health-snapshot` (the first real job, granted `cron:health`, `jinn:cron`,
`jinn:fs`). Entry shapes, grants, and the default job table live in the kit
builder (`tools/cron-kit`, `profile()`), which writes the document with the
honest artifact pins — a profile pins plugins by content hash (kernel
Law 5), so the document is GENERATED, never hand-maintained:

```
cargo run -p cron-kit -- kit <root> [--every-ms N] [--tick-ms N]
```

`--every-ms` is the `health` job's period; `--tick-ms` is the scheduler's
alarm period (see the settings namespace in
`plugins/cron/jinn-cron/README.md`). Both write into the generated profile.

The command writes `<root>/artifacts/*.wasm` (+ `.sha256` sidecars) and
`<root>/profile.json`. Boot it with the pinned kernel's daemon:

```
jinnd --profile <root>/profile.json --ledger <root>/ledger.sqlite
```

That is the whole duty setup: the scheduler holds its own `jinn:clock`
alarm, so nothing outside the daemon has to keep it running.

The real-composition proofs for this tree live in `tests/composition`.
