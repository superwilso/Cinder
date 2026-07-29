#!/system/bin/sh
# cinder_probed — ALSA/USB/BT snapshots for the USB-DAC -> LDAC probe.
# Runs in NORMAL boot (audio stack alive), launched by the reversible scrobbler
# wrapper that install_probe.sh drops. We have no live command channel (no adb,
# no network), so this captures a timeline to /contents while the user performs
# the physical test. Read the logs afterwards over USB-MSC.
#
# QUIET: a tick is logged only while a PCM substream is RUNNING (idle ticks are
# suppressed — the first build flooded the log with 300+ idle ticks). Each RUNNING
# stream logs status + hw_params (format/rate/channels) — what a PCM bridge needs.
# A small static topology snapshot (card names, pcm list, asound.conf) is written
# once per boot to cinder_info.log.
#
# READ-ONLY: only reads /proc, /sys, getprop, logs; never writes /system.
#
# NOTE: this file is the reference copy; the authoritative copy is the quoted
# heredoc embedded in install_probe.sh (that is what the .UPG actually deploys).
# Keep the two in sync.
BB=/system/xbin/busybox
[ -x "$BB" ] || BB=/xbin/busybox
LOG=/contents/cinder_probe.log
INFO=/contents/cinder_info.log
INTERVAL=3
gp() { getprop "$1" 2>/dev/null; }
# cxd3778gf runtime routing: this codec exposes its state via module parameters
# (per Wampy ALSA.md). Diffing these across jack vs LDAC reveals the output-route
# control we would flip to send USB-DAC audio to the BT/LDAC encoder.
cxd() {
  d=/sys/module/snd_soc_cxd3778gf/parameters
  [ -d "$d" ] || { echo "cxd: (none)"; return; }
  o=""
  for p in "$d"/*; do
    [ -f "$p" ] || continue
    o="$o ${p##*/}=$(cat "$p" 2>/dev/null | tr '\n' ',')"
  done
  echo "cxd:$o"
}

# static topology — once per boot, overwritten (small + readable)
{
  echo "== cinder_info  $(date 2>/dev/null)"
  echo "== $(uname -a 2>/dev/null)"
  echo "-- /proc/asound/cards --"; cat /proc/asound/cards 2>/dev/null
  echo "-- /proc/asound/pcm --";   cat /proc/asound/pcm 2>/dev/null
  echo "-- asound.conf --";        cat /system/etc/asound.conf 2>/dev/null
  echo "-- audiohal plugins --";   ls -1 /system/vendor/sony/lib/libaudiohal-*.so 2>/dev/null
} > "$INFO" 2>&1

# fresh main log each boot
echo "== cinder_probed start $(date 2>/dev/null) — logs only while a PCM is RUNNING" > "$LOG"

last=""
while true; do
  run=""
  for s in /proc/asound/card*/pcm*/sub*/status; do
    [ -f "$s" ] || continue
    grep -q "state: RUNNING" "$s" 2>/dev/null && run="$run $s"
  done
  # emit while anything runs, plus once on the transition back to idle
  if [ -n "$run" ] || [ "$run" != "$last" ]; then
    {
      echo "---- $(date 2>/dev/null) ----"
      echo "usb.cfg=$(gp sys.usb.config) sony.cfg=$(gp sys.sony.config) func=$(cat /sys/class/android_usb/android0/functions 2>/dev/null) pid=$(cat /sys/class/android_usb/android0/idProduct 2>/dev/null)"
      for s in $run; do
        printf 'RUNNING %s :: ' "$s"; tr '\n' ' ' < "$s"; echo
        hw="${s%status}hw_params"
        printf '   hw_params :: '; tr '\n' ' ' < "$hw" 2>/dev/null; echo
      done
      echo "mtkbt: $($BB tail -n 3 /tmp/mtkbt.log 2>/dev/null | tr '\n' '|')"
    } >> "$LOG" 2>&1
    sync 2>/dev/null
  fi
  last="$run"
  sleep "$INTERVAL"
done
