/* pcm_rate.h — how fast PCM really reaches the Bluetooth encoder, against the rate it should have
 * (#44: "the speed changes over Bluetooth").
 *
 * Every codec's PCM passes `btmtk_a2dp_send_audio_stream_data` in this process, so this is the one
 * place a wrong rate (audio at 44.1 kHz into an encoder running at 48, which plays 9 % fast) or a
 * stalled source (the headphones' buffer drains, and some answer by changing speed) can be seen
 * for SBC, LDAC and AAC alike. Pure, so the host test can drive it with made-up times.
 *
 * The first window of a stream is not judged: the stack fills its queue as fast as it can at the
 * start, which reads as a rate well above the real one. */
#ifndef PCM_RATE_H
#define PCM_RATE_H

enum {
    PCM_RATE_WINDOW_MS = 10000,  /* rate judged over 10 s */
    PCM_RATE_STALL_MS = 150,     /* a wait this long between PCM calls is a stall */
    PCM_RATE_OFF_PERMILLE = 10,  /* a window more than 1 % off the expected rate */
    PCM_RATE_STALL = 1,          /* pcm_rate_feed's results, OR-ed */
    PCM_RATE_OFF = 2,
};

typedef struct {
    int expect_hz, frame_bytes;
    long long t0_ms, last_ms, win_ms;
    unsigned long long bytes, win_bytes;
    int judged;                  /* the first window has passed */
    long worst_gap_ms;
    unsigned long stalls, off_windows;
    long last_hz;                /* the latest judged window's rate */
} pcm_rate;

static inline void pcm_rate_start(pcm_rate* m, int expect_hz, int frame_bytes, long long now_ms)
{
    pcm_rate z = {0};
    *m = z;
    m->expect_hz = expect_hz;
    m->frame_bytes = frame_bytes > 0 ? frame_bytes : 4;
    m->t0_ms = m->last_ms = m->win_ms = now_ms;
}

/* One PCM call of `n` bytes at `now_ms`. Returns PCM_RATE_* bits; *gap_ms is the wait before
 * this call, and m->last_hz the rate of a window that just closed. */
static inline int pcm_rate_feed(pcm_rate* m, unsigned n, long long now_ms, long* gap_ms)
{
    int r = 0;
    const long gap = (long)(now_ms - m->last_ms);
    *gap_ms = gap;
    m->last_ms = now_ms;
    m->bytes += n;
    if (m->bytes > n && gap > PCM_RATE_STALL_MS) {   /* not the first call */
        m->stalls++;
        if (gap > m->worst_gap_ms) m->worst_gap_ms = gap;
        r |= PCM_RATE_STALL;
        /* The window would average the stall with its catch-up; start a fresh one after it. */
        m->win_ms = now_ms;
        m->win_bytes = n;
        return r;
    }
    m->win_bytes += n;
    const long span = (long)(now_ms - m->win_ms);
    if (span >= PCM_RATE_WINDOW_MS) {
        m->last_hz = (long)(m->win_bytes * 1000ULL / (unsigned)m->frame_bytes / (unsigned long long)span);
        m->win_ms = now_ms;
        m->win_bytes = 0;
        if (m->judged && m->expect_hz > 0) {
            long d = m->last_hz - m->expect_hz;
            if (d < 0) d = -d;
            if (d * 1000 > (long)m->expect_hz * PCM_RATE_OFF_PERMILLE) {
                m->off_windows++;
                r |= PCM_RATE_OFF;
            }
        }
        m->judged = 1;
    }
    return r;
}

/* The whole stream's average rate, in Hz. */
static inline long pcm_rate_avg(const pcm_rate* m)
{
    const long long span = m->last_ms - m->t0_ms;
    return span > 0 ? (long)(m->bytes * 1000ULL / (unsigned)m->frame_bytes / (unsigned long long)span) : 0;
}

#endif
