// tuner_shim.cpp — implements cinder_tuner.h over Sony's FM tuner services.
//
// LOADED WITH dlopen, NOT LINKED. cinder-home is the Home app: if it fails to start, the device
// has no UI and the launcher's bad-boot counter reverts to stock. A DT_NEEDED on
// libTunerPlayerService/libAudioInPlayerService would make FM a boot dependency for a feature the
// user may never open. dlopen keeps the failure local — no radio, everything else fine. Same
// reasoning as the libNfcService rule.
//
// The services are reached by their exported client FACTORIES and then by VTABLE SLOT; the slot
// numbers were recovered from R_ARM_ABS32 relocations (analysis/RE_fm_tuner.md) because the
// vtables are relocation-filled and read as zeros in the file.
#include "cinder_tuner.h"

#include <dlfcn.h>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <unistd.h>
#include <pthread.h>
#include <cstdio>

// ALSA is dlopen'd for the same reason the Sony services are: cinder-home is the Home app, and a
// missing library must not stop it booting. It is only needed by the scanner.
typedef struct _snd_pcm snd_pcm_t;
typedef unsigned long snd_pcm_uframes_t;
typedef signed long   snd_pcm_sframes_t;

namespace {

// ── vtable slots ────────────────────────────────────────────────────────────────────────────
enum {  // TunerPlayerServiceClient
    T_GetTunerState = 3, T_Open = 4, T_Close = 5, T_Play = 6, T_Stop = 7,
    T_GetStereoState = 12, T_GetFrequency = 16, T_SetFrequency = 17,
};
enum {  // AudioInPlayerServiceClient
    A_Play = 3, A_Stop = 5, A_GetState = 6,
};

typedef void* (*factory_fn)(void);
typedef int (*fn_v)(void*);
typedef int (*fn_pu)(void*, unsigned*);
typedef int (*fn_cu)(void*, const unsigned*);
typedef int (*fn_pi)(void*, int*);

inline void* vslot(void* obj, int i) { return (*(void***)obj)[i]; }

// ALSA entry points, resolved on first scan.
struct Alsa {
    int (*open)(snd_pcm_t**, const char*, int, int) = nullptr;
    int (*set_params)(snd_pcm_t*, int, int, unsigned, unsigned, int, unsigned) = nullptr;
    snd_pcm_sframes_t (*readi)(snd_pcm_t*, void*, snd_pcm_uframes_t) = nullptr;
    int (*recover)(snd_pcm_t*, int, int) = nullptr;
    int (*prepare)(snd_pcm_t*) = nullptr;
    int (*start)(snd_pcm_t*) = nullptr;
    int (*drop)(snd_pcm_t*) = nullptr;
    int (*close)(snd_pcm_t*) = nullptr;
    bool ok = false;
};
Alsa g_alsa;

bool alsa_load() {
    if (g_alsa.ok) return true;
    void* h = dlopen("libasound.so.2", RTLD_NOW);
    if (!h) h = dlopen("libasound.so", RTLD_NOW);
    if (!h) return false;
    #define SYM(f, n) *(void**)(&g_alsa.f) = dlsym(h, n); if (!g_alsa.f) return false;
    SYM(open, "snd_pcm_open") SYM(set_params, "snd_pcm_set_params") SYM(readi, "snd_pcm_readi")
    SYM(recover, "snd_pcm_recover") SYM(prepare, "snd_pcm_prepare") SYM(start, "snd_pcm_start")
    SYM(drop, "snd_pcm_drop") SYM(close, "snd_pcm_close")
    #undef SYM
    g_alsa.ok = true;
    return true;
}

// ── THE CHIP, DIRECTLY: /proc/regmon/Si4708icx ──────────────────────────────────────────────
//
// Sony's driver registers the tuner with the kernel's generic register monitor, which exposes
// every Si470x register over I2C as a `target` + `value` pair. That is the whole reason this file
// no longer has to measure the radio by listening to it:
//
//   * `STATUS_RSSI[7:0]`  — a REAL graded signal meter. Sony's GetSignalLevel returns a constant
//     1 at every frequency in the band, and the V4L2 `signal` field is binary 0/65535.
//   * `STATUS_RSSI[14]` STC — tune-complete. Waiting on it rather than on a fixed settle turns a
//     90-second band scan into a ~9-second one. Not faster than that: MEASURED on device, a tune
//     costs the CHIP about 45 ms to settle, and 206 steps of that is the floor. The register path
//     removes the software tax, not the physics.
//   * `POWERCFG` SEEK/SEEKUP/SKMODE — the chip walks the band itself. Sony's StartAutoTuning is a
//     48-byte stub that never reads its arguments.
//
// WHY SONY'S SEEK ALWAYS FAILED, and it is not a code defect: stock `SEEKTH` is 18, and no station
// in range reads above 14. The threshold sits above the entire band, so seek runs to the band limit
// and raises SF/BL every time. We lower it and it works.
//
// The nodes ship root-only; `cinder-fm` (setuid, src/cinder-fm.c) widens exactly those two files,
// after which this is plain file I/O at uid 100.
//
// RULES, learned by probing the live chip — see analysis/RE_fm_tuner.md:
//   * NEVER write BAND/SPACE (`SYSCONFIG2[7:4]`). They define the channel<->frequency mapping that
//     the Sony driver's own SetFrequency arithmetic assumes; changing them desyncs the two. We READ
//     them and follow whatever the driver chose, and we only ever write SEEKTH, the top byte.
//   * Never write TEST1 or BOOTCONFIG. Those are the chip's bring-up state.
//   * `target` is a single global selector, so every access has to be serialised.
namespace regmon {

const char* const TARGET = "/proc/regmon/Si4708icx/target";
const char* const VALUE  = "/proc/regmon/Si4708icx/value";
const char* const HELPER = "/system/vendor/unknown321/bin/cinder-fm";

enum {  // Si470x register map, as the driver names it in `target`
    R_DEVICEID = 0x00, R_POWERCFG = 0x02, R_CHANNEL = 0x03, R_SYSCONFIG1 = 0x04,
    R_SYSCONFIG2 = 0x05, R_SYSCONFIG3 = 0x06, R_STATUS = 0x0A, R_READCHAN = 0x0B,
};
enum {  // STATUS_RSSI
    ST_STC = 1 << 14, ST_SFBL = 1 << 13, ST_STEREO = 1 << 8, ST_RSSI = 0xFF,
};
enum {  // POWERCFG
    PC_DMUTE = 1 << 14, PC_SKMODE = 1 << 10, PC_SEEKUP = 1 << 9, PC_SEEK = 1 << 8,
    PC_ENABLE = 1 << 0,
};
enum { CH_TUNE = 1 << 15, CH_MASK = 0x03FF };

pthread_mutex_t g_lock = PTHREAD_MUTEX_INITIALIZER;
int g_live = -1;   // -1 = not probed yet, 0 = unavailable, 1 = usable

// Seek threshold, refreshed by every scan from the band's measured noise floor so it tracks this
// aerial and this location. 6 is the measured-good default (floor 5-6, carriers 9-14).
int g_seek_th = 6;

bool put(const char* path, unsigned v) {
    FILE* f = std::fopen(path, "w");
    if (!f) return false;
    int n = std::fprintf(f, "0x%04X\n", v & 0xFFFF);
    return std::fclose(f) == 0 && n > 0;
}

int value_get() {
    FILE* f = std::fopen(VALUE, "r");
    if (!f) return -1;
    unsigned v = 0;
    int n = std::fscanf(f, "%x", &v);   // the node prints "0x0000000B"
    std::fclose(f);
    return n == 1 ? (int)(v & 0xFFFF) : -1;
}

// Caller holds g_lock.
int rd_l(int reg) { return put(TARGET, (unsigned)reg) ? value_get() : -1; }
bool wr_l(int reg, unsigned v) { return put(TARGET, (unsigned)reg) && put(VALUE, v); }

int rd(int reg) {
    pthread_mutex_lock(&g_lock);
    int v = rd_l(reg);
    pthread_mutex_unlock(&g_lock);
    return v;
}

// Non-blocking read, for anything on the render thread. A scan or seek slice holds the lock for a
// few ms at a time, and the 1 Hz meter poll waiting on it would put the UI's own thread to sleep —
// the same class of stall this file was just restructured to remove. A skipped sample is nothing;
// a skipped frame is visible.
int rd_try(int reg) {
    if (pthread_mutex_trylock(&g_lock) != 0) return -1;
    int v = rd_l(reg);
    pthread_mutex_unlock(&g_lock);
    return v;
}

// Are the nodes usable by this uid? Runs the setuid helper once if not, exactly as the GPU path
// runs cinder-gpunode, then re-checks. A failure here is never fatal: everything falls back to the
// audio-measured routes below.
bool available() {
    if (g_live >= 0) return g_live == 1;
    for (int attempt = 0; attempt < 2; attempt++) {
        if (access(TARGET, R_OK | W_OK) == 0 && access(VALUE, R_OK | W_OK) == 0) {
            // Prove it end to end rather than trusting the mode bits: DEVICEID is a constant.
            pthread_mutex_lock(&g_lock);
            int id = rd_l(R_DEVICEID);
            pthread_mutex_unlock(&g_lock);
            if (id > 0) {
                std::fprintf(stderr, "[cinder-tuner] regmon live, DEVICEID=0x%04X\n", id);
                g_live = 1;
                return true;
            }
            std::fprintf(stderr, "[cinder-tuner] regmon readable but DEVICEID read failed\n");
            break;
        }
        if (attempt == 0) {
            int rc = std::system(HELPER);
            std::fprintf(stderr, "[cinder-tuner] %s rc=%d (widening regmon nodes)\n",
                         HELPER, (rc == -1) ? -1 : ((rc >> 8) & 0xff));
        }
    }
    std::fprintf(stderr, "[cinder-tuner] regmon UNAVAILABLE — falling back to audio measurement\n");
    g_live = 0;
    return false;
}

// The chip's own channel<->frequency plan, READ from SYSCONFIG2 rather than assumed, so a
// different region setting cannot silently put every frequency out by a factor.
struct Plan { int base_khz, space_khz, top_khz; };
Plan plan_l() {
    Plan p = { 87500, 100, 108000 };
    int s2 = rd_l(R_SYSCONFIG2);
    if (s2 < 0) return p;
    switch ((s2 >> 6) & 3) {                      // BAND
        case 0:  p.base_khz = 87500; p.top_khz = 108000; break;
        case 1:  p.base_khz = 76000; p.top_khz = 108000; break;
        default: p.base_khz = 76000; p.top_khz =  90000; break;
    }
    switch ((s2 >> 4) & 3) {                      // SPACE
        case 0:  p.space_khz = 200; break;
        case 1:  p.space_khz = 100; break;
        default: p.space_khz =  50; break;
    }
    return p;
}

// Wait for STC. It is normally set by the second read; the bound is a safety net, not a budget.
int wait_stc_l() {
    for (int i = 0; i < 200; i++) {
        int s = rd_l(R_STATUS);
        if (s < 0) return -1;
        if (s & ST_STC) return s;
        usleep(1000);
    }
    return rd_l(R_STATUS);
}

// RSSI at one frequency, tuning there to get it. Caller holds g_lock.
int rssi_at_l(const Plan& p, int khz) {
    int chan = (khz - p.base_khz) / p.space_khz;
    if (chan < 0 || chan > CH_MASK) return -1;
    if (!wr_l(R_CHANNEL, (unsigned)(chan | CH_TUNE))) return -1;
    int st = wait_stc_l();
    wr_l(R_CHANNEL, (unsigned)chan);
    return st < 0 ? -1 : (st & ST_RSSI);
}

// Put a seek result back on the 100 kHz raster. The driver runs the chip at SPACE=50 kHz, so its
// hardware seek can and does stop on a HALF-step — 91.45 MHz, measured — which is the shoulder of a
// carrier rather than a carrier. European broadcasts are on the 100 kHz raster, so the right answer
// is one of the two neighbours; which one is not guessable from the offset, so we measure both and
// keep the stronger. Two tunes, about 90 ms.
//
// Caller holds g_lock.
int snap_l(const Plan& p, int khz) {
    const int RASTER = 100;
    if (khz % RASTER == 0) return khz;
    int lo = (khz / RASTER) * RASTER, hi = lo + RASTER;
    int rl = rssi_at_l(p, lo), rh = rssi_at_l(p, hi);
    int pick = (rh > rl) ? hi : lo;
    rssi_at_l(p, pick);                       // leave the chip on the one we chose
    return pick;
}

// Tune the chip directly and return where it landed, in kHz (0 on failure). Caller holds g_lock.
int tune_l(const Plan& p, int khz) {
    int chan = (khz - p.base_khz) / p.space_khz;
    if (chan < 0 || chan > CH_MASK) return 0;
    if (!wr_l(R_CHANNEL, (unsigned)(chan | CH_TUNE))) return 0;
    int st = wait_stc_l();
    int rc = rd_l(R_READCHAN);
    wr_l(R_CHANNEL, (unsigned)chan);              // clear TUNE, which clears STC
    if (st < 0 || rc < 0) return 0;
    return p.base_khz + (rc & CH_MASK) * p.space_khz;
}

}  // namespace regmon

void* g_tuner = nullptr;   // TunerPlayerServiceClient
void* g_ain   = nullptr;   // AudioInPlayerServiceClient
bool  g_playing = false;

void* load(const char* so, const char* sym) {
    void* h = dlopen(so, RTLD_NOW | RTLD_GLOBAL);
    if (!h) {
        std::fprintf(stderr, "[cinder-tuner] dlopen(%s) FAILED: %s\n", so, dlerror());
        return nullptr;
    }
    factory_fn f = (factory_fn)dlsym(h, sym);
    if (!f) {
        std::fprintf(stderr, "[cinder-tuner] dlsym(%s) FAILED\n", sym);
        return nullptr;
    }
    void* obj = nullptr;
    try { obj = f(); } catch (...) { obj = nullptr; }
    std::fprintf(stderr, "[cinder-tuner] %s -> client %p\n", so, obj);
    return obj;
}

// THE STEP EVERYTHING ELSE DEPENDS ON. `analog input device` selects what feeds the codec's ADC,
// and the FM path is analogue into that ADC. With it left at `off`, AudioInPlayerService opens
// happily, returns 0, reports PlayerState 2 — and plays silence, because it is reading a dead
// input. Two 45-second "I hear nothing" tests on a strong carrier were this and nothing else.
void route(bool on) {
    int rc = std::system(on ? "amixer -c0 cset numid=26 1 >/dev/null 2>&1"
                            : "amixer -c0 cset numid=26 0 >/dev/null 2>&1");
    std::fprintf(stderr, "[cinder-tuner] analog input device -> %s (rc=%d)\n",
                 on ? "tuner" : "off", rc);
}

bool ensure_clients() {
    if (!g_tuner)
        g_tuner = load("libTunerPlayerService.so",
                       "_ZN3pst8services31TunerPlayerServiceClientFactory14CreateInstanceEv");
    if (!g_ain)
        g_ain = load("libAudioInPlayerService.so",
                     "_ZN3pst8services33AudioInPlayerServiceClientFactory14CreateInstanceEv");
    return g_tuner != nullptr;
}


cinder_tuner_progress_fn g_progress = nullptr;

// Measure ONE frequency and return its high-frequency ratio. Low = station, high = hiss.
// The windows are parameters because a live seek must feel like a sweep (short windows) while a
// full scan can afford to be careful (long ones).
double measure(snd_pcm_t* pcm, int khz, int settle_us, int grab_frames) {
    unsigned uf = (unsigned)khz;
    try { ((fn_cu)vslot(g_tuner, T_SetFrequency))(g_tuner, &uf); } catch (...) { return 9.9; }
    usleep(settle_us);
    g_alsa.drop(pcm); g_alsa.prepare(pcm); g_alsa.start(pcm);
    static short mbuf[(44100 / 5) * 2];
    int got = 0;
    while (got < grab_frames) {
        snd_pcm_sframes_t n = g_alsa.readi(pcm, mbuf + got * 2, grab_frames - got);
        if (n < 0) { g_alsa.recover(pcm, (int)n, 1); break; }
        if (n == 0) break;
        got += (int)n;
    }
    if (got < grab_frames / 2) return 9.9;
    double sabs = 0, sdif = 0;
    int prev = mbuf[0];
    for (int k = 0; k < got; k++) {
        int v = mbuf[k * 2];
        sabs += (v < 0 ? -v : v);
        int d = v - prev; sdif += (d < 0 ? -d : d);
        prev = v;
    }
    return sabs > 0 ? sdif / sabs : 9.9;
}

} // namespace

extern "C" {

int cinder_tuner_start(int khz) {
    if (!ensure_clients()) return -1;
    route(true);                                    // 1. ADC source = tuner
    unsigned f = (unsigned)khz;
    try {
        ((fn_v)vslot(g_tuner, T_Open))(g_tuner);    // 2. tuner up, tuned, streaming
        ((fn_cu)vslot(g_tuner, T_SetFrequency))(g_tuner, &f);
        ((fn_v)vslot(g_tuner, T_Play))(g_tuner);
    } catch (...) { route(false); return -1; }
    // 3. the audio path, AFTER the tuner is streaming
    int ain_rc = -1, ain_st = -1;
    if (g_ain) {
        try { ain_rc = ((fn_v)vslot(g_ain, A_Play))(g_ain); } catch (...) {}
        try { ain_st = ((fn_v)vslot(g_ain, A_GetState))(g_ain); } catch (...) {}
    }
    std::fprintf(stderr, "[cinder-tuner] start(%d kHz): AudioIn rc=%d state=%d (2 = path open)\n",
                 khz, ain_rc, ain_st);
    // Re-assert the frequency once everything is up; Play() is not guaranteed to keep it.
    try { ((fn_cu)vslot(g_tuner, T_SetFrequency))(g_tuner, &f); } catch (...) {}
    g_playing = true;
    return 0;
}

int cinder_tuner_stop(void) {
    if (g_ain && g_playing) { try { ((fn_v)vslot(g_ain, A_Stop))(g_ain); } catch (...) {} }
    if (g_tuner) {
        try { ((fn_v)vslot(g_tuner, T_Stop))(g_tuner); } catch (...) {}
        try { ((fn_v)vslot(g_tuner, T_Close))(g_tuner); } catch (...) {}
    }
    route(false);
    g_playing = false;
    return 0;
}

int cinder_tuner_audio_start(void) {
    if (!g_ain) return -1;
    int rc = -1;
    try { rc = ((fn_v)vslot(g_ain, A_Play))(g_ain); } catch (...) { return -1; }
    std::fprintf(stderr, "[cinder-tuner] audio path start rc=%d\n", rc);
    return rc;
}

int cinder_tuner_audio_stop(void) {
    if (!g_ain) return -1;
    try { ((fn_v)vslot(g_ain, A_Stop))(g_ain); } catch (...) { return -1; }
    std::fprintf(stderr, "[cinder-tuner] audio path stopped (hw:0,1 released)\n");
    return 0;
}

// Is there real PCM on the Bluetooth bridge's source? Returns the RMS of `ms` of capture from
// hw:0,1 (0..32767), or <0 if the path could not be opened.
//
// This is the precondition the BT-out button depends on and which had never actually been checked:
// FM audio is ANALOGUE into the codec, and only becomes PCM because `analog input device` routes it
// to the ADC. If that route is wrong the capture still opens, still returns frames, and they are
// all silence — the exact failure that cost two 45-minute "I hear nothing" sessions on the local
// path. So the bridge should ask before it claims to be sending anything.
//
// It BORROWS hw:0,1 the same way the audio seek does: AudioInPlayerService owns that PCM while the
// radio is audible on the jack, so the local path is stopped for the duration and handed back.
int cinder_tuner_capture_rms(int ms) {
    if (!alsa_load()) return -1;
    if (ms < 20) ms = 20;
    if (ms > 2000) ms = 2000;

    const bool was = g_playing;
    if (was && g_ain) { try { ((fn_v)vslot(g_ain, A_Stop))(g_ain); } catch (...) {} }

    snd_pcm_t* pcm = nullptr;
    double rms = -1;
    const unsigned RATE = 44100;
    // Report the actual errno. "capture failed" is not a diagnosis: -EBUSY means somebody else
    // holds hw:0,1 (hagodaemon does, and does not necessarily let go on Stop), while -ENOENT or
    // -ENODEV means the device is not there at all. Those need opposite responses.
    int orc = g_alsa.open(&pcm, "hw:0,1", 1, 0);
    int prc = (orc >= 0) ? g_alsa.set_params(pcm, 2, 3, 2, RATE, 1, 200000) : -1;
    if (orc < 0 || prc < 0)
        std::fprintf(stderr, "[cinder-tuner] hw:0,1 open=%d set_params=%d (-16 = EBUSY, held by "
                             "another process)\n", orc, prc);
    if (orc >= 0 && prc >= 0) {
        g_alsa.start(pcm);                       // this device does not start capture on its own
        const int want = (int)RATE * ms / 1000;
        static short buf[(44100 / 2) * 2];
        const int cap = (int)(sizeof buf / sizeof buf[0]) / 2;
        int need = want < cap ? want : cap, got = 0;
        while (got < need) {
            snd_pcm_sframes_t n = g_alsa.readi(pcm, buf + got * 2, need - got);
            if (n < 0) { g_alsa.recover(pcm, (int)n, 1); break; }
            if (n == 0) break;
            got += (int)n;
        }
        if (got > 0) {
            double acc = 0;
            for (int k = 0; k < got; k++) {
                double v = buf[k * 2];
                acc += v * v;
            }
            rms = std::sqrt(acc / got);
        }
        g_alsa.close(pcm);
    }
    if (was && g_ain) { try { ((fn_v)vslot(g_ain, A_Play))(g_ain); } catch (...) {} }
    std::fprintf(stderr, "[cinder-tuner] capture rms(%d ms) = %.1f\n", ms, rms);
    return rms < 0 ? -1 : (int)rms;
}

int cinder_tuner_set_khz(int khz) {
    if (!g_tuner) return -1;
    unsigned f = (unsigned)khz;
    try { ((fn_cu)vslot(g_tuner, T_SetFrequency))(g_tuner, &f); } catch (...) { return -1; }
    return 0;
}

int cinder_tuner_get_khz(void) {
    if (!g_tuner) return 0;
    unsigned f = 0;
    try { ((fn_pu)vslot(g_tuner, T_GetFrequency))(g_tuner, &f); } catch (...) { return 0; }
    return (int)f;
}

int cinder_tuner_state(void) {
    if (!g_tuner) return -1;
    try { return ((fn_v)vslot(g_tuner, T_GetTunerState))(g_tuner); } catch (...) { return -1; }
}

int cinder_tuner_stereo(void) {
    // The chip's own ST bit is the truth. Sony's GetStereoState read 0 on every station ever
    // tested, including ones that were audibly playing, so it is only a fallback.
    if (regmon::available()) {
        int s = regmon::rd_try(regmon::R_STATUS);
        if (s >= 0) return (s & regmon::ST_STEREO) ? 1 : 0;
    }
    if (!g_tuner) return -1;
    int st = -1;
    try { ((fn_pi)vslot(g_tuner, T_GetStereoState))(g_tuner, &st); } catch (...) { return -1; }
    return st;
}

int cinder_tuner_hw(void) { return regmon::available() ? 1 : 0; }

int cinder_tuner_signal(void) {
    if (!regmon::available()) return -1;
    int s = regmon::rd_try(regmon::R_STATUS);     // never block the caller; see rd_try
    return s < 0 ? -1 : (s & regmon::ST_RSSI);
}

void cinder_tuner_set_progress_cb(cinder_tuner_progress_fn cb) { g_progress = cb; }

// HARDWARE SEEK. The chip walks the band itself: set SEEK/SEEKUP in POWERCFG, poll STC, read
// READCHAN. Unlike the audio-measured seek below this needs neither ALSA nor the capture PCM, so
// AudioInPlayerService keeps hw:0,1 the whole time — the radio stays audible while it seeks, and
// a seek can no longer leave the audio path stopped if the shell's guard fires mid-sweep.
//
// READCHAN tracks the chip as it moves, so polling it drives a REAL sweep on the dial rather than
// a jump. SKMODE is left clear so the band wraps, which is how a radio is expected to behave; the
// chip raises SF/BL by itself once it has been all the way round.
static int seek_hw(int from_khz, int dir, cinder_tuner_step_fn on_step) {
    using namespace regmon;
    pthread_mutex_lock(&g_lock);
    Plan p = plan_l();
    int pc0 = rd_l(R_POWERCFG), s2 = rd_l(R_SYSCONFIG2), s3 = rd_l(R_SYSCONFIG3);
    if (pc0 < 0 || s2 < 0 || s3 < 0) { pthread_mutex_unlock(&g_lock); return 0; }

    tune_l(p, from_khz);                                     // start from where the UI thinks we are
    // SEEKTH is the top byte ONLY — BAND/SPACE in the low byte stay exactly as the driver set them.
    wr_l(R_SYSCONFIG2, (unsigned)((s2 & 0x00FF) | ((g_seek_th & 0xFF) << 8)));
    // SKSNR=1 / SKCNT=1: the most permissive real quality gates. Stock leaves both at 0, i.e.
    // disabled, which lets seek stop on noise; the strict AN230 values reject everything here.
    wr_l(R_SYSCONFIG3, (unsigned)((s3 & 0xFF00) | (1 << 4) | 1));

    unsigned go = ((unsigned)pc0 & ~(unsigned)(PC_SKMODE | PC_SEEKUP)) | PC_SEEK;
    if (dir >= 0) go |= PC_SEEKUP;
    wr_l(R_POWERCFG, go);

    int found = 0, last_step = 0;
    for (int i = 0; i < 4000; i++) {                          // ~4 s ceiling; the chip is far faster
        int s = rd_l(R_STATUS);
        if (s < 0) break;
        if (s & ST_STC) {
            int rc = rd_l(R_READCHAN);
            if (rc >= 0 && !(s & ST_SFBL))
                found = p.base_khz + (rc & CH_MASK) * p.space_khz;
            break;
        }
        int rc = rd_l(R_READCHAN);                            // animate the dial from the hardware
        if (rc >= 0 && on_step) {
            int khz = p.base_khz + (rc & CH_MASK) * p.space_khz;
            if (khz != last_step) { last_step = khz; on_step(khz); }
        }
        usleep(1000);
    }

    wr_l(R_POWERCFG, (unsigned)pc0);                          // clear SEEK, which clears STC
    wr_l(R_SYSCONFIG2, (unsigned)s2);                         // put the thresholds back
    wr_l(R_SYSCONFIG3, (unsigned)s3);
    if (found) found = snap_l(p, found);                      // off the 50 kHz shoulder, onto the raster
    pthread_mutex_unlock(&g_lock);

    // Hand the result to the Sony service so the service and the chip agree about the frequency —
    // otherwise cinder_tuner_get_khz() keeps returning where the service last thought it was.
    if (found && g_tuner) {
        unsigned uf = (unsigned)found;
        try { ((fn_cu)vslot(g_tuner, T_SetFrequency))(g_tuner, &uf); } catch (...) {}
    }
    std::fprintf(stderr, "[cinder-tuner] hw seek from %d dir %+d -> %d kHz\n", from_khz, dir, found);
    return found;
}

// ── CHUNKED SEEK — same reason as the chunked scan above ────────────────────────────────────
//
// The chip does the walking; all we do is poll STC. Blocking that poll on the render thread cost
// 1-4 s of frozen UI AND made the dial sweep invisible: on_step marked the UI dirty, but the thread
// that paints was the one sitting in the loop. Polled a slice at a time, the sweep is real.
namespace {
struct SeekJob {
    bool active = false;
    regmon::Plan plan;
    int pc0 = 0, s2 = 0, s3 = 0;
    int polls = 0;
} g_seek;
}  // namespace

int cinder_tuner_seek_begin(int from_khz, int dir) {
    using namespace regmon;
    if (!available()) return 0;
    if (dir == 0) dir = 1;
    pthread_mutex_lock(&g_lock);
    g_seek.plan = plan_l();
    g_seek.pc0 = rd_l(R_POWERCFG);
    g_seek.s2  = rd_l(R_SYSCONFIG2);
    g_seek.s3  = rd_l(R_SYSCONFIG3);
    if (g_seek.pc0 < 0 || g_seek.s2 < 0 || g_seek.s3 < 0) {
        pthread_mutex_unlock(&g_lock);
        return 0;
    }
    tune_l(g_seek.plan, from_khz);
    wr_l(R_SYSCONFIG2, (unsigned)((g_seek.s2 & 0x00FF) | ((g_seek_th & 0xFF) << 8)));
    wr_l(R_SYSCONFIG3, (unsigned)((g_seek.s3 & 0xFF00) | (1 << 4) | 1));
    unsigned go = ((unsigned)g_seek.pc0 & ~(unsigned)(PC_SKMODE | PC_SEEKUP)) | PC_SEEK;
    if (dir >= 0) go |= PC_SEEKUP;
    wr_l(R_POWERCFG, go);
    pthread_mutex_unlock(&g_lock);
    g_seek.polls = 0;
    g_seek.active = true;
    return 1;
}

// 0 = still walking (*cur_khz = where the chip is, for the dial), >0 = landed there, -1 = nothing.
int cinder_tuner_seek_step(int* cur_khz) {
    using namespace regmon;
    if (!g_seek.active) return -1;
    Plan& p = g_seek.plan;

    pthread_mutex_lock(&g_lock);
    int s = rd_l(R_STATUS);
    int rc = rd_l(R_READCHAN);
    pthread_mutex_unlock(&g_lock);
    if (s < 0) { cinder_tuner_seek_abort(); return -1; }

    if (cur_khz && rc >= 0) *cur_khz = p.base_khz + (rc & CH_MASK) * p.space_khz;

    // ~4 s of frames; the chip is normally done in well under one.
    if (!(s & ST_STC) && ++g_seek.polls < 400) return 0;

    int found = 0;
    if ((s & ST_STC) && !(s & ST_SFBL) && rc >= 0)
        found = p.base_khz + (rc & CH_MASK) * p.space_khz;

    pthread_mutex_lock(&g_lock);
    wr_l(R_POWERCFG, (unsigned)g_seek.pc0);
    wr_l(R_SYSCONFIG2, (unsigned)g_seek.s2);
    wr_l(R_SYSCONFIG3, (unsigned)g_seek.s3);
    if (found) found = snap_l(p, found);          // off the 50 kHz shoulder, onto the raster
    pthread_mutex_unlock(&g_lock);
    g_seek.active = false;

    if (found && g_tuner) {
        unsigned uf = (unsigned)found;
        try { ((fn_cu)vslot(g_tuner, T_SetFrequency))(g_tuner, &uf); } catch (...) {}
    }
    std::fprintf(stderr, "[cinder-tuner] chunked seek -> %d kHz after %d polls\n", found, g_seek.polls);
    return found ? found : -1;
}

void cinder_tuner_seek_abort(void) {
    using namespace regmon;
    if (!g_seek.active) return;
    pthread_mutex_lock(&g_lock);
    wr_l(R_POWERCFG, (unsigned)g_seek.pc0);
    wr_l(R_SYSCONFIG2, (unsigned)g_seek.s2);
    wr_l(R_SYSCONFIG3, (unsigned)g_seek.s3);
    pthread_mutex_unlock(&g_lock);
    g_seek.active = false;
}

int cinder_tuner_seek(int from_khz, int dir, cinder_tuner_step_fn on_step) {
    if (dir == 0) dir = 1;
    if (regmon::available()) return seek_hw(from_khz, dir, on_step);
    if (!g_tuner || !alsa_load()) return 0;
    // The audio path holds hw:0,1 while playing, so a seek BORROWS it: stop AudioIn, sweep with the
    // capture PCM, hand it back at the end. The tuner itself keeps playing throughout.
    bool was = g_playing;
    if (was && g_ain) { try { ((fn_v)vslot(g_ain, A_Stop))(g_ain); } catch (...) {} }

    snd_pcm_t* pcm = nullptr;
    int found = 0;
    if (g_alsa.open(&pcm, "hw:0,1", 1, 0) >= 0 &&
        g_alsa.set_params(pcm, 2, 3, 2, 44100, 1, 200000) >= 0) {
        g_alsa.start(pcm);
        const int SETTLE = 80000, GRAB = 44100 / 16;   // ~80 ms settle + ~60 ms of audio
        // Calibrate the noise floor on the frequency we are leaving, so the threshold adapts to
        // this aerial and this location instead of being a constant baked in here.
        double base = measure(pcm, from_khz, SETTLE, GRAB);
        if (base > 1.0 || base < 0.05) base = 0.45;
        int khz = from_khz;
        for (int i = 0; i < 210; i++) {
            khz += dir * 100;
            if (khz > 108000) khz = 87500;
            if (khz < 87500) khz = 108000;
            if (khz == from_khz) break;                 // wrapped the band, nothing out there
            double hf = measure(pcm, khz, SETTLE, GRAB);
            if (on_step) on_step(khz);
            if (hf < base * 0.7) { found = khz; break; }
        }
        g_alsa.close(pcm);
    }
    if (found) {
        unsigned uf = (unsigned)found;
        try { ((fn_cu)vslot(g_tuner, T_SetFrequency))(g_tuner, &uf); } catch (...) {}
    }
    if (was && g_ain) { try { ((fn_v)vslot(g_ain, A_Play))(g_ain); } catch (...) {} }
    return found;
}

// HARDWARE SCAN. Step the chip across the band, wait for STC, read the RSSI. MEASURED end to end
// on device: 206 steps in ~9.3 s, i.e. ~45 ms a step, which is the Si4708's own tune time and not
// something more code can remove. Against the ~450 ms an audio window costs per step that is a 10x
// improvement, and it is the honest number — an earlier draft of this comment said "about a
// second", which was the shell-loop measurement misread as the chip's.
//
// It needs neither ALSA nor AudioInPlayerService, so unlike the audio scan below it does NOT have
// to stop the radio first. The chip is muted for the sweep (DMUTE cleared) so the user hears a
// clean pause rather than the band being dragged past their ears, and POWERCFG is restored after.
static int scan_hw(int start_khz, int end_khz, int* out_khz, int max) {
    using namespace regmon;
    struct Hit { int khz, rssi; };
    static Hit hits[256];
    int nh = 0;

    pthread_mutex_lock(&g_lock);
    Plan p = plan_l();
    int pc0 = rd_l(R_POWERCFG);
    if (pc0 < 0) { pthread_mutex_unlock(&g_lock); return 0; }
    wr_l(R_POWERCFG, (unsigned)pc0 & ~(unsigned)PC_DMUTE);    // mute for the sweep

    // 100 kHz is the raster the UI tunes on, whatever the chip's own SPACE happens to be.
    const int step = 100;
    const int span = (end_khz - start_khz) / step + 1;
    for (int f = start_khz; f <= end_khz && nh < 256; f += step) {
        int chan = (f - p.base_khz) / p.space_khz;
        if (chan < 0 || chan > CH_MASK) continue;
        if (!wr_l(R_CHANNEL, (unsigned)(chan | CH_TUNE))) break;
        int s = wait_stc_l();
        wr_l(R_CHANNEL, (unsigned)chan);
        if (s < 0) break;
        hits[nh].khz = f;
        hits[nh].rssi = s & ST_RSSI;
        nh++;
        if (g_progress && span > 0) g_progress(nh * 100 / span);
    }
    wr_l(R_POWERCFG, (unsigned)pc0);                          // unmute, back to how we found it
    pthread_mutex_unlock(&g_lock);
    if (nh < 8) return 0;

    // The noise floor is whatever most of the band reads — measured 5-6 here, but it moves with the
    // aerial, so take it from the data rather than baking a number in. Carriers ran 9-14 against
    // that floor, so +3 is a real peak and not a wobble.
    static int sorted[256];
    for (int i = 0; i < nh; i++) sorted[i] = hits[i].rssi;
    for (int a = 1; a < nh; a++) {
        int v = sorted[a], b = a - 1;
        while (b >= 0 && sorted[b] > v) { sorted[b + 1] = sorted[b]; b--; }
        sorted[b + 1] = v;
    }
    const int floor_rssi = sorted[nh / 2];
    const int cut = floor_rssi + 3;
    // Remember the floor for seek: SEEKTH has to sit above the noise and below the weakest station,
    // and stock's 18 sits above the entire band, which is exactly why Sony's seek never worked.
    g_seek_th = floor_rssi + 1;

    // A transmitter lights three or four adjacent 100 kHz steps. Keep the strongest of each run,
    // so the preset list holds stations rather than shoulders.
    struct Peak { int khz, rssi; };
    static Peak peaks[64];
    int np = 0;
    for (int i = 0; i < nh; i++) {
        if (hits[i].rssi < cut) continue;
        if (np > 0 && hits[i].khz - peaks[np - 1].khz <= 200) {
            if (hits[i].rssi > peaks[np - 1].rssi) peaks[np - 1] = { hits[i].khz, hits[i].rssi };
            continue;
        }
        if (np < 64) peaks[np++] = { hits[i].khz, hits[i].rssi };
    }
    for (int a = 1; a < np; a++) {                            // strongest first
        Peak v = peaks[a]; int b = a - 1;
        while (b >= 0 && peaks[b].rssi < v.rssi) { peaks[b + 1] = peaks[b]; b--; }
        peaks[b + 1] = v;
    }
    int out = 0;
    for (int i = 0; i < np && out < max; i++) out_khz[out++] = peaks[i].khz;
    std::fprintf(stderr, "[cinder-tuner] hw scan %d-%d kHz: floor=%d cut=%d -> %d station(s)\n",
                 start_khz, end_khz, floor_rssi, cut, out);
    return out;
}

// ── CHUNKED SCAN — the same sweep, one channel per call ─────────────────────────────────────
//
// WHY THIS EXISTS: cinder-home runs input, actions AND painting on one thread (the render worker;
// carry_out is called from input_pump, which sits above cinder_render_tick in that loop). So the
// blocking scan below froze the screen for its whole ~10 s — reported from the device 2026-08-18,
// "paused and wouldn't respond". The progress callback could not help: it marks the UI dirty, but
// nothing can paint while the thread that paints is inside the scan.
//
// A worker thread was the obvious fix and is the wrong one here: the sweep brackets itself with
// Sony service calls (Open/Play/Stop/Close), and pst clients are not something to move off their
// pump thread on a hunch — see reference_pst_ipc_pump.
//
// So the sweep is turned inside out instead. One channel is ~45 ms (the chip's own tune time), so
// a per-frame slice costs about two frames and the loop keeps running: the progress bar becomes
// real, the UI stays live, and SCAN can actually be cancelled mid-sweep — which the screen already
// offered and could not previously deliver.
namespace {
struct ScanJob {
    bool  active = false;
    regmon::Plan plan;
    int   pc0 = 0;
    int   f = 0, start = 0, end = 0, span = 1;
    struct Hit { int khz, rssi; } hits[256];
    int   nh = 0;
} g_scan;
}  // namespace

int cinder_tuner_scan_begin(int start_khz, int end_khz) {
    using namespace regmon;
    if (!available()) return 0;
    pthread_mutex_lock(&g_lock);
    g_scan.plan = plan_l();
    g_scan.pc0 = rd_l(R_POWERCFG);
    if (g_scan.pc0 >= 0)
        wr_l(R_POWERCFG, (unsigned)g_scan.pc0 & ~(unsigned)PC_DMUTE);   // mute for the sweep
    pthread_mutex_unlock(&g_lock);
    if (g_scan.pc0 < 0) return 0;
    g_scan.start = g_scan.f = start_khz;
    g_scan.end = end_khz;
    g_scan.span = (end_khz - start_khz) / 100 + 1;
    g_scan.nh = 0;
    g_scan.active = true;
    return 1;
}

int cinder_tuner_scan_step(void) {
    using namespace regmon;
    if (!g_scan.active) return -1;
    if (g_scan.f > g_scan.end || g_scan.nh >= 256) return -1;

    Plan& p = g_scan.plan;
    int chan = (g_scan.f - p.base_khz) / p.space_khz;
    if (chan >= 0 && chan <= CH_MASK) {
        pthread_mutex_lock(&g_lock);
        int s = -1;
        if (wr_l(R_CHANNEL, (unsigned)(chan | CH_TUNE))) {
            s = wait_stc_l();
            wr_l(R_CHANNEL, (unsigned)chan);
        }
        pthread_mutex_unlock(&g_lock);
        if (s < 0) return -1;                       // the bus went away; let the caller finish
        g_scan.hits[g_scan.nh].khz = g_scan.f;
        g_scan.hits[g_scan.nh].rssi = s & ST_RSSI;
        g_scan.nh++;
    }
    g_scan.f += 100;                                 // the raster the UI tunes on
    int done = g_scan.nh * 100 / (g_scan.span > 0 ? g_scan.span : 1);
    return done > 99 ? 99 : done;                    // 100 is reserved for "finished"
}

// Peak-pick what the sweep collected, put the chip back, and end the job. Safe to call at any time
// — a cancel is just this with whatever has been gathered so far.
int cinder_tuner_scan_collect(int* out_khz, int max) {
    using namespace regmon;
    if (!g_scan.active) return 0;
    pthread_mutex_lock(&g_lock);
    if (g_scan.pc0 >= 0) wr_l(R_POWERCFG, (unsigned)g_scan.pc0);   // unmute, as we found it
    pthread_mutex_unlock(&g_lock);
    g_scan.active = false;

    const int nh = g_scan.nh;
    if (!out_khz || max <= 0 || nh < 8) return 0;

    static int sorted[256];
    for (int i = 0; i < nh; i++) sorted[i] = g_scan.hits[i].rssi;
    for (int a = 1; a < nh; a++) {
        int v = sorted[a], b = a - 1;
        while (b >= 0 && sorted[b] > v) { sorted[b + 1] = sorted[b]; b--; }
        sorted[b + 1] = v;
    }
    const int floor_rssi = sorted[nh / 2];
    const int cut = floor_rssi + 3;
    g_seek_th = floor_rssi + 1;

    struct Peak { int khz, rssi; };
    static Peak peaks[64];
    int np = 0;
    for (int i = 0; i < nh; i++) {
        if (g_scan.hits[i].rssi < cut) continue;
        if (np > 0 && g_scan.hits[i].khz - peaks[np - 1].khz <= 200) {
            if (g_scan.hits[i].rssi > peaks[np - 1].rssi)
                peaks[np - 1] = { g_scan.hits[i].khz, g_scan.hits[i].rssi };
            continue;
        }
        if (np < 64) peaks[np++] = { g_scan.hits[i].khz, g_scan.hits[i].rssi };
    }
    for (int a = 1; a < np; a++) {
        Peak v = peaks[a]; int b = a - 1;
        while (b >= 0 && peaks[b].rssi < v.rssi) { peaks[b + 1] = peaks[b]; b--; }
        peaks[b + 1] = v;
    }
    int out = 0;
    for (int i = 0; i < np && out < max; i++) out_khz[out++] = peaks[i].khz;
    std::fprintf(stderr, "[cinder-tuner] chunked scan: %d steps, floor=%d cut=%d -> %d station(s)\n",
                 nh, floor_rssi, cut, out);
    return out;
}

int cinder_tuner_scan(int start_khz, int end_khz, int* out_khz, int max) {
    if (!out_khz || max <= 0) return 0;
    if (regmon::available()) {
        // The chip has to be powered for its registers to mean anything, and Sony's Open() owns
        // that sequence. If the radio is already playing this is a no-op on a live client.
        if (!ensure_clients()) return 0;
        bool opened = false;
        if (!g_playing) {
            route(true);
            try {
                ((fn_v)vslot(g_tuner, T_Open))(g_tuner);
                ((fn_v)vslot(g_tuner, T_Play))(g_tuner);
            } catch (...) { route(false); return 0; }
            opened = true;
        }
        int n = scan_hw(start_khz, end_khz, out_khz, max);
        if (opened) {
            try { ((fn_v)vslot(g_tuner, T_Stop))(g_tuner); } catch (...) {}
            try { ((fn_v)vslot(g_tuner, T_Close))(g_tuner); } catch (...) {}
            route(false);
        } else if (g_tuner) {
            // Put the service's idea of the frequency back where the user left it.
            unsigned uf = (unsigned)cinder_tuner_get_khz();
            if (uf) { try { ((fn_cu)vslot(g_tuner, T_SetFrequency))(g_tuner, &uf); } catch (...) {} }
        }
        return n;
    }
    if (!ensure_clients() || !alsa_load()) return 0;
    route(true);
    try {
        ((fn_v)vslot(g_tuner, T_Open))(g_tuner);
        ((fn_v)vslot(g_tuner, T_Play))(g_tuner);
    } catch (...) { route(false); return 0; }

    // The capture PCM is the instrument. AudioInPlayerService must not be playing — it owns
    // hw:0,1 and this open would fail with EBUSY.
    snd_pcm_t* pcm = nullptr;
    if (g_alsa.open(&pcm, "hw:0,1", /*CAPTURE*/1, 0) < 0) { route(false); return 0; }
    const unsigned RATE = 44100;
    // format 2 = S16_LE, access 3 = RW_INTERLEAVED (ALSA-stable values; see the shim header)
    if (g_alsa.set_params(pcm, 2, 3, 2, RATE, 1, 200000) < 0) {
        g_alsa.close(pcm); route(false); return 0;
    }
    g_alsa.start(pcm);   // this device does not start capture on its own

    // 60 ms of audio per step, not 200. MEASURED: the discriminator is a ratio, so a shorter
    // window costs precision, not validity — and 206 steps at 450 ms was 90 s, which is why the
    // scan felt nothing like Sony's (theirs uses the chip's own seek, which finds nothing here).
    const int FR = (int)RATE / 16;
    static short buf[(44100 / 5) * 2];
    struct Hit { int khz; double hf; };
    static Hit hits[256];
    int nh = 0;
    for (int f = start_khz; f <= end_khz && nh < 256; f += 100) {
        unsigned uf = (unsigned)f;
        try { ((fn_cu)vslot(g_tuner, T_SetFrequency))(g_tuner, &uf); } catch (...) { break; }
        usleep(80000);                            // settle; 80 ms is enough for the ratio
        g_alsa.drop(pcm); g_alsa.prepare(pcm); g_alsa.start(pcm);
        int got = 0;
        while (got < FR) {
            snd_pcm_sframes_t n = g_alsa.readi(pcm, buf + got * 2, FR - got);
            if (n < 0) { g_alsa.recover(pcm, (int)n, 1); break; }
            if (n == 0) break;
            got += (int)n;
        }
        if (g_progress) {
            int span = (end_khz - start_khz) / 100 + 1;
            g_progress(span > 0 ? ((f - start_khz) / 100) * 100 / span : 0);
        }
        if (got < FR / 2) continue;
        // First-difference high-pass proxy: white noise ~1.4, programme material far below.
        double sabs = 0, sdif = 0;
        int prev = buf[0];
        for (int k = 0; k < got; k++) {
            int v = buf[k * 2];
            sabs += (v < 0 ? -v : v);
            int d = v - prev; sdif += (d < 0 ? -d : d);
            prev = v;
        }
        hits[nh].khz = f;
        hits[nh].hf  = sabs > 0 ? sdif / sabs : 9.9;
        nh++;
    }
    g_alsa.close(pcm);
    try { ((fn_v)vslot(g_tuner, T_Stop))(g_tuner); } catch (...) {}
    try { ((fn_v)vslot(g_tuner, T_Close))(g_tuner); } catch (...) {}
    route(false);
    if (nh == 0) return 0;

    for (int a = 1; a < nh; a++) {                // sort by hf ascending
        Hit t = hits[a]; int b = a - 1;
        while (b >= 0 && hits[b].hf > t.hf) { hits[b + 1] = hits[b]; b--; }
        hits[b + 1] = t;
    }
    // Keep only what clearly beats the band's own baseline, and never adjacent steps of the same
    // carrier — a transmitter lights several 100 kHz steps and would otherwise fill the list.
    const double median = hits[nh / 2].hf, cut = median * 0.85;
    int out = 0;
    for (int i = 0; i < nh && out < max; i++) {
        if (hits[i].hf >= cut) break;
        bool adjacent = false;
        for (int j = 0; j < out; j++)
            if (std::abs(hits[i].khz - out_khz[j]) <= 200) { adjacent = true; break; }
        if (!adjacent) out_khz[out++] = hits[i].khz;
    }
    return out;
}

} // extern "C"
