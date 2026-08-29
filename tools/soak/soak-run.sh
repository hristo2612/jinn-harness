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
label=run.jinn.harness-soak
# A dry run prints the decision and touches nothing — including this. On a
# root that does not exist yet, creating `run/` was the only thing that made
# the root exist at all, and it also handed the decision below an enumerable
# empty directory it had manufactured itself.
if [ "${SOAK_DRY_RUN:-}" != 1 ]; then
    mkdir -p "$SOAK/logs" "$SOAK/run"
fi

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
#                           daemon died with the host
#   keepalive-restart       same host boot, nobody asked: launchd replaced
#                           a daemon that ended uncleanly
#   first-supervised-start  the run directory was enumerable and held no
#                           record: that ABSENCE is the evidence
#   unknown                 anything else, including everything not yet
#                           imagined — see below
#
# --- Why `unknown` exists, and why it is the default.
#
# Three times running, a missing or unreadable input degraded into a value
# that made a positive claim true: a vanished stamp file wrote `boot`; an
# unreadable `kern.boottime` fell back to a zero epoch and would have
# written `host up since 1970-01-01T00:00:00Z`; a torn record — the pid
# read, then the file gone before the `stat` — left `prev_start=0`, which
# makes `boottime > prev_start` trivially true, and answered `boot` at rc 0.
#
# So the default inverts. Each input below is read into either a value the
# wrapper can prove it read, or the literal `unknown`. No `0`, no empty
# string, no zero epoch: nothing a later comparison would mistake for a
# measurement. `boot` (and equally `keepalive-restart`, which claims the
# host did NOT reboot) is then reachable only with proof from BOTH sides,
# and every other path — unreadable, missing, torn, ambiguous, unimagined —
# falls through to `unknown` by construction rather than by a guard someone
# remembered to write.
#
# A claim is derived from proof, never from the absence of a contradiction.

# A reading is a run of digits, or it is not a reading.
proven_digits() {
    case "$1" in
        '' | *[!0-9]*) printf 'unknown' ;;
        *) printf '%s' "$1" ;;
    esac
}

# --- Input 1: the host's boot time.
boot_sec=$(proven_digits "$(sysctl -n kern.boottime 2>/dev/null \
    | sed -n 's/^{[[:space:]]*sec[[:space:]]*=[[:space:]]*\([0-9][0-9]*\)[^0-9].*/\1/p')")
if [ "$boot_sec" = unknown ]; then
    # sysctl absent, or `{ sec = N, usec = M }` reshaped under a future
    # macOS. Nothing was measured, so nothing is said.
    boot_at=unknown
else
    boot_at=$(date -u -r "$boot_sec" +%FT%TZ 2>/dev/null || printf unknown)
fi

# --- Inputs 2 and 3: the previous pid and the previous start's mtime.
#
# They are one record, so they are proven as one. The record is looked at
# twice with the read between: a record that answers both looks with the
# same mtime is one record the wrapper actually read. Any tear — gone by
# the second look, or replaced between them — leaves BOTH facts unknown,
# because a pid from one record and an mtime from another is not a
# previous start.
pid_file="$SOAK/run/jinnd.pid"
prev_pid=unknown
prev_start=unknown
prev_record=unknown
if run_entries=$(ls -a "$SOAK/run" 2>/dev/null); then
    # The directory was enumerable, so what is not in it is provably absent.
    case "
$run_entries
" in
        *"
jinnd.pid
"*) prev_record=present ;;
        *) prev_record=absent ;;
    esac
fi
if [ "$prev_record" = present ]; then
    before=$(proven_digits "$(stat -f %m "$pid_file" 2>/dev/null || true)")
    pid=$(proven_digits "$(cat "$pid_file" 2>/dev/null | tr -d '[:space:]')")
    after=$(proven_digits "$(stat -f %m "$pid_file" 2>/dev/null || true)")
    if [ "$before" != unknown ] && [ "$before" = "$after" ] && [ "$pid" != unknown ]; then
        prev_start=$before
        prev_pid=$pid
    fi
fi

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

# --- The decision. Every branch that claims something names its proof.
reason_file="$SOAK/run/launchd.reason"
operator_reason=unknown
if [ -f "$reason_file" ]; then
    operator_reason=$(cat "$reason_file" 2>/dev/null || printf unknown)
    [ -n "$operator_reason" ] || operator_reason=unknown
fi
unproven=
if [ "$operator_reason" != unknown ]; then
    reason=$operator_reason
elif [ "$prev_record" = absent ]; then
    reason=first-supervised-start
elif [ "$boot_sec" != unknown ] && [ "$prev_start" != unknown ]; then
    if [ "$boot_sec" -gt "$prev_start" ]; then
        reason=boot
    else
        reason=keepalive-restart
    fi
else
    reason=unknown
    [ "$boot_sec" != unknown ] || unproven="$unproven host-boot-time"
    [ "$prev_record" != unknown ] || unproven="$unproven run-directory"
    [ "$prev_start" != unknown ] || unproven="$unproven previous-start-record"
    unproven=${unproven# }
fi

# Dry run (the harness-pin gate, and an operator checking the decision):
# print it, touch nothing, start nothing. An unproven decision names what
# it could not read, so the reader is never left guessing which half was
# missing. (Against a root that does not exist this now reads `unknown`
# with `run-directory` unproven, where it used to manufacture the empty
# directory it then reasoned from.)
if [ "${SOAK_DRY_RUN:-}" = 1 ]; then
    printf 'reason=%s prev_pid=%s prev_end="%s" last_seen=%s host_boot=%s' \
        "$reason" "$prev_pid" "$prev_end" "$last_seen" "$boot_at"
    [ -z "$unproven" ] || printf ' unproven=%s' "$unproven"
    printf '\n'
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
    unknown)
        # Something ended and the wrapper cannot prove what: it says so,
        # names the input it could not read, and claims neither a boot nor
        # a same-boot restart. This line is the audit's cue to go looking.
        printf '%s previous jinnd %s ended, PROVENANCE UNKNOWN (could not read: %s): %s; last log line %s; host boot %s\n' \
            "$now" "$prev_pid" "$unproven" "$prev_end" "$last_seen" "$boot_at" >>"$SOAK/logs/ops.log" ;;
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
