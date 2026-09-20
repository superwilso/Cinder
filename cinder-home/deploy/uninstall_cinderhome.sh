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
SIG=$BIN/cinder-signature.sh

mount -t ext4 -o rw /emmc@android /system 2>/dev/null
mount -o remount,rw /emmc@android /system 2>/dev/null

# /data, mounted by us for the same reason as in install_cinderhome.sh (see its "state
# partition" block): the NWZ updater's ramdisk does not provide it, and the `off` flag below
# must reach the NEXT boot's launcher, not the ramdisk. Until 0.3.8 this write never landed —
# only the /contents twin did (which is what actually kept a failed uninstall non-bricking,
# because the launcher honours both). With the mount, the authoritative /data flag and the
# state cleanup further down work as written (issue #14, same root cause as the install).
[ -d /data ] || "$BB" mkdir -p /data 2>/dev/null
mount -t ext4 -o rw /emmc@usrdata /data 2>/dev/null
mount -o remount,rw /emmc@usrdata /data 2>/dev/null
if "$BB" grep -q " /data " /proc/mounts 2>/dev/null; then
    echo "state: /data (/emmc@usrdata) mounted"
else
    echo "WARN: /emmc@usrdata could not be mounted at /data — the off flag is set on /contents only"
fi

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
touch /data/cinder/off 2>/dev/null && echo "off flag: /data/cinder/off"
touch /contents/cinderhome_off 2>/dev/null; sync

# Only remove the launcher + binary once stock is verifiably restored. Otherwise KEEP them, so the
# boot path stays valid: either stock (restored .appcfg) or cinder-home (launcher honours
# cinderhome_off -> runs stock). Deleting the launcher under a broken .appcfg = soft-brick.
if [ "$restored" = 1 ]; then
    # 1. UNDO THE SOUND-SIGNATURE PATCH FIRST, while the helper that knows how is still here.
    #
    #    This is the one part of an install that does NOT live under $BIN: it patches three bytes
    #    of Sony's own libaudiohal-adleralsa.so. Every uninstall before this left that patch in
    #    place, so a device the user believed was back to stock kept a modified audio HAL and a
    #    raised CPU floor, with nothing left on it to explain why or undo it.
    #
    #    `revert` restores the pristine .stock snapshot and refuses if that snapshot's md5 is not
    #    the known-stock one, so it cannot make things worse. It is BEST-EFFORT: the brick-critical
    #    .appcfg restore has already succeeded by this point, and a failed audio revert must not
    #    turn a working uninstall into an aborted one. It prints "nothing to revert" on the normal
    #    case where the signature was never changed.
    if [ -f "$SIG" ]; then
        sh "$SIG" revert 2>&1 || echo "signature: revert did not run — the stock HAL backup may be absent"
    else
        echo "signature: no helper installed, nothing to revert"
    fi

    # 1b. Give Wampy its library back if the mono shim holds its place (install_cinderhome.sh 1g2).
    #     A rename, so it is right even on a live system. Best-effort, like the revert above.
    MONO_PATH=/system/vendor/unknown321/lib/libsound_service_fw.so
    MONO_WAMPY=/system/vendor/unknown321/lib/libsound_service_fw.wampy.so
    if [ -f "$MONO_PATH" ] && "$BB" grep -q libcinder_mono.so "$MONO_PATH" 2>/dev/null; then
        if [ -s "$MONO_WAMPY" ]; then
            "$BB" mv -f "$MONO_WAMPY" "$MONO_PATH" && echo "mono: Wampy's library restored to $MONO_PATH"
        else
            "$BB" rm -f "$MONO_PATH" && echo "mono: shim removed (no Wampy library was kept to restore)"
        fi
    else
        echo "mono: no shim installed, nothing to restore"
    fi

    # 2. Now the binaries. EVERY file install_cinderhome.sh puts in $BIN is listed here; the three
    #    that used to be missing (cinder-battery, cinder-voltable, cinder-signature.sh) meant two
    #    setuid-root helpers survived a "full uninstall" — exactly what the comment below forbids.
    "$BB" rm -f "$BIN/cinderhome-launch.sh" "$BIN/cinder-home" 2>/dev/null
    # setuid-root helpers must not outlive the app they exist for.
    "$BB" rm -f "$BIN/cinder-umount" "$BIN/cinder-gpunode" "$BIN/cinder-power" "$BIN/cinder-msc" \
        "$BIN/cinder-clock" "$BIN/cinder-fm" "$BIN/cinder-voltable" "$BIN/cinder-battery" \
        "$BIN/cinder-probe" 2>/dev/null
    "$BB" rm -f "$SIG" 2>/dev/null
    # The volume tables an install copied in for `wm1a`/`w1` (install_cinderhome.sh 1f3b). Sony's
    # own /system/usr/share/audio_dac is never touched.
    "$BB" rm -rf /system/vendor/unknown321/usr/share/cinder 2>/dev/null
    "$BB" rm -rf /data/cinder 2>/dev/null
    "$BB" rm -f /contents/cinderhome_off /contents/cinderhome_bootcount /contents/cinderhome_DISABLED_badboot /contents/cinder_gpu_on /contents/cinderhome_clear /contents/cinderhome_cable_off 2>/dev/null
    # The volume curve is applied by the LAUNCHER on every boot, so removing the launcher already
    # returns the stock curve. The conf is removed so a later reinstall honours the new choice
    # instead of silently keeping this one — install_cinderhome.sh keeps an existing file.
    "$BB" rm -f /contents/cinder_voltable.conf 2>/dev/null
    echo "removed launcher + binary + setuid helpers + flags (full uninstall)"
else
    echo "kept launcher/binary (restore incomplete); cinderhome_off set -> launcher runs stock."
fi
sync
umount /data 2>/dev/null
umount /system 2>/dev/null
echo "== done. reboot to normal -> stock Qt UI. =="
echo "   Your music, playlists and settings on the data partition were not touched."
exit 0
