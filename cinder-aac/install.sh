#!/bin/bash
# install.sh — puts cinder-aac on a player over adb (root adb, Cinder's hagodaemon wrapper in place).
#
#   ./install.sh <cinder-hagowrap>   install: the library, the wrapper that loads it, AAC switched on
#   ./install.sh off                 AAC off (the library stays, every hook passes through)
#   ./install.sh xq | xq-off         SBC XQ (Dual Channel, bitpool 47) on / off
#   ./install.sh remove              library and switch removed; the wrapper stays (it is Cinder's)
#
# <cinder-hagowrap> is a Cinder build with the per-service preload rule (src/cinder-hagowrap.c,
# "ANY SERVICE can be given a preload"). A new wrapper build goes on trial under Cinder's boot
# guard: play five seconds of audio on the first boot, or the guard puts Sony's hagodaemon back.
# Everything takes effect at the next reboot.
set -eu
cd "$(dirname "$0")"
LIB=build/libcinder_aac.so
DST=/system/vendor/unknown321/lib/preload/BtCommonService.so
HAGO=/system/vendor/sony/bin/hagodaemon
FDK=build/libfdk-aac-enc.so                       # optional: ./build_fdk.sh
FDKDST=/system/vendor/unknown321/lib/libfdk-aac-enc.so

dev() { adb shell "$1" | tr -d '\r'; }

case "${1:-}" in
xq)     dev 'touch /data/cinder/bt_sbc_xq && chmod 644 /data/cinder/bt_sbc_xq && echo "SBC XQ on from the next reboot"'; exit ;;
xq-off) dev 'rm /data/cinder/bt_sbc_xq 2>/dev/null; echo "SBC XQ off from the next reboot"'; exit ;;
off)    dev 'rm /data/cinder/bt_aac 2>/dev/null; echo "AAC off from the next reboot"'; exit ;;
remove) dev "mount -o remount,rw /system && rm $DST; rm $FDKDST 2>/dev/null; sync; mount -o remount,ro /system; rm /data/cinder/bt_aac /data/cinder/bt_sbc_xq /data/cinder/bt_aptxhd 2>/dev/null; echo removed"; exit ;;
"")     echo "usage: install.sh <cinder-hagowrap> | off | remove"; exit 2 ;;
esac

WRAP=$1
[ -s "$LIB" ] || { echo "build first: ./build.sh"; exit 1; }
grep -aq cinder-hagowrap "$WRAP" || { echo "$WRAP is not a cinder-hagowrap build"; exit 1; }
grep -aq unknown321/lib/preload/ "$WRAP" || { echo "$WRAP has no per-service preload rule (too old)"; exit 1; }
[ "$(dev "[ -s $HAGO.real ] && echo y")" = y ] || { echo "no $HAGO.real: Cinder's preload component is not installed"; exit 1; }

adb push "$LIB" /data/local/tmp/cinder_aac.so >/dev/null
adb push "$WRAP" /data/local/tmp/cinder-hagowrap.new >/dev/null
if [ -s "$FDK" ]; then adb push "$FDK" /data/local/tmp/fdk.so >/dev/null; else echo "no $FDK: the firmware encoder is used"; fi
dev 'BB=/xbin/busybox; [ -x $BB ] || BB=/system/xbin/busybox
put() { $BB cat "$1" > "$2.tmp" && $BB cmp -s "$1" "$2.tmp" && $BB chown $4 "$2.tmp" && $BB chmod $3 "$2.tmp" && $BB mv "$2.tmp" "$2" && echo "installed $2"; }
mount -o remount,rw /system || exit 1
$BB mkdir -p /system/vendor/unknown321/lib/preload
# adb shell makes folders 0700; the service runs as uid 100 and must see in.
$BB chmod 755 /system/vendor/unknown321/lib/preload
put /data/local/tmp/cinder_aac.so '"$DST"' 644 0:0
put /data/local/tmp/cinder-hagowrap.new '"$HAGO"' 755 0:2000
[ -s /data/local/tmp/fdk.so ] && put /data/local/tmp/fdk.so '"$FDKDST"' 644 0:0
sync; mount -o remount,ro /system
rm /data/local/tmp/cinder_aac.so /data/local/tmp/cinder-hagowrap.new /data/local/tmp/fdk.so 2>/dev/null
touch /data/cinder/bt_aac && chmod 644 /data/cinder/bt_aac && echo "AAC on from the next reboot"'
