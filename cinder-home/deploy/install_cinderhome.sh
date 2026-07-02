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
# UPDATER TOOLING (hardened 2026-07-01 after a false-abort on the first flash): the NWZ
#   updater's AMBIENT shell utilities are unreliable — a bare `wc -c` returned 0 (→ the
#   size-sanity check false-aborted a GOOD copy) and `rm -f` choked on its own flag. Wampy
#   avoids this by invoking `/xbin/busybox <cmd>` for every op; we now do the same, so the
#   brick-critical .appcfg write no longer rides on those flaky tools. A runtime fallback
#   (/xbin/busybox → /system/xbin/busybox → bare) covers layout variance. mount/umount/sync
#   stay ambient — exec_file.sh already proved the updater's own mount works (it remounts
#   /contents rw and our log lands there).
#
# SAFETY — BAD-BOOT COUNTER + AUTO-REVERT (hardened 2026-06-26 after a hung launch needed wbrt):
#   The launcher increments /contents/cinderhome_bootcount each boot; the counter is reset to 0
#   ONLY by cinder-home once it proves HEALTHY (painted + survived its risky init). So a crash OR
#   HANG never resets it -> it accumulates and after MAXBAD=2 boots the launcher execs the stock
#   Qt app -> stock UI returns on its own, NO PC/wbrt. (The old blind 60s-timer reset is removed:
#   a hung process "survived" it -> the counter never accumulated -> soft-brick.) Plus a ~2s
#   pre-launch window: connect USB (or create /contents/cinderhome_off) -> boot stock immediately.
#   All writes here are ATOMIC (temp+verify+mv) + a FINAL SANITY GATE reverts to stock if any
#   piece is wrong, so a partial install can't soft-brick. Full revert: cinder_home_uninstall.upg.
# The original Qt binary is never modified; only the .appcfg (backed up to .appcfg.real).
LOG=/contents/cinder_home_install.log
exec >>"$LOG" 2>&1
echo "================================================================"
echo "== cinder-home installer  $(date 2>/dev/null)"

# ── busybox anchor ─────────────────────────────────────────────────────────────────────────
# Route every file op through the updater's known-good busybox (see UPDATER TOOLING above).
BB=/xbin/busybox
[ -x "$BB" ] || BB=/system/xbin/busybox
[ -x "$BB" ] || BB=busybox      # last resort: whatever is on PATH (may be the flaky one)
echo "busybox: $BB ($("$BB" 2>&1 | head -1 2>/dev/null))"

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
[ -d "$BIN" ] || "$BB" mkdir -p "$BIN"

# 1) install the cinder-home binary ATOMICALLY (write temp -> verify -> mv). A truncated binary
#    must never become the live one. (busybox cp is flaky -> use cat>.)
"$BB" cat "$SRC" > "$BIN/cinder-home.tmp" 2>/dev/null
if [ ! -s "$BIN/cinder-home.tmp" ]; then
    echo "ERROR: failed to stage $BIN/cinder-home (copy failed/zero bytes). ABORT (no .appcfg change)."
    "$BB" rm -f "$BIN/cinder-home.tmp" 2>/dev/null; sync; umount /system 2>/dev/null; exit 0
fi
# size sanity: the binary is ~2.6 MB. Measure with busybox (the ambient wc returned 0 and
# false-aborted the first flash). Compare against the SOURCE size too. Only abort on a
# *measured* implausibly-small file; if size is genuinely unmeasurable, the -s check above
# already proved the file is non-empty, so proceed and let the bad-boot counter be the net.
sz=$("$BB" wc -c < "$BIN/cinder-home.tmp" 2>/dev/null | "$BB" tr -cd '0-9')
srcsz=$("$BB" wc -c < "$SRC" 2>/dev/null | "$BB" tr -cd '0-9')
case "$sz"    in ''|*[!0-9]*) sz=-1;;    esac
case "$srcsz" in ''|*[!0-9]*) srcsz=-1;; esac
echo "staged size: $sz bytes (source $srcsz bytes)"
if [ "$sz" -ge 0 ] && [ "$sz" -lt 1000000 ]; then
    echo "ERROR: staged binary only $sz bytes (expected ~2.6MB) — partial copy. ABORT (no .appcfg change)."
    "$BB" rm -f "$BIN/cinder-home.tmp" 2>/dev/null; sync; umount /system 2>/dev/null; exit 0
fi
if [ "$sz" -ge 0 ] && [ "$srcsz" -ge 0 ] && [ "$sz" != "$srcsz" ]; then
    echo "ERROR: staged $sz != source $srcsz bytes — truncated copy. ABORT (no .appcfg change)."
    "$BB" rm -f "$BIN/cinder-home.tmp" 2>/dev/null; sync; umount /system 2>/dev/null; exit 0
fi
[ "$sz" -lt 0 ] && echo "WARN: size unmeasurable even via busybox; file is non-empty (-s passed) — proceeding; bad-boot counter is the net."
"$BB" chmod 0755 "$BIN/cinder-home.tmp"
"$BB" mv -f "$BIN/cinder-home.tmp" "$BIN/cinder-home"
echo "installed binary: $BIN/cinder-home ($sz bytes)"

# 2) back up the ORIGINAL .appcfg BEFORE writing anything. If this fails we must NOT touch
#    the .appcfg (otherwise the stock launch config is lost with no .real to restore).
if [ ! -f "$APPCFG.real" ]; then
    "$BB" cat "$APPCFG" > "$APPCFG.real" && "$BB" chmod 0644 "$APPCFG.real"
    if [ ! -s "$APPCFG.real" ]; then
        echo "ERROR: failed to back up $APPCFG -> .appcfg.real. ABORT (no .appcfg change)."
        sync; umount /system 2>/dev/null; exit 0
    fi
    echo "backed up $APPCFG -> .appcfg.real"
fi

# 3) write the launcher ATOMICALLY (temp -> verify -> mv). A truncated launcher would fail to
#    exec cinder-home AND never run the counter -> no auto-revert. Quoted heredoc = verbatim.
#    (The launcher runs at NORMAL boot, where /system/bin/sh + standard tools are available.)
"$BB" cat > "$LAUNCH.tmp" <<'LAUNCH_EOF'
#!/system/bin/sh
# cinder-home launcher — appmgr execs this (via the repointed .appcfg command:). It runs
# cinder-home behind a BAD-BOOT COUNTER + an escape window so a failed/HUNG launch reverts
# to the stock Qt app WITHOUT a wbrt restore. The stock Qt binary is never modified.
#
# SAFETY MODEL (rewritten 2026-06-26 after a hung launch required wbrt):
#  * The counter is incremented HERE every boot and persisted (sync). It is reset to 0 ONLY by
#    cinder-home itself, after it has proven healthy (painted + survived its risky init). So a
#    HANG — which never resets the counter — ACCUMULATES across (force-)reboots and auto-reverts
#    after MAXBAD. (The old launcher reset the counter on a blind 60 s timer, which a hung
#    process "survives" → it never accumulated → soft-brick. That bug is removed.)
#  * A PRE-LAUNCH ESCAPE WINDOW lets you bail to stock fast: power the device off (hold Power
#    ~8 s if hung), then during this window connect USB *or* leave an escape file → stock.
BOOTCOUNT=/contents/cinderhome_bootcount
MAXBAD=2
REAL=/system/vendor/sony/bin/HgrmMediaPlayerApp           # untouched stock Qt app
HOME_BIN=/system/vendor/unknown321/bin/cinder-home
export LD_LIBRARY_PATH="/system/vendor/sony/lib:/system/vendor/unknown321/lib:/system/lib:/usr/lib:/lib:$LD_LIBRARY_PATH"

run_stock() { exec "$REAL" "$@"; }
usb_connected() {
    for p in /sys/class/android_usb/android0/state /sys/class/power_supply/usb/online \
             /sys/class/power_supply/usb/present /sys/class/power_supply/usb/uevent; do
        v=$(cat "$p" 2>/dev/null) || continue
        case "$v" in *CONFIGURED*|*POWER_SUPPLY_ONLINE=1*) return 0;; esac
        [ "$v" = "1" ] && return 0
    done
    return 1
}

# explicit disable / missing binary -> stock, no counting
[ -f /contents/cinderhome_off ] && run_stock "$@"
[ ! -x "$HOME_BIN" ] && run_stock "$@"

# bad-boot counter: increment + persist FIRST. cinder-home resets it once healthy.
n=0; [ -f "$BOOTCOUNT" ] && n=$(cat "$BOOTCOUNT" 2>/dev/null)
# a partial write could leave non-numeric garbage -> treat as 0 (don't let `$(())`/`[ -ge ]` error)
case "$n" in ''|*[!0-9]*) n=0;; esac
n=$((n + 1)); echo "$n" > "$BOOTCOUNT"; sync
if [ "$n" -ge "$MAXBAD" ]; then
    touch /contents/cinderhome_off /contents/cinderhome_DISABLED_badboot; sync
    run_stock "$@"
fi

# pre-launch escape window (~3 s): connect USB or drop /contents/cinderhome_off -> stock.
i=0
while [ "$i" -lt 2 ]; do
    [ -f /contents/cinderhome_off ] && run_stock "$@"
    if usb_connected; then
        echo "usb-at-launch -> stock for recovery" > /contents/cinderhome_escape 2>/dev/null
        run_stock "$@"
    fi
    sleep 1
    i=$((i + 1))
done

# optional USB-DAC->LDAC bridge supervisor (no-op if the bridge isn't installed). Started
# HERE because appmgr execs only this launcher at boot — nothing else starts it.
[ -x /system/vendor/unknown321/bin/ldac-run.sh ] && \
    /system/vendor/unknown321/bin/ldac-run.sh >/dev/null 2>&1 &

# hand over to cinder-home (replaces this process; keeps the appmgr-expected name/args).
exec "$HOME_BIN" "$@" >/contents/cinderhome.log 2>&1
LAUNCH_EOF
# verify the launcher wrote fully (must contain its final exec line) before activating it
if ! "$BB" grep -q 'exec "\$HOME_BIN"' "$LAUNCH.tmp" 2>/dev/null; then
    echo "ERROR: launcher write was truncated. ABORT (no .appcfg change; stock intact)."
    "$BB" rm -f "$LAUNCH.tmp" 2>/dev/null; sync; umount /system 2>/dev/null; exit 0
fi
"$BB" chmod 0755 "$LAUNCH.tmp"
"$BB" mv -f "$LAUNCH.tmp" "$LAUNCH"
echo "wrote launcher: $LAUNCH"

# 4) repoint the .appcfg command: at the launcher, ATOMICALLY. This is THE most brick-sensitive
#    write: a truncated/empty .appcfg means appmgr can't launch ANY Home app, and the launcher
#    (hence the bad-boot counter) never runs -> unrecoverable soft-brick. So: write temp, VERIFY
#    it parses, then mv over the live one (rename is atomic on one fs). Keep name/type/hidden =
#    the stock Home contract; matches the stock 4-line format exactly.
"$BB" cat > "$APPCFG.tmp" <<'APPCFG_EOF'
name: HgrmMediaPlayerApp
command: /system/vendor/unknown321/bin/cinderhome-launch.sh
type: Home
hidden: false
APPCFG_EOF
if ! "$BB" grep -q '^command: /system/vendor/unknown321/bin/cinderhome-launch.sh$' "$APPCFG.tmp" \
   || ! "$BB" grep -q '^type: Home$' "$APPCFG.tmp"; then
    echo "ERROR: new .appcfg failed verification — NOT activating (stock .appcfg untouched)."
    "$BB" rm -f "$APPCFG.tmp" 2>/dev/null; sync; umount /system 2>/dev/null; exit 0
fi
"$BB" chmod 0644 "$APPCFG.tmp"
"$BB" mv -f "$APPCFG.tmp" "$APPCFG"
echo "repointed $APPCFG command: -> $LAUNCH"

# ── FINAL SANITY GATE ─────────────────────────────────────────────────────────────────────
# A half/broken install must boot to working STOCK, never soft-brick. Verify every piece the
# boot path needs; if ANY is wrong, restore the stock .appcfg (revert) before rebooting.
ok=1
[ -x "$BIN/cinder-home" ]    || { echo "sanity: cinder-home not executable"; ok=0; }
[ -x "$LAUNCH" ]             || { echo "sanity: launcher not executable"; ok=0; }
"$BB" grep -q 'cinderhome-launch.sh' "$APPCFG" 2>/dev/null || { echo "sanity: .appcfg not repointed"; ok=0; }
[ -x "$SONYBIN/HgrmMediaPlayerApp" ] || { echo "sanity: STOCK revert target missing!"; ok=0; }
[ -s "$APPCFG.real" ]       || { echo "sanity: .appcfg.real backup missing"; ok=0; }
if [ "$ok" != 1 ]; then
    echo "!! SANITY FAILED — reverting .appcfg to stock so the device boots normally."
    if [ -s "$APPCFG.real" ]; then
        "$BB" cat "$APPCFG.real" > "$APPCFG.tmp" && "$BB" mv -f "$APPCFG.tmp" "$APPCFG"
        echo "   restored stock .appcfg."
    fi
    sync; umount /system 2>/dev/null
    echo "== install ABORTED safely; device will boot the stock UI. =="
    exit 0
fi

# fresh install = enabled: clear any prior disable/bad-boot flags.
"$BB" rm -f /contents/cinderhome_off /contents/cinderhome_bootcount /contents/cinderhome_DISABLED_badboot 2>/dev/null
echo "cleared prior disable flags (fresh install = enabled)"
echo "left staged binary at $SRC (safe to delete once cinder-home is confirmed)"
sync
umount /system 2>/dev/null
echo "== done. reboot to normal; appmgr launches cinder-home as the Home app. =="
echo "   SAFETY: a failed/hung launch AUTO-REVERTS to stock after 2 boots (no wbrt)."
echo "   fast escape: during the ~3s pre-launch window, connect USB (or create"
echo "   /contents/cinderhome_off) to boot stock. logs: /contents/cinderhome.log."
exit 0
