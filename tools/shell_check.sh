#!/usr/bin/env bash
# shell_check.sh — syntax-check and lint every shell script in the tree.
#
# WHY A SCRIPT AND NOT A FEW LINES IN ci.yml. The first version of the CI job inlined these steps,
# they were verified locally by running the same commands BY HAND, and the job failed on its first
# run anyway: the local shellcheck was 0.11.0 and the runner's was older, and the two disagree
# about `#!/system/xbin/busybox sh` (older ones emit SC2187 for it, newer ones do not).
#
# Two lessons, both baked in here:
#   1. Local and CI must run THE SAME THING, not two things that look alike. CI calls this file.
#   2. A lint gate is only reproducible if its version is. See PINNED below.
#
# 5,288 lines across 33 scripts, and it is not peripheral code: the launcher, the crash supervisor,
# the bad-boot counter and the USB-MSC mount ordering all live here, all run as root, and all sit
# on the boot path (docs/SHORTCOMINGS.md §A3).
#
#   tools/shell_check.sh          check everything
#   tools/shell_check.sh --list   just list what would be checked
set -uo pipefail
cd "$(dirname "$0")/.." || { echo "cannot reach the repo root" >&2; exit 2; }

# The version CI installs and the one to match locally:  pip install shellcheck-py==0.11.0.1
# Kept as a note rather than enforced, so a contributor with a distro shellcheck is not blocked —
# but a DISAGREEMENT is reported, because that is what made the first CI run red.
PINNED_MAJOR_MINOR="0.11"

mapfile -t SCRIPTS < <(find . -name '*.sh' -not -path './.git/*' | sort)
if [ "${1:-}" = "--list" ]; then printf '%s\n' "${SCRIPTS[@]}"; exit 0; fi

echo "── shell check ─────────────────────────────────────────────────────"
echo "   ${#SCRIPTS[@]} scripts"

fail=0

# ── 1. syntax ───────────────────────────────────────────────────────────
# `bash -n` on a dash/ash script is still worth running: the constructs these scripts use are a
# subset both accept, so a parse error here is a real error either way.
syn=0
for f in "${SCRIPTS[@]}"; do
    if ! err=$(bash -n "$f" 2>&1); then
        echo "  FAIL (syntax)  $f"
        printf '%s\n' "$err" | sed 's/^/      /'
        syn=$((syn + 1)); fail=1
    fi
done
[ "$syn" -eq 0 ] && echo "  ok    bash -n: all parse"

# ── 2. lint ─────────────────────────────────────────────────────────────
if ! command -v shellcheck >/dev/null 2>&1; then
    echo "  SKIP  shellcheck not installed (pip install shellcheck-py==$PINNED_MAJOR_MINOR.0.1)"
else
    have=$(shellcheck --version | awk '/^version:/ {print $2}')
    case "$have" in
        "$PINNED_MAJOR_MINOR"*) : ;;
        *) echo "  NOTE  shellcheck $have, expected $PINNED_MAJOR_MINOR.x — findings may differ"
           echo "        (this exact skew is what made the first CI run of this gate red)" ;;
    esac
    # -S warning, not the default. The tree is clean at that level; `info`/`style` would add ~55
    # findings that are taste, not defects. Raise the bar once those are worked off, not before.
    if shellcheck -S warning -f gcc "${SCRIPTS[@]}"; then
        echo "  ok    shellcheck $have -S warning: clean"
    else
        echo "  FAIL  shellcheck findings above"
        fail=1
    fi
fi

echo
[ "$fail" -eq 0 ] && echo "── all clear ───────────────────────────────────────────────────────" \
                  || echo "── FAILURES above ──────────────────────────────────────────────────"
exit "$fail"
