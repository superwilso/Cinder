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
    if (!g_tuner) return -1;
    int st = -1;
    try { ((fn_pi)vslot(g_tuner, T_GetStereoState))(g_tuner, &st); } catch (...) { return -1; }
    return st;
}

void cinder_tuner_set_progress_cb(cinder_tuner_progress_fn cb) { g_progress = cb; }

int cinder_tuner_seek(int from_khz, int dir, cinder_tuner_step_fn on_step) {
    if (!g_tuner || !alsa_load()) return 0;
    if (dir == 0) dir = 1;
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

int cinder_tuner_scan(int start_khz, int end_khz, int* out_khz, int max) {
    if (!out_khz || max <= 0) return 0;
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
