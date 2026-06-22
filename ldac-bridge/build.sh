#!/usr/bin/env bash
# Cross-build ldac-bridge for the NW-A50 (arm-linux-gnueabihf / glibc, dynamic).
# Unlike the Cinder UI (musl/static), this links Sony's services so it MUST match
# the device ABI: glibc + ld-linux-armhf.so.3 + (at link time) the device .so set.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOTFS="$HERE/../artifacts/rootfs_mnt"
CC=arm-linux-gnueabihf-gcc
OUT="$HERE/cinder-ldac-bridge"

# Device libraries to link against (link stubs come straight from the extracted rootfs).
LIBDIRS=(-L"$ROOTFS/lib" -L"$ROOTFS/vendor/sony/lib" -L"$ROOTFS/usr/lib")

# ALSA headers: install with `sudo apt install libasound2-dev` (API headers are
# arch-independent; we link the DEVICE's libasound.so at runtime).
CFLAGS="-O2 -Wall -Wextra -I$HERE/src"

echo "compile..."
$CC $CFLAGS -c "$HERE/src/main.c"     -o "$HERE/main.o"
$CC $CFLAGS -c "$HERE/src/btclient.c" -o "$HERE/btclient.o"
$CC $CFLAGS -c "$HERE/src/capture.c"  -o "$HERE/capture.o"   # needs alsa/asoundlib.h

echo "link..."
# -lasound for capture; -lBtTransmitterService for the factory symbol. The latter
# drags in transitive deps (libpstcore, libBtCompIf, libConfigurationService,
# libc++, libcxxrt, ...) — all present under vendor/sony/lib and lib.
$CC "$HERE/main.o" "$HERE/btclient.o" "$HERE/capture.o" \
    "${LIBDIRS[@]}" \
    -Wl,--allow-shlib-undefined -Wl,-rpath-link,"$ROOTFS/vendor/sony/lib:$ROOTFS/lib" \
    -lasound -lBtTransmitterService \
    -o "$OUT"

arm-linux-gnueabihf-strip "$OUT" 2>/dev/null || true
echo "built: $OUT"
file "$OUT"
