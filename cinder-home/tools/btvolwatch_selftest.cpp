// btvolwatch_selftest — host test for the dead-volume detector (src/bt_vol_watch.h).
//
// The detector hangs up on the user's headphones when it fires, so the cases that must NOT fire
// matter as much as the one that must. Built and run by tools/selftests.sh.
#include "../src/bt_vol_watch.h"
#include <cstdio>

static int fails = 0;
static void check(bool ok, const char* what) {
    std::printf("%s  %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok) fails++;
}

int main() {
    const int A = CINDER_BT_VOLW_ANSWER_MS;
    cinder_bt_volw w;

    // The healthy boot: the first step goes unanswered, the next one is answered.
    cinder_bt_volw_link(&w);
    cinder_bt_volw_step(&w, 1000, 1);
    check(cinder_bt_volw_tick(&w, 1000 + A - 1) == CINDER_BT_VOLW_NOTHING, "still inside the answer window");
    check(cinder_bt_volw_tick(&w, 1000 + A) == CINDER_BT_VOLW_MISS, "first silent step is only a miss");
    cinder_bt_volw_step(&w, 5000, 1);
    cinder_bt_volw_report(&w);
    cinder_bt_volw_step(&w, 9000, 1);
    check(cinder_bt_volw_tick(&w, 9000 + A) == CINDER_BT_VOLW_MISS, "an answer resets the count");

    // The fault: two silent steps in a row.
    cinder_bt_volw_link(&w);
    cinder_bt_volw_step(&w, 1000, 1);
    cinder_bt_volw_tick(&w, 1000 + A);
    cinder_bt_volw_step(&w, 5000, 1);
    check(cinder_bt_volw_tick(&w, 5000 + A) == CINDER_BT_VOLW_REPAIR, "two silent steps ask for a repair");

    // Two quick presses are one question.
    cinder_bt_volw_link(&w);
    cinder_bt_volw_step(&w, 1000, 1);
    cinder_bt_volw_step(&w, 1200, 1);
    check(cinder_bt_volw_tick(&w, 1000 + A) == CINDER_BT_VOLW_MISS, "a press while waiting adds nothing");

    // Steps at the ends of the scale are not counted.
    check(!cinder_bt_volw_can_move(127, 127, 1) && !cinder_bt_volw_can_move(126, 127, 1),
          "up at the top cannot move");
    check(!cinder_bt_volw_can_move(0, 127, 0) && !cinder_bt_volw_can_move(2, 127, 0),
          "down at the bottom cannot move");
    check(cinder_bt_volw_can_move(40, 127, 1) && cinder_bt_volw_can_move(40, 127, 0), "the middle moves");
    cinder_bt_volw_link(&w);
    cinder_bt_volw_step(&w, 1000, 0);
    check(cinder_bt_volw_tick(&w, 1000 + A) == CINDER_BT_VOLW_NOTHING, "an end-of-scale step arms nothing");

    // A new link forgets the old one's misses.
    cinder_bt_volw_link(&w);
    cinder_bt_volw_step(&w, 1000, 1);
    cinder_bt_volw_tick(&w, 1000 + A);
    cinder_bt_volw_link(&w);
    cinder_bt_volw_step(&w, 5000, 1);
    check(cinder_bt_volw_tick(&w, 5000 + A) == CINDER_BT_VOLW_MISS, "a reconnect starts the count over");

    return fails ? 1 : 0;
}
