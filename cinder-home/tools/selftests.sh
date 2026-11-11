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
# With clang as well, where there is one: the player is built with clang, and a test that passes
# under gcc alone has not tested what ships. soundscape.h drew two random numbers inside one call,
# gcc and clang worked the arguments out in opposite orders, and the fire the tests measured was
# not the fire the player made (2026-11-11).
compilers=(cc)
command -v clang++ >/dev/null 2>&1 && compilers+=(clang++)
for src in *_selftest.cpp; do
    t="${src%_selftest.cpp}"
    for c in "${compilers[@]}"; do
        # cinder-audio/include: effectshim_selftest compiles the real shim, which includes its header.
        if ! "$c" -O2 -I ../../cinder-audio/include -o "$out/$t" "$src" -lstdc++ -lm 2>"$out/$t.log"; then
            cat "$out/$t.log"; echo "FAIL: $t did not build ($c)"; fail=1; continue
        fi
        if "$out/$t" >"$out/$t.log" 2>&1; then echo "ok    $t ($c)"; else cat "$out/$t.log"; echo "FAIL: $t ($c)"; fail=1; fi
    done
done
exit "$fail"
