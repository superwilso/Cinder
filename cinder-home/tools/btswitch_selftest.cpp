// btswitch_selftest — host test for the Bluetooth switch/radio reconcile (src/bt_switch.h).
//
// Same shape as btedge_selftest / jackedge_selftest / dbsig_selftest, and by far the most
// load-bearing of the four: the flag this rule maintains gates the auto-reconnect, the NFC reader,
// and whether the radio is told to keep retrying at all. A single un-retried boot read used to
// decide it for the whole session.
#include "../src/bt_switch.h"
#include <cstdio>

static int fails = 0;
static void check(bool ok, const char* what) {
    std::printf("%s  %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok) fails++;
}

// One-shot helper for cases that do not care about the tally.
static int once(int status, int believed, int settling) {
    int down = 0;
    return cinder_bt_switch_reconcile(status, believed, settling, &down);
}

int main() {
    // ── THE BUG THIS EXISTS FOR ──────────────────────────────────────────────────────────────
    // deferred_up's read failed (-1) or came back unknown (0), the switch latched OFF, and the
    // radio was up all along. The very next poll that sees 2 or 3 must correct it.
    check(once(2, 0, 0) == CINDER_BT_SWITCH_ON, "radio idle (2) with the switch OFF turns it ON");
    check(once(3, 0, 0) == CINDER_BT_SWITCH_ON, "radio connected (3) with the switch OFF turns it ON");

    // ── WHAT MUST NOT MOVE ANYTHING ──────────────────────────────────────────────────────────
    // The two values the failing boot read produced are not evidence of anything.
    check(once(0, 1, 0) == CINDER_BT_SWITCH_LEAVE, "unknown (0) never turns a live switch off");
    check(once(-1, 1, 0) == CINDER_BT_SWITCH_LEAVE, "no client (-1) never turns a live switch off");
    check(once(0, 0, 0) == CINDER_BT_SWITCH_LEAVE, "unknown (0) never turns the switch on either");
    check(once(-1, 0, 0) == CINDER_BT_SWITCH_LEAVE, "no client (-1) never turns the switch on either");

    // Agreement is silence — the caller logs every non-LEAVE as a correction, so a rule that
    // returned the current value would log on every single 3 s poll forever.
    check(once(2, 1, 0) == CINDER_BT_SWITCH_LEAVE, "radio up and switch on: nothing to say");
    check(once(3, 1, 0) == CINDER_BT_SWITCH_LEAVE, "radio connected and switch on: nothing to say");
    check(once(7, 0, 0) == CINDER_BT_SWITCH_LEAVE, "radio off and switch off: nothing to say");

    // ── OFF IS ASYMMETRIC: it takes agreement over time ──────────────────────────────────────
    {
        int down = 0;
        check(cinder_bt_switch_reconcile(7, 1, 0, &down) == CINDER_BT_SWITCH_LEAVE,
              "one off-read does not drop the switch");
        check(cinder_bt_switch_reconcile(7, 1, 0, &down) == CINDER_BT_SWITCH_LEAVE,
              "two off-reads do not drop the switch");
        check(cinder_bt_switch_reconcile(7, 1, 0, &down) == CINDER_BT_SWITCH_OFF,
              "the third consecutive off-read drops it");
    }
    // …and the run has to be CONSECUTIVE.
    {
        int down = 0;
        cinder_bt_switch_reconcile(7, 1, 0, &down);
        cinder_bt_switch_reconcile(7, 1, 0, &down);
        check(cinder_bt_switch_reconcile(2, 1, 0, &down) == CINDER_BT_SWITCH_LEAVE,
              "a radio-up read in the middle is not a correction (switch already on)");
        check(cinder_bt_switch_reconcile(7, 1, 0, &down) == CINDER_BT_SWITCH_LEAVE,
              "…and it reset the tally, so the next off-read starts over");
        check(cinder_bt_switch_reconcile(7, 1, 0, &down) == CINDER_BT_SWITCH_LEAVE, "still counting");
        check(cinder_bt_switch_reconcile(7, 1, 0, &down) == CINDER_BT_SWITCH_OFF, "now it drops");
    }
    // An UNKNOWN read in the middle must not count as a vote for off — that is the whole point.
    {
        int down = 0;
        cinder_bt_switch_reconcile(7, 1, 0, &down);
        cinder_bt_switch_reconcile(0, 1, 0, &down);   // service hiccup
        cinder_bt_switch_reconcile(-1, 1, 0, &down);  // client gone for a moment
        check(down == 1, "unknown reads do not advance the off tally");
        check(cinder_bt_switch_reconcile(7, 1, 0, &down) == CINDER_BT_SWITCH_LEAVE,
              "so two real off-reads still are not enough");
        check(cinder_bt_switch_reconcile(7, 1, 0, &down) == CINDER_BT_SWITCH_OFF,
              "and the third real one is");
    }

    // ── THE SETTLE WINDOW ────────────────────────────────────────────────────────────────────
    // apply_bt_toggle stops waiting for the radio after ~0.9 s, so a switch just turned ON can
    // honestly still read "not up". Reconciling then would slam it back under the user's finger.
    check(once(7, 1, 1) == CINDER_BT_SWITCH_LEAVE, "settling: a not-up read cannot undo a toggle ON");
    check(once(2, 0, 1) == CINDER_BT_SWITCH_LEAVE, "settling: an up read cannot undo a toggle OFF");
    // But the tally keeps running underneath, so the truth lands the moment the window closes
    // rather than restarting the count from zero.
    {
        int down = 0;
        cinder_bt_switch_reconcile(7, 1, 1, &down);
        cinder_bt_switch_reconcile(7, 1, 1, &down);
        cinder_bt_switch_reconcile(7, 1, 1, &down);
        check(cinder_bt_switch_reconcile(7, 1, 0, &down) == CINDER_BT_SWITCH_OFF,
              "the tally accrues while settling, so the first read after it decides");
    }

    // ── THE RADIO IS THE TRUTH ───────────────────────────────────────────────────────────────
    // A switch the user turned OFF against a radio that stayed up is corrected too: bt_set_rf
    // returning is not evidence the radio went down.
    check(once(2, 1, 0) == CINDER_BT_SWITCH_LEAVE, "(control) up + on agrees");
    check(once(3, 0, 0) == CINDER_BT_SWITCH_ON, "a radio that refused to switch off shows as ON");

    std::printf(fails ? "\nbtswitch_selftest: %d FAILURE(S)\n" : "\nbtswitch_selftest: all good\n", fails);
    return fails ? 1 : 0;
}
