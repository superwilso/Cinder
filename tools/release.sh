#!/usr/bin/env bash
# Cut a release: prepare everything in ONE pass, then tag and push.
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
#   tools/release.sh v1.2.3            prepare, or (once that is committed) tag and push
#   tools/release.sh v1.2.3 --dry-run  say what would change; edit nothing, tag nothing
#
# --dry-run makes no edits and creates no tag. It does still REBUILD cinder-home/dist/stable,
# because rebuilding is how "is the committed payload stale?" is answered at all — there is no
# way to check that without producing the bytes to compare. Nothing else is written.
#
# ── TWO RUNS, ONE COMMIT ────────────────────────────────────────────────────────────────────────
# It used to take three. Each of the version bump, the payload rebuild and the manifest was its own
# "stop, commit, re-run" — so cutting v0.1.6 cost three commits (3b67cd4, 788099b, aefbca6), and
# each stop re-ran the multi-minute cross build to discover the next thing that also needed
# committing. Now PREPARE does the whole job in one pass:
#
#     run 1   bump the version, rebuild the payload, roll the changelog, write the manifest
#             → review the diff, `git add -A && git commit` (the script prints the command)
#     run 2   everything already matches → verify, tag, push
#
# It still never commits. Staging and committing stays yours — the diff is the last point at which
# a human looks at what is about to ship, and that is worth keeping.

set -euo pipefail
cd "$(dirname "$0")/.."

TAG="${1:-}"
DRY=""
[ "${2:-}" = "--dry-run" ] && DRY=1

die() { printf '\n\033[1;31m✗ %s\033[0m\n' "$*" >&2; exit 1; }
ok()  { printf '\033[1;32m✓\033[0m %s\n' "$*"; }
note(){ printf '  %s\n' "$*"; }
act() { printf '\033[1;33m~\033[0m %s\n' "$*"; }
# Join "${CHANGED[@]}" with ", ". `IFS=', '` looks like it does this and does not: parameter
# expansion uses only the FIRST character of IFS, so the summary came out comma-jammed.
joined() { local out="" x; for x in "$@"; do out="${out:+$out, }$x"; done; printf '%s' "$out"; }

[ -n "$TAG" ] || die "usage: tools/release.sh vX.Y.Z [--dry-run]"
[[ "$TAG" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] \
    || die "tag must look like v1.2.3 or v1.2.3-rc1 — the workflow only triggers on 'v*'"

VER="${TAG#v}"; VER="${VER%%-*}"

git rev-parse "$TAG" >/dev/null 2>&1 && die "$TAG already exists — pick a new version"

# What this run changed, so the summary at the end can name it. A release that needed nothing
# changed is the second run, and that is the one that tags.
CHANGED=()

# The tree may be dirty going IN — that is the normal state when you have just written the
# changelog entry for the release you are cutting. What matters is that it is clean when we tag,
# which is checked at the end, after everything below has had its say.
DIRTY_AT_START=""
[ -n "$(git status --porcelain)" ] && DIRTY_AT_START=1

# ── 1. the installer's version must match the tag ───────────────────────────────────────────
HAVE="$(grep -m1 '^version' installer/Cargo.toml | cut -d'"' -f2)"
if [ "$HAVE" != "$VER" ]; then
    if [ -z "$DRY" ]; then
        sed -i "0,/^version = \"$HAVE\"$/s//version = \"$VER\"/" installer/Cargo.toml
        sed -i "/^name = \"cinder-installer\"$/,/^version = / s/^version = \".*\"$/version = \"$VER\"/" installer/Cargo.lock
    fi
    act "installer version $HAVE → $VER (Cargo.toml + Cargo.lock)${DRY:+  [would]}"
    CHANGED+=("installer version")
else
    ok "installer version $HAVE matches $TAG"
fi

# ── 2. roll the changelog's [Unreleased] section into this version ──────────────────────────
# The release body links CHANGELOG.md for "the curated entry for this version", so shipping a tag
# whose notes are still filed under [Unreleased] means the link lands on nothing. Rolling it is
# mechanical — a heading, a date and two link definitions — which is exactly the kind of step that
# gets forgotten at the end of a release rather than the kind worth doing by hand.
#
# Only ever rolled ONCE: on the second run the [X.Y.Z] heading already exists and this is a no-op,
# the same way the version bump above is.
CL=CHANGELOG.md
if grep -q "^## \[$VER\]" "$CL"; then
    ok "changelog already has a [$VER] section"
elif ! grep -q '^## \[Unreleased\]' "$CL"; then
    note "changelog has no [Unreleased] section — leaving it alone"
else
    # Anything between [Unreleased] and the next version heading. Whitespace-only means there is
    # nothing to release under this version, which is worth stopping for rather than cutting a
    # release whose notes are an empty heading.
    BODY="$(awk '/^## \[Unreleased\]/{f=1;next} /^## \[/{f=0} f' "$CL" | tr -d '[:space:]')"
    [ -n "$BODY" ] || die "CHANGELOG.md's [Unreleased] section is empty — write the entry for $TAG first"

    # sed, not awk's 3-argument match(): that form is a GNU extension, and this script is the one
    # a contributor runs on whatever awk their machine has.
    PREV="$(sed -n 's/^## \[\([0-9]\+\.[0-9]\+\.[0-9]\+\)\].*/\1/p' "$CL" | head -1)"
    # The compare-URL base, read from the existing link definitions rather than hard-coded: this
    # file is the only place the repo URL appears and a fork should not have to edit the script.
    BASE="$(awk -F': ' '/^\[Unreleased\]: /{print $2; exit}' "$CL" | sed 's#/compare/.*##;s#/releases/tag/.*##')"
    TODAY="$(date +%F)"

    # New empty [Unreleased] above the version being cut, so the next change has somewhere to go.
    awk -v ver="$VER" -v today="$TODAY" '
        /^## \[Unreleased\]/ { print "## [Unreleased]"; print ""; print "## [" ver "] — " today; next }
        { print }
    ' "$CL" > "$CL.tmp"

    # Link definitions: [Unreleased] now compares against THIS tag, and this tag gets its own line.
    if [ -n "$BASE" ] && [ -n "$PREV" ]; then
        awk -v ver="$VER" -v prev="$PREV" -v base="$BASE" -v tag="$TAG" '
            /^\[Unreleased\]: / {
                print "[Unreleased]: " base "/compare/" tag "...HEAD"
                print "[" ver "]: " base "/compare/v" prev "..." tag
                next
            }
            { print }
        ' "$CL.tmp" > "$CL.tmp2" && mv "$CL.tmp2" "$CL.tmp"
    fi
    if [ -n "$DRY" ]; then rm -f "$CL.tmp"; else mv "$CL.tmp" "$CL"; fi
    act "changelog: [Unreleased] → [$VER] — $TODAY (and a fresh [Unreleased] above it)${DRY:+  [would]}"
    CHANGED+=("changelog")
fi

# ── 3. rebuild the payload, so what ships is what the tree produces ─────────────────────────
# This is the whole point of the script. The build writes into dist/stable in place; if that
# changes anything, the committed payload was stale and the rebuilt one is now staged for the
# single commit at the end — rather than stopping to demand a commit of its own.
note "rebuilding the stable channel …"
BEFORE="$(find cinder-home/dist/stable -type f -exec md5sum {} + | sort -k2)"
bash cinder-home/build.sh stable >/tmp/cinder-release-build.log 2>&1 \
    || { tail -20 /tmp/cinder-release-build.log; die "build failed — see /tmp/cinder-release-build.log"; }
AFTER="$(find cinder-home/dist/stable -type f -exec md5sum {} + | sort -k2)"

if [ "$BEFORE" != "$AFTER" ]; then
    diff <(echo "$BEFORE") <(echo "$AFTER") | sed 's/^/    /' || true
    act "dist/stable was STALE — rebuilt from source (the files above changed)"
    CHANGED+=("payload binaries")
else
    ok "dist/stable matches a fresh build"
fi

# ── 4. every file the installer embeds must exist ───────────────────────────────────────────
# build.rs fails loudly on a missing payload, but failing HERE names the file and costs no CI run.
# One list, used by both the existence check and the manifest below — they drifted apart as two
# copies of the same ten lines, which is one edit away from a manifest that verifies nine files.
PAYLOAD_FILES=(
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
)
MISSING=0
for f in "${PAYLOAD_FILES[@]}"; do
    [ -f "$f" ] || { echo "    missing: $f"; MISSING=1; }
done
[ "$MISSING" = 0 ] || die "payload incomplete — the installer would not build"
ok "all payload files present"

# ── 5. write the payload MANIFEST, so the runner can check what this script checked ─────────
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
#     naming the previous version and fails.
#
# NO COMMIT LINE. It used to record `commit $(git rev-parse HEAD)`, and that could never converge:
# the manifest is written BEFORE the commit that contains it, so run 1 recorded the parent, you
# committed, and run 2 recomputed the line against the new HEAD — a fresh diff, another demand to
# commit, for ever. v0.1.6 sat in that loop. The hashes are the provenance that matters; the commit
# is `git log -1 -- cinder-home/dist/PAYLOAD.sha256` for anyone who wants it, and unlike a
# self-referential field it is actually true.
#
# It is not a signature and does not pretend to be — anyone who can push can also rewrite the
# manifest. It closes the ACCIDENT (a forgotten step, a stale payload), which is the failure this
# project has actually had, and it makes the deliberate bypass an explicit act rather than a
# default. Provenance proper is D7 and needs signing.
MANIFEST=cinder-home/dist/PAYLOAD.sha256
{
    echo "# Cinder release payload manifest."
    echo "# Written by tools/release.sh AFTER a byte-for-byte rebuild check; verified by"
    echo "# .github/workflows/release.yml before anything is published. Do not edit by hand —"
    echo "# a mismatch here is the runner telling you the payload is not what was verified."
    echo "tag $TAG"
    sha256sum "${PAYLOAD_FILES[@]}"
} > "$MANIFEST.tmp"
if cmp -s "$MANIFEST.tmp" "$MANIFEST"; then
    rm -f "$MANIFEST.tmp"
    ok "payload manifest current for $TAG"
elif [ -n "$DRY" ]; then
    rm -f "$MANIFEST.tmp"
    act "payload manifest written for $TAG  [would]"
    CHANGED+=("payload manifest")
else
    mv "$MANIFEST.tmp" "$MANIFEST"
    act "payload manifest written for $TAG"
    CHANGED+=("payload manifest")
fi

# ── 6. the installer itself must build and pass its tests ──────────────────────────────────
note "building + testing the installer locally …"
( cd installer && cargo test --quiet && cargo build --release --quiet ) \
    || die "installer build/test failed — fix before tagging"
ok "installer builds and tests pass"

# ── 7. stop and hand the diff over, or tag ─────────────────────────────────────────────────
if [ -n "$DRY" ]; then
    if [ ${#CHANGED[@]} -gt 0 ]; then
        printf '\n\033[1;33m--dry-run: preparing %s would change: %s.\033[0m\n' \
               "$TAG" "$(joined "${CHANGED[@]}")"
        printf 'Nothing was edited and nothing was tagged. Re-run without --dry-run to do it.\n\n'
    else
        printf '\n\033[1;33m--dry-run: everything is already prepared for %s. ' "$TAG"
        printf 'Nothing was tagged.\033[0m\n\n'
    fi
    exit 0
fi

# A tag points at a commit, so anything uncommitted is simply not in the release. This is the ONE
# place that check belongs — after every change this script makes, so it is asked once and answered
# with a single commit, rather than three times with three.
if [ -n "$(git status --porcelain)" ]; then
    printf '\n'
    if [ ${#CHANGED[@]} -gt 0 ]; then
        ok "prepared $TAG: $(joined "${CHANGED[@]}")"
    else
        note "the tree was already dirty before this run — nothing here changed it"
    fi
    printf '\n\033[1mReview, then commit — everything %s needs is in this one diff:\033[0m\n\n' "$TAG"
    git status --short | sed 's/^/    /'
    printf '\n    git add -A && git commit -m "release: prepare %s"\n' "$TAG"
    printf '    tools/release.sh %s\n\n' "$TAG"
    [ -n "$DIRTY_AT_START" ] && printf '  (the tree already had uncommitted work when this started — check the list above\n   is only what you meant to ship.)\n\n'
    exit 0
fi
ok "working tree clean — everything for $TAG is committed"

# ── 8. tag and push ────────────────────────────────────────────────────────────────────────
git tag -a "$TAG" -m "Cinder $TAG"
ok "tagged $TAG"
git push origin "$TAG"
ok "pushed $TAG"

REPO="$(git remote get-url origin | sed -e 's#.*github.com[:/]##' -e 's#\.git$##')"
printf '\nGitHub is now building the installers. Watch it:\n'
printf '    https://github.com/%s/actions\n' "$REPO"
printf 'The release appears at:\n'
printf '    https://github.com/%s/releases/tag/%s\n\n' "$REPO" "$TAG"
