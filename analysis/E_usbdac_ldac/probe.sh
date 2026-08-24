#!/system/xbin/busybox sh
# shellcheck shell=dash
#   ^ The shebang is busybox's `sh`, which on the device is ASH. Older shellcheck reads that as
#   ash and emits SC2187 asking for exactly this line; newer versions stay quiet. Declaring it is
#   not just noise-suppression — it makes shellcheck analyse the file against the shell it will
#   really run under, instead of assuming bash features this script must never use.
# cinder_probe.sh — Phase E device-side probe for the USB-DAC -> LDAC question.
#
# READ-ONLY. Captures the state needed to answer CLAUDE.md Part H6 (#1-#4):
#   #1  Can BtTransmitter + UsbDeviceAudioPlayer be open at once? (any runtime mutex?)
#   #2  What ALSA device does the UAC path write to (confirm hw:0,4)?
#   #3  Does the BT/A2DP source path expose an ALSA entry point we can write PCM into?
#   #4  When entering USB-DAC mode, does the player tear down BT, or just show the overlay?
#
# Run ONCE in EACH audio condition, passing a label:
#   sh probe.sh idle      # nothing playing, no BT
#   sh probe.sh bt_ldac   # LDAC headphones connected + music playing
#   sh probe.sh usbdac    # USB-DAC mode active (plugged into a PC playing audio)
#   sh probe.sh both      # THE TEST: LDAC connected & playing, THEN enter USB-DAC
# Then diff the four snapshots.
#
# Optional live capture across a transition (run, then toggle USB-DAC on the device):
#   sh probe.sh watch 30  # tails the system log for 30s while you flip modes
#
# Output appends to a log on the FAT user partition, readable over USB-MSC.

BB=/system/xbin/busybox
[ -x "$BB" ] || BB=/xbin/busybox          # Wampy ships busybox here
[ -x "$BB" ] || BB=busybox
# Log to the FAT user partition by default (readable over USB-MSC); override with
# LOG=/data/local/tmp/cinder_probe.log when driving over adb.
LOG="${LOG:-/contents/cinder_probe.log}"
MODE="${1:-snapshot}"

run() {  # run "<label>" "<command string>"
  echo "----- $1 -----" >> "$LOG"
  eval "$2" >> "$LOG" 2>&1
  echo >> "$LOG"
}

{
  echo "================================================================"
  echo "== cinder_probe  mode=${MODE}  $($BB date 2>/dev/null)"
  echo "================================================================"
} >> "$LOG"

if [ "$MODE" = "watch" ]; then
  SECS="${2:-30}"
  echo "watching system log for ${SECS}s — toggle USB-DAC / BT on the device now" >> "$LOG"
  # Sony logs via icx_syslog; logcat may also be present. Capture whichever exists.
  if command -v logcat >/dev/null 2>&1; then
    timeout "$SECS" logcat -v time >> "$LOG" 2>&1
  else
    # icx syslog lives on /emmc@var; tail the freshest log file for SECS
    LF="$(ls -t /emmc@var/log/* 2>/dev/null | head -1)"
    [ -n "$LF" ] && timeout "$SECS" $BB tail -f "$LF" >> "$LOG" 2>&1
  fi
  echo "== END watch ==" >> "$LOG"
  exit 0
fi

run "uname"             "$BB uname -a"
run "audio/usb/bt props" "(getprop 2>/dev/null || true) | $BB grep -iE 'audio|usb|bt|ldac|sony|config'"
run "audio processes"   "$BB ps 2>/dev/null | $BB grep -iE 'hagodaemon|Hgrm|mtkbt|wampy|adbd'"
run "asound cards"      "cat /proc/asound/cards"
run "asound devices"    "cat /proc/asound/devices"
run "asound pcm list"   "cat /proc/asound/pcm"
# Per-substream status: which PCMs are RUNNING + their rate/format/channels.
# This is the heart of H6 #1/#2 — in 'both' mode we want to see the UAC capture
# AND a BT/A2DP playback substream RUNNING simultaneously.
run "pcm status"        "for s in /proc/asound/card0/pcm*/sub*/status; do echo \"== \$s\"; cat \"\$s\"; done"
run "pcm hw_params"     "for s in /proc/asound/card0/pcm*/sub*/hw_params; do echo \"== \$s\"; cat \"\$s\"; done"
run "cxd3778gf params"  "for p in /sys/module/snd_soc_cxd3778gf/parameters/*; do echo \"== \$p\"; cat \"\$p\" 2>/dev/null; done"
run "asound.conf"       "cat /system/etc/asound.conf 2>/dev/null; cat /vendor/etc/asound.conf 2>/dev/null"
run "usb gadget state"  "for f in /sys/class/android_usb/android0/state /sys/class/android_usb/android0/functions /sys/class/android_usb/android0/idProduct; do echo \"== \$f\"; cat \"\$f\" 2>/dev/null; done"
run "usb config props"  "getprop sys.usb.config 2>/dev/null; getprop sys.sony.config 2>/dev/null; getprop persist.sys.usb.config 2>/dev/null"
run "mtkbt log tail"    "$BB tail -n 60 /tmp/mtkbt.log 2>/dev/null"
run "bt sysfs"          "ls -l /sys/class/bluetooth 2>/dev/null; cat /sys/class/bluetooth/*/address 2>/dev/null"
# audiohal HAL plugins present (dualtrackmixalsa = the concurrency evidence)
run "audiohal plugins"  "ls -l /system/vendor/sony/lib/libaudiohal-*.so 2>/dev/null"

echo "== END mode=${MODE} ==" >> "$LOG"
echo >> "$LOG"
