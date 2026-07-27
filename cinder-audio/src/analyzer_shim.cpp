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
#include <dlfcn.h>
#include <vector>
#include <csignal>
#include <pthread.h>

// From libcinder_ffi (linked into cinder-home): push pre-computed spectrum bands to the renderer.
extern "C" void cinder_set_spectrum(const int* bands, int n);

namespace pst { namespace services { namespace audioanalyzerservice {

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

// Resolved once, on the first start, and reused. The analyzer is now started and stopped on demand
// (see the shell's viz_analyzer_tick), so this runs on every entry to Now Playing rather than once
// at boot — and dlsym over a Sony client library's symbol table is not free on a single-core ARMv7
// competing with the render thread. Resolution is all-or-nothing, so caching cannot half-apply.
fn_getinst p_getinst = nullptr;
fn_mode    p_setmode = nullptr;
fn_rate    p_setrate = nullptr;
fn_samples p_setsamp = nullptr;
fn_start   p_start   = nullptr;
fn_void    p_stop    = nullptr;
bool       g_resolved = false;

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

    // Configure only what the caller asked for; otherwise inherit the service defaults (same params
    // the stock spectrum screen uses). SetMode picks level vs spectrum.
    if (setmode)                    setmode(g_inst, mode);
    if (setsamp && calc_samples)    setsamp(g_inst, calc_samples);
    if (setrate && update_hz > 0.f) setrate(g_inst, update_hz);

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

int cinder_analyzer_last(int* out, int max) {
    if (out && max > 0) {
        int m = max < 16 ? max : 16;
        for (int i = 0; i < m; ++i) out[i] = g_last_vals[i];
    }
    return g_last_n;
}

} // extern "C"
