/* bt_edge.h — when a Bluetooth link observation should pause playback.
 *
 * The headphone-jack half of this rule lives in jack_edge.h and shipped first. Bluetooth is the
 * same feature through a different sink: the music should stop when the thing playing it goes
 * away, whether that is a cable being pulled or a link dropping.
 *
 * WHAT COUNTS AS "LINKED" IS THE PEER ADDRESS, NOT `GetBtStatus`. Measured on device: with the
 * link dropped, `GetBtStatus` still reads 3 while `AvSrc` sits at 1 and `GetConnectInformation`
 * returns an empty address (`cinder-probe --btlink drop`). So the route flag would say "still on
 * Bluetooth" for a sink that is not there — the address is the only honest signal, and it is what
 * `refresh_bt_connected` already tracks.
 *
 * EDGE-TRIGGERED on linked -> unlinked ONLY, for the same three reasons as the jack:
 *   - level-triggering would re-pause forever while no headphones are connected;
 *   - the first observation of a boot only seeds the state, so a device that boots with nothing
 *     paired does not pause the moment it starts;
 *   - connecting must obviously do nothing.
 *
 * `playing` is the player's own transport intent. A device that is already paused has nothing to
 * pause, and saying so in the log would be noise on every headphone power-off.
 *
 * This deliberately does NOT distinguish a link that dropped by itself (range, the headphones
 * switching off, another host stealing them) from one the user hung up on in the Bluetooth screen,
 * or from the radio being switched off. All three mean the same thing to the person listening: the
 * sound has nowhere to go. The caller logs which it was.
 */
#ifndef CINDER_BT_EDGE_H
#define CINDER_BT_EDGE_H

/* `prev` < 0 means "nothing observed yet this boot". `prev`/`now`: 1 = a peer address is present. */
static inline int cinder_bt_should_pause(int prev, int now, int playing)
{
    if (prev < 0) return 0;              /* first observation: seed only */
    if (!playing) return 0;              /* nothing to pause */
    return (prev != 0 && now == 0);      /* the disconnect edge, and nothing else */
}

#endif /* CINDER_BT_EDGE_H */
