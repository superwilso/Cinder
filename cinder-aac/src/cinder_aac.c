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
#include <fcntl.h>
#include <pthread.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

#include "aac_enc.h"
#include "pcm_rate.h"
#include "sbc_abr.h"

#ifndef CINDER_AAC_DIR
#define CINDER_AAC_DIR "/data/cinder"
#endif
static const char kOn[] = CINDER_AAC_DIR "/bt_aac";
static const char kXq[] = CINDER_AAC_DIR "/bt_sbc_xq";
static const char kHd[] = CINDER_AAC_DIR "/bt_aptxhd";
static const char kStd[] = CINDER_AAC_DIR "/bt_sbc_std";   /* plain SBC chosen: hold XQ back */
static const char kLog[] = "/tmp/cinder_aac.log";
static const char kStatus[] = "/tmp/cinder_aac_status";
static const char kCodecs[] = "/tmp/cinder_aac_codecs";
/* Measurement for an SBC XQ Auto (analysis/RE_bt_send_timing.md): while this file exists, an SBC
 * stream asks mtkbt for its transmit-buffer status every 100 ms, as LDAC's Best effort does, and
 * logs what comes back once a second. Nothing is changed in what is sent. */
static const char kProbe[] = "/tmp/cinder_sbc_probe";

#define EXPORT __attribute__((visibility("default")))

/* Endpoint list layout (blueangel's own, one 0x14-byte entry per endpoint). */
enum { SEP_SIZE = 0x14, SEP_SLOT_APTX_HD = 2, CODEC_SBC = 1, CODEC_AAC = 3 };
/* AAC entry fields, as mtkbt reads them when it turns the entry into AVDTP capabilities. */
enum { AAC_OBJ = 4, AAC_FREQ = 6, AAC_CHAN = 8, AAC_VBR = 9, AAC_RATE = 0xc };
enum { AAC_OBJ_MPEG2_LC = 0x80, AAC_F44 = 0x10, AAC_F48 = 0x08, AAC_STEREO = 1 };
enum { AAC_MAX_BITRATE = 320000 };
/* SBC entry fields: bitpool range, then channel modes (joint 1, stereo 2, dual 4, mono 8). */
enum { SEP_SLOT_SBC = 0, SBC_MIN_BP = 4, SBC_MAX_BP = 5, SBC_BLOCKS = 6, SBC_MODE = 10 };
enum { SBC_DUAL = 4, SBC_XQ_BITPOOL = 47, SBC_XQ_MIN_BITPOOL = 2 };

/* ---- log: one file, throttled at the print ------------------------------------------------ */

static void logf_(const char* fmt, ...)
{
    /* Readable and writable by everyone: the service starts as root and carries on as `system`,
     * root's umask here is 077, and cinder-home (uid 100) copies these lines into its own log. */
    int fd = open(kLog, O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC, 0666);
    if (fd < 0) return;
    if (geteuid() == 0) fchmod(fd, 0666);
    FILE* f = fdopen(fd, "a");
    if (!f) { close(fd); return; }
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
typedef void (*req_status_fn)(void);
typedef void (*abr_proc_fn)(int);
typedef void (*sbc_set_fn)(void*, int, int, int, int, unsigned char);

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
    is_src_fn is_snk;        /* libBtMw: in sink (receiver) mode; neither before a role is set */
    req_status_fn req_status;  /* blueangel: ask mtkbt for the AVDTP buffer status (msg 0x21d) */
    abr_proc_fn abr_proc;      /* blueangel: where the answer goes, LDAC's ABR */
    sbc_set_fn sbc_set;        /* libsbc_enc: the encoder's settings, bitpool included */
} real;

static pthread_once_t g_once = PTHREAD_ONCE_INIT;
static int g_on;             /* AAC wanted and every symbol found */
static int g_xq;             /* SBC XQ wanted */
static int g_slot;           /* what holds aptX HD's slot: 0 not built yet, 'a' AAC, 'h' aptX HD */
static int g_sbc;            /* this start's SBC endpoint, with g_xq: 0 not built yet, 'x' XQ, 's' plain */

static void publish(int aac, int xq)
{
    FILE* f = fopen(kCodecs, "w");
    if (!f) return;
    fchmod(fileno(f), 0644);   /* the service's umask is 077 */
    if (aac) fputs("aac\n", f);
    if (xq) fputs("sbc-xq\n", f);
    if (aac && g_slot) fputs(g_slot == 'a' ? "slot aac\n" : "slot aptxhd\n", f);
    if (xq && g_sbc) fputs(g_sbc == 'x' ? "sbc now xq\n" : "sbc now std\n", f);
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
    real.is_snk = (is_src_fn)sym(mw, "btmwAvBothIsSnk");
    real.req_status = (req_status_fn)sym(ba, "btmtk_a2dp_request_avdtp_buffer_status");
    real.abr_proc = (abr_proc_fn)sym(ba, "btmtk_a2dp_ldac_abr_proc");
    real.sbc_set = (sbc_set_fn)sym(dlopen("libsbc_enc.so", RTLD_NOW | RTLD_NOLOAD), "sbc_set");
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
    if (g_xq && access(kStd, F_OK) == 0) {
        /* #44: some headphones' radios cannot carry XQ's rate; the stack drops packets and the
         * music plays fast. Plain SBC is then the choice, and Sony's endpoint stays as it is. */
        g_sbc = 's';
        logf_("SBC endpoint stays Sony's (plain SBC is the choice)");
    } else if (g_xq && n > SEP_SLOT_SBC && sbc[0] == CODEC_SBC) {
        g_sbc = 'x';
        logf_("SBC endpoint was: bitpool %d..%d, modes 0x%x", sbc[SBC_MIN_BP], sbc[SBC_MAX_BP],
              sbc[SBC_MODE]);
        sbc[SBC_MODE] = SBC_DUAL;
        sbc[SBC_MAX_BP] = SBC_XQ_BITPOOL;
        /* Sony's minimum of 35 suits Joint Stereo, where headphones go to 53. In Dual Channel many
         * stop lower (CMF Buds Pro 2: 31 on one connect, 39 on another), and the link came up as
         * "bitpool 35..31": a range the A2DP rules do not allow, which that pair let through and a
         * stricter one may refuse. The encoder runs at the top of the range, so the floor costs
         * nothing. 2 is the spec's. */
        sbc[SBC_MIN_BP] = SBC_XQ_MIN_BITPOOL;
        logf_("SBC endpoint is now XQ: Dual Channel, bitpool %d..%d", sbc[SBC_MIN_BP], SBC_XQ_BITPOOL);
    }
    publish(g_on, g_xq);
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

/* Receiving, the receiver's own setting; otherwise the codec choice.
 *
 * "Otherwise" includes NO ROLE YET. At boot btmwAvBothIsSrc is 0 as well as btmwAvBothIsSnk (the
 * A2DP state is neither 3/6 nor 2/5), and asking "are we a source?" sent the receiver's value
 * (on) at every boot: the choice was never applied, and a headphone reconnecting on its own came up
 * AAC with SBC XQ chosen (2026-11-02, CMF Buds Pro 2; the log said "transmit AAC off"). Transmit is
 * the default role, so only a sink, or Cinder's receiver mode, gets the receiver's value. */
static const char kRx[] = "/tmp/cinder_bt_rx";   /* Cinder's receiver mode is on */

static void aac_switch(void)
{
    /* The stack is a sink, or Cinder said its receiver is on (before a phone connects the stack
     * reports neither role). */
    const int snk = (real.is_snk ? real.is_snk() : !real.is_src()) || access(kRx, F_OK) == 0;
    const uint8_t v = !snk && g_slot == 'a' ? (uint8_t)g_aac_tx : g_aac_rx;
    /* What is SENT, not what was asked for. */
    logf_("mtkbt AAC switch -> %s (%s, slot %c, transmit wanted %d, receiver %d)", v ? "on" : "off",
          snk ? "receiving" : "transmitting", g_slot ? g_slot : '-', g_aac_tx, g_aac_rx);
    real.aac_enable(v);
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
    logf_("activate(%d, %d): source %d, sink %d", a, b, real.is_src(), real.is_snk ? real.is_snk() : -1);
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
    unsigned long bursts;    /* times it fell behind and sent back to back to catch up (#44) */
    long worst_ms;           /* the furthest behind it has been, in ms */
    int behind;              /* behind on the last frame: a burst is under way */
    unsigned long resets;    /* times it fell so far behind that the clock restarted */
    struct timespec last_in; /* when the last PCM had been handled */
    long gap_ms;             /* the wait for the latest PCM: a long one = the source stalled */
    long overslept_ms;       /* how much longer the last pace sleep took than asked: = we stalled */
    pcm_rate mon;            /* every codec's PCM, timed against its rate (#44) */
    int mon_on, mon_codec;   /* a stream is running; its codec (CODEC_AAC when ours) */
    unsigned long mon_stalls_said, mon_off_said;
    /* The SBC buffer-status probe (kProbe): on for this stream, when it last asked, and this
     * second's answers. */
    int probe;
    long long probe_asked_ms, probe_win_ms;
    unsigned long probe_asks, probe_answers;
    int q_min, q_max, q_n;
    long q_sum;
    /* SBC XQ Auto (sbc_abr.h): on for an XQ stream; the encoder's sbc_set arguments, captured at
     * its start, so the bitpool can be set again with the rest unchanged; a change waiting for the
     * sending thread. */
    int abr_on, link_min;
    sbc_abr abr;
    struct {
        void* state;
        int alloc, subbands, blocks, bitpool;
        unsigned char mode;
        int have;
    } set;
    int abr_pending;
    unsigned long abr_changes;
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
        if (cfg[0] == CODEC_SBC) g.link_min = cfg[SBC_MIN_BP];
        if (cfg[0] == CODEC_SBC)
            logf_("link is SBC: bitpool %d..%d, blocks/subbands/alloc %d/%d/%d, freq %d, mode %d",
                  cfg[SBC_MIN_BP], cfg[SBC_MAX_BP], cfg[SBC_BLOCKS], cfg[SBC_BLOCKS + 1],
                  cfg[SBC_BLOCKS + 2], cfg[SBC_BLOCKS + 3], cfg[SBC_MODE]);
        else
            logf_("link is codec 0x%02x%s", cfg[0],
                  cfg[0] == CODEC_AAC ? " (AAC, not transmitting)" : "");
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

static long long ms_of(const struct timespec* t)
{
    return (long long)t->tv_sec * 1000 + t->tv_nsec / 1000000;
}

/* Times one PCM call against the stream's rate (pcm_rate.h). Called with g.lock held. A wait of
 * two seconds or more is a pause, not a stall: the measurement starts again after it. */
static void watch_pcm(unsigned n)
{
    if (!g.mon_on) return;
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    const long long now = ms_of(&ts);
    if (g.mon.bytes && now - g.mon.last_ms >= 2000) {
        pcm_rate keep = g.mon;
        pcm_rate_start(&g.mon, keep.expect_hz, keep.frame_bytes, now);
        g.mon.t0_ms = keep.t0_ms;               /* the summary still covers the whole stream */
        g.mon.bytes = keep.bytes;
        g.mon.stalls = keep.stalls;
        g.mon.worst_gap_ms = keep.worst_gap_ms;
        g.mon.off_windows = keep.off_windows;
    }
    long gap;
    const int r = pcm_rate_feed(&g.mon, n, now, &gap);
    if (r & PCM_RATE_STALL)
        LOG_RARE(g.mon_stalls_said, "PCM waited %ld ms (codec %d): the sound service fell behind, "
                 "not the radio", gap, g.mon_codec);
    if ((r & PCM_RATE_OFF) && g.enc)
        LOG_RARE(g.mon_off_said, "AAC: PCM arrives at %ld Hz but the encoder runs at %d Hz: "
                 "this plays at the wrong speed", g.mon.last_hz, g.mon.expect_hz);
    else if (r & PCM_RATE_OFF)
        LOG_RARE(g.mon_off_said, "PCM arrives at %ld Hz, %d expected (codec %d)", g.mon.last_hz,
                 g.mon.expect_hz, g.mon_codec);
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
        g.bursts = 0;
        g.worst_ms = 0;
        g.behind = 0;
        g.resets = 0;
        g.last_in.tv_sec = 0;
        g.gap_ms = g.overslept_ms = 0;
        g.t0.tv_sec = 0;
        logf_("encoder start: %d Hz, %d bit/s, payload %d (mtu arg %d, limit %d): %s (%s, peak cap %d)",
              g.cfg_rate, br, g.payload, mtu, lim, g.enc ? "ok" : why,
              aac_enc_lib ? aac_enc_lib : "-", aac_enc_peak_capped);
        char s[96];
        snprintf(s, sizeof s, "AAC %d %d\n", g.cfg_rate, br);
        status(g.enc ? s : "AAC failed\n");
    }
    int ours = want && g.enc;
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    pcm_rate_start(&g.mon, ours ? g.cfg_rate : rate, 4, ms_of(&now));
    g.mon_on = 1;
    g.mon_codec = ours ? CODEC_AAC : codec;
    g.probe = !ours && codec == CODEC_SBC && real.req_status && access(kProbe, F_OK) == 0;
    g.probe_asked_ms = g.probe_win_ms = 0;
    g.probe_asks = g.probe_answers = 0;
    g.q_n = 0;
    if (g.probe) logf_("sbc-probe: on for this stream (bitpool %d, mode %d)", a, mode);
    g.mon_stalls_said = g.mon_off_said = 0;
    pthread_mutex_unlock(&g.lock);
    if (ours) return 0;
    /* Every codec, not only SBC XQ: which encoder ran, at which rate, is the first question a
     * speed report raises (#44). */
    logf_("%s encoder start: codec %d, args %d %d %d %d, rate %d, mode %d, mtu %d",
          codec == CODEC_SBC ? (g_xq && g_sbc != 's' ? "SBC XQ" : "SBC") : "Sony", codec, a, b, c, d, rate, mode,
          mtu);
    pthread_mutex_lock(&g.lock);
    g.set.have = 0;
    pthread_mutex_unlock(&g.lock);
    const int r = real.enc_init ? real.enc_init(codec, a, b, c, d, rate, mode, mtu) : -1;
    /* SBC XQ Auto: an XQ stream (the Dual Channel endpoint is ours) whose encoder settings were
     * captured, and the two calls it needs. */
    pthread_mutex_lock(&g.lock);
    g.abr_on = codec == CODEC_SBC && g_xq && g_sbc == 'x' && g.set.have && real.sbc_set &&
               real.req_status;
    g.abr_pending = 0;
    g.abr_changes = 0;
    if (g.abr_on) {
        clock_gettime(CLOCK_MONOTONIC, &now);
        sbc_abr_start(&g.abr, g.set.bitpool, g.link_min, ms_of(&now));
        logf_("sbc-abr: on, bitpool %d..%d", g.abr.floor, g.abr.top);
    }
    pthread_mutex_unlock(&g.lock);
    return r;
}

/* libsbc_enc's settings call, through blueangel's PLT: kept so SBC XQ Auto can set the bitpool
 * again with everything else as the encoder started. */
EXPORT void sbc_set(void* state, int alloc, int subbands, int blocks, int bitpool, unsigned char mode)
{
    init();
    pthread_mutex_lock(&g.lock);
    g.set.state = state;
    g.set.alloc = alloc;
    g.set.subbands = subbands;
    g.set.blocks = blocks;
    g.set.bitpool = bitpool;
    g.set.mode = mode;
    g.set.have = 1;
    pthread_mutex_unlock(&g.lock);
    if (real.sbc_set) real.sbc_set(state, alloc, subbands, blocks, bitpool, mode);
}

EXPORT void btmtk_a2dp_stream_data_encoder_deinit(int x)
{
    init();
    pthread_mutex_lock(&g.lock);
    if (g.probe) {
        logf_("sbc-probe: stream stop, %lu asked, %lu answered", g.probe_asks, g.probe_answers);
        g.probe = 0;
    }
    if (g.abr_on) {
        logf_("sbc-abr: stream stop at bitpool %d, %lu changes", g.abr.cur, g.abr_changes);
        g.abr_on = 0;
    }
    if (g.mon_on) {
        g.mon_on = 0;
        logf_("stream stop: codec %d, %lld s, PCM at %ld Hz (%d expected), worst wait %ld ms, "
              "%lu stalls over %d ms, %lu of its 10 s windows more than 1%% off",
              g.mon_codec, (g.mon.last_ms - g.mon.t0_ms) / 1000, pcm_rate_avg(&g.mon),
              g.mon.expect_hz, g.mon.worst_gap_ms, g.mon.stalls, PCM_RATE_STALL_MS,
              g.mon.off_windows);
    }
    int ours = g.enc != NULL;
    if (ours) {
        logf_("encoder stop: %lu frames, %lu too big, %lu failed, %lu waits, %lu catch-up bursts "
              "(worst %ld ms behind), %lu clock restarts, ended at %d bit/s",
              g.frames, g.dropped, g.failed, g.waits, g.bursts, g.worst_ms, g.resets,
              aac_enc_bitrate(g.enc));
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
    enum { LEAD_MS = 60, RESET_MS = 300, BURST_MS = 25 };
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    if (!g.t0.tv_sec) { g.t0 = now; return; }
    long due = (long)((long long)g.frames * AAC_FRAME_SAMPLES * 1000 / g.cfg_rate);
    long ahead = due - ms_since(&g.t0, &now);
    /* Behind by more than a frame (~21 ms) means the frames that follow go out back to back: a
     * burst the headphones may answer by changing their playback speed (#44). Counted, not
     * changed, until a log shows how often and how far. Each one says whose stall it was: a long
     * wait for PCM is the source (the player, the sound service), an oversleep is this thread. */
    if (ahead < -BURST_MS) {
        if (-ahead > g.worst_ms) g.worst_ms = -ahead;
        if (!g.behind && ahead >= -RESET_MS)   /* once per fall, not per frame */
            LOG_RARE(g.bursts, "behind %ld ms, catching up: waited %ld ms for PCM, last sleep "
                     "overslept %ld ms", -ahead, g.gap_ms, g.overslept_ms);
        g.behind = 1;
    } else {
        g.behind = 0;
    }
    if (ahead < -RESET_MS) {
        /* Fell behind (a pause, an underrun): restart the clock instead of bursting. */
        LOG_RARE(g.resets, "behind %ld ms, clock restarted: waited %ld ms for PCM", -ahead,
                 g.gap_ms);
        g.t0 = now;
        g.frames = 0;
    } else if (ahead > LEAD_MS) {
        g.waits++;
        usleep((useconds_t)(ahead - LEAD_MS) * 1000);
        struct timespec woke;
        clock_gettime(CLOCK_MONOTONIC, &woke);
        g.overslept_ms = ms_since(&now, &woke) - (ahead - LEAD_MS);
    }
}

/* mtkbt's answer to the buffer-status request reaches blueangel's LDAC ABR through this PLT entry.
 * While an SBC stream is probed the answer is ours, and LDAC (not running) never sees it; otherwise
 * it goes on to LDAC unchanged. */
EXPORT void btmtk_a2dp_ldac_abr_proc(int status)
{
    init();
    pthread_mutex_lock(&g.lock);
    if (!g.probe && !g.abr_on) {
        pthread_mutex_unlock(&g.lock);
        if (real.abr_proc) real.abr_proc(status);
        return;
    }
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    const long long now = ms_of(&ts);
    const int q = status & 0xff;
    if (g.abr_on) {
        const int bp = sbc_abr_feed(&g.abr, q, now);
        if (bp) {
            g.abr_pending = bp;
            g.abr_changes++;
            /* Every change: at most one a second down and one per 10 s up, and each is evidence. */
            logf_("sbc-abr: queue %d -> bitpool %d (about %d kbit/s)", q, bp,
                  (12 + 4 * bp) * 8 * 44100 / 128 / 1000);
        }
    }
    if (!g.probe) {
        pthread_mutex_unlock(&g.lock);
        return;
    }
    if (!g.probe_answers) logf_("sbc-probe: first answer %d", q);
    g.probe_answers++;
    if (!g.q_n || q < g.q_min) g.q_min = q;
    if (!g.q_n || q > g.q_max) g.q_max = q;
    g.q_sum = g.q_n ? g.q_sum + q : q;
    g.q_n++;
    if (!g.probe_win_ms) g.probe_win_ms = now;
    if (now - g.probe_win_ms >= 1000) {
        logf_("sbc-q: %d..%d mean %ld.%ld over %d answers", g.q_min, g.q_max, g.q_sum / g.q_n,
              (g.q_sum * 10 / g.q_n) % 10, g.q_n);
        g.q_n = 0;
        g.probe_win_ms = now;
    }
    pthread_mutex_unlock(&g.lock);
}

EXPORT void btmtk_a2dp_send_audio_stream_data(void* data, unsigned len)
{
    init();
    pthread_mutex_lock(&g.lock);
    watch_pcm(len & 0xffff);
    if (!g.enc) {
        int ask = 0, apply = 0;
        if (g.abr_pending) {
            apply = g.abr_pending;
            g.abr_pending = 0;
        }
        if (g.probe || g.abr_on) {
            struct timespec ts;
            clock_gettime(CLOCK_MONOTONIC, &ts);
            const long long now = ms_of(&ts);
            if (now - g.probe_asked_ms >= 100) {
                g.probe_asked_ms = now;
                g.probe_asks++;
                ask = 1;
            }
        }
        void* st = g.set.state;
        const int al = g.set.alloc, sb = g.set.subbands, bl = g.set.blocks;
        const unsigned char md = g.set.mode;
        pthread_mutex_unlock(&g.lock);
        /* On the sending thread, between frames: libsbc picks the new bitpool up on the next one. */
        if (apply) real.sbc_set(st, al, sb, bl, apply, md);
        if (real.send_pcm) real.send_pcm(data, len);
        if (ask) real.req_status();
        return;
    }
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    g.gap_ms = g.last_in.tv_sec ? ms_since(&g.last_in, &now) : 0;
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
    /* From the END of this call, so the gap is the source's wait and not our own pace sleep. */
    clock_gettime(CLOCK_MONOTONIC, &g.last_in);
    pthread_mutex_unlock(&g.lock);
}
