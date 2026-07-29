#!/bin/sh
# uninstall_cinder.sh — exec_file payload; runs once in the NWZ UPDATER as root.
# Reverses install_cinder.sh: restores the original scrobbler and removes Cinder.
#
# ROBUSTNESS: the updater busybox showed `mv -f` failing with "Invalid cross-device
# link", which would silently leave the wrapper in place (a broken uninstall). So we
# restore with a verified `cat >` content copy and never rely on mv/cp applet quirks.
LOG=/contents/cinder_install.log
exec >>"$LOG" 2>&1
echo "================================================================"
echo "== cinder UI UNINSTALL  $(date 2>/dev/null)"

VENDOR=/system/vendor/unknown321
BIN=$VENDOR/bin

mount -t ext4 -o rw /emmc@android /system 2>/dev/null
mount -o remount,rw /emmc@android /system 2>/dev/null

if [ -f "$BIN/scrobbler.real" ]; then
    cat "$BIN/scrobbler.real" > "$BIN/scrobbler"
    if [ -s "$BIN/scrobbler" ]; then
        chmod 0755 "$BIN/scrobbler"
        rm -f "$BIN/scrobbler.real"
        echo "restored scrobbler from scrobbler.real"
    else
        echo "ERROR: restore of scrobbler FAILED — leaving scrobbler.real in place."
        echo "       (scrobbler may still be the wrapper; retry or use wbrt.)"
    fi
else
    echo "note: no scrobbler.real backup found — nothing to restore."
fi

rm -f "$BIN/cinder-device" && echo "removed cinder-device"

# clear the control/state flags so a future install starts clean
rm -f /contents/cinder_off /contents/cinder_bootcount /contents/cinder_DISABLED_badboot \
      /contents/cinder-device 2>/dev/null
echo "cleared cinder flags + staged binary from /contents"

sync
umount /system 2>/dev/null
echo "== uninstall done. reboot to normal. =="
exit 0
