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
[ "$HAVE" = "$WANT" ] || {
    printf '\ninstaller/Cargo.toml says version = "%s" but the tag is %s.\n' "$HAVE" "$TAG"
    if [ -t 0 ]; then
        read -r -p "Update the installer version to $WANT now? [y/N] " ANSWER
    else
        ANSWER=""
    fi
    case "$ANSWER" in
        y|Y|yes|YES)
            sed -i "0,/^version = \"$HAVE\"$/s//version = \"$WANT\"/" installer/Cargo.toml
            sed -i "/^name = \"cinder-installer\"$/,/^version = / s/^version = \".*\"$/version = \"$WANT\"/" installer/Cargo.lock
            ok "updated installer version to $WANT in Cargo.toml and Cargo.lock"
            die "version updated — review, commit, then rerun: tools/release.sh $TAG"
            ;;
        *)
            die "version unchanged — update it manually or rerun and answer yes"
            ;;
    esac
}
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

# ── 4b. write the payload MANIFEST, so the runner can check what this script checked ────────
#
# THE HOLE THIS CLOSES (docs/SHORTCOMINGS.md item 4, D4). Everything above is real verification and
# none of it is REQUIRED: `git tag v1.2.3 && git push --tags` triggers release.yml directly, and
# that workflow only checks the payload files EXIST. So the one guard standing between a stale
# dist/ and a published installer full of last week's binaries was a script you had to remember to
# use. "The release integrity guard is real, correct, and opt-in."
#
# The runner cannot re-run the check itself — the byte-for-byte comparison needs the glibc-2.23
# cross toolchain, which is the whole reason dist/ is committed in the first place. So instead this
# records WHAT WAS VERIFIED, and the runner checks the record still describes the tree:
#
#   * the sha256 of every payload file, so a payload edited after verification fails;
#   * the TAG it was verified for, so a later `git tag` that skipped this script finds a manifest
#     naming the previous version and fails;
#   * the commit, for the log.
#
# It is not a signature and does not pretend to be — anyone who can push can also rewrite the
# manifest. It closes the ACCIDENT (a forgotten step, a stale payload), which is the failure this
# project has actually had, and it makes the deliberate bypass an explicit act rather than a
# default. Provenance proper is D7 and needs signing.
MANIFEST=cinder-home/dist/PAYLOAD.sha256
note "writing the payload manifest …"
{
    echo "# Cinder release payload manifest."
    echo "# Written by tools/release.sh AFTER a byte-for-byte rebuild check; verified by"
    echo "# .github/workflows/release.yml before anything is published. Do not edit by hand —"
    echo "# a mismatch here is the runner telling you the payload is not what was verified."
    echo "tag $TAG"
    echo "commit $(git rev-parse HEAD)"
    while read -r f; do
        [ -n "$f" ] || continue
        sha256sum "$f"
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
} > "$MANIFEST.tmp"
mv "$MANIFEST.tmp" "$MANIFEST"

# The tag must point at a commit that CONTAINS this manifest, so if writing it changed anything,
# stop and ask for a commit — the same shape as the version bump above. On a re-run after that
# commit the file is identical, git is clean, and this falls through.
if [ -n "$(git status --porcelain "$MANIFEST")" ]; then
    git --no-pager diff --stat "$MANIFEST" | sed 's/^/    /'
    die "payload manifest updated for $TAG — commit it, then re-run:
    git add $MANIFEST && git commit -m 'release: payload manifest for $TAG'
    tools/release.sh $TAG"
fi
ok "payload manifest committed and current for $TAG"

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
