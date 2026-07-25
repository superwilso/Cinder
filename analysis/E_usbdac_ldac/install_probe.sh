#!/bin/sh
# install_probe.sh — packaged as the exec_file payload (the second file in the
# .UPG). Runs ONCE in the NWZ UPDATER as root (exec_file.sh already cleared the
# fup flag, so this is brick-safe). It:
#   1. mounts the system partition read-write (same as Wampy's run.sh),
#   2. drops the cinder_probed daemon into /system,
#   3. hooks it via a REVERSIBLE wrapper around an existing unknown321 boot
#      binary (scrobbler preferred) so it starts every normal boot,
#   4. leaves the boot image / initrd UNTOUCHED.
# Reverse with uninstall_probe.sh. Recoverable via the wbrt eMMC backup.

LOG=/contents/cinder_install.log
exec >>"$LOG" 2>&1
echo "================================================================"
echo "== cinder probe installer  $(date 2>/dev/null)"

VENDOR=/system/vendor/unknown321
BIN=$VENDOR/bin

# 1. mount /system rw (no-op if already mounted)
mount -t ext4 -o rw /emmc@android /system 2>/dev/null
mount -o remount,rw /emmc@android /system 2>/dev/null
if [ ! -d "$BIN" ]; then
    echo "ERROR: $BIN not found — is Wampy/scrobbler installed? aborting (no changes)."
    sync
    exit 0
fi

# 2. write the daemon (verbatim — quoted heredoc, no expansion here)
cat > "$BIN/cinder_probed.sh" <<'PROBE_EOF'
#!/system/bin/sh
# cinder_probed — ALSA/USB/BT snapshots for the USB-DAC -> LDAC probe.
# QUIET: a tick is logged only while a PCM substream is RUNNING (idle ticks are
# suppressed — the old build flooded the log). Each RUNNING stream logs status +
# hw_params (format/rate/channels) — what a PCM bridge needs. A small static
# topology snapshot (card names, pcm list, asound.conf) is written once per boot
# to cinder_info.log. READ-ONLY; runs in normal boot via the scrobbler wrapper.
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
      cxd
      echo "mtkbt: $($BB tail -n 3 /tmp/mtkbt.log 2>/dev/null | tr '\n' '|')"
    } >> "$LOG" 2>&1
    sync 2>/dev/null
  fi
  last="$run"
  sleep "$INTERVAL"
done
PROBE_EOF
chmod 0755 "$BIN/cinder_probed.sh"
echo "installed daemon: $BIN/cinder_probed.sh"

# 3. reversible wrapper around an existing boot binary (prefer scrobbler)
TARGET=""
for cand in scrobbler wampy pstserver; do
    if [ -f "$BIN/$cand.real" ]; then
        TARGET="$cand"  # already wrapped — refresh the wrapper (idempotent)
        echo "note: $cand already wrapped (.real present)"
        break
    elif [ -f "$BIN/$cand" ]; then
        TARGET="$cand"; break
    fi
done

if [ -z "$TARGET" ]; then
    echo "ERROR: no wrap target (scrobbler/wampy/pstserver) found — daemon installed but NOT hooked."
    sync; exit 0
fi

if [ ! -f "$BIN/$TARGET.real" ]; then
    cp -p "$BIN/$TARGET" "$BIN/$TARGET.real"
    echo "backed up $TARGET -> $TARGET.real"
fi
cat > "$BIN/$TARGET" <<WRAP_EOF
#!/system/bin/sh
# cinder_probed launch hook (reversible — original at $TARGET.real)
$BIN/cinder_probed.sh &
exec $BIN/$TARGET.real "\$@"
WRAP_EOF
chmod 0755 "$BIN/$TARGET"
echo "hooked via wrapper: $BIN/$TARGET (orig -> $TARGET.real)"

sync
umount /system 2>/dev/null
echo "== done. reboot to normal, then run the test sequence. =="
exit 0
