#!/usr/bin/env bash
# Cross-build cinder-home for the NW-A50 (arm-linux-gnueabihf).
#
# ABI: Sony's easel/appmgr symbols use libc++ (std::__1::...), so this MUST be built
# with clang -stdlib=libc++ (NOT g++/libstdc++) or the std::function/unique_ptr objects
# we pass into CuiAppModule will be ABI-incompatible and crash. We link the device's
# libc++/libcxxrt + easel libs from the extracted rootfs.
#
# PREREQUISITE (not yet satisfied in this tree — see README):
#   - a libc++ for armhf whose ABI matches the device (ideally the device's own
#     libc++.so.1 + matching headers). Host clang's libc++18 headers are a starting
#     point but the runtime ABI must be validated on-device.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOTFS="$HERE/../artifacts/rootfs_mnt"
OUT="$HERE/cinder-home"

CXX=clang++
TARGET=arm-linux-gnueabihf
SYSROOT=/usr/arm-linux-gnueabihf            # g++-arm-linux-gnueabihf provides this

LIBDIRS=(-L"$ROOTFS/vendor/sony/lib" -L"$ROOTFS/lib" -L"$ROOTFS/usr/lib")
# easel + appmgr + core (drag in libc++/libcxxrt transitively, present on device)
LIBS=(-leaselcore -leaselcui -lpstcore -lappmgrservice)

echo "compile (clang/libc++, armhf)…"
$CXX --target=$TARGET -stdlib=libc++ --sysroot="$SYSROOT" \
     -fPIC -O2 -Wall -std=c++17 -I"$HERE/src" \
     -c "$HERE/src/main.cpp" -o "$HERE/main.o"
# render core (plain C — reuse the cinder-device framebuffer logic here)
${TARGET}-gcc -fPIC -O2 -Wall -c "$HERE/src/render.c" -o "$HERE/render.o" 2>/dev/null \
     || echo "note: src/render.c not present yet (stub the C render entry points)"

echo "link…"
$CXX --target=$TARGET -stdlib=libc++ --sysroot="$SYSROOT" \
     "$HERE/main.o" ${render_o:-$HERE/render.o} \
     "${LIBDIRS[@]}" \
     -Wl,--allow-shlib-undefined -Wl,-rpath-link,"$ROOTFS/vendor/sony/lib:$ROOTFS/lib" \
     "${LIBS[@]}" \
     -o "$OUT"
${TARGET}-strip "$OUT" 2>/dev/null || true
echo "built: $OUT"; file "$OUT"
