// jackedge_selftest — the headphone-unplug rule, checked on the host.
//
// Includes src/jack_edge.h, the same header main.cpp uses. The rule is small enough to look
// obviously right and was still written wrong first time (it paused on plug-IN), which is the
// entire argument for this file.
#include "../src/jack_edge.h"
#include <cstdio>

static int fails = 0;
static void check(bool ok, const char* what)
{
    std::printf("  %s %s\n", ok ? "PASS" : "FAIL", what);
    if (!ok) fails++;
}

int main()
{
    std::printf("jack unplug edge self-test\n");

    // The one case that must act.
    check(cinder_jack_should_pause(2, 0) == 1, "headphone (2) -> empty (0) pauses");
    check(cinder_jack_should_pause(1, 0) == 1, "headset (1) -> empty (0) pauses");

    // Everything else must not.
    check(cinder_jack_should_pause(0, 2) == 0, "plugging IN does not pause");
    check(cinder_jack_should_pause(0, 1) == 0, "plugging in a headset does not pause");
    check(cinder_jack_should_pause(2, 2) == 0, "no change does not pause");
    check(cinder_jack_should_pause(0, 0) == 0, "still empty does not pause (not level-triggered)");
    check(cinder_jack_should_pause(1, 2) == 0, "headset -> headphone does not pause");

    // Boot and failure states.
    check(cinder_jack_should_pause(-1, 0) == 0, "first observation of the boot only seeds");
    check(cinder_jack_should_pause(-1, 2) == 0, "…whatever it sees");
    check(cinder_jack_should_pause(2, -1) == 0, "an unreadable node never acts on a guess");

    std::printf(fails ? "FAILED\n" : "ALL PASS\n");
    return fails ? 1 : 0;
}
