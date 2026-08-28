#!/bin/sh
# The soak daemon's supervised entry point (SOAK.md §Supervisor).
#
# launchd runs THIS, not `jinnd` directly, for two reasons:
#
#   1. The plist cannot expand `$HOME`, but this script can: the runtime
#      root is derived here, so nothing machine-specific is ever tracked.
#   2. A supervisor start has to be as visible in `ops.log` as an operator
#      start. This script appends one `started (launchd; reason=...)` line
#      before exec'ing the daemon, so the +7d audit can count host reboots
#      and crash restarts apart from planned stop/start cycles.
#
# It `exec`s the daemon, so the daemon inherits this pid: launchd supervises
# jinnd itself, `$SOAK/run/jinnd.pid` stays the daemon's pid as SOAK.md's
# health check and §Stop expect, and the exit status launchd sees is the
# daemon's own (which is what `KeepAlive.SuccessfulExit=false` keys on).
#
# Absolute `--profile`/`--ledger` paths are the wrapper's canonical form.
# They are no longer a workaround: since pin 9e61e47 the daemon resolves a
# relative profile path itself and arms its watcher BEFORE writing any
# boot evidence (FINDINGS.md #18 closed); the start evidence an operator
# waits for is the daemon's readiness line in jinnd.log (SOAK.md §Start).
set -eu

SOAK=${SOAK:-$HOME/.local/state/jinn-harness-soak}
mkdir -p "$SOAK/logs" "$SOAK/run"

# From here, anything the daemon says on stdout/stderr is the soak log.
exec >>"$SOAK/logs/jinnd.log" 2>&1

# Why this start happened, in the audit's vocabulary:
#   adopt|planned-start  the operator asked for it (a reason file, dropped
#                        by SOAK.md's planned-start step and consumed here)
#   boot                 first supervised start since this host booted
#   keepalive-restart    same host boot, nobody asked — launchd replaced a
#                        daemon that exited uncleanly
reason_file="$SOAK/run/launchd.reason"
hostboot_file="$SOAK/run/launchd.hostboot"
hostboot=$(sysctl -n kern.boottime 2>/dev/null || echo unknown)
if [ -f "$reason_file" ]; then
    reason=$(cat "$reason_file")
    rm -f "$reason_file"
elif [ ! -f "$hostboot_file" ] || [ "$hostboot" != "$(cat "$hostboot_file")" ]; then
    reason=boot
else
    reason=keepalive-restart
fi
printf '%s\n' "$hostboot" >"$hostboot_file"

pin=$(cat "$SOAK/bin/jinnd.commit" 2>/dev/null || echo unknown)
printf '%s started (launchd; reason=%s): jinnd %s (pin %s)\n' \
    "$(date -u +%FT%TZ)" "$reason" "$$" "$pin" >>"$SOAK/logs/ops.log"
printf '%s\n' "$$" >"$SOAK/run/jinnd.pid"

exec "$SOAK/bin/jinnd" \
    --profile "$SOAK/profile.json" \
    --ledger "$SOAK/ledger.sqlite"
