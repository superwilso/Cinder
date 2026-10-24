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
VENDOR=/system/vendor/cinder
OLD=/system/vendor/unknown321   # Wampy's folder, where Cinder lived until 2026-10-24
BIN=$VENDOR/bin
# An install from before then, never updated: its helpers are the ones that know how to undo it.
[ ! -f "$BIN/cinder-home" ] && [ -f "$OLD/bin/cinder-home" ] && BIN=$OLD/bin
SIG=$BIN/cinder-signature.sh
CHAIN=$BIN/cinder-chain.sh

mount -t ext4 -o rw /emmc@android /system 2>/dev/null
mount -o remount,rw /emmc@android /system 2>/dev/null

# /data, mounted by us for the same reason as in install_cinderhome.sh (see its "state
# partition" block): the NWZ updater's ramdisk does not provide it, and the `off` flag below
# must reach the NEXT boot's launcher, not the ramdisk. Until 0.3.8 this write never landed —
# only the /contents twin did (which is what actually kept a failed uninstall non-bricking,
# because the launcher honours both). With the mount, the authoritative /data flag and the
# state cleanup further down work as written (issue #14, same root cause as the install).
# The raw-partition retry and the ramdisk sentinel are the install payload's, for the reasons
# written out there: the /emmc@* aliases and /proc are both optional in the updater, and a write
# to an unmounted /data reads back exactly like a good one.
# The sentinel carries this run's token so that a 0-byte stray an older live install left on the
# real partition cannot read as "our marker is still visible, so /data did not mount" (see the
# install payload's note), and it is removed unconditionally for the same reason.
[ -d /data ] || "$BB" mkdir -p /data 2>/dev/null
SENTINEL=0
SENTINEL_TOKEN="cinder-premount $$"
echo "$SENTINEL_TOKEN" > /data/.cinder_premount 2>/dev/null \
    && [ "$("$BB" cat /data/.cinder_premount 2>/dev/null)" = "$SENTINEL_TOKEN" ] && SENTINEL=1
data_is_mounted() {
    if [ "$SENTINEL" = 1 ]; then
        [ "$("$BB" cat /data/.cinder_premount 2>/dev/null)" = "$SENTINEL_TOKEN" ] && return 1
        return 0
    fi
    "$BB" grep -q " /data " /proc/mounts 2>/dev/null
}
mount -t ext4 -o rw /emmc@usrdata /data 2>/dev/null
mount -o remount,rw /emmc@usrdata /data 2>/dev/null
data_is_mounted || mount -t ext4 -o rw /dev/block/mmcblk0p28 /data 2>/dev/null
if data_is_mounted; then
    echo "state: /data (/emmc@usrdata) mounted"
else
    echo "WARN: /emmc@usrdata could not be mounted at /data — the off flag is set on /contents only"
fi
"$BB" rm -f /data/.cinder_premount 2>/dev/null

# Restore the stock .appcfg ATOMICALLY + verified. NEVER leave a truncated .appcfg (soft-brick).
# A backup that names Cinder's launcher is no backup (install_cinderhome.sh step 2): restoring the
# owner's, from 2026-09-22, and then deleting the launcher below would leave appmgr no Home app.
# Then it is Sony's own four lines, the same on stock 1.02 and Walkman One 3.02.
stock_appcfg() {
    "$BB" cat <<'STOCK_EOF'
name: HgrmMediaPlayerApp
command: HgrmMediaPlayerApp
type: Home
hidden: false
STOCK_EOF
}
good_backup() {
    [ -s "$1" ] && "$BB" grep -q '^type: Home' "$1" && "$BB" grep -q '^command: ' "$1" \
        && ! "$BB" grep -q cinderhome-launch "$1"
}
restored=0
if good_backup "$APPCFG" && [ ! -f "$APPCFG.real" ]; then
    echo "the .appcfg is Sony's already and there is no backup - nothing to restore."
    restored=1
else
    if good_backup "$APPCFG.real"; then
        "$BB" cat "$APPCFG.real" > "$APPCFG.tmp" 2>/dev/null
    else
        echo "the .appcfg.real backup is missing or names Cinder's launcher - writing Sony's own"
        stock_appcfg > "$APPCFG.tmp"
    fi
    if good_backup "$APPCFG.tmp"; then
        # Stock is 0755. Under this script's umask 077 the temp file is 0600 root:root, and appmgr
        # (uid 100) must READ the .appcfg — moved in as-is, the "restored" stock app never launches.
        "$BB" chmod 0755 "$APPCFG.tmp"
        "$BB" mv -f "$APPCFG.tmp" "$APPCFG"
        "$BB" rm -f "$APPCFG.real" 2>/dev/null
        restored=1
        echo "restored stock .appcfg (stock Qt app re-enabled)"
    else
        "$BB" rm -f "$APPCFG.tmp" 2>/dev/null
        echo "ERROR: restore verify failed — leaving the install intact (device still boots)."
    fi
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
    # The effect chain, the same way: put Sony's own sound library back if Clear Bass was set.
    if [ -f "$CHAIN" ]; then
        sh "$CHAIN" set default 2>&1 || echo "chain: revert did not run — the kept original may be absent"
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

    # 1c. The `preload` component (install_cinderhome.sh 1g3): Sony's hagodaemon back in place of
    #     the wrapper, Sony's boot script back, the guard and the shim gone. The script that put
    #     them there is the one that takes them away, and it is removed with the binaries below.
    #     Best-effort: every failure in there leaves a wrapper that only passes through.
    if [ -s "$BIN/cinder-preload.sh" ]; then
        sh "$BIN/cinder-preload.sh" remove 2>&1 || echo "preload: could not be removed cleanly (see the line above)"
    else
        echo "preload: not installed, nothing to remove"
    fi

    # 1d. The switches of `bt-aac` (install_cinderhome.sh 1g4); its libraries go with the folder.
    "$BB" rm -f /data/cinder/bt_aac /data/cinder/bt_sbc_xq /data/cinder/bt_aptxhd 2>/dev/null

    # 2. Now the files. Cinder's own folder is all Cinder's, so all of it: the binaries, the
    #    setuid-root helpers (which must not outlive the app they exist for), the libraries of
    #    1g3-1g6 and the volume tables of 1f3b. Sony's own /system/usr/share/audio_dac is never
    #    touched.
    "$BB" rm -rf "$VENDOR" 2>/dev/null
    # An install from before 2026-10-24 sits in Wampy's folder: Cinder's files by name, then the
    # folders only if that left them empty, so Wampy's own files stay.
    for f in cinder-home cinderhome-launch.sh cinder-umount cinder-gpunode cinder-power cinder-msc \
             cinder-clock cinder-fm cinder-voltable cinder-battery cinder-probe cinder-signature.sh \
             cinder-chain.sh cinder-preload.sh launch-probe.sh; do
        "$BB" rm -f "$OLD/bin/$f" 2>/dev/null
    done
    "$BB" rm -f "$OLD/lib/libcinder_mono.so" "$OLD/lib/libfdk-aac-enc.so" \
        "$OLD/lib/preload/BtCommonService.so" "$OLD/lib/preload/MediaStoreService.so" \
        "$OLD/lib/preload/AudioInPlayerService.so" "$OLD/HgrmMediaPlayerApp.appcfg.real" 2>/dev/null
    "$BB" rm -rf "$OLD/usr/share/cinder" 2>/dev/null
    for d in lib/preload lib bin usr/share usr ""; do
        "$BB" rmdir "$OLD/$d" 2>/dev/null
    done
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
