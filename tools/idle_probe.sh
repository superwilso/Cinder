#!/bin/bash
# idle_probe.sh — what the player does OFF THE CABLE, which adb can never see.
#
# Every idle figure taken over adb has the cable in it: USB0 clocked, the gadget's threads running,
# adbd awake, and deep idle blocked by the port. This arms a probe on the player, then the cable is
# pulled; the probe waits for that, lets the player settle, takes two snapshots a window apart and
# leaves them in /contents/idle_probe/. Plug back in and run `idle_probe.sh read`.
#
#   tools/idle_probe.sh arm [window_s] [settle_s]    default 180 s window after 60 s settle
#   tools/idle_probe.sh read                         pull the result and print the summary
#
# Read-only on the player apart from its own output directory. It reads cumulative counters at both
# ends (never an instantaneous value), whole files only, and /proc/clkmgr/fmeter bounded with dd.
set -u
OUT=/contents/idle_probe
case "${1:-}" in
arm)
    WIN=${2:-180}; SETTLE=${3:-60}
    adb shell "mkdir -p $OUT; rm $OUT/a.* $OUT/b.* $OUT/done 2>/dev/null"
    T=$(mktemp)
    cat > "$T" <<EOF
snap() {
    cat /proc/uptime > $OUT/\$1.uptime
    cat /proc/stat > $OUT/\$1.stat
    cat /proc/interrupts > $OUT/\$1.irq
    cat /sys/power/idle_state > $OUT/\$1.idle 2>/dev/null
    cat /sys/power/dpidle_state > $OUT/\$1.dpidle 2>/dev/null
    cat /proc/clkmgr/pll_test > $OUT/\$1.pll 2>/dev/null
    cat /proc/clkmgr/subsys_test > $OUT/\$1.subsys 2>/dev/null
    dd if=/proc/clkmgr/fmeter bs=4096 count=1 > $OUT/\$1.fmeter 2>/dev/null
    cat /sys/devices/system/cpu/cpu0/cpufreq/stats/time_in_state > $OUT/\$1.freq
    cat /sys/class/power_supply/battery/voltage_now > $OUT/\$1.volt 2>/dev/null
    for t in /proc/[0-9]*/task/*; do
        echo "T \$t \$(cat \$t/comm 2>/dev/null) \$(grep '^voluntary' \$t/status 2>/dev/null | cut -f2)"
    done > $OUT/\$1.threads
}
i=0
while [ "\$(cat /sys/class/power_supply/usb/online)" != "0" ]; do
    sleep 2; i=\$((i+1)); [ \$i -gt 300 ] && exit 0      # nobody pulled the cable in 10 minutes
done
sleep $SETTLE
snap a
sleep $WIN
snap b
tail -40 /contents/cinderhome.log > $OUT/log.txt
echo ok > $OUT/done
sync
EOF
    adb push "$T" /tmp/idle_probe_dev.sh > /dev/null && rm -f "$T"
    # busybox's own sh in its own session, and a command after the `&`: with the stock shell, or with
    # the `&` last on the line, adb shell either never returns or takes the probe down with it.
    timeout 10 adb shell '/xbin/busybox setsid /xbin/busybox sh /tmp/idle_probe_dev.sh > /dev/null 2>&1 < /dev/null & sleep 1' || true
    adb shell '/xbin/busybox ps w | grep -q "[i]dle_probe_dev" && echo "probe running on the player" || echo "PROBE DID NOT START"'
    echo "armed: pull the cable now. Leave the player alone for $((SETTLE + WIN + 15)) s, then plug back in"
    echo "       and run: tools/idle_probe.sh read"
    ;;
read)
    D=$(mktemp -d)
    adb shell "ls $OUT/done" >/dev/null 2>&1 || { echo "no finished probe on the player ($OUT/done missing)"; exit 1; }
    adb pull $OUT "$D" >/dev/null 2>&1
    python3 - "$D/$(basename $OUT)" <<'EOF'
import sys, os
d = sys.argv[1]
def rd(n):
    try: return open(os.path.join(d, n), errors='replace').read()
    except OSError: return ''
ta, tb = float(rd('a.uptime').split()[0]), float(rd('b.uptime').split()[0])
w = tb - ta
print(f"window {w:.0f} s, off the cable")
def stat(n, key):
    for l in rd(n).splitlines():
        if l.startswith(key + ' '): return int(l.split()[1])
    return 0
print(f"interrupts/s {(stat('b.stat','intr')-stat('a.stat','intr'))/w:.0f}   ctxt/s {(stat('b.stat','ctxt')-stat('a.stat','ctxt'))/w:.0f}")
def irq(n):
    o = {}
    for l in rd(n).splitlines():
        p = l.split()
        if not p or not p[0].endswith(':'): continue
        c = 0; i = 1
        while i < len(p) and p[i].isdigit(): c += int(p[i]); i += 1
        o[p[0] + ' ' + ' '.join(p[i:])] = c
    return o
a, b = irq('a.irq'), irq('b.irq')
for v, k in sorted(((b[k] - a.get(k, 0)) / w, k) for k in b)[-6:][::-1]:
    print(f"  {v:8.1f}/s  {k}")
def thr(n):
    o = {}
    for l in rd(n).splitlines():
        p = l.split()
        if len(p) >= 4 and p[0] == 'T' and p[-1].isdigit(): o[(p[1], ' '.join(p[2:-1]))] = int(p[-1])
    return o
a, b = thr('a.threads'), thr('b.threads')
print("busiest threads (voluntary switches/s):")
for v, k in sorted(((b[k] - a.get(k, b[k])) / w, k) for k in b)[-10:][::-1]:
    print(f"  {v:8.1f}/s  {k[1]}")
import re
for tag in ('a', 'b'):
    m = re.search(r'dpidle_cnt\[0\]=(\d+)', rd(tag + '.idle'))
    print(f"deep idle entries at {tag}: {m.group(1) if m else '?'}")
print("PLLs on:", ', '.join(re.findall(r'\]\s*(\w+):\s*[\d.]+ MHz:\s+ON', rd('b.pll'))))
print("power domains on:", ', '.join(re.findall(r'\[(SYS_\w+)\s*\]=\[\w+\], state\(1\)', rd('b.subsys'))))
for l in rd('b.fmeter').splitlines():
    if 'WHPLL' in l or 'UNIV_48M' in l: print("fmeter:", l.strip())
print("battery mV:", rd('a.volt').strip(), "->", rd('b.volt').strip())
EOF
    echo "raw files: $D"
    ;;
*)
    sed -n '2,12p' "$0"; exit 2 ;;
esac
