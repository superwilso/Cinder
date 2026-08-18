/* vol_ramp.h — how fast the volume rocker travels while it is held.
 *
 * Shared between main.cpp (which uses it) and tools/volramp_selftest.cpp (which checks it), so the
 * curve exists exactly once. A duplicated copy in a test proves only that the copy is self-
 * consistent.
 *
 * WHY A RAMP AT ALL. Both volume scales are deliberately fine: the 3.5 mm route is 0..120, which is
 * 1:1 with the hardware `master volume` control, and Bluetooth went to 0..127 on 2026-08-18 to sit
 * 1:1 on AVRCP's own 7-bit field. Neither can be made finer — those ARE the hardware limits — but at
 * a flat one step per 120 ms tick, crossing either takes about fifteen seconds.
 *
 * So the tick rate stays put and the STEP SIZE grows with how long the button has been down. A tap
 * is always exactly one step, which is the entire point of having a fine scale; only a sustained
 * hold accelerates.
 *
 * GEOMETRIC, not linear. Loudness is perceived roughly logarithmically, so doubling the step each
 * bucket feels like a steady rate of change rather than a lurch that runs away at the top.
 */
#ifndef CINDER_VOL_RAMP_H
#define CINDER_VOL_RAMP_H

/* One tick of the auto-repeat, in ms. */
#define CINDER_VOL_REPEAT_EVERY_MS 120
/* Hold this long before the ramp starts at all. */
#define CINDER_VOL_REPEAT_DELAY_MS 350

/* Steps to advance on a tick, given how long the rocker has been held.
 *
 *   held < 1.5 s -> 1     1.5-3 s -> 2     3-5 s -> 4     5 s+ -> 8
 *
 * The first ~1.2 s of ramping is single-step on purpose: that is the range you are inside when you
 * are aiming, and it must stay as precise as a tap. */
static inline int cinder_vol_repeat_steps(long held_ms)
{
    if (held_ms < 1500) return 1;
    if (held_ms < 3000) return 2;
    if (held_ms < 5000) return 4;
    return 8;
}

#endif /* CINDER_VOL_RAMP_H */
