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
#   tools/devctl.sh wait PATTERN [SECONDS] [AFTER]        block until cinderhome.log has a line matching
#                                                          PATTERN (grep -E) after line AFTER (default: now);
#                                                          exit 1 on timeout (default 60 s). adb shell's own
#                                                          exit code is always 0, so `until adb shell grep` never waits
#   tools/devctl.sh latency [KEY]                          press KEY (default play) and time it ON the player:
#                                                          press -> first PCM stream RUNNING, /proc/uptime (10 ms)
#   tools/devctl.sh wake                                   light the screen if it is dark (Power only then — a
#                                                          blind Power press toggles it OFF on a lit screen)
#   tools/devctl.sh stalls [N]                             dev build: the last N render-loop stalls
#                                                          (> 50 ms, screen on) and slow housekeeping
#   tools/devctl.sh sh 'CMD'                               run CMD, exit with ITS code (adb shell's is always 0)
#   tools/devctl.sh probe ARGS...                          push and run cinder-probe (dev build)
#   tools/devctl.sh hci on|off|pull DIR                    Sony's HCI trace (btsnoop) in /tmp
#   tools/devctl.sh set [PATTERN | KEY=VALUE… | restore | adv …]   settings, live (tools/devset.sh)
#   tools/devctl.sh cmd 'CMD' ['CMD'…]                     playback with no screen (dev build): state,
#                                                          play, pause, next, prev, seek MS, repeat 0-3, shuffle 0|1,
#                                                          find TEXT, album ID|TEXT [SONG] (replaces the queue),
#                                                          restore (reload /tmp/cinder_restore_*.conf)
#   tools/devctl.sh queue save|restore DIR                 back up the queue; put it back with no reboot
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
set) shift; exec bash tools/devset.sh "$@" ;;
cmd) shift
     [ $# -gt 0 ] || { echo "cmd: state | play | pause | toggle | next | prev | seek MS | repeat 0-3 | shuffle 0|1 | find TEXT | album ID|TEXT [SONG] | restore | reload | radio N (-1 stops) | fm 0|1 | wifi 0|1" >&2; exit 2; }
     # One command a line; the shell answers in cinder_cmd.out within its 1 s tick.
     body=$(printf '%s\n' "$@")
     # adb's shell writes 0600 root: cinder-home (uid 100) must be able to read it.
     sh_ "rm /tmp/cinder_cmd.out 2>/dev/null; printf '%s\n' '$body' > /tmp/cinder_cmd; chmod 644 /tmp/cinder_cmd"
     # 10 s: behind a dark screen the shell's tick slows and an answer can take over 5 s.
     for _ in $(seq 20); do
         sleep 0.5
         out=$(sh_ 'cat /tmp/cinder_cmd.out 2>/dev/null' | tr -d '\r')
         [ -n "$out" ] && { printf '%s\n' "$out"; exit 0; }
     done
     echo "no answer in 10 s (a stable build, or cinder-home not running?)" >&2; exit 1 ;;
queue)
    # Back up the owner's queue before a test that replaces it; put it back with no reboot.
    d=${3:?"queue save|restore DIR"}
    case "${2:-}" in
        save) mkdir -p "$d" && adb pull /data/cinder/queue.conf "$d/" && adb pull /data/cinder/resume.conf "$d/" ;;
        restore)
            # cinder-home copies these over the live files itself (its own save would race an adb copy).
            adb push "$d/queue.conf" /tmp/cinder_restore_queue.conf && adb push "$d/resume.conf" /tmp/cinder_restore_resume.conf || exit 1
            sh_ "chmod 644 /tmp/cinder_restore_queue.conf /tmp/cinder_restore_resume.conf"
            exec "$0" cmd restore ;;
        *) echo "queue save|restore DIR" >&2; exit 2 ;;
    esac ;;
state)
    # mtkbt only uses CPU while A2DP is streaming, so its tick count moving = playing over BT.
    sh_ 'p=$(ps | grep "[/]bin/mtkbt" | '"$BB"' awk "{print \$2}"); a=$(cut -d" " -f14 /proc/$p/stat); sleep 2; b=$(cut -d" " -f14 /proc/$p/stat)
         echo "uptime $(cut -d" " -f1 /proc/uptime)   bt streaming: $([ "$b" != "$a" ] && echo yes || echo no)"
         grep -E "bt-sound:|screen: |suspend: (idle|Power|screen)|bt-codec:" /contents/cinderhome.log | tail -4
         echo "aac: $(cat /tmp/cinder_aac_status 2>/dev/null)"; tail -2 /tmp/cinder_aac.log' ;;
log) sh_ "tail -${2:-30} /contents/cinderhome.log" ;;
wait)
    pat="$2"; secs="${3:-60}"
    from="${4:-$(adb shell 'wc -l < /contents/cinderhome.log' | tr -d '\r ')}"
    end=$(( $(date +%s) + secs ))
    while [ "$(date +%s)" -lt "$end" ]; do
        hit="$(adb shell "tail -n +$((from + 1)) /contents/cinderhome.log" 2>/dev/null | tr -d '\r' | grep -aE -m1 -- "$pat")" || true
        [ -n "$hit" ] && { echo "$hit"; exit 0; }
        sleep 1
    done
    echo "wait: no line matching '$pat' within ${secs} s" >&2; exit 1 ;;
latency)
    case "${2:-play}" in play) c=28 ;; next) c=106 ;; prev) c=105 ;; *) echo "latency: play|next|prev" >&2; exit 2 ;; esac
    sh_ "t0=\$(cut -d' ' -f1 /proc/uptime); sendevent $KEYS 1 $c 1; sendevent $KEYS 0 0 0; sendevent $KEYS 1 $c 0; sendevent $KEYS 0 0 0
         i=0; while [ \$i -lt 1000 ]; do grep -l RUNNING /proc/asound/card0/pcm*p/sub0/status >/dev/null 2>&1 && break; i=\$((i+1)); done
         t1=\$(cut -d' ' -f1 /proc/uptime); echo \"press at \$t0, stream RUNNING at \$t1 (\$(grep -l RUNNING /proc/asound/card0/pcm*p/sub0/status 2>/dev/null | head -1))\"" ;;
wake)
    last="$(adb shell 'grep -a "input: touch \(WAKE\|SLEEP\)" /contents/cinderhome.log | tail -1' | tr -d '\r')"
    case "$last" in *SLEEP*) ;; *) echo "screen: already on"; exit 0 ;; esac
    from="$(adb shell 'wc -l < /contents/cinderhome.log' | tr -d '\r ')"
    "$0" key power >/dev/null
    "$0" wait 'touch WAKE' 10 "$from" >/dev/null && echo "screen: woken" ;;
stalls) sh_ "grep -a \"stall: \" /contents/cinderhome.log | tail -${2:-30}" ;;
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
