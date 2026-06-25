#!/bin/sh
# install_cinderhome.sh — exec_file payload; runs ONCE in the NWZ UPDATER as root
# (exec_file.sh already cleared the fup flag, so this is brick-safe). It installs the
# cinder-home easel Home app and makes appmgr launch it INSTEAD of the stock Qt
# HgrmMediaPlayerApp — the true Option-B replacement (no SIGSTOP overlay, no blit war).
#
# MECHANISM (least-invasive): the 9 MB Qt binary is left 100% untouched. We only swap the
# 78-byte HgrmMediaPlayerApp.appcfg so appmgr's `command:` points at a tiny launcher, which
# execs cinder-home. cinder-home registers with appmgr under the name "HgrmMediaPlayerApp"
# and completes the Foreground handshake via easel::ApplicationBase::run() — so appmgr is
# satisfied and does NOT reboot (see analysis/F_appmgr_home/RE_findings.md).
#
# SAFETY — BAD-BOOT COUNTER + AUTO-REVERT (the net for this FIRST on-device bring-up):
#   The launcher bumps /contents/cinderhome_bootcount each boot. If cinder-home fails to
#   reach Foreground (crash/timeout), appmgr reboots; after MAXBAD such boots the launcher
#   auto-creates /contents/cinderhome_off and execs the REAL Qt app -> stock UI returns on
#   its own in ~2-3 boot cycles, NO PC/wbrt needed. A boot that survives 60 s resets the
#   counter. Manual escape: create /contents/cinderhome_off (over USB-MSC) then reboot.
#   Full revert: flash cinder_home_uninstall.upg (restores .appcfg.real). Last resort: wbrt.
# The original Qt binary is never modified; only the .appcfg (backed up to .appcfg.real).
LOG=/contents/cinder_home_install.log
exec >>"$LOG" 2>&1
echo "================================================================"
echo "== cinder-home installer  $(date 2>/dev/null)"

VENDOR=/system/vendor/unknown321
BIN=$VENDOR/bin
SRC=/contents/cinder-home
SONYBIN=/system/vendor/sony/bin
APPCFG=$SONYBIN/HgrmMediaPlayerApp.appcfg
LAUNCH=$BIN/cinderhome-launch.sh

mount -t ext4 -o rw /emmc@android /system 2>/dev/null
mount -o remount,rw /emmc@android /system 2>/dev/null

# the staged binary must be present (user copies 'cinder-home' to the storage root first)
if [ ! -f "$SRC" ]; then
    echo "ERROR: $SRC not found — copy the 'cinder-home' binary to the storage root"
    echo "       (tools/flash.sh --push cinder-home/cinder-home) before flashing. ABORT (no changes)."
    sync; umount /system 2>/dev/null; exit 0
fi
if [ ! -f "$APPCFG" ]; then
    echo "ERROR: $APPCFG not found — wrong device/layout. ABORT (no changes)."
    sync; umount /system 2>/dev/null; exit 0
fi

# ensure the install dir exists (Wampy provides it; create if missing)
[ -d "$BIN" ] || mkdir -p "$BIN"

# 1) install the cinder-home binary (verified content copy; busybox cp is flaky -> use cat>)
cat "$SRC" > "$BIN/cinder-home" 2>/dev/null
if [ ! -s "$BIN/cinder-home" ]; then
    echo "ERROR: failed to install $BIN/cinder-home (copy failed/zero bytes). ABORT (no .appcfg change)."
    sync; umount /system 2>/dev/null; exit 0
fi
chmod 0755 "$BIN/cinder-home"
echo "installed binary: $BIN/cinder-home (present, non-empty)"

# 2) back up the ORIGINAL .appcfg BEFORE writing anything. If this fails we must NOT touch
#    the .appcfg (otherwise the stock launch config is lost with no .real to restore).
if [ ! -f "$APPCFG.real" ]; then
    cat "$APPCFG" > "$APPCFG.real" && chmod 0644 "$APPCFG.real"
    if [ ! -s "$APPCFG.real" ]; then
        echo "ERROR: failed to back up $APPCFG -> .appcfg.real. ABORT (no .appcfg change)."
        sync; umount /system 2>/dev/null; exit 0
    fi
    echo "backed up $APPCFG -> .appcfg.real"
fi

# 3) write the launcher (bad-boot counter + exec cinder-home, revert to stock on failure).
#    Quoted heredoc = written verbatim (no install-time expansion).
cat > "$LAUNCH" <<'LAUNCH_EOF'
#!/system/bin/sh
# cinder-home launcher — appmgr execs this (via the repointed .appcfg command:).
# It execs cinder-home (the replacement Home app) behind a BAD-BOOT COUNTER so a
# failed launch auto-reverts to the stock Qt app. Original Qt binary is untouched.
BOOTCOUNT=/contents/cinderhome_bootcount
MAXBAD=3
REAL=/system/vendor/sony/bin/HgrmMediaPlayerApp           # untouched stock Qt app
HOME_BIN=/system/vendor/unknown321/bin/cinder-home
# cinder-home needs the Sony easel/PlayerService libs + device libc++; make sure they're found.
export LD_LIBRARY_PATH="/system/vendor/sony/lib:/system/vendor/unknown321/lib:/system/lib:/usr/lib:/lib:$LD_LIBRARY_PATH"

# escape hatch / disabled / missing binary -> run stock, no counting
if [ -f /contents/cinderhome_off ] || [ ! -x "$HOME_BIN" ]; then
    exec "$REAL" "$@"
fi

# RECOVERY SAFETY: if USB is connected to a PC at launch, run STOCK. This guarantees a
# no-tools escape during this first bring-up — cinder-home doesn't manage USB-MSC yet, so
# "plug into the PC, then reboot" ALWAYS gives you the stock UI with working mass storage
# (to read logs or flash the uninstaller). Test cinder-home UNPLUGGED (on battery).
if [ "$(cat /sys/class/android_usb/android0/state 2>/dev/null)" = "CONFIGURED" ]; then
    echo "usb-connected-at-launch -> running stock for recovery" > /contents/cinderhome_usbskip 2>/dev/null
    exec "$REAL" "$@"
fi

# --- bad-boot counter ---
n=0
[ -f "$BOOTCOUNT" ] && n=$(cat "$BOOTCOUNT" 2>/dev/null)
[ -z "$n" ] && n=0
n=$((n + 1))
echo "$n" > "$BOOTCOUNT"
if [ "$n" -ge "$MAXBAD" ]; then
    # cinder-home failed to survive too many times -> disable + revert to stock.
    touch /contents/cinderhome_off /contents/cinderhome_DISABLED_badboot
    exec "$REAL" "$@"
fi

# heartbeat: if this boot survives 60 s, it's good -> reset the counter.
( sleep 60; echo 0 > "$BOOTCOUNT" ) &

# hand over to cinder-home (replaces this process; keeps the appmgr-expected name/args).
exec "$HOME_BIN" "$@" >/contents/cinderhome.log 2>&1
LAUNCH_EOF
chmod 0755 "$LAUNCH"
echo "wrote launcher: $LAUNCH"

# 4) repoint the .appcfg command: at the launcher (keep name/type/hidden = the Home contract).
cat > "$APPCFG" <<'APPCFG_EOF'
name: HgrmMediaPlayerApp
command: /system/vendor/unknown321/bin/cinderhome-launch.sh
type: Home
hidden: false
APPCFG_EOF
chmod 0644 "$APPCFG"
echo "repointed $APPCFG command: -> $LAUNCH"

# fresh install = enabled: clear any prior disable/bad-boot flags.
rm -f /contents/cinderhome_off /contents/cinderhome_bootcount /contents/cinderhome_DISABLED_badboot 2>/dev/null
echo "cleared prior disable flags (fresh install = enabled)"
echo "left staged binary at $SRC (safe to delete once cinder-home is confirmed)"
sync
umount /system 2>/dev/null
echo "== done. reboot to normal; appmgr launches cinder-home as the Home app. =="
echo "   SAFETY: if it fails to foreground 3x it AUTO-REVERTS to the stock Qt UI (~2-3 boots)."
echo "   manual escape: create /contents/cinderhome_off, then reboot."
echo "   logs: /contents/cinderhome.log (cinder-home stdout/stderr)."
exit 0
