#!/usr/bin/env bash
# Prove tools/build-slot behaves without building anything: no cargo, no
# workspace compile. Every case runs against KONTRA_SLOT_ROOT inside a throwaway
# /var/tmp directory, so a bug here cannot touch the real slots or /var/tmp/tgt-*.
#
#   tools/test_build_slot.sh
#
# /var/tmp, not /tmp: the point of the tool is that slots live on real disk, and
# a test that put them on the 16 GB tmpfs would be testing the wrong filesystem.
set -euo pipefail
export LC_ALL=C

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
slot="$repo_root/tools/build-slot"
tmp=$(mktemp -d /var/tmp/build-slot-test.XXXXXX)
trap 'rm -rf -- "$tmp"' EXIT
export KONTRA_SLOT_ROOT="$tmp/slots"
root=$KONTRA_SLOT_ROOT

pass=0
fail=0
ok()   { pass=$((pass + 1)); printf 'ok   %s\n' "$1"; }
bad()  { fail=$((fail + 1)); printf 'FAIL %s\n' "$1"; }
check() { # description actual expected
    if [ "$2" = "$3" ]; then ok "$1"; else
        bad "$1"
        printf '       expected: %s\n       actual:   %s\n' "$3" "$2"
    fi
}
contains() { # description haystack needle
    case $2 in *"$3"*) ok "$1" ;; *) bad "$1"; printf '       missing: %s\n       in: %s\n' "$3" "$2" ;; esac
}

[ -x "$slot" ] || { echo "tools/build-slot is not executable" >&2; exit 1; }

# --- a fake worktree: a real repository, small, on real disk ----------------
mkrepo() { # path
    git init -q --initial-branch=main "$1"
    git -C "$1" config user.email t@t
    git -C "$1" config user.name t
    printf 'a\n' >"$1/a"
    git -C "$1" add a
    git -C "$1" commit -qm one
}

# A lane merged into main and left clean: HEAD on lane, lane == main's merge.
merged_repo() { # path
    mkrepo "$1"
    git -C "$1" checkout -q -b lane
    printf 'b\n' >>"$1/a"; git -C "$1" commit -qam two
    git -C "$1" checkout -q main
    git -C "$1" merge -q --no-ff lane -m merge
    git -C "$1" checkout -q lane
}

# --- 1. the command receives the slot it was told to use --------------------
merged_repo "$tmp/wt-alloc"
got=$(cd "$tmp/wt-alloc" && "$slot" --name alpha bash -c 'printf "%s" "$CARGO_TARGET_DIR"' 2>/dev/null)
check "--name alpha sets CARGO_TARGET_DIR" "$got" "$root/kontra-slot-alpha"

got=$(cd "$tmp/wt-alloc" && "$slot" bash -c 'printf "%s" "$CARGO_TARGET_DIR"' 2>/dev/null)
check "default name is the worktree basename" "$got" "$root/kontra-slot-wt-alloc"

[ -f "$root/kontra-slot-wt-alloc.slot" ] && ok "marker written on every run" || bad "marker written on every run"
contains "marker records worktree" "$(cat "$root/kontra-slot-wt-alloc.slot")" "$tmp/wt-alloc"
contains "marker records branch" "$(cat "$root/kontra-slot-wt-alloc.slot")" "branch='lane'"
contains "marker records base" "$(cat "$root/kontra-slot-wt-alloc.slot")" "base='main'"
contains "marker records last use" "$(cat "$root/kontra-slot-wt-alloc.slot")" "last_used='"

# The target directory must be a plain directory under the root, never a
# hand-named legacy dir.
contains "slot lives under KONTRA_SLOT_ROOT" "$got" "$root/kontra-slot-"
case $got in *"/tgt-"*) bad "slot must not use the legacy tgt-* names" ;; *) ok "slot must not use the legacy tgt-* names" ;; esac

# A second worktree with the same basename must not adopt that slot: sharing a
# target directory between worktrees is how this workspace got false passes.
mkdir -p "$tmp/other/wt-alloc"
merged_repo "$tmp/other/wt-alloc"
rm -f "$tmp/ran"
set +e
out=$(cd "$tmp/other/wt-alloc" && "$slot" bash -c "touch '$tmp/ran'" 2>&1)
rc=$?
set -e
[ "$rc" -ne 0 ] && ok "a slot is not shared between same-named worktrees ($rc)" || bad "a slot is not shared between same-named worktrees"
[ -e "$tmp/ran" ] && bad "collision must not run the command" || ok "collision must not run the command"
contains "collision names the owning worktree" "$out" "$tmp/wt-alloc"

# --- 2. exit code is the command's, not the wrapper's -----------------------
set +e
(cd "$repo_root" && "$slot" --name exit7 bash -c 'exit 7' >/dev/null 2>&1)
rc=$?
set -e
check "exit status propagates (7)" "$rc" "7"

# A command's own flags are not parsed as this script's.
got=$(cd "$repo_root" && "$slot" --name exit7 printf '%s' --gc 2>/dev/null)
check "flags after the command reach the command" "$got" "--gc"

# --- 3. low space refuses before running, and says how to reclaim ----------
rm -f "$tmp/ran"
set +e
out=$(cd "$repo_root" && "$slot" --name lowspace --min-free-gb 999999 bash -c "touch '$tmp/ran'" 2>&1)
rc=$?
set -e
[ "$rc" -ne 0 ] && ok "low space exits non-zero ($rc)" || bad "low space exits non-zero"
[ -e "$tmp/ran" ] && bad "low space must not run the command" || ok "low space must not run the command"
contains "refusal names the floor" "$out" "floor"
contains "refusal names the shortfall" "$out" "short of"
contains "refusal lists largest slots" "$out" "reclaim candidates"
contains "refusal prints the gc command" "$out" "--gc"
[ -d "$root/kontra-slot-lowspace" ] && bad "refusal leaves no slot directory" || ok "refusal leaves no slot directory"

# --- 4. --list reports size and owner --------------------------------------
head -c 2097152 /dev/zero >"$root/kontra-slot-alpha/blob"
out=$(cd "$repo_root" && "$slot" --list 2>/dev/null)
contains "--list names the slot" "$out" "wt-alloc"
contains "--list reports the owning worktree" "$out" "$tmp/wt-alloc"
contains "--list reports a size" "$out" "MB"
contains "--list reports the branch" "$out" "lane"
contains "--list totals the slots" "$out" "total"

# --- 5. --gc removes merged+clean, skips unmerged and current --------------
# A legacy hand-named directory sits in the same root to prove --gc leaves it
# alone: it is not a slot, and removing it is not this tool's decision.
mkdir -p "$root/tgt-legacy" && printf 'x\n' >"$root/tgt-legacy/keep"

merged_repo "$tmp/wt-merged"                       # merged and clean -> removed
merged_repo "$tmp/wt-current"                      # merged and clean, but gc runs here -> kept
mkrepo "$tmp/wt-ahead"                             # a commit main does not have -> kept
git -C "$tmp/wt-ahead" checkout -q -b lane
printf 'b\n' >>"$tmp/wt-ahead/a"; git -C "$tmp/wt-ahead" commit -qam two

for r in wt-merged wt-current wt-ahead; do
    (cd "$tmp/$r" && "$slot" --base main true) 2>/dev/null
done

# Give the merged slot something to reclaim, so the byte count is meaningful.
head -c 1048576 /dev/zero >"$root/kontra-slot-wt-merged/blob"

out=$(cd "$tmp/wt-current" && "$slot" --gc 2>&1); rc=$?
check "--gc exits 0" "$rc" "0"
contains "--gc removes the merged+clean slot" "$out" "remove wt-merged"
[ -d "$root/kontra-slot-wt-merged" ] && bad "merged slot directory removed" || ok "merged slot directory removed"
[ -f "$root/kontra-slot-wt-merged.slot" ] && bad "merged slot marker removed" || ok "merged slot marker removed"
contains "--gc keeps the current slot" "$out" "current worktree"
[ -d "$root/kontra-slot-wt-current" ] && ok "current slot directory kept" || bad "current slot directory kept"
contains "--gc skips the unmerged lane" "$out" "unmerged"
[ -d "$root/kontra-slot-wt-ahead" ] && ok "unmerged slot directory kept" || bad "unmerged slot directory kept"
contains "--gc reports bytes reclaimed" "$out" "reclaimed"
contains "--gc never touches legacy tgt-*" "$( [ -f "$root/tgt-legacy/keep" ] && echo kept || echo gone )" "kept"

# --- 6. a dirty lane is skipped, not deleted --------------------------------
merged_repo "$tmp/wt-dirty2"
(cd "$tmp/wt-dirty2" && "$slot" --base main true) 2>/dev/null
printf 'scratch\n' >"$tmp/wt-dirty2/untracked-file"
out=$(cd "$repo_root" && "$slot" --gc 2>&1)
contains "--gc skips a dirty worktree" "$out" "dirty"
[ -d "$root/kontra-slot-wt-dirty2" ] && ok "dirty slot directory kept" || bad "dirty slot directory kept"

# --- 7. the integration worktree's own slot is never reclaimed --------------
mkrepo "$tmp/wt-integration"
git -C "$tmp/wt-integration" checkout -q -b feat/integration
(cd "$tmp/wt-integration" && "$slot" true) 2>/dev/null
out=$(cd "$repo_root" && "$slot" --gc 2>&1)
contains "--gc keeps the integration worktree's slot" "$out" "branch is the base"
[ -d "$root/kontra-slot-wt-integration" ] && ok "integration slot directory kept" || bad "integration slot directory kept"

# --- 8. --gc honours KONTRA_SLOT_ROOT: the real root is untouched ----------
out=$(cd "$repo_root" && "$slot" --list 2>&1)
contains "--list reads the same root" "$out" "$root"

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[ "$fail" -eq 0 ]
