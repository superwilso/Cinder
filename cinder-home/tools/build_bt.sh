#!/usr/bin/env bash
# build_bt.sh — the Phase 4 Bluetooth tools (docs/PLAN_hardware_first.md) for the player, against
# its glibc 2.23 (cinder-aac/build.sh's recipe): btspike, btbridge, btpair, btsource.
# Output in cinder-home/build/bt/; push with `adb push … /data/local/tmp/`.
set -eu
HERE=$(cd "$(dirname "$0")/.." && pwd)
REPO=$(cd "$HERE/.." && pwd)
: "${FW:=$REPO/artifacts/rootfs_mnt}"
: "${RAMLIB:=$REPO/analysis/ramdisk/lib}"
: "${DEVSYS:=$HOME/toolchains/xenial-armhf-sysroot/sysroot}"
CC=arm-linux-gnueabihf-gcc
L=$DEVSYS/usr/lib/arm-linux-gnueabihf
SYS=(-U_TIME_BITS -U_FILE_OFFSET_BITS -nostdinc -isystem "$($CC -print-file-name=include)"
     -isystem "$DEVSYS/usr/include/arm-linux-gnueabihf" -isystem "$DEVSYS/usr/include"
     -isystem /usr/arm-linux-gnueabihf/include)
# shellcheck disable=SC2054  # -Wl,flag is one argument, not a list
LIBS=(-nostdlib -L"$RAMLIB" -L"$FW/lib" -L"$FW/usr/lib" -l:libdl.so.2 -l:libpthread.so.0
      -l:libm.so.6 -l:libc.so.6 "$L/libc_nonshared.a" "$($CC -print-libgcc-file-name)"
      -Wl,--allow-shlib-undefined)
OUT=$HERE/build/bt
mkdir -p "$OUT"
for t in btspike btbridge btpair btsource; do
    extra=()
    case $t in btpair) extra=("$REPO/cinder-aac/src/aac_enc.c") ;;
               # LDAC from cinder-aac/build_ldac.sh (AOSP's encoder, Apache 2.0); SBC is the firmware's.
               btsource) [ -f "$REPO/cinder-aac/build/libldacBT_enc.a" ] || "$REPO/cinder-aac/build_ldac.sh"
                         extra=("$REPO/cinder-aac/src/aac_enc.c" "$REPO/cinder-aac/build/libldacBT_enc.a") ;; esac
    $CC -O2 -Wall -Wextra -Werror "${SYS[@]}" -o "$OUT/$t" "$HERE/tools/$t.c" "${extra[@]}" \
        "$L/crt1.o" "$L/crti.o" "${LIBS[@]}" "$L/crtn.o" -Wl,-dynamic-linker,/lib/ld-linux-armhf.so.3
    bad=$(arm-linux-gnueabihf-readelf -V "$OUT/$t" | grep -oE 'GLIBC_[0-9.]+' | sort -uV |
          awk -F_ '{split($2,a,"."); if (a[1]>2 || (a[1]==2 && a[2]>23)) print}')
    [ -z "$bad" ] || { echo "FAIL: $t needs glibc newer than 2.23: $bad"; exit 1; }
    echo "built $OUT/$t ($(stat -c %s "$OUT/$t") bytes)"
done
