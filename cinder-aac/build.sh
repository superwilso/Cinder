#!/bin/bash
# Builds for the player's glibc 2.23 userland (same toolchain and sysroot as Cinder's build.sh).
#   build.sh          libcinder_aac.so (the preload) + build/enc_test (the qemu encoder test)
#   build.sh test     also runs the encoder test against the firmware's own libFraunhoferAAC.so
set -eu
cd "$(dirname "$0")"
: "${FW:=$HOME/sony/artifacts/rootfs_mnt}"                              # stock rootfs (read only)
: "${RAMLIB:=$HOME/sony/analysis/ramdisk/lib}"                         # device glibc 2.23 (boot ramdisk /lib)
: "${DEVSYS:=$HOME/toolchains/xenial-armhf-sysroot/sysroot}"            # glibc-2.23 headers + crt
CC=arm-linux-gnueabihf-gcc
L=$DEVSYS/usr/lib/arm-linux-gnueabihf
SYS=(-U_TIME_BITS -U_FILE_OFFSET_BITS -nostdinc -isystem "$($CC -print-file-name=include)"
     -isystem "$DEVSYS/usr/include/arm-linux-gnueabihf" -isystem "$DEVSYS/usr/include"
     -isystem /usr/arm-linux-gnueabihf/include)
LIBS=(-nostdlib -L"$RAMLIB" -L"$FW/lib" -L"$FW/usr/lib" -l:libdl.so.2 -l:libpthread.so.0
      -l:libm.so.6 -l:libc.so.6 "$L/libc_nonshared.a" "$($CC -print-libgcc-file-name)"
      -Wl,--allow-shlib-undefined)
mkdir -p build

$CC -O2 -Wall -Wextra "${SYS[@]}" -o build/enc_test test/enc_test.c src/aac_enc.c \
    "$L/crt1.o" "$L/crti.o" "${LIBS[@]}" "$L/crtn.o" -Wl,-dynamic-linker,/lib/ld-linux-armhf.so.3

if [ -f src/cinder_aac.c ]; then
    $CC -O2 -Wall -Wextra -shared -fPIC -fvisibility=hidden "${SYS[@]}" -Wl,-soname,libcinder_aac.so \
        -o build/libcinder_aac.so src/cinder_aac.c src/aac_enc.c "${LIBS[@]}"
    bad=$(arm-linux-gnueabihf-readelf -V build/libcinder_aac.so | grep -oE 'GLIBC_[0-9.]+' | sort -uV |
          awk -F_ '{split($2,a,"."); if (a[1]>2 || (a[1]==2 && a[2]>23)) print}')
    [ -z "$bad" ] || { echo "FAIL: needs glibc newer than 2.23: $bad"; exit 1; }
    echo "built build/libcinder_aac.so ($(stat -c %s build/libcinder_aac.so) bytes)"
fi

if [ "${1:-}" = test ]; then
    # qemu needs one root holding both halves of the player's userland: the ramdisk's /lib (glibc)
    # and the rootfs as /system (the encoder and its C++ runtime).
    QROOT=build/qroot; mkdir -p $QROOT
    ln -sfn "$RAMLIB" $QROOT/lib; ln -sfn "$FW" $QROOT/system
    # Both encoders: Cinder's fdk-aac 2.x build (when build_fdk.sh has run), then the firmware's.
    libpaths=(/lib:/system/lib:/system/usr/lib)
    [ -f build/libfdk-aac-enc.so ] && { ln -sfn "$PWD/build" $QROOT/cinder; libpaths=(/cinder:${libpaths[0]} "${libpaths[@]}"); }
    for lp in "${libpaths[@]}"; do
        for r in "44100 668" "48000 1005" "44100 1005" "44100 882"; do
            set -- $r
            qemu-arm-static -L "$QROOT" -E LD_LIBRARY_PATH="$lp" build/enc_test "build/t$1.loas" "$1" "$2"
        done
    done
fi
