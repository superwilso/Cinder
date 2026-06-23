#!/bin/sh
# uninstall_ldac.sh — exec_file payload; removes the LDAC bridge + supervisor.
# (The cinder wrapper's ldac-run.sh launch line is a no-op once the script is gone.)
LOG=/contents/ldac_install.log
exec >>"$LOG" 2>&1
echo "== ldac bridge UNINSTALL  $(date 2>/dev/null)"
BIN=/system/vendor/unknown321/bin
mount -t ext4 -o rw /emmc@android /system 2>/dev/null
mount -o remount,rw /emmc@android /system 2>/dev/null
rm -f "$BIN/cinder-ldac-bridge" "$BIN/ldac-run.sh" && echo "removed bridge + supervisor"
rm -f /contents/ldac_on /contents/ldac_off /contents/ldac.pid 2>/dev/null
sync
umount /system 2>/dev/null
echo "== done. =="
exit 0
