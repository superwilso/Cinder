#!/bin/bash
# ssfw_cost.sh — where the sound service's CPU goes while a track plays. Cable in, music playing.
#
#   tools/ssfw_cost.sh [seconds]        default 30
#
# Read-only. Prints, for the busiest thread of SoundServiceFw: user and kernel time over the window,
# a perf profile by library and symbol if the kernel allows it, the filter configuration the service
# logged, the open PCM's hardware parameters and the deep-idle switch. See analysis/RE_sound_service.md.
set -u
S=${1:-30}
adb get-state >/dev/null 2>&1 || { echo "no adb device"; exit 1; }
T=$(mktemp); trap 'rm -f "$T"' EXIT
cat > "$T" <<'EOF'
PID=$(for p in /proc/[0-9]*; do case "$(tr '\0' ' ' < $p/cmdline 2>/dev/null)" in /system/vendor/sony/bin/hagodaemon\ SoundServiceFw*) echo ${p##*/};; esac; done | head -1)
[ -z "$PID" ] && { echo "SoundServiceFw not found"; exit 1; }
snap() { for t in /proc/$PID/task/*; do set -- $(cat $t/stat 2>/dev/null); echo "${t##*/} ${14} ${15}"; done; }
snap > /tmp/ssfw.a; sleep $S; snap > /tmp/ssfw.b
echo "== threads of SoundServiceFw (pid $PID), busiest first: tid, user %, kernel % of one core over ${S} s"
/xbin/busybox awk -v s=$S 'NR==FNR{u[$1]=$2;k[$1]=$3;next} {du=$2-u[$1]; dk=$3-k[$1]; if(du+dk>0) printf "%s %.2f %.2f\n",$1,du/s,dk/s}' /tmp/ssfw.a /tmp/ssfw.b | sort -k2 -rn | head -5 > /tmp/ssfw.top
cat /tmp/ssfw.top
TID=$(head -1 /tmp/ssfw.top | cut -d' ' -f1)
echo "== perf, thread $TID, 15 s"
if perf record -t $TID -o /tmp/ssfw.perf sleep 15 >/dev/null 2>/tmp/ssfw.perr; then
    perf report -i /tmp/ssfw.perf --sort dso,symbol 2>/dev/null | grep -v '^#' | grep . | head -40
else
    echo "perf failed: $(head -2 /tmp/ssfw.perr)"
fi
echo "== filter configuration the service last logged"
logcat -d 2>/dev/null | grep -E 'Configuration:|Enabled Filters|Filter: |Created FilterChain|OperationMode|is_proc_ updated' | tail -30
echo "== open PCMs"
for d in /proc/asound/card0/pcm*p/sub0; do st=$(head -1 $d/status); [ "$st" = closed ] || { echo "$d: $st"; cat $d/hw_params; }; done
echo "== deep idle"
grep -E 'switch|mode|dpidle_cnt|block_cnt' /sys/power/dpidle_state | head -12
echo "== suspend parameters"; for f in /sys/module/*/parameters/slp_*; do echo "$f $(cat $f 2>/dev/null)"; done
echo "== cpufreq"; cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_min_freq /sys/devices/system/cpu/cpu0/cpufreq/scaling_max_freq
EOF
adb push "$T" /data/local/tmp/ssfw_cost.sh >/dev/null && adb shell "S=$S sh /data/local/tmp/ssfw_cost.sh"
