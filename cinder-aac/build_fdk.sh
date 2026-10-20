#!/bin/bash
# Builds fdk-aac's encoder (third_party/fdk-aac-<ver>, the upstream source, not committed) as
# build/libfdk-aac-enc.so for the player's glibc 2.23. aac_enc.c prefers it over the firmware's
# older encoder: same C interface, and it can cap single frames (AACENC_PEAK_BITRATE).
# The source is fetched on the first run.
set -eu
cd "$(dirname "$0")"
: "${DEVSYS:=$HOME/toolchains/xenial-armhf-sysroot/sysroot}"
: "${RAMLIB:=$HOME/sony/analysis/ramdisk/lib}"
SRC=third_party/fdk-aac-2.0.3
CXX=arm-linux-gnueabihf-g++
[ -f $SRC/Makefile.am ] || { mkdir -p third_party && curl -fsSL \
    https://github.com/mstorsjo/fdk-aac/archive/refs/tags/v2.0.3.tar.gz | tar xz -C third_party; }
L=$DEVSYS/usr/lib/arm-linux-gnueabihf
# The encoder half of upstream's Makefile.am source lists.
files=$(awk '/^(AACENC|FDK|MPEGTPENC|PCMUTILS|SACENC|SBRENC|SYS)_SRC =/{on=1; next}
             on && /\.cpp/{gsub(/[\\ ]/,""); print; next} {on=0}' $SRC/Makefile.am)
inc=$(for d in $SRC/lib*/include; do printf -- '-I%s ' "$d"; done)
mkdir -p build/fdk
objs=()
for f in $files; do
    o=build/fdk/$(basename "${f%.cpp}").o; objs+=("$o")
    [ "$o" -nt "$SRC/$f" ] && continue
    # No C++ runtime: the library uses none, so it needs only libc and libgcc on the player.
    $CXX -O2 -fPIC -fno-exceptions -fno-rtti -fno-threadsafe-statics -fno-stack-protector -mfpu=neon -mfloat-abi=hard \
        -U_TIME_BITS -U_FILE_OFFSET_BITS -nostdinc -isystem "$($CXX -print-file-name=include)" \
        -isystem "$DEVSYS/usr/include/arm-linux-gnueabihf" -isystem "$DEVSYS/usr/include" \
        $inc -w -c "$SRC/$f" -o "$o" &
    [ "$(jobs -r | wc -l)" -ge "$(nproc)" ] && wait -n
done
wait
$CXX -shared -nostdlib -Wl,-soname,libfdk-aac-enc.so -Wl,--version-script=<(printf '{ global: aacEnc*; local: *; };') \
    -o build/libfdk-aac-enc.so "${objs[@]}" -L"$RAMLIB" -l:libm.so.6 -l:libc.so.6 "$L/libc_nonshared.a" \
    "$($CXX -print-libgcc-file-name)" -Wl,--no-undefined
bad=$(arm-linux-gnueabihf-readelf -V build/libfdk-aac-enc.so | grep -oE 'GLIBC_[0-9.]+' | sort -uV |
      awk -F_ '{split($2,a,"."); if (a[1]>2 || (a[1]==2 && a[2]>23)) print}')
[ -z "$bad" ] || { echo "FAIL: needs glibc newer than 2.23: $bad"; exit 1; }
echo "built build/libfdk-aac-enc.so ($(stat -c %s build/libfdk-aac-enc.so) bytes)"
