#!/bin/bash
# dev_remote.sh — see and drive Cinder on the player over adb (dev build, cable in).
#
#   tools/dev_remote.sh shot [out.png]         the screen as a PNG (default /tmp/cinder_shot.png)
#   tools/dev_remote.sh tap X Y                a tap, in screen pixels (480 x 800)
#   tools/dev_remote.sh swipe X1 Y1 X2 Y2      a drag over about 0.3 s
#   tools/dev_remote.sh key NAME               play next prev volup voldown power
#
# Needs nothing on the player: the framebuffer is read with dd and input goes in through sendevent.
# The panel speaks multitouch protocol A at twice the screen's resolution, and the app ignores a
# contact with no SYN_MT_REPORT (see the memory note "Device UI capture").
set -u
adb get-state >/dev/null 2>&1 || { echo "no adb device"; exit 1; }
T=/dev/input/event1
down() { echo "sendevent $T 1 330 1; sendevent $T 3 48 40; sendevent $T 3 50 40; sendevent $T 3 53 $(($1 * 2)); sendevent $T 3 54 $(($2 * 2)); sendevent $T 0 2 0; sendevent $T 0 0 0;"; }
UP="sendevent $T 1 330 0; sendevent $T 0 2 0; sendevent $T 0 0 0"
case "${1:-}" in
shot)
    out=${2:-/tmp/cinder_shot.png}
    raw=$(mktemp); trap 'rm -f "$raw"' EXIT      # this adbd has no exec-out: copy, then pull
    adb shell '/xbin/busybox dd if=/dev/graphics/fb0 of=/tmp/cinder_shot.raw bs=4096 count=375 2>/dev/null' \
        && adb pull /tmp/cinder_shot.raw "$raw" >/dev/null && python3 -c '
import sys, zlib, struct
d = open(sys.argv[2], "rb").read(); w, h = 480, 800
assert len(d) == w * h * 4, f"short framebuffer read: {len(d)} bytes"
rows = b"".join(b"\0" + bytes(b for i in range(y * w * 4, (y + 1) * w * 4, 4) for b in (d[i + 2], d[i + 1], d[i])) for y in range(h))
chunk = lambda t, b: struct.pack(">I", len(b)) + t + b + struct.pack(">I", zlib.crc32(t + b))
open(sys.argv[1], "wb").write(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))
' "$out" "$raw" && echo "$out" ;;
tap)   adb shell "$(down "$2" "$3") $UP" ;;
swipe)
    cmd=""
    for i in 0 1 2 3 4 5 6; do cmd+="$(down $(($2 + ($4 - $2) * i / 6)) $(($3 + ($5 - $3) * i / 6)))"; done
    adb shell "$cmd $UP" ;;
key)
    case "${2:-}" in
        play) n=4 c=28 ;; next) n=4 c=106 ;; prev) n=4 c=105 ;; voldown) n=4 c=114 ;; volup) n=4 c=115 ;;
        power) n=0 c=116 ;;
        *) echo "key: play next prev volup voldown power"; exit 2 ;;
    esac
    adb shell "sendevent /dev/input/event$n 1 $c 1; sendevent /dev/input/event$n 0 0 0; sleep 0.2; sendevent /dev/input/event$n 1 $c 0; sendevent /dev/input/event$n 0 0 0" ;;
*)  sed -n '2,7p' "$0"; exit 2 ;;
esac
