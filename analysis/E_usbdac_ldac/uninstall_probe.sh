#!/bin/sh
# uninstall_probe.sh — packaged as an exec_file payload; runs once in the NWZ
# UPDATER as root. Reverses install_probe.sh: restores the wrapped boot binary
# from its .real backup and removes the daemon. Leaves the log in /contents.

LOG=/contents/cinder_install.log
exec >>"$LOG" 2>&1
echo "================================================================"
echo "== cinder probe UNINSTALL  $(date 2>/dev/null)"

VENDOR=/system/vendor/unknown321
BIN=$VENDOR/bin

mount -t ext4 -o rw /emmc@android /system 2>/dev/null
mount -o remount,rw /emmc@android /system 2>/dev/null

for cand in scrobbler wampy pstserver; do
    if [ -f "$BIN/$cand.real" ]; then
        mv -f "$BIN/$cand.real" "$BIN/$cand"
        chmod 0755 "$BIN/$cand"
        echo "restored $cand from $cand.real"
    fi
done

rm -f "$BIN/cinder_probed.sh" && echo "removed daemon"

sync
umount /system 2>/dev/null
echo "== uninstall done. reboot to normal. =="
exit 0
