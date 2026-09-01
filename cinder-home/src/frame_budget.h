/* frame_budget.h — how long the render loop may sleep, given what is still owed.
 *
 * THE REPORT. "3.5 mm volume is not responsive when the screen is off." The volume path is not at
 * fault. The frame pacing is.
 *
 * THE STATE. With the panel dark the loop sleeps in poll() on the input nodes, budgeted to the next
 * 1 Hz housekeeping deadline — up to a full second. That is deliberate and it is the single biggest
 * battery lever in the app: dark is the longest-lived state a music player has, and poll() returns
 * immediately on an EVENT, so a long budget costs nothing in wake latency. The press that wakes the
 * device has always landed in the very next iteration.
 *
 * WHAT IT MISSED. Two things the volume rocker needs are not events. They are DEADLINES, and they
 * are serviced at the TOP of the loop, once per iteration:
 *
 *   vol_repeat_tick()  the synthesized ramp — a step every VOL_REPEAT_EVERY_MS while the rocker is
 *                      held, after an initial VOL_REPEAT_DELAY_MS. Nothing generates an event for
 *                      the next step; the button is already down.
 *   volume_flush()     the trailing write. An amixer write is a fork+exec of /bin/sh, so steps are
 *                      coalesced to one write per VOL_WRITE_EVERY_MS and the level the user
 *                      actually stopped on is written afterwards, from g_vol_pending.
 *
 * Awake the budget is 16 ms and both are served without anyone thinking about it. Dark, nothing was
 * waking the loop for either. Release the rocker within VOL_WRITE_EVERY_MS of the last write and
 * the final level sat in g_vol_pending until the next housekeeping tick — up to a second of nothing
 * after you stopped pressing. That is what "not responsive" describes.
 *
 * WHY IT ONLY SHOWS WITH THE SCREEN OFF *DELIBERATELY*. An idle blank sets g_screen_auto_off, and
 * input_pump calls screen_auto_wake() on any non-Power key — so a rocker press on an idle-blanked
 * panel lights it, the budget snaps to 16 ms, and the defect is invisible. Power-off clears
 * g_screen_auto_off (screen_toggle "takes ownership of the panel state"), so there the panel stays
 * dark under the press. Pocket case, which is the one the rocker exists for.
 *
 * THE RULE. Never sleep past work that is already owed. This computes the earliest such deadline;
 * the caller takes the minimum of it and the housekeeping budget.
 *
 * WHAT IT MUST NOT DO is shorten the sleep when nothing is owed. The 1000 ms budget is worth ~9
 * fewer wakeups a second in the state the device spends hours in, and a fix that traded that away
 * to make a two-second interaction feel right would be a bad trade. So this returns "no deadline"
 * unless a rocker is actually held or a write is actually pending — seconds at a time, not the
 * pocket.
 *
 * Pure arithmetic on times passed in, so tools/framebudget_selftest.cpp checks the SHIPPING rule
 * rather than a copy of it. See vol_ramp.h and bt_poll.h for the same pattern.
 */
#ifndef CINDER_FRAME_BUDGET_H
#define CINDER_FRAME_BUDGET_H

/* Returned when nothing is owed: the caller keeps its housekeeping budget untouched. */
#define CINDER_NO_DEADLINE (-1L)

/* `vol_btn`      >= 0 while the rocker is held, else < 0.
 * `vol_down_ms`  when that press landed.
 * `vol_last_ms`  when the last ramp step was emitted.
 * `vol_pending`  >= 0 when a coalesced level is waiting to be written, else < 0.
 * `vol_write_ms` when the last actual write happened.
 * Returns the absolute time of the earliest deadline, or CINDER_NO_DEADLINE.
 */
static inline long cinder_vol_deadline(int vol_btn, long vol_down_ms, long vol_last_ms,
                                       int vol_pending, long vol_write_ms,
                                       long repeat_delay_ms, long repeat_every_ms,
                                       long write_every_ms) {
    long deadline = CINDER_NO_DEADLINE;
    if (vol_btn >= 0) {
        /* Still inside the pre-ramp delay: the deadline is when the ramp is allowed to START.
         * Using the step interval here instead would wake the loop repeatedly through the delay
         * for a tap that is never going to become a ramp. */
        long ramp_at = (vol_last_ms < vol_down_ms + repeat_delay_ms)
                     ? vol_down_ms + repeat_delay_ms
                     : vol_last_ms + repeat_every_ms;
        deadline = ramp_at;
    }
    if (vol_pending >= 0) {
        long flush_at = vol_write_ms + write_every_ms;
        if (deadline == CINDER_NO_DEADLINE || flush_at < deadline) deadline = flush_at;
    }
    return deadline;
}

/* Fold a deadline into a sleep budget. `budget_ms` is what the caller would otherwise sleep;
 * `now_ms` is the current time. Never returns less than 1 — a deadline already in the past means
 * "go round again now", not "spin on a zero timeout". */
static inline long cinder_clamp_budget(long budget_ms, long deadline, long now_ms) {
    if (deadline == CINDER_NO_DEADLINE) return budget_ms;
    long left = deadline - now_ms;
    if (left < 1) left = 1;
    return left < budget_ms ? left : budget_ms;
}

#endif /* CINDER_FRAME_BUDGET_H */
