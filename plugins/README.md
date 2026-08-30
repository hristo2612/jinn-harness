# plugins/

First-party plugin crates, each building to a wasm32 component against the
vendored contract surface in `kernel-pin/`. Organized by capability seam per
the seam-triple naming in `AGENTS.md` (service definition · providers ·
consumers); each seam's group directory carries a role-table README.

| Seam | What it is |
|---|---|
| `cron/` | Scheduled work (phase 1.3): `jinn-cron` · `cron-scheduler` · `health-snapshot`. |
| `api/` | The operator API (phase 2.1): `jinn-api` · `jinn-api-http` (+ its codec `jinn-api-http-wire`) · `jinn-status`, `jinn-profile-edit`. |
| `settings/` | Per-plugin settings (phase 2.2): `jinn-settings` · `jinn-settings-profile`, `jinn-settings-store` · consumed by `cron-scheduler` and exposed by `jinn-api-http`. |
| `sessions/` | Durable conversations (phase 2.4): `jinn-session` · `jinn-session-fs`, `jinn-session-memory` · `jinn-api-http`. A store drives the ENGINES definition; neither seam knows the other's provider. |
| `engines/` | Coding agents (phase 2.3): `jinn-engine` · `jinn-engine-claude`, `jinn-engine-codex`, `jinn-engine-echo` (+ the `-wire` stream codecs) · `jinn-engine-probe` and `jinn-api-http`. One contract per engine id, so providers coexist. |
