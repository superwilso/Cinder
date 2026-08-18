/* jack_edge.h — when a headphone-jack observation should pause playback.
 *
 * Shared between main.cpp and tools/jackedge_selftest.cpp so the rule is tested where it lives.
 * It is three lines and it was still wrong the first time it was written — the original inverted
 * form paused on plug-IN, which is the exact opposite of the feature.
 *
 * The jack is read from /sys/class/switch/cxd3778gf_h2w/state, the standard Android headset switch:
 * 0 = nothing in the jack, non-zero = headphone or headset (the value distinguishes which).
 *
 * EDGE-TRIGGERED on plugged -> unplugged ONLY:
 *   - level-triggering would re-pause forever while the jack sits empty;
 *   - the first observation of a boot must never act, or a device powered on with an empty jack
 *     would pause something the moment it started;
 *   - and plugging IN must obviously do nothing.
 */
#ifndef CINDER_JACK_EDGE_H
#define CINDER_JACK_EDGE_H

/* `prev` < 0 means "nothing observed yet this boot". */
static inline int cinder_jack_should_pause(int prev, int now)
{
    if (prev < 0) return 0;          /* first observation: seed only */
    if (now  < 0) return 0;          /* unreadable: never act on a guess */
    return (prev != 0 && now == 0);  /* the unplug edge, and nothing else */
}

#endif /* CINDER_JACK_EDGE_H */
