#!/usr/bin/env bash
# fetch_dev_payload.sh — put a release's developer payload into cinder-home/dist/dev and check it.
#
#   tools/fetch_dev_payload.sh v1.2.3
#
# The developer build is not committed (only its checksums are, in cinder-home/dist/PAYLOAD.sha256):
# tools/release.sh uploads it to the draft release `payload-<tag>` before pushing the tag. This
# downloads it, unpacks it, and runs the manifest check WITH the developer lines, so a payload that
# is not the one release.sh verified stops the build here. Run by every installer job in
# release.yml; needs `gh` with a token that can see drafts (GH_TOKEN).
set -euo pipefail
cd "$(dirname "$0")/.."
TAG="${1:?usage: tools/fetch_dev_payload.sh <tag>}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

gh release download "payload-$TAG" -p dev-payload.tar.gz -D "$WORK" \
    || { echo "::error::no developer payload for $TAG (the draft release payload-$TAG) — was it cut with tools/release.sh?"; exit 1; }
rm -rf cinder-home/dist/dev
tar -xzf "$WORK/dev-payload.tar.gz" -C cinder-home/dist
bash tools/verify_payload_manifest.sh --with-dev "$TAG"
