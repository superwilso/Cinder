/* bt_vol_watch.h — notice a Bluetooth link whose volume steps go nowhere, and say when to redial.
 *
 * THE FAULT (owner report, 2026-10-24): rarely, and only right after a boot, the volume rocker
 * moves the bar but not the headphones. Disconnecting and reconnecting the headphones fixes it.
 * The suspected cause is a connect race at boot (the ladder paging while the headphones page us)
 * that leaves audio up but the remote-control channel, which carries volume, dead.
 *
 * THE SIGNAL. Every step that reaches a sink comes back as OnNotifyChangeVolume — measured on
 * every sink tried — so a step with no answer is a step that went nowhere. One silent step is not
 * enough: the FIRST step after a boot routinely gets no answer on a healthy link (three boots,
 * 2026-10-24) and the next ones do. So it takes CINDER_BT_VOLW_MISSES silent steps in a row, and any
 * answer clears the count.
 *
 * Steps at the ends of the scale are not counted: a sink already at the top has nothing to report
 * when told to go up. Neither are presses that land while one is still waiting — two quick presses
 * are one question.
 *
 * The caller adds two guards this header cannot see: only a sink that has answered before (some
 * never send volume reports at all, and would otherwise be hung up on every second press), and at
 * most one repair per boot.
 */
#ifndef CINDER_BT_VOL_WATCH_H
#define CINDER_BT_VOL_WATCH_H

#define CINDER_BT_VOLW_ANSWER_MS 1500
#define CINDER_BT_VOLW_MISSES    2

enum { CINDER_BT_VOLW_NOTHING = 0, CINDER_BT_VOLW_MISS = 1, CINDER_BT_VOLW_REPAIR = 2 };

typedef struct {
    long long pending_at;   /* when the unanswered step went out; 0 = none waiting */
    int misses;             /* silent steps in a row on this link */
} cinder_bt_volw;

/* A new link: nothing it has done yet counts. */
static inline void cinder_bt_volw_link(cinder_bt_volw* w) { w->pending_at = 0; w->misses = 0; }

/* Can a step in this direction move a sink at `level` of `max`? */
static inline int cinder_bt_volw_can_move(int level, int max, int up)
{
    return up ? level < max - 2 : level > 2;
}

static inline void cinder_bt_volw_step(cinder_bt_volw* w, long long now, int can_move)
{
    if (can_move && !w->pending_at) w->pending_at = now;
}

static inline void cinder_bt_volw_report(cinder_bt_volw* w) { w->pending_at = 0; w->misses = 0; }

/* Call while a step is pending. */
static inline int cinder_bt_volw_tick(cinder_bt_volw* w, long long now)
{
    if (!w->pending_at || now - w->pending_at < CINDER_BT_VOLW_ANSWER_MS) return CINDER_BT_VOLW_NOTHING;
    w->pending_at = 0;
    return ++w->misses >= CINDER_BT_VOLW_MISSES ? CINDER_BT_VOLW_REPAIR : CINDER_BT_VOLW_MISS;
}

#endif /* CINDER_BT_VOL_WATCH_H */
