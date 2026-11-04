#!/system/bin/sh
# wifi-down.sh [DIR] [RUN] — leave the network and power the Wi-Fi function off. The modules stay loaded
# (unloading the driver is the path least exercised upstream; a reboot unloads them anyway).
D="${1:-/tmp/wifi}"
RUN="${2:-/tmp/cinder-wifi}"
"$D/wpa_cli" -p "$RUN/ctrl" -i wlan0 terminate >/dev/null 2>&1
[ -f "$RUN/udhcpc.pid" ] && kill "$(cat "$RUN/udhcpc.pid")" 2>/dev/null
busybox ifconfig wlan0 down 2>/dev/null
echo 0 > /dev/wmtWifi 2>/dev/null
echo "wifi-down: off"
