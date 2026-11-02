/* usb_resume_gate.h — when to run `cinder-msc usb-resume` after a resume from RAM.
 *
 * Shared between main.cpp and tools/usbresume_selftest.cpp so the rule is tested where it lives.
 *
 * #52: a WALL CHARGER reads as a cable (usb/online = 1) and never has a host behind it, so the
 * helper walked all five steps every time: 4 s waits for steps 0-3, init's adb block (up to 6 s of
 * getprop polling), and 4 s more — over 25 s, against a 20 s watchdog that is fatal by design. The
 * player was left on its charger overnight, the screen-off suspend resumed, the watchdog fired after
 * 12.5 healthy hours, and the crash latched every later boot to stock.
 *
 * So: the budget covers the helper's worst case with room, and a run that found no host is not
 * repeated until the cable comes out — a charger costs one attempt per plug, not one per resume. */
#ifndef CINDER_USB_RESUME_GATE_H
#define CINDER_USB_RESUME_GATE_H

/* Seconds to let the port come up by itself first; seconds the helper may take. */
enum { USB_RESUME_SETTLE_S = 5, USB_RESUME_BUDGET_S = 60 };

/* Called once a second. `pending`: a resume from RAM has not been answered yet. `cable`: usb/online.
 * `gave_up`: an earlier run on this same plug found no host. Returns 1 when the helper should run. */
static inline int cinder_usb_resume_due(int* plugged_s, int pending, int cable, int gave_up)
{
    if (!pending || !cable || gave_up) { *plugged_s = 0; return 0; }
    if (++*plugged_s < USB_RESUME_SETTLE_S) return 0;
    *plugged_s = 0;
    return 1;
}

#endif /* CINDER_USB_RESUME_GATE_H */
