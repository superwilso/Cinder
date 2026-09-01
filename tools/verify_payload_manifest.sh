#!/usr/bin/env bash
# Verify that cinder-home/dist/stable is the payload tools/release.sh actually checked.
#
#   tools/verify_payload_manifest.sh v1.2.3    check the manifest describes this tree AND that tag
#   tools/verify_payload_manifest.sh           check the hashes only (no tag assertion)
#
# WHY THIS EXISTS. release.yml builds only the installer; the ARM payload it embeds is committed
# under cinder-home/dist/ because building it needs a glibc-2.23 cross toolchain not worth
# reproducing on a runner. tools/release.sh rebuilds that payload from source and refuses to tag
# unless every committed byte matches — real verification, and until now entirely OPT-IN, because
# `git tag && git push --tags` reaches the workflow directly. A stale dist/ then shipped as an
# installer full of last week's binaries, with a green tick and no warning.
# (docs/SHORTCOMINGS.md item 4 / D4: "real, correct, and opt-in".)
#
# The runner cannot repeat the comparison — no cross toolchain, by design. So release.sh records
# WHAT IT VERIFIED and this checks the record still describes the tree.
#
# AND IT IS A SCRIPT, not inlined YAML, for the reason this repo already learned the hard way when
# the linter gate was inlined: a check CI runs and a contributor cannot is a check that fails for
# the first time during a release. Run it locally; the workflow runs this exact file.
# (A line beginning "# shellcheck" would be read as a directive, hence the wording above.)
#
# NOT A SIGNATURE. Anyone who can push can rewrite the manifest. This closes the ACCIDENT — the
# forgotten step, the stale payload — which is the failure this project has actually had. Signing
# and provenance are D7 and are a different job.
set -euo pipefail
cd "$(dirname "$0")/.."

WANT_TAG="${1:-}"
M=cinder-home/dist/PAYLOAD.sha256

die() { printf '\033[1;31m✗ %s\033[0m\n' "$*" >&2; [ -n "${GITHUB_ACTIONS:-}" ] && echo "::error::$*"; exit 1; }
ok()  { printf '\033[1;32m✓\033[0m %s\n' "$*"; }

[ -f "$M" ] || die "$M is missing — this tree was not prepared with tools/release.sh${WANT_TAG:+ (run: tools/release.sh $WANT_TAG)}"

if [ -n "$WANT_TAG" ]; then
    have="$(awk '$1=="tag"{print $2}' "$M")"
    [ -n "$have" ] || die "manifest has no 'tag' line — it is malformed"
    [ "$have" = "$WANT_TAG" ] || die "manifest was verified for '$have' but this release is '$WANT_TAG' — the payload has not been re-verified (run: tools/release.sh $WANT_TAG)"
    ok "manifest was verified for $WANT_TAG"
fi

# A manifest with no hashes would otherwise "pass" — sha256sum -c on empty input succeeds.
n="$(grep -cE '^[0-9a-f]{64}  ' "$M" || true)"
[ "${n:-0}" -ge 1 ] || die "manifest lists no payload files — it is malformed"

grep -E '^[0-9a-f]{64}  ' "$M" | sha256sum -c --strict - \
    || die "a payload file does not match the manifest — dist/ changed after it was verified"
ok "all $n payload files match the manifest"
