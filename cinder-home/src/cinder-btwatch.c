/* cinder-btwatch.c — opens AVRCP when Sony's Bluetooth stack linked A2DP without it.
 *
 * WHY. Volume to Bluetooth headphones travels over AVRCP. Sony's middleware (libBtMw.so,
 * FUN_00027920, the A2DP source state handler) opens AVRCP after an A2DP connect only when the
 * state before the connect was 3 (our own page); otherwise it waits for the headphones to open it,
 * and nothing retries. Seen 2026-11-18 on the owner's player: A2DP linked (state 5), AVRCP
 * disconnected for minutes, seven volume presses unanswered, then cinder-home's repair hung up and
 * redialled. docs/PLAN_hardware_first.md, Phase 0.
 *
 * WHAT. Loaded into Sony's Bluetooth service (hagoromo27) by cinder-hagowrap
 * (HAGO_DIR/BtCommonService.2.so). One thread: once a second it reads the A2DP state and whether
 * AVRCP is down. Linked without AVRCP for 3 s in a row → it arms a middleware timer whose callback
 * calls Sony's own btmwAvrcpSrcConnect(), so the connect runs on the middleware's timer thread, as
 * Sony's own deferred AVRCP connect does (timer 0x52, the same id, so Sony's TimerStop(0x52) on a
 * disconnect cancels ours too). At most once per 30 s.
 *
 * Every lookup that fails leaves the service exactly as Sony shipped it. Nothing is hooked.
 */
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#include <dlfcn.h>
#include <fcntl.h>
#include <pthread.h>
#include <stdarg.h>
#include <stdio.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

static const char kLog[] = "/tmp/cinder_btwatch.log";
enum { AVSRC_LINKED = 5, TIMER_AVRCP = 0x52, UNIT_MS = 1, DOWN_SECONDS = 3, GAP_SECONDS = 30 };

typedef void* (*instance_fn)(void);
typedef int (*bool_fn)(void);
typedef void (*void_fn)(void);
typedef int (*timer_fn)(int id, unsigned n, unsigned unit, void (*cb)(void*), void* arg);

static void_fn g_connect;
static bool_fn g_avrcp_down;

static void say(const char* fmt, ...)
{
    /* cinder-home (uid 100) copies these lines into its own log; the service may run as root. */
    int fd = open(kLog, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0666);
    if (fd < 0) return;
    if (geteuid() == 0) fchmod(fd, 0666);
    char b[200];
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    int n = snprintf(b, sizeof b, "%ld.%03ld ", (long)ts.tv_sec, ts.tv_nsec / 1000000);
    va_list ap;
    va_start(ap, fmt);
    n += vsnprintf(b + n, sizeof b - n, fmt, ap);
    va_end(ap);
    if (n > (int)sizeof b - 2) n = sizeof b - 2;
    b[n++] = '\n';
    (void)!write(fd, b, n);
    close(fd);
}

/* Runs on the middleware's timer thread. Sony's function itself does nothing unless AVRCP is
 * disconnected; the down check is repeated because the link may have changed in the meantime. */
static void connect_avrcp(void* arg)
{
    (void)arg;
    if (g_avrcp_down()) g_connect();
}

static void* watch(void* arg)
{
    (void)arg;
    void* mw = NULL;
    /* hagodaemon dlopens the service libraries privately after start; wait for libBtMw. */
    for (int i = 0; i < 120 && !(mw = dlopen("libBtMw.so", RTLD_NOW | RTLD_NOLOAD)); i++) sleep(1);
    instance_fn avsrc = mw ? (instance_fn)dlsym(mw, "btmwAvSrcGetInstance") : NULL;
    g_avrcp_down = mw ? (bool_fn)dlsym(mw, "btmwAvrcpSrcIsDisconnected") : NULL;
    g_connect = mw ? (void_fn)dlsym(mw, "btmwAvrcpSrcConnect") : NULL;
    timer_fn timer = mw ? (timer_fn)dlsym(mw, "btmwTimerStart") : NULL;
    if (!avsrc || !g_avrcp_down || !g_connect || !timer) {
        say("not watching: libBtMw %s", mw ? "lacks a function" : "never loaded");
        return NULL;
    }
    say("watching for A2DP without AVRCP");
    int down = 0;
    time_t last = 0;
    for (;;) {
        sleep(1);
        char* a = (char*)avsrc();
        if (!a || *(volatile int*)(a + 4) != AVSRC_LINKED || !g_avrcp_down()) {
            down = 0;
            continue;
        }
        time_t now = time(NULL);
        if (++down < DOWN_SECONDS || now - last < GAP_SECONDS) continue;
        last = now;
        down = 0;
        int rc = timer(TIMER_AVRCP, 100, UNIT_MS, connect_avrcp, NULL);
        say("A2DP linked without AVRCP for %d s: asked for AVRCP (timer rc=%d)", DOWN_SECONDS, rc);
    }
    return NULL;
}

__attribute__((constructor)) static void at_load(void)
{
    if (access("/data/cinder/btwatch_off", F_OK) == 0) return;
    pthread_t t;
    pthread_attr_t at;
    pthread_attr_init(&at);
    pthread_attr_setdetachstate(&at, PTHREAD_CREATE_DETACHED);
    pthread_attr_setstacksize(&at, 64 * 1024);
    if (pthread_create(&t, &at, watch, NULL) != 0) say("not watching: no thread");
    pthread_attr_destroy(&at);
}
