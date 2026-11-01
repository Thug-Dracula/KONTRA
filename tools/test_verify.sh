#!/usr/bin/env bash
# The gate's own parsers, tested against the shapes real output takes.
#
# A gate row that misreads its input fails in one of two ways, and both have
# happened on this machine: it passes what it should fail, or it fails what it
# should pass. `init unanswered` did the second -- an instrument that had
# answered every init action printed `[]`, the sum over an empty list came back
# empty, and the row reported "load printed no counts" on the one load that had
# nothing left to answer. The ceiling exists to reach zero, so the gate went red
# exactly when the work succeeded.
#
# The functions are lifted out of tools/verify.sh rather than copied: a copy
# would test the copy. If the extraction finds nothing, the calls below fail and
# so does this script -- a rename cannot make it pass vacuously.
set -uo pipefail

HERE=$(cd "$(dirname "$0")" && pwd)
WORK=$(mktemp -d "${TMPDIR:-/tmp}/verify-parsers.XXXXXX")
trap 'rm -rf "$WORK"' EXIT
FAILED=0

# shellcheck disable=SC1090
source <(sed -n '/^init_unanswered()/,/^}/p' "$HERE/verify.sh")
# The panel row's decision, lifted the same way. It is a function rather than a
# condition inside the row for exactly this reason: the order of its two
# questions is the whole bug, and an `if` inside an `echo` cannot be held still
# without a render and a capture.
# shellcheck disable=SC1090
source <(sed -n '/^panel_row_kind()/,/^}/p' "$HERE/verify.sh")

check() {
    local name=$1 want=$2 file=$3 got
    got=$(init_unanswered "$file")
    if [ "$got" = "$want" ]; then
        printf '  ok   %-34s %s\n' "$name" "'$got'"
    else
        printf '  FAIL %-34s got %s, want %s\n' "$name" "'$got'" "'$want'"
        FAILED=1
    fi
}

# What the panel row does with the reference, the opt-out and the golden.
mode() { # name want exists allow golden
    local got
    if ! declare -F panel_row_kind >/dev/null; then
        printf '  FAIL %-34s verify.sh has no panel_row_kind to test\n' "$1"
        FAILED=1
        return
    fi
    got=$(panel_row_kind "$3" "$4" "$5")
    if [ "$got" = "$2" ]; then
        printf '  ok   %-34s %s\n' "$1" "'$got'"
    else
        printf '  FAIL %-34s got %s, want %s\n' "$1" "'$got'" "'$2'"
        FAILED=1
    fi
}

# A load that answered everything: the target state, and it must pass.
printf 'load: 6232 zones\ninit unanswered: []\nscripts: 0 slots\n' > "$WORK/empty"
check "every action handled counts 0" 0 "$WORK/empty"

# The shape measured today on Electric Mint.
printf 'init unanswered: [(SetNksNavPar, 25), (SetNksNavName, 21), (MfSetExportArea, 2), (MfCopyExportArea, 2), (MfSetNumExportAreas, 1)]\n' > "$WORK/some"
check "the shape measured today sums to 51" 51 "$WORK/some"

# No line at all: the load failed or the summary changed shape. This must not
# read as zero -- that was the older bug, and it hid a failed load.
printf 'load: failed\n' > "$WORK/missing"
check "a missing line is not a measurement" "" "$WORK/missing"

# Present but unparseable: also not a measurement.
printf 'init unanswered: [garbage]\n' > "$WORK/malformed"
check "a malformed list is not a measurement" "" "$WORK/malformed"

# One pair among junk: a partial parse must not report a partial sum.
printf 'init unanswered: [(SetNksNavPar, 4), junk]\n' > "$WORK/partial"
check "a partly parsed list is not a measurement" "" "$WORK/partial"

# Whitespace inside the brackets must not read as an empty list.
printf 'init unanswered: [   ]\n' > "$WORK/spaces"
check "brackets holding only spaces count 0" 0 "$WORK/spaces"

# --- the panel row: the opt-out has to be read before the missing file ------
# ALLOW_NO_PIXEL_REF=1 is the documented way to run the gate on a machine with no
# capture. The row tested the missing reference first, so the opt-out could not
# work in the one case it exists for, and the gate went red for a reason the
# caller had already said to ignore.
mode "no reference, no opt-out fails"          missing       0 0 0
mode "no reference, opt-out is off, not bad"   off-no-ref    0 1 0
mode "reference, no opt-out, compares"         compare       1 0 0
mode "reference, opt-out, no golden is off"    off-no-golden 1 1 0
mode "reference, opt-out, golden compares"     compare       1 1 1

# The row's message for a missing reference is unchanged: the opt-out reorders
# the questions, it does not soften the failure.
if grep -q 'no reference; run tools/kontakt-reference/panel-reference.py' "$HERE/verify.sh"; then
    printf '  ok   %-34s %s\n' "the missing-reference row keeps its message" "yes"
else
    printf '  FAIL %-34s %s\n' "the missing-reference row keeps its message" "message gone"
    FAILED=1
fi

# And the row must actually ask the tested function. A decision nobody consults
# is a comment: the cases above would all stay green while the row went back to
# checking the file first.
if grep -q 'case $(panel_row_kind' "$HERE/verify.sh"; then
    printf '  ok   %-34s %s\n' "the panel row asks panel_row_kind" "yes"
else
    printf '  FAIL %-34s %s\n' "the panel row asks panel_row_kind" "the row decides by itself"
    FAILED=1
fi

echo
if [ "$FAILED" -ne 0 ]; then
    echo "FAILED: a gate parser misreads its input"
    exit 1
fi
echo "PASS: 13 parser cases"
