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
- **Verify before claiming done** and paste real output:
  `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`.
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
