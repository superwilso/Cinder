#!/usr/bin/env bash
# host_syntax_check.sh — compile-check the C/C++ that nothing else compiles.
#
# WHY THIS EXISTS. cinder-home/build.sh is the only thing that ever compiles the C++, it needs a
# glibc-2.23 + libc++-3.9.0 cross toolchain, and CI therefore skips the ARM side entirely — which
# leaves ~19,400 lines of C and C++ with NO automated gate of any kind. That is the code that runs
# as root, owns the boot path, and drives closed Sony services (docs/SHORTCOMINGS.md §A1).
#
# A full cross build on a runner is not worth the machinery. A SYNTAX CHECK is: it needs only a
# stock clang++/gcc, runs in seconds, and catches the whole class of error that currently reaches
# the device — a typo, a missing include, a wrong signature, an unbalanced brace.
#
# It is not a substitute for build.sh. It cannot check the ABI, the glibc ceiling, or anything that
# depends on the device's own headers. It answers exactly one question: does this parse?
#
# The first run of this script, 2026-08-23, found a real latent bug: probe.cpp used uintptr_t in
# seven places without including <cstdint>, and built only because the device toolchain pulled it in
# transitively.
#
#   tools/host_syntax_check.sh          check everything
#   tools/host_syntax_check.sh -v       show each command
set -uo pipefail
cd "$(dirname "$0")/.." || { echo "cannot reach the repo root" >&2; exit 2; }

VERBOSE=""
[ "${1:-}" = "-v" ] && VERBOSE=1

CXX="${CXX:-clang++-18}"
command -v "$CXX" >/dev/null 2>&1 || CXX=clang++
command -v "$CXX" >/dev/null 2>&1 || CXX=g++
CC="${CC:-gcc}"

INC=(-I cinder-home/src -I player/cinder-ffi/include -I cinder-audio/include -I cinder-audio/src
     -I ldac-bridge/include)

# CINDER_HOST_SYNTAX_ONLY drops the two static_asserts that state DEVICE layout facts (32-bit
# libc++ 3.9: vector 12 B, string 12 B). They cannot hold on a 64-bit libstdc++ host and they still
# fire on every real build, which is the only place their answer means anything.
# -Wall -Wextra -Werror. The C/C++ had NEVER been compiled with warnings enabled — the first run
# with them on, 2026-08-24, produced six across 19,435 lines, which is remarkably clean. All six
# were fixed rather than tolerated (a dead popen() helper, a dead locked wrapper, a captured-and-
# discarded value, a DEV-only helper that warned in stable builds, an unused parameter, a
# _GNU_SOURCE redefinition), so the tree can be held at zero from here.
#
# -Werror is the point. "I looked and it was clean" is worth one afternoon; a gate is worth every
# afternoon after it — the same lesson as the six self-tests that existed for months and ran when
# somebody remembered.
CXXFLAGS=(-fsyntax-only -std=c++14 -Wall -Wextra -Werror -DCINDER_HOST_SYNTAX_ONLY)
CFLAGS=(-fsyntax-only -std=gnu99 -D_GNU_SOURCE -Wall -Wextra -Werror)

fails=0
checked=0

check() { # check <compiler> <label> <file> <flags...>
    local comp="$1" label="$2" file="$3"; shift 3
    [ -f "$file" ] || return 0
    checked=$((checked + 1))
    [ -n "$VERBOSE" ] && printf '    %s %s %s\n' "$comp" "$*" "$file"
    local err
    if err=$("$comp" "$@" "$file" 2>&1); then
        printf '  ok    %s\n' "$label"
    else
        printf '  FAIL  %s\n' "$label"
        printf '%s\n' "$err" | head -20 | sed 's/^/          /'
        fails=$((fails + 1))
    fi
}

echo "── host syntax check ───────────────────────────────────────────────"
echo "   $($CXX --version | head -1)"
echo

echo "cinder-home (the shell, the probe):"
check "$CXX" "src/main.cpp"     cinder-home/src/main.cpp     "${CXXFLAGS[@]}" "${INC[@]}"
# …and again as the DEV channel builds it. take_req and the discovery dump only exist there, so a
# warning (or an error) in that half would otherwise ship unseen.
check "$CXX" "src/main.cpp [dev]" cinder-home/src/main.cpp "${CXXFLAGS[@]}" -DCINDER_DEV=1 "${INC[@]}"
check "$CXX" "src/probe.cpp"    cinder-home/src/probe.cpp    "${CXXFLAGS[@]}" "${INC[@]}"
check "$CXX" "src/discover.cpp" cinder-home/src/discover.cpp "${CXXFLAGS[@]}" "${INC[@]}"

echo
echo "cinder-audio (the Sony IPC shims — no tests, see SHORTCOMINGS.md §A2):"
for f in cinder-audio/src/*.cpp; do
    check "$CXX" "${f#cinder-audio/}" "$f" "${CXXFLAGS[@]}" "${INC[@]}"
done

echo
echo "setuid helpers and C sources:"
for f in cinder-home/src/*.c ldac-bridge/src/*.c; do
    [ -f "$f" ] || continue
    # glibc223_compat.c is EXCLUDED, and not because it is broken. It calls __xstat(_STAT_VER, …),
    # and _STAT_VER was removed from glibc's headers in 2.33 — the file exists precisely to target
    # the device's glibc 2.23, so it can only compile against that sysroot. Checking it here would
    # report a permanent, meaningless failure.
    case "$f" in *glibc223_compat.c) continue;; esac
    check "$CC" "${f#cinder-home/}" "$f" "${CFLAGS[@]}" -I cinder-home/src -I ldac-bridge/include
done

for f in ldac-bridge/src/*.cpp; do
    [ -f "$f" ] || continue
    check "$CXX" "${f#ldac-bridge/}" "$f" "${CXXFLAGS[@]}" "${INC[@]}"
done

echo
if [ "$fails" -eq 0 ]; then
    echo "── $checked file(s) checked, all parse ─────────────────────────────"
else
    echo "── $checked file(s) checked, $fails FAILED ─────────────────────────"
fi
exit $((fails > 0))
