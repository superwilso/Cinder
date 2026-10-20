/* cinder_aac.c — libcinder_aac.so: AAC Bluetooth transmit for the NW-A50 series.
 *
 * Preloaded into the hagodaemon that hosts BtCommonService/BtTransmitterService. Notes on the
 * stack this talks to are in docs/STACK.md; in short, three layers share the work:
 *
 *   libBtMw.so (Sony)        picks the encoder from the negotiated codec, feeds it PCM
 *   libbluetooth.blueangel   builds the endpoint list, encodes, sends frames to mtkbt
 *   mtkbt (separate process) AVDTP: advertises the endpoints, negotiates, sends packets
 *
 * mtkbt already knows AAC as a *source* codec (it converts AAC capabilities and ranks AAC after
 * LDAC and aptX when choosing), so it needs no change. What is missing is above it:
 *
 *   1. No AAC source endpoint is ever registered. The list has room for four, all taken (SBC,
 *      LDAC, aptX HD, aptX). AAC and aptX HD take turns in aptX HD's slot: the list is built
 *      afresh each time Bluetooth starts, and AAC goes in unless /data/cinder/bt_aptxhd (Cinder:
 *      "aptX HD is the choice") says to leave aptX HD there.    -> sep_default_setting_initialized
 *   2. mtkbt only picks AAC while its AAC switch is on. While AAC holds the slot, Sony's aptX HD
 *      switch drives it instead (AAC has no transmit switch of its own), so Cinder's codec choice
 *      reaches AAC through SetAptxHD.           -> send_aptxHD_enable_req, send_aac_enable_req
 *   3. libBtMw forwards PCM only for SBC, LDAC and aptX; for any other codec it drops it. So when
 *      the link comes up AAC, libBtMw is told it is SBC at the same rate, and its SBC calls (encoder
 *      start/stop, PCM) land here and are answered with AAC.     -> bt_av_callbacks' codec config
 *   4. The AAC frames go to mtkbt the way SBC's do.              -> send_audio_encoded_stream_data
 *
 * Every hook here is a function blueangel calls through its own PLT, so a preload stands in
 * front of it; each forwards to blueangel's own copy whenever AAC is not in play.
 *
 * Off unless /data/cinder/bt_aac exists when the service starts. With the file absent every hook
 * is a straight pass-through. /tmp/cinder_aac_codecs says what this boot offers ("aac", "sbc-xq",
 * one a line) and, once Bluetooth has started, what holds the shared slot ("slot aac" or
 * "slot aptxhd"), for Cinder's Bluetooth screen.
 *
 * SBC XQ, separately, with /data/cinder/bt_sbc_xq: the SBC endpoint offers Dual Channel only, at
 * up to bitpool 47. mtkbt takes the first channel mode both sides share (joint stereo first), so
 * offering only Dual Channel is what makes it pick it, and in Dual Channel the bitpool counts per
 * channel: about 550 kbit/s at 44.1 kHz where joint stereo at 53 gives 328. Every A2DP sink must
 * accept Dual Channel; one that does not would fail to connect over SBC, so it stays opt-in.
 */
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#include <dlfcn.h>
#include <pthread.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

#include "aac_enc.h"

#ifndef CINDER_AAC_DIR
#define CINDER_AAC_DIR "/data/cinder"
#endif
static const char kOn[] = CINDER_AAC_DIR "/bt_aac";
static const char kXq[] = CINDER_AAC_DIR "/bt_sbc_xq";
static const char kHd[] = CINDER_AAC_DIR "/bt_aptxhd";
static const char kLog[] = "/tmp/cinder_aac.log";
static const char kStatus[] = "/tmp/cinder_aac_status";
static const char kCodecs[] = "/tmp/cinder_aac_codecs";

#define EXPORT __attribute__((visibility("default")))

/* Endpoint list layout (blueangel's own, one 0x14-byte entry per endpoint). */
enum { SEP_SIZE = 0x14, SEP_SLOT_APTX_HD = 2, CODEC_SBC = 1, CODEC_AAC = 3 };
/* AAC entry fields, as mtkbt reads them when it turns the entry into AVDTP capabilities. */
enum { AAC_OBJ = 4, AAC_FREQ = 6, AAC_CHAN = 8, AAC_VBR = 9, AAC_RATE = 0xc };
enum { AAC_OBJ_MPEG2_LC = 0x80, AAC_F44 = 0x10, AAC_F48 = 0x08, AAC_STEREO = 1 };
enum { AAC_MAX_BITRATE = 320000 };
/* SBC entry fields: bitpool range, then channel modes (joint 1, stereo 2, dual 4, mono 8). */
enum { SEP_SLOT_SBC = 0, SBC_MIN_BP = 4, SBC_MAX_BP = 5, SBC_BLOCKS = 6, SBC_MODE = 10 };
enum { SBC_DUAL = 4, SBC_XQ_BITPOOL = 47 };

/* ---- log: one file, throttled at the print ------------------------------------------------ */

static void logf_(const char* fmt, ...)
{
    FILE* f = fopen(kLog, "a");
    if (!f) return;
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    fprintf(f, "%ld.%03ld ", (long)ts.tv_sec, ts.tv_nsec / 1000000);
    va_list ap;
    va_start(ap, fmt);
    vfprintf(f, fmt, ap);
    va_end(ap);
    fputc('\n', f);
    fclose(f);
}

/* Logs the first few of a repeating event, then every 1000th. */
#define LOG_RARE(counter, ...)                                                        \
    do {                                                                              \
        unsigned long n_ = ++(counter);                                               \
        if (n_ <= 5 || n_ % 1000 == 0) logf_(__VA_ARGS__);                            \
    } while (0)

/* ---- blueangel's and libBtMw's own functions ----------------------------------------------- */

typedef void (*sep_init_fn)(uint8_t*, int, char);
typedef void (*activate_fn)(int, int);
typedef void (*aac_enable_fn)(uint8_t);  /* also the aptX HD switch's shape */
typedef int (*enc_init_fn)(int, int, int, int, int, int, int, int);
typedef void (*enc_deinit_fn)(int);
typedef void (*send_pcm_fn)(void*, unsigned);
typedef void (*send_frame_fn)(const void*, size_t, uint32_t);
typedef void (*cfg_cb_fn)(void*, uint8_t*);
typedef int (*is_src_fn)(void);

static struct {
    sep_init_fn sep_init;
    activate_fn activate;
    aac_enable_fn aac_enable;
    aac_enable_fn aptxhd_enable;
    enc_init_fn enc_init;
    enc_deinit_fn enc_deinit;
    send_pcm_fn send_pcm;
    send_frame_fn send_frame;
    void** callbacks;        /* blueangel's bt_av_callbacks: the table libBtMw registered */
    uint8_t* app;            /* blueangel's bt_a2dp_app (a struct): +0x70 is the media payload limit */
    is_src_fn is_src;        /* libBtMw: the A2DP side is in source (transmit) mode */
} real;

static pthread_once_t g_once = PTHREAD_ONCE_INIT;
static int g_on;             /* AAC wanted and every symbol found */
static int g_xq;             /* SBC XQ wanted */
static int g_slot;           /* what holds aptX HD's slot: 0 not built yet, 'a' AAC, 'h' aptX HD */

static void publish(int aac, int xq)
{
    FILE* f = fopen(kCodecs, "w");
    if (!f) return;
    fchmod(fileno(f), 0644);   /* the service's umask is 077 */
    if (aac) fputs("aac\n", f);
    if (xq) fputs("sbc-xq\n", f);
    if (aac && g_slot) fputs(g_slot == 'a' ? "slot aac\n" : "slot aptxhd\n", f);
    fclose(f);
}

/* From the switches at load, so the screen is right before Bluetooth first starts; resolve()
 * corrects it once the symbols are checked. */
__attribute__((constructor)) static void at_load(void)
{
    publish(access(kOn, F_OK) == 0, access(kXq, F_OK) == 0);
}

static void* sym(void* lib, const char* name)
{
    void* p = lib ? dlsym(lib, name) : NULL;
    if (!p) logf_("missing %s", name);
    return p;
}

static void resolve(void)
{
    /* Both are loaded by the time anything calls in: the calls come from blueangel, which
     * libBtMw needs. dlsym on blueangel's own handle finds its definitions, not ours. */
    void* ba = dlopen("libbluetooth.blueangel.so", RTLD_NOW | RTLD_NOLOAD);
    void* mw = dlopen("libBtMw.so", RTLD_NOW | RTLD_NOLOAD);
    real.sep_init = (sep_init_fn)sym(ba, "btmtk_a2dp_sep_default_setting_initialized");
    real.activate = (activate_fn)sym(ba, "btmtk_a2dp_send_activate_req");
    real.aac_enable = (aac_enable_fn)sym(ba, "btmtk_a2dp_send_aac_enable_req");
    real.aptxhd_enable = (aac_enable_fn)sym(ba, "btmtk_a2dp_send_aptxHD_enable_req");
    real.enc_init = (enc_init_fn)sym(ba, "btmtk_a2dp_stream_data_encoder_init");
    real.enc_deinit = (enc_deinit_fn)sym(ba, "btmtk_a2dp_stream_data_encoder_deinit");
    real.send_pcm = (send_pcm_fn)sym(ba, "btmtk_a2dp_send_audio_stream_data");
    real.send_frame = (send_frame_fn)sym(ba, "btmtk_a2dp_send_audio_encoded_stream_data");
    real.callbacks = (void**)sym(ba, "bt_av_callbacks");
    real.app = (uint8_t*)sym(ba, "bt_a2dp_app");
    real.is_src = (is_src_fn)sym(mw, "btmwAvBothIsSrc");
    int all = real.sep_init && real.activate && real.aac_enable && real.aptxhd_enable && real.enc_init &&
              real.enc_deinit && real.send_pcm && real.send_frame && real.callbacks && real.app &&
              real.is_src;
    g_on = all && access(kOn, F_OK) == 0;
    g_xq = all && access(kXq, F_OK) == 0;
    logf_("loaded: symbols %s, AAC %s, SBC XQ %s", all ? "ok" : "MISSING", g_on ? "on" : "off",
          g_xq ? "on" : "off");
    publish(g_on, g_xq);
}

static void init(void) { pthread_once(&g_once, resolve); }

/* ---- 1. the AAC endpoint ------------------------------------------------------------------- */

EXPORT void btmtk_a2dp_sep_default_setting_initialized(uint8_t* seps, int n, char sink)
{
    init();
    if (real.sep_init) real.sep_init(seps, n, sink);
    if (sink) return;
    uint8_t* sbc = seps + SEP_SLOT_SBC * SEP_SIZE;
    if (g_xq && n > SEP_SLOT_SBC && sbc[0] == CODEC_SBC) {
        logf_("SBC endpoint was: bitpool %d..%d, modes 0x%x", sbc[SBC_MIN_BP], sbc[SBC_MAX_BP],
              sbc[SBC_MODE]);
        sbc[SBC_MODE] = SBC_DUAL;
        sbc[SBC_MAX_BP] = SBC_XQ_BITPOOL;
        logf_("SBC endpoint is now XQ: Dual Channel, bitpool %d..%d", sbc[SBC_MIN_BP], SBC_XQ_BITPOOL);
    }
    if (!g_on || n <= SEP_SLOT_APTX_HD) return;
    g_slot = access(kHd, F_OK) == 0 ? 'h' : 'a';
    publish(g_on, g_xq);
    if (g_slot == 'h') {
        logf_("source endpoint %d stays aptX HD (aptX HD is the choice)", SEP_SLOT_APTX_HD);
        return;
    }
    uint8_t* e = seps + SEP_SLOT_APTX_HD * SEP_SIZE;
    memset(e, 0, SEP_SIZE);
    e[0] = CODEC_AAC;
    e[AAC_OBJ] = AAC_OBJ_MPEG2_LC;
    uint16_t freq = AAC_F44 | AAC_F48;
    memcpy(e + AAC_FREQ, &freq, 2);
    e[AAC_CHAN] = AAC_STEREO;
    e[AAC_VBR] = 0;
    uint32_t rate = AAC_MAX_BITRATE;
    memcpy(e + AAC_RATE, &rate, 4);
    logf_("source endpoint %d is now AAC (was aptX HD)", SEP_SLOT_APTX_HD);
}

/* ---- 2. mtkbt's AAC switch ----------------------------------------------------------------- */

static int g_aac_tx = 1;     /* transmit AAC wanted: Sony's aptX HD switch, last set */
static uint8_t g_aac_rx = 1; /* the receiver's AAC setting, last set */

/* Transmitting, the switch follows the codec choice; receiving, the receiver's own setting. */
static void aac_switch(void)
{
    real.aac_enable(real.is_src() && g_slot == 'a' ? (uint8_t)g_aac_tx : g_aac_rx);
}

EXPORT void btmtk_a2dp_send_aac_enable_req(uint8_t on)
{
    init();
    if (!g_on) {
        if (real.aac_enable) real.aac_enable(on);
        return;
    }
    g_aac_rx = on;
    aac_switch();
}

EXPORT void btmtk_a2dp_send_aptxHD_enable_req(uint8_t on)
{
    init();
    if (real.aptxhd_enable) real.aptxhd_enable(on);
    if (!g_on) return;
    g_aac_tx = on != 0;
    if (g_slot == 'a') logf_("transmit AAC %s (Sony's aptX HD switch)", on ? "on" : "off");
    aac_switch();
}

static void swap_callbacks(void);

EXPORT void btmtk_a2dp_send_activate_req(int a, int b)
{
    init();
    if (real.activate) real.activate(a, b);
    if (g_on) aac_switch();
    if (g_on || g_xq) swap_callbacks();
}

/* ---- 3. what libBtMw is told -------------------------------------------------------------- */

static struct {
    pthread_mutex_t lock;
    int cfg_rate;            /* the negotiated AAC link's rate, 0 when the link is not AAC */
    uint32_t cfg_max_rate;   /* the headphones' bitrate ceiling, 0 = none given */
    struct aac_enc* enc;
    int payload;
    int16_t pcm[AAC_FRAME_SAMPLES * 2];
    int fill;                /* bytes in pcm */
    uint8_t out[2048];
    struct timespec t0;      /* pacing: when frame 0 was due */
    unsigned long frames, dropped, failed, waits;
} g = {.lock = PTHREAD_MUTEX_INITIALIZER};

static void* g_cb_copy[16];  /* libBtMw's table is 0x40 bytes: 16 slots */
static cfg_cb_fn g_cfg_cb;   /* libBtMw's own codec-config callback (slot 4) */

static void status(const char* s)
{
    FILE* f = fopen(kStatus, "w");
    if (!f) return;
    fchmod(fileno(f), 0644);
    fputs(s, f);
    fclose(f);
}

static void on_codec_config(void* addr, uint8_t* cfg)
{
    if (cfg && cfg[0] == CODEC_AAC && real.is_src()) {
        uint16_t freq;
        uint32_t max;
        memcpy(&freq, cfg + AAC_FREQ, 2);
        memcpy(&max, cfg + AAC_RATE, 4);
        int rate = (freq & AAC_F48) ? 48000 : (freq & AAC_F44) ? 44100 : 0;
        pthread_mutex_lock(&g.lock);
        g.cfg_rate = rate;
        g.cfg_max_rate = max;
        pthread_mutex_unlock(&g.lock);
        logf_("link is AAC: freq 0x%x -> %d Hz, max %u bit/s, channels 0x%x", freq, rate, max,
              cfg[AAC_CHAN]);
        char s[32];
        snprintf(s, sizeof s, "AAC %d\n", rate);
        status(s);
        if (rate) {
            /* The same link as SBC would describe it, so libBtMw starts "SBC" at the right rate:
             * bitpool 2..53, 16 blocks, 8 subbands, loudness, joint stereo. */
            uint8_t sbc[SEP_SIZE] = {CODEC_SBC, 0, 0, 0, 2, 53, 1, 1, 1, rate == 48000 ? 1 : 2, 1};
            g_cfg_cb(addr, sbc);
            return;
        }
    } else if (cfg) {
        if (cfg[0] == CODEC_SBC)
            logf_("link is SBC: bitpool %d..%d, blocks/subbands/alloc %d/%d/%d, freq %d, mode %d",
                  cfg[SBC_MIN_BP], cfg[SBC_MAX_BP], cfg[SBC_BLOCKS], cfg[SBC_BLOCKS + 1],
                  cfg[SBC_BLOCKS + 2], cfg[SBC_BLOCKS + 3], cfg[SBC_MODE]);
        pthread_mutex_lock(&g.lock);
        g.cfg_rate = 0;
        pthread_mutex_unlock(&g.lock);
        status("");
    }
    g_cfg_cb(addr, cfg);
}

/* Puts on_codec_config in front of libBtMw's codec-config callback. Re-run on every activation:
 * libBtMw registers its table again when Bluetooth is switched on. */
static void swap_callbacks(void)
{
    void* cur = *real.callbacks;
    if (!cur || cur == g_cb_copy) return;
    memcpy(g_cb_copy, cur, sizeof g_cb_copy);
    g_cfg_cb = (cfg_cb_fn)g_cb_copy[4];
    if (!g_cfg_cb) return;
    g_cb_copy[4] = (void*)on_codec_config;
    *real.callbacks = g_cb_copy;
    logf_("codec-config callback wrapped");
}

/* ---- libBtMw's "SBC" calls, answered with AAC ---------------------------------------------- */

EXPORT int btmtk_a2dp_stream_data_encoder_init(int codec, int a, int b, int c, int d, int rate,
                                               int mode, int mtu)
{
    init();
    pthread_mutex_lock(&g.lock);
    int want = g_on && codec == CODEC_SBC && g.cfg_rate;
    if (want && !g.enc) {
        int lim = *(uint16_t*)(real.app + 0x70);
        g.payload = (mtu > 0 && mtu < lim) ? mtu : lim;
        if (g.payload > (int)sizeof g.out) g.payload = sizeof g.out;
        int cap = AAC_MAX_BITRATE;
        /* A ceiling below 64 kbit/s is not one any AAC headphone means; read it as none. */
        if (g.cfg_max_rate >= 64000 && (int)g.cfg_max_rate < cap) cap = (int)g.cfg_max_rate;
        int br = aac_bitrate_for(g.payload, g.cfg_rate, cap);
        const char* why = "";
        g.enc = aac_enc_open(g.cfg_rate, br, AAC_LATM, &why);
        g.fill = 0;
        g.frames = g.dropped = g.failed = g.waits = 0;
        g.t0.tv_sec = 0;
        logf_("encoder start: %d Hz, %d bit/s, payload %d (mtu arg %d, limit %d): %s (%s, peak cap %d)",
              g.cfg_rate, br, g.payload, mtu, lim, g.enc ? "ok" : why,
              aac_enc_lib ? aac_enc_lib : "-", aac_enc_peak_capped);
        char s[96];
        snprintf(s, sizeof s, "AAC %d %d\n", g.cfg_rate, br);
        status(g.enc ? s : "AAC failed\n");
    }
    int ours = want && g.enc;
    pthread_mutex_unlock(&g.lock);
    if (ours) return 0;
    if (g_xq && codec == CODEC_SBC)
        logf_("SBC encoder start: args %d %d %d %d, rate %d, mode %d, mtu %d", a, b, c, d, rate,
              mode, mtu);
    return real.enc_init ? real.enc_init(codec, a, b, c, d, rate, mode, mtu) : -1;
}

EXPORT void btmtk_a2dp_stream_data_encoder_deinit(int x)
{
    init();
    pthread_mutex_lock(&g.lock);
    int ours = g.enc != NULL;
    if (ours) {
        logf_("encoder stop: %lu frames, %lu too big, %lu failed, %lu waits, ended at %d bit/s",
              g.frames, g.dropped, g.failed, g.waits, aac_enc_bitrate(g.enc));
        aac_enc_close(g.enc);
        g.enc = NULL;
        char s[32];
        snprintf(s, sizeof s, g.cfg_rate ? "AAC %d\n" : "", g.cfg_rate);
        status(s);
    }
    pthread_mutex_unlock(&g.lock);
    if (!ours && real.enc_deinit) real.enc_deinit(x);
}

static long ms_since(const struct timespec* a, const struct timespec* b)
{
    return (b->tv_sec - a->tv_sec) * 1000 + (b->tv_nsec - a->tv_nsec) / 1000000;
}

/* Hands frames to mtkbt no faster than real time, a little ahead. SBC's sender does the same; the
 * PCM arrives as fast as the socket is read, so without this mtkbt's queue overflows. */
static void pace(void)
{
    enum { LEAD_MS = 60, RESET_MS = 300 };
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    if (!g.t0.tv_sec) { g.t0 = now; return; }
    long due = (long)((long long)g.frames * AAC_FRAME_SAMPLES * 1000 / g.cfg_rate);
    long ahead = due - ms_since(&g.t0, &now);
    if (ahead < -RESET_MS) {
        /* Fell behind (a pause, an underrun): restart the clock instead of bursting. */
        g.t0 = now;
        g.frames = 0;
    } else if (ahead > LEAD_MS) {
        g.waits++;
        usleep((useconds_t)(ahead - LEAD_MS) * 1000);
    }
}

EXPORT void btmtk_a2dp_send_audio_stream_data(void* data, unsigned len)
{
    init();
    pthread_mutex_lock(&g.lock);
    if (!g.enc) {
        pthread_mutex_unlock(&g.lock);
        if (real.send_pcm) real.send_pcm(data, len);
        return;
    }
    const uint8_t* p = data;
    len &= 0xffff;
    while (len) {
        unsigned take = sizeof g.pcm - (unsigned)g.fill;
        if (take > len) take = len;
        memcpy((uint8_t*)g.pcm + g.fill, p, take);
        g.fill += (int)take;
        p += take;
        len -= take;
        if (g.fill < (int)sizeof g.pcm) break;
        g.fill = 0;
        /* One byte of the payload may go to SCMS-T's content-protection header. */
        int n = aac_enc_fit(g.enc, g.pcm, g.out, sizeof g.out, g.payload - 1);
        if (n == -2) {
            LOG_RARE(g.dropped, "frame too big for payload %d: now %d bit/s", g.payload,
                     aac_enc_bitrate(g.enc));
            continue;
        }
        if (n < 0) { LOG_RARE(g.failed, "encode failed"); continue; }
        if (n == 0) continue;
        pace();
        g.frames++;
        /* mtkbt adds the last argument to the RTP timestamp after each packet: samples. */
        real.send_frame(g.out, (size_t)n, AAC_FRAME_SAMPLES);
    }
    pthread_mutex_unlock(&g.lock);
}
