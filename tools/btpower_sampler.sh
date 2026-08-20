#!/system/bin/sh
# btpower_sampler.sh — runs ON the Walkman. Two raw samples around a sleep; no arithmetic here.
#
# Everything read is a CUMULATIVE counter or a state, so the two samples subtract cleanly and the
# sampling itself is the only intrusion — which is why there are exactly two of them.
#
# IT NEVER WRITES A CODEC REGISTER. /proc/regmon/<chip>/target selects and `value` reads; writing
# `value` would change the audio hardware under the running player. Selecting is harmless.
# ONE SHOT PER CALL. An earlier version backgrounded itself with nohup and slept through the
# window; adb kills the process group when its shell exits, so the second sample never happened —
# the file came back with a T0 and nothing else. Two separate invocations need no daemon, and the
# window becomes "however long you left it", which is what you wanted to measure anyway.
WHEN="${1:-T0}"          # T0 = start of the window, T1 = end
OUT="${2:-/contents/btpower.txt}"

REGS="SYSTEM OSC_ON OSC_SEL OSC_EN PLUG_DET MICBIAS BLK_ON0 BLK_ON1 SD_ENABLE DSD_ENABLE
      CODEC_PLAYVOL PHV_SEL PHV_L PHV_R PHV_CTRL0 PHV_CTRL1 LINEOUT_VOL
      HPOUT2_CTRL1 HPOUT3_CTRL1 DNC1_START SMS_NS_PMUTE"

sample() {
    echo "## $1"
    echo "uptime $(cat /proc/uptime 2>/dev/null)"
    grep -E '^(cpu |ctxt|processes|intr )' /proc/stat 2>/dev/null | cut -c1-120
    echo "-- time_in_state"
    cat /sys/devices/system/cpu/cpu0/cpufreq/stats/time_in_state 2>/dev/null
    echo "-- battery"
    for f in capacity voltage_now status; do
        echo "$f $(cat /sys/class/power_supply/battery/$f 2>/dev/null)"
    done
    echo "usb_online $(cat /sys/class/power_supply/usb/online 2>/dev/null)"
    echo "-- procs"
    # No pidof on this shell (memory: reference_device_shell_gotchas), so walk /proc.
    #
    # THE NAME COMES FROM cmdline, NOT comm. Sony starts its services under `logwrapper`, so 30 of
    # the processes on this device report the comm `(logwrapper)` and the thing you actually want
    # to name — hagodaemon11, hagodaemon27 — is only in the command line. Matching on comm found
    # cinder-home and nothing else, which made the audio and Bluetooth services invisible.
    for p in /proc/[0-9]*; do
        [ -r "$p/stat" ] || continue
        name=$(tr "\0" " " < "$p/cmdline" 2>/dev/null | cut -d" " -f1)
        [ -n "$name" ] || continue
        case "$name" in
            *cinder*|*hagodaemon*|*Hgrm*|*bluetooth*|*audio*|*logwrapper*)
                set -- $(cat "$p/stat" 2>/dev/null)
                # $1 = pid, $14 utime, $15 stime — positional and stable.
                # A logwrapper shell is only worth naming by what it wrapped, so keep the argv[0]
                # basename either way; the pid disambiguates the rest.
                echo "proc (${name##*/}) $1 utime=${14} stime=${15}"
                ;;
        esac
    done
    echo "-- alsa"
    for f in /proc/asound/card*/pcm*/sub*/status; do
        s=$(head -1 "$f" 2>/dev/null)
        [ "$s" = "closed" ] || echo "$f $s"
    done
    echo "-- jack $(cat /sys/class/switch/cxd3778gf_h2w/state 2>/dev/null)"
    echo "-- codec"
    for r in $REGS; do
        echo "$r" > /proc/regmon/cxd3778gf/target 2>/dev/null
        echo "$r $(cat /proc/regmon/cxd3778gf/value 2>/dev/null)"
    done
}

if [ "$WHEN" = "T0" ]; then
    { echo "# btpower"; sample T0; } > "$OUT" 2>/dev/null
else
    { sample T1; echo "# done"; } >> "$OUT" 2>/dev/null
fi
