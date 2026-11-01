#!/usr/bin/env bash
# Gate a build on the self-test loop before any binary is handed over.
#
#   tools/verify.sh            run every check, print a table, exit non-zero on failure
#   tools/verify.sh --quick    skip the audio render (fast; still runs the unit layers)
#   tools/verify.sh --portable run only the rows that need nothing from this
#                              machine: no sample library, no Kontakt, no GUI.
#                              That is tests, build, clap_entry, rustfmt and
#                              clippy -- the same checks CI can run. It prints
#                              its own PORTABLE PASSED line, because it is not a
#                              delivery gate and must never be read as one.
#
# What it refuses to let through:
#   - a failing test in the crates the change touched;
#   - a plugin build with no CLAP entry (a host would not see it);
#   - Electric Mint asking for more script calls than the agreed ceiling, or a
#     load that produced no answer at all;
#   - a render that does not complete, drops sample frames, or clips;
#   - a UI whose controls are not where they are drawn (layer 2), including a
#     ui-drive that does not build and a page that drew no control to check;
#   - a page that no longer looks like the reviewed one (layer 3: a golden
#     regression check, not a live Kontakt comparison -- it catches a page that
#     changes, not a page that was wrong to begin with). A live comparison is
#     now possible: tools/oracle-mcp renders and screenshots real Kontakt on a
#     private Xvfb with DXVK + lavapipe (docs/re/kontakt-headless-gui.md), so
#     `tools/oracle-mcp/parity.py` is the on-demand check and this gate stays
#     offline and fast. A geometry check cannot
#     see a picture that resolved to nothing -- it still has a rect -- which is
#     how a missing graphic shipped once. If no reference capture exists the
#     gate fails: ALLOW_NO_PIXEL_REF=1 is a logged admission that layer 3 is off.
#   - a Sound Preset panel that no longer looks like *Kontakt's* render of it
#     (layer 3, second row): a crop of Kontakt 8.14.1's own page, cut by
#     tools/kontakt-reference/panel-reference.py, diffed against our panel at a
#     measured ceiling. The golden row below it compares us to ourselves and so
#     can only see change; this row can see a panel that was simply wrong, which
#     is how the preset field persisted in review.
set -uo pipefail

REPO=$(cd "$(dirname "$0")/.." && pwd)
# /tmp is a 16 GB tmpfs on this machine; a release build of the workspace does
# not fit beside three lanes. Keep the target on real disk.
TARGET=${CARGO_TARGET_DIR:-/var/tmp/tgt-int}
# $HOME rather than an absolute path: the gate is run from a checkout, and a
# committed home directory names whoever ran it last.
EMINT=${EMINT:-$HOME/Armory/VSTs/Kontakt1/Native Instruments Session Guitarist Electric Mint 1.1.0/Session Guitarist - Electric Mint Library/Instruments/Electric Mint.nki}
# Ceilings, not goals: they must only ever come down.
MAX_UNHANDLED=${MAX_UNHANDLED:-55}
MAX_INIT_UNANSWERED=${MAX_INIT_UNANSWERED:-51}
# Percent of the page allowed to differ from Kontakt's own render.
MAX_PIXEL_PCT=${MAX_PIXEL_PCT:-0.5}
# A build or a GUI step that hangs is a failed row, not a hung gate: every cargo
# and ui call below is wrapped. 30 minutes is generous for a cold release build
# of the crates the gate names and still finite.
STEP_TIMEOUT=${STEP_TIMEOUT:-1800}
QUICK=
MODE=full
for arg in "$@"; do
    case "$arg" in
        --quick) QUICK=--quick ;;
        --portable) MODE=portable ;;
        *) echo "verify: unknown argument '$arg'" >&2; exit 2 ;;
    esac
done

cd "$REPO"
# One private directory holds every log, JSON and PNG this gate writes, made
# before the first redirect. A fixed /tmp path is a symlink a local user can
# pre-create (CWE-377), and on a machine without that directory the redirect
# fails and the step "fails" without ever running. Kept after the run so a
# failure message can name a file that still exists.
LOGS=${KONTRA_VERIFY_LOGS:-$(mktemp -d "${TMPDIR:-/tmp}/kontra-verify.XXXXXX")} \
    || { echo "verify: cannot create a log directory (mktemp failed)" >&2; exit 2; }
mkdir -p "$LOGS"
FAILED=0
declare -a ROWS

row() { ROWS+=("$(printf '%-42s %s' "$1" "$2")"); }
bad() { row "$1" "FAIL  $2"; FAILED=1; }
ok()  { row "$1" "ok    $2"; }

# Print the table and exit, for both modes. A function rather than two copies of
# the tail, so the portable path can never drift into printing the delivery
# gate's success line.
finish() {
    echo
    printf '%s\n' "${ROWS[@]}"
    echo
    echo "logs: $LOGS"
    if [ "$FAILED" -ne 0 ]; then
        echo "VERIFY FAILED -- do not deploy"
        exit 1
    fi
    # The play dump is a few hundred MB of per-voice detail that nothing reads
    # after the rows above, and /tmp is a 16 GB tmpfs here. Keep it (and the
    # rest) only when something failed and it is worth reading.
    rm -f "$LOGS/verify-play.json"
    if [ "$MODE" = portable ]; then
        echo "PORTABLE PASSED -- not a delivery gate: the library, render, ui-drive"
        echo "and pixel rows did not run. tools/verify.sh (no flag) is what gates a deploy."
        exit 0
    fi
    echo "VERIFY PASSED -- safe to deploy with tools/deploy.sh"
}

# Print the table and exit, for both modes. A function rather than two copies of
# the tail, so the portable path can never drift into printing the delivery
# gate's success line.
finish() {
    echo
    printf '%s\n' "${ROWS[@]}"
    echo
    echo "logs: $LOGS"
    if [ "$FAILED" -ne 0 ]; then
        echo "VERIFY FAILED -- do not deploy"
        exit 1
    fi
    # The play dump is a few hundred MB of per-voice detail that nothing reads
    # after the rows above, and /tmp is a 16 GB tmpfs here. Keep it (and the
    # rest) only when something failed and it is worth reading.
    rm -f "$LOGS/verify-play.json"
    if [ "$MODE" = portable ]; then
        echo "PORTABLE PASSED -- not a delivery gate: the library, render, ui-drive"
        echo "and pixel rows did not run. tools/verify.sh (no flag) is what gates a deploy."
        exit 0
    fi
    echo "VERIFY PASSED -- safe to deploy with tools/deploy.sh"
}

# One layer-3 row from one pixel-diff run: the label, then pixel-diff's own
# arguments (reference, render, thresholds). Exit 1 is over the ceiling, exit 2
# means the two could not be compared at all -- neither of which is a pass.
pixel_row() {
    local name=$1
    shift
    local out status reason
    out=$(python3 "$REPO/tools/pixel-diff.py" "$@" 2>&1)
    status=$?
    if [ "$status" -eq 0 ]; then
        ok "$name" "$(printf '%s\n' "$out" | grep -o 'differing.*%' | head -1)"
    elif [ "$status" -eq 2 ]; then
        # "cannot be compared" (missing, unreadable, or different-sized
        # without --scale or --crop) is neither a threshold failure nor a pass.
        bad "$name" "$(printf '%s\n' "$out" | head -1)"
    else
        reason=$(printf '%s\n' "$out" | grep -oE '[0-9.]+% differs, ceiling [0-9.]+%' | head -1)
        bad "$name" "${reason:-pixel-diff exited $status with no summary}"
        printf '%s\n' "$out" | grep -E '^ +[0-9]+ px' | head -5 | sed 's/^/      /'
    fi
}

run_tests() {
    local crate=$1
    local out
    out=$(CARGO_TARGET_DIR="$TARGET" timeout "$STEP_TIMEOUT" cargo test -p "$crate" 2>&1)
    local line
    line=$(printf '%s\n' "$out" | grep -E "^test result:" | awk '{p+=$4; f+=$6} END {print p" passed, "f" failed"}')
    # A crate that fails to COMPILE prints no result line at all, so "no
    # result" is a failure, not a pass -- that is how a broken test file once
    # reported ok.
    if printf '%s\n' "$out" | grep -qE "^test result: FAILED"; then
        bad "tests: $crate" "${line:-some tests failed}"
    elif ! printf '%s\n' "$out" | grep -qE "^test result:"; then
        bad "tests: $crate" "did not compile or did not run"
    else
        ok "tests: $crate" "${line:-no result line}"
    fi
}

# rustfmt, first-party only. cargo fmt --all --check exits non-zero when
# vendor/ differs, which it does and always has, so the exit code is not the
# verdict: the verdict is whether any *first-party* file appears in the diff
# list. A run that produced no diff list at all (no rustfmt, broken manifest) is
# a failure too -- "nothing to report" must never mean "passed".
lint_fmt() {
    local out status diffs
    out=$(cargo fmt --all --check 2>&1); status=$?
    diffs=$(printf '%s\n' "$out" | grep '^Diff in ' | grep -v '/vendor/' |
        sed 's/^Diff in //;s/:[0-9]*:$//' | sort -u)
    if [ -n "$diffs" ]; then
        bad "rustfmt" "$(printf '%s\n' "$diffs" | wc -l) first-party file(s) not formatted"
        printf '%s\n' "$diffs" | head -5 | sed 's/^/      /'
    elif [ "$status" -ne 0 ] && ! printf '%s\n' "$out" | grep -q '^Diff in '; then
        bad "rustfmt" "could not run: $(printf '%s\n' "$out" | head -1)"
    else
        ok "rustfmt" "first-party code is formatted (vendor/ excluded)"
    fi
}

# clippy, every target, first-party only, warnings are errors. The exclusion
# list is derived from the manifests' paths, not hand-kept, so vendoring another
# crate cannot silently put it inside our lint.
lint_clippy() {
    local out status errs excludes
    excludes=$(cargo metadata --no-deps --format-version 1 2>/dev/null | python3 -c '
import json, sys
m = json.load(sys.stdin)
print(" ".join("--exclude " + p["name"] for p in m["packages"]
               if "/vendor/" in p["manifest_path"].replace("\\\\", "/")))
' 2>/dev/null)
    # Word-splitting is intended: $excludes is a list of --exclude flags.
    # shellcheck disable=SC2086
    out=$(CARGO_TARGET_DIR="$TARGET" timeout "$STEP_TIMEOUT" cargo clippy \
        --workspace $excludes --all-targets --no-deps \
        --message-format=short -- -D warnings 2>&1); status=$?
    errs=$(printf '%s\n' "$out" | grep -E '^[^ :]+:[0-9]+:[0-9]+: error' | sort -u)
    if [ -n "$errs" ]; then
        bad "clippy" "$(printf '%s\n' "$errs" | wc -l) finding(s) under -D warnings"
        printf '%s\n' "$errs" | head -5 | sed 's/^/      /'
    elif [ "$status" -ne 0 ]; then
        bad "clippy" "could not run: $(printf '%s\n' "$out" | grep -m1 '^error' || printf '%s\n' "$out" | head -1)"
    else
        ok "clippy" "first-party crates clean under -D warnings (all targets)"
    fi
}

echo "== layer 0/1: tests =="
# kontra-plugin and kontra-ui-pictures are here because the plugin is what
# ships: a page that renders in the harness and not in the host is exactly how
# the wallpaper went missing (a 7952-row picture became 15904 at the live 2x
# device scale, past the renderer's 8192-pixel atlas, and was dropped silently).
#
# kontra-racks, kontra-ledger, kontra-inspect and ui-drive are here because
# their tests ran in NEITHER this gate nor CI (the CI layers list neither crate,
# and `cargo test -p kontra-engine` compiles racks without running it). Two of
# them are the gate's own instruments -- kontra-inspect measures every audio row
# below and ui-drive asserts the UI -- so a silent break there would make the
# gate report a pass it could not justify.
for c in kontra-engine kontra-host kontra-ui mui kontra-native-host kontra-plugin kontra-ui-pictures kontra-racks kontra-ledger kontra-inspect ui-drive; do run_tests "$c"; done
if [ -d crates/dsp/delay ]; then run_tests kontra-dsp-delay; fi

# The two parity regressions need a Kontakt library installed outside the
# repository. They are `#[ignore]`d so a clean checkout reports them as
# ignored rather than passing them without reading anything; run them here
# when the library is present, and fail the gate if either one fails.
PARITY=${KONTRA_TEST_LIBRARY:-${HOME}/Armory/VSTs/Kontakt1}
if [ -d "$PARITY" ]; then
    out=$(CARGO_TARGET_DIR="$TARGET" timeout "$STEP_TIMEOUT" cargo test -p kontra-host --test parity_regressions -- --ignored 2>&1)
    line=$(printf '%s\n' "$out" | grep -E "^test result:" | awk '{p+=$4; f+=$6} END {print p" passed, "f" failed"}')
    if printf '%s\n' "$out" | grep -qE "^test result: FAILED"; then
        bad "tests: parity regressions" "${line:-some tests failed}"
    elif ! printf '%s\n' "$out" | grep -qE "^test result:"; then
        bad "tests: parity regressions" "did not compile or did not run"
    else
        ok "tests: parity regressions" "${line:-no result line}"
    fi
else
    row "tests: parity regressions" "skipped (no library at $PARITY)"
fi

echo "== build =="
if CARGO_TARGET_DIR="$TARGET" timeout "$STEP_TIMEOUT" cargo build --release -p kontra-plugin >"$LOGS/verify-build.log" 2>&1; then
    ok "build kontra-plugin" "$(stat -c%s "$TARGET/release/libkontra_plugin.so" 2>/dev/null || echo '?') bytes"
else
    bad "build kontra-plugin" "see $LOGS/verify-build.log"
fi
if nm -D --defined-only "$TARGET/release/libkontra_plugin.so" 2>/dev/null | grep -q " clap_entry$"; then
    ok "exports clap_entry" "a CLAP host will see it"
else
    bad "exports clap_entry" "the .so has no CLAP entry"
fi

echo "== layer 0/1: the style the review checklist claims =="
# Neither this gate nor CI ran rustfmt or clippy before 2026-11-01, so the
# review checklist's "fmt and clippy clean on the touched crates" was a
# convention with nothing behind it: a warning could be merged and nobody would
# know for a week. Both are rows now, and both are strict.
#
# vendor/ is excluded from both. It is third-party source we consume, its
# formatting is not ours to churn, and it carries two clippy errors we did not
# write (vendor/mui's #[allow] without a reason). Excluding by manifest path
# rather than by a hand-kept list, so vendoring another crate cannot silently
# put it inside our lint.
lint_fmt
lint_clippy

# Portable mode stops here. Everything below needs this machine's sample
# library, and on a runner that has none the rows would print "skipped" -- a
# word this project has learned to read as "passed". Named as not-run instead.
if [ "$MODE" = portable ]; then
    row "load + render (Electric Mint)" "local-only, not run (--portable)"
    row "ui-drive (both pages)" "local-only, not run (--portable)"
    row "library free of probe copies" "local-only, not run (--portable)"
    row "pixel diff (golden + Kontakt panel)" "local-only, not run (--portable)"
    finish
fi

echo "== layer 0: Electric Mint =="
if CARGO_TARGET_DIR="$TARGET" cargo build --release -p kontra-inspect >"$LOGS/verify-inspect.log" 2>&1; then
    INSPECT="$TARGET/release/kontra-inspect"
    LOADOUT=$("$INSPECT" load "$EMINT" 2>&1)
    LOADSTATUS=$?
    printf '%s\n' "$LOADOUT" >"$LOGS/verify-load.txt"
    LOAD=$(printf '%s\n' "$LOADOUT" | grep -o "init unanswered: \[[^]]*\]" | head -1)
    # A sum over nothing is 0, which used to read as "nothing unanswered" when
    # the load had failed, the instrument was missing, or the output changed
    # shape. An empty sum is now distinguishable from a measured zero.
    INIT=$(printf '%s' "$LOAD" | python3 -c 'import re,sys; n=re.findall(r", (\d+)\)", sys.stdin.read()); print(sum(int(i) for i in n) if n else "")')
    if [ "$LOADSTATUS" -ne 0 ]; then
        bad "init unanswered" "kontra-inspect load exited $LOADSTATUS; see $LOGS/verify-load.txt"
    elif [ -z "$LOAD" ] || [ -z "$INIT" ]; then
        bad "init unanswered" "load printed no 'init unanswered:' counts; see $LOGS/verify-load.txt"
    elif [ "$INIT" -le "$MAX_INIT_UNANSWERED" ]; then
        ok "init unanswered" "$INIT (ceiling $MAX_INIT_UNANSWERED)"
    else
        bad "init unanswered" "$INIT above ceiling $MAX_INIT_UNANSWERED"
    fi

    if [ "$QUICK" != "--quick" ]; then
        "$INSPECT" play "$EMINT" --json "$LOGS/verify-play.json" >"$LOGS/verify-play.txt" 2>&1
        PLAYSTATUS=$?
        if [ "$PLAYSTATUS" -ne 0 ] || [ ! -f "$LOGS/verify-play.json" ]; then
            bad "render completes" "kontra-inspect play exited $PLAYSTATUS; see $LOGS/verify-play.txt"
        else
            read -r DONE UNH FNV FRAMES CLIPPED < <(python3 - "$LOGS/verify-play.json" "$LOGS/verify-play.txt" <<'PY'
import json,re,sys
d=json.load(open(sys.argv[1]))['result']
m=re.search(r'actions_unhandled: (\d+)', d['engine_stats'])
txt=open(sys.argv[2]).read()
fr=re.search(r'missing sample frames: (\d+)', txt)
cl=re.search(r'clipped samples (\d+)', txt)
print(d['render_complete'], m.group(1) if m else '?', d['audio_fnv1a64'],
      fr.group(1) if fr else '?', cl.group(1) if cl else '?')
PY
)
            [ "$DONE" = "True" ] && ok "render completes" "yes" || bad "render completes" "no"
            [ "${UNH:-999999}" -le "$MAX_UNHANDLED" ] 2>/dev/null && ok "actions_unhandled" "$UNH (ceiling $MAX_UNHANDLED)" || bad "actions_unhandled" "$UNH above ceiling"
            [ "${FRAMES:-1}" = "0" ] && ok "missing sample frames" "0" || bad "missing sample frames" "$FRAMES"
            [ "${CLIPPED:-1}" = "0" ] && ok "clipped samples" "0" || bad "clipped samples" "$CLIPPED"
            ok "audio fingerprint" "$FNV"
        fi
    fi
else
    bad "build kontra-inspect" "see $LOGS/verify-inspect.log"
fi

echo "== layer 0: the sample library is not our scratch space =="
LIB=$(dirname "$(dirname "$EMINT")")
LEFTOVER=$(ls -d "$LIB"/Instruments/__* 2>/dev/null | head -1)
if [ -n "$LEFTOVER" ]; then
    bad "library free of probe copies" "left behind: $(basename "$LEFTOVER")"
else
    ok "library free of probe copies" "no __* directory under Instruments/"
fi

echo "== layer 2: the UI drives where it draws =="
if CARGO_TARGET_DIR="$TARGET" cargo build --release -p ui-drive >"$LOGS/verify-drive.log" 2>&1; then
    if "$TARGET/release/ui-drive" "$EMINT" >"$LOGS/verify-drive.txt" 2>&1; then
        ok "ui-drive" "$(tail -1 "$LOGS/verify-drive.txt" | cut -c1-60)"
    else
        bad "ui-drive" "see $LOGS/verify-drive.txt"
    fi
    # The Melody instrument is a different page with different widgets -- its
    # pattern table is eight ui_xy rows, and for a while every one of them was
    # dead while the main instrument passed. One page is not a UI.
    MELODY="$(dirname "$EMINT")/Electric Mint (Melody).nki"
    if [ -f "$MELODY" ]; then
        if "$TARGET/release/ui-drive" "$MELODY" >"$LOGS/verify-drive-melody.txt" 2>&1; then
            ok "ui-drive (Melody)" "$(tail -1 "$LOGS/verify-drive-melody.txt" | cut -c1-52)"
        else
            bad "ui-drive (Melody)" "see $LOGS/verify-drive-melody.txt"
        fi
    else
        row "ui-drive (Melody)" "no Melody instrument installed"
    fi
else
    # ui-drive is a workspace member: a build failure is a defect, not "not
    # built yet". Recorded as skipped, a compile break silently switched off
    # layer 2 while the gate still printed PASSED.
    bad "ui-drive" "build failed; see $LOGS/verify-drive.log"
fi

echo "== layer 3: the page still looks like the reviewed capture =="
REF=${PIXEL_REF:-$(ls "$REPO"/tools/kontakt-reference/*-page.png 2>/dev/null | head -1)}
# The panel, cut out of Kontakt 8.14.1's own render (tools/kontakt-reference/
# panel-reference.py records the alignment and how to regenerate it). This row
# exists because the golden row compares us to *our own* earlier render: it
# can see that the page changed and never that it was wrong when it was
# captured, which is how a preset field drew a stock frame over its plate
# through three review rounds while every round agreed with the last wrong
# picture. This reference has no such memory of us in it.
PANEL_REF=${PIXEL_PANEL_REF:-$REPO/tools/kontakt-reference/electric-mint-panel-kontakt.png}
# Percent of the panel allowed to differ from Kontakt's. Ceilings, not goals:
# this one only ever comes down.
#
# Measured 2026-10-31 on this tree -- 2,304 of 12,213 px = 18.87% at tolerance
# 12/channel -- and it is the same number on every run, because the render is
# deterministic (the golden row reads 0 px against our own capture). The residue
# is known, and each part of it is a defect this row would rather we fixed:
#
#   713 px (5.84%) the preset plate's frame is drawn 1 px up and 1 px left of
#                  Kontakt's: its whole-width top and bottom border rows
#                  (page rows 34/35 and 51/52) and the columns of its sides;
#   416 px (3.41%) text rasterisation, "Sound Preset": our face is 64 px wide
#                  where Kontakt's bitmap is 68, so no glyph lands on the same
#                  pixel;
#   478 px (3.91%) the same, "Spacious EDM" in the field;
#   384 px (3.14%) the same, "Link to Song";
#   277 px (2.27%) the save / clear / arrow buttons in the header row, a pixel
#                  off Kontakt's own pictograms;
#    36 px (0.29%) the mint swatch in the "Link to Song" row.
#
# 21.0 is 2.13 points above that measurement and 8.12 points below the defect
# this row is for: with the field's stock frame drawn over the plate again (the
# fix in 1e6159ac5 reverted) the same region measures 29.12% (3,557 px).
MAX_PANEL_PCT=${MAX_PANEL_PCT:-21.0}
# The panel's rect in page coordinates: the preset plate (446,32,171,21) grown
# to take in the header row above it and the Link to Song row below, 2 px clear
# either side. panel-reference.py cuts the reference to exactly this window, so
# the two sides are compared in the same pixels. --merge 1 (not the default 3):
# the panel is 12,213 px, and a default-merged finding is one blob covering
# everything that moved; at 1 a field frame comes back as its own region at
# (447,33) and the swatch as its own at (449,61), which names the defect.
PANEL_CROP=${PANEL_CROP:-444,12,177,69}

SHOT=
SHOTERR=
if { [ -n "$REF" ] && [ -f "$REF" ]; } ||
   { [ -f "$PANEL_REF" ] && [ "${ALLOW_NO_PIXEL_REF:-0}" != "1" ]; }; then
    SHOTS=$LOGS/verify-shots
    rm -rf "$SHOTS"; mkdir -p "$SHOTS"
    if CARGO_TARGET_DIR="$TARGET" cargo run --release -q -p ui-shots -- "$SHOTS" "$EMINT" >"$LOGS/verify-shots.log" 2>&1; then
        SHOT=$(ls "$SHOTS"/*.png 2>/dev/null | head -1)
    else
        SHOTERR="see $LOGS/verify-shots.log"
    fi
fi

if [ -n "$REF" ] && [ -f "$REF" ]; then
    if [ -n "$SHOTERR" ]; then
        bad "pixel diff vs golden" "$SHOTERR"
    elif [ -n "$SHOT" ]; then
        pixel_row "pixel diff vs golden" "$REF" "$SHOT" --scale \
            --max-pct "$MAX_PIXEL_PCT" --out "$LOGS/verify-pixel"
    else
        bad "pixel diff vs golden" "ui-shots produced no png"
    fi
elif [ "${ALLOW_NO_PIXEL_REF:-0}" = "1" ]; then
    row "pixel diff vs golden" "OFF (ALLOW_NO_PIXEL_REF=1)"
else
    bad "pixel diff vs golden" "no golden in tools/kontakt-reference/"
fi

echo "== layer 3: the panel still looks like Kontakt's own render =="
if [ ! -f "$PANEL_REF" ]; then
    bad "pixel diff vs Kontakt panel" "no reference; run tools/kontakt-reference/panel-reference.py"
elif [ "${ALLOW_NO_PIXEL_REF:-0}" = "1" ] && [ -z "$REF" ]; then
    row "pixel diff vs Kontakt panel" "OFF (ALLOW_NO_PIXEL_REF=1)"
elif [ -n "$SHOTERR" ]; then
    bad "pixel diff vs Kontakt panel" "$SHOTERR"
elif [ -z "$SHOT" ]; then
    bad "pixel diff vs Kontakt panel" "ui-shots produced no png"
else
    pixel_row "pixel diff vs Kontakt panel" "$PANEL_REF" "$SHOT" \
        --crop "$PANEL_CROP" --merge 1 --max-pct "$MAX_PANEL_PCT" --out "$LOGS/verify-panel"
fi

finish
