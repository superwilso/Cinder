/* Host test for pcm_rate.h: cc -O2 -Wall -o /tmp/pcm_rate_test test/pcm_rate_test.c && /tmp/pcm_rate_test */
#include <assert.h>
#include <stdio.h>

#include "../src/pcm_rate.h"

/* Feeds `secs` of PCM at `hz` in 46 ms writes (Sony's 2048-frame chunk), starting at *t. */
static int run(pcm_rate* m, long long* t, int hz, int secs)
{
    int bits = 0;
    long gap;
    const unsigned n = 2048 * 4;
    const long long end = *t + secs * 1000LL;
    double due = (double)*t;
    while (*t < end) {
        bits |= pcm_rate_feed(m, n, *t, &gap);
        due += 2048.0 * 1000.0 / hz;
        *t = (long long)due;
    }
    return bits;
}

int main(void)
{
    pcm_rate m;
    long long t = 1000;
    long gap;

    /* Right rate: never off, never a stall. */
    pcm_rate_start(&m, 44100, 4, t);
    assert(run(&m, &t, 44100, 60) == 0);
    assert(m.off_windows == 0 && m.stalls == 0);
    assert(pcm_rate_avg(&m) > 43900 && pcm_rate_avg(&m) < 44300);

    /* 44.1 kHz PCM into a 48 kHz encoder: every judged window is off. */
    pcm_rate_start(&m, 48000, 4, t);
    assert(run(&m, &t, 44100, 60) & PCM_RATE_OFF);
    assert(m.off_windows >= 4);
    assert(m.last_hz > 43500 && m.last_hz < 44700);

    /* The first window is not judged, however fast the start-up fill is. */
    pcm_rate_start(&m, 44100, 4, t);
    for (int i = 0; i < 200; i++) assert(pcm_rate_feed(&m, 8192, t + i * 50, &gap) == 0);

    /* A 400 ms wait is a stall, counted with its length. */
    pcm_rate_start(&m, 44100, 4, t);
    run(&m, &t, 44100, 5);
    t += 400;
    assert(pcm_rate_feed(&m, 8192, t, &gap) & PCM_RATE_STALL);
    assert(gap >= 400 && m.stalls == 1 && m.worst_gap_ms >= 400);

    puts("pcm_rate: ok");
    return 0;
}
