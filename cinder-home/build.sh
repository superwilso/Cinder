#!/usr/bin/env bash
# Cross-build cinder-home for the NW-A50 — a real, device-loadable ARM binary.
# VERIFIED 2026-06-24: links clean, needs only GLIBC_2.4 (device is glibc 2.23), every
# undefined symbol resolves against the device libs. ~2.5 MB stripped ARM PIE.
#
# ── The three things that make this work ──────────────────────────────────────────────
# 1. ABI = libc++ (std::__1), NOT libstdc++. Sony's easel/appmgr/PlayerService symbols are
#    libc++-mangled, so the C++ shell is compiled clang -stdlib=libc++ with the device's
#    libc++ headers, and -fno-rtti (easel::ApplicationBase's typeinfo is a LOCAL symbol in
#    libeaselcore, can't be linked against; we never typeid/dynamic_cast the app).
# 2. glibc 2.23 TARGET. The device is glibc 2.23 (2016); this host's cross-gcc is glibc 2.39.
#    Linking against 2.39 emits GLIBC_2.28..2.34 symbol refs the device's ld-2.23 REFUSES.
#    Fix: a glibc-2.23 sysroot (Ubuntu-16.04 "xenial" armhf .debs) for crt + libc, forced
#    via -B<crt> and the xenial libdirs first (clang otherwise grabs the gcc-13 glibc 2.39).
# 3. The Rust render staticlib (cinder-ffi) and its bundled SQLite must ALSO be 2.23-clean:
#    - SQLite compiled against the 2.23 headers (-nostdinc -isystem <xenial>) with LFS off
#      (-DSQLITE_DISABLE_LFS) and 32-bit time (-U_TIME_BITS) so it uses stat/fcntl/time, not
#      the *_time64 / stat64 / fcntl64 symbols that DON'T EXIST in glibc 2.23.
#    - glibc223_compat.c shims stat/fstat/lstat/fstatat (+64) -> __xstat/__fxstat/... which
#      glibc 2.23 exports as @GLIBC_2.4 (it doesn't export plain stat; SQLite takes &stat).
# See README.md and the project memory for the full derivation.
#
# ── PREREQUISITES (all offline, no device, no sudo) ───────────────────────────────────
#   * clang-18 (clang++-18).
#   * libc++-18 + libc++abi-18 HEADERS. Get without sudo:
#       cd <dir> && curl -fLO https://releases.llvm.org/3.9.0/libcxx-3.9.0.src.tar.xz
#       tar xf libcxx-3.9.0.src.tar.xz
#       LIBCXX_V1=<dir>/libcxx-3.9.0.src/include
#     MUST be libc++ 3.9.0 — the device is libcxx-3.9.0 (clang 3.9, Chromium-OS, 2016) and
#     its std::function layout (24B, functor ptr @+16) differs from modern libc++; building
#     with libc++18 corrupts the CuiAppModule callbacks -> hang in OnInitialize on device.
#   * The glibc-2.23 sysroot (xenial). Get without sudo:
#       mkdir -p $DEVSYS/.. && cd <dir>
#       B=http://ports.ubuntu.com/ubuntu-ports/pool/main/g/glibc
#       curl -fLO $B/libc6-dev_2.23-0ubuntu11.3_armhf.deb
#       curl -fLO $B/libc6_2.23-0ubuntu11.3_armhf.deb
#       for d in *.deb; do dpkg-deb -x "$d" sysroot; done   # -> $DEVSYS = .../sysroot
#   * The device runtime libs are already in the repo at analysis/ramdisk/lib (libc++.so.1,
#     libcxxrt.so.1, libgcc_s.so.1, libc-2.23/ld-2.23, libpthread/libdl/libm) and the easel/
#     PlayerService libs at artifacts/rootfs_mnt/vendor/sony/lib — NO device pull needed.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$HERE/.."
RAMLIB="$REPO/analysis/ramdisk/lib"               # device glibc 2.23 + libc++/libcxxrt/libgcc_s
SONYLIB="$REPO/artifacts/rootfs_mnt/vendor/sony/lib"  # easel + PlayerService
RUSTLIB="$REPO/player/target/arm-unknown-linux-gnueabihf/release"
AUDIO="$REPO/cinder-audio"
OUT="$HERE/cinder-home"

# ── build channel: `stable` (default) or `dev`. Same tree, one flag. ────────────────────────
#   stable: the lean player, no adb.
#   dev:    adds a visible "CINDER DEV" marker (cargo `dev` feature) AND the dev binary enables
#           adb at boot (guarded, dev-only) for push-and-run iteration. Both build from this tree;
#           artifacts land in dist/<channel>/ so they never clobber each other.
CHANNEL="${1:-stable}"
case "$CHANNEL" in
    stable) CARGO_FEATURES=();          CHANNEL_DEF=(); DISCOVER_MAIN=() ;;
    # dev links the discovery dump into cinder-home (auto-runs at boot); stable doesn't (the probe
    # links it in both channels regardless).
    dev)    CARGO_FEATURES=(--features dev); CHANNEL_DEF=(-DCINDER_DEV=1); DISCOVER_MAIN=("$HERE/discover.o") ;;
    *) echo "usage: build.sh [stable|dev]"; exit 1 ;;
esac
DIST="$HERE/dist/$CHANNEL"
echo "── CINDER build channel: $CHANNEL ──"

TARGET=arm-linux-gnueabihf
CXX=clang++-18
CC=arm-linux-gnueabihf-gcc

# --- configurable paths (override via env) -------------------------------------------------
: "${DEVSYS:=$HOME/toolchains/xenial-armhf-sysroot/sysroot}"   # glibc-2.23 sysroot
: "${LIBCXX_V1:=$HOME/toolchains/libcxx-3.9.0.src/include}"   # libc++ 3.9.0 headers (device ABI)
[ -f "$LIBCXX_V1/functional" ] || { echo "ERR: libc++ 3.9.0 headers not at $LIBCXX_V1 (see PREREQUISITES)"; exit 1; }
[ -d "$DEVSYS/usr/lib/arm-linux-gnueabihf" ] || { echo "ERR: glibc-2.23 sysroot not at $DEVSYS (see PREREQUISITES)"; exit 1; }

GCCINC="$($CC -print-file-name=include)"           # gcc builtin headers (stddef/stdarg)
KHDR=/usr/arm-linux-gnueabihf/include              # modern kernel uapi headers (asm/, linux/) — ABI-stable
# 2.23 header search for C: gcc builtins, then xenial glibc, then kernel uapi.
SYS223=(-nostdinc -isystem "$GCCINC" \
        -isystem "$DEVSYS/usr/include/arm-linux-gnueabihf" -isystem "$DEVSYS/usr/include" \
        -isystem "$KHDR")
# kill the cross-gcc's forced 64-bit time/offset (the device's glibc 2.23 has no time64).
T32=(-U_TIME_BITS -U_FILE_OFFSET_BITS)

INCLUDES=(-I"$HERE/src" -I"$REPO/player/cinder-ffi/include" -I"$AUDIO/include" -I"$AUDIO/src")

echo "[1/4] build cinder-ffi staticlib (Rust UI + SQLite vs glibc 2.23)…"
( cd "$REPO/player" && \
  CC_arm_unknown_linux_gnueabihf="$CC" \
  AR_arm_unknown_linux_gnueabihf=arm-linux-gnueabihf-ar \
  CFLAGS_arm_unknown_linux_gnueabihf="-DSQLITE_DISABLE_LFS ${T32[*]} ${SYS223[*]}" \
    cargo build -p cinder-ffi --release --target arm-unknown-linux-gnueabihf "${CARGO_FEATURES[@]}" )

echo "[2/4] compile C++ shell + audio shim (clang/libc++, -fno-rtti)…"
# Compile against the device libc++ headers AND the glibc-2.23 C headers. We must force the
# 2.23 C headers explicitly (-nostdinc + -isystem): --sysroot alone does NOT switch them, so
# clang would otherwise use the host's glibc-2.39 stdio/stdlib whose C23 redirects pull in
# __isoc23_scanf/__isoc23_strtol — symbols that DON'T EXIST on the device's glibc 2.23.
# Order: libc++ 3.9 C++ headers, then clang's builtin headers (stddef/stdarg), then xenial
# 2.23 glibc, then kernel uapi. T32 kills the forced 64-bit time/offset.
CLANGRES="$($CXX -print-resource-dir)"
CXXINC=(-nostdinc++ -isystem "$LIBCXX_V1" \
        -nostdinc -isystem "$CLANGRES/include" -isystem "$GCCINC" \
        -isystem "$DEVSYS/usr/include/arm-linux-gnueabihf" -isystem "$DEVSYS/usr/include" \
        -isystem "$KHDR")
for src in "$HERE/src/main.cpp:$HERE/main.o" \
           "$AUDIO/src/player_shim.cpp:$HERE/player_shim.o" \
           "$AUDIO/src/effect_shim.cpp:$HERE/effect_shim.o" \
           "$AUDIO/src/analyzer_shim.cpp:$HERE/analyzer_shim.o" \
           "$AUDIO/src/power_shim.cpp:$HERE/power_shim.o" \
           "$HERE/src/discover.cpp:$HERE/discover.o"; do
    $CXX --target=$TARGET -stdlib=libc++ "${CXXINC[@]}" "${T32[@]}" \
         -fPIC -O2 -Wall -std=c++14 -fno-rtti "${CHANNEL_DEF[@]}" "${INCLUDES[@]}" \
         -c "${src%%:*}" -o "${src##*:}"
done

echo "[3/4] compile glibc-2.23 compat shim (stat/* -> __xstat/*)…"
$CC -Os -fPIC "${T32[@]}" "${SYS223[@]}" \
    -c "$HERE/src/glibc223_compat.c" -o "$HERE/glibc223_compat.o"

echo "[4/4] link (xenial 2.23 crt forced via -B; device shared libs)…"
# crt override dir: force the xenial 2.23 start files (clang otherwise uses gcc-13/glibc-2.39).
CRT="$HERE/.crt223"; mkdir -p "$CRT"
cp -f "$DEVSYS/usr/lib/arm-linux-gnueabihf"/{Scrt1.o,crt1.o,crti.o,crtn.o} "$CRT/"
$CXX --target=$TARGET --sysroot="$DEVSYS" -B"$CRT" -nostdlib++ \
     -L"$DEVSYS/usr/lib/arm-linux-gnueabihf" -L"$DEVSYS/lib/arm-linux-gnueabihf" \
     "$HERE/main.o" "$HERE/player_shim.o" "$HERE/effect_shim.o" "$HERE/analyzer_shim.o" "$HERE/power_shim.o" "${DISCOVER_MAIN[@]}" "$HERE/glibc223_compat.o" \
     -L"$SONYLIB" -L"$RAMLIB" -L"$RUSTLIB" \
     -Wl,--allow-shlib-undefined -Wl,-rpath-link,"$SONYLIB:$RAMLIB" \
     -leaselcore -leaselcui -lpstcore -lappmgrservice -lPlayerServiceClient -lEffectCtrlDmp -lPowerMgrServiceClient \
     -l:libc++.so.1 -l:libcxxrt.so.1 -lcinder_ffi \
     -l:libpthread.so.0 -l:libdl.so.2 -l:libm.so.6 \
     -o "$OUT"

gate_glibc() {  # gate_glibc <binary> : fail if it needs GLIBC newer than the device's 2.23
    local b="$1"
    local bad
    bad="$(${TARGET}-readelf -V "$b" 2>/dev/null | grep -oE 'GLIBC_[0-9.]+' | sort -uV | \
           awk -F_ '{split($2,a,"."); if (a[1]>2 || (a[1]==2 && a[2]>23)) print $0}')"
    if [ -n "$bad" ]; then echo "FAIL: $b needs GLIBC newer than device 2.23:"; echo "$bad"; exit 1; fi
    echo "OK: $(basename "$b") GLIBC needs = $(${TARGET}-readelf -V "$b" 2>/dev/null | grep -oE 'GLIBC_[0-9.]+' | sort -uV | tr '\n' ' ')"
}

echo "── self-test: crash+hang GUARD recovery (host) ──"
# Validates the run_guarded() pattern main.cpp relies on: a crash/hang inside a guarded call is
# caught and skipped, the process survives. Fast, host-native, no device.
if cc -O2 -o "$HERE/.guard_selftest" "$HERE/tools/guard_selftest.cpp" 2>/dev/null; then
    if "$HERE/.guard_selftest" >/dev/null 2>&1; then echo "OK: guard recovers crash + hang"; \
    else echo "FAIL: guard self-test did not recover"; exit 1; fi
    rm -f "$HERE/.guard_selftest"
else echo "(skip: no host cc)"; fi

echo "── verify: device glibc compatibility gate ──"
gate_glibc "$OUT"
cp "$OUT" "$OUT.unstripped"; ${TARGET}-strip "$OUT"
echo "built: $OUT  ($(stat -c%s "$OUT") bytes)"; file "$OUT" | cut -d, -f1-3

echo "[5] build cinder-probe (standalone diagnostic — no easel lifecycle, no boot impact)…"
PROBE="$HERE/cinder-probe"
$CXX --target=$TARGET -stdlib=libc++ "${CXXINC[@]}" "${T32[@]}" \
     -fPIC -O2 -Wall -std=c++14 -fno-rtti "${CHANNEL_DEF[@]}" "${INCLUDES[@]}" \
     -c "$HERE/src/probe.cpp" -o "$HERE/probe.o"
# Links WITHOUT easelcore/easelcui/pstcore/appmgrservice — the probe never does the app
# lifecycle, only the render/DB/PlayerService calls, so it can't register as the Home app.
$CXX --target=$TARGET --sysroot="$DEVSYS" -B"$CRT" -nostdlib++ \
     -L"$DEVSYS/usr/lib/arm-linux-gnueabihf" -L"$DEVSYS/lib/arm-linux-gnueabihf" \
     "$HERE/probe.o" "$HERE/player_shim.o" "$HERE/analyzer_shim.o" "$HERE/discover.o" "$HERE/glibc223_compat.o" \
     -L"$SONYLIB" -L"$RAMLIB" -L"$RUSTLIB" \
     -Wl,--allow-shlib-undefined -Wl,-rpath-link,"$SONYLIB:$RAMLIB" \
     -lPlayerServiceClient -l:libc++.so.1 -l:libcxxrt.so.1 -lcinder_ffi \
     -l:libpthread.so.0 -l:libdl.so.2 -l:libm.so.6 \
     -o "$PROBE"
gate_glibc "$PROBE"
cp "$PROBE" "$PROBE.unstripped"; ${TARGET}-strip "$PROBE"
echo "built: $PROBE  ($(stat -c%s "$PROBE") bytes)"

# ── offline bring-up gate: construct the real device objects under qemu (no device needed) ──
# Catches std::function-ABI / ctor-signature / object-SIZE regressions BEFORE flashing.
if [ "${SKIP_PREFLIGHT:-0}" != "1" ]; then
    echo "── preflight: qemu construction gate ──"
    DEVSYS="$DEVSYS" LIBCXX_V1="$LIBCXX_V1" bash "$HERE/tools/preflight_qemu.sh"
fi

# ── stage the channel binaries into dist/<channel>/ (the two channels never clobber each other).
# pack_upg.sh <channel> packs the matching install/uninstall .UPGs alongside them.
mkdir -p "$DIST"
cp -f "$OUT" "$DIST/cinder-home"
cp -f "$HERE/cinder-probe" "$DIST/cinder-probe"
echo "staged $CHANNEL binaries -> $DIST/"
echo "── done ($CHANNEL). next: bash tools/pack_upg.sh $CHANNEL ──"
