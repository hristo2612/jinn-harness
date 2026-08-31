#!/bin/sh
# Record WHICH KERNEL was installed into the soak root, by derivation.
#
# usage: tools/soak/record-build.sh <built-daemon-dir> [<repo-root>]
#
#   <built-daemon-dir>  the composition harness's pinned build —
#                       `target/composition/pinned-jinnd`: the extracted
#                       archive, carrying the `.commit` marker the
#                       git-archive build wrote at its root and the daemon
#                       under its own `target/debug/`.
#   <repo-root>         where KERNEL-PIN.md is read from (default: the
#                       repo this script lives in).
#
# It installs the binary at `$SOAK/bin/jinnd` and writes
# `$SOAK/bin/jinnd.build`, which is the ONLY thing the wrapper will accept
# as an account of what it is running.
#
# --- Why this script exists at all.
#
# The pin used to reach the soak as `cp …/.commit "$SOAK/bin/jinnd.commit"`
# beside the binary, and the +7d audit's own artifact (`meta.json`) was
# maintained by hand. On 2026-08-31 a COO audit found the two disagreeing
# with each other and both disagreeing with KERNEL-PIN.md: a third pin bump
# had happened and nobody wrote it down (SOAK.md §What the record is).
#
# Nothing detected that, because nothing could: two files sitting in one
# directory make no claim about each other. So the record carries the
# DIGEST of the binary it describes, and the wrapper re-computes that digest
# at every start. A record left behind by an earlier install then fails to
# describe the binary that is running, which is a readable fact rather than
# an invisible one.
#
# --- Why every field is derived and none is an argument.
#
# A field a person can type is a field that can be typed wrong, and this
# packet exists because one was. So:
#
#   binary-sha256  computed here, from the bytes being installed
#   running-pin    read from the composition build's `.commit` marker,
#                  which the git-archive build derived from the commit it
#                  checked out
#   harness-pin    read from KERNEL-PIN.md's `commit:` line — a DIFFERENT
#                  reading (what the harness ships), kept in its own field
#                  so the distance between the two stays visible
#
# Any of those unreadable is a refusal. A record with a hole in it is what
# an auditor cannot tell from a whole one.
set -eu

SOAK=${SOAK:-$HOME/.local/state/jinn-harness-soak}
built=${1:-}
repo=${2:-$(cd "$(dirname "$0")/../.." && pwd)}

[ -n "$built" ] || { echo "usage: record-build.sh <built-daemon-dir> [<repo-root>]" >&2; exit 2; }

fail() { echo "record-build: $1" >&2; exit 1; }

# The daemon's place inside the build, resolved once and REPORTED. The
# composition harness extracts the archive and builds inside it, so the
# binary sits under the archive's own target dir; a bare directory holding
# the binary is accepted too. Which one answered is printed, because a
# script that silently accepts two shapes is a script whose reader does not
# know which one it got.
daemon=
for candidate in "$built/target/debug/jinnd" "$built/jinnd"; do
    [ -f "$candidate" ] || continue
    daemon=$candidate
    break
done
[ -n "$daemon" ] || fail "no daemon under $built (looked in target/debug/jinnd and jinnd)"
[ -f "$built/.commit" ] || fail "no .commit marker at $built — the pin is derived from it, never typed"
[ -f "$repo/KERNEL-PIN.md" ] || fail "no KERNEL-PIN.md under $repo — the harness pin is read from it"

running_pin=$(tr -d '[:space:]' <"$built/.commit")
case "$running_pin" in
    *[!0-9a-f]* | '') fail "the .commit marker is not a commit: '$running_pin'" ;;
esac

harness_pin=$(sed -n 's/^commit:[[:space:]]*\([0-9a-f][0-9a-f]*\)[[:space:]]*$/\1/p' "$repo/KERNEL-PIN.md" | head -1)
[ -n "$harness_pin" ] || fail "no 'commit:' line in $repo/KERNEL-PIN.md"

mkdir -p "$SOAK/bin"
install -m 0755 "$daemon" "$SOAK/bin/jinnd"

# The digest is taken from the INSTALLED bytes, not the source ones: what
# the wrapper will hash is what is described.
sha=$(shasum -a 256 "$SOAK/bin/jinnd" 2>/dev/null | awk '{print $1}')
case "$sha" in
    *[!0-9a-f]* | '') fail "could not digest $SOAK/bin/jinnd" ;;
esac

record=$SOAK/bin/jinnd.build
cat >"$record.tmp" <<RECORD
binary-sha256=$sha
running-pin=$running_pin
harness-pin=$harness_pin
recorded-utc=$(date -u +%FT%TZ)
RECORD
mv "$record.tmp" "$record"

# The retired hand-copied marker. Leaving it would leave a second, unbound
# account of the same fact lying beside the bound one (AGENTS.md standing
# order 5: one home per fact).
rm -f "$SOAK/bin/jinnd.commit"

echo "installed: $SOAK/bin/jinnd (from $daemon)"
echo "recorded:  $record"
sed 's/^/  /' "$record"
