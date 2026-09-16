/* cinder-mono.c — libcinder_mono.so: system-wide mono, preloaded into Sony's SoundServiceFw.
 *
 * WHAT IT DOES. While /tmp/cinder_mono exists, every stereo buffer SoundServiceFw sends to the
 * 3.5 mm jack (snd_pcm_writei) or to the Bluetooth transmitter (write/send on the
 * `pst::services::bttransmitterservice` socket) goes out with left and right summed. The sums are
 * in mono_sum.h, host-tested by tools/mono_selftest.cpp; this file is only the interposition.
 *
 * HOW IT GETS THERE. LD_PRELOAD on the SoundServiceFw service (init.hagoromo.rc), next to Wampy's
 * libsound_service_fw.so when that is installed — `setenv LD_PRELOAD "<wampy> <this>"`. Wampy hooks
 * FilterChain methods, this hooks libc and libasound, so the two do not overlap. Every original is
 * reached with dlsym(RTLD_NEXT), so the order between them does not matter either.
 *
 * WHY IT IS SAFE TO LOAD.
 *   * Outside SoundServiceFw it does nothing: the constructor checks the command line, and every
 *     hook's first test is that flag.
 *   * With mono off it does nothing but a cached access() every 250 ms on the audio path.
 *   * The only fds it looks at are ones it saw connect to the transmitter socket, and it changes no
 *     byte on them until the handshake has gone by (see mono_sum.h for why that matters).
 *   * Anything it does not recognise — a format, a channel count, DSD over PCM, a frame type — it
 *     passes through exactly as it came.
 *   * No allocation it cannot survive: if the scratch buffer cannot grow, the original goes out.
 */
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <pthread.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/syscall.h>
#include <sys/types.h>
#include <sys/uio.h>
#include <sys/un.h>
#include <time.h>
#include <unistd.h>

#include "mono_sum.h"

/* The test (tools/test_mono_shim.sh) points these somewhere private; the device build never does. */
#ifndef CINDER_MONO_DIR
#define CINDER_MONO_DIR "/tmp"
#endif
static const char kFlag[] = CINDER_MONO_DIR "/cinder_mono";
static const char kAlive[] = CINDER_MONO_DIR "/cinder_mono_shim";
static const char kLog[] = CINDER_MONO_DIR "/cinder_mono.log";
static const char kBtName[] = "pst::services::bttransmitterservice";

/* ── how it is loaded, and how it gets out of the way ─────────────────────────────────────────
 * Cinder never edits the boot image, so it cannot add an LD_PRELOAD of its own. What it can use is
 * the one Wampy's installer already put there: `setenv LD_PRELOAD <kWampyPath>` on SoundServiceFw.
 * The installer moves Wampy's library to kWampyOrig and puts this one at kWampyPath; this one then
 * loads Wampy's with RTLD_GLOBAL before hagodaemon dlopens libSoundServiceFw.so, so Wampy's
 * FilterChain hooks still come first in the global scope and its dlsym(RTLD_NEXT) still finds
 * Sony's originals after it.
 *
 * A SoundServiceFw that dies takes the whole player down — stopping it at runtime POWERED THE
 * DEVICE OFF on 2026-09-16 — so a library that crashed it at load would do that on every boot, and
 * no escape in the launcher runs early enough to help. Hence kBoots: every load counts itself
 * BEFORE doing anything else, the count is cleared once audio has gone through a hook (and by
 * cinder-home once the boot is healthy), and a library that finds kMaxBoots uncleared loads in SAFE
 * MODE: no hooks, no Wampy, exactly stock. kOff is the manual switch for the hooks alone. */
#ifndef CINDER_MONO_DATA
#define CINDER_MONO_DATA "/data/cinder"
#endif
#ifndef CINDER_WAMPY_LIB
#define CINDER_WAMPY_LIB "/system/vendor/unknown321/lib/libsound_service_fw"
#endif
static const char kBoots[] = CINDER_MONO_DATA "/mono_shim_boots";
static const char kOff[] = CINDER_MONO_DATA "/mono_shim_off";
static const char kWampyOrig[] = CINDER_WAMPY_LIB ".wampy.so";
enum { kMaxBoots = 2 };

static int g_active = 0;   /* this process is SoundServiceFw (or a test that says it is) */

static ssize_t (*real_write)(int, const void*, size_t);
static ssize_t (*real_send)(int, const void*, size_t, int);
static ssize_t (*real_sendto)(int, const void*, size_t, int, const struct sockaddr*, socklen_t);
static ssize_t (*real_writev)(int, const struct iovec*, int);
static ssize_t (*real_sendmsg)(int, const struct msghdr*, int);
static int (*real_connect)(int, const struct sockaddr*, socklen_t);
static int (*real_close)(int);
static long (*real_writei)(void*, const void*, unsigned long);
static int (*real_set_format)(void*, void*, int);
static int (*real_set_channels)(void*, void*, unsigned int);
static int (*real_pcm_close)(void*);

/* hagodaemon runs the constructor as ROOT and only then becomes `system` (uid 100, no
 * CAP_DAC_OVERRIDE), and root's umask here is 077 — so a file the constructor creates in /tmp is
 * root 0600 and every later line from the audio threads fails silently (read off the device
 * 2026-09-16: an empty log after real playback proved nothing). Files meant to be written again, or
 * read by cinder-home, get their mode set explicitly. O_NOFOLLOW: the root phase must not be
 * steered through a link someone left in /tmp. */
static int open_shared(const char* path, int flags)
{
    int fd = (int)syscall(SYS_open, path, flags | O_CLOEXEC | O_NOFOLLOW, 0666);
    if (fd >= 0 && (flags & O_CREAT) && geteuid() == 0) (void)syscall(SYS_fchmod, fd, 0666);
    return fd;
}

/* ── log: a few lines per session, never the audio path's problem ────────────────────────────── */
static int g_log_lines = 0;
static void mlog(const char* fmt, ...)
{
    if (g_log_lines >= 200) return;
    g_log_lines++;
    char m[256];
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    int k = snprintf(m, sizeof m, "%ld.%03ld [%d] ", (long)ts.tv_sec, ts.tv_nsec / 1000000L, (int)getpid());
    va_list ap;
    va_start(ap, fmt);
    if (k > 0 && k < (int)sizeof m) k += vsnprintf(m + k, sizeof m - (size_t)k, fmt, ap);
    va_end(ap);
    if (k <= 0) return;
    if (k >= (int)sizeof m - 1) k = (int)sizeof m - 2;
    m[k++] = '\n';
    int fd = open_shared(kLog, O_WRONLY | O_CREAT | O_APPEND);
    if (fd < 0) return;
    (void)syscall(SYS_write, fd, m, (size_t)k);
    (void)syscall(SYS_close, fd);
}

/* ── the switch ──────────────────────────────────────────────────────────────────────────────── */
static long g_flag_checked_ms = -1000000;
static int g_flag_on = 0;
static int mono_on(void)
{
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    const long now = (long)ts.tv_sec * 1000L + ts.tv_nsec / 1000000L;
    if (now - g_flag_checked_ms >= 250) {
        g_flag_checked_ms = now;
        const int on = access(kFlag, F_OK) == 0;
        if (on != g_flag_on) mlog("mono %s", on ? "ON" : "off");
        g_flag_on = on;
    }
    return g_flag_on;
}

/* ── scratch buffer, one per thread ──────────────────────────────────────────────────────────── */
static __thread uint8_t* t_buf = NULL;
static __thread size_t t_cap = 0;
static uint8_t* scratch(size_t n)
{
    if (n <= t_cap) return t_buf;
    size_t cap = n < 16384 ? 16384 : n;
    uint8_t* b = (uint8_t*)realloc(t_buf, cap);
    if (!b) return NULL;
    t_buf = b;
    t_cap = cap;
    return b;
}

#define RESOLVE(var, name) do { if (!(var)) (var) = dlsym(RTLD_NEXT, name); } while (0)

__attribute__((constructor)) static void cinder_mono_init(void)
{
    RESOLVE(real_write, "write");
    RESOLVE(real_send, "send");
    RESOLVE(real_sendto, "sendto");
    RESOLVE(real_writev, "writev");
    RESOLVE(real_sendmsg, "sendmsg");
    RESOLVE(real_connect, "connect");
    RESOLVE(real_close, "close");

    char cmd[512];
    int fd = (int)syscall(SYS_open, "/proc/self/cmdline", O_RDONLY | O_CLOEXEC);
    ssize_t n = fd >= 0 ? (ssize_t)syscall(SYS_read, fd, cmd, sizeof cmd - 1) : -1;
    if (fd >= 0) (void)syscall(SYS_close, fd);
    if (n <= 0) return;
    cmd[n] = '\0';
    /* init starts the service as `logwrapper hagodaemon SoundServiceFw …`, and LD_PRELOAD reaches the
     * logwrapper too. Only the service itself counts: two processes counting one boot would reach
     * safe mode after a single uncleared boot. */
    if (strstr(cmd, "logwrapper")) return;   /* argv[0], before the NULs become spaces */
    for (ssize_t i = 0; i < n; i++) if (cmd[i] == '\0') cmd[i] = ' ';
    if (!strstr(cmd, "SoundServiceFw")) return;

    /* Count this load first; everything after it is what could fail. */
    int boots = 0;
    int bf = (int)syscall(SYS_open, kBoots, O_RDONLY | O_CLOEXEC);
    if (bf >= 0) {
        char b[16] = { 0 };
        if (syscall(SYS_read, bf, b, sizeof b - 1) > 0) boots = atoi(b);
        (void)syscall(SYS_close, bf);
    }
    bf = (int)syscall(SYS_open, kBoots, O_WRONLY | O_CREAT | O_TRUNC | O_CLOEXEC, 0600);
    if (bf >= 0) {
        char b[16];
        int k = snprintf(b, sizeof b, "%d\n", boots + 1);
        if (k > 0) (void)syscall(SYS_write, bf, b, (size_t)k);
        (void)syscall(SYS_fsync, bf);
        (void)syscall(SYS_close, bf);
    }
    if (boots >= kMaxBoots) {
        mlog("SAFE MODE: %d loads in a row never reached audio — no hooks, no Wampy chain", boots);
        return;
    }

    if (access(kWampyOrig, F_OK) == 0) {
        void* h = dlopen(kWampyOrig, RTLD_NOW | RTLD_GLOBAL);
        mlog(h ? "chained Wampy's library (%s)" : "could not chain Wampy's library (%s): %s",
             kWampyOrig, h ? "" : dlerror());
    }
    if (access(kOff, F_OK) == 0) {
        mlog("hooks off (%s exists)", kOff);
        return;
    }
    g_active = 1;
    int a = open_shared(kAlive, O_WRONLY | O_CREAT | O_TRUNC);
    if (a >= 0) {
        char p[24];
        int k = snprintf(p, sizeof p, "%d\n", (int)getpid());
        if (k > 0) (void)syscall(SYS_write, a, p, (size_t)k);
        (void)syscall(SYS_close, a);
    }
    mlog("loaded into SoundServiceFw");
}

/* Audio went through a hook and the process is still here: the load is good. Once per process. */
static int g_boots_cleared = 0;
static void load_proved_good(void)
{
    if (g_boots_cleared) return;
    g_boots_cleared = 1;
    (void)syscall(SYS_unlink, kBoots);
    mlog("audio flowed — load count cleared");
}

/* ── the jack: libasound ─────────────────────────────────────────────────────────────────────── */
typedef struct { void* pcm; int fmt; int ch; int said; } pcm_info;
#define MAX_PCM 16
static pcm_info g_pcm[MAX_PCM];
static pthread_mutex_t g_pcm_lock = PTHREAD_MUTEX_INITIALIZER;

static pcm_info* pcm_slot(void* pcm, int create)   /* lock held */
{
    pcm_info* free_slot = NULL;
    for (int i = 0; i < MAX_PCM; i++) {
        if (g_pcm[i].pcm == pcm) return &g_pcm[i];
        if (!g_pcm[i].pcm && !free_slot) free_slot = &g_pcm[i];
    }
    if (!create || !free_slot) return NULL;
    memset(free_slot, 0, sizeof *free_slot);
    free_slot->pcm = pcm;
    return free_slot;
}

int snd_pcm_hw_params_set_format(void* pcm, void* params, int fmt)
{
    RESOLVE(real_set_format, "snd_pcm_hw_params_set_format");
    if (!real_set_format) return -ENOSYS;
    const int r = real_set_format(pcm, params, fmt);
    if (g_active && r == 0) {
        pthread_mutex_lock(&g_pcm_lock);
        pcm_info* s = pcm_slot(pcm, 1);
        if (s) { s->fmt = fmt; s->said = 0; }
        pthread_mutex_unlock(&g_pcm_lock);
    }
    return r;
}

int snd_pcm_hw_params_set_channels(void* pcm, void* params, unsigned int ch)
{
    RESOLVE(real_set_channels, "snd_pcm_hw_params_set_channels");
    if (!real_set_channels) return -ENOSYS;
    const int r = real_set_channels(pcm, params, ch);
    if (g_active && r == 0) {
        pthread_mutex_lock(&g_pcm_lock);
        pcm_info* s = pcm_slot(pcm, 1);
        if (s) { s->ch = (int)ch; s->said = 0; }
        pthread_mutex_unlock(&g_pcm_lock);
    }
    return r;
}

int snd_pcm_close(void* pcm)
{
    RESOLVE(real_pcm_close, "snd_pcm_close");
    if (g_active) {
        pthread_mutex_lock(&g_pcm_lock);
        pcm_info* s = pcm_slot(pcm, 0);
        if (s) s->pcm = NULL;
        pthread_mutex_unlock(&g_pcm_lock);
    }
    return real_pcm_close ? real_pcm_close(pcm) : -ENOSYS;
}

long snd_pcm_writei(void* pcm, const void* buf, unsigned long frames)
{
    RESOLVE(real_writei, "snd_pcm_writei");
    if (!real_writei) return -ENOSYS;
    if (!g_active || !buf || frames == 0) return real_writei(pcm, buf, frames);
    if (!mono_on()) {
        const long r = real_writei(pcm, buf, frames);
        if (r > 0) load_proved_good();
        return r;
    }

    pcm_info info = { NULL, 0, 0, 1 };
    int said = 1;
    pthread_mutex_lock(&g_pcm_lock);
    pcm_info* s = pcm_slot(pcm, 0);
    if (s) { info = *s; said = s->said; s->said = 1; }
    pthread_mutex_unlock(&g_pcm_lock);
    if (!s) return real_writei(pcm, buf, frames);

    const int sb = cm_sample_bytes(info.fmt);
    if (info.ch != 2 || sb == 0) {
        if (!said) mlog("jack: pcm %p fmt %d ch %d — not summable, left alone", pcm, info.fmt, info.ch);
        return real_writei(pcm, buf, frames);
    }
    const size_t bytes = (size_t)frames * 2u * (size_t)sb;
    uint8_t* out = scratch(bytes);
    if (!out) return real_writei(pcm, buf, frames);
    memcpy(out, buf, bytes);
    const int summed = cm_sum_buffer(out, frames, info.fmt, 2);
    if (!said) mlog("jack: pcm %p fmt %d — %s", pcm, info.fmt, summed ? "summing" : "DSD over PCM, left alone");
    const long r = real_writei(pcm, summed ? out : buf, frames);
    if (r > 0) load_proved_good();
    return r;
}

/* ── Bluetooth: the transmitter socket ───────────────────────────────────────────────────────── */
typedef struct { int fd; unsigned gen; cm_bt_stream st; int said; } bt_fd;
#define MAX_BT 8
static bt_fd g_bt[MAX_BT];
static int g_bt_count = 0;
static unsigned g_bt_gen = 0;
static pthread_mutex_t g_bt_lock = PTHREAD_MUTEX_INITIALIZER;

static int is_transmitter(const struct sockaddr* a, socklen_t len)
{
    if (!a || a->sa_family != AF_UNIX) return 0;
    const size_t off = offsetof(struct sockaddr_un, sun_path);
    if ((size_t)len <= off + 1) return 0;
    const struct sockaddr_un* u = (const struct sockaddr_un*)a;
    if (u->sun_path[0] != '\0') return 0;
    const size_t nl = sizeof kBtName - 1;
    if ((size_t)len - off - 1 < nl) return 0;
    /* Sony binds addrlen 110, NUL-padded — compare the name and require only padding after it. */
    if (memcmp(u->sun_path + 1, kBtName, nl) != 0) return 0;
    for (size_t i = off + 1 + nl; i < (size_t)len; i++)
        if (((const char*)a)[i] != '\0') return 0;
    return 1;
}

int connect(int fd, const struct sockaddr* addr, socklen_t len)
{
    RESOLVE(real_connect, "connect");
    const int r = real_connect ? real_connect(fd, addr, len) : (int)syscall(SYS_connect, fd, addr, len);
    if (g_active && (r == 0 || errno == EINPROGRESS) && is_transmitter(addr, len)) {
        const int e = errno;
        pthread_mutex_lock(&g_bt_lock);
        bt_fd* slot = NULL;
        for (int i = 0; i < MAX_BT; i++) if (g_bt[i].fd == fd && g_bt[i].gen) { slot = &g_bt[i]; break; }
        for (int i = 0; !slot && i < MAX_BT; i++) if (!g_bt[i].gen) { slot = &g_bt[i]; g_bt_count++; }
        if (slot) {
            memset(slot, 0, sizeof *slot);
            slot->fd = fd;
            slot->gen = ++g_bt_gen ? g_bt_gen : ++g_bt_gen;
        }
        pthread_mutex_unlock(&g_bt_lock);
        mlog(slot ? "bt: fd %d connected to the transmitter — watching for the handshake"
                  : "bt: fd %d connected but no slot free — left alone", fd);
        errno = e;
    }
    return r;
}

int close(int fd)
{
    RESOLVE(real_close, "close");
    if (g_active && g_bt_count) {
        pthread_mutex_lock(&g_bt_lock);
        for (int i = 0; i < MAX_BT; i++)
            if (g_bt[i].gen && g_bt[i].fd == fd) { g_bt[i].gen = 0; g_bt_count--; }
        pthread_mutex_unlock(&g_bt_lock);
    }
    return real_close ? real_close(fd) : (int)syscall(SYS_close, fd);
}

/* Look the fd up and take a copy of its state. 0 = not ours. */
static int bt_begin(int fd, cm_bt_stream* st, unsigned* gen, int* said)
{
    if (!g_active || !g_bt_count) return 0;
    int hit = 0;
    pthread_mutex_lock(&g_bt_lock);
    for (int i = 0; i < MAX_BT; i++)
        if (g_bt[i].gen && g_bt[i].fd == fd) { *st = g_bt[i].st; *gen = g_bt[i].gen; *said = g_bt[i].said; hit = 1; break; }
    pthread_mutex_unlock(&g_bt_lock);
    return hit;
}

/* Advance the real state by what the kernel took. */
static void bt_commit(int fd, unsigned gen, const uint8_t* in, size_t n_avail, size_t n_done, int mono, int said_now)
{
    pthread_mutex_lock(&g_bt_lock);
    for (int i = 0; i < MAX_BT; i++) {
        if (g_bt[i].gen != gen || g_bt[i].fd != fd) continue;
        const uint8_t before = g_bt[i].st.mode;
        cm_bt_walk(&g_bt[i].st, in, n_avail, n_done, NULL, mono);
        if (before != CM_BT_PCM && g_bt[i].st.mode == CM_BT_PCM)
            mlog("bt: fd %d handshake: %u ch, %u Hz — PCM from here", fd, g_bt[i].st.channels, g_bt[i].st.rate);
        if (g_bt[i].st.mode == CM_BT_GIVEUP && before != CM_BT_GIVEUP)
            mlog("bt: fd %d sent a frame this does not know — left alone for good", fd);
        if (said_now) g_bt[i].said = 1;
        break;
    }
    pthread_mutex_unlock(&g_bt_lock);
}

/* The shared body of write/send/sendto: build the bytes to send, send them, commit. */
static ssize_t bt_send(int fd, const void* buf, size_t n, ssize_t (*sendfn)(int, const void*, size_t, void*), void* ctx)
{
    cm_bt_stream st;
    unsigned gen = 0;
    int said = 1;
    if (!buf || n == 0 || !bt_begin(fd, &st, &gen, &said)) return sendfn(fd, buf, n, ctx);
    const int mono = mono_on();
    const uint8_t* in = (const uint8_t*)buf;
    const void* to_send = buf;
    int said_now = 0;
    if (mono && (st.mode == CM_BT_PCM || st.mode == CM_BT_FRAMES)) {
        uint8_t* out = scratch(n);
        if (out) {
            cm_bt_stream tmp = st;
            cm_bt_walk(&tmp, in, n, n, out, mono);
            to_send = out;
            if (!said && tmp.mode == CM_BT_PCM && tmp.channels == 2) {
                mlog("bt: fd %d — summing", fd);
                said_now = 1;
            }
        }
    }
    const ssize_t r = sendfn(fd, to_send, n, ctx);
    if (r > 0) {
        bt_commit(fd, gen, in, n, (size_t)r, to_send != buf ? mono : 0, said_now);
        if (st.mode == CM_BT_PCM) load_proved_good();
    }
    return r;
}

static ssize_t do_write(int fd, const void* b, size_t n, void* ctx)
{
    (void)ctx;
    return real_write ? real_write(fd, b, n) : (ssize_t)syscall(SYS_write, fd, b, n);
}
typedef struct { int flags; const struct sockaddr* to; socklen_t tolen; int is_sendto; } send_ctx;
static ssize_t do_send(int fd, const void* b, size_t n, void* ctx)
{
    const send_ctx* c = (const send_ctx*)ctx;
    if (c->is_sendto) {
        RESOLVE(real_sendto, "sendto");
        return real_sendto ? real_sendto(fd, b, n, c->flags, c->to, c->tolen)
                           : (ssize_t)syscall(SYS_sendto, fd, b, n, c->flags, c->to, c->tolen);
    }
    RESOLVE(real_send, "send");
    return real_send ? real_send(fd, b, n, c->flags)
                     : (ssize_t)syscall(SYS_sendto, fd, b, n, c->flags, NULL, 0);
}

ssize_t write(int fd, const void* buf, size_t n)
{
    if (!g_bt_count) return do_write(fd, buf, n, NULL);
    return bt_send(fd, buf, n, do_write, NULL);
}

ssize_t send(int fd, const void* buf, size_t n, int flags)
{
    send_ctx c = { flags, NULL, 0, 0 };
    if (!g_bt_count) return do_send(fd, buf, n, &c);
    return bt_send(fd, buf, n, do_send, &c);
}

ssize_t sendto(int fd, const void* buf, size_t n, int flags, const struct sockaddr* to, socklen_t tolen)
{
    send_ctx c = { flags, to, tolen, 1 };
    if (!g_bt_count || to) return do_send(fd, buf, n, &c);
    return bt_send(fd, buf, n, do_send, &c);
}

/* writev/sendmsg on a watched fd are not summed, but their bytes still have to be counted, or the
 * frame alignment would drift and the next summed write would pair the wrong samples. */
static void bt_observe_iov(int fd, const struct iovec* iov, int cnt, ssize_t done)
{
    cm_bt_stream st;
    unsigned gen = 0;
    int said = 0;
    if (done <= 0 || !bt_begin(fd, &st, &gen, &said)) return;
    size_t left = (size_t)done;
    for (int i = 0; i < cnt && left; i++) {
        const size_t take = iov[i].iov_len < left ? iov[i].iov_len : left;
        bt_commit(fd, gen, (const uint8_t*)iov[i].iov_base, iov[i].iov_len, take, 0, 0);
        left -= take;
    }
}

ssize_t writev(int fd, const struct iovec* iov, int cnt)
{
    RESOLVE(real_writev, "writev");
    const ssize_t r = real_writev ? real_writev(fd, iov, cnt) : (ssize_t)syscall(SYS_writev, fd, iov, cnt);
    if (g_bt_count) bt_observe_iov(fd, iov, cnt, r);
    return r;
}

ssize_t sendmsg(int fd, const struct msghdr* msg, int flags)
{
    RESOLVE(real_sendmsg, "sendmsg");
    const ssize_t r = real_sendmsg ? real_sendmsg(fd, msg, flags) : (ssize_t)syscall(SYS_sendmsg, fd, msg, flags);
    if (g_bt_count && msg) bt_observe_iov(fd, msg->msg_iov, (int)msg->msg_iovlen, r);
    return r;
}
