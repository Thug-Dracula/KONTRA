#!/usr/bin/env bash
# Build KONTRA and put it where hosts actually load it.
#
# REAPER loads the CLAP. Installing only the VST3 means the host keeps running
# the previous build and every test is of the wrong binary -- which is exactly
# what happened on 2026-10-30. Both are installed here, always.
#
#   tools/deploy.sh            build, install, and say what changed
#   tools/deploy.sh --reaper   also restart REAPER and place it on the KRDP monitor
set -euo pipefail

REPO=$(cd "$(dirname "$0")/.." && pwd)
# The gate's default, so `verify.sh` PASSED means this build is the one that was
# checked: a second target directory would install a binary no test has seen.
TARGET=${CARGO_TARGET_DIR:-/var/tmp/tgt-int}
CLAP="$HOME/.clap/KONTRA.clap"
VST3="$HOME/.vst3/KONTRA.vst3/Contents/x86_64-linux/KONTRA.so"

cd "$REPO"
echo "building kontra-plugin (release)"
CARGO_TARGET_DIR="$TARGET" cargo build --release -p kontra-plugin

BUILT="$TARGET/release/libkontra_plugin.so"
[ -f "$BUILT" ] || { echo "no $BUILT" >&2; exit 1; }
STAMP=$(date +%H:%M:%S)

install -Dm644 "$BUILT" "$CLAP"
echo "  $CLAP  $(stat -c%s "$CLAP") bytes  $STAMP"
if [ -d "$(dirname "$VST3")" ]; then
    install -Dm644 "$BUILT" "$VST3"
    echo "  $VST3  $(stat -c%s "$VST3") bytes  $STAMP"
fi

# Both files must be the same build, or the host and the next test disagree.
if [ -f "$VST3" ] && ! cmp -s "$CLAP" "$VST3"; then
    echo "CLAP and VST3 differ -- one host will run a stale build" >&2
    exit 1
fi

if [ "${1:-}" = "--reaper" ]; then
    # This used to kill REAPER, relaunch it and move its window on the user's
    # desktop (`pgrep -x reaper` + `wmctrl -e`). That is the user's session, not
    # ours to drive: an operator who asked for a plugin install has not asked
    # for their window manager to be handled. Say what to do instead.
    cat <<'EOF'
REAPER loads a CLAP at scan time, so it has to be restarted to pick this up.
Do it yourself, when it suits you:

  * quit REAPER and start it again (reaper -new if an instance may be lingering)
  * the plugin appears as "KONTRA" in both VST3 and CLAP
  * if it does not, check the log it writes under ~/.cache/kontra/
EOF
fi
