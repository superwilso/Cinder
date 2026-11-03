/* Host test for sbc_abr.h: cc -O2 -Wall -o /tmp/sbc_abr_test test/sbc_abr_test.c && /tmp/sbc_abr_test */
#include <assert.h>
#include <stdio.h>

#include "../src/sbc_abr.h"

/* Feeds `secs` of readings at `q`, ten a second, from *t. Returns the last bitpool change seen. */
static int run(sbc_abr* m, long long* t, int q, int secs)
{
    int last = 0;
    for (int i = 0; i < secs * 10; i++, *t += 100) {
        int bp = sbc_abr_feed(m, q, *t);
        if (bp) last = bp;
    }
    return last;
}

int main(void)
{
    sbc_abr m;
    long long t = 0;

    /* A link that keeps up never moves off the top. */
    sbc_abr_start(&m, 47, 2, t);
    assert(run(&m, &t, 0, 60) == 0 && m.cur == 47);

    /* One reading of 4..7 is not enough; two in a row are, and step once. */
    sbc_abr_start(&m, 47, 2, t);
    t += 2000;
    assert(sbc_abr_feed(&m, 5, t) == 0);
    assert(sbc_abr_feed(&m, 0, t += 100) == 0);
    assert(sbc_abr_feed(&m, 5, t += 100) == 0);
    assert(sbc_abr_feed(&m, 5, t += 100) == 38);

    /* A full queue (8+) means packets are going: two steps at once, on the first reading. */
    sbc_abr_start(&m, 47, 2, t);
    t += 2000;
    assert(sbc_abr_feed(&m, 10, t) == 30);

    /* The walk: the queue full for 40 s. Down to the floor in two seconds, no lower. */
    sbc_abr_start(&m, 47, 2, t);
    t += 2000;
    run(&m, &t, 10, 1);
    assert(m.cur == 30);
    run(&m, &t, 10, 40);
    assert(m.cur == 24);

    /* Back in range: up one step per 10 quiet seconds, to the top and no further. */
    assert(run(&m, &t, 0, 9) == 0 && m.cur == 24);
    run(&m, &t, 0, 2);
    assert(m.cur == 30);
    run(&m, &t, 0, 40);
    assert(m.cur == 47);
    run(&m, &t, 0, 30);
    assert(m.cur == 47);

    /* Walk 2: a step up at the edge of range that the queue answers at once. The next step up
     * waits twice as long; a minute settled at the top puts it back to 10 s. */
    sbc_abr_start(&m, 47, 2, t);
    t += 2000;
    run(&m, &t, 10, 2);
    assert(m.cur == 24);
    run(&m, &t, 0, 11);
    assert(m.cur == 30 && m.quiet_need_ms == SBC_ABR_QUIET_MS);
    run(&m, &t, 10, 1);
    assert(m.cur == 24 && m.quiet_need_ms == 2 * SBC_ABR_QUIET_MS);
    assert(run(&m, &t, 0, 15) == 0 && m.cur == 24);
    run(&m, &t, 0, 6);
    assert(m.cur == 30);
    run(&m, &t, 0, 120);
    assert(m.cur == 47);
    run(&m, &t, 0, 61);
    assert(m.quiet_need_ms == SBC_ABR_QUIET_MS);

    /* A shallow queue (1..3) is neither a reason to drop nor quiet enough to climb. */
    sbc_abr_start(&m, 47, 2, t);
    run(&m, &t, 10, 3);
    const int low = m.cur;
    assert(run(&m, &t, 2, 30) == 0 && m.cur == low);

    /* The link's agreed range wins: a top of 39 and a minimum of 35 keep it in 35..39. */
    sbc_abr_start(&m, 39, 35, t);
    run(&m, &t, 10, 10);
    assert(m.cur == 35);
    run(&m, &t, 0, 30);
    assert(m.cur == 39);

    puts("sbc_abr: ok");
    return 0;
}
