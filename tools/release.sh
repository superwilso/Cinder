#!/usr/bin/env bash
# Cut a release: verify, tag, push. The tag is what makes GitHub build and publish the installers.
#
# WHY A SCRIPT AND NOT "git tag && git push". The release workflow builds ONLY the installer. The
# ARM payload it embeds — cinder-home, cinder-probe, the setuid helpers, the .UPG — is committed
# under cinder-home/dist/, because building it needs a glibc-2.23 + libc++-3.9.0 cross toolchain
# matched to the player's runtime, which is not worth reproducing on a hosted runner.
#
# That split has one failure mode, and it is silent: tag a commit whose dist/ is STALE and the
# release ships an installer full of last week's binaries, with a green tick and no warning. This
# script exists to make that impossible — it rebuilds from source and refuses to tag unless every
# committed payload byte matches what the current tree produces.
#
#   tools/release.sh v1.2.3            verify, then tag and push
#   tools/release.sh v1.2.3 --dry-run  verify only, touch nothing
#
# It never commits. If something is stale or dirty it tells you what to do and stops — staging and
# committing stays yours.

set -euo pipefail
cd "$(dirname "$0")/.."

TAG="${1:-}"
DRY=""
[ "${2:-}" = "--dry-run" ] && DRY=1

die() { printf '\n\033[1;31m✗ %s\033[0m\n' "$*" >&2; exit 1; }
ok()  { printf '\033[1;32m✓\033[0m %s\n' "$*"; }
note(){ printf '  %s\n' "$*"; }

[ -n "$TAG" ] || die "usage: tools/release.sh vX.Y.Z [--dry-run]"
[[ "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] \
    || die "tag must look like v1.2.3 or v1.2.3-rc1 — the workflow only triggers on 'v*'"

# ── 1. the tree must be committed ───────────────────────────────────────────────────────────
# A tag points at a commit, so anything uncommitted is simply not in the release. Better to stop
# than to ship a tag that does not describe what was tested.
if [ -n "$(git status --porcelain)" ]; then
    git status --short | sed 's/^/    /'
    die "working tree is dirty — commit (or stash) before tagging"
fi
ok "working tree clean"

git rev-parse "$TAG" >/dev/null 2>&1 && die "$TAG already exists — pick a new version"

# ── 2. the installer's version must match the tag ───────────────────────────────────────────
WANT="${TAG#v}"; WANT="${WANT%%-*}"
HAVE="$(grep -m1 '^version' installer/Cargo.toml | cut -d'"' -f2)"
[ "$HAVE" = "$WANT" ] || die "installer/Cargo.toml says version = \"$HAVE\" but the tag is $TAG.
    Fix one of them and commit:  sed -i 's/^version = .*/version = \"$WANT\"/' installer/Cargo.toml"
ok "installer version $HAVE matches $TAG"

# ── 3. the committed payload must match a fresh build ───────────────────────────────────────
# This is the whole point of the script. Rebuild the stable channel and compare byte-for-byte.
note "rebuilding the stable channel to check dist/ is current …"
BEFORE="$(find cinder-home/dist/stable -type f -exec md5sum {} + | sort -k2)"
bash cinder-home/build.sh stable >/tmp/cinder-release-build.log 2>&1 \
    || { tail -20 /tmp/cinder-release-build.log; die "build failed — see /tmp/cinder-release-build.log"; }
AFTER="$(find cinder-home/dist/stable -type f -exec md5sum {} + | sort -k2)"

if [ "$BEFORE" != "$AFTER" ]; then
    diff <(echo "$BEFORE") <(echo "$AFTER") | sed 's/^/    /' || true
    die "dist/stable is STALE — the build just changed it.
    Commit the rebuilt payload, then re-run:  git add cinder-home/dist && git commit"
fi
ok "dist/stable matches a fresh build — the payload in the release will be the real one"

# ── 4. every file the installer embeds must exist ───────────────────────────────────────────
# build.rs fails loudly on a missing payload, but failing HERE names the file and costs no CI run.
MISSING=0
while read -r f; do
    [ -f "$f" ] || { echo "    missing: $f"; MISSING=1; }
done <<'PAYLOAD'
cinder-home/dist/stable/cinder-home
cinder-home/dist/stable/cinder-probe
cinder-home/dist/stable/cinder-umount
cinder-home/dist/stable/cinder-power
cinder-home/dist/stable/cinder-msc
cinder-home/dist/stable/cinder-clock
cinder-home/dist/stable/cinder-signature.sh
cinder-home/dist/stable/cinder_components.conf
cinder-home/dist/stable/cinder_home_install.upg
cinder-home/dist/stable/cinder_home_uninstall.upg
PAYLOAD
[ "$MISSING" = 0 ] || die "payload incomplete — the installer would not build"
ok "all payload files present"

# ── 5. the installer itself must build and pass its tests ──────────────────────────────────
note "building + testing the installer locally …"
( cd installer && cargo test --quiet && cargo build --release --quiet ) \
    || die "installer build/test failed — fix before tagging"
ok "installer builds and tests pass"

if [ -n "$DRY" ]; then
    printf '\n\033[1;33m--dry-run: everything checks out, nothing was tagged.\033[0m\n'
    exit 0
fi

# ── 6. tag and push ────────────────────────────────────────────────────────────────────────
git tag -a "$TAG" -m "Cinder $TAG"
ok "tagged $TAG"
git push origin "$TAG"
ok "pushed $TAG"

REPO="$(git remote get-url origin | sed -e 's#.*github.com[:/]##' -e 's#\.git$##')"
printf '\nGitHub is now building the installers. Watch it:\n'
printf '    https://github.com/%s/actions\n' "$REPO"
printf 'The release appears at:\n'
printf '    https://github.com/%s/releases/tag/%s\n\n' "$REPO" "$TAG"
