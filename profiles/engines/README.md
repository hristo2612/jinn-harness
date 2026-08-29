# The engines profile

The engines seam mounted beside the operator API, the settings pair and the
cron seam — the tree the seam's real-composition proofs boot. Entry shapes
and grants live in the kit builder (`tools/engine-kit`), which writes the
document with honest artifact pins, so the profile is GENERATED and never
hand-maintained:

```
cargo run -p engine-kit -- kit <root> --port N \
    [--claude-bin PATH] [--codex-bin PATH] [--probe-every-ms N] \
    [--every-ms N] [--tick-ms N]
```

Boot it in the operator layout:

```
jinnd --profile <root>/profile.json --ledger <root>/ledger.sqlite \
      --artifacts <root>/artifacts --data <root>
```

## What it mounts

| Entry | Role |
|---|---|
| `jinn-engine-default` | The SWITCHABLE slot — engine id `default`, served by the echo package at kit time. |
| `jinn-engine-claude` | Mounted only when the `claude` CLI is on this host. |
| `jinn-engine-codex` | Mounted only when the `codex` CLI is on this host. |
| `jinn-engine-probe` | The consumer: one prompt through engine `default` on a schedule, recorded under `engine-probe/`. |
| the api trio, the settings pair, the cron pair | Unchanged, from `profiles/operator-api` and `profiles/cron`. |

`jinn-engine-echo`'s own entry is deliberately **not** in the base
document. It is the extension proof: the composition suite adds it to a
LIVE daemon by a profile edit alone, against an artifact the kit already
built — no rebuild, no definition change, no consumer change.

## What a vendor CLI's absence means

The kit resolves each CLI's absolute path from its flag or from `PATH` on
this host, and simply does not mount a provider whose CLI is not here.
That is why the switchable slot starts on the echo package: the tree boots
and answers everywhere, including CI, and a run against a real engine is
proven where a real engine exists. A provider that is mounted but cannot
authenticate answers `unavailable` and the run is recorded as
environment-gated — never faked.

A CLI's absolute path is machine state. It is written into the generated
document (which lives under a scratch root, never in the repo) and into
the `jinn:process` grant's `exec` allowlist beside it; it appears in no
tracked file.

## The authority the document writes

Each provider holds: its own `jinn:engine.<id>` contract (providing is
authority — the kernel checks the grant on `provide`), `jinn:clock` for
its one-shot poll wakes, `jinn:keystore` on the `engines/` prefix
attenuated to `ops: ["get"]` (read a secret value, never write, delete or
enumerate one), and — only if it spawns something — a `jinn:process` scope
naming the ONE executable and the environment allowlist `["HOME", "PATH"]`:
`HOME` because each CLI opens its own credential file under it, `PATH`
because a node-hosted CLI needs its interpreter. An allowlist, never
inherit-all; the harness never reads those credential files.

The probe holds exactly the one engine contract it is pointed at, the
seam's event topic (a listen grant), a clock, and a `jinn:fs` scope on its
own directory. The API holds one grant per engine it may route to — so an
operator API that may run the echo engine and not a metered one is a
profile edit, not a code path.

The real-composition proofs for this tree live in
`tests/composition/tests/engines.rs`.
