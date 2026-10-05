#!/usr/bin/env bash
# The files in contracts/ are shared with Flint, byte for byte. Each repository's tests read its
# own copy; this is the check that the copies are the same. Skips when Flint is not checked out.
set -u
HERE="$(cd "$(dirname "$0")/.." && pwd)"; FLINT="${FLINT:-$HERE/../flint}"
[ -d "$FLINT/contracts" ] || { echo "skip: no Flint checkout at $FLINT"; exit 0; }
diff -r "$HERE/contracts" "$FLINT/contracts" && echo "contracts: identical"
