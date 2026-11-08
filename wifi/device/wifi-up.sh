#!/system/bin/sh
# wifi-up.sh [DIR] [CONF] [RUN] — bring Wi-Fi up on an NW-A50-series player, as root.
#
#   DIR    holds the four modules, wpa_supplicant, wpa_cli and dhcp.sh   (default /tmp/wifi)
#   CONF   wpa_supplicant's network list                                  (default DIR/wpa.conf)
#   RUN    writable state: control socket, DHCP pid, DNS                 (default /tmp/cinder-wifi;
#          DIR may be read-only, as /system/vendor/cinder/wifi is)
#
# Loads compat, cfg80211, mtk_wmt_wifi_soc and wlan_mt (once per boot), powers the Wi-Fi function
# through WMT, gives the interface this player's own address, joins, and leaves udhcpc running for
# renewals. Exit 0 = associated and leased; the last line says what failed otherwise.
D="${1:-/tmp/wifi}"
CONF="${2:-$D/wpa.conf}"
RUN="${3:-/tmp/cinder-wifi}"
mkdir -p "$RUN" && chmod 700 "$RUN"
export WIFI_RUN="$RUN"   # dhcp.sh reads it
say() { echo "wifi-up: $*"; }

# The driver has no calibration file, so it would make up 00:08:22 plus three bytes of the boot
# clock — a new address on every power-on. Give it one derived from this player's eMMC CID instead:
# stable across boots, different per player, and locally administered (02:...), so it can never
# clash with a manufacturer's. wlan_mt takes it as `mac=` (patches/0002), so it is fixed per boot.
cid="$(cat /sys/block/mmcblk0/device/cid 2>/dev/null)"
mac=""
[ -n "$cid" ] && mac="02:$(echo "$cid" | busybox md5sum | cut -c1-10 | busybox sed 's/\(..\)/\1:/g; s/:$//')"
if ! grep -q '^wlan_mt ' /proc/modules; then
    for m in compat cfg80211 mtk_wmt_wifi_soc; do
        grep -q "^$m " /proc/modules || insmod "$D/$m.ko" || { say "insmod $m failed"; exit 1; }
    done
    insmod "$D/wlan_mt.ko" ${mac:+mac=$mac} || { say "insmod wlan_mt failed"; exit 1; }
fi
echo 1 > /dev/wmtWifi || { say "WMT would not power Wi-Fi"; exit 1; }
i=0; while [ ! -e /sys/class/net/wlan0 ] && [ $i -lt 20 ]; do sleep 1; i=$((i + 1)); done
[ -e /sys/class/net/wlan0 ] || { say "no wlan0 after 20 s"; exit 1; }

busybox ifconfig wlan0 up || { say "wlan0 would not come up"; exit 1; }

[ -f "$CONF" ] || { say "no network list at $CONF"; exit 1; }
"$D/wpa_cli" -p "$RUN/ctrl" -i wlan0 status >/dev/null 2>&1 \
    || "$D/wpa_supplicant" -B -D nl80211 -i wlan0 -c "$CONF" >/dev/null   # CONF names RUN/ctrl
# Nothing saved yet: up for Select network to scan, nothing to join.
grep -q 'network=' "$CONF" || { say "up: no saved network"; exit 0; }
i=0; while [ $i -lt 30 ]; do
    "$D/wpa_cli" -p "$RUN/ctrl" -i wlan0 status 2>/dev/null | grep -q '^wpa_state=COMPLETED' && break
    sleep 1; i=$((i + 1))
done
[ $i -lt 30 ] || { say "did not join a network in 30 s"; exit 1; }

# 802.11 power save: the radio sleeps between beacons while nothing is sent, which is what phones
# do. The driver starts with it off. With it on, internet radio streamed 58 s of 58 without a stall
# (2026-11-05); the battery saving itself is not yet measured.
"$D/iw" dev wlan0 set power_save on 2>/dev/null

busybox udhcpc -i wlan0 -b -p "$RUN/udhcpc.pid" -t 8 -s "$D/dhcp.sh" >/dev/null 2>&1
i=0; while ! busybox ifconfig wlan0 | grep -q 'inet addr' && [ $i -lt 15 ]; do sleep 1; i=$((i + 1)); done
busybox ifconfig wlan0 | grep -q 'inet addr' || { say "joined, but no address from DHCP"; exit 1; }
say "up: $(busybox ifconfig wlan0 | busybox sed -n 's/.*inet addr:\([0-9.]*\).*/\1/p') on $(busybox cat /sys/class/net/wlan0/address)"
