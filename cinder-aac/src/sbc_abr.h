/* sbc_abr.h — SBC XQ's adaptive bitpool ("Auto", like LDAC's Best effort), #44.
 *
 * mtkbt reports its transmit queue's depth in packets when asked (the request LDAC's ABR makes,
 * analysis/RE_bt_send_timing.md): 0 while the radio keeps up, rising to its ceiling of 10 when it
 * cannot, and at 10 the stack throws packets away, which plays as the music speeding up. libsbc
 * re-reads the bitpool on every frame, so the rate can change between frames with no restart.
 *
 * This decides the bitpool from the queue readings (about ten a second). Down one step when two
 * readings in a row are 4 or more, at most once a second, so the queue has time to drain; down two
 * at once on a reading of 8 or more, because by then packets are being thrown away (the first walk
 * test took 3 s to reach the floor and the skips fell in those 3 s). Up one step after 10 s of
 * readings at 0; a step up that the queue answers within 15 s doubles the quiet needed for the
 * next one (to 80 s at most), and a minute at the top resets it (the second walk stepped up at the
 * edge of range and was straight back at 10). Never above the stream's starting bitpool (the
 * agreed top) or below the link's agreed minimum. Pure, so the host test can drive it. */
#ifndef SBC_ABR_H
#define SBC_ABR_H

enum {
    SBC_ABR_HOT = 4,          /* a reading this deep is the radio falling behind */
    SBC_ABR_FULL = 8,         /* this deep, packets are being dropped: step twice, at once */
    SBC_ABR_RETRY_MS = 15000, /* congestion this soon after a step up: that step failed */
    SBC_ABR_QUIET_MAX_MS = 80000,
    SBC_ABR_SETTLED_MS = 60000, /* this long at the top resets the quiet needed */
    SBC_ABR_DOWN_GAP_MS = 1000,
    SBC_ABR_QUIET_MS = 10000, /* this long at 0 before stepping back up */
};

/* Dual Channel at 44.1 kHz: about 551, 452, 364 and 298 kbit/s. 24 is about plain SBC's rate. */
static const int SBC_ABR_STEPS[] = {47, 38, 30, 24};
#define SBC_ABR_NSTEPS ((int)(sizeof SBC_ABR_STEPS / sizeof SBC_ABR_STEPS[0]))

typedef struct {
    int top, floor, cur;      /* bitpools: the start, the lowest allowed, the one in use */
    int hot;                  /* readings in a row at SBC_ABR_HOT or more */
    long long changed_ms;     /* when cur last changed */
    long long quiet_ms;       /* when the readings went to 0; -1 while they are not */
    long long up_ms;          /* when it last stepped up; -1 for never */
    long quiet_need_ms;       /* quiet needed before the next step up */
} sbc_abr;

static inline void sbc_abr_start(sbc_abr* m, int top, int link_min, long long now_ms)
{
    m->top = m->cur = top;
    const int lowest = SBC_ABR_STEPS[SBC_ABR_NSTEPS - 1];
    m->floor = link_min > lowest ? link_min : lowest;
    if (m->floor > top) m->floor = top;
    m->hot = 0;
    m->changed_ms = now_ms;
    m->quiet_ms = -1;
    m->up_ms = -1;
    m->quiet_need_ms = SBC_ABR_QUIET_MS;
}

/* The next step below / above `bp`, kept inside floor..top. */
static inline int sbc_abr_below(const sbc_abr* m, int bp)
{
    for (int i = 0; i < SBC_ABR_NSTEPS; i++)
        if (SBC_ABR_STEPS[i] < bp) return SBC_ABR_STEPS[i] > m->floor ? SBC_ABR_STEPS[i] : m->floor;
    return m->floor;
}
static inline int sbc_abr_above(const sbc_abr* m, int bp)
{
    for (int i = SBC_ABR_NSTEPS - 1; i >= 0; i--)
        if (SBC_ABR_STEPS[i] > bp) return SBC_ABR_STEPS[i] < m->top ? SBC_ABR_STEPS[i] : m->top;
    return m->top;
}

/* One queue reading. Returns the new bitpool when it should change, else 0. */
static inline int sbc_abr_feed(sbc_abr* m, int q, long long now_ms)
{
    m->hot = q >= SBC_ABR_HOT ? m->hot + 1 : 0;
    if (q > 0) m->quiet_ms = -1;
    else if (m->quiet_ms < 0) m->quiet_ms = now_ms;
    const int full = q >= SBC_ABR_FULL;
    if ((m->hot >= 2 || full) && m->cur > m->floor && now_ms - m->changed_ms >= SBC_ABR_DOWN_GAP_MS) {
        if (m->up_ms >= 0 && now_ms - m->up_ms < SBC_ABR_RETRY_MS) {
            m->quiet_need_ms *= 2;
            if (m->quiet_need_ms > SBC_ABR_QUIET_MAX_MS) m->quiet_need_ms = SBC_ABR_QUIET_MAX_MS;
        }
        m->up_ms = -1;
        m->cur = sbc_abr_below(m, m->cur);
        if (full) m->cur = sbc_abr_below(m, m->cur);
        m->changed_ms = now_ms;
        return m->cur;
    }
    if (m->quiet_ms >= 0 && m->cur < m->top && now_ms - m->quiet_ms >= m->quiet_need_ms &&
        now_ms - m->changed_ms >= m->quiet_need_ms) {
        m->cur = sbc_abr_above(m, m->cur);
        m->changed_ms = now_ms;
        m->up_ms = now_ms;
        m->quiet_ms = now_ms;   /* the next step up waits its own quiet stretch */
        return m->cur;
    }
    if (m->cur == m->top && m->quiet_ms >= 0 && now_ms - m->changed_ms >= SBC_ABR_SETTLED_MS)
        m->quiet_need_ms = SBC_ABR_QUIET_MS;
    return 0;
}

#endif
