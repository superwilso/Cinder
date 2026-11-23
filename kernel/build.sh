#!/usr/bin/env bash
# build.sh — the NW-A50 series kernel, rebuilt from Sony's own GPL source with Cinder's fixes.
#
#   ./build.sh                     -> out/zImage, out/modules/*.ko, out/System.map
#   ./build.sh STOCK_BOOT.img      -> also out/boot.img: that image's ramdisk with the new kernel
#   UNDERVOLT_MV=1090 ./build.sh   -> a TEST kernel with the 1040/747.5/598 MHz step at that voltage
#                                     (patch 0010), in out/test-uv1090/
#   LP_RING_MB=8 ./build.sh        -> a TEST kernel whose low-power PCM ring can be that large (patch
#                                     0011), in out/test-ring8/; both together: out/test-uv1090-ring8/.
#                                     A test kernel is never flashed or shipped: boot it from RAM.
#
# Source: Sony's linux-kernel-3.10.26 for the NW-A57 (the A50 series' kernel), configured with
# BBDMP5_linux_debug_defconfig, which equals the player's /proc/config.gz line for line. Toolchain
# and tarball come from cinder-llusbdac's build.sh, pinned by hash there; point KWORK at its work/
# folder (default: the sibling checkout).
#
# patches/ holds every change, applied in order; each says why. The tree under work/kernel is a git
# repository whose first commit is Sony's tarball, so `git diff` there shows exactly what changed.
#
# How close is this to the player's kernel? Built unpatched (0001 only), it has 47,715 symbols to
# the shipped kernel's 47,719; the difference is small static functions GCC 4.9 inlines where
# Sony's GCC 4.8 did not. Every import of Sony's 12 binary modules resolves (checked: SONY_MODULES).
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
KWORK="${KWORK:-$HERE/../../cinder-llusbdac/work}"
TARBALL="$KWORK/linux-kernel-3.10.26.tar.gz"
TARBALL_SHA=3aef2bdcc4ecd43ab9fed0f038fa384e2515c58851573908dc8b1743febb1609
export PATH="$KWORK/bin:$PATH"
W="$HERE/work"; K="$W/kernel"; OUT="$HERE/out"
UV="${UNDERVOLT_MV:-}" RING="${LP_RING_MB:-}" TAG=""
if [ -n "$UV" ]; then
    # MediaTek's validated low corner is 1090 mV; below 1050 no source of theirs goes.
    [[ "$UV" =~ ^1[01][0-9]{2}$ ]] && [ "$UV" -ge 1050 ] && [ "$UV" -le 1150 ] \
        || { echo "UNDERVOLT_MV must be 1050..1150" >&2; exit 2; }
    TAG="-uv$UV"
fi
if [ -n "$RING" ]; then
    # Preallocated at boot as one contiguous block: 16 MB is 3.5 % of the player's 467 MB.
    [[ "$RING" =~ ^[0-9]+$ ]] && [ "$RING" -ge 2 ] && [ "$RING" -le 16 ] \
        || { echo "LP_RING_MB must be 2..16" >&2; exit 2; }
    TAG="$TAG-ring$RING"
fi
[ -z "$TAG" ] || OUT="$OUT/test$TAG"
mkdir -p "$OUT"
mkdir -p "$W" "$OUT/modules"
echo "$TARBALL_SHA  $TARBALL" | sha256sum -c --status \
    || { echo "no Sony kernel tarball at $TARBALL (run cinder-llusbdac/build.sh, or set KWORK)" >&2; exit 1; }

if [ ! -d "$K/.git" ]; then
    echo "== Sony's tree"
    [ -e "$K" ] && mv "$K" "$W/kernel.old.$$"   # never rm -rf a computed path; clean these by hand
    mkdir -p "$K"
    tar xzf "$TARBALL" -C "$K"
    git -C "$K" init -q
    git -C "$K" add -A
    git -C "$K" -c user.name=sony -c user.email=sony@invalid commit -qm "Sony linux-kernel-3.10.26 (NW-A57), as released"
fi

echo "== patches"
git -C "$K" reset -q --hard
git -C "$K" clean -qfdx -e .config
for p in "$HERE"/patches/*.patch; do
    echo "   $(basename "$p")"
    git -C "$K" apply --whitespace=nowarn "$p"
done

echo "== build"
MK=(make -C "$K" ARCH=arm CROSS_COMPILE=arm-linux-gnueabihf-
    MTK_TB_WIFI_3G_MODE=WIFI_ONLY LOCALVERSION=   # set but empty: in a git tree the kernel would call itself 3.10.26+ and refuse Sony's modules
    MTK_CM4_SUPPORT=yes   # without it the low-power playback PCM fails to open and Sony's hang checker restarts the player
    KBUILD_BUILD_USER=cinder KBUILD_BUILD_HOST=cinder   # not the builder's own account and PC name
    # A fixed version line, so the same source gives the same zImage on any machine: CI's payload
    # check compares the release's kernel byte for byte with the maintainer's build.
    "KBUILD_BUILD_TIMESTAMP=Wed Jan  1 00:00:00 UTC 2025" KBUILD_BUILD_VERSION=1
    # __DATE__ and __TIME__ pinned too (half a dozen of Sony's drivers print them), with patch 0009.
    "KCFLAGS=-I$K/drivers/staging/android/ion -I$K/drivers/misc/mediatek/lcm -Wno-builtin-macro-redefined -D__DATE__='\"2025-01-01\"' -D__TIME__='\"00:00:00\"' -DCINDER_DVFS_V2=${UV:-1150} -DCINDER_LP_RING_MB=${RING:-2}")
"${MK[@]}" BBDMP5_linux_debug_defconfig >/dev/null
# LZO instead of Sony's XZ: the bootloader hands over to a kernel that unpacks in 1.3 s, not 2.7 s
# (measured 2026-11-15, patch 0006's log line against bootprof.lk_t). Needs lzop (apt install lzop).
"$K/scripts/config" --file "$K/.config" --disable KERNEL_XZ --enable KERNEL_LZO
"${MK[@]}" olddefconfig >/dev/null
"${MK[@]}" -j"$(nproc)" zImage modules >"$W/build.log" 2>&1 || { grep -E "error|Error" "$W/build.log" | tail -20; exit 1; }

cp "$K/arch/arm/boot/zImage" "$K/System.map" "$OUT/"
find "$K" -name '*.ko' -not -path '*/backports-*' -exec cp {} "$OUT/modules/" \;
grep -q 'vermagic=3.10.26 SMP preempt mod_unload ARMv7 ' <(strings "$OUT/modules/lcd.ko") \
    || { echo "wrong vermagic" >&2; exit 1; }

# Sony's binary-only modules must still find every symbol they import. SONY_MODULES = a folder of
# the player's /system/lib/modules/*.ko; skipped when unset.
if [ -n "${SONY_MODULES:-}" ]; then
    exports="$(awk '{print $2}' "$K/Module.symvers"; for m in "$SONY_MODULES"/*.ko; do arm-linux-gnueabihf-nm --defined-only "$m" | awk '$2 ~ /[TDBR]/ {print $3}'; done)"
    for m in "$SONY_MODULES"/*.ko; do
        miss="$(arm-linux-gnueabihf-nm -u "$m" | awk '{print $2}' | sort -u | comm -23 - <(sort -u <<<"$exports"))"
        [ -z "$miss" ] || { echo "$(basename "$m") would not load, missing: $miss" >&2; exit 1; }
    done
    echo "   Sony's modules: every import resolves"
fi

# czram.ko: the fixed zram (patch 0005) as a module beside the shipped kernel's built-in zram, which
# keeps its broken notifier. Same code, renamed so both can exist: /dev/block/czram0,
# /sys/block/czram0, /proc/czraminfo. Lets swap run on a stock kernel with nothing flashed.
echo "== czram.ko"
Z="$W/czram"; rm -rf "$Z"; mkdir -p "$Z"
cp "$K"/drivers/staging/zram/{zram_drv.c,zram_sysfs.c,zram_drv.h} "$Z/"
sed -i -e 's/"zram%d"/"czram%d"/; s/register_blkdev(0, "zram")/register_blkdev(0, "czram")/' \
       -e 's/unregister_blkdev(zram_major, "zram")/unregister_blkdev(zram_major, "czram")/' \
       -e 's/"zraminfo"/"czraminfo"/; s/#define KMSG_COMPONENT "zram"/#define KMSG_COMPONENT "czram"/' \
       -e 's|^\tunregister_blkdev(zram_major, "czram");$|\tremove_proc_entry("czraminfo", NULL);\n&|' "$Z/zram_drv.c"
[ "$(grep -c 'czram' "$Z/zram_drv.c")" -ge 6 ] || { echo "czram rename did not apply" >&2; exit 1; }
printf 'obj-m := czram.o\nczram-objs := zram_drv.o zram_sysfs.o\nccflags-y += -I%s/drivers/staging/zram\n' "$K" >"$Z/Makefile"   # for its "../zsmalloc/zsmalloc.h"
"${MK[@]}" M="$Z" modules >"$W/czram.log" 2>&1 || { tail -20 "$W/czram.log"; exit 1; }
arm-linux-gnueabihf-strip --strip-debug -o "$OUT/czram.ko" "$Z/czram.ko"

if [ $# -ge 1 ]; then
    python3 "$HERE/bootimg.py" unpack "$1" "$W/stock"
    python3 "$HERE/bootimg.py" pack "$OUT/zImage" "$W/stock/ramdisk.cpio.gz" "$OUT/boot.img"
fi
( cd "$OUT" && sha256sum zImage czram.ko ${1:+boot.img} )
