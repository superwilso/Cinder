#!/usr/bin/env bash
# btsource_run.sh — Phase 4a on the player in one step each way (docs/PLAN_hardware_first.md):
# Sony's Bluetooth off through Cinder's switch, Linux's stack on the radio (btbridge, with the
# player's real address), and btsource playing the sound service's stream to the headphones.
#
#   cinder-home/tools/btsource_run.sh start AA:BB:CC:DD:EE:FF [GAIN_DB]   default gain -20 dB
#   cinder-home/tools/btsource_run.sh gain DB        live: louder/quieter, 0 = the sound service's level
#                                                    (headphones with absolute volume play at 0 dB and
#                                                    take the player's volume keys instead)
#   cinder-home/tools/btsource_run.sh log            both logs and the route hold's
#   cinder-home/tools/btsource_run.sh snoop DIR      pull the HCI capture (/tmp/cinder_bt.btsnoop, ~20 KB/s
#                                                    while streaming; /tmp is 32 MB)
#   cinder-home/tools/btsource_run.sh stop           everything back: Sony's Bluetooth on again
#
# Needs a dev build installed with --preload (the route hold, libcinder_pathwatch.so, loads into
# PathMgr at boot) and cinder-home/tools/build_bt.sh run. Keep Wi-Fi off: its compat.ko is a
# different build from the Bluetooth one, and only one can be loaded.
set -uo pipefail
cd "$(dirname "$0")/../.." || exit 1
D=/data/local/tmp/bt BB=/system/xbin/busybox
sh_() { adb shell "$1" </dev/null | tr -d '\r'; }
die() { echo "btsource_run: $*" >&2; exit 1; }

case "${1:-}" in
start)
    addr=${2:?start AA:BB:CC:DD:EE:FF [GAIN_DB]}
    gain=${3:--20}
    [ "$(sh_ 'cat /data/cinder/bootcount')" = 0 ] || die "bootcount is not 0"
    pm=$(sh_ 'for p in /proc/[0-9]*; do c=$(tr "\0" " " < $p/cmdline 2>/dev/null); case "$c" in /system/vendor/sony/bin/hagodaemon*PathMgrServiceFw*) echo ${p#/proc/};; esac; done' | head -1)
    [ -n "$pm" ] && sh_ "grep -q PathMgrServiceFw.so /proc/$pm/maps && echo yes" | grep -q yes \
        || die "the route hold is not loaded in PathMgr (pid ${pm:-?}): install with --preload, then reboot"
    sh_ 'lsmod' | grep -q '^cfg80211\|^wlan_mt' && die "Wi-Fi is on: turn it off first (devctl.sh cmd 'wifi 0')"
    sh_ "mkdir -p $D"
    for f in cinder-home/build/bt/btbridge cinder-home/build/bt/btsource wifi/out/bt/compat.ko \
             wifi/out/bt/bluetooth.ko wifi/out/bt/hci_vhci.ko; do
        [ -f "$f" ] || die "$f missing (cinder-home/tools/build_bt.sh)"
        adb push "$f" "$D/" >/dev/null || die "push $f"
    done
    sh_ "chmod 755 $D/btbridge $D/btsource"
    echo "Sony's Bluetooth off (Cinder's switch)…"
    tools/devctl.sh cmd 'bt 0' >/dev/null
    for _ in $(seq 20); do
        # Sony's stack (mtkbt) only: our own bridge from an earlier start may hold it too.
        held=$(sh_ 'n=0; for p in /proc/[0-9]*; do case "$(cat $p/cmdline 2>/dev/null)" in /tmp/btbridge*) continue;; esac; ls -l $p/fd 2>/dev/null | grep -q stpbt && n=$((n+1)); done; echo $n')
        [ "$held" = 0 ] && break
        sleep 0.5
    done
    [ "$held" = 0 ] || { tools/devctl.sh cmd 'bt 1' >/dev/null; die "Sony's stack still holds /dev/stpbt; switched back on"; }
    for m in compat bluetooth hci_vhci; do
        sh_ 'lsmod' | grep -q "^$m " && continue
        out=$(sh_ "insmod $D/$m.ko 2>&1")
        [ -z "$out" ] || die "insmod $m: $out (run 'stop' to put Sony's Bluetooth back)"
    done
    # /data is noexec: run from /tmp. adb kills a plain background job when its shell returns, so
    # each one gets its own session (setsid) and a second to start.
    sh_ "rm /tmp/cinder_bt_gain /tmp/cinder_bt_volume /tmp/cinder_bt_volume_now 2>/dev/null; cp $D/btbridge $D/btsource /tmp/ && chmod 755 /tmp/btbridge /tmp/btsource"
    run_bg() { sh_ "$BB setsid sh -c 'exec $1 > $2 2>&1' </dev/null >/dev/null 2>&1 & sleep 1"; }
    [ -n "$(sh_ 'ls -l /proc/[0-9]*/fd 2>/dev/null | grep stpbt')" ] \
        || { run_bg "$BB env CINDER_BT_SNOOP=/tmp/cinder_bt.btsnoop /tmp/btbridge -w 86400" /tmp/btbridge.log; sleep 2; }
    run_bg "/tmp/btsource $addr $gain" /tmp/btsource.log
    for _ in $(seq 30); do
        sleep 1
        l=$(sh_ 'cat /tmp/btsource.log')
        echo "$l" | grep -qE 'linked:|no saved key|refused|L2CAP 25|management socket|listen @' && break
    done
    echo "── btbridge"; sh_ 'cat /tmp/btbridge.log'
    echo "── btsource"; echo "$l"
    echo "$l" | grep -q 'linked:' || echo "(not linked yet: it retries every 5 s; 'log' to watch, 'stop' to undo)"
    ;;
gain)
    sh_ "echo ${2:?gain DB} > /tmp/cinder_bt_gain && chmod 644 /tmp/cinder_bt_gain" && echo "gain ${2} dB (applies within a second)" ;;
log)
    echo "── btsource"; sh_ 'cat /tmp/btsource.log'
    echo "── btbridge"; sh_ 'tail -20 /tmp/btbridge.log'
    echo "── route hold (PathMgr)"; sh_ 'tail -5 /tmp/cinder_pathwatch.log 2>/dev/null' ;;
snoop)
    mkdir -p "${2:?snoop DIR}" && adb pull /tmp/cinder_bt.btsnoop "$2/" >/dev/null && echo "$2/cinder_bt.btsnoop" ;;
stop)
    sh_ 'for p in /proc/[0-9]*; do case "$(cat $p/cmdline 2>/dev/null)" in /tmp/btsource*) kill ${p#/proc/};; esac; done'
    sleep 1
    sh_ 'for p in /proc/[0-9]*; do case "$(cat $p/cmdline 2>/dev/null)" in /tmp/btbridge*) kill ${p#/proc/};; esac; done'
    sleep 1
    sh_ 'rm /tmp/cinder_a2dp_target /tmp/cinder_bt_gain /tmp/cinder_bt_volume /tmp/cinder_bt_volume_now /tmp/cinder_bt.btsnoop 2>/dev/null; rmmod hci_vhci; rmmod bluetooth; rmmod compat' 2>/dev/null
    echo "Sony's Bluetooth on (Cinder's switch)…"
    tools/devctl.sh cmd 'bt 1' >/dev/null
    echo "── btsource (last lines)"; sh_ 'tail -5 /tmp/btsource.log'
    echo "modules left: $(sh_ 'lsmod' | grep -cE '^(compat|bluetooth|hci_vhci) ')" ;;
*) sed -n 2,13p "$0"; exit 2 ;;
esac
