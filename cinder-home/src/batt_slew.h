/* batt_slew.h — how far the reported battery level may move, given one sysfs reading and the time
 * since the last one.
 *
 * WHY THERE IS A RULE AT ALL. This platform has no fuel gauge: every MediaTek state-of-charge
 * source is disabled in the kernel, and `capacity` comes from Sony's bq24262_wmport driver, which
 * has only terminal voltage to work from (this repo's own 123-sample log has it tracking
 * voltage_now at r = 0.96, about 4.65 mV per point). Two things move terminal voltage without
 * moving the charge: LOAD, which sags the cell by tens of millivolts the moment the amp, the screen
 * and the radio arrive, and A CHARGER, which lifts it to the charger's regulation voltage the
 * instant the cable lands. Reported 2026-09-18: 22%, then 58% seconds after plugging in, then 61%.
 * 36 points in the time it takes to push a plug in, which no battery does.
 *
 * THE RULE, AND WHY IT IS ASYMMETRIC.
 *
 *   UP is limited to one point per POLL INTERVAL OF REAL TIME. One point per CALL was the first
 *   version, and it is the same thing only while the gauge is actually polling. The screen goes
 *   off, the SoC suspends, and the next call arrives an hour later — an hour in which a charger may
 *   have done real work. A per-call limit then walks up at 6 points a minute, so an 80-point gap
 *   takes thirteen minutes, during which the device tells its owner the charger is not working.
 *
 *   DOWN is not scaled, and that is the point of the whole mechanism. The first reading after a
 *   resume is taken with the screen on and the amp running — the hardest sag there is — and
 *   battery_guard switches the player off at 3%. A scaled step would let one sagged wake-up sample
 *   take the device down in the middle of a track. Nothing true is hidden: a discharge across a
 *   suspend is small (deep idle, amp off), and a genuinely flat battery comes back as a fresh boot,
 *   where the level seeds from the raw reading with no smoothing at all.
 *
 * Pure arithmetic and no I/O, so it is testable on the host: tools/battslew_selftest.cpp.
 */
#ifndef CINDER_BATT_SLEW_H
#define CINDER_BATT_SLEW_H

/* One step. `level` is the level last reported, or < 0 for "nothing reported yet" (the first
 * reading of a session seeds it). `raw` is what sysfs just said. `elapsed_ms` is the real time
 * since the previous reading. `poll_ms` is the gauge's own interval — one upward step per one of
 * those. `max_step` is the floor on the upward step and the whole of the downward one.
 *
 * Returns the level to report. Never moves past `raw` in either direction. */
static inline int cinder_batt_step(int level, int raw, long elapsed_ms, long poll_ms, int max_step)
{
    if (level < 0) return raw;              /* nothing to smooth against */
    if (poll_ms <= 0) poll_ms = 1;
    if (max_step < 1) max_step = 1;

    /* The upward allowance: one point per poll interval of elapsed time, never below the plain
     * step, never above a full sweep of the scale (more than 100 cannot mean more than 100), and
     * never negative if the clock is ever read backwards. */
    long steps = (elapsed_ms > 0 ? elapsed_ms : 0) / poll_ms;
    if (steps < max_step) steps = max_step;
    if (steps > 100)      steps = 100;

    if (raw > level) {
        const int room = raw - level;
        return level + (room > (int)steps ? (int)steps : room);
    }
    if (raw < level) {
        const int room = level - raw;
        return level - (room > max_step ? max_step : room);
    }
    return level;
}

#endif /* CINDER_BATT_SLEW_H */
