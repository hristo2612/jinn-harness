# The engines seam

Coding agents as a capability on the kernel — the third core-port seam
under the malleability contract (phase 2.3), and the first one whose
providers reach outside the daemon at all. Roles per the seam-triple
naming law (AGENTS.md):

| Role | Package | What it is |
|---|---|---|
| Service definition | `jinn-engine` | The `jinn:engine.<id>` contract: the run request (engine, model, effort, prompt, cwd, tool policy, budget, and secrets as keystore REFERENCES only), the typed run events on `jinn:engine/event`, the `describe`/`run`/`run-get`/`cancel` operations, and `Runs` — the run registry, event sequencing, answer assembly and budget accounting every provider shares. Owns the `engines` settings namespace. Pure types + logic. |
| Provider | `jinn-engine-claude` | Spawns the `claude` CLI in print/stream-json mode through `jinn:process` under an executable-allowlist grant and an env allowlist. Its stream codec is `jinn-engine-claude-wire`. |
| Provider | `jinn-engine-codex` | The same shape for `codex exec --json`. Its stream codec is `jinn-engine-codex-wire`. |
| Provider | `jinn-engine-echo` | Two shapes, one package. Without `command` it answers from the prompt itself: the EXTENSION proof (a third engine added by a profile edit alone) and the seam's CI-runnable evidence, since it needs no vendor authentication. With `command` it spawns that absolute path through `jinn:process` — the PROCESS-LIFECYCLE witness, so a cancel that kills a pid, a suspend that kills one in flight, an exec-allowlist refusal and an env-policy bound are all checked where no vendor CLI exists. |
| Consumer | `jinn-engine-probe` | Runs one short prompt through the configured engine on a schedule and records the result through `jinn:fs` — the seam's real duty. |
| Consumer | `jinn-api-http` (`plugins/api/`) | Exposes the engines over the operator API: list, describe, run, read a run, cancel. |

## Why the contract name carries the engine id

The kernel holds ONE provider slot per contract name — a second provider
of an occupied slot is refused (`DuplicateProvision`). N engines
coexisting therefore means N contract names, so the seam's name is
instanced: `jinn:engine.<engine-id>`, the id read from the provider
entry's own `config.data.engine` and written nowhere else. That single
decision is what makes the three malleability proofs profile edits:

- **Switch** — change an entry's `package` and `hash`, keep its id and its
  `engine`: a different implementation serves the same contract name, and
  every consumer is untouched.
- **Coexistence** — a second entry with a different `engine` provides a
  different contract name; a consumer routes by engine id, which *is* the
  contract name.
- **Extension** — a third provider is an entry plus a grant. No definition
  change, no consumer change, no new contract.

The kernel has no shape for instance multiplicity of one contract;
`FINDINGS.md` #29 records the friction and what would retire the encoding.

## Where the machine lives

A provider's absolute CLI path, its models, and its poll cadence are
**profile state**, carried in the entry's `config.data` and never in
source or in a committed fixture — the repo is public and holds no
machine paths (AGENTS.md §Repo hygiene). A run's secret material is
never in the profile either: a request carries `{"$secret": "<key>"}`
references (the settings seam's typed shape, one home per fact) and the
provider resolves them through its granted `jinn:keystore` prefix at
spawn time. The CLIs' own credential files are opened by the CLIs
themselves, under the host's uid; nothing here reads, copies, or names
them.

The contract surface is documented in `jinn-engine/README.md` — one home
per fact. Guest crates here are NOT workspace members (see the workspace
manifest's note); `engine-kit` builds them into the engines profile
(`profiles/engines/README.md`). Real-composition proof lives in
`tests/composition/tests/engines.rs`.
