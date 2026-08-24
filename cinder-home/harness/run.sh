#!/usr/bin/env bash
# run.sh — build and run the off-device harness.
#
# The harness links the REAL cinder-home/src/main.cpp against fake Sony services and a virtual
# clock, boots it, and asserts on the resulting call trace. It is the only place outside the
# Walkman itself where the app's bring-up sequence and its polling behaviour can be observed, and
# it needs no device, no cross-compiler and no network.
#
#   ./cinder-home/harness/run.sh              # every scenario
#   ./cinder-home/harness/run.sh boot         # one scenario
#   CINDER_HARNESS_TRACE=1 ./…/run.sh boot    # …and dump the full trace if it fails
#
# NOT a substitute for a device session. The fakes answer the way the RE notes say the services
# answer; where the notes are wrong the harness is confidently wrong with them. What it proves is
# that the app does the right thing GIVEN those answers — which is exactly the half that kept
# breaking.
set -u

cd "$(dirname "$0")/../.." || exit 1
OUT="${CINDER_HARNESS_OUT:-.harness}"
mkdir -p "$OUT" || exit 1

CXX="${CXX:-}"
if [ -z "$CXX" ]; then
    for c in clang++-18 clang++ g++; do command -v "$c" >/dev/null 2>&1 && { CXX="$c"; break; }; done
fi
[ -n "$CXX" ] || { echo "harness: no C++ compiler found" >&2; exit 1; }

INC=(-I "$OUT" -I cinder-home/harness -I cinder-home/src -I player/cinder-ffi/include
     -I cinder-audio/include -I cinder-audio/src -I ldac-bridge/include)
# CINDER_HOST_SYNTAX_ONLY drops the two static_asserts that state 32-bit DEVICE struct layouts;
# they cannot hold on a 64-bit host and still fire on every real build. -Dmain= renames main.cpp's
# entry point so the harness can call it (and so the harness can own `main`).
# No -g by default: debug info roughly doubles the compile time of a 7,900-line main.cpp for a run
# whose output is a call trace, not a debugger session. CINDER_HARNESS_DEBUG=1 turns it back on.
CXXFLAGS=(-std=c++14 -O0 -DCINDER_HOST_SYNTAX_ONLY)
[ -n "${CINDER_HARNESS_DEBUG:-}" ] && CXXFLAGS+=(-g)

# The harness's own code is held to the same -Werror standard as the app it checks. Two exemptions,
# both structural rather than laziness:
#   -Wno-unused-private-field  easel_abi.hpp reserves storage the DEVICE ctor writes into and we
#                              must never touch. Unused is the whole point of it.
#   -Wno-unused-parameter      the generated stubs keep the real headers' parameter lists so a stub
#                              cannot drift from what it replaces, and ignore most of them.
WARN=(-Wall -Wextra -Werror)

echo "harness: $($CXX --version | head -1)"

# Regenerate both tables every run, from the sources they describe: a header that grows a function
# the app calls must not be able to leave a stale stub behind, and a vtable slot index that is
# corrected in main.cpp must not leave the harness naming the old method.
python3 cinder-home/harness/gen_stubs.py . > "$OUT/stubs.cpp" || exit 1
python3 cinder-home/harness/gen_slotmap.py cinder-home/src/main.cpp > "$OUT/slotmap.h" || exit 1

build() { # build <out.o> <src> [extra flags...]
    local o="$1" src="$2"; shift 2
    "$CXX" -c "${CXXFLAGS[@]}" "$@" "${INC[@]}" -o "$o" "$src" || {
        echo "harness: FAILED to compile $src" >&2; exit 1; }
}

# main.cpp's warnings are already gated by tools/host_syntax_check.sh, which compiles it with
# -Werror in both channels; repeating that here would only slow the harness down.
build "$OUT/main.o"       cinder-home/src/main.cpp -Dmain=cinder_app_main
build "$OUT/harness.o"    cinder-home/harness/harness.cpp   "${WARN[@]}"
build "$OUT/fake_easel.o" cinder-home/harness/fake_easel.cpp "${WARN[@]}" -Wno-unused-private-field
build "$OUT/fake_pst.o"   cinder-home/harness/fake_pst.cpp  "${WARN[@]}"
build "$OUT/fakefs.o"     cinder-home/harness/fakefs.cpp    "${WARN[@]}"
build "$OUT/fakeinput.o"  cinder-home/harness/fakeinput.cpp "${WARN[@]}"
build "$OUT/scenarios.o"  cinder-home/harness/scenarios.cpp "${WARN[@]}"
build "$OUT/stubs.o"      "$OUT/stubs.cpp"                  "${WARN[@]}" -Wno-unused-parameter

# An explicit list, not "$OUT"/*.o: a stray object left in the build directory (an exploratory
# scenario with its own main, say) would otherwise break the link with a duplicate-symbol error
# that says nothing about what is wrong.
"$CXX" -o "$OUT/harness" \
    "$OUT/main.o" "$OUT/harness.o" "$OUT/fake_easel.o" "$OUT/fake_pst.o" \
    "$OUT/fakefs.o" "$OUT/fakeinput.o" \
    "$OUT/scenarios.o" "$OUT/stubs.o" -lpthread || {
    echo "harness: FAILED to link" >&2; exit 1; }

if [ $# -gt 0 ]; then
    scenarios="$*"
else
    scenarios="$("$OUT/harness" list)"
fi

fails=0
for s in $scenarios; do
    # Each scenario boots the app, and main.cpp's bring-up is one-shot static state — so each
    # scenario gets its own process, not just its own reset.
    "$OUT/harness" "$s" || fails=$((fails + 1))
    echo
done

if [ "$fails" -ne 0 ]; then
    echo "harness: $fails scenario(s) FAILED"
    exit 1
fi
echo "harness: all scenarios passed"
