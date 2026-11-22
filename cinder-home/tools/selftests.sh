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
# Every (test, compiler) pair builds and runs at once, then the results print in the usual order:
# 70 s one after another on a 12-core host (11-22), the biggest share of a no-change build.
one() {   # one SRC COMPILER: build and run, result line (and any log) in $out/T.C.res
    local t="${1%_selftest.cpp}" c="$2" r="$out/${1%_selftest.cpp}.$2.res"
    # cinder-audio/include: effectshim_selftest compiles the real shim, which includes its header.
    if ! "$c" -O2 -I ../../cinder-audio/include -o "$out/$t.$c" "$1" -lstdc++ -lm >"$r" 2>&1; then
        echo "FAIL: $t did not build ($c)" >>"$r"
    elif "$out/$t.$c" >"$r" 2>&1; then echo "ok    $t ($c)" >"$r"
    else echo "FAIL: $t ($c)" >>"$r"; fi
}
for src in *_selftest.cpp; do
    for c in "${compilers[@]}"; do one "$src" "$c" & done
done
wait
for src in *_selftest.cpp; do
    for c in "${compilers[@]}"; do
        r="$out/${src%_selftest.cpp}.$c.res"
        cat "$r"
        tail -1 "$r" | grep -q '^ok ' || fail=1
    done
done
exit "$fail"
