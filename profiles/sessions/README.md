# The `sessions` profile

The fourth core-port profile: the sessions seam mounted on top of the
engines seam, the operator API, the settings pair and the cron seam. Built
by `session-kit` (`cargo run -p session-kit -- kit <root> --port N`) and
booted in the OPERATOR layout (`--data <root>`), like the engines profile.

## What it mounts, and why each entry is here

| Entry | Package | Its job in this profile |
|---|---|---|
| `jinn-session-default` | `sessions/jinn-session-fs` | The SWITCHABLE slot, store id `default`. Durable. The swap proof moves this entry's package to the ephemeral store and leaves the id, the store id, the API and every engine alone. |
| `jinn-session-memory` | `sessions/jinn-session-memory` | The COEXISTENCE half, store id `memory`. Live at the same time on its own contract name, routed per session by the store in the path. |
| `jinn-session-scratch` | — | NOT mounted. The EXTENSION proof: a third store joins a live daemon by profile edit alone, against an artifact the kit already built and with no change to the definition. |
| the engines entries | `engines/…` | Mounted exactly as the engines profile mounts them (one home for those entries: `tools/engine-kit`). A store drives them; it never spawns one. |
| the api trio, settings pair, cron pair | | As in the operator-API profile. |

## The authority shape

A store holds its own `jinn:session.<store-id>` contract (providing IS
authority — the kernel checks the grant on `provide` exactly as on a
call), `jinn:clock` for the poll wakes that drive its turns, and ONE
`jinn:engine.<id>` grant per engine it may drive. Per-engine authority is
the kernel's, not a code path: a store that may run the echo engine and
not a paid one is that grant list.

A durable store also holds a `jinn:fs` scope naming the one directory its
journals live in (`sessions/` under the data root) and nothing outside it.
The ephemeral store holds **no `jinn:fs` grant at all** — `durable: false`
is an authority fact here, not only a declaration a provider makes about
itself.

The API's `jinn:session.<id>` grants and its `stores` setting are written
from the same source: the grant is what the kernel enforces, the setting
is that same fact told to the provider so an unroutable id is answered
without spending a kernel call.

## What survives a restart

The journals, and only the journals. A store's registry, its run ids and
its event ring are all per incarnation. A turn that was in flight when the
daemon stopped comes back `interrupted` with a reason — never `running`,
and never silently `done`. The law is the definition's
(`plugins/sessions/jinn-session/README.md`); the proof is
`tests/composition/tests/sessions.rs`.
