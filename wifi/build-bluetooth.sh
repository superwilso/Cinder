#!/usr/bin/env bash
# build-bluetooth.sh — the Linux Bluetooth core and the virtual HCI driver for the NW-A50 kernel
# (docs/PLAN_hardware_first.md, Phase 4 spike), from the same MediaTek backports-3.17.1 tree as
# build.sh's Wi-Fi modules. One backports build carries both: its compat.ko differs from build.sh's,
# so load this set's compat.ko for Wi-Fi too when Bluetooth is wanted.
#
#   wifi/build-bluetooth.sh     ->  out/bt/{compat,bluetooth,hci_vhci,hci_stp}.ko (+ the Wi-Fi modules)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
KWORK="${KWORK:-$HERE/../../cinder-llusbdac/work}"
K="$KWORK/kernel"
export PATH="$KWORK/bin:$PATH"
[ -f "$K/.prepared" ] || { echo "no prepared kernel at $K (run cinder-llusbdac/build.sh, or set KWORK)" >&2; exit 1; }
W="$HERE/work"; OUT="$HERE/out/bt"
mkdir -p "$W" "$OUT"
MK=(ARCH=arm CROSS_COMPILE=arm-linux-gnueabihf-)
BP="$W/backports-bt"
[ -d "$BP" ] && mv "$BP" "$W/backports-bt.old.$$"   # never rm -rf a computed path; clean these by hand
cp -a "$K/backports-3.17.1-1" "$BP"
for p in "$HERE"/patches/*.patch; do patch -s -p1 -d "$BP" < "$p"; done
sed -i 's/^# CPTCFG_BT is not set/CPTCFG_BT=m/' "$BP/.config"
echo "CPTCFG_BT_HCIVHCI=m" >> "$BP/.config"
# Cinder's own driver for the SoC's Bluetooth core (bt/hci_stp.c, analysis/DESIGN_hci_stp.md).
cp "$HERE/bt/hci_stp.c" "$BP/drivers/bluetooth/"
echo 'obj-m += hci_stp.o' >> "$BP/drivers/bluetooth/Makefile"
{ yes "" || true; } | make -C "$BP" KLIB="$K" KLIB_BUILD="$K" "${MK[@]}" oldconfig >"$W/bt-config.log" 2>&1
make -C "$BP" KLIB="$K" KLIB_BUILD="$K" "${MK[@]}" \
    CONFIG_MTK_CONNSYS_WIFI=m CONFIG_MTK_WLAN_SUPPORT=y CONFIG_MTK_WLAN_CHIP=CONSYS -j"$(nproc)" >"$W/bt-build.log" 2>&1 \
    || { tail -30 "$W/bt-build.log"; exit 1; }
for ko in "$BP/compat/compat.ko" "$BP/net/bluetooth/bluetooth.ko" "$BP/drivers/bluetooth/hci_vhci.ko" "$BP/drivers/bluetooth/hci_stp.ko" \
          "$BP/net/wireless/cfg80211.ko" "$BP/drivers/net/wireless/mt_soc/conn_soc/mt_wifi/wlan/wlan_mt.ko"; do
    arm-linux-gnueabihf-strip --strip-debug -o "$OUT/$(basename "$ko")" "$ko"
    vm="$(modinfo -F vermagic "$OUT/$(basename "$ko")")"
    case "$vm" in "3.10.26 SMP preempt mod_unload ARMv7"*) ;; *) echo "wrong vermagic: $vm" >&2; exit 1 ;; esac
done
( cd "$OUT" && sha256sum ./*.ko | tee SHA256SUMS-modules.txt )
