#!/bin/sh
# uninstall_cinderhome.sh — exec_file payload; reverts install_cinderhome.sh by restoring
# the original HgrmMediaPlayerApp.appcfg (stock Qt app launches again). Brick-safe.
#
# Like the installer, this routes every file op through the updater's known-good busybox
# (the ambient wc/rm/mv in the NWZ updater are unreliable — see install_cinderhome.sh
# "UPDATER TOOLING"). The .appcfg restore is brick-critical, so it must not ride on them.
LOG=/contents/cinder_home_install.log
exec >>"$LOG" 2>&1
echo "================================================================"
echo "== cinder-home UNINSTALL  $(date 2>/dev/null)"

BB=/xbin/busybox
[ -x "$BB" ] || BB=/system/xbin/busybox
[ -x "$BB" ] || BB=busybox
echo "busybox: $BB"

SONYBIN=/system/vendor/sony/bin
APPCFG=$SONYBIN/HgrmMediaPlayerApp.appcfg
BIN=/system/vendor/unknown321/bin

mount -t ext4 -o rw /emmc@android /system 2>/dev/null
mount -o remount,rw /emmc@android /system 2>/dev/null

# Restore the stock .appcfg ATOMICALLY + verified. NEVER leave a truncated .appcfg (soft-brick).
restored=0
if [ -f "$APPCFG.real" ]; then
    "$BB" cat "$APPCFG.real" > "$APPCFG.tmp" 2>/dev/null
    if [ -s "$APPCFG.tmp" ] && "$BB" grep -q '^type: Home' "$APPCFG.tmp"; then
        "$BB" mv -f "$APPCFG.tmp" "$APPCFG"
        "$BB" rm -f "$APPCFG.real" 2>/dev/null
        restored=1
        echo "restored stock .appcfg (stock Qt app re-enabled)"
    else
        "$BB" rm -f "$APPCFG.tmp" 2>/dev/null
        echo "ERROR: restore verify failed — leaving the install intact (device still boots)."
    fi
else
    echo "no $APPCFG.real backup found — nothing to restore (already stock?)."
fi

# ALWAYS set the escape flag: if the .appcfg still points at our launcher (restore didn't run),
# the launcher reads this and runs stock. This is what keeps a failed uninstall non-bricking.
# Set it in BOTH places: /data/cinder/off is the one the launcher treats as authoritative,
# /contents/cinderhome_off is the USB-MSC-visible copy (and what pre-2026-07-26 launchers read).
"$BB" mkdir -p /data/cinder 2>/dev/null
touch /data/cinder/off 2>/dev/null
touch /contents/cinderhome_off 2>/dev/null; sync

# Only remove the launcher + binary once stock is verifiably restored. Otherwise KEEP them, so the
# boot path stays valid: either stock (restored .appcfg) or cinder-home (launcher honours
# cinderhome_off -> runs stock). Deleting the launcher under a broken .appcfg = soft-brick.
if [ "$restored" = 1 ]; then
    "$BB" rm -f "$BIN/cinderhome-launch.sh" "$BIN/cinder-home" 2>/dev/null
    # setuid-root helpers must not outlive the app they exist for.
    "$BB" rm -f "$BIN/cinder-umount" "$BIN/cinder-gpunode" "$BIN/cinder-power" "$BIN/cinder-msc" 2>/dev/null
    "$BB" rm -rf /data/cinder 2>/dev/null
    "$BB" rm -f /contents/cinderhome_off /contents/cinderhome_bootcount /contents/cinderhome_DISABLED_badboot /contents/cinder_gpu_on /contents/cinderhome_clear /contents/cinderhome_cable_off 2>/dev/null
    echo "removed launcher + binary + setuid helpers + flags (full uninstall)"
else
    echo "kept launcher/binary (restore incomplete); cinderhome_off set -> launcher runs stock."
fi
sync
umount /system 2>/dev/null
echo "== done. reboot to normal -> stock Qt UI. =="
exit 0
