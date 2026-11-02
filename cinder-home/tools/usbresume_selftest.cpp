// usbresume_selftest — host test for src/usb_resume_gate.h: when the shell runs `cinder-msc
// usb-resume`. The case that mattered (#52): a wall charger, where every run finds no host, must cost
// one run per plug and not one per resume.
#include "../src/usb_resume_gate.h"
#include <cstdio>

static int fails = 0;
static void check(bool ok, const char* what) {
    std::printf("%s  %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok) fails++;
}

// Seconds of ticks until the gate opens, or -1 if it stays shut for a minute.
static int seconds_to_run(int pending, int cable, int gave_up) {
    int plugged = 0;
    for (int s = 1; s <= 60; s++)
        if (cinder_usb_resume_due(&plugged, pending, cable, gave_up)) return s;
    return -1;
}

int main() {
    check(seconds_to_run(1, 1, 0) == USB_RESUME_SETTLE_S, "a cable after a resume: runs once it has settled");
    check(seconds_to_run(0, 1, 0) == -1, "no resume to answer: never runs");
    check(seconds_to_run(1, 0, 0) == -1, "no cable: never runs");
    check(seconds_to_run(1, 1, 1) == -1, "#52: an earlier run on this plug found no host: not again");

    int plugged = 0;
    for (int s = 0; s < USB_RESUME_SETTLE_S - 1; s++) cinder_usb_resume_due(&plugged, 1, 1, 0);
    cinder_usb_resume_due(&plugged, 1, 0, 0);
    check(cinder_usb_resume_due(&plugged, 1, 1, 0) == 0, "a cable that blipped out starts the wait again");

    // The helper's worst case with no host: four 4 s steps, 0.5 s, up to 6 s of getprop polling
    // (each poll a popen), 4 s more, two dmesg copies. Well over the old 20.
    check(USB_RESUME_BUDGET_S >= 40, "#52: the budget covers the helper's walk with no host");
    return fails ? 1 : 0;
}
