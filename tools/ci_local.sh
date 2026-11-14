#!/usr/bin/env bash
# ci_local.sh — run the `payload` workflow's build here, as a clean GitHub runner would, before pushing.
#
#   tools/ci_local.sh                 the committed tree (what a push would build)
#   tools/ci_local.sh --uncommitted   the working tree as it is, uncommitted changes and new files too
#   tools/ci_local.sh --fresh         forget the cached toolchains first (a runner's very first run)
#
# THE TRICK. The payload must be built at /home/sony/sony with HOME=/home/sony (its files carry those
# paths), on a machine with no firmware and none of the maintainer's toolchains, and that is this
# machine's real home. So this runs tools/ci_build_payload.sh in a private mount namespace: an empty
# folder is mounted over /home/sony for that one process tree, and nothing outside it sees the
# change. No Docker, no sudo; needs unprivileged user namespaces (on in WSL2 and most distros).
#
# The empty home is kept between runs (~/.cache/cinder-ci-home, like the workflow's cache), so the
# toolchains, the kernel tree and the downloads are fetched once. The packages are this machine's
# (SKIP_APT): Ubuntu 24.04 with the same versions the runner installs; the script prints them.
#
# Exit status is the comparison's: 0 when every payload file matches cinder-home/dist/PAYLOAD.sha256.
set -euo pipefail
REPO="$(cd "$(dirname "$0")/.." && pwd)"
FAKE="${CINDER_CI_HOME:-${XDG_CACHE_HOME:-$HOME/.cache}/cinder-ci-home}"
MODE=committed
for a in "$@"; do
    case "$a" in
        --uncommitted) MODE=uncommitted ;;
        --fresh) rm -rf "$FAKE" ;;
        *) sed -n '2,20p' "$0"; exit 2 ;;
    esac
done
mkdir -p "$FAKE"
SRC="$(mktemp -d)"
trap 'rm -rf "$SRC"' EXIT

# What the runner's checkout would hold: the commit, plus (on request) the working tree's changes.
git -C "$REPO" archive --format=tar HEAD | tar -x -C "$SRC"
if [ "$MODE" = uncommitted ]; then
    git -C "$REPO" diff HEAD --binary | (cd "$SRC" && git apply --allow-empty -)
    git -C "$REPO" ls-files --others --exclude-standard -z | (cd "$REPO" && xargs -0 -r cp --parents -t "$SRC")
    git -C "$REPO" ls-files --deleted -z | (cd "$SRC" && xargs -0 -r rm -f --)
fi
echo "── tree: $MODE ($(git -C "$REPO" rev-parse --short HEAD)); home: $FAKE ──"

unshare --user --map-root-user --mount bash -s "$FAKE" "$SRC" <<'EOF'
set -euo pipefail
mkdir -p /tmp/cinder-ci-src
mount --bind "$2" /tmp/cinder-ci-src     # first: /home/sony is about to disappear
mount --bind "$1" /home/sony
export HOME=/home/sony CARGO_HOME=/home/sony/.cargo RUSTUP_HOME=/home/sony/.rustup
# As on a runner, sbin included: wifi/build.sh checks each module with modinfo (/usr/sbin).
export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
# Root in here is only this user: tar must not try to hand files to the archive's owners.
export TAR_OPTIONS=--no-same-owner
rm -rf /home/sony/sony
cp -a /tmp/cinder-ci-src /home/sony/sony
cd /home/sony/sony
SKIP_APT=1 bash tools/ci_build_payload.sh
EOF
