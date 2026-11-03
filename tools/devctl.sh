#!/usr/bin/env bash
# devctl.sh — drive a dev-channel player over adb without looking at it. Nothing is installed:
# buttons and touches are injected with sendevent, so Cinder reads them as if pressed.
#
#   tools/devctl.sh key play|next|prev|volup|voldn|power   one press
#   tools/devctl.sh tap X Y                                screen coordinates, 480x800
#   tools/devctl.sh swipe X0 Y0 X1 Y1                      one drag
#   tools/devctl.sh shot [out.png]                         cinder-home/tools/screenshot.sh
#   tools/devctl.sh state                                  playing?, codec, screen, suspend
#   tools/devctl.sh log [N]                                last N lines of cinderhome.log
#   tools/devctl.sh sh 'CMD'                               run CMD, exit with ITS code (adb shell's is always 0)
#   tools/devctl.sh probe ARGS...                          push and run cinder-probe (dev build)
#   tools/devctl.sh hci on|off|pull DIR                    Sony's HCI trace (btsnoop) in /tmp
#
# /tmp is a 32 MB RAM disk and the HCI trace grows ~130 KB/s on LDAC (~40 KB/s on AAC): it fills
# /tmp in about four minutes, and a full /tmp breaks Cinder's own status files. Pull and delete
# often; `hci off` only closes it if cinder-probe fits in /tmp, so turn it off BEFORE it fills.
# Touch is protocol A: without SYN_MT_REPORT the app ignores the contact. The panel's raw range
# is 2x the screen. Never inject code 35 (the Hold switch).
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
TOUCH=/dev/input/event1 KEYS=/dev/input/event4 PWR=/dev/input/event0
BB=/system/xbin/busybox
LIBS=/system/vendor/sony/lib:/system/vendor/cinder/lib:/system/lib
sh_() { adb shell "$1" </dev/null; }

case "${1:-}" in
key)
    case "${2:-}" in
        play) c=28 ;; next) c=106 ;; prev) c=105 ;; volup) c=115 ;; voldn) c=114 ;; power) c=116 ;;
        *) echo "key: play|next|prev|volup|voldn|power" >&2; exit 2 ;;
    esac
    n=$KEYS; [ "$c" = 116 ] && n=$PWR
    sh_ "sendevent $n 1 $c 1; sendevent $n 0 0 0; sendevent $n 1 $c 0; sendevent $n 0 0 0" ;;
tap)
    x=$(( $2 * 2 )) y=$(( $3 * 2 ))
    sh_ "sendevent $TOUCH 1 330 1; sendevent $TOUCH 3 48 40; sendevent $TOUCH 3 50 40; sendevent $TOUCH 3 53 $x; sendevent $TOUCH 3 54 $y; sendevent $TOUCH 0 2 0; sendevent $TOUCH 0 0 0; sendevent $TOUCH 3 48 0; sendevent $TOUCH 0 2 0; sendevent $TOUCH 1 330 0; sendevent $TOUCH 0 0 0" ;;
swipe)
    x0=$(($2*2)) y0=$(($3*2)) x1=$(($4*2)) y1=$(($5*2)) n=12
    cmd="sendevent $TOUCH 1 330 1;"
    for ((i=0;i<=n;i++)); do
        cmd+=" sendevent $TOUCH 3 48 40; sendevent $TOUCH 3 50 40; sendevent $TOUCH 3 53 $(( x0+(x1-x0)*i/n )); sendevent $TOUCH 3 54 $(( y0+(y1-y0)*i/n )); sendevent $TOUCH 0 2 0; sendevent $TOUCH 0 0 0;"
    done
    sh_ "$cmd sendevent $TOUCH 3 48 0; sendevent $TOUCH 0 2 0; sendevent $TOUCH 1 330 0; sendevent $TOUCH 0 0 0" ;;
shot) shift; exec bash cinder-home/tools/screenshot.sh "$@" ;;
state)
    # mtkbt only uses CPU while A2DP is streaming, so its tick count moving = playing over BT.
    sh_ 'p=$(ps | grep "[/]bin/mtkbt" | '"$BB"' awk "{print \$2}"); a=$(cut -d" " -f14 /proc/$p/stat); sleep 2; b=$(cut -d" " -f14 /proc/$p/stat)
         echo "uptime $(cut -d" " -f1 /proc/uptime)   bt streaming: $([ "$b" != "$a" ] && echo yes || echo no)"
         grep -E "bt-sound:|screen: |suspend: (idle|Power|screen)|bt-codec:" /contents/cinderhome.log | tail -4
         echo "aac: $(cat /tmp/cinder_aac_status 2>/dev/null)"; tail -2 /tmp/cinder_aac.log' ;;
log) sh_ "tail -${2:-30} /contents/cinderhome.log" ;;
sh)
    # Sony's adbd predates adb's shell protocol, so `adb shell` exits 0 whatever the command did.
    # Ask the player for the code on a last line of its own, then strip it. The command runs in a
    # subshell so its own `exit N` still reaches the marker.
    shift
    out=$(sh_ "( $* ); printf '\n__rc=%d' \$?" | tr -d '\r')
    case "$out" in
        *__rc=*) rc=${out##*__rc=}; out=${out%__rc=*}; out=${out%$'\n'} ;;
        *) echo "sh: no exit code from the player (is adb connected?)" >&2; rc=255 ;;
    esac
    printf '%s' "$out"
    exit "$rc" ;;
probe)
    shift
    adb push cinder-home/dist/dev/cinder-probe /tmp/cinder-probe >/dev/null || exit 1
    sh_ "chmod 755 /tmp/cinder-probe; cd /tmp; LD_LIBRARY_PATH=$LIBS $BB timeout -t 60 /tmp/cinder-probe $* 2>&1" ;;
hci)
    case "${2:-}" in
        on|off) "$0" probe --btlink hci "$2" | grep -E "SetHci|sniffer" ;;
        pull) mkdir -p "${3:?hci pull DIR}"
              for f in $(sh_ 'ls /tmp/hci_sniffer_log_*.cfa 2>/dev/null' | tr -d '\r'); do adb pull "$f" "$3/" >/dev/null && echo "$3/$(basename "$f")"; done ;;
        *) echo "hci: on|off|pull DIR" >&2; exit 2 ;;
    esac ;;
*) sed -n 2,14p "$0"; exit 2 ;;
esac
