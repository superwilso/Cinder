#!/usr/bin/env bash
# battery_track.sh — deploy, start, stop and fetch the on-device long-run power tracker.
#
#   tools/battery_track.sh start [interval_s]   # push + launch detached (default 60 s)
#   tools/battery_track.sh status               # is it alive, how many samples, how long
#   tools/battery_track.sh fetch [out.tsv]      # pull the log to the host
#   tools/battery_track.sh report [out.tsv]     # fetch + analyse
#   tools/battery_track.sh stop                 # kill it (the log stays)
#
# Use this for "what does a day of real use cost". Use btpower.sh for "what does THIS state cost" —
# they answer different questions and neither replaces the other.
#
# The tracker is detached with setsid so it survives the adb shell exiting. That matters: an earlier
# attempt at an on-device sampler was simply backgrounded, and adb killed the process group when its
# shell went away (see the note in btpower.sh).

set -uo pipefail
DEV_SRC="$(dirname "$0")/battery_tracker_device.sh"
DEV_PATH=/data/cinder/battery_tracker.sh
LOG=/data/cinder/battery_track.tsv
PIDFILE=/data/cinder/battery_track.pid

die() { printf ' err  %s\n' "$*" >&2; exit 1; }
ok()  { printf ' ok   %s\n' "$*"; }
info(){ printf '==> %s\n' "$*"; }

adb get-state >/dev/null 2>&1 || die "no adb device connected"

running_pid() {
    # No pidof on this device (docs/DEVICE_SHELL_GOTCHAS.md), so check the pidfile against /proc.
    local p
    p=$(adb shell "cat $PIDFILE 2>/dev/null" | tr -d '\r')
    # An EMPTY pid must fail here. `[ -d /proc/$p ]` with $p empty tests /proc, which exists, so a
    # dead tracker reported as running until this guard was added.
    case "$p" in ''|*[!0-9]*) return 1 ;; esac
    adb shell "[ -d /proc/$p ] && echo yes" 2>/dev/null | grep -q yes || return 1
    echo "$p"
}

case "${1:-}" in
start)
    interval="${2:-60}"
    [ -f "$DEV_SRC" ] || die "missing $DEV_SRC"
    if pid=$(running_pid); then
        ok "already running (pid $pid) — leaving it alone. 'stop' first to restart."
        exit 0
    fi
    info "pushing tracker"
    adb shell 'mkdir -p /data/cinder' >/dev/null 2>&1
    adb push "$DEV_SRC" "$DEV_PATH" >/dev/null 2>&1 || die "push failed"
    adb shell "chmod 755 $DEV_PATH" >/dev/null 2>&1
    info "launching detached (interval ${interval}s)"
    # THREE THINGS HAVE TO BE RIGHT HERE, and each cost a debugging round:
    #
    #  1. /data IS noexec (so is /contents; only /tmp is executable, and /tmp is tmpfs and dies on
    #     reboot). So the script cannot be exec'd from where it lives — it is fed to an interpreter
    #     instead, which noexec does not block. Its shebang is therefore decorative.
    #  2. `setsid` is not on PATH. It exists only as a busybox applet, hence `busybox setsid`.
    #  3. setsid + a full redirect of all three fds. Backgrounding alone is not enough: adb kills
    #     the process group when its shell exits, which is how an earlier on-device sampler died
    #     (see the note in btpower.sh).
    #  4. The trailing `sleep 2` is not padding. Without it adb tears the session down before
    #     setsid has finished establishing the new one, and the tracker dies with no output at all
    #     — which looks exactly like the script being broken, and is not.
    adb shell "INTERVAL=$interval busybox setsid /system/xbin/busybox sh $DEV_PATH >/dev/null 2>&1 < /dev/null & sleep 2" >/dev/null 2>&1
    sleep 2
    if pid=$(running_pid); then
        ok "tracker running (pid $pid), appending to $LOG"
        echo
        echo "  Now UNPLUG and use the player normally. Plug back in whenever you like —"
        echo "  charging periods are recorded, not avoided, and the analyser splits on them."
        echo "  Check in with:  tools/battery_track.sh status"
    else
        die "tracker did not stay up — run 'adb shell sh $DEV_PATH' by hand to see why"
    fi
    ;;
status)
    if pid=$(running_pid); then ok "running (pid $pid)"; else printf ' --   not running\n'; fi
    n=$(adb shell "grep -c . $LOG 2>/dev/null" | tr -d '\r')
    [ -n "$n" ] && [ "$n" != "0" ] || { info "no samples yet"; exit 0; }
    info "$((n - 1)) sample(s)"
    adb shell "head -2 $LOG; echo ...; tail -2 $LOG" 2>/dev/null | tr -d '\r'
    ;;
fetch|report)
    out="${2:-battery_track.tsv}"
    adb pull "$LOG" "$out" >/dev/null 2>&1 || die "no log on device yet"
    ok "pulled $(wc -l < "$out") line(s) -> $out"
    [ "$1" = "report" ] && python3 "$(dirname "$0")/battery_track_report.py" "$out"
    ;;
stop)
    if pid=$(running_pid); then
        adb shell "kill $pid" >/dev/null 2>&1
        ok "stopped (pid $pid). Log kept at $LOG"
    else
        printf ' --   not running\n'
    fi
    ;;
*)
    sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'
    ;;
esac
