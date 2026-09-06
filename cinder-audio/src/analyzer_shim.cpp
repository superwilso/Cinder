// analyzer_shim.cpp — implements cinder_analyzer.h over Sony's built-in spectrum analyzer,
// libAudioAnalyzerServiceClient.so (pst::services::audioanalyzerservice::AudioAnalyzerService).
//
// WHY dlopen (not link): the EQ effect shim links libEffectCtrlDmp directly because EQ is core.
// The visualiser is OPTIONAL eye-candy and this service is the riskier, device-only path, so we
// resolve it at runtime — a missing/renamed .so then just disables the feature instead of breaking
// cinder-home's dynamic load (which, on the boot path, would mean a failed boot). See the header.
//
// HOW it works: Sony's analyzer does the FFT itself and pushes per-band magnitudes to a listener
// via OnSpectrumUpdate(const vector<int>&). We register a faithful IEventListener and forward each
// frame to cinder_set_spectrum() — Sony's exact path for the stock spectrum-analyzer screen.
//
// THE ONE ABI WE REPRODUCE — the IEventListener vtable. RE of libAudioAnalyzerService.so
// (AudioAnalyzerServiceServiceImpl's secondary base at +4) gives the listener sub-object vtable:
//     [0] ~IEventListener (complete-object dtor, D1)
//     [1] deleting dtor   (D0)
//     [2] OnLevelUpdate(const std::vector<int>&)
//     [3] OnSpectrumUpdate(const std::vector<int>&)
// which is exactly the libc++ layout the declaration below produces (virtual dtor, then the two
// virtuals in declaration order). AudioAnalyzerService::Start(IEventListener*) just stores the
// pointer; the analyzer thread later calls listener->vtable[3](spectrum). Compile with the same
// clang -stdlib=libc++ (libc++ 3.9 ABI) as the other shims so std::vector<int> matches Sony's.
//
// THREADING: OnSpectrumUpdate fires on the analyzer's IPC thread, not the render pump. cinder_set_*
// takes the renderer's global mutex, so this is safe; the critical section is a 36-int copy.
#include <cstdlib>
#include <cstring>
#include <ctime>
#include <dlfcn.h>
#include <vector>
#include <csignal>
#include <pthread.h>

#include "cinder_analyzer.h"

// From libcinder_ffi (linked into cinder-home): push pre-computed spectrum bands to the renderer.
extern "C" void cinder_set_spectrum(const int* bands, int n);

namespace pst { namespace services { namespace audioanalyzerservice {

// One analysis band. `value` is the band's centre frequency in Hz; `mean` is a per-band scaling
// reference the service wants alongside it. Layout is {int; float} = 8 bytes, POD — confirmed
// against Sony's own client library and against wampy's independently-derived declaration.
struct Passband {
    int value;
    float mean;
};

// Faithful re-declaration of Sony's listener interface. Only the vtable layout matters; we never
// link Sony's IEventListener (our derived class is self-contained). The dtor is defined out-of-line
// (below) so this TU emits the vtable.
class IEventListener {
public:
    virtual ~IEventListener();
    virtual void OnLevelUpdate(const std::vector<int>& levels) = 0;
    virtual void OnSpectrumUpdate(const std::vector<int>& spectrum) = 0;
};
IEventListener::~IEventListener() {}

} } } // namespace

namespace aas = pst::services::audioanalyzerservice;

namespace {

// Diagnostics for cinder-probe --analyzer (on-device validation + calibration of Sony's units):
// how many spectrum frames have arrived, and a snapshot of the most recent one. Declared before
// the listener so its inline OnSpectrumUpdate can reference them.
volatile int g_frames = 0;
int g_last_n = 0;
int g_last_vals[16] = {0};

// ── frame log ────────────────────────────────────────────────────────────────────────────────
// The most recent frames, each stamped with the arrival time and the passband GENERATION that was
// current when it arrived. Sony's units, emit rate and post-SetPassband settling are all things we
// were guessing at from a single 8-value snapshot; a timestamped ring makes them measurable on
// device (cinder-probe --vizlab). Written only by the analyzer thread, read only by the probe —
// a torn read costs one wrong diagnostic line, never correctness, so it stays lock-free.
const int LOG_CAP   = 256;   // frames kept
const int LOG_BANDS = 24;    // values kept per frame (>= any table we install)
struct LogFrame { unsigned ts_ms; int gen; int n; int v[LOG_BANDS]; };
LogFrame g_log[LOG_CAP];
volatile int g_log_write = 0;   // total frames written since reset (index = % LOG_CAP)
volatile int g_gen = 0;         // bumped by every cinder_analyzer_set_bands

unsigned now_ms() {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return static_cast<unsigned>(ts.tv_sec * 1000u + ts.tv_nsec / 1000000u);
}

// Block SIGALRM on the CURRENT thread (called from the analyzer's monitor thread, where the
// callbacks run). This keeps the shell's per-frame/guard watchdog (SIGALRM) from being delivered to
// the analyzer thread — it always reaches the pump thread, which is the one that armed alarm(). We
// do this from the thread itself (never the pump thread), so it can't defeat any guard.
//
// THREAD-LOCAL, not a plain static. The flag used to be process-wide, which was correct back when
// the analyzer started once at boot and its thread lived for the whole session. It is now started
// and stopped on demand — every screen blank, pause and screen wake — so if Sony's Start() hands
// the callbacks to a FRESH thread each time, only the very first one would ever have masked the
// signal and every later analyzer thread would be a valid target for the shell's watchdog alarm.
// Per-thread state costs one TLS slot and makes the guarantee actually hold.
static void mask_sigalrm_self_once() {
    static thread_local bool done = false;
    if (done) return;
    done = true;
    sigset_t block;
    sigemptyset(&block);
    sigaddset(&block, SIGALRM);
    pthread_sigmask(SIG_BLOCK, &block, nullptr);
}

// Our concrete listener: forward spectrum frames to the renderer; ignore the level callback.
class CinderListener : public aas::IEventListener {
public:
    void OnLevelUpdate(const std::vector<int>& /*levels*/) override { mask_sigalrm_self_once(); }
    void OnSpectrumUpdate(const std::vector<int>& spectrum) override {
        mask_sigalrm_self_once();
        const std::size_t n = spectrum.size();
        if (n == 0) return;
        // Copy to a flat buffer (the FFI takes const int*); cap to keep the critical section small.
        int buf[256];
        const std::size_t m = n > 256 ? 256 : n;
        for (std::size_t i = 0; i < m; ++i) buf[i] = spectrum[i];
        cinder_set_spectrum(buf, static_cast<int>(m));
        // diagnostics snapshot (lock-free; probe only reads it)
        g_last_n = static_cast<int>(n);
        for (int i = 0; i < 16; ++i) g_last_vals[i] = i < static_cast<int>(m) ? buf[i] : 0;
        LogFrame& f = g_log[g_log_write % LOG_CAP];
        f.ts_ms = now_ms();
        f.gen   = g_gen;
        f.n     = static_cast<int>(m);
        for (int i = 0; i < LOG_BANDS; ++i) f.v[i] = i < static_cast<int>(m) ? buf[i] : 0;
        g_log_write++;
        g_frames++;
    }
};

CinderListener g_listener;   // static lifetime: outlives Start..Stop (we never dlclose)
void* g_lib   = nullptr;     // dlopen handle (kept for process lifetime)
void* g_inst  = nullptr;     // AudioAnalyzerService* from GetInstance (Sony-allocated singleton)
bool  g_running = false;

// Resolved entry points (members called via dlsym + the AAPCS `this`-in-r0 convention).
using fn_getinst = void* (*)();
using fn_void    = void  (*)(void*);
using fn_mode    = void  (*)(void*, int);
using fn_rate    = void  (*)(void*, float);
using fn_samples = void  (*)(void*, unsigned);
using fn_start   = void  (*)(void*, void*);
// SetPassband(const std::vector<Passband>&) — a libc++ vector crosses this boundary, which is fine
// because this shim is built with the same clang -stdlib=libc++ (libc++ 3.9 ABI) as Sony's libs.
using fn_passband = void (*)(void*, const std::vector<aas::Passband>*);

// Resolved once, on the first start, and reused. The analyzer is now started and stopped on demand
// (see the shell's viz_analyzer_tick), so this runs on every entry to Now Playing rather than once
// at boot — and dlsym over a Sony client library's symbol table is not free on a single-core ARMv7
// competing with the render thread. Resolution is all-or-nothing, so caching cannot half-apply.
fn_getinst p_getinst = nullptr;
fn_mode    p_setmode = nullptr;
fn_rate    p_setrate = nullptr;
fn_samples p_setsamp = nullptr;
fn_start    p_start    = nullptr;
fn_passband p_passband = nullptr;
fn_void     p_stop     = nullptr;
bool       g_resolved = false;

// ── the passband table ───────────────────────────────────────────────────────────────────────
// TWELVE log-spaced centres from 32 Hz to 16 kHz (a constant ratio of 500^(1/11) = 1.76, i.e.
// 0.81 octave per step) with the Q that makes those filters MEET rather than leave gaps: for a
// step ratio r, contiguous coverage wants Q = 1 / (sqrt(r) - 1/sqrt(r)) = 1.75.
//
// This replaces the table Sony's own player uses (50..28000 Hz at Q=456), which was reproduced
// here on the reasoning that a measured value beats an invented one. Two things were wrong with
// it once the service was disassembled: `mean` is the filter's Q, so 456 means every band is a
// ~1/300-octave needle — twelve tone detectors with almost the whole spectrum falling between
// them, which is why the magnitudes swung three decades frame to frame — and its top band at
// 28 kHz is above fs/2, which the service answers by zeroing that band's coefficients, so one of
// the twelve columns could never be anything but 0.
const int   DEFAULT_N = 12;
const cinder_passband_t DEFAULT_BANDS[DEFAULT_N] = {
    {   32, 1.75f }, {   56, 1.75f }, {  100, 1.75f }, {  175, 1.75f },
    {  305, 1.75f }, {  540, 1.75f }, {  950, 1.75f }, { 1670, 1.75f },
    { 2930, 1.75f }, { 5160, 1.75f }, { 9080, 1.75f }, { 16000, 1.75f },
};

const int MAX_BANDS = 24;              // what we will hand the service; it uses the first 12
cinder_passband_t g_bands[MAX_BANDS];
int g_bands_n = 0;                     // 0 = never set, use DEFAULT_BANDS

// Push the current table to the service. Returns 0 on success, or the same negative stage codes
// as start(). Bumps the generation so the frame log can attribute frames to a table.
int push_bands() {
    if (!p_passband) return -2;
    if (!g_inst) return -3;
    const cinder_passband_t* src = g_bands_n > 0 ? g_bands : DEFAULT_BANDS;
    const int n = g_bands_n > 0 ? g_bands_n : DEFAULT_N;
    std::vector<aas::Passband> bands;
    bands.reserve(static_cast<std::size_t>(n));
    for (int i = 0; i < n; ++i) bands.push_back(aas::Passband{ src[i].hz, src[i].q });
    g_gen++;
    p_passband(g_inst, &bands);
    return 0;
}

} // namespace

extern "C" {

int cinder_analyzer_start(int mode, float update_hz, unsigned calc_samples) {
    if (g_running) return -4;

    if (!g_lib) {
        // Try the soname (resolved via LD_LIBRARY_PATH = /vendor/sony/lib on device) then absolutes.
        const char* paths[] = {
            "libAudioAnalyzerServiceClient.so",
            "/vendor/sony/lib/libAudioAnalyzerServiceClient.so",
            "/system/vendor/sony/lib/libAudioAnalyzerServiceClient.so",
        };
        for (const char* p : paths) {
            g_lib = dlopen(p, RTLD_NOW | RTLD_GLOBAL);
            if (g_lib) break;
        }
        if (!g_lib) return -1;
    }

    if (!g_resolved) {
        p_getinst = reinterpret_cast<fn_getinst>(dlsym(g_lib,
            "_ZN3pst8services20audioanalyzerservice20AudioAnalyzerService11GetInstanceEv"));
        p_setmode = reinterpret_cast<fn_mode>(dlsym(g_lib,
            "_ZN3pst8services20audioanalyzerservice20AudioAnalyzerService7SetModeENS1_6mode_tE"));
        p_setrate = reinterpret_cast<fn_rate>(dlsym(g_lib,
            "_ZN3pst8services20audioanalyzerservice20AudioAnalyzerService13SetUpdateRateEf"));
        p_setsamp = reinterpret_cast<fn_samples>(dlsym(g_lib,
            "_ZN3pst8services20audioanalyzerservice20AudioAnalyzerService14SetCalcSamplesEj"));
        p_start   = reinterpret_cast<fn_start>(dlsym(g_lib,
            "_ZN3pst8services20audioanalyzerservice20AudioAnalyzerService5StartEPNS1_14IEventListenerE"));
        p_passband = reinterpret_cast<fn_passband>(dlsym(g_lib,
            "_ZN3pst8services20audioanalyzerservice20AudioAnalyzerService11SetPassbandERKNSt3__1"
            "6vectorINS1_8PassbandENS3_9allocatorIS5_EEEE"));
        p_stop    = reinterpret_cast<fn_void>(dlsym(g_lib,
            "_ZN3pst8services20audioanalyzerservice20AudioAnalyzerService4StopEv"));
        g_resolved = true;
    }
    auto getinst = p_getinst;
    auto setmode = p_setmode;
    auto setrate = p_setrate;
    auto setsamp = p_setsamp;
    auto start   = p_start;

    if (!getinst || !start || !p_stop) return -2;   // hard requirements

    if (!g_inst) g_inst = getinst();
    if (!g_inst) return -3;

    // Configure only what the caller asked for; otherwise inherit the service defaults. SetMode
    // picks level vs spectrum.
    if (setmode)                    setmode(g_inst, mode);
    if (setsamp && calc_samples)    setsamp(g_inst, calc_samples);
    if (setrate && update_hz > 0.f) setrate(g_inst, update_hz);

    // SET THE PASSBANDS. Without this the service has nothing to analyse and emits no frames at
    // all. The table is DEFAULT_BANDS unless the caller installed one (cinder_analyzer_set_bands).
    //
    // The twelve-band ceiling is the service's, and it is structural rather than a validation
    // check: SpectrumAnalyzer's CONSTRUCTOR walks a hardcoded 12-entry default list and allocates
    // ceil(12/5) = 3 level-detector objects, and SetPassband only re-assigns the vector — it never
    // makes another detector. A 13th band therefore has nothing to run in and is ignored.
    push_bands();

    // NOTE: do NOT mask SIGALRM around start() here — start() runs inside the shell's run_guarded
    // (alarm-based watchdog), so blocking SIGALRM on THIS (pump) thread would defeat that watchdog
    // and a hung start() would hang the boot. Instead the analyzer thread masks SIGALRM on itself
    // from inside the first callback (see CinderListener), which can't touch the pump thread. The
    // shell's thread-owner fault check is the actual safety net regardless.
    start(g_inst, static_cast<aas::IEventListener*>(&g_listener));
    g_running = true;
    return 0;
}

void cinder_analyzer_stop(void) {
    if (g_running && g_inst && p_stop) {
        p_stop(g_inst);
    }
    g_running = false;
}

int cinder_analyzer_is_running(void) { return g_running ? 1 : 0; }

int cinder_analyzer_frames(void) { return g_frames; }

int cinder_analyzer_set_bands(const cinder_passband_t* bands, int n) {
    if (!bands || n <= 0) return -2;
    if (n > MAX_BANDS) n = MAX_BANDS;
    for (int i = 0; i < n; ++i) g_bands[i] = bands[i];
    g_bands_n = n;
    // Before start there is no instance yet: the table is stored and start() pushes it. While
    // running, the service recomputes the filter coefficients in place (its SetPassband takes the
    // stream lock and calls UpdateCoefSet itself), so the swap needs no stop/start.
    if (!g_inst) return 0;
    return push_bands();
}

int cinder_analyzer_set_window(unsigned calc_samples) {
    if (!calc_samples) return 0;
    if (!p_setsamp) return -2;
    if (!g_inst) return -3;
    p_setsamp(g_inst, calc_samples);
    return 0;
}

int cinder_analyzer_log_count(void) { return g_log_write; }

void cinder_analyzer_log_reset(void) { g_log_write = 0; }

int cinder_analyzer_log_get(int idx, unsigned* ts_ms, int* gen, int* vals, int max) {
    const int total = g_log_write;
    if (idx < 0 || idx >= total) return 0;
    // Only the last LOG_CAP frames still exist; older indices have been overwritten.
    const int first = total > LOG_CAP ? total - LOG_CAP : 0;
    if (idx < first) return 0;
    const LogFrame& f = g_log[idx % LOG_CAP];
    if (ts_ms) *ts_ms = f.ts_ms;
    if (gen)   *gen   = f.gen;
    if (vals && max > 0) {
        int m = max < LOG_BANDS ? max : LOG_BANDS;
        for (int i = 0; i < m; ++i) vals[i] = f.v[i];
    }
    return f.n;
}

int cinder_analyzer_last(int* out, int max) {
    if (out && max > 0) {
        int m = max < 16 ? max : 16;
        for (int i = 0; i < m; ++i) out[i] = g_last_vals[i];
    }
    return g_last_n;
}

} // extern "C"
