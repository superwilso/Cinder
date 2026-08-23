/* bt_switch.h — reconciling the Settings Bluetooth switch with what the radio actually reports.
 *
 * WHY THIS EXISTS. `cinder_set_bt_on` had exactly ONE call site in the whole shell: a single,
 * un-retried `GetBtStatus` read in `deferred_up`, on the boot path, inside a `run_guarded` that
 * swallows failure. That read can legitimately fail — `bt_status()` returns -1 when
 * BtCommonServiceClient cannot be built and 0 for the service's own "unknown", both of which a
 * `hagodaemon` still coming up produces — and `bt_radio_up()` calls those OFF.
 *
 * Nothing ever asked again, even though `refresh_bt_route` re-read the very same GetBtStatus every
 * 3 s for the rest of the session and used the answer only for the volume route. The flag is not
 * passive: `bt_reconnect_tick`'s "radio off" branch actively calls `bt_connect_wait(false)` and
 * `bt_service_retry(false, false)`, i.e. tells the radio to stop retrying AND stop accepting
 * incoming links; the NFC reader is gated on the same flag; and the switch shows OFF while the
 * radio is up. So one unlucky boot read disabled Bluetooth and NFC for a whole session.
 *
 * THE RULE, and why it is asymmetric:
 *
 *   - Statuses are 2 (on, idle), 3 (connected), 7 (off). 0 = the service's unknown/error,
 *     -1 = no client. **Only 2/3/7 are evidence.** 0 and -1 are precisely what the failing boot
 *     read produced, so acting on them is the bug itself — they decide nothing and do not even
 *     count toward the tally.
 *   - **UP wins on one read.** A status of 2 or 3 is positive proof the radio is up; there is
 *     nothing to wait for.
 *   - **OFF has to be seen repeatedly.** "Not up yet" and "off" are indistinguishable in a single
 *     sample, and the poll runs every 3 s, so a threshold of 3 is ~9 s of agreement.
 *   - **A user toggle buys a settle window.** `apply_bt_toggle` stops waiting for the radio after
 *     ~0.9 s by design (a longer wait freezes the UI, which read as "the switch doesn't work"), so
 *     the next poll can honestly still see "not up" on a switch just turned ON. Reconciling then
 *     would slam it back under the finger that moved it.
 *
 * THE RADIO IS THE TRUTH, NOT THE SWITCH. Outside the settle window the switch is what moves, in
 * either direction — including a switch the user turned OFF against a radio that stayed up, because
 * `bt_set_rf(false)` returning is not evidence the radio went down. Same rule the project already
 * applies to the mixer: a control accepting a write is not proof the hardware did anything.
 *
 * Header-only and dependency-free so tools/btswitch_selftest.cpp can exercise it on the host — the
 * treatment bt_edge.h, jack_edge.h and db_sig.h get, for the same reason.
 */
#ifndef CINDER_BT_SWITCH_H
#define CINDER_BT_SWITCH_H

/* Consecutive KNOWN-off reads before the switch is allowed to fall. */
#define CINDER_BT_DOWN_READS 3

/* Outcome of one observation. */
enum {
    CINDER_BT_SWITCH_LEAVE = -1,  /* no decision — say nothing, change nothing */
    CINDER_BT_SWITCH_OFF   = 0,
    CINDER_BT_SWITCH_ON    = 1
};

/* `status`      — the raw GetBtStatus value (2/3 up, 7 off, 0 unknown, -1 no client).
 * `believed_on` — what the Settings switch currently says (1/0).
 * `settling`    — a user toggle happened too recently to argue with (1/0).
 * `down_reads`  — caller-owned tally, updated here. Must persist across calls.
 *
 * Returns CINDER_BT_SWITCH_ON / _OFF when the switch should be set to that, or _LEAVE. Only ever
 * returns a value that DIFFERS from `believed_on`, so the caller can log every non-LEAVE result as
 * a genuine correction. */
static inline int cinder_bt_switch_reconcile(int status, int believed_on, int settling,
                                             int* down_reads) {
    const int up = (status == 2 || status == 3);
    const int known_off = (status == 7);

    /* The tally moves on evidence only — an unknown read is not a vote for "off". */
    if (up) *down_reads = 0;
    else if (known_off && *down_reads < CINDER_BT_DOWN_READS) (*down_reads)++;

    if (settling) return CINDER_BT_SWITCH_LEAVE;
    const int decided = up || (known_off && *down_reads >= CINDER_BT_DOWN_READS);
    if (!decided) return CINDER_BT_SWITCH_LEAVE;
    if (up == (believed_on != 0)) return CINDER_BT_SWITCH_LEAVE;   /* already agrees */
    return up ? CINDER_BT_SWITCH_ON : CINDER_BT_SWITCH_OFF;
}

#endif /* CINDER_BT_SWITCH_H */
