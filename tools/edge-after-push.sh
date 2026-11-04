#!/usr/bin/env bash
# edge-after-push.sh — publish an edge build of a commit once GitHub has it. Started in the
# background by .githooks/pre-push after a push to main, when this clone has opted in:
#
#     git config cinder.edgeOnPush true      # on      (false, or unset, is off)
#
# By hand:  tools/edge-after-push.sh <commit> [--dry-run]
# Log:      .git/edge/last.log (one per run, the newest linked as last.log)
#
# It builds in a THROWAWAY CLONE of exactly that commit, never in this working tree: edge.sh
# refuses a tree with uncommitted changes, and the build would overwrite this tree's own
# cinder-home/dist with that commit's payload while you work. What the
# build needs that git does not hold is linked in from here: Sony's files (artifacts/), the
# player's runtime libraries (analysis/ramdisk/) and the AAC encoder source
# (cinder-aac/third_party/). The Rust build cache for the ARM target lives in
# ~/.cache/cinder-edge (not in .git, where 150 MB of build output would count as the repository's
# size), so only what changed is rebuilt.
#
# One build at a time. A build that waited for another to finish is skipped if main has moved on
# since — the edge workflow would cancel it for the newer one anyway.
set -uo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"
SHA="${1:?usage: tools/edge-after-push.sh <commit> [--dry-run]}"
DRY="${2:-}"
STATE="$REPO/.git/edge"
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/cinder-edge"
mkdir -p "$STATE" "$CACHE/target"
LOG="$STATE/edge-$(date +%Y%m%d-%H%M%S)-${SHA:0:7}.log"
ln -sfn "$(basename "$LOG")" "$STATE/last.log"
exec >>"$LOG" 2>&1
echo "edge-after-push: ${SHA:0:7}, $(date)"

exec 9>"$STATE/lock"
flock 9

ORIGIN="$(git -C "$REPO" remote get-url origin)"
# Wait for the push to land (the hook runs BEFORE git sends anything), up to five minutes.
remote=
for _ in $(seq 1 60); do
    remote="$(git ls-remote "$ORIGIN" refs/heads/main | cut -f1)"
    [ "$remote" = "$SHA" ] && break
    # Already past it: a later push landed while this one waited for the lock.
    if [ -n "$remote" ] && git -C "$REPO" merge-base --is-ancestor "$SHA" "$remote" 2>/dev/null; then
        echo "skipped: main has moved on to ${remote:0:7}, which gets its own build"
        exit 0
    fi
    sleep 5
done
[ "$remote" = "$SHA" ] || { echo "the push never reached GitHub (main is ${remote:0:7}); no edge build"; exit 1; }

W="$(mktemp -d)"
trap 'rm -rf "$W"' EXIT
git clone -q --no-checkout "$REPO" "$W/c" && git -C "$W/c" checkout -q "$SHA" \
    || { echo "could not make the build clone"; exit 1; }
for p in artifacts analysis/ramdisk cinder-aac/third_party; do
    [ -e "$REPO/$p" ] || { echo "missing $p in $REPO — the build needs it"; exit 1; }
    ln -s "$REPO/$p" "$W/c/$p"
done
ln -s "$CACHE/target" "$W/c/player/target"
# edge.sh checks the commit is on GitHub's main, so its origin must be GitHub, not this clone.
git -C "$W/c" remote set-url origin "$ORIGIN"

if ( cd "$W/c" && bash tools/edge.sh $DRY ); then
    echo "done: $(date)"
else
    echo "FAILED: $(date) — see above"
    exit 1
fi
