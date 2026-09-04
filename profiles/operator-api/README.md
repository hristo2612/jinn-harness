# The operator-API profile

The api trio and the settings pair mounted beside the cron seam, seven
entries: `cron-scheduler` and `health-snapshot` exactly as `profiles/cron`
mounts them (one home: `cron-kit`'s `cron_entries`; the scheduler holds
`jinn:settings`, the `jinn:settings/changed` listen grant and its job
topics' emit grants in both profiles — in the cron-only profile the
resolve answers missing-dependency and the entry layer is the whole
truth),
`jinn-settings-profile` (granted `jinn:settings` to provide,
`jinn:settings-store`, and `jinn:profile` scoped to exactly the entries
it may patch: every namespace owner and the store) and
`jinn-settings-store` (granted only what it provides; its
`config.data.overlays` is the hot layer's home), plus `jinn-api-http` (granted `jinn:net`
scoped to exactly its port, both api contracts, the settings contract,
and `jinn:auth` — the door: every request is put to the kernel's
`verify` before it dispatches, packet 2.8; the bundle declares no scope,
so the grant is bare),
`jinn-status` (granted `jinn:api-status`, `jinn:cron` for its probe, the
read-only kernel contracts, and `jinn:profile` over every entry
attenuated to `ops: ["entry", "document"]` — a viewer that cannot patch)
and `jinn-profile-edit` (granted `jinn:api-profile` and `jinn:profile`
over every entry with the reads AND `patch-entry`). Entry
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

The profile document is READ and WRITTEN through the kernel's own
`jinn:profile` since pin `3fd7b05` — `document` for the read, `patch-entry`
for the edit — so neither depends on where the document sits and this
layout is a convenience, not a requirement (FINDINGS.md #25 closed; the
soak's layout, the profile beside the data root, serves the same complete
answers). With
`--data <root>` the cron seam's files land at `<root>/cron/` and
`<root>/health/`, the kernel's inverse-retention store at
`<root>.inverses/` (a sibling of the root, the kernel's `<data>.inverses`
rule), the `jinn:auth` credential of record at `<root>.operator-token`
(the same sibling rule; the launcher writes it, mode 0600, and the daemon
only reads it — the composition rig provisions it in `Daemon::spawn`, the
soak's wrapper in `tools/soak/provision-token.sh`), and the daemon watches
`<root>` for the profile and `<root>/artifacts` for swaps — nothing else
under the root is classified by the watcher.

Authority over the document is exactly as wide as its use: a grant carries
an operation class since pin `3fd7b05`, so the status consumer's
`jinn:profile` grant names the reads alone and holds no write authority at
all, while the editor's names the reads and `patch-entry` (FINDINGS.md #24
closed). Neither consumer holds any `jinn:fs` grant on the document. The
editor's write is no longer a fiber effect (#21 closed): disposing
the editor leaves the document exactly as patched.

The API serves the harness's own composition only — loopback, one port,
no production routing (AGENTS.md cutover rule) — and, since packet 2.8,
only the operator: a request presents the credential as
`Authorization: Bearer <value>` or is answered a typed 401
(`docs/notes/2026-09-02-the-door-presents-what-it-was-given.md`). The
real-composition proofs for this tree live in
`tests/composition/tests/api.rs`; the door's in
`tests/composition/tests/auth.rs`.
