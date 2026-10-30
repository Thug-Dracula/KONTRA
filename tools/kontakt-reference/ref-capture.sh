#!/usr/bin/env bash
# Reference capture of a Kontakt instrument page, headless, on the private Xvfb :77.
#
# WHY: KONTRA draws each instrument page from the NKI's UI model. Nothing has ever compared
# that render with the page Kontakt itself draws, which is how a missing graphic shipped.
# This script is the sanctioned capture path: it only ever touches display :77 (no desktop,
# no user window, no remote-debugging port) and a Wine prefix of its own.
#
# WHAT IT DOES TODAY (measured 2026-10-30, Kontakt 8.14.1 Portable, wine 11.16 staging):
#   prefix    builds a private prefix that can run the portable Kontakt (see the d3d note)
#   register  registers a library folder, so Kontakt stops refusing its instruments with
#             "This instrument belongs to a library that is not installed currently!"
#   capture   launches Kontakt with an NKI, waits, screenshots the whole :77 desktop
#   probe     reports whether Kontakt's QML content is taking pointer input
#
# KNOWN BLOCKER (documented here so nobody repeats the search): in this environment the
# Kontakt window paints its browser but its QML client area takes NO pointer input — hovering
# and clicking anywhere below the native File/Help menu bar change zero pixels, with or
# without `explorer /desktop`, with wined3d AND with DXVK, and with a focused window.
# Only the Win32 menu (File, Help) responds. `explorer /desktop` + `KONTRA_REFERENCE_DIR` from
# kontakt.sh is not enough on this wine build. Reloading a page therefore needs a real window
# manager on :77 (kwin_x11 / openbox / fluxbox are not installed here) or a user-side capture.
# The Lua API cannot help: it is engine-only (load_instrument, get_*/set_* instrument data),
# and `load_instrument` does succeed — it is *displaying* the page that needs the mouse.
#
# COORDINATES (1600x1000 desktop; Kontakt's window sits at 0,0 and sizes itself):
#   top bar (logo, transport, rack icon, memory/disk/cpu, "!"): y 55..85
#     - logo drop-down chevron x~245, play x~260, rack icon x~285
#     - memory/disk/cpu readout x 890..930 (1000-wide window) .. 1190..1230 (1347-wide)
#   browser content (search box y~121, tabs y~168, banner y 258..480): everything below y~85
#   File menu opens at 18,40 (Options... at 18,60, Exit at 18,80) — the only live controls found
#   Library Manager (Helper/Library Manager/Library Manager.exe), window at 600,240:
#     "Libraries folder" checkbox 692,456 · Start 827,613 · SAVE 960,656 · report dialogs close
#     with Alt+F4 (they have no OK button on screen)
#
# WHY A PRIVATE PREFIX: ~/.wine and $WINEPREFIX (Armory/WINE) carry DXVK's d3d11.dll, and
# wine then refuses the builtin ("found in WINEDLLPATH but not a builtin, ignoring") when the
# overrides below force builtin. The prefix this script builds is disposable instead.
#
# LIBRARY REGISTRATION (this part works and is reusable): the portable Kontakt keeps its
# library list in <portable>/UserData/Settings.cfg, one section per library, with
# ContentDir relative to <portable>/Libraries — so the library has to be reachable there
# (a symlink is enough). Library Manager writes those sections (and mirrors them into
# HKLM\Software\Native Instruments\<name>). Writing the registry keys by hand is NOT enough.
#
# Usage:
#   ref-capture.sh prefix                     # once per machine
#   ref-capture.sh register "<library dir>"   # once per library (needs the Helper tool)
#   ref-capture.sh capture IN.nki OUTDIR [WxH+X+Y]   # loads the NKI, screenshots
#   ref-capture.sh probe                      # pointer-input check (prints Live/DEAD)
#   ref-capture.sh stop
set -euo pipefail

DISPLAY_NUM=":77"
RES="${REF_RES:-1600x1000}"
PREFIX="${REF_PREFIX:-$HOME/.cache/refcmp/prefix}"
PORTABLE="${REF_PORTABLE:-/home/dracula/Armory/VSTs/Kontakt 8.14.1 Portable}"
EXE="$PORTABLE/Kontakt 8/x64/Kontakt 8.exe"
export DISPLAY="$DISPLAY_NUM"
export WINEPREFIX="$PREFIX"
export WINEDLLOVERRIDES="d3d11,d3d10core,dxgi,d3d9=b"   # wined3d: DXVK is not reliable on Xvfb
click() { xdotool mousemove "$1" "$2"; sleep 0.4; xdotool click 1; sleep "${3:-1.2}"; }
shot() { import -window root "$1"; }
need() { command -v "$1" >/dev/null || { echo "ref-capture: need $1" >&2; exit 2; }; }

case ${1:-} in
prefix)
  need wine; need xdotool; need import
  pgrep -x Xvfb -a | grep -q "$DISPLAY_NUM" || {
    setsid nohup Xvfb "$DISPLAY_NUM" -screen 0 "${RES}x24" >/dev/null 2>&1 & sleep 1; }
  mkdir -p "$PREFIX"
  WINEDLLOVERRIDES="mscoree,mshtml=" wineboot -u >/dev/null 2>&1 || true
  # wine 11.16's own lib64 copies are not marked as builtin, and this prefix gets no
  # d3d11/d3d9 at all, so a forced-builtin launch fails with c0000135. ~/.wine's copies
  # carry the wine builtin marker and load; copy those four in.
  for d in d3d11.dll d3d9.dll d3d10core.dll dxgi.dll; do
    [ -f "$PREFIX/drive_c/windows/system32/$d" ] && continue
    cp "$HOME/.wine/drive_c/windows/system32/$d" "$PREFIX/drive_c/windows/system32/$d"
  done
  echo "ref-capture: prefix ready at $PREFIX"
  ;;
register)
  lib=${2:?usage: ref-capture.sh register '<library dir>'}
  [ -d "$lib" ] || { echo "ref-capture: no such library dir: $lib" >&2; exit 2; }
  mkdir -p "$PORTABLE/Libraries"
  ln -sfn "$(cd "$lib" && pwd)" "$PORTABLE/Libraries/$(basename "$lib")"
  # Library Manager is a 32-bit Win32 app: it takes input normally, unlike Kontakt's QML.
  ( cd "$PORTABLE/Helper/Library Manager" && setsid nohup wine "Library Manager.exe" \
      >/tmp/ref-capture-libmgr.log 2>&1 & ) ; sleep 15
  click 692 456 0.6        # "Libraries folder" checkbox
  click 827 613 6          # Start -> scan
  sleep 4; xdotool mousemove 550 390; xdotool click 1; xdotool key alt+F4; sleep 3   # close first report
  xdotool mousemove 550 390; xdotool click 1; xdotool key alt+F4; sleep 3            # close any second
  click 960 656 4          # SAVE
  xdotool key Return; sleep 1
  pkill -f 'Library.Manager[.]exe' 2>/dev/null || true
  grep -q "^\[$(basename "$lib" .nicnt)\]" "$PORTABLE/UserData/Settings.cfg" ||
    grep -q "$(basename "$lib")" "$PORTABLE/UserData/Settings.cfg" ||
    { echo "ref-capture: registration did not reach Settings.cfg" >&2; exit 1; }
  echo "ref-capture: registered $(basename "$lib") in $PORTABLE/UserData/Settings.cfg"
  ;;
capture)
  nki=${2:?usage: ref-capture.sh capture IN.nki OUTDIR [CROP]}
  out=${3:?missing OUTDIR}; crop=${4:-}
  mkdir -p "$out"
  win_nki=$(winepath -w "$nki")
  ( setsid nohup wine "$EXE" "$win_nki" >"$out/kontakt.log" 2>&1 & )
  sleep "${REF_WAIT:-45}"
  shot "$out/desktop.png"
  [ -n "$crop" ] && import -window root -crop "$crop" +repage "$out/page.png"
  echo "ref-capture: wrote $out/desktop.png${crop:+ and $out/page.png}"
  echo "ref-capture: run 'ref-capture.sh probe' before driving the UI by hand."
  ;;
probe)
  # A live Kontakt answers a click on its browser tab row; this one does not.
  need xdotool; need magick
  t=$(mktemp -d); shot "$t/a.png"
  xdotool mousemove 210 168; sleep 0.4; xdotool click 1; sleep 1.5; shot "$t/b.png"
  d=$( { magick compare -metric AE "$t/a.png" "$t/b.png" null: 2>&1 || true; } | awk '{print $1}')
  rm -rf "$t"
  if [ "${d%%.*}" -gt 50 ]; then echo "probe: pointer input Live (${d}px changed)"; else
    echo "probe: pointer input DEAD (${d}px changed) — Kontakt's QML area is not taking input;"
    echo "       see the KNOWN BLOCKER note at the top of this script."; exit 1; fi
  ;;
stop)
  pkill -x 'Kontakt 8.exe' 2>/dev/null || true
  sleep 3
  pgrep -x Xvfb -a | grep -q "$DISPLAY_NUM" && pkill -f "Xvfb $DISPLAY_NUM" || true
  ;;
*) sed -n 2,10p "$0"; exit 2 ;;
esac
