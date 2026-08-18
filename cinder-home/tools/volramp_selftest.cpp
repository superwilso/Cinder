// volramp_selftest — checks the volume rocker's hold-to-accelerate curve on the host.
//
// It includes src/vol_ramp.h, the same header main.cpp uses, so this tests the shipping curve
// rather than a copy of it. Run from build.sh; a failure refuses the pack.
//
// What is actually being protected here, in order of importance:
//   1. A TAP IS ONE STEP. The fine scales (0..120 wired, 0..127 Bluetooth) exist so small
//      adjustments are possible; an acceleration that starts immediately would throw that away.
//   2. The ramp never goes backwards, and never overshoots so hard you cannot stop on a value.
//   3. A full sweep of either scale is a handful of seconds, not fifteen — the reason this exists.
#include "../src/vol_ramp.h"
#include <cstdio>

static int fails = 0;
static void check(bool ok, const char* what)
{
    std::printf("  %s %s\n", ok ? "PASS" : "FAIL", what);
    if (!ok) fails++;
}

// How long to cross `range` steps while holding, in ms — the ramp's whole point.
static long sweep_ms(int range)
{
    long held = CINDER_VOL_REPEAT_DELAY_MS;
    int done = 0;
    while (done < range && held < 60000) {
        done += cinder_vol_repeat_steps(held - CINDER_VOL_REPEAT_DELAY_MS);
        held += CINDER_VOL_REPEAT_EVERY_MS;
    }
    return held;
}

int main()
{
    std::printf("volume ramp self-test\n");

    // 1. Precision window.
    check(cinder_vol_repeat_steps(0) == 1, "the first tick of a hold is a single step");
    check(cinder_vol_repeat_steps(1499) == 1, "still single-stepping at 1.5 s (the aiming window)");

    // 2. Monotonic, and geometric rather than runaway.
    int prev = 0;
    bool monotonic = true, doubling = true;
    for (long t = 0; t <= 8000; t += 50) {
        int s = cinder_vol_repeat_steps(t);
        if (s < prev) monotonic = false;
        if (prev && s != prev && s != prev * 2) doubling = false;
        prev = s;
    }
    check(monotonic, "step size never decreases while held");
    check(doubling, "each bucket exactly doubles the last (geometric, no lurch)");
    check(cinder_vol_repeat_steps(60000) == 8, "the ramp CAPS — it does not run away on a long hold");

    // 3. The sweep is quick but still stoppable. Both real scales.
    long wired = sweep_ms(120);   // 3.5 mm: 0..120, 1:1 with the hardware master volume
    long bt    = sweep_ms(127);   // Bluetooth: 0..127, 1:1 with AVRCP absolute volume
    std::printf("  full sweep: wired(120) %ld ms, bluetooth(127) %ld ms\n", wired, bt);
    check(wired < 7000 && bt < 7000, "a full sweep of either scale is under 7 s");
    check(wired > 2000 && bt > 2000, "and over 2 s, so a hold can still be stopped on a value");

    // 4. A flat ramp would be the regression this exists to prevent.
    check(sweep_ms(127) < 127L * CINDER_VOL_REPEAT_EVERY_MS,
          "accelerating beats one-step-per-tick (the ~15 s this replaced)");

    std::printf(fails ? "FAILED\n" : "ALL PASS\n");
    return fails ? 1 : 0;
}
