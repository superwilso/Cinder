#!/bin/sh
# install_cinder.sh — exec_file payload; runs ONCE in the NWZ UPDATER as root
# (exec_file.sh already cleared the fup flag, so this is brick-safe). It installs
# the cinder-device framebuffer UI binary into /system and hooks it to launch at
# every normal boot via a REVERSIBLE wrapper around scrobbler.
#
# The binary is too big to embed in this script, so the user first copies the
# stripped `cinder-device` binary to the storage root (it appears at
# /contents/cinder-device); this script moves it into /system.
#
# ESCAPE HATCH: create an empty file /contents/cinder_off (over USB-MSC) to make
# the wrapper skip launching Cinder — recovers the stock UI without a reflash.
# Full revert: flash cinder_uninstall.upg. Recoverable via the wbrt eMMC backup.
LOG=/contents/cinder_install.log
exec >>"$LOG" 2>&1
echo "================================================================"
echo "== cinder UI installer  $(date 2>/dev/null)"

VENDOR=/system/vendor/unknown321
BIN=$VENDOR/bin
SRC=/contents/cinder-device

mount -t ext4 -o rw /emmc@android /system 2>/dev/null
mount -o remount,rw /emmc@android /system 2>/dev/null
if [ ! -d "$BIN" ]; then
    echo "ERROR: $BIN not found — is Wampy installed? aborting (no changes)."
    sync; exit 0
fi
if [ ! -f "$SRC" ]; then
    echo "ERROR: $SRC not found — copy the 'cinder-device' binary to the storage"
    echo "       root before flashing. aborting (no changes)."
    sync; exit 0
fi

cp "$SRC" "$BIN/cinder-device" && chmod 0755 "$BIN/cinder-device"
echo "installed binary: $BIN/cinder-device ($(wc -c < "$BIN/cinder-device" 2>/dev/null) bytes)"

# reversible wrapper around scrobbler (the proven boot-hook host)
TARGET=scrobbler
if [ ! -f "$BIN/$TARGET" ] && [ ! -f "$BIN/$TARGET.real" ]; then
    echo "ERROR: $BIN/$TARGET not found. aborting."
    sync; exit 0
fi
if [ ! -f "$BIN/$TARGET.real" ]; then
    cp -p "$BIN/$TARGET" "$BIN/$TARGET.real"
    echo "backed up $TARGET -> $TARGET.real"
fi
cat > "$BIN/$TARGET" <<WRAP_EOF
#!/system/bin/sh
# cinder-device launch hook (reversible; original at $TARGET.real).
# Option B (full replacement): Cinder REPLACES the stock Qt player. We SIGKILL
# HgrmMediaPlayerApp so it releases the framebuffer AND its RAM to Cinder; the loop
# re-kills it in case the launcher respawns it. The hagoromo AUDIO services are
# SEPARATE processes and keep running, so playback/DSP survive — Cinder drives them
# over IPC. (Cinder re-blits the panel every ~40ms, so any flicker as the Qt app
# dies is repainted immediately.)
# Escape hatch: create /contents/cinder_off, then REBOOT — the wrapper then skips the
# kill and the stock UI launches normally (no reflash). Unlike the old SIGSTOP hook
# there is no instant resume; recovery from kill mode is via reboot.
# Optional USB-DAC->LDAC bridge supervisor (no-op if the bridge isn't installed).
[ -x $BIN/ldac-run.sh ] && $BIN/ldac-run.sh >/dev/null 2>&1 &
if [ ! -f /contents/cinder_off ]; then
    (
        sleep 15
        $BIN/cinder-device >/contents/cinder_device.log 2>&1 &
        while [ ! -f /contents/cinder_off ]; do
            killall -KILL HgrmMediaPlayerApp 2>/dev/null \
                || kill -KILL \$(pidof HgrmMediaPlayerApp 2>/dev/null) 2>/dev/null
            sleep 2
        done
        # escape requested: stop Cinder; the stock UI returns on the next reboot
        killall -KILL cinder-device 2>/dev/null
    ) &
fi
exec $BIN/$TARGET.real "\$@"
WRAP_EOF
chmod 0755 "$BIN/$TARGET"
echo "hooked via wrapper: $BIN/$TARGET (orig -> $TARGET.real)"

rm -f "$SRC" && echo "removed staged $SRC"
sync
umount /system 2>/dev/null
echo "== done. reboot to normal; Cinder paints ~15s after boot. =="
echo "   escape hatch: create /contents/cinder_off, THEN REBOOT, to get stock UI back."
exit 0
