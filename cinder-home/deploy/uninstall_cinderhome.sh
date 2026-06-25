#!/bin/sh
# uninstall_cinderhome.sh — exec_file payload; reverts install_cinderhome.sh by restoring
# the original HgrmMediaPlayerApp.appcfg (stock Qt app launches again). Brick-safe.
LOG=/contents/cinder_home_install.log
exec >>"$LOG" 2>&1
echo "================================================================"
echo "== cinder-home UNINSTALL  $(date 2>/dev/null)"

SONYBIN=/system/vendor/sony/bin
APPCFG=$SONYBIN/HgrmMediaPlayerApp.appcfg
BIN=/system/vendor/unknown321/bin

mount -t ext4 -o rw /emmc@android /system 2>/dev/null
mount -o remount,rw /emmc@android /system 2>/dev/null

if [ -f "$APPCFG.real" ]; then
    # verified content restore (busybox mv/cp are flaky -> cat>)
    cat "$APPCFG.real" > "$APPCFG" 2>/dev/null
    if [ -s "$APPCFG" ]; then
        rm -f "$APPCFG.real"
        echo "restored $APPCFG from .appcfg.real (stock Qt app re-enabled)"
    else
        echo "ERROR: restore wrote empty $APPCFG — leaving .appcfg.real in place. Re-run or use wbrt."
    fi
else
    echo "no $APPCFG.real backup found — nothing to restore (already stock?)."
fi

# stop cinder-home from launching even if the .appcfg somehow still points at it
touch /contents/cinderhome_off 2>/dev/null
rm -f "$BIN/cinderhome-launch.sh" "$BIN/cinder-home" 2>/dev/null
rm -f /contents/cinderhome_bootcount /contents/cinderhome_DISABLED_badboot 2>/dev/null
echo "removed launcher + binary + bad-boot flags"
sync
umount /system 2>/dev/null
echo "== done. reboot to normal -> stock Qt UI. =="
exit 0
