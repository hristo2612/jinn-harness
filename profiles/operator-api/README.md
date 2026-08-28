# The operator-API profile

The api trio mounted beside the cron seam, five entries: `cron-scheduler`
and `health-snapshot` exactly as `profiles/cron` mounts them (one home:
`cron-kit`'s `cron_entries`), plus `jinn-api-http` (granted `jinn:net`
scoped to exactly its port, `jinn:clock`, and both api contracts),
`jinn-status` (granted `jinn:api-status`, `jinn:fs` scoped to
`profile.json`, and `jinn:cron` for its probe) and `jinn-profile-edit`
(granted `jinn:api-profile` and `jinn:fs` scoped to `profile.json`). Entry
shapes and grants live in the kit builder (`tools/api-kit`,
`api_entries`); the document is GENERATED with honest artifact pins
(kernel Law 5), never hand-maintained:

```
cargo run -p api-kit -- kit <root> --port N [--every-ms N] [--tick-ms N]
```

## The operator layout

Boot it with the data root AT the kit root:

```
jinnd --profile <root>/profile.json --ledger <root>/ledger.sqlite --artifacts <root>/artifacts --data <root>
```

The profile document is edited THROUGH `jinn:fs` (the consumers hold a
`path-prefix` scope of `profile.json`), and `jinn:fs` resolves every path
under the daemon's data root — so the document must sit inside it. With
`--data <root>` the cron seam's files land at `<root>/cron/` and
`<root>/health/`, the kernel's inverse-retention store at
`<root>.inverses/` (a sibling of the root, the kernel's `<data>.inverses`
rule), and the daemon watches `<root>` for the profile and
`<root>/artifacts` for swaps — nothing else under the root is classified
by the watcher.

Two authority consequences of that layout are on the record: a `jinn:fs`
grant cannot be attenuated to read-only, so `jinn-status` holds a
write-capable scope on the document it only reads (FINDINGS.md #24); and
the editor's write is a revertible effect of its entry (#21).

The API serves the harness's own composition only — loopback, one port,
no production routing (AGENTS.md cutover rule). The real-composition
proofs for this tree live in `tests/composition/tests/api.rs`.
