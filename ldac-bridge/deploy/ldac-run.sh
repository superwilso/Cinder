#!/system/bin/sh
# ldac-run.sh — file-triggered supervisor for the USB-DAC -> LDAC bridge.
#
# The device has no shell, so control is entirely via files on the storage root
# (visible/creatable over USB-MSC):
#   create /contents/ldac_on   -> start bridging (USB-DAC capture -> LDAC transmit)
#   remove /contents/ldac_on   -> stop bridging (idle, keep watching)
#   create /contents/ldac_off  -> exit the supervisor entirely
# Each bridge run logs to /contents/ldac.log (appended; includes the connect/socket
# diagnostics and any capture -EBUSY — the two things the on-device test must confirm).
#
# Launched at boot by the cinder wrapper ([ -x .../ldac-run.sh ] && ... &). Safe to
# run with or without Cinder; it only acts while /contents/ldac_on is present.
BIN=/system/vendor/unknown321/bin
BRIDGE=$BIN/cinder-ldac-bridge
LOG=/contents/ldac.log
PIDF=/contents/ldac.pid

# single-instance guard
if [ -f "$PIDF" ] && kill -0 "$(cat "$PIDF" 2>/dev/null)" 2>/dev/null; then
    exit 0
fi
echo $$ > "$PIDF"

while [ ! -f /contents/ldac_off ]; do
    if [ -f /contents/ldac_on ]; then
        echo "=== ldac-run: starting bridge $(date 2>/dev/null) ===" >> "$LOG"
        "$BRIDGE" >>"$LOG" 2>&1 &
        bpid=$!
        # bridge until it exits (e.g. capture error) or the trigger is removed
        while [ -f /contents/ldac_on ] && [ ! -f /contents/ldac_off ] && kill -0 "$bpid" 2>/dev/null; do
            sleep 1
        done
        kill "$bpid" 2>/dev/null
        echo "=== ldac-run: bridge stopped ===" >> "$LOG"
        # if it died on its own while still requested, back off to avoid a hot loop
        [ -f /contents/ldac_on ] && sleep 3
    else
        sleep 2
    fi
done
rm -f "$PIDF"
exit 0
