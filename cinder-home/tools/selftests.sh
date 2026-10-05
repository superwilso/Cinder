#!/usr/bin/env bash
# selftests.sh — build and run every cinder-home/tools/*_selftest.cpp with the host compiler.
#
# Globbed, not listed. CI, build.sh and .githooks/pre-push each carried their own copy of the list,
# and the copies had drifted to 13, 12 and 11 entries. A new self-test is picked up by dropping the
# file in; there is nothing to keep equal.
set -u
cd "$(dirname "$0")" || exit 2
command -v cc >/dev/null 2>&1 || { echo "(skip: no host cc)"; exit 0; }
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
fail=0
for src in *_selftest.cpp; do
    t="${src%_selftest.cpp}"
    if ! cc -O2 -o "$out/$t" "$src" -lstdc++ -lm 2>"$out/$t.log"; then
        cat "$out/$t.log"; echo "FAIL: $t did not build"; fail=1; continue
    fi
    if "$out/$t" >"$out/$t.log" 2>&1; then echo "ok    $t"; else cat "$out/$t.log"; echo "FAIL: $t"; fail=1; fi
done
exit "$fail"
