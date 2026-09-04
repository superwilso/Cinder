#!/system/bin/sh
# pmtest.sh — off-cable suspend/resume test that leaves evidence behind.
#
# WHY THIS EXISTS. The 2026-09-04 attempt ended with a dark screen and no USB, and that was read as
# "the device never resumed". It is equally consistent with "it resumed, but the display and the USB
# gadget did not come back" — and the forced reboot destroyed the only counters that could tell the
# two apart. This script makes the difference observable from outside:
#
#   * the log lives on /contents (vfat, survives a reboot) and is sync'd after every line
#   * /proc/uptime counts time spent suspended, so a jump between two samples taken 1 s apart IS
#     the length of a suspend — that is the detector
#   * the RED led is lit the moment a resume is detected. It is off in normal operation and is not
#     the charge indicator (that is green), so it cannot be confused for anything else
#
# SELF-RECOVERY (added after run 1, 2026-09-04). Run 1 proved the device suspends and resumes fine
# — resume_count reached 5 — but it never came back to a usable state, because NOTHING writes `on`
# back to /sys/power/state. Left alone it just cycles: suspend, wake on the timer, stay up for the
# ~20 s of Sony's resume wakelock (icx_pm_helper resume_lock_ms=20000), suspend again. The screen
# stays dark and a replug during a suspended window does nothing, which is what "wedged" looked
# like. So this now writes `on` itself after CYCLES resumes and disarms the wake timer, and the
# device returns on its own with no forced reboot.
#
# usage: pmtest.sh [delay_before_suspend] [seconds_to_watch] [dry] [cycles]
#        dry    = run the whole harness but DO NOT write /sys/power/state
#        cycles = how many suspend/resume cycles to allow before recovering (default 3)
#
# POWER-KEY MODE: `pmtest.sh 30 300 powerkey 1` arms a LONG wake timer (300 s) so a timer wake is
# unlikely to beat you to it, suspends once, and waits for you to press Power. The point is the
# `r12` value in the resulting SUSPEND RETURNED line: bit 6 (0x40) is EINT, which is how the power
# key reaches SPM (MT6323 PMIC = EINT 150). Every wake measured so far has been GPT (0x20), i.e. a
# timer — the power-key path is derived from the register tables and has NOT been observed yet.
# This is the test that settles it.
set -u
# The cable is going to be pulled while this runs, which kills adbd and SIGHUPs everything it
# started. Ignore that — surviving the unplug is the entire point of this script.
trap '' 1 2 15   # HUP INT TERM — this shell's trap wants numbers, not names
DELAY=${1:-25}
DUR=${2:-240}
DRY=${3:-}
CYCLES=${4:-3}
# powerkey mode is a real run (not dry) with a long timer, so the button is the likely waker.
PWAKE=30
if [ "$DRY" = "powerkey" ]; then PWAKE=300; DRY=""; fi
M=/contents/cinder_pm.log
PM=/sys/devices/platform/icx_pm_helper

up() { cut -d. -f1 /proc/uptime; }
note() { echo "$*" >> $M; sync; }

G=/sys/class/android_usb/android0
U=/sys/devices/platform/mt_usb/musb-hdrc.0
usb_state() {
    echo "enable=$(cat $G/enable 2>/dev/null) state=$(cat $G/state 2>/dev/null) fn=$(cat $G/functions 2>/dev/null) online=$(cat /sys/class/power_supply/usb/online 2>/dev/null) mode=$(cat $U/mode 2>/dev/null) irq=$(awk '/musb/{print $2}' /proc/interrupts)"
}

# After a suspend/resume the host never sees the device again — adb stays down until a reboot.
# Run 3 showed the gadget layer is NOT the problem: `enable` was already 1 and `functions` already
# `adb`, so bouncing enable changed nothing. The fault is lower down (musb/PHY/cable-detect), and
# it only becomes visible once a cable is actually present.
#
# So this waits for the plug rather than guessing blind, then escalates through the candidate
# repairs and records the state after each. Whichever one turns `state` into CONFIGURED is the fix;
# one run answers it instead of one reboot per guess.
usb_repair() {
    note "usb: post-resume  $(usb_state)"
    note "usb: waiting up to 120s for a cable — PLUG IN NOW"
    j=0
    healed=0
    while [ "$j" -lt 120 ]; do
        if [ "$(cat /sys/class/power_supply/usb/online 2>/dev/null)" = "1" ]; then
            if [ "$(cat $G/state 2>/dev/null)" = "CONFIGURED" ]; then
                note "usb: CONFIGURED on its own after ${j}s — no repair needed. $(usb_state)"
                healed=1
                break
            fi
            # Cable is present but the gadget never configured. Escalate.
            note "usb: cable seen at ${j}s but not configured — $(usb_state)"

            note "usb: [1] bouncing enable"
            echo 0 > $G/enable 2>/dev/null; sleep 2; echo 1 > $G/enable 2>/dev/null; sleep 3
            note "usb: [1] -> $(usb_state)"
            [ "$(cat $G/state 2>/dev/null)" = "CONFIGURED" ] && { note "usb: FIXED by [1] enable bounce"; healed=1; break; }

            note "usb: [2] re-setting functions"
            echo 0 > $G/enable 2>/dev/null; sleep 1
            echo none > $G/functions 2>/dev/null; sleep 1
            echo adb > $G/functions 2>/dev/null; sleep 1
            echo 1 > $G/enable 2>/dev/null; sleep 3
            note "usb: [2] -> $(usb_state)"
            [ "$(cat $G/state 2>/dev/null)" = "CONFIGURED" ] && { note "usb: FIXED by [2] functions re-set"; healed=1; break; }

            note "usb: [3] forcing musb OTG mode to b_peripheral"
            echo b_peripheral > $U/mode 2>/dev/null; sleep 3
            note "usb: [3] -> $(usb_state)"
            [ "$(cat $G/state 2>/dev/null)" = "CONFIGURED" ] && { note "usb: FIXED by [3] musb mode"; healed=1; break; }

            note "usb: ALL REPAIRS FAILED — dmesg tail follows"
            dmesg 2>/dev/null | grep -iE "musb|usb|charger|vbus" | tail -40 >> $M
            sync
            break
        fi
        j=$((j + 1))
        sleep 1
    done
    [ "$healed" = "0" ] && note "usb: unresolved (healed=0)"
    note "usb: final  $(usb_state)"
}

: > $M
note "=== pmtest armed  date=$(date)  uptime=$(up)s  dry=${DRY:-no} ==="
note "pre  $(grep 'dpidle_cnt\[0\]' /sys/power/idle_state)"
note "pre  $(grep by_vtg /sys/power/dpidle_state)"
echo "$PWAKE" > /sys/module/mt_sleep/parameters/slp_pwake_time 2>/dev/null
note "pre  resume_count=$(cat $PM/resume_count)  pwake=$(cat /sys/module/mt_sleep/parameters/slp_pwake_time)"
note "sleeping ${DELAY}s before arming — unplug NOW"

sleep "$DELAY"

if [ "$DRY" = "dry" ]; then
    note "--- DRY RUN: not writing /sys/power/state ---"
else
    note "--- writing mem to /sys/power/state at uptime=$(up)s ---"
    echo mem > /sys/power/state
fi

# Heartbeat. Each pass sleeps 1 s, so any uptime delta beyond a couple of seconds is time the CPU
# spent powered down. This loop is the only thing that can observe that, because it is also the
# thing that stops running while it happens.
prev=$(up)
i=0
woke=0
while [ "$i" -lt "$DUR" ]; do
    now=$(up)
    gap=$((now - prev))
    if [ "$gap" -gt 3 ]; then
        woke=$((woke + 1))
        note "*** SUSPEND RETURNED: gap=${gap}s at uptime=${now}s  rc=$(cat $PM/resume_count) r12=$(cat $PM/spm_r12) timer_out=$(cat $PM/spm_timer_out)"
        # r12 bit 6 (0x40) = EINT = the power key path. Anything else is a timer.
        note "    wake_kt: suspend_kt=$(cat $PM/suspend_kt) resume_kt=$(cat $PM/resume_kt) post=$(cat $PM/post_suspend_kt)"
        note "    eint_sta=$(cat $PM/eint_sta)"
        # Visible, immediate, and independent of the display coming back.
        echo 255 > /sys/class/leds/red/brightness 2>/dev/null
        echo 128 > /sys/class/leds/lcd-backlight/brightness 2>/dev/null

        # Recover. This runs inside an awake window, which is the only time anything can run.
        if [ "$woke" -ge "$CYCLES" ]; then
            note "--- ${woke} cycles done: disarming and writing 'on' to leave suspend ---"
            echo -1 > /sys/module/mt_sleep/parameters/slp_pwake_time
            echo on > /sys/power/state
            usb_repair
            note "recovered: autosleep=$(cat /sys/power/autosleep) pwake=$(cat /sys/module/mt_sleep/parameters/slp_pwake_time)"
            break
        fi
    fi
    echo "t=$i up=${now} rc=$(cat $PM/resume_count) r12=$(cat $PM/spm_r12) $(grep -o 'dpidle_cnt\[0\]=[0-9]*' /sys/power/idle_state)" >> $M
    sync
    prev=$now
    i=$((i + 1))
    sleep 1
done

# Belt and braces: if the loop ran out before CYCLES resumes happened, still leave the device in a
# usable state rather than cycling forever.
if [ "$(cat /sys/power/autosleep)" != "off" ]; then
    note "--- watch window ended while still suspending: forcing recovery ---"
    echo -1 > /sys/module/mt_sleep/parameters/slp_pwake_time
    echo on > /sys/power/state
fi
note "=== watch window over, woke=${woke} ==="
note "post $(grep 'dpidle_cnt\[0\]' /sys/power/idle_state)"
note "post $(grep by_vtg /sys/power/dpidle_state)"
note "post resume_count=$(cat $PM/resume_count) r12=$(cat $PM/spm_r12) eint_sta=$(cat $PM/eint_sta)"
note "post suspend_ts=$(cat $PM/suspend_ts) resume_ts=$(cat $PM/resume_ts)"
echo 255 > /sys/class/leds/red/brightness 2>/dev/null
