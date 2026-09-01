// framebudget_selftest — host test for the dark-screen sleep rule (src/frame_budget.h).
//
// Includes the SAME header main.cpp uses, so this tests the shipping rule rather than a copy.
// Built and run by build.sh and by CI's "C++ self-tests" step.
//
// The defect it pins: "3.5 mm volume is not responsive when the screen is off". With the panel
// dark the loop sleeps to the next 1 Hz housekeeping deadline, and the volume ramp's next step and
// the coalesced write's trailing flush are DEADLINES rather than events — so nothing woke the loop
// for either, and the level the user stopped on could land up to a second late.
//
// The harness cannot reach this: its clock is virtual, and darkening the panel needs a Power press
// to reach carry_out, which the harness stubs. Pure arithmetic, so it is tested here instead.
#include <cstdio>
#include "../src/frame_budget.h"

// The shipping constants, from main.cpp.
static const long DELAY = 400;   // VOL_REPEAT_DELAY_MS
static const long EVERY = 120;   // VOL_REPEAT_EVERY_MS
static const long WRITE = 150;   // VOL_WRITE_EVERY_MS
static const long HOUSE = 1000;  // the dark housekeeping budget

static int fails = 0;
static void check(int cond, const char* what) {
    std::printf("  %-4s %s\n", cond ? "ok" : "FAIL", what);
    if (!cond) fails = 1;
}
static void check_eq(long got, long want, const char* what) {
    std::printf("  %-4s %s (got %ld, want %ld)\n", got == want ? "ok" : "FAIL", what, got, want);
    if (got != want) fails = 1;
}

// Convenience: deadline with the shipping intervals.
static long dl(int btn, long down, long last, int pending, long wrote) {
    return cinder_vol_deadline(btn, down, last, pending, wrote, DELAY, EVERY, WRITE);
}

int main() {
    std::printf("test 1: nothing owed -> the dark budget is untouched\n");
    // This is the one that matters for battery. Dark and idle is the state the device spends hours
    // in, and shortening the sleep there would undo the single biggest power saving in the app.
    check_eq(dl(-1, 0, 0, -1, 0), CINDER_NO_DEADLINE, "no rocker, no pending write -> no deadline");
    check_eq(cinder_clamp_budget(HOUSE, CINDER_NO_DEADLINE, 50000), HOUSE,
             "so the loop still sleeps the full housekeeping second");

    std::printf("test 2: rocker just went down -> wake when the ramp may START\n");
    // Not every EVERY ms through the delay: a tap that never becomes a ramp must not wake the loop
    // three times on its way to doing nothing.
    check_eq(dl(9, 10000, 10000, -1, 0), 10400, "deadline is down + the pre-ramp delay");
    check_eq(cinder_clamp_budget(HOUSE, dl(9, 10000, 10000, -1, 0), 10000), 400,
             "…so it sleeps 400ms, not 1000");

    std::printf("test 3: ramp running -> wake for the next step\n");
    check_eq(dl(9, 10000, 10600, -1, 0), 10720, "deadline is the last step + the step interval");
    check_eq(cinder_clamp_budget(HOUSE, dl(9, 10000, 10600, -1, 0), 10600), 120,
             "…so it sleeps one step, not a whole second");

    std::printf("test 4: THE REPORTED BUG — released, level pending, no further input\n");
    // The rocker is up (btn < 0) so nothing will generate another event, and the last write was
    // 40 ms ago so the flush is rate-limited. Before this rule the loop slept to housekeeping and
    // the level the user stopped on landed up to a second later.
    long d = dl(-1, 0, 0, 77, 12000);
    check_eq(d, 12150, "deadline is the last write + the coalescing interval");
    check_eq(cinder_clamp_budget(HOUSE, d, 12040), 110, "sleeps 110ms to the flush, not 1000ms");
    check(cinder_clamp_budget(HOUSE, d, 12040) < 200,
          "the level the user stopped on lands within a fifth of a second");

    std::printf("test 5: both owed -> the EARLIER one wins\n");
    // Held rocker with a write also pending. Waking for the later of the two would miss the earlier.
    check_eq(dl(9, 10000, 10600, 77, 10500), 10650, "flush at 10650 beats the next step at 10720");
    check_eq(dl(9, 10000, 10600, 77, 10700), 10720, "…and the step wins when it is the earlier");

    std::printf("test 6: a deadline already past means go round NOW, not spin\n");
    // poll() with a 0 timeout returns instantly and the loop becomes a busy-wait on a single core.
    check_eq(cinder_clamp_budget(HOUSE, 9000, 12000), 1, "an overdue deadline clamps to 1ms");
    check_eq(cinder_clamp_budget(HOUSE, 12000, 12000), 1, "…and so does one due exactly now");

    std::printf("test 7: the deadline never LENGTHENS a sleep\n");
    // Awake the budget is 16ms; a 150ms flush deadline must not stretch it and cost touch latency.
    check_eq(cinder_clamp_budget(16, dl(-1, 0, 0, 77, 12000), 12000), 16,
             "a 16ms awake budget is unaffected by a 150ms deadline");

    std::printf(fails ? "FRAMEBUDGET SELFTEST FAILED\n"
                      : "PASS — the loop wakes for owed volume work and sleeps through nothing else\n");
    return fails;
}
