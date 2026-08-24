/* bt_poll.h — how often to ask the radio anything, given what is already pushing at us.
 *
 * THE PROBLEM. A connected Bluetooth session with the panel dark is the LONGEST-LIVED state on a
 * music player — hours, in a pocket — and Cinder spent all of it making ~2.2 synchronous binder
 * round trips a second on the render thread:
 *
 *     GetCurrentStatus      1.00/s   detect a track boundary (~1 per 3-4 minutes)
 *     GetSoundStatus        0.50/s   the negotiated codec, which its own comment says
 *                                    "changes on the order of once a session"
 *     GetBtStatus           0.33/s   link up/down
 *     GetConnectInformation 0.33/s   which peer
 *
 * Every one of those has an event-driven or free replacement already in place.
 *
 * THE ENABLING FACT, and why this is safe NOW and was not before. `OnNotifyBtStatus`,
 * `OnNotifyAclStateChanged` and `OnNotifyDisconnectEnd` set `g_bt_state_dirty` the moment the link
 * moves — but until 2026-08-23 that listener was registered ONLY by `apply_bt_scan`, i.e. only if
 * the user had opened Devices and pressed Scan. On an ordinary boot it was never registered at all,
 * so the timer genuinely WAS the mechanism and 3 s was the right number. With the listener now
 * registered at boot, the timer is the safety net its own comment always claimed it was.
 *
 * ── THE ONE THING THIS MUST NOT BREAK ──────────────────────────────────────────────────────────
 * Pause-on-disconnect. When headphones drop, the music must stop promptly — `cinder_bt_should_pause`
 * rides `refresh_bt_connected`, which the route poll calls. That stays instant while the listener is
 * up, because a listener event forces the poll on the very next frame regardless of the timer.
 *
 * So the relaxed interval is gated on `listener_on`. If `AddListener` ever fails, the timer is once
 * again the only thing that can notice a dropped link, and this returns the fast interval. A backoff
 * that assumed an event source it did not verify would trade a few milliamps for music playing into
 * headphones that are not there.
 *
 * Header-only and dependency-free so tools/btpoll_selftest.cpp can exercise it on the host — the
 * same treatment bt_edge.h, jack_edge.h, db_sig.h and bt_switch.h get.
 */
#ifndef CINDER_BT_POLL_H
#define CINDER_BT_POLL_H

/* Radio down: nothing can connect, and every path that powers it up refreshes the route itself. */
#define CINDER_BT_POLL_RADIO_DOWN_MS 15000
/* Radio up but no peer named — connecting, searching, or idle. Latency matters here; unchanged. */
#define CINDER_BT_POLL_ACTIVE_MS      3000
/* Connected, named, and the listener is pushing changes. The timer is a backstop, not the source. */
#define CINDER_BT_POLL_STEADY_MS     30000
/* The codec is negotiated once per link and has its own event bypass, so its backstop is slower. */
#define CINDER_BT_POLL_CODEC_STEADY_MS 60000
/* …and its rate while anything is still moving. */
#define CINDER_BT_POLL_CODEC_MS       2000

/* A link that nothing needs to be asked about: a peer is named AND something else is watching it.
 * `listener_on` is `g_bt_listener_on` — the real registration result, never an assumption. */
static inline int cinder_bt_link_steady(int listener_on, int radio_up, int have_name) {
    return listener_on && radio_up && have_name;
}

/* Interval for the route poll (GetBtStatus, and the peer read inside it). */
static inline int cinder_bt_route_poll_ms(int listener_on, int radio_up, int have_name) {
    if (!radio_up) return CINDER_BT_POLL_RADIO_DOWN_MS;
    if (cinder_bt_link_steady(listener_on, radio_up, have_name)) return CINDER_BT_POLL_STEADY_MS;
    return CINDER_BT_POLL_ACTIVE_MS;
}

/* Interval for the negotiated-codec poll. Radio down is not special-cased: the call site only
 * reaches this while the route IS Bluetooth. */
static inline int cinder_bt_codec_poll_ms(int listener_on, int radio_up, int have_name) {
    return cinder_bt_link_steady(listener_on, radio_up, have_name)
         ? CINDER_BT_POLL_CODEC_STEADY_MS : CINDER_BT_POLL_CODEC_MS;
}

#endif /* CINDER_BT_POLL_H */
