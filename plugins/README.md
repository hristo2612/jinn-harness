# plugins/

First-party plugin crates, each building to a wasm32 component against the
vendored contract surface in `kernel-pin/`. Organized by capability seam per
the seam-triple naming in `AGENTS.md` (service definition · providers ·
consumers); each seam's group directory carries a role-table README.

Empty by design at phase 1.1 — the first capability (cron) lands at phase 1.3.
