/* cinder-pathwatch.c — keeps Sony's audio route on Bluetooth while Cinder's own Bluetooth stack
 * carries the stream (docs/PLAN_hardware_first.md, Phase 4a).
 *
 * WHY. Sony's sound service sends its processed Bluetooth stream (after DSEE, the EQs and the rest)
 * only while PathMgr's A2DP flag is set (PathMgrServiceServiceImpl +0x8c, device 5, which outranks
 * every other output). Only Sony's own Bluetooth stack sets it, through ConnMgr's listener; no IPC
 * method does (PathMgrServiceFwClient has GetCurrentOutputDevice, SetPath, GetPath and the
 * listeners, nothing else). With Sony's stack stopped so ours can use the radio, the flag drops and
 * the music moves to the jack.
 *
 * WHAT. Loaded into PathMgrServiceFw (hagoromo19) by cinder-hagowrap as HAGO_DIR/PathMgrServiceFw.so.
 * It stands in front of PathMgrServiceServiceImpl::Start(), which Sony's own library calls through
 * its PLT, only to learn the object. One thread, every 0.5 s: while /tmp/cinder_a2dp_target is
 * fresh (Cinder's stack rewrites it each second) and the flag is off, it calls Sony's own
 * InformDeviceEnabled(A2DP, 1), the call ConnMgr's listener makes; once the file is gone or stale it
 * calls InformDeviceEnabled(A2DP, 0) if it was the one that set the flag. Sony's own calls are never
 * changed. Every lookup that fails leaves the service exactly as Sony shipped it.
 */
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#include <dlfcn.h>
#include <fcntl.h>
#include <link.h>
#include <pthread.h>
#include <stdarg.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

#ifndef CINDER_A2DP_TARGET
#define CINDER_A2DP_TARGET "/tmp/cinder_a2dp_target"
#endif
static const char kLog[] = "/tmp/cinder_pathwatch.log";
enum { DEVICE_A2DP = 5, FRESH_SECONDS = 5, POLL_MS = 500 };

typedef int (*start_fn)(void* self);
typedef void (*inform_fn)(void* self, const int* device, const int* enabled);
typedef int (*connected_fn)(void* self, const int* device);

static void* g_impl;
static inform_fn g_inform;
static connected_fn g_connected;

static void say(const char* fmt, ...)
{
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

/* The target counts only while Cinder's stack keeps it fresh: a stack that died without cleaning up
 * must not hold the route on a socket nobody reads. Shared rule with cinder-mono.c's redirect. */
static int target_live(void)
{
    struct stat st;
    return stat(CINDER_A2DP_TARGET, &st) == 0 && st.st_mtime >= time(NULL) - FRESH_SECONDS;
}

static void* watch(void* arg)
{
    (void)arg;
    const int dev = DEVICE_A2DP;
    int ours = 0;
    for (;;) {
        usleep(POLL_MS * 1000);
        const int live = target_live();
        const int on = g_connected(g_impl, &dev) == 1;
        if (live && !on) {
            const int one = 1;
            g_inform(g_impl, &dev, &one);
            ours = 1;
            say("A2DP held on for Cinder's Bluetooth (now %d)", g_connected(g_impl, &dev));
        } else if (!live && ours) {
            ours = 0;
            if (on) {
                const int zero = 0;
                g_inform(g_impl, &dev, &zero);
                say("A2DP released (now %d)", g_connected(g_impl, &dev));
            }
        }
    }
    return NULL;
}

/* Sony's library, by name among the loaded objects: hagodaemon dlopens it privately, so RTLD_NEXT
 * from a preload never sees it, and the call into Start is a tail call, so the return address is
 * not in it either (11-22 on the player: the hook found nothing and PathMgr's own Start never ran
 * that boot). */
static int find_lib(struct dl_phdr_info* info, size_t size, void* out)
{
    (void)size;
    const char* n = info->dlpi_name;
    const size_t len = n ? strlen(n) : 0, want = sizeof "/libPathMgrServiceFw.so" - 1;
    if (len < want || strcmp(n + len - want, "/libPathMgrServiceFw.so")) return 0;
    *(const char**)out = n;
    return 1;
}

static void* sony_sym(const char* name)
{
    const char* path = NULL;
    dl_iterate_phdr(find_lib, &path);
    void* h = path ? dlopen(path, RTLD_NOW | RTLD_NOLOAD) : NULL;
    void* f = h ? dlsym(h, name) : NULL;
    if (h) dlclose(h);   /* NOLOAD took a reference; the library stays loaded */
    return f;
}

int _ZN3pst8services8funcarch7pathmgr25PathMgrServiceServiceImpl5StartEv(void* self)
{
    static start_fn real;
    if (!real) real = (start_fn)sony_sym("_ZN3pst8services8funcarch7pathmgr25PathMgrServiceServiceImpl5StartEv");
    if (!real) {
        say("Sony's Start not found: PathMgr will not start (report this)");
        return 0;
    }
    const int rc = real(self);
    if (!g_impl) {
        g_inform = (inform_fn)sony_sym(
            "_ZN3pst8services8funcarch7pathmgr25PathMgrServiceServiceImpl19InformDeviceEnabledERKNS1_6DeviceERKNS1_7connmgr7EnabledE");
        g_connected = (connected_fn)sony_sym(
            "_ZN3pst8services8funcarch7pathmgr25PathMgrServiceServiceImpl17IsDeviceConnectedERKNS1_12OutputDeviceE");
        pthread_t t;
        if (g_inform && g_connected) {
            g_impl = self;
            if (pthread_create(&t, NULL, watch, NULL) == 0) {
                pthread_detach(t);
                say("watching %s", CINDER_A2DP_TARGET);
            }
        } else {
            say("Sony's PathMgr symbols not found; doing nothing");
        }
    }
    return rc;
}
