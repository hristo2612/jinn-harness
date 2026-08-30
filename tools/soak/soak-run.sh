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
#                           dropped by SOAK.md's planned-start step) — a
#                           report of what the file said, not an inference
#   boot-consistent         the readings are CONSISTENT with the daemon
#                           having died with the host: a boot time later
#                           than the previous start
#   keepalive-restart-consistent
#                           the readings are consistent with the opposite —
#                           previous start on THIS host boot, launchd's
#                           retained status unclean
#   first-supervised-start  the run directory was enumerable and held no
#                           record: that ABSENCE is the evidence
#   unknown                 anything else, including everything not yet
#                           imagined — see below
#
# --- Why two of those say `-consistent`, and none says `boot`.
#
# `boot` is a causal claim about the HOST. The wrapper's inputs are a
# sysctl reading and a file's mtime; from those it can derive that the
# readings line up with a reboot, never that a reboot happened. The
# distinction stopped being pedantic the moment a record replaced between
# the two looks — mtime preserved — produced `reason=boot` at rc 0 in
# 10 runs out of 10. No care at the read site fixes that: `stat`-after-read
# proves "I read a pid and an mtime together", never "this mtime belongs to
# that pid". The primitive cannot carry the claim, so the claim is retired
# and the DERIVATION is labelled as one.
#
# Which is only half of it. A label an auditor cannot see through is still
# an oracle. So every line the wrapper writes carries its INPUTS verbatim
# beside the inference — the boot time as read (raw and rendered), the
# record's status, its pid and mtime as read, launchd's status raw and
# decoded, the last-seen bound, and the names of whatever was unreadable.
# A wrong input is then visible as a wrong input on 2026-09-04, instead of
# hiding inside a confident word.
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
# measurement. Both derivations above rest on the same two readings — one
# says the host rebooted under the daemon, the other says it did not — so
# both are reachable only with proof from BOTH sides, and every other path
# — unreadable, missing, torn, ambiguous, unimagined — falls through to
# `unknown` by construction rather than by a guard someone remembered to
# write. That inversion is round 2's and it stays; round 3 adds what it
# could not supply, which is the evidence beside the answer.
#
# A claim is derived from proof, never from the absence of a contradiction.

# A reading is a run of digits, or it is not a reading.
proven_digits() {
    case "$1" in
        '' | *[!0-9]*) printf 'unknown' ;;
        *) printf '%s' "$1" ;;
    esac
}

# A wait status is an optionally-signed run of digits, or it is not one.
proven_status() {
    case "${1#-}" in
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
# What the two readings CANNOT establish, stated once so no reader assumes
# otherwise: that the mtime and the pid came from the same record. A record
# replaced between the looks with its mtime preserved reads as coherent here.
# That takes a writer inside `$SOAK/run/` forging records — an adversary who
# can edit `ops.log` directly, so the label buys nothing against them
# (SOAK.md §Known limits). Against the accidental cases this wrapper is for —
# races, missing files, unreadable sysctls, torn records — the pair holds.
if [ "$prev_start" = unknown ]; then
    prev_start_at=unknown
else
    prev_start_at=$(date -u -r "$prev_start" +%FT%TZ 2>/dev/null || printf unknown)
fi

# --- Input 4: how the previous instance ended.
#
# launchd's LastExitStatus is a wait status: a signal in the low seven
# bits, an exit code in the high byte (the table form of `launchctl list`
# shows a signal as a negative number; the detail form as positive).
#
# Round 3 decoded it HERE for the `prev_end=` field and worded the ops.log
# narrative THERE, as a literal — and on a real start path with
# `LastExitStatus = 15` the two met on one line at rc 0: "ended UNCLEAN …
# a daemon that ended on its own", beside `prev_end="killed by signal 15
# (SIGTERM)"`. One line, disagreeing with itself. Same defect class as the
# three above, arrived at from the printing side instead of the reading
# side: a statement made without the proof beside it drifts from the
# proof.
#
# So there is one decode, and everything said about the end is rendered
# from it — the field, the narrative phrase, the clean/unclean token. The
# phrase EMBEDS the field rather than paraphrasing it, so the two cannot
# disagree because there is no second wording to disagree with. Not a
# check that they agree: nothing left to check.
prev_end_raw=$(proven_status "$(launchctl list "$label" 2>/dev/null \
    | sed -n 's/.*"LastExitStatus" = \(-\{0,1\}[0-9][0-9]*\);.*/\1/p')")
signal_name() { kill -l "$1" 2>/dev/null || echo '?'; }
case "$prev_end_raw" in
    unknown) end_kind=unrecorded; end_detail=unknown ;;
    -*)      end_kind=signal;     end_detail=${prev_end_raw#-} ;;
    *)       if [ $((prev_end_raw & 127)) -ne 0 ]; then
                 end_kind=signal; end_detail=$((prev_end_raw & 127))
             else
                 end_kind=exit;   end_detail=$((prev_end_raw >> 8))
             fi ;;
esac
case "$end_kind" in
    signal)
        prev_end="killed by signal $end_detail (SIG$(signal_name "$end_detail"))"
        prev_end_clean=no ;;
    exit)
        prev_end="exit $end_detail"
        if [ "$end_detail" -eq 0 ]; then prev_end_clean=yes; else prev_end_clean=no; fi ;;
    *)
        prev_end="end status unknown (launchd retained none)"
        prev_end_clean=unknown ;;
esac
# The narrative the ops.log lines open with. It is the same decode worded
# for a reader, and it CONTAINS the field verbatim — the one construction
# under which the prose cannot say something the field denies. Note what
# it does not say: a signal has no sender here, because nothing retains
# one (SOAK.md §Known limits).
case "$prev_end_clean" in
    yes)     prev_end_phrase="ended CLEANLY, on its own: $prev_end" ;;
    no)      case "$end_kind" in
                 signal) prev_end_phrase="ended UNCLEAN: $prev_end" ;;
                 *)      prev_end_phrase="ended UNCLEAN, on its own: $prev_end" ;;
             esac ;;
    *)       prev_end_phrase="ended, HOW UNKNOWN: $prev_end" ;;
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
# What could not be read is an OBSERVATION, so it is computed before the
# decision and reported on every line — `unproven=none` on the proven lane
# included. A field that only appears when the answer is already `unknown`
# tells an auditor nothing about the answers that are not.
# A provably absent record is not an unread one: absence that the wrapper
# enumerated is evidence, and `prev_record=absent` is where it is reported.
unproven=
[ "$boot_sec" != unknown ] || unproven="$unproven host-boot-time"
[ "$prev_record" != unknown ] || unproven="$unproven run-directory"
if [ "$prev_record" != absent ] && [ "$prev_start" = unknown ]; then
    unproven="$unproven previous-start-record"
fi
unproven=${unproven# }
unproven=${unproven:-none}

if [ "$operator_reason" != unknown ]; then
    reason=$operator_reason
elif [ "$prev_record" = absent ]; then
    reason=first-supervised-start
elif [ "$boot_sec" != unknown ] && [ "$prev_start" != unknown ]; then
    if [ "$boot_sec" -gt "$prev_start" ]; then
        reason=boot-consistent
    else
        reason=keepalive-restart-consistent
    fi
else
    reason=unknown
fi

# Built ONCE, printed everywhere: the dry run, the death line and the start
# line are three views of one record, so they cannot drift apart.
evidence=$(printf 'host_boot_sec=%s host_boot=%s prev_record=%s prev_pid=%s prev_start_sec=%s prev_start=%s prev_end_raw=%s prev_end="%s" prev_end_clean=%s last_seen=%s unproven=%s' \
    "$boot_sec" "$boot_at" "$prev_record" "$prev_pid" "$prev_start" "$prev_start_at" \
    "$prev_end_raw" "$prev_end" "$prev_end_clean" "$last_seen" "$unproven")

# Dry run (the harness-pin gate, and an operator checking the decision):
# print it, touch nothing, start nothing. An unproven decision names what
# it could not read, so the reader is never left guessing which half was
# missing. (Against a root that does not exist this now reads `unknown`
# with `run-directory` unproven, where it used to manufacture the empty
# directory it then reasoned from.)
if [ "${SOAK_DRY_RUN:-}" = 1 ]; then
    printf 'reason=%s %s\n' "$reason" "$evidence"
    exit 0
fi
rm -f "$reason_file"

# From here, anything the daemon says on stdout/stderr is the soak log.
exec >>"$SOAK/logs/jinnd.log" 2>&1

now=$(date -u +%FT%TZ)
# Each line states the DERIVATION as a derivation ("readings are consistent
# with"), then hands over the readings. The audit counts the word; a reader
# who doubts it re-derives the answer from the same evidence, on the spot.
case "$reason" in
    boot-consistent)
        printf '%s previous jinnd %s %s; DERIVED boot-consistent: the readings are consistent with the daemon having died with the host (an inference from the evidence below, not an observation of a reboot). evidence: %s\n' \
            "$now" "$prev_pid" "$prev_end_phrase" "$evidence" >>"$SOAK/logs/ops.log" ;;
    keepalive-restart-consistent)
        printf '%s previous jinnd %s %s; DERIVED keepalive-restart-consistent: the readings are consistent with the previous start belonging to THIS host boot, so the host did not reboot under the daemon and launchd relaunched it (an inference, not an observation). evidence: %s\n' \
            "$now" "$prev_pid" "$prev_end_phrase" "$evidence" >>"$SOAK/logs/ops.log" ;;
    unknown)
        # Something ended and the wrapper cannot prove WHY this start
        # happened: it says so, names the input it could not read, and
        # derives nothing. How the previous instance ended is a separate
        # reading and is still reported, from the same one decode.
        printf '%s previous jinnd %s %s, PROVENANCE UNKNOWN (could not read: %s). evidence: %s\n' \
            "$now" "$prev_pid" "$prev_end_phrase" "$unproven" "$evidence" >>"$SOAK/logs/ops.log" ;;
esac

pin=$(cat "$SOAK/bin/jinnd.commit" 2>/dev/null || echo unknown)
printf '%s started (launchd; reason=%s): jinnd %s (pin %s) evidence: %s\n' \
    "$(date -u +%FT%TZ)" "$reason" "$$" "$pin" "$evidence" >>"$SOAK/logs/ops.log"
printf '%s\n' "$$" >"$SOAK/run/jinnd.pid"

exec "$SOAK/bin/jinnd" \
    --profile "$SOAK/data/profile.json" \
    --ledger "$SOAK/ledger.sqlite" \
    --artifacts "$SOAK/artifacts" \
    --data "$SOAK/data"
