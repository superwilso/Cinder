// analyzershim_selftest — host test for cinder-audio/src/analyzer_shim.cpp (docs/SHORTCOMINGS.md §A2).
//
// Sony's analyzer calls back through our listener's vtable: slot 3 is OnSpectrumUpdate, slot 2
// OnLevelUpdate (the RE in the shim's header). The fake service does exactly that, by slot number,
// so a reordered or extra virtual in the declaration fails here instead of on the player. The
// loader is faked as in storageshim_selftest. Pinned besides: the twelve default passbands, which
// settings reach the service, a table set before start pushed by start and one set while running
// pushed at once, the 24-band cap, the 256-value copy cap, and stop/start without a second resolve.
#include <cstdio>
#include <cstring>
#include <dlfcn.h>  // the real declarations first, so the macros below rename only the shim's calls
#include <vector>

static void* fake_dlopen(const char*, int);
static void* fake_dlsym(void*, const char*);
#define dlopen fake_dlopen
#define dlsym fake_dlsym
#include "../../cinder-audio/src/analyzer_shim.cpp"
#undef dlopen
#undef dlsym

static_assert(sizeof(aas::Passband) == 8, "Passband is {int; float}");

namespace {
int g_service;                 // the singleton GetInstance hands out
int g_dlsyms = 0;
const char* g_missing = nullptr;
int g_mode = -1, g_samples = -1, g_starts = 0, g_stops = 0, g_pushes = 0;
float g_rate = -1;
std::vector<aas::Passband> g_table;
void* g_listener_seen = nullptr;
std::vector<int> g_spectrum;   // what reached the renderer

void* getinst() { return &g_service; }
void setmode(void* s, int m) { if (s == &g_service) g_mode = m; }
void setrate(void* s, float r) { if (s == &g_service) g_rate = r; }
void setsamp(void* s, unsigned n) { if (s == &g_service) g_samples = static_cast<int>(n); }
void start(void* s, void* l) { if (s == &g_service) { g_listener_seen = l; ++g_starts; } }
void stop(void* s) { if (s == &g_service) ++g_stops; }
void passband(void* s, const std::vector<aas::Passband>* t) { if (s == &g_service) { g_table = *t; ++g_pushes; } }

// Sony's analyzer thread: listener->vtable[slot](listener, frame).
void sony_calls(int slot, const std::vector<int>& frame) {
    using cb = void (*)(void*, const std::vector<int>&);
    void** vtbl = *static_cast<void***>(g_listener_seen);
    reinterpret_cast<cb>(vtbl[slot])(g_listener_seen, frame);
}
} // namespace

extern "C" void cinder_set_spectrum(const int* bands, int n) { g_spectrum.assign(bands, bands + n); }

static void* fake_dlopen(const char*, int) { return &g_service; }
static void* fake_dlsym(void*, const char* name) {
    ++g_dlsyms;
    if (g_missing && std::strstr(name, g_missing)) return nullptr;
    struct { const char* part; void* fn; } table[] = {
        {"11GetInstanceEv", reinterpret_cast<void*>(&getinst)},
        {"7SetModeE", reinterpret_cast<void*>(&setmode)},
        {"13SetUpdateRateEf", reinterpret_cast<void*>(&setrate)},
        {"14SetCalcSamplesEj", reinterpret_cast<void*>(&setsamp)},
        {"5StartEPNS1_14IEventListenerE", reinterpret_cast<void*>(&start)},
        {"4StopEv", reinterpret_cast<void*>(&stop)},
        {"11SetPassbandERKNSt3__16vectorINS1_8PassbandENS3_9allocatorIS5_EEEE", reinterpret_cast<void*>(&passband)},
    };
    for (auto& e : table) {
        if (std::strstr(name, e.part)) return e.fn;
    }
    return nullptr;
}

static int fails = 0;
static void eq(const char* what, long got, long want) {
    std::printf("  %-4s %s (got %ld, want %ld)\n", got == want ? "ok" : "FAIL", what, got, want);
    if (got != want) fails = 1;
}

int main() {
    std::printf("a library without Start\n");
    g_missing = "5StartE";
    eq("start is -2", cinder_analyzer_start(CINDER_ANALYZER_SPECTRUM, 20.f, 0), -2);
    eq("not running", cinder_analyzer_is_running(), 0);
    g_resolved = false;
    g_missing = nullptr;

    std::printf("a table set before start\n");
    const cinder_passband_t mine[3] = {{100, 2.f}, {1000, 2.f}, {10000, 2.f}};
    eq("stored, 0", cinder_analyzer_set_bands(mine, 3), 0);
    eq("nothing pushed yet", g_pushes, 0);

    std::printf("start\n");
    g_dlsyms = 0;
    eq("returns 0", cinder_analyzer_start(CINDER_ANALYZER_SPECTRUM, 20.f, 0), 0);
    eq("mode reached the service", g_mode, CINDER_ANALYZER_SPECTRUM);
    eq("rate reached the service", static_cast<long>(g_rate), 20);
    eq("a 0 window leaves the service's", g_samples, -1);
    eq("start pushed the stored table", static_cast<long>(g_table.size()), 3);
    eq("its last centre", g_table.back().value, 10000);
    eq("a second start is -4", cinder_analyzer_start(CINDER_ANALYZER_SPECTRUM, 20.f, 0), -4);
    eq("Start reached once", g_starts, 1);

    std::printf("Sony calling back by slot\n");
    sony_calls(3, {5, 6, 7});
    eq("slot 3 reached the renderer", static_cast<long>(g_spectrum.size()), 3);
    eq("in order", g_spectrum[2], 7);
    eq("one frame counted", cinder_analyzer_frames(), 1);
    g_spectrum.clear();
    sony_calls(2, {1, 2});
    eq("slot 2 (levels) reaches nothing", static_cast<long>(g_spectrum.size()), 0);
    sony_calls(3, std::vector<int>(300, 9));
    eq("a frame is copied up to 256 values", static_cast<long>(g_spectrum.size()), 256);
    eq("but counted at its own size", cinder_analyzer_last(nullptr, 0), 300);
    unsigned ts;
    int gen, vals[4];
    eq("the log keeps it", cinder_analyzer_log_get(1, &ts, &gen, vals, 4), 256);

    std::printf("a table set while running\n");
    cinder_passband_t many[30];
    for (int i = 0; i < 30; ++i) many[i] = {100 * (i + 1), 1.f};
    const int before = g_pushes;
    eq("pushed at once, 0", cinder_analyzer_set_bands(many, 30), 0);
    eq("one push", g_pushes - before, 1);
    eq("capped at 24", static_cast<long>(g_table.size()), 24);
    eq("the window reaches the service", cinder_analyzer_set_window(4096), 0);
    eq("as given", g_samples, 4096);

    std::printf("stop and start again\n");
    cinder_analyzer_stop();
    eq("Stop reached", g_stops, 1);
    eq("not running", cinder_analyzer_is_running(), 0);
    eq("start again returns 0", cinder_analyzer_start(CINDER_ANALYZER_SPECTRUM, 0.f, 512), 0);
    eq("names resolved once", g_dlsyms, 7);
    eq("a 0 rate leaves the service's", static_cast<long>(g_rate), 20);

    std::printf("the default table\n");
    g_bands_n = 0;
    push_bands();
    eq("twelve bands", static_cast<long>(g_table.size()), 12);
    eq("from 32 Hz", g_table.front().value, 32);
    eq("to 16 kHz, below fs/2", g_table.back().value, 16000);
    eq("Q 1.75 x100", static_cast<long>(g_table[5].mean * 100 + 0.5f), 175);

    std::printf(fails ? "analyzershim_selftest: FAILED\n" : "analyzershim_selftest: all passed\n");
    return fails;
}
