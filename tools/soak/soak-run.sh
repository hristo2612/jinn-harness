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
# Absolute paths are the wrapper's canonical form. They are no longer a
# workaround: since pin 9e61e47 the daemon resolves a relative profile
# path itself and arms its watcher BEFORE writing any boot evidence
# (FINDINGS.md #18 closed); the start evidence an operator waits for is
# the daemon's readiness line in jinnd.log (SOAK.md §Start).
#
# Since the sixth bump (pin 57360cc, the operator API mounted) the
# profile sits INSIDE the data root — `$SOAK/data/profile.json` — so the
# api consumers can read the document of record through their scoped
# `jinn:fs` (FINDINGS.md #25); `--artifacts` and `--data` are therefore
# passed explicitly (the daemon's defaults are cwd-relative, and the
# profile's directory is no longer the root). The data root itself, the
# ledger and the retention store did not move.
set -eu

SOAK=${SOAK:-$HOME/.local/state/jinn-harness-soak}
mkdir -p "$SOAK/logs" "$SOAK/run"
label=run.jinn.harness-soak

# --- What happened to the previous instance, and why this start happened.
#
# The wrapper exec's the daemon, so nobody is standing beside it when it
# dies: the record of a death is written HERE, at the next start, from
# what launchd retained (the daemon's own wait status) and what the daemon
# last said (its final log line) — and it is written BEFORE the start
# line, so the audit reads the death, then the recovery.
#
# The reason vocabulary (what the +7d audit counts):
#   adopt|planned-start     the operator asked for it (a reason file,
#                           dropped by SOAK.md's planned-start step)
#   boot                    the host booted AFTER the previous start: the
#                           daemon died with the host (checked against
#                           kern.boottime, never against a scratch file —
#                           2026-08-29 a missing stamp wrote `boot` for a
#                           SIGTERM on a host that had not rebooted)
#   keepalive-restart       same host boot, nobody asked: launchd replaced
#                           a daemon that ended uncleanly
#   first-supervised-start  no previous pid on record: provenance unknown,
#                           never asserted as a boot
boot_sec=$(sysctl -n kern.boottime 2>/dev/null | sed -n 's/^{ sec = \([0-9]*\),.*/\1/p')
if [ -n "$boot_sec" ]; then
    boot_at=$(date -u -r "$boot_sec" +%FT%TZ 2>/dev/null || echo unknown)
else
    # No reading: sysctl absent, or `{ sec = N, usec = M }` reshaped under a
    # future macOS. Say `unknown` rather than fall back to the zero epoch —
    # `date -r 0` prints 1970 quite happily, and the death line would then
    # assert a boot time nobody measured. A zero also keeps the comparison
    # below conservative: with no evidence of a boot, none is claimed.
    boot_sec=0
    boot_at=unknown
fi
prev_pid=$(cat "$SOAK/run/jinnd.pid" 2>/dev/null || true)
prev_start=$(stat -f %m "$SOAK/run/jinnd.pid" 2>/dev/null || echo 0)
# launchd's LastExitStatus is a wait status: a signal in the low seven
# bits, an exit code in the high byte (the table form of `launchctl list`
# shows a signal as a negative number; the detail form as positive).
raw=$(launchctl list "$label" 2>/dev/null | sed -n 's/.*"LastExitStatus" = \(-*[0-9]*\);.*/\1/p')
signal_name() { kill -l "$1" 2>/dev/null || echo '?'; }
case "${raw:-}" in
    '') prev_end="end status unknown (launchd retained none)" ;;
    -*) prev_end="killed by signal ${raw#-} (SIG$(signal_name "${raw#-}"))" ;;
    *)  if [ $((raw & 127)) -ne 0 ]; then
            prev_end="killed by signal $((raw & 127)) (SIG$(signal_name $((raw & 127))))"
        else
            prev_end="exit $((raw >> 8))"
        fi ;;
esac
esc=$(printf '\033')
last_seen=$(LC_ALL=C sed "s/${esc}\[[0-9;]*m//g" "$SOAK/logs/jinnd.log" 2>/dev/null \
    | grep -o '^[0-9][0-9-]*T[0-9:.]*Z' | tail -1)
last_seen=${last_seen:-unknown}

reason_file="$SOAK/run/launchd.reason"
if [ -f "$reason_file" ]; then
    reason=$(cat "$reason_file")
elif [ -z "$prev_pid" ]; then
    reason=first-supervised-start
elif [ "$boot_sec" -gt "$prev_start" ]; then
    reason=boot
else
    reason=keepalive-restart
fi

# Dry run (the harness-pin gate, and an operator checking the decision):
# print it, touch nothing, start nothing.
if [ "${SOAK_DRY_RUN:-}" = 1 ]; then
    printf 'reason=%s prev_pid=%s prev_end="%s" last_seen=%s host_boot=%s\n' \
        "$reason" "${prev_pid:-none}" "$prev_end" "$last_seen" "$boot_at"
    exit 0
fi
rm -f "$reason_file"

# From here, anything the daemon says on stdout/stderr is the soak log.
exec >>"$SOAK/logs/jinnd.log" 2>&1

now=$(date -u +%FT%TZ)
case "$reason" in
    boot)
        printf '%s previous jinnd %s died with the host (unplanned): last log line %s; host booted %s; launchd last status: %s\n' \
            "$now" "$prev_pid" "$last_seen" "$boot_at" "$prev_end" >>"$SOAK/logs/ops.log" ;;
    keepalive-restart)
        printf '%s previous jinnd %s ended UNCLEAN (unplanned; host up since %s): %s; last log line %s; KeepAlive relaunching\n' \
            "$now" "$prev_pid" "$boot_at" "$prev_end" "$last_seen" >>"$SOAK/logs/ops.log" ;;
esac

pin=$(cat "$SOAK/bin/jinnd.commit" 2>/dev/null || echo unknown)
printf '%s started (launchd; reason=%s): jinnd %s (pin %s)\n' \
    "$(date -u +%FT%TZ)" "$reason" "$$" "$pin" >>"$SOAK/logs/ops.log"
printf '%s\n' "$$" >"$SOAK/run/jinnd.pid"

exec "$SOAK/bin/jinnd" \
    --profile "$SOAK/data/profile.json" \
    --ledger "$SOAK/ledger.sqlite" \
    --artifacts "$SOAK/artifacts" \
    --data "$SOAK/data"
