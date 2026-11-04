#!/usr/bin/env bash
# fetch_payload.sh — put the verified device payload into cinder-home/dist and check it.
#
#   tools/fetch_payload.sh            the release named in PAYLOAD.sha256 (public download, no token)
#   tools/fetch_payload.sh v1.2.3     the draft release payload-v1.2.3 (release.yml; needs GH_TOKEN)
#
# THE PAYLOAD IS NOT IN GIT. Only its checksums are, in cinder-home/dist/PAYLOAD.sha256. Its ARM
# binaries need Sony's libraries to build, so tools/release.sh builds them on a maintainer's machine
# and uploads cinder-payload.tar.gz (dist/stable and dist/dev) to the draft `payload-<tag>` before
# pushing the tag; release.yml attaches the same file to the published release. Everything that
# builds an installer from a checkout (ci.yml, release.yml, a fresh clone) runs this first.
#
# Whatever it downloads must match the manifest, or it stops. A tree whose payload already matches
# (a maintainer who just ran release.sh) is left alone, and nothing is downloaded.
set -euo pipefail
cd "$(dirname "$0")/.."
TAG="${1:-}"
M=cinder-home/dist/PAYLOAD.sha256
REPO="${GITHUB_REPOSITORY:-superwilso/Cinder}"
DEV=; grep -q '  cinder-home/dist/dev/' "$M" && DEV=--with-dev

if [ -z "$TAG" ] && bash tools/verify_payload_manifest.sh $DEV >/dev/null 2>&1; then
    echo "payload already matches the manifest — nothing to fetch"
    exit 0
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
if [ -n "$TAG" ]; then
    gh release download "payload-$TAG" -R "$REPO" -p cinder-payload.tar.gz -D "$WORK" \
        || { echo "::error::no payload for $TAG (the draft release payload-$TAG) — was it cut with tools/release.sh?"; exit 1; }
else
    FROM="$(awk '$1=="tag"{print $2}' "$M")"
    curl -fsSL --retry 3 -o "$WORK/cinder-payload.tar.gz" \
        "https://github.com/$REPO/releases/download/$FROM/cinder-payload.tar.gz" \
        || { echo "::error::could not download cinder-payload.tar.gz from release $FROM"; exit 1; }
fi
rm -rf cinder-home/dist/stable cinder-home/dist/dev
tar -xzf "$WORK/cinder-payload.tar.gz" -C cinder-home/dist
bash tools/verify_payload_manifest.sh $DEV ${TAG:+"$TAG"}
