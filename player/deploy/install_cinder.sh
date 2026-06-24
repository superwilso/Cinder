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
# SAFETY: the installed wrapper has a BAD-BOOT COUNTER — if Cinder reboots the device
# 3x within 60s it auto-disables and the stock UI returns on its own (~2 min, no PC
# needed). Manual escape: create /contents/cinder_off (over USB-MSC) then reboot.
# Full revert: flash cinder_uninstall.upg. Last resort: wbrt eMMC restore. See RECOVERY.md.
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

# Copy the binary with `cat >` (not `cp` — the updater busybox showed cp/rename
# quirks like "Invalid cross-device link"; a plain redirect always does a content
# copy) and VERIFY it landed non-empty BEFORE touching anything else. If this fails
# we must NOT write the wrapper — otherwise we'd leave a wrapper that launches a
# missing binary (the "cinder-device: not found" trap). (busybox here has no `wc`,
# so we use the POSIX `-s` test.)
cat "$SRC" > "$BIN/cinder-device" 2>/dev/null
if [ ! -s "$BIN/cinder-device" ]; then
    echo "ERROR: failed to install $BIN/cinder-device (copy failed or zero bytes)."
    echo "       leaving staged $SRC in place; NO wrapper changes. Re-push & retry."
    sync; umount /system 2>/dev/null; exit 0
fi
chmod 0755 "$BIN/cinder-device"
echo "installed binary: $BIN/cinder-device (present, non-empty)"

# reversible wrapper around scrobbler (the proven boot-hook host)
TARGET=scrobbler
if [ ! -f "$BIN/$TARGET" ] && [ ! -f "$BIN/$TARGET.real" ]; then
    echo "ERROR: $BIN/$TARGET not found. aborting."
    sync; exit 0
fi
if [ ! -f "$BIN/$TARGET.real" ]; then
    # Back up the ORIGINAL scrobbler with a verified content copy. If this fails we
    # MUST NOT overwrite scrobbler with the wrapper — otherwise the original is lost
    # with no .real to restore from (uninstall would be impossible).
    cat "$BIN/$TARGET" > "$BIN/$TARGET.real" && chmod 0755 "$BIN/$TARGET.real"
    if [ ! -s "$BIN/$TARGET.real" ]; then
        echo "ERROR: failed to back up $TARGET -> $TARGET.real; aborting (no wrapper written)."
        sync; umount /system 2>/dev/null; exit 0
    fi
    echo "backed up $TARGET -> $TARGET.real"
fi
cat > "$BIN/$TARGET" <<WRAP_EOF
#!/system/bin/sh
# cinder-device launch hook (reversible; original at $TARGET.real).
#
# SAFETY GUARDRAILS (added after a SIGKILL kill-loop caused a boot loop):
#  1. BAD-BOOT COUNTER + AUTO-REVERT. Each Cinder-attempt boot bumps
#     /contents/cinder_bootcount. After 3 boots that don't survive 60s, Cinder
#     AUTO-DISABLES itself (creates cinder_off) so the device self-recovers to the
#     stock UI in ~2 min — NO wbrt / reflash needed. A boot that survives 60s resets
#     the counter to 0.
#  2. SIGSTOP, NOT KILL. The stock Qt app is FROZEN (kept alive) so init's watchdog
#     never sees it die and force a reboot — killing it is what caused the loop.
#     SIGCONT on escape brings it back. (A true kill would free more RAM but needs the
#     watchdog neutralised first — deferred; safety first.)
#  3. MISSING-BINARY GUARD. If cinder-device isn't present+executable we leave the
#     stock UI completely untouched (never freeze stock with nothing to show).
#  4. USB MASS-STORAGE HANDOFF. cinder-device detects a PC and creates /dev/cinder_usb;
#     while that exists we THAW stock (SIGCONT) so it can mount mass storage cleanly
#     (only stock can release /contents — and pushing /contents/cinder_off, the manual
#     escape, requires mass storage, so this also keeps recovery working under Cinder).
#
# Escape (manual): create /contents/cinder_off, then reboot -> stock UI.
# Re-enable after an auto-disable: delete /contents/cinder_off, cinder_bootcount,
#   and cinder_DISABLED_badboot, then reboot.
BOOTCOUNT=/contents/cinder_bootcount
MAXBAD=3

# optional USB-DAC->LDAC bridge supervisor (no-op if the bridge isn't installed)
[ -x $BIN/ldac-run.sh ] && $BIN/ldac-run.sh >/dev/null 2>&1 &

# --- bad-boot counter: only counts boots where we actually attempt Cinder ---
if [ ! -f /contents/cinder_off ] && [ -x $BIN/cinder-device ]; then
    n=0
    [ -f "\$BOOTCOUNT" ] && n=\$(cat "\$BOOTCOUNT" 2>/dev/null)
    [ -z "\$n" ] && n=0
    n=\$((n + 1))
    echo "\$n" > "\$BOOTCOUNT"
    if [ "\$n" -ge "\$MAXBAD" ]; then
        # too many boots that didn't survive 60s -> assume Cinder is unstable; disable.
        touch /contents/cinder_off
        touch /contents/cinder_DISABLED_badboot
    fi
fi

if [ ! -f /contents/cinder_off ] && [ -x $BIN/cinder-device ]; then
    (
        # CRITICAL: do NOT freeze the stock app before it reaches Foreground — appmgr
        # waits for the Home app to foreground and REBOOTS on timeout, so an early
        # SIGSTOP causes a boot loop (observed 2026-06-24). Wait until HgrmMediaPlayerApp
        # is up, then a grace period for it to complete the appmgr Foreground handshake,
        # THEN freeze it and paint over it.
        i=0
        while [ \$i -lt 90 ] && ! pidof HgrmMediaPlayerApp >/dev/null 2>&1; do
            sleep 1; i=\$((i + 1))
        done
        sleep 12   # let it reach Foreground (appmgr handshake) before we freeze it
        $BIN/cinder-device >/contents/cinder_device.log 2>&1 &
        # FREEZE stock with SIGSTOP (alive -> no watchdog reboot, no respawn fight),
        # EXCEPT during a USB mass-storage handoff: when cinder-device detects a PC it
        # creates /dev/cinder_usb and stops painting; we then THAW stock so it can mount
        # mass storage CLEANLY (only stock releases /contents — a frozen app can't, and
        # forcing umount would corrupt the vfat volume). Cable out -> cinder-device
        # removes the flag -> we re-freeze and Cinder resumes.
        while [ ! -f /contents/cinder_off ]; do
            if [ -e /dev/cinder_usb ]; then
                killall -CONT HgrmMediaPlayerApp 2>/dev/null \
                    || kill -CONT \$(pidof HgrmMediaPlayerApp 2>/dev/null) 2>/dev/null
            else
                killall -STOP HgrmMediaPlayerApp 2>/dev/null \
                    || kill -STOP \$(pidof HgrmMediaPlayerApp 2>/dev/null) 2>/dev/null
            fi
            sleep 2
        done
        killall -CONT HgrmMediaPlayerApp 2>/dev/null \
            || kill -CONT \$(pidof HgrmMediaPlayerApp 2>/dev/null) 2>/dev/null
    ) &
    # heartbeat: survive 90s without a reboot -> this boot is good, clear the counter
    ( sleep 90; echo 0 > "\$BOOTCOUNT" ) &
fi
exec $BIN/$TARGET.real "\$@"
WRAP_EOF
chmod 0755 "$BIN/$TARGET"
echo "hooked via wrapper: $BIN/$TARGET (orig -> $TARGET.real)"

# Fresh install = re-enable: clear any prior disable/bad-boot flags so re-flashing after an
# auto-disable brings Cinder back without manual cleanup.
rm -f /contents/cinder_off /contents/cinder_bootcount /contents/cinder_DISABLED_badboot 2>/dev/null
echo "cleared prior disable flags (fresh install = enabled)"
# Leave the staged binary in place so a re-flash needs no re-push (removing it was the
# trap that produced a "wrapper present, binary gone" state). Delete it manually later.
echo "left staged binary at $SRC (safe to delete once Cinder is confirmed)"
sync
umount /system 2>/dev/null
echo "== done. reboot to normal; Cinder paints ~15s after boot. =="
echo "   SAFETY: if Cinder reboots 3x within 60s it AUTO-DISABLES (stock UI returns)."
echo "   manual escape: create /contents/cinder_off, then reboot."
exit 0
