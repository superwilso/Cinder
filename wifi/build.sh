#!/usr/bin/env bash
# build.sh — the four kernel modules that give the NW-A50 series Wi-Fi, from Sony's own GPL tree.
#
#   ./build.sh            -> out/{compat,cfg80211,mtk_wmt_wifi_soc,wlan_mt}.ko
#
# Needs a PREPARED copy of Sony's NW-A57 kernel source (linux-kernel-3.10.26, configured with
# BBDMP5_linux_debug_defconfig, `make prepare scripts` done) and GCC 4.9 for ARM. cinder-llusbdac's
# build.sh makes exactly that pair, pinned by hash; point KWORK at its work/ folder (default: the
# sibling checkout). Its .config matches the player's /proc/config.gz line for line.
#
# What it builds, all already in Sony's tree and switched off in the player's kernel:
#   backports-3.17.1-1/                 compat.ko, cfg80211.ko, and MediaTek's CONSYS Wi-Fi driver
#                                       (wlan_mt.ko), which knows the MT8590 by chip id
#   drivers/misc/mediatek/conn_soc/.../wmt_chrdev_wifi.c
#                                       mtk_wmt_wifi_soc.ko: /dev/wmtWifi, power on/off through WMT
# patches/ holds the only changes; each says why.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
KWORK="${KWORK:-$HERE/../../cinder-llusbdac/work}"
K="$KWORK/kernel"
export PATH="$KWORK/bin:$PATH"
[ -f "$K/.prepared" ] || { echo "no prepared kernel at $K (run cinder-llusbdac/build.sh, or set KWORK)" >&2; exit 1; }
W="$HERE/work"; OUT="$HERE/out"
mkdir -p "$W" "$OUT"
MK=(ARCH=arm CROSS_COMPILE=arm-linux-gnueabihf-)

echo "== backports + MediaTek wlan"
BP="$W/backports"
[ -d "$BP" ] && mv "$BP" "$W/backports.old.$$"   # never rm -rf a computed path; clean these by hand
cp -a "$K/backports-3.17.1-1" "$BP"
for p in "$HERE"/patches/*.patch; do echo "applying $(basename "$p")"; patch -s -p1 -d "$BP" < "$p"; done
# The wlan directory is gated on the KERNEL's config (CONFIG_, not backports' CPTCFG_), which Sony
# left off; say it is on for this build only.
make -C "$BP" KLIB="$K" KLIB_BUILD="$K" "${MK[@]}" \
    CONFIG_MTK_CONNSYS_WIFI=m CONFIG_MTK_WLAN_SUPPORT=y CONFIG_MTK_WLAN_CHIP=CONSYS -j"$(nproc)" >"$W/backports.log" 2>&1 \
    || { tail -30 "$W/backports.log"; exit 1; }

echo "== mtk_wmt_wifi_soc"
C="$K/drivers/misc/mediatek/conn_soc"
M="$W/wmtwifi"; mkdir -p "$M"
cp "$C/common/linux/pub/wmt_chrdev_wifi.c" "$M/"
cat >"$M/Makefile" <<EOF
obj-m := mtk_wmt_wifi_soc.o
mtk_wmt_wifi_soc-objs := wmt_chrdev_wifi.o
ccflags-y += -I$C/common/linux/include -I$C/common/linux/pri/include -I$C/common/core/include
ccflags-y += -I$C/common/include -I$C/common/linux -I$C/common -I$C/../common_detect -I$C/include
# The player's kernel exports the ABSTRACT WMT API (functions take a chip type); without this the
# header declares the one-argument forms and the calls pass garbage for it.
ccflags-y += -DWMT_CREATE_NODE_DYNAMIC=1 -DMTK_WCN_WMT_STP_EXP_SYMBOL_ABSTRACT
EOF
make -C "$K" "${MK[@]}" M="$M" modules >"$W/wmtwifi.log" 2>&1 || { tail -30 "$W/wmtwifi.log"; exit 1; }

echo "== out/"
for ko in "$BP/compat/compat.ko" "$BP/net/wireless/cfg80211.ko" "$M/mtk_wmt_wifi_soc.ko" \
          "$BP/drivers/net/wireless/mt_soc/conn_soc/mt_wifi/wlan/wlan_mt.ko"; do
    arm-linux-gnueabihf-strip --strip-debug -o "$OUT/$(basename "$ko")" "$ko"
    vm="$(modinfo -F vermagic "$OUT/$(basename "$ko")")"
    case "$vm" in "3.10.26 SMP preempt mod_unload ARMv7"*) ;; *) echo "wrong vermagic: $vm" >&2; exit 1 ;; esac
done
( cd "$OUT" && rm -f SHA256SUMS.ko && sha256sum ./*.ko | tee SHA256SUMS-modules.txt )
