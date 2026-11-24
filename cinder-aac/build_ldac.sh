#!/bin/bash
# Builds AOSP's LDAC encoder (third_party/libldac, Apache 2.0, the upstream source, not committed) as
# build/libldacBT_enc.a for the player's glibc 2.23, for the own Bluetooth stack's btsource
# (cinder-home/tools/build_bt.sh). HOST=1 builds it for this machine instead (build/host/), for the
# host tests. The source is fetched on the first run, at a pinned commit (git checks the bytes).
set -eu
export SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-1735689600}"
cd "$(dirname "$0")"
REV=eeee1a3f5f8df1282e3a6d297085885fd886737b   # external/libldac, 2025-01-10
SRC=third_party/libldac
if [ "$(git -C $SRC rev-parse HEAD 2>/dev/null)" != $REV ]; then
    rm -rf $SRC
    mkdir -p third_party
    git -C third_party init -q libldac
    git -C $SRC fetch -q --depth=1 https://android.googlesource.com/platform/external/libldac $REV
    git -C $SRC -c advice.detachedHead=false checkout -q FETCH_HEAD
fi
if [ "${HOST:-0}" = 1 ]; then
    CC="cc"; AR="ar"; OUT=build/host; FLAGS=()
else
    : "${DEVSYS:=$HOME/toolchains/xenial-armhf-sysroot/sysroot}"
    CC=arm-linux-gnueabihf-gcc; AR=arm-linux-gnueabihf-ar; OUT=build
    # The player's glibc headers, not the cross toolchain's newer ones (build_bt.sh's recipe).
    FLAGS=(-U_TIME_BITS -U_FILE_OFFSET_BITS -nostdinc -isystem "$($CC -print-file-name=include)"
           -isystem "$DEVSYS/usr/include/arm-linux-gnueabihf" -isystem "$DEVSYS/usr/include"
           -marm -mfpu=neon -mfloat-abi=hard
           -fsigned-char -fno-stack-protector)   # its tables hold -1 in plain char, which is unsigned on ARM
fi
mkdir -p $OUT/ldac
for f in ldaclib ldacBT; do   # upstream's Android.bp: these two include the rest
    $CC -O2 -Wall -Wextra -Werror -fPIC "${FLAGS[@]}" -I$SRC/inc -c $SRC/src/$f.c -o $OUT/ldac/$f.o
done
rm -f $OUT/libldacBT_enc.a
$AR rcsD $OUT/libldacBT_enc.a $OUT/ldac/ldaclib.o $OUT/ldac/ldacBT.o
echo "built $OUT/libldacBT_enc.a ($(stat -c %s $OUT/libldacBT_enc.a) bytes)"
