#!/system/xbin/busybox sh
# battery_tracker_device.sh — long-run power tracker. Runs ON THE DEVICE, detached, for days.
#
# WHY THIS EXISTS ALONGSIDE btpower.sh. btpower.sh answers "what does THIS state cost" by taking two
# samples around a window you deliberately spend unplugged. That is the right tool for an A/B, and
# the wrong one for "what does a day of actual use cost" — you cannot hold the player in one state
# for a day, and every `adb` command to check on it wakes the core (598 MHz idle reads 1.3 GHz the
# moment a shell attaches) and pins the gauge to charging.
#
# So this samples itself, on a timer, with nobody attached, and writes one line per sample. The host
# reads the file afterwards and does the arithmetic. Charging periods are RECORDED, not avoided —
# the analyser splits on the status field, which is the only honest way to handle a device you are
# actually using.
#
# COST. Every field is a sysfs or procfs read; no binder IPC, no probe, no service calls. One sample
# is a few file reads and one append. At the default 60 s that is ~1440 lines/day, ~200 KB.
#
# WHERE IT WRITES. /data/cinder — ext4, and NOT handed to the PC by USB-MSC. /tmp is tmpfs and dies
# on reboot; /contents is FAT and disappears under the host while mass storage is mounted, which is
# exactly when you least want the log to vanish.
#
# A REBOOT STOPS IT. There is deliberately no boot hook: this is a diagnostic, and nothing
# diagnostic belongs on the boot path of a device whose escape ladder is the only way back. Restart
# it with `tools/battery_track.sh start`; it APPENDS, so a reboot is a gap in the series, not a
# lost run.

OUT=${OUT:-/data/cinder/battery_track.tsv}
INTERVAL=${INTERVAL:-60}
PIDFILE=/data/cinder/battery_track.pid

mkdir -p /data/cinder 2>/dev/null
echo $$ > "$PIDFILE" 2>/dev/null

# Header only once, so restarting after a reboot appends to the same series.
if [ ! -s "$OUT" ]; then
    echo "# cinder battery track — epoch uptime capacity voltage_uv status usb backlight cpu_idle cpu_total pcm home_jiffies" > "$OUT"
fi

r() { cat "$1" 2>/dev/null; }

while : ; do
    EPOCH=$(date +%s 2>/dev/null)
    UP=$(cut -d" " -f1 /proc/uptime 2>/dev/null)
    CAP=$(r /sys/class/power_supply/battery/capacity)
    VOLT=$(r /sys/class/power_supply/battery/voltage_now)
    STAT=$(r /sys/class/power_supply/battery/status)
    USB=$(r /sys/class/power_supply/usb/online)
    BL=$(r /sys/class/leds/lcd-backlight/brightness)

    # /proc/stat's cpu line: user nice system idle iowait irq softirq ...
    # Cumulative jiffies, so the analyser differences consecutive samples. Idle+iowait is "not busy".
    # shellcheck disable=SC2046
    set -- $(grep -m1 '^cpu ' /proc/stat 2>/dev/null)
    CPU_IDLE=$(( ${5:-0} + ${6:-0} ))
    CPU_TOTAL=$(( ${2:-0} + ${3:-0} + ${4:-0} + ${5:-0} + ${6:-0} + ${7:-0} + ${8:-0} ))

    # Which ALSA substreams are open, if any. This is what separates "playing" from "idle" after the
    # fact without having to ask any service.
    PCM=""
    for f in /proc/asound/card*/pcm*/sub*/status; do
        s=$(head -1 "$f" 2>/dev/null)
        [ "$s" = "closed" ] && continue
        [ -z "$s" ] && continue
        n=${f#/proc/asound/card}
        n=${n%%/sub*}
        PCM="$PCM,${n}:${s}"
    done
    [ -z "$PCM" ] && PCM="-" || PCM=${PCM#,}

    # cinder-home's own CPU, so "the app got busy" can be told apart from "a Sony service did".
    HOME_J=0
    for p in /proc/[0-9]*; do
        [ -r "$p/cmdline" ] || continue
        case "$(tr '\0' ' ' < "$p/cmdline" 2>/dev/null)" in
            *cinder-home*)
                # shellcheck disable=SC2046
                set -- $(cat "$p/stat" 2>/dev/null)
                HOME_J=$(( ${14:-0} + ${15:-0} ))
                break
                ;;
        esac
    done

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$EPOCH" "$UP" "$CAP" "$VOLT" "$STAT" "$USB" "$BL" \
        "$CPU_IDLE" "$CPU_TOTAL" "$PCM" "$HOME_J" >> "$OUT"

    sleep "$INTERVAL"
done
