// battslew_selftest — host test for the battery gauge's slew rule (src/batt_slew.h).
//
// Includes the SAME header main.cpp uses, so this tests the shipping rule rather than a copy.
// Built and run by build.sh and by CI's "C++ self-tests" step.
//
// The two defects it pins, one in each direction:
//   * UP: the level is slew-limited to keep a charger's own voltage from arriving as 36 points of
//     "charge" the instant the cable lands (reported 2026-09-18). Limit that per CALL rather than
//     per unit of TIME and a screen-off charge comes back at 6 points a minute — thirteen minutes
//     of the device telling its owner the charger is not working.
//   * DOWN: catching up downward would let the first, hardest-sagging reading after a resume hand
//     battery_guard a level low enough to switch the player off in the middle of a track.
//
// The off-device harness cannot reach either: it stubs sysfs and its clock is virtual, so an hour
// of screen-off never happens. Pure arithmetic, so it is tested here instead.
#include <cstdio>
#include "../src/batt_slew.h"

// The shipping constants, from main.cpp.
static const long POLL = 10000;   // BATT_POLL_MS
static const int  STEP = 1;       // BATT_MAX_STEP

static int fails = 0;
static void check_eq(int got, int want, const char* what) {
    std::printf("  %-4s %s (got %d, want %d)\n", got == want ? "ok" : "FAIL", what, got, want);
    if (got != want) fails = 1;
}

static int step(int level, int raw, long elapsed_ms) {
    return cinder_batt_step(level, raw, elapsed_ms, POLL, STEP);
}

int main() {
    std::printf("test 1: the first reading of a session seeds, it does not slew\n");
    // A player switched on at 4% must say 4%, not walk up from nothing.
    check_eq(step(-1, 4,  POLL), 4,  "no previous level -> report the raw reading");
    check_eq(step(-1, 97, POLL), 97, "…at the top of the scale too");

    std::printf("test 2: THE REPORTED JUMP — a charger lands between two polls\n");
    // 22%, then sysfs says 58% ten seconds later. That is the charger's regulation voltage, not
    // 36 points of charge.
    check_eq(step(22, 58, POLL), 23, "one poll of real time -> one point");
    check_eq(step(23, 61, POLL), 24, "…and the next poll is one more");

    std::printf("test 3: load sag pulls the raw reading down -> still one point\n");
    // The amp, the screen and the radio arriving at once. battery_guard switches off at 3%.
    check_eq(step(40, 4, POLL), 39, "a 36-point sag moves the report by one");
    check_eq(step(4,  1, POLL), 3,  "…and near the floor it is still one, not a shutdown");

    std::printf("test 4: THE FIX — a screen-off hour on the charger catches up\n");
    // An hour is 360 poll intervals, so an 80-point gap is covered in the first reading after the
    // resume rather than in the next thirteen minutes.
    check_eq(step(20, 100, 3600000), 100, "an hour's gap covers a full charge");
    check_eq(step(20, 55,  600000),  55,  "ten minutes covers 35 points (60 allowed)");
    check_eq(step(20, 95,  600000),  80,  "…but ten minutes is 60 points and no more");

    std::printf("test 5: the catch-up is UPWARD ONLY\n");
    // The same hour-long gap, with the wake-up reading sagged. This is the case that must NOT be
    // scaled: one sample at the moment of highest load cannot be allowed to take the device down.
    check_eq(step(80, 20, 3600000), 79, "an hour's gap still walks DOWN one point");
    check_eq(step(5,  1,  86400000), 4, "…even a day of it, right above the 3% cutoff");

    std::printf("test 6: it never overshoots the reading in either direction\n");
    check_eq(step(50, 51, 3600000), 51, "a 1-point rise with a huge allowance lands on 51");
    check_eq(step(50, 49, POLL),    49, "a 1-point fall lands on 49");
    check_eq(step(50, 50, 3600000), 50, "equal readings do not move");

    std::printf("test 7: a clock that goes backwards, or stands still, still moves one point\n");
    // The house clock is monotonic, but nothing here depends on that being true.
    check_eq(step(50, 90, 0),     51, "zero elapsed -> the plain step");
    check_eq(step(50, 90, -5000), 51, "negative elapsed -> the plain step, never a jump");

    std::printf("test 8: the allowance is capped at a full sweep of the scale\n");
    // 100 points is the whole gauge; a week of elapsed time cannot mean more than that.
    check_eq(cinder_batt_step(0, 100, 604800000L, POLL, STEP), 100, "a week covers the scale, once");

    std::printf(fails ? "BATTSLEW SELFTEST FAILED\n"
                      : "PASS — the gauge catches up with a charge and never runs down to meet a sag\n");
    return fails;
}
