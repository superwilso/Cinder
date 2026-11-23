#!/usr/bin/env bash
# wakeups.sh — what wakes the player, per thread, over N seconds (battery work; docs/DEVICE_CHECKLIST.md
# 51.8, 51.13). Read-only: it reads /proc and changes nothing.
#
#   tools/wakeups.sh [SECONDS] [TOP]      default 60 s, top 15 threads
#
# For each thread: voluntary context switches over the window (a thread that sleeps and wakes, as
# a select/poll/usleep loop does, counts one per wake), its process (argv, which names the Sony
# service for a hagodaemon), and its name. Plus the window's total interrupts.
#
# Measure screen-off: the display keeps refreshing for 1-2 min after the screen goes dark (IRQ
# mtk_disp), so wait for that to stop before starting, or the first minute is the panel. On the
# cable, deep idle is blocked by USB, so these are wake counts, not energy.
set -uo pipefail
SECS=${1:-60}
TOP=${2:-15}
adb get-state >/dev/null 2>&1 || { echo "wakeups: no player on adb" >&2; exit 1; }
snap() {
    # tid pid voluntary_ctxt_switches comm   (one line per thread)
    adb shell 'for t in /proc/[0-9]*/task/[0-9]*; do
        v=$(grep -m1 "^voluntary_ctxt_switches" $t/status 2>/dev/null); [ -n "$v" ] || continue
        p=${t#/proc/}; p=${p%%/*}
        echo "${t##*/} $p ${v##*:} $(cat $t/comm 2>/dev/null)"
    done; echo "IRQ $(awk "NR>1{for(i=2;i<=NF;i++) if (\$i ~ /^[0-9]+$/) s+=\$i} END{print s}" /proc/interrupts)"' </dev/null | tr -d '\r'
}
names() {
    adb shell 'for p in /proc/[0-9]*; do echo "${p#/proc/} $(tr "\0" " " < $p/cmdline 2>/dev/null | cut -c1-70)"; done' </dev/null | tr -d '\r'
}
A=$(mktemp); B=$(mktemp); N=$(mktemp)
trap 'rm -f "$A" "$B" "$N"' EXIT
snap >"$A"
sleep "$SECS"
snap >"$B"
names >"$N"
irq_a=$(awk '$1=="IRQ"{print $2}' "$A"); irq_b=$(awk '$1=="IRQ"{print $2}' "$B")
echo "window ${SECS} s: $(( (irq_b - irq_a) / SECS )) interrupts/s"
echo "wakes/s  tid    process / thread"
awk -v secs="$SECS" 'FNR==NR { if ($1 != "IRQ") a[$1] = $3; next }
     $1 != "IRQ" && ($1 in a) { d = $3 - a[$1]; if (d > 0) printf "%7.1f  %-6s %s %s\n", d / secs, $1, $2, $4 }' "$A" "$B" |
    sort -rn | head -"$TOP" |
    while read -r rate tid pid comm; do
        printf '%7s  %-6s %s  [%s]\n' "$rate" "$tid" "$(awk -v p="$pid" '$1==p { $1=""; print substr($0,2); exit }' "$N")" "$comm"
    done
