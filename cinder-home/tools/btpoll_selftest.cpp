// btpoll_selftest — host test for the Bluetooth poll-interval rule (src/bt_poll.h).
//
// This one decides how long a DROPPED LINK can go unnoticed, which is how long music keeps playing
// into headphones that are not there. The backoff it grants is only safe because a listener is
// pushing link changes — so the tests that matter most are the ones proving it does NOT back off
// when that listener is absent.
#include "../src/bt_poll.h"
#include <cstdio>

static int fails = 0;
static void check(bool ok, const char* what) {
    std::printf("%s  %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok) fails++;
}

int main() {
    // listener_on, radio_up, have_name
    // ── THE SAVING ────────────────────────────────────────────────────────────────────────────
    check(cinder_bt_route_poll_ms(1, 1, 1) == CINDER_BT_POLL_STEADY_MS,
          "connected + listener up: the route poll relaxes to the backstop rate");
    check(cinder_bt_codec_poll_ms(1, 1, 1) == CINDER_BT_POLL_CODEC_STEADY_MS,
          "connected + listener up: the codec poll relaxes further still");
    check(CINDER_BT_POLL_STEADY_MS > CINDER_BT_POLL_ACTIVE_MS, "steady really is slower than active");
    check(CINDER_BT_POLL_CODEC_STEADY_MS > CINDER_BT_POLL_CODEC_MS, "…and so is the codec's");

    // ── THE SAFETY RULE: NO LISTENER, NO BACKOFF ─────────────────────────────────────────────
    // If AddListener failed, the timer is the ONLY thing that can notice a dropped link. Backing
    // off here would leave the music playing into headphones that are gone.
    check(cinder_bt_route_poll_ms(0, 1, 1) == CINDER_BT_POLL_ACTIVE_MS,
          "connected but NO listener: stays fast — the timer is the only detector");
    check(cinder_bt_codec_poll_ms(0, 1, 1) == CINDER_BT_POLL_CODEC_MS,
          "connected but NO listener: the codec poll stays fast too");
    check(!cinder_bt_link_steady(0, 1, 1), "a link with no listener watching it is never 'steady'");

    // ── TRANSIENT STATES KEEP THE FAST RATE ──────────────────────────────────────────────────
    // Connecting, searching, or idle-with-radio-on: this is where a user is waiting for something
    // to happen and latency is the whole point.
    check(cinder_bt_route_poll_ms(1, 1, 0) == CINDER_BT_POLL_ACTIVE_MS,
          "radio up, no peer named yet: fast (this is the connecting case)");
    check(!cinder_bt_link_steady(1, 1, 0), "no peer named is not steady");

    // ── RADIO DOWN ───────────────────────────────────────────────────────────────────────────
    check(cinder_bt_route_poll_ms(1, 0, 0) == CINDER_BT_POLL_RADIO_DOWN_MS,
          "radio down: the long interval, nothing can connect");
    check(cinder_bt_route_poll_ms(0, 0, 0) == CINDER_BT_POLL_RADIO_DOWN_MS,
          "radio down without a listener: still the long interval, nothing to miss");
    // A stale have_name with the radio down must not be read as steady — the radio wins.
    check(cinder_bt_route_poll_ms(1, 0, 1) == CINDER_BT_POLL_RADIO_DOWN_MS,
          "radio down beats a stale peer name");
    check(!cinder_bt_link_steady(1, 0, 1), "…and is not steady either");

    // ── THE INTERVALS THEMSELVES ─────────────────────────────────────────────────────────────
    // A dropped link with no listener is noticed within one ACTIVE interval. Keep that human.
    check(CINDER_BT_POLL_ACTIVE_MS <= 3000, "the no-listener fallback notices a drop within 3 s");
    // And the backstop must still be short enough to be a backstop.
    check(CINDER_BT_POLL_STEADY_MS <= 60000, "the steady backstop is still under a minute");

    std::printf(fails ? "\nbtpoll_selftest: %d FAILURE(S)\n" : "\nbtpoll_selftest: all good\n", fails);
    return fails ? 1 : 0;
}
