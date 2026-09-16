#!/usr/bin/env bash
# test_mono_shim.sh — build libcinder_mono.so and drive it through LD_PRELOAD.
#
#   host:   always — proves the interposition and the pass-throughs with the host's glibc.
#   device: when the cross toolchain and the device libraries are present (cinder-home/build.sh
#           PREREQUISITES) — builds against glibc 2.23 and runs the same test under qemu-arm with
#           the player's own libc, which is what catches a symbol version the device does not have.
#
# The two runs share one abstract socket name, so they run one after the other, never in parallel.
set -euo pipefail
HERE="$(cd "$(dirname "$0")/.." && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
if [ -n "${MONO_TEST_KEEP:-}" ]; then W="$MONO_TEST_KEEP"; mkdir -p "$W"; else W="$(mktemp -d)"; trap 'rm -rf "$W"' EXIT; fi
fail=0

S="$W/state"
expect() {  # expect <label> <condition-description> <test-expression...>
    local label="$1" what="$2"; shift 2
    if "$@"; then echo "$label: ok   $what"; else echo "$label: FAIL $what"; fail=1; fi
}
run_one() {  # run_one <label> <who> <MONO_EXPECT> <runner...>
    local label="$1" who="$2" exp="$3"; shift 3
    if ! MONO_TEST_DIR="$S" MONO_EXPECT="$exp" "$@" "$who" >"$W/out" 2>&1; then
        echo "FAIL $label ($who, $exp):"; cat "$W/out"; fail=1
    else
        tail -1 "$W/out" | sed "s/^/$label: /"
    fi
}
# The four situations the device can be in, each from a clean state directory.
run_all() {  # run_all <label> <runner...>
    local label="$1"; shift
    rm -rf "$S"; mkdir -p "$S"; cp "$WAMPY" "$S/libsound_service_fw.wampy.so"
    run_one "$label" SoundServiceFw active "$@"
    expect "$label" "Wampy's library was chained"                    test -e "$S/wampy_loaded"
    expect "$label" "audio through a hook cleared the load count"    test ! -e "$S/mono_shim_boots"

    rm -rf "$S"; mkdir -p "$S"; cp "$WAMPY" "$S/libsound_service_fw.wampy.so"
    run_one "$label" hagodaemon active "$@"
    expect "$label" "another process: no load count, no chain"       test ! -e "$S/mono_shim_boots" -a ! -e "$S/wampy_loaded"

    rm -rf "$S"; mkdir -p "$S"; cp "$WAMPY" "$S/libsound_service_fw.wampy.so"; echo 2 > "$S/mono_shim_boots"
    run_one "$label" SoundServiceFw inactive "$@"
    expect "$label" "two uncleared loads: SAFE MODE, Wampy not chained" test ! -e "$S/wampy_loaded"
    expect "$label" "…and the count keeps counting (3)"              grep -qx 3 "$S/mono_shim_boots"

    rm -rf "$S"; mkdir -p "$S"; cp "$WAMPY" "$S/libsound_service_fw.wampy.so"; touch "$S/mono_shim_off"
    run_one "$label" SoundServiceFw inactive "$@"
    expect "$label" "mono_shim_off: hooks off, Wampy still chained"  test -e "$S/wampy_loaded"
}

# ── host ──
DEFS=(-DCINDER_MONO_DIR="\"$S\"" -DCINDER_MONO_DATA="\"$S\"" -DCINDER_WAMPY_LIB="\"$S/libsound_service_fw\"")
cc -O2 -Wall -Wextra -shared -fPIC "${DEFS[@]}" -I"$HERE/src" \
   -o "$W/libcinder_mono.so" "$HERE/src/cinder-mono.c" -ldl -lpthread
cc -O2 -Wall -shared -fPIC -DFAKE_WAMPY -o "$W/fakewampy.so" "$HERE/tools/mono_shim_test.c"
WAMPY="$W/fakewampy.so"
cc -O2 -Wall -shared -fPIC -DFAKE_ASOUND -o "$W/libfakeasound.so" "$HERE/tools/mono_shim_test.c"
cc -O2 -Wall -o "$W/mono_shim_test" "$HERE/tools/mono_shim_test.c" -L"$W" -lfakeasound -Wl,-rpath,"$W"
run_all host env LD_PRELOAD="$W/libcinder_mono.so" "$W/mono_shim_test"

# ── device glibc under qemu ──
DEVSYS="${DEVSYS:-$HOME/toolchains/xenial-armhf-sysroot/sysroot}"
RAMLIB="$REPO/analysis/ramdisk/lib"
QEMU="$(command -v qemu-armhf-static || command -v qemu-arm-static || true)"
if [ -d "$DEVSYS/usr/include" ] && [ -f "$RAMLIB/libc.so.6" ] && [ -n "$QEMU" ] && command -v arm-linux-gnueabihf-gcc >/dev/null; then
    CC=arm-linux-gnueabihf-gcc
    GCCINC="$($CC -print-file-name=include)"
    SYS223=(-nostdinc -isystem "$GCCINC" -isystem "$DEVSYS/usr/include/arm-linux-gnueabihf" -isystem "$DEVSYS/usr/include"
            -isystem /usr/arm-linux-gnueabihf/include -U_TIME_BITS -U_FILE_OFFSET_BITS)
    SR="$W/sysroot"; mkdir -p "$SR/lib"; cp -a "$RAMLIB"/. "$SR/lib/"
    LINK=(-nostdlib -L"$RAMLIB" -l:libc.so.6 -l:libdl.so.2 -l:libpthread.so.0 "$($CC -print-libgcc-file-name)")
    "$CC" -O2 -Wall -shared -fPIC "${SYS223[@]}" "${DEFS[@]}" -I"$HERE/src" \
          -Wl,-soname,libcinder_mono.so -o "$SR/lib/libcinder_mono.so" "$HERE/src/cinder-mono.c" "${LINK[@]}"
    "$CC" -O2 -shared -fPIC "${SYS223[@]}" -DFAKE_WAMPY -o "$W/fakewampy_arm.so" "$HERE/tools/mono_shim_test.c" "${LINK[@]}"
    WAMPY="$W/fakewampy_arm.so"
    "$CC" -O2 -shared -fPIC "${SYS223[@]}" -DFAKE_ASOUND -Wl,-soname,libfakeasound.so \
          -o "$SR/lib/libfakeasound.so" "$HERE/tools/mono_shim_test.c" "${LINK[@]}"
    CRT="$DEVSYS/usr/lib/arm-linux-gnueabihf"
    "$CC" -O2 "${SYS223[@]}" -nostdlib -o "$SR/mono_shim_test" \
          "$CRT/crt1.o" "$CRT/crti.o" "$HERE/tools/mono_shim_test.c" \
          -L"$SR/lib" -l:libfakeasound.so "${LINK[@]}" "$CRT/libc_nonshared.a" "$CRT/crtn.o" \
          -Wl,--dynamic-linker=/lib/ld-linux-armhf.so.3
    bad="$(arm-linux-gnueabihf-readelf -V "$SR/lib/libcinder_mono.so" | grep -oE 'GLIBC_[0-9.]+' | sort -uV |
           awk -F_ '{split($2,a,"."); if (a[1]>2 || (a[1]==2 && a[2]>23)) print}')"
    if [ -n "$bad" ]; then echo "FAIL device: needs glibc newer than 2.23: $bad"; fail=1; fi
    echo "device: libcinder_mono.so needs $(arm-linux-gnueabihf-readelf -V "$SR/lib/libcinder_mono.so" | grep -oE 'GLIBC_[0-9.]+' | sort -uV | tr '\n' ' ')"
    # qemu -L only redirects paths that exist under the sysroot, so the state dir is a host path.
    run_all "device (qemu)" "$QEMU" -L "$SR" -E LD_LIBRARY_PATH=/lib -E LD_PRELOAD=/lib/libcinder_mono.so "$SR/mono_shim_test"
else
    echo "device (qemu): skipped — cross toolchain, device libs or qemu not present"
fi
exit $fail
