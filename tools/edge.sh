#!/usr/bin/env bash
# edge.sh — publish an untested build of the latest commit as the rolling `edge` pre-release.
#
#   tools/edge.sh             build HEAD's payload here, upload it, have GitHub build the installers
#   tools/edge.sh --dry-run   build and pack the payload only; upload nothing, start nothing
#
# WHY HALF HERE AND HALF ON GITHUB. The player's binaries link against Sony's own libraries, so
# they can only be built on a maintainer's machine (cinder-home/build.sh); the installers can only
# be built for every platform on GitHub's runners (macOS needs Apple's toolchain). So this builds
# the payload exactly as tools/release.sh does, packs it with the commit it came from and a
# checksum list, uploads it to a DRAFT release (`edge-payload`, visible to maintainers only), and
# starts .github/workflows/edge.yml. That workflow checks out the same commit, unpacks the payload
# into it, builds the Windows, Linux and macOS installers as release.yml does, and replaces the
# downloads on the public `edge` pre-release.
#
# What it is not: a release. No version bump, no changelog fold, no tag a user's installer will
# ever see — the installer's update check reads releases/latest, which skips pre-releases.
#
# The committed dist/stable is put back afterwards, whatever happens: the build rewrites it in
# place, and a rebuilt dist/stable committed by accident is what turned CI red on 2026-10-22.
set -euo pipefail
cd "$(dirname "$0")/.."

DRY=
case "${1:-}" in
    "") ;;
    --dry-run) DRY=1 ;;
    *) echo "usage: tools/edge.sh [--dry-run]" >&2; exit 2 ;;
esac
die() { echo "edge: $*" >&2; exit 1; }

SHA="$(git rev-parse HEAD)"
SHORT="${SHA:0:7}"

# The workflow builds from a checkout of THIS commit, so the payload must come from exactly it
# (no uncommitted edits) and the commit must already be on GitHub. A dry run uploads nothing, so
# it builds whatever the tree holds.
if [ -z "$DRY" ]; then
    [ -z "$(git status --porcelain --untracked-files=no)" ] || die "uncommitted changes — commit or stash them first"
    git fetch -q origin main
    git merge-base --is-ancestor "$SHA" origin/main || die "$SHORT is not on origin/main — push it first"
fi

WORK="$(mktemp -d)"
LOG="$WORK/build.log"
restore() { git checkout -q -- cinder-home/dist/stable; }
trap 'restore; [ -n "$DRY" ] || rm -rf "$WORK"' EXIT

echo "edge: building the payload for $SHORT (log: $LOG) …"
# The same three steps tools/release.sh runs, in the same order.
bash cinder-home/build.sh stable >"$LOG" 2>&1 || { tail -20 "$LOG"; die "build failed"; }
for model in nw-a50 nw-wm1a; do
    bash cinder-home/tools/pack_upg.sh stable "$model" >>"$LOG" 2>&1 \
        || { tail -20 "$LOG"; die "pack_upg $model failed"; }
done
bash tools/check_arm_payload.sh cinder-home/dist/stable >>"$LOG" 2>&1 \
    || { tail -20 "$LOG"; die "the payload does not fit the player (check_arm_payload.sh)"; }

# Pack: the payload, the commit it came from, and a checksum list the workflow verifies.
mkdir "$WORK/p"
cp -a cinder-home/dist/stable "$WORK/p/stable"
echo "$SHA" >"$WORK/p/EDGE_COMMIT"
( cd "$WORK/p" && find stable -type f | LC_ALL=C sort | xargs sha256sum >SHA256SUMS )
tar -czf "$WORK/edge-payload.tar.gz" -C "$WORK/p" EDGE_COMMIT SHA256SUMS stable
restore
echo "edge: payload packed — $(du -h "$WORK/edge-payload.tar.gz" | cut -f1), $(wc -l <"$WORK/p/SHA256SUMS") files"

if [ -n "$DRY" ]; then
    echo "edge: dry run — nothing uploaded; the payload is at $WORK/edge-payload.tar.gz"
    exit 0
fi

# The transport: a draft release, so the public edge page only ever lists installers.
if ! gh release view edge-payload >/dev/null 2>&1; then
    gh release create edge-payload --draft --title "edge payload (for the edge workflow, not for download)" \
        --notes "Uploaded by tools/edge.sh and read by .github/workflows/edge.yml. Not for installing." >/dev/null
fi
gh release upload edge-payload "$WORK/edge-payload.tar.gz" --clobber
gh workflow run edge.yml -f sha="$SHA"
echo "edge: uploaded and started the edge workflow for $SHORT"
echo "      watch it:  gh run watch \$(gh run list --workflow edge.yml -L1 --json databaseId -q '.[0].databaseId')"
echo "      result:    https://github.com/superwilso/Cinder/releases/tag/edge"
