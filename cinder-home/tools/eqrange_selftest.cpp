// eqrange_selftest — host test for the EQ range rule (cinder-audio/src/eq_range.h).
//
// THE FIRST AUTOMATED TEST OF ANY KIND OVER cinder-audio (docs/SHORTCOMINGS.md §A2: ~2,500 lines
// driving every Sony service, "no tests of any kind and not compiled by CI"). Most of that surface
// is IPC over hand-recovered vtables and genuinely cannot be tested off-device. This is the part
// that can: a range rule, which is pure arithmetic.
//
// WHY IT IS WORTH A TEST. SetEq10BandValue takes half-dB units — the EQ screen's ±20 is ±10 dB —
// and a value outside that range does NOT clamp inside the service, it ZEROES the band. So the
// failure is silent: the EQ appears to work with one band mysteriously flat. The out-of-range value
// does not come from the UI (every site there clamps); it comes from the settings file on
// /contents, which is vfat and writable by any PC, parsed as an i8 that accepts -128..127.
//
// Includes the SAME header effect_shim.cpp uses, so this tests the shipping rule.
#include <cstdio>
#include "../../cinder-audio/src/eq_range.h"

static int fails = 0;
static void eq(int got, int want, const char* what) {
    std::printf("  %-4s %s (got %d, want %d)\n", got == want ? "ok" : "FAIL", what, got, want);
    if (got != want) fails = 1;
}

int main() {
    std::printf("test 1: in-range gains pass through untouched\n");
    // The whole usable range must survive exactly — a clamp that quietly rounded would be worse
    // than no clamp, because the EQ screen would stop agreeing with the DSP.
    for (int g = -CINDER_EQ_BAND_MAX; g <= CINDER_EQ_BAND_MAX; ++g) {
        if (cinder_eq_clamp_gain(g) != g) {
            std::printf("  FAIL gain %d was altered to %d\n", g, cinder_eq_clamp_gain(g));
            fails = 1;
        }
    }
    std::printf("  %-4s every gain in [-20, +20] is passed through unchanged\n", fails ? "FAIL" : "ok");
    eq(cinder_eq_clamp_gain(0), 0, "flat stays flat");

    std::printf("test 2: out of range CLAMPS rather than being forwarded\n");
    // The bug: forwarded, the service zeroes the band. Clamped, the user gets maximum boost/cut,
    // which is what a corrupted-but-plausible value should mean.
    eq(cinder_eq_clamp_gain(21), CINDER_EQ_BAND_MAX, "one over the top pins to +20");
    eq(cinder_eq_clamp_gain(-21), -CINDER_EQ_BAND_MAX, "one under the bottom pins to -20");
    eq(cinder_eq_clamp_gain(100), CINDER_EQ_BAND_MAX, "a hand-edited 100 pins to +20, not 0");
    eq(cinder_eq_clamp_gain(127), CINDER_EQ_BAND_MAX, "i8 max pins to +20");
    eq(cinder_eq_clamp_gain(-128), -CINDER_EQ_BAND_MAX, "i8 min pins to -20");

    std::printf("test 3: the band COUNT is bounded at both ends\n");
    // There are exactly ten bands and Eq10Band is an enum; a count past ten would cast past the
    // end of it, and a negative one is a caller bug worth refusing rather than relying on the
    // loop condition happening to be false.
    eq(cinder_eq_clamp_count(10), 10, "ten bands is the whole curve");
    eq(cinder_eq_clamp_count(11), 10, "eleven is trimmed to ten");
    eq(cinder_eq_clamp_count(1000), 10, "and so is anything larger");
    eq(cinder_eq_clamp_count(0), 0, "zero sends nothing");
    eq(cinder_eq_clamp_count(-1), 0, "a negative count sends nothing rather than looping");
    eq(cinder_eq_clamp_count(3), 3, "a partial curve is left alone");

    std::printf("test 4: the range agrees with the UI's own limit\n");
    // If these ever drift apart, the screen and the DSP disagree about what maximum boost means.
    // cinder_ui::eq::BAND_MAX is 20; this is the one place the two are asserted equal.
    eq(CINDER_EQ_BAND_MAX, 20, "CINDER_EQ_BAND_MAX matches cinder_ui::eq::BAND_MAX");

    std::printf(fails ? "EQRANGE SELFTEST FAILED\n"
                      : "PASS — out-of-range gains clamp instead of silently zeroing a band\n");
    return fails;
}
