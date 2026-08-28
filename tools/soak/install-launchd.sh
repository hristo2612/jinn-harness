#!/bin/sh
# Install (or refresh) the soak's user LaunchAgent — files only.
#
# Deliberately does NOT bootstrap the agent: loading it starts a daemon, and
# starting a daemon while one is on duty is a double start. SOAK.md
# §Supervisor owns the adoption sequence (clean stop, then bootstrap) and
# every step is an ops.log line.
#
# usage: tools/soak/install-launchd.sh
set -eu

SOAK=${SOAK:-$HOME/.local/state/jinn-harness-soak}
here=$(cd "$(dirname "$0")" && pwd)
label=run.jinn.harness-soak
plist="$HOME/Library/LaunchAgents/$label.plist"

[ -x "$SOAK/bin/jinnd" ] || {
    echo "no daemon at \$SOAK/bin/jinnd — run SOAK.md §Setup first" >&2
    exit 1
}

mkdir -p "$SOAK/bin" "$SOAK/logs" "$SOAK/run" "$HOME/Library/LaunchAgents"
install -m 0755 "$here/soak-run.sh" "$SOAK/bin/soak-run.sh"
sed "s#__SOAK__#$SOAK#g" "$here/$label.plist.template" >"$plist.tmp"
plutil -lint "$plist.tmp" >/dev/null
mv "$plist.tmp" "$plist"

echo "installed: $SOAK/bin/soak-run.sh"
echo "installed: $plist"
echo
echo "NOT loaded. To adopt the soak under it, follow SOAK.md §Supervisor:"
echo "  stop the running daemon cleanly (SOAK.md §Stop), then"
echo "  printf adopt > \"\$SOAK/run/launchd.reason\""
echo "  launchctl bootstrap gui/\$(id -u) \"$plist\""
