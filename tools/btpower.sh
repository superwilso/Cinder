#!/usr/bin/env bash
# btpower.sh — measure what a listening session actually costs, so a battery change can be
# argued from a number instead of a hunch.
#
# WHY IT SAMPLES ON THE DEVICE AND NOT OVER ADB. Every adb command wakes the core: the CPU sits at
# 598 MHz on an idle device and reads 1.3 GHz the moment a shell attaches, and a cable also pins the
# battery gauge to "charging" so the level tells you nothing (see memory: reference_power_measurement).
# So the sampler runs detached on the device, takes cumulative counters at both ends of a window you
# spend WITH THE CABLE OUT, and the host does the arithmetic afterwards.
#
#   tools/btpower.sh start [label]    # take the opening sample, then unplug and listen
#   tools/btpower.sh report [label]   # replug, take the closing sample, print the deltas
#
# There is no daemon and no fixed window: the window is however long you left it. (The first
# version backgrounded a sampler on the device — adb kills the process group when its shell exits,
# so the closing sample never ran.)
#
# The point is the A/B. Run it three times — `idle`, `jack`, `bt` — and compare:
#   tools/btpower.sh start bt      (unplug, play over Bluetooth with the screen off, ~10 min)
#   tools/btpower.sh start jack    (same music, same length, down the 3.5 mm jack)
#   tools/btpower.sh start idle    (paused, same length)
# …each followed by `report <label>` once the cable is back in.
#
# What the codec registers are for: during Bluetooth playback the audio leaves through the radio,
# not the CXD3778GF — so if BLK_ON0/BLK_ON1, the oscillators or the headphone amp stages are still
# powered in the `bt` run, that is a DAC nobody is listening to, drawing current. That is the
# question this tool exists to answer.
set -uo pipefail

CMD="${1:-help}"; shift || true
LABEL="${1:-session}"
LABEL="$(printf '%s' "$LABEL" | tr -cd 'A-Za-z0-9_-')"
[ -n "$LABEL" ] || LABEL=session

SELF="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DEV_SCRIPT=/contents/cinder_btpower.sh
OUT="/contents/btpower_${LABEL}.txt"
export ADB_MDNS=0

adb_up() { [ "$(adb get-state 2>/dev/null)" = "device" ]; }
adb_up || { echo "ERROR: no adb device (dev-channel build, cable in)." >&2; exit 1; }

start() {
    adb push "$SELF/btpower_sampler.sh" "$DEV_SCRIPT" >/dev/null || exit 1
    adb shell "chmod 0755 $DEV_SCRIPT; sh $DEV_SCRIPT T0 $OUT; echo opened" >/dev/null
    cat <<TXT
opening sample taken -> $OUT

NOW, in this order:
  1. unplug the cable    (a cable pins the gauge to "charging", so the level would say nothing —
                          and it is also rung 0 of the escape ladder)
  2. start the music, put the screen to sleep
  3. leave it alone — ten minutes is enough to move a whole percent
  4. replug, then:  tools/btpower.sh report $LABEL
TXT
}

report() {
    local tmp="${TMPDIR:-/tmp}/btpower_${LABEL}.txt"
    adb shell "[ -f $OUT ] || echo MISSING; sh $DEV_SCRIPT T1 $OUT" | grep -q MISSING && {
        echo "no opening sample at $OUT — run: tools/btpower.sh start $LABEL" >&2; exit 1; }
    adb pull "$OUT" "$tmp" >/dev/null 2>&1 || { echo "could not pull $OUT" >&2; exit 1; }
    python3 "$SELF/btpower_report.py" "$tmp"
}

case "$CMD" in
    start)  start ;;
    report) report ;;
    *)      sed -n '2,30p' "$0"; exit 2 ;;
esac
