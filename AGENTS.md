# Agent Instructions — jinn-harness

You are working on the Jinn **distribution**: first-party plugins, profiles, and
product taste on top of the pinned `jinnd` kernel. Read `README.md` and
`KERNEL-PIN.md` before anything else.

## Standing orders

1. **Kernel changes are NEVER made here.** If your work needs anything from the
   kernel — a contract change, a new host capability, a bug fix — you do not
   patch it, vendor it, or work around it with a side door. You file a finding
   for a `jinnd` packet card and report it to the COO. The kernel repo has its
   own law regime; this repo builds only against the pinned contract surface in
   `kernel-pin/`.

2. **The cutover rule (verbatim):** the old gateway keeps ALL production until
   parity. No plugin here reads or writes production data before the parity
   gate passes for its instance.

3. **REAL-COMPOSITION RULE.** Every plugin is proven by booting through the real
   loader from a profile. Hand-mounted tests never count as integration. A
   plugin without a passing real-composition boot is not done, whatever its
   unit coverage says.

4. **Seam-triple naming.** Every capability seam is three roles, named so the
   tree stays navigable at hundreds of packages:
   - **Service definition** — the abstract contract package; owns the settings
     namespace (e.g. `jinn-shell`).
   - **Providers** — implementations, named definition-first
     (e.g. `jinn-shell-local`, `jinn-shell-sandbox`).
   - **Consumers** — plugins that inject the service, named by their own
     capability (e.g. `jinn-tool-bash`).
   Group directories carry a role-table README.

5. **One home per fact.** Documentation tiers, each fact in exactly one:
   - *Orders* — this file. What agents must do.
   - *Map* — `README.md`. What lives where.
   - *Reference* — `docs/`. Contracts, formats, procedures.
   - *Agent Notes* — `docs/notes/`. Rationale for non-obvious decisions;
     non-trivial changes ship one in the same PR.
   - *Postmortems* — `docs/postmortems/`. Defects harden into rules here, then
     into this file.
   - *Per-package contract READMEs* — each plugin package documents its own
     contract surface.
   Never restate a fact outside its home; link to it.

## Working rules

- **TDD.** A failing test exists before your implementation, always.
- **Verify before claiming done.** Full Rust gates are `cargo fmt --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`
  and `cargo build --workspace --locked`. Workspace tests already include the
  pin crate; run each required suite once per evidence set. Record full commit
  SHA, commands and actual tails; run affected web gates as defined by the web
  package and CI. Current PR and post-merge Linux CI remain required. A skipped
  composition CI job does not replace local real-loader proof.
- Branch + PR per work packet (the initial scaffold commit is the one
  exception). Conventional commits (`feat:`, `fix:`, `test:`, `docs:`,
  `chore:`). No co-author trailers.
- **Kernel pin discipline.** Plugins build only against `kernel-pin/`. Bumping
  the pin follows the procedure in `KERNEL-PIN.md`: one commit, hashes +
  commit + vendored surface updated together, never implicit.

## Repo hygiene

This repo becomes public at the M4 rename. **Zero personal data**: no real
names beyond the repo owner's public identity, no keys, no emails, no
machine-specific paths (write paths relative to the repo root or via
environment variables), no session ids, no references to private
infrastructure.

## Scoped verification evidence

For new work after this amendment, full gates remain the default. An independent
reviewer may reuse a previously successful independent gate result for unchanged
inputs: record both complete SHAs, the full intervening diff, original command and
output, platform/toolchain, dependency locks, build scripts/config and features. For the
harness also record the exact kernel pin and loader/profile/plugin identity.
Prove that the changed files cannot affect that gate's behavior, including generated
assets and indirect inputs. Label the result reused, not newly executed.

A docs/comment-only or web-only delta can qualify; file extension alone is not
proof. Rerun all affected behavior and acceptance checks on the new head, including
live UI checks and real-loader composition for affected integration behavior.
Changes to Rust/runtime/contract/pin, profiles, loaders, dependencies or build
machinery require the relevant full Rust/composition gates. Missing, ambiguous or
non-independent prior evidence requires full validation. Source-of-truth invariants,
verifier ownership and every required PR/post-merge CI gate remain binding.

When web bytes change, rebuild the actual UI artifact from the new head and prove
its relevant real-loader/live path against that artifact. Unchanged Rust sources
alone do not preserve a test result that embeds web assets. Name the actual changed
paths and compare pin, lockfiles, build scripts/config, features and toolchain;
cite the independent prior evidence SHA. Missing provenance means rerun. Required
CI remains intact; reuse cannot turn a skipped job into proof.
