#!/bin/sh
# uninstall_cinder.sh — exec_file payload; runs once in the NWZ UPDATER as root.
# Reverses install_cinder.sh: restores scrobbler from its .real backup and removes
# the cinder-device binary. Leaves logs in /contents.
LOG=/contents/cinder_install.log
exec >>"$LOG" 2>&1
echo "================================================================"
echo "== cinder UI UNINSTALL  $(date 2>/dev/null)"

VENDOR=/system/vendor/unknown321
BIN=$VENDOR/bin

mount -t ext4 -o rw /emmc@android /system 2>/dev/null
mount -o remount,rw /emmc@android /system 2>/dev/null

if [ -f "$BIN/scrobbler.real" ]; then
    mv -f "$BIN/scrobbler.real" "$BIN/scrobbler"
    chmod 0755 "$BIN/scrobbler"
    echo "restored scrobbler from scrobbler.real"
fi
rm -f "$BIN/cinder-device" && echo "removed cinder-device"

sync
umount /system 2>/dev/null
echo "== uninstall done. reboot to normal. =="
exit 0
