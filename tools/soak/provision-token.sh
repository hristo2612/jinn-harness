#!/bin/sh
# The launcher's half of the door (harness packet 2.8): provision the
# `jinn:auth` credential of record for the soak daemon IF ABSENT.
#
# usage: tools/soak/provision-token.sh            (uses $SOAK, or its default)
#
# The pinned kernel's `jinn:auth` (M2-K21) reads ONE launcher-owned token
# file beside the data root — `<data>.operator-token`, a sibling of
# `data/`, never inside a guest's `jinn:fs` reach — on EVERY call. The
# daemon never writes it: whoever boots the daemon owns it. This script is
# that owner for the soak, and both start lanes (SOAK.md §Start) call it
# before the daemon, so one file is the one home of the provisioning rule.
#
# What it does, exactly:
#   absent   → 32 random bytes from /dev/urandom, hex-encoded (64 ASCII
#              characters), written under umask 077 to a staging file and
#              renamed into place — mode 0600 from the first byte, never a
#              window where a wider mode holds a credential
#   present  → left EXACTLY as it is (a restart keeps its credential; a
#              rotation is the operator's deliberate overwrite, not this
#              script's business), but its mode is CHECKED: the kernel
#              refuses a group- or world-accessible file, so a wrong mode
#              is reported here, at the start, rather than discovered as a
#              wall of refusals in the ledger
#
# What it never does: print the value, log the value, or accept one as an
# argument. The operator reads it with `cat "$SOAK/data.operator-token"`
# and presents it as `Authorization: Bearer <value>` (SOAK.md §Credential).
set -eu

SOAK=${SOAK:-$HOME/.local/state/jinn-harness-soak}
token="$SOAK/data.operator-token"

fail() { echo "provision-token: $1" >&2; exit 1; }

# The mode as the kernel will judge it: any of the group/other bits set
# and the file is not a credential.
mode_is_private() {
    perms=$(stat -f %Lp "$1" 2>/dev/null || stat -c %a "$1" 2>/dev/null) || return 1
    case "$perms" in
        *[!0-7]* | '') return 1 ;;
    esac
    [ $((0$perms & 077)) -eq 0 ]
}

if [ -e "$token" ]; then
    [ -f "$token" ] || fail "$token exists and is not a regular file"
    mode_is_private "$token" \
        || fail "$token is group- or world-accessible; the kernel will refuse it (chmod 600)"
    echo "operator-token present: $token (kept)"
    exit 0
fi

mkdir -p "$(dirname "$token")"
umask 077
staging="$token.provision-tmp"
rm -f "$staging"
# 32 bytes, as 64 hex characters: within the kernel's bounds (16..4096
# after trimming) and free of anything a shell or a header would mangle.
value=$(od -An -tx1 -N32 /dev/urandom | tr -d ' \n')
[ "${#value}" -eq 64 ] || { rm -f "$staging"; fail "could not draw 32 random bytes"; }
printf '%s\n' "$value" >"$staging"
unset value
chmod 600 "$staging"
mv "$staging" "$token"
mode_is_private "$token" || fail "$token landed with a wrong mode"
echo "operator-token provisioned: $token (mode 0600; read it with: cat $token)"
