#!/bin/sh
# cinder-fastboot.sh — the `fast-boot` component: Sony's boot-time firmware checks, skipped while
# nothing has changed.
#
#   cinder-fastboot.sh install | remove | status
#
# Run as root with /system writable: by install_cinderhome.sh in Sony's updater, by
# uninstall_cinderhome.sh, and by tools/cinder-install.sh --fast-boot on a live player.
#
# WHY. init runs /system/usr/bin/optproc_for_boot.sh with `exec`, so every Sony service, and so
# Cinder, waits for it. It checks the touch panel, codec and NFC firmware on every boot, about
# 1.9 s on an NW-A55 (2026-11-15), though the firmware files only change with a Sony update.
#
# WHAT IT DOES. Sony's script is kept as optproc_for_boot.sh.precinder and the script below takes
# its place. That runs Sony's checks exactly as before, and when all three report nothing to do it
# keeps a stamp of the firmware files and the scripts that use them (/data/cinder/fwcheck). While
# the stamp still matches, later boots skip the checks. A changed file, or a check that did real
# work (an update), and Sony's checks run on every boot until all three have nothing to do again.
# A Sony update replaces the script with its own, which puts the checks back on every boot.
#
# Only Sony's script, by hash, is ever replaced; anything else and nothing is installed.

BB=/xbin/busybox
[ -x "$BB" ] || BB=/system/xbin/busybox
[ -x "$BB" ] || BB=busybox

OPT=/system/usr/bin/optproc_for_boot.sh
OPT_STOCK=8042d61671cc9a010c8333b0e611192092b025d539363833a80ed379c323665f
STAMP=/data/cinder/fwcheck
MARK=cinder-fastboot   # a string only our script carries

say() { echo "fast-boot: $*"; }
sha() { "$BB" sha256sum "$1" 2>/dev/null | "$BB" cut -d' ' -f1; }
ours() { "$BB" grep -q "$MARK" "$1" 2>/dev/null; }

skip_script() {
    "$BB" cat <<'SKIP'
#!/bin/sh
# Installed by Cinder's fast-boot component (cinder-fastboot.sh) in place of Sony's script, which
# is kept as optproc_for_boot.sh.precinder and does the work. Sony's firmware checks run until all
# three report nothing to do; then they are skipped while the files below are unchanged.
ORIG=/system/usr/bin/optproc_for_boot.sh.precinder
STAMP=/data/cinder/fwcheck
FW=/system/etc/firmware
U=/system/usr/bin

NOW=`busybox ls -l $FW/Sony_Tovis_*.bin $FW/cxd3778gf_ucom_fw_*.bin $FW/cxd225x_firmware.bin \
    $ORIG $U/optproc_for_boot_1291.sh $U/sony_tovis_updater.sh $U/cxd3778gf_ucom_fwupdate.sh \
    $U/nfc_cxd225x_updater.sh 2>&1 | busybox md5sum`

if [ -n "$NOW" ] && [ "`cat $STAMP 2>/dev/null`" = "$NOW" ]; then
    echo "[cinder-fastboot] firmware unchanged since all three checks found nothing to do - skipped"
    exit 0
fi

OUT=`/bin/sh $ORIG 2>&1`
echo "$OUT"
case "$OUT" in *"FW Update Not Exec"*) ;; *) exit 0 ;; esac   # touch panel: current
case "$OUT" in *"FW is latest"*) ;; *) exit 0 ;; esac         # codec microcontroller: current
case "$OUT" in *"skip downloading"*) ;; *) exit 0 ;; esac     # NFC: current
echo "$NOW" > $STAMP.tmp 2>/dev/null && mv $STAMP.tmp $STAMP 2>/dev/null
echo "[cinder-fastboot] all three firmware checks found nothing to do - later boots skip them"
exit 0
SKIP
}

# put_skip <dst> — the skip script to a temporary name, checked, renamed. Never writes in place.
put_skip() {
    skip_script > "$1.tmp" 2>/dev/null
    skip_script > "$1.want" 2>/dev/null
    if [ -s "$1.tmp" ] && "$BB" cmp -s "$1.tmp" "$1.want"; then
        "$BB" rm -f "$1.want"
        "$BB" chown 0:0 "$1.tmp" 2>/dev/null
        "$BB" chmod 755 "$1.tmp"
        "$BB" mv -f "$1.tmp" "$1" && return 0
    fi
    "$BB" rm -f "$1.tmp" "$1.want" 2>/dev/null
    return 1
}

case "${1:-}" in
install)
    H="$(sha "$OPT")"
    if ours "$OPT"; then
        if [ "$(sha "$OPT.precinder")" != "$OPT_STOCK" ]; then
            say "installed, but Sony's script is not kept beside it - left as it is"
            exit 1
        fi
        put_skip "$OPT" && say "installed (brought up to date)" || { say "could not update - the installed one stays"; exit 1; }
        exit 0
    fi
    if [ "$H" != "$OPT_STOCK" ]; then
        say "optproc_for_boot.sh is not the script this was written for (sha256 ${H:-unreadable}) - nothing installed"
        exit 1
    fi
    "$BB" cat "$OPT" > "$OPT.precinder.tmp" 2>/dev/null
    if [ "$(sha "$OPT.precinder.tmp")" != "$OPT_STOCK" ]; then
        "$BB" rm -f "$OPT.precinder.tmp" 2>/dev/null
        say "could not keep a copy of Sony's script - nothing installed"
        exit 1
    fi
    "$BB" chown 0:0 "$OPT.precinder.tmp" 2>/dev/null
    "$BB" chmod 755 "$OPT.precinder.tmp"
    "$BB" mv -f "$OPT.precinder.tmp" "$OPT.precinder" || { say "could not keep a copy of Sony's script - nothing installed"; exit 1; }
    if put_skip "$OPT"; then
        say "installed: Sony's firmware checks are skipped once they find nothing to do (Sony's script kept as $OPT.precinder)"
    else
        "$BB" rm -f "$OPT.precinder"
        say "could not install - Sony's script is unchanged"
        exit 1
    fi
    ;;
remove)
    ours "$OPT" || { "$BB" rm -f "$STAMP" 2>/dev/null; exit 0; }
    if [ "$(sha "$OPT.precinder")" != "$OPT_STOCK" ]; then
        say "no copy of Sony's script to put back - left as it is (it still runs Sony's checks when anything changes)"
        exit 1
    fi
    "$BB" cat "$OPT.precinder" > "$OPT.tmp" 2>/dev/null
    if [ "$(sha "$OPT.tmp")" = "$OPT_STOCK" ]; then
        "$BB" chown 0:0 "$OPT.tmp" 2>/dev/null
        "$BB" chmod 755 "$OPT.tmp"
        "$BB" mv -f "$OPT.tmp" "$OPT" && "$BB" rm -f "$OPT.precinder" "$STAMP" && say "removed: Sony's firmware checks run on every boot again"
    else
        "$BB" rm -f "$OPT.tmp" 2>/dev/null
        say "could not put Sony's script back - left as it is"
        exit 1
    fi
    ;;
status)
    if ours "$OPT"; then say "installed"; elif [ "$(sha "$OPT")" = "$OPT_STOCK" ]; then say "not installed (Sony's script)"; else say "not installed (unknown script)"; fi
    if [ -s "$STAMP" ]; then say "checks: skipped while the firmware is unchanged"; else say "checks: run at the next boot"; fi
    ;;
*)
    echo "usage: cinder-fastboot.sh install | remove | status"
    exit 2
    ;;
esac
exit 0
