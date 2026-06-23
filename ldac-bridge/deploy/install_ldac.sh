#!/bin/sh
# install_ldac.sh — exec_file payload; runs ONCE in the NWZ UPDATER as root
# (exec_file.sh already cleared the fup flag, so this is brick-safe). Installs the
# USB-DAC -> LDAC bridge daemon + its file-triggered supervisor into /system.
#
# The binary is staged by the user first: copy `cinder-ldac-bridge` to the storage
# root (it appears at /contents/cinder-ldac-bridge) before flashing. This script
# moves it into /system and drops the supervisor alongside it.
#
# RUNS with the Cinder boot hook: the cinder wrapper launches ldac-run.sh at boot if
# present, so after this install + a reboot the bridge is controllable via files:
#   create /contents/ldac_on  -> start ; remove it -> stop. See ldac-bridge/TEST.md.
# Full revert: flash ldac_uninstall.upg (or just `rm` the two files over USB-MSC).
LOG=/contents/ldac_install.log
exec >>"$LOG" 2>&1
echo "================================================================"
echo "== ldac bridge installer  $(date 2>/dev/null)"

VENDOR=/system/vendor/unknown321
BIN=$VENDOR/bin
SRC=/contents/cinder-ldac-bridge
SUP=/contents/ldac-run.sh

mount -t ext4 -o rw /emmc@android /system 2>/dev/null
mount -o remount,rw /emmc@android /system 2>/dev/null
if [ ! -d "$BIN" ]; then
    echo "ERROR: $BIN not found — is Wampy installed? aborting (no changes)."
    sync; exit 0
fi
if [ ! -f "$SRC" ]; then
    echo "ERROR: $SRC not found — copy the 'cinder-ldac-bridge' binary to the"
    echo "       storage root before flashing. aborting (no changes)."
    sync; exit 0
fi

# copy + VERIFY non-empty before removing the source (busybox here has no `wc`)
cp "$SRC" "$BIN/cinder-ldac-bridge"
if [ ! -s "$BIN/cinder-ldac-bridge" ]; then
    echo "ERROR: failed to install $BIN/cinder-ldac-bridge (cp failed or zero bytes)."
    echo "       leaving staged $SRC in place. Re-push & retry."
    sync; umount /system 2>/dev/null; exit 0
fi
chmod 0755 "$BIN/cinder-ldac-bridge"
echo "installed binary: $BIN/cinder-ldac-bridge (present, non-empty)"

# supervisor: prefer a staged copy; otherwise write the known-good script inline
if [ -f "$SUP" ]; then
    cp "$SUP" "$BIN/ldac-run.sh"
else
    cat > "$BIN/ldac-run.sh" <<'SUP_EOF'
#!/system/bin/sh
BIN=/system/vendor/unknown321/bin
BRIDGE=$BIN/cinder-ldac-bridge
LOG=/contents/ldac.log
PIDF=/contents/ldac.pid
if [ -f "$PIDF" ] && kill -0 "$(cat "$PIDF" 2>/dev/null)" 2>/dev/null; then exit 0; fi
echo $$ > "$PIDF"
while [ ! -f /contents/ldac_off ]; do
    if [ -f /contents/ldac_on ]; then
        echo "=== ldac-run: starting bridge $(date 2>/dev/null) ===" >> "$LOG"
        "$BRIDGE" >>"$LOG" 2>&1 &
        bpid=$!
        while [ -f /contents/ldac_on ] && [ ! -f /contents/ldac_off ] && kill -0 "$bpid" 2>/dev/null; do sleep 1; done
        kill "$bpid" 2>/dev/null
        echo "=== ldac-run: bridge stopped ===" >> "$LOG"
        [ -f /contents/ldac_on ] && sleep 3
    else
        sleep 2
    fi
done
rm -f "$PIDF"
exit 0
SUP_EOF
fi
chmod 0755 "$BIN/ldac-run.sh"
echo "installed supervisor: $BIN/ldac-run.sh"

if [ ! -f "$BIN/scrobbler.real" ]; then
    echo "NOTE: cinder wrapper not detected (no scrobbler.real). Install Cinder so the"
    echo "      boot hook launches ldac-run.sh, or launch it by another hook."
fi

rm -f "$SRC" "$SUP" 2>/dev/null
sync
umount /system 2>/dev/null
echo "== done. reboot, then: create /contents/ldac_on to start; remove it to stop. =="
echo "   watch /contents/ldac.log for socket/connect + capture diagnostics."
exit 0
