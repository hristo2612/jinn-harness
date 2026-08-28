# profiles/

Named plugin trees. A product is a profile, not a codebase: the same kernel
boots different trees for different instances. Real-composition tests boot
these profiles through the actual loader — that boot is the integration
evidence for every plugin (AGENTS.md standing order 3).

| Profile | What it boots |
|---|---|
| `cron/` | The cron seam alone — the soak's duty tree. |
| `operator-api/` | The api trio beside the cron seam, in the operator layout (`--data <root>`). |
