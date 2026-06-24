#!/usr/bin/env bash
# Cross-build cinder-home for the NW-A50 (arm-linux-gnueabihf).
#
# ABI: Sony's easel/appmgr symbols use libc++ (std::__1::...), so this MUST be built
# with clang -stdlib=libc++ (NOT g++/libstdc++) or the std::function/unique_ptr objects
# we pass into CuiAppModule will be ABI-incompatible and crash. We link the device's
# libc++/libcxxrt + easel libs from the extracted rootfs.
#
# -fno-rtti is REQUIRED: easel::ApplicationBase's typeinfo (_ZTIN5easel15ApplicationBaseE)
# is a LOCAL symbol inside libeaselcore (not exported), so a subclass's typeinfo can't link
# against it. We never dynamic_cast/typeid the app, so -fno-rtti (null typeinfo slot in our
# vtable) is harmless and makes every symbol resolve. VERIFIED 2026-06-24: with libc++ +
# -fno-rtti, all 22 easel refs + all 11 PlayerService refs from main.o/player_shim.o match
# the device .so exports exactly (real std::__1 mangling).
#
# PREREQUISITES:
#   - libc++-18-dev installed (headers), OR pass -nostdinc++ -isystem <libc++/v1 headers>.
#   - the DEVICE's libc++.so.1 + libcxxrt.so.1 for the final link + correct runtime ABI
#     (NOT in the extracted rootfs — adb-pull / MSC-pull them into $ROOTFS/lib first).
#   - on-device validation that libc++ struct layout (function/unique_ptr/string) matches.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOTFS="$HERE/../artifacts/rootfs_mnt"
OUT="$HERE/cinder-home"

CXX=clang++
TARGET=arm-linux-gnueabihf
SYSROOT=/usr/arm-linux-gnueabihf            # g++-arm-linux-gnueabihf provides this

# Rust render core (glibc C-ABI staticlib). Build it first with:
#   cd ../player && cargo build -p cinder-ffi --release --target arm-unknown-linux-gnueabihf
RUSTLIB="$HERE/../player/target/arm-unknown-linux-gnueabihf/release"

AUDIO="$HERE/../cinder-audio"

LIBDIRS=(-L"$ROOTFS/vendor/sony/lib" -L"$ROOTFS/lib" -L"$ROOTFS/usr/lib" -L"$RUSTLIB")
# easel + appmgr + core + PlayerService client (drag in libc++/libcxxrt transitively, on device)
LIBS=(-leaselcore -leaselcui -lpstcore -lappmgrservice -lPlayerServiceClient)
# the Rust UI + the system libs its std needs (static)
RUSTLIBS=(-lcinder_ffi -lpthread -ldl -lm)

INCLUDES=(-I"$HERE/src" -I"$HERE/../player/cinder-ffi/include" \
          -I"$AUDIO/include" -I"$AUDIO/src")

echo "compile shell + audio shim (clang/libc++, armhf)…"
$CXX --target=$TARGET -stdlib=libc++ --sysroot="$SYSROOT" \
     -fPIC -O2 -Wall -std=c++17 -fno-rtti "${INCLUDES[@]}" \
     -c "$HERE/src/main.cpp" -o "$HERE/main.o"
$CXX --target=$TARGET -stdlib=libc++ --sysroot="$SYSROOT" \
     -fPIC -O2 -Wall -std=c++17 -fno-rtti "${INCLUDES[@]}" \
     -c "$AUDIO/src/player_shim.cpp" -o "$HERE/player_shim.o"

echo "link…"
$CXX --target=$TARGET -stdlib=libc++ --sysroot="$SYSROOT" \
     "$HERE/main.o" "$HERE/player_shim.o" \
     "${LIBDIRS[@]}" \
     -Wl,--allow-shlib-undefined -Wl,-rpath-link,"$ROOTFS/vendor/sony/lib:$ROOTFS/lib" \
     "${LIBS[@]}" "${RUSTLIBS[@]}" \
     -o "$OUT"
${TARGET}-strip "$OUT" 2>/dev/null || true
echo "built: $OUT"; file "$OUT"
