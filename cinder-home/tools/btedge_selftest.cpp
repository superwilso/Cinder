// btedge_selftest — host test for the Bluetooth-disconnect pause rule (src/bt_edge.h).
//
// Same shape as jackedge_selftest: the rule is four lines, it decides whether the music stops, and
// the jack version of it was wrong the first time it was written. Built and run by build.sh.
#include "../src/bt_edge.h"
#include <cstdio>

static int fails = 0;
static void check(bool ok, const char* what) {
    std::printf("%s  %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok) fails++;
}

int main() {
    // The edge the feature exists for.
    check(cinder_bt_should_pause(1, 0, 1) == 1, "linked -> unlinked while playing pauses");

    // Everything that must NOT pause.
    check(cinder_bt_should_pause(1, 0, 0) == 0, "already paused: nothing to do");
    check(cinder_bt_should_pause(0, 1, 1) == 0, "connecting does not pause");
    check(cinder_bt_should_pause(1, 1, 1) == 0, "still linked does not pause");
    check(cinder_bt_should_pause(0, 0, 1) == 0, "still unlinked does not pause (not level-triggered)");

    // First observation of a boot only seeds the state.
    check(cinder_bt_should_pause(-1, 0, 1) == 0, "first observation of the boot only seeds");
    check(cinder_bt_should_pause(-1, 1, 1) == 0, "…whatever it sees");

    // A second drop after a reconnect pauses again — the state is per-transition, not once-ever.
    check(cinder_bt_should_pause(1, 0, 1) == 1 && cinder_bt_should_pause(1, 0, 1) == 1,
          "a later drop pauses again");

    // "Turn off when idle": is the radio carrying nothing?
    check(cinder_bt_radio_unused(0, 1, 0) == 1, "silence: unused, linked or not");
    check(cinder_bt_radio_unused(1, 0, 0) == 1, "music on the jack with no link: unused");
    check(cinder_bt_radio_unused(1, 1, 0) == 0, "music with a link up: in use");
    check(cinder_bt_radio_unused(1, -1, 0) == 0, "link not read yet this boot: not unused");
    check(cinder_bt_radio_unused(1, 0, 1) == 0, "receiver or a Bluetooth soundscape: in use");

    std::printf(fails ? "\n%d FAILED\n" : "\nall passed\n", fails);
    return fails ? 1 : 0;
}
