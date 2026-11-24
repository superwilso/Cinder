#!/usr/bin/env bash
# test_bt_enc.sh — tools/bt_enc.h (btsource's SBC and LDAC encoders) under qemu-arm against the
# player's own libraries: tools/bt_enc_test.c. Needs the firmware's rootfs (FW), the ramdisk's glibc
# (RAMLIB), the xenial sysroot (DEVSYS) and cinder-aac/build_ldac.sh's library (built here if missing).
set -eu
HERE=$(cd "$(dirname "$0")/.." && pwd)
REPO=$(cd "$HERE/.." && pwd)
: "${FW:=$REPO/artifacts/rootfs_mnt}"
: "${RAMLIB:=$REPO/analysis/ramdisk/lib}"
: "${DEVSYS:=$HOME/toolchains/xenial-armhf-sysroot/sysroot}"
QEMU="$(command -v qemu-armhf-static || command -v qemu-arm-static || true)"
[ -n "$QEMU" ] && [ -f "$FW/lib/libsbc_enc.so" ] || { echo "(skip: needs qemu-arm and the firmware rootfs)"; exit 0; }
[ -f "$REPO/cinder-aac/build/libldacBT_enc.a" ] || "$REPO/cinder-aac/build_ldac.sh"
CC=arm-linux-gnueabihf-gcc
L=$DEVSYS/usr/lib/arm-linux-gnueabihf
OUT=$HERE/build/bt
mkdir -p "$OUT/qroot"
$CC -O2 -Wall -Wextra -Werror -U_TIME_BITS -U_FILE_OFFSET_BITS -nostdinc -isystem "$($CC -print-file-name=include)" \
    -isystem "$DEVSYS/usr/include/arm-linux-gnueabihf" -isystem "$DEVSYS/usr/include" -isystem /usr/arm-linux-gnueabihf/include \
    -o "$OUT/bt_enc_test" "$HERE/tools/bt_enc_test.c" "$REPO/cinder-aac/src/aac_enc.c" "$REPO/cinder-aac/build/libldacBT_enc.a" \
    "$L/crt1.o" "$L/crti.o" -nostdlib -L"$RAMLIB" -L"$FW/lib" -L"$FW/usr/lib" -l:libdl.so.2 -l:libpthread.so.0 \
    -l:libm.so.6 -l:libc.so.6 "$L/libc_nonshared.a" "$($CC -print-libgcc-file-name)" -Wl,--allow-shlib-undefined \
    "$L/crtn.o" -Wl,-dynamic-linker,/lib/ld-linux-armhf.so.3
# One root with both halves of the player's userland: the ramdisk's /lib (glibc), the rootfs as /system.
ln -sfn "$RAMLIB" "$OUT/qroot/lib"
ln -sfn "$FW" "$OUT/qroot/system"
"$QEMU" -L "$OUT/qroot" -E LD_LIBRARY_PATH=/lib:/system/lib:/system/usr/lib "$OUT/bt_enc_test"
