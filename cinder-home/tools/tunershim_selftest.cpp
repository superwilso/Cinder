// tunershim_selftest — host test for cinder-audio/src/tuner_shim.cpp (docs/SHORTCOMINGS.md §A2).
//
// Two fakes. The Si4708 behind /proc/regmon (fopencookie streams: writing `target` selects a
// register, `value` reads or writes it), tuning, seeking and reporting RSSI the way the chip does.
// And Sony's two clients, called by vtable slot number, recording which slot each call took.
//
// Pinned: the slot sequence of start and stop (Open, SetFrequency, Play; AudioIn Play once the
// tuner streams; Stop, Close), the ADC route around both; the RE_fm_tuner.md register rules (only
// POWERCFG, CHANNEL, SYSCONFIG2's top byte and SYSCONFIG3 are ever written, every one put back);
// stock's powered chip left alone and Walkman One's powered down one powered, tuned and powered off
// again; a seek that stops on the 50 kHz shoulder snapped to the stronger raster neighbour and
// handed to the service; a scan that returns stations strongest first, not their shoulders, and
// takes its seek threshold from the band's floor; a chip that never finishes a tune ending a scan.
#include <cerrno>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <dlfcn.h>
#include <pthread.h>
#include <string>
#include <unistd.h>
#include <vector>

// ── the chip ────────────────────────────────────────────────────────────────────────────────
namespace chip {
const int BASE = 87500, SPACE = 50;          // BAND 0, SPACE 2 (50 kHz), as Sony's driver sets it
unsigned reg[16];
int target = 0;
bool stuck = false;                          // never completes a tune
std::vector<std::pair<int, unsigned>> writes;

int rssi(int khz) {
    switch (khz) {
        case 91450: return 10;               // the shoulder a 50 kHz seek stops on
        case 91500: return 12;
        case 91600: return 9;
        case 96000: return 14;
        default: return 5;                   // the floor
    }
}
int khz_of(unsigned chan) { return BASE + static_cast<int>(chan & 0x3FF) * SPACE; }
void settle(unsigned chan) {
    reg[0x0B] = chan & 0x3FF;
    reg[0x0A] = (stuck ? 0 : 1u << 14) | static_cast<unsigned>(rssi(khz_of(chan)));
}
void reset(unsigned powercfg) {
    std::memset(reg, 0, sizeof reg);
    reg[0x00] = 0x1242;                      // DEVICEID
    reg[0x02] = powercfg;
    reg[0x05] = 0x1220;                      // SEEKTH 18 (stock), BAND 0, SPACE 2
    reg[0x06] = 0x0000;
    writes.clear();
    stuck = false;
}
void write(int r, unsigned v) {
    writes.push_back({r, v});
    if (r == 0x03) {
        if (v & 0x8000) settle(v);
        else reg[0x0A] &= ~(1u << 14);
    } else if (r == 0x02) {
        if ((v & (1u << 8)) && !(reg[0x02] & (1u << 8))) {   // SEEK rising: walk the band
            const int th = static_cast<int>(reg[0x05] >> 8), dir = (v & (1u << 9)) ? 1 : -1;
            unsigned c = reg[0x0B];
            bool hit = false;
            for (int i = 0; i < 410 && !hit; i++) {
                c = (c + dir) & 0x3FF;
                if (khz_of(c) > 108000 || khz_of(c) < BASE) break;
                hit = rssi(khz_of(c)) >= th;
            }
            reg[0x0B] = c;
            reg[0x0A] = (1u << 14) | (hit ? 0 : 1u << 13) | static_cast<unsigned>(rssi(khz_of(c)));
        } else if (!(v & (1u << 8))) {
            reg[0x0A] &= ~((1u << 14) | (1u << 13));
        }
    }
    reg[r] = v & 0xFFFF;
}

struct Node { bool is_target; std::string out; size_t at = 0; std::string in; };
ssize_t node_read(void* c, char* buf, size_t n) {
    Node* node = static_cast<Node*>(c);
    if (node->out.empty()) {
        char tmp[16];
        std::snprintf(tmp, sizeof tmp, "0x%08X\n", reg[target]);
        node->out = tmp;
    }
    size_t m = node->out.size() - node->at < n ? node->out.size() - node->at : n;
    std::memcpy(buf, node->out.data() + node->at, m);
    node->at += m;
    return static_cast<ssize_t>(m);
}
ssize_t node_write(void* c, const char* buf, size_t n) {
    static_cast<Node*>(c)->in.append(buf, n);
    return static_cast<ssize_t>(n);
}
int node_close(void* c) {
    Node* node = static_cast<Node*>(c);
    if (!node->in.empty()) {
        unsigned v = static_cast<unsigned>(std::strtoul(node->in.c_str(), nullptr, 16));
        if (node->is_target) target = static_cast<int>(v & 0xF);
        else write(target, v);
    }
    delete node;
    return 0;
}
} // namespace chip

static FILE* fake_fopen(const char* path, const char* mode) {
    const bool t = !std::strcmp(path, "/proc/regmon/Si4708icx/target");
    if (!t && std::strcmp(path, "/proc/regmon/Si4708icx/value")) return std::fopen(path, mode);
    cookie_io_functions_t io = {chip::node_read, chip::node_write, nullptr, chip::node_close};
    return fopencookie(new chip::Node{t, {}, 0, {}}, mode, io);
}
static int fake_access(const char* path, int) {
    return std::strncmp(path, "/proc/regmon/Si4708icx/", 23) ? -1 : 0;
}
std::vector<std::string> g_shell;
static int fake_system(const char* cmd) { g_shell.push_back(cmd); return 0; }
static int fake_usleep(useconds_t) { return 0; }

// ── Sony's clients ──────────────────────────────────────────────────────────────────────────
std::vector<std::string> g_calls;
unsigned g_service_khz = 0;
template <char W, int S> int slot(void*, unsigned* arg) {
    std::string s = std::string(1, W) + std::to_string(S);
    if (W == 'T' && S == 17) { g_service_khz = *arg; s += ":" + std::to_string(*arg); }
    if (W == 'T' && S == 16) *arg = g_service_khz;
    if (W == 'T' && S == 12) *reinterpret_cast<int*>(arg) = 0;
    g_calls.push_back(s);
    return W == 'A' && S == 6 ? 2 : 0;
}
template <char W, int... S> struct Client {
    void* vtbl[20] = {};
    void** self = vtbl;
    Client() { ((vtbl[S] = reinterpret_cast<void*>(&slot<W, S>)), ...); }
};
Client<'T', 3, 4, 5, 6, 7, 12, 16, 17> g_tuner_obj;
Client<'A', 3, 5, 6> g_ain_obj;
void* tuner_factory() { return &g_tuner_obj.self; }
void* ain_factory() { return &g_ain_obj.self; }
static void* fake_dlopen(const char*, int) { return &g_calls; }
static void* fake_dlsym(void*, const char* name) {
    if (std::strstr(name, "TunerPlayerServiceClientFactory")) return reinterpret_cast<void*>(&tuner_factory);
    if (std::strstr(name, "AudioInPlayerServiceClientFactory")) return reinterpret_cast<void*>(&ain_factory);
    return nullptr;                          // no ALSA: the hardware paths only
}
namespace std { using ::fake_fopen; using ::fake_system; }

#define fopen fake_fopen
#define access fake_access
#define system fake_system
#define usleep fake_usleep
#define dlopen fake_dlopen
#define dlsym fake_dlsym
#include "../../cinder-audio/src/tuner_shim.cpp"
#undef fopen
#undef access
#undef system
#undef usleep
#undef dlopen
#undef dlsym

static int fails = 0;
static void eq(const char* what, long got, long want) {
    std::printf("  %-4s %s (got %ld, want %ld)\n", got == want ? "ok" : "FAIL", what, got, want);
    if (got != want) fails = 1;
}
static void seq(const char* what, const std::vector<std::string>& got, const std::vector<std::string>& want) {
    std::string g, w;
    for (auto& s : got) g += s + " ";
    for (auto& s : want) w += s + " ";
    std::printf("  %-4s %s (got %s, want %s)\n", g == w ? "ok" : "FAIL", what, g.c_str(), w.c_str());
    if (g != w) fails = 1;
}
// The register rules: only these four, SYSCONFIG2's low byte (BAND/SPACE) as the driver set it.
static void rules(const char* what) {
    long bad = 0;
    for (auto& w : chip::writes) {
        if (w.first != 0x02 && w.first != 0x03 && w.first != 0x05 && w.first != 0x06) bad++;
        if (w.first == 0x05 && (w.second & 0xFF) != 0x20) bad++;
    }
    eq(what, bad, 0);
}
static int writes_to(int r) {
    int n = 0;
    for (auto& w : chip::writes) n += w.first == r;
    return n;
}

int main() {
    std::printf("stock: the chip is up after Open\n");
    chip::reset(0x4001);
    eq("start returns 0", cinder_tuner_start(91500), 0);
    seq("the slots, in order", g_calls, {"T4", "T17:91500", "T6", "A3", "A6", "T17:91500"});
    seq("the ADC routed to the tuner, no helper needed", g_shell, {"amixer -c0 cset numid=26 1 >/dev/null 2>&1"});
    eq("POWERCFG left alone", writes_to(0x02), 0);
    g_calls.clear();
    eq("stop returns 0", cinder_tuner_stop(), 0);
    seq("AudioIn stops first, then the tuner", g_calls, {"A5", "T7", "T5"});
    eq("the ADC route taken off", g_shell.back() == "amixer -c0 cset numid=26 0 >/dev/null 2>&1", 1);
    eq("still never powered", writes_to(0x02), 0);
    eq("stereo from the chip's ST bit, off", cinder_tuner_stereo(), 0);
    eq("signal from STATUS", cinder_tuner_signal(), static_cast<long>(chip::reg[0x0A] & 0xFF));

    std::printf("Walkman One: the service is a stub and the chip sits powered down\n");
    chip::reset(0x2000);
    g_calls.clear();
    cinder_tuner_start(96000);
    eq("powered up with ENABLE|DMUTE", chip::writes.front().second, 0x4001);
    eq("tuned through the registers", cinder_tuner_get_khz(), 96000);
    cinder_tuner_set_khz(91500);
    eq("set_khz tunes the chip too", cinder_tuner_get_khz(), 91500);
    cinder_tuner_stop();
    eq("powered down again on stop", chip::reg[0x02], 0x0041);
    rules("only the allowed registers were written");

    std::printf("hardware seek\n");
    chip::reset(0x4001);
    g_calls.clear();
    const int got = cinder_tuner_seek(87500, 1, nullptr);
    eq("stopped on 91.45 and snapped to the stronger 91.5", got, 91500);
    eq("the service told", static_cast<long>(g_service_khz), 91500);
    eq("SYSCONFIG2 put back as found", chip::reg[0x05], 0x1220);
    eq("SYSCONFIG3 put back as found", chip::reg[0x06], 0x0000);
    eq("POWERCFG put back as found", chip::reg[0x02], 0x4001);
    eq("the threshold was lowered for the walk", (chip::writes[2].first == 0x05) ? chip::writes[2].second >> 8 : -1, 6);
    rules("only the allowed registers were written");

    std::printf("chunked seek, down\n");
    chip::reset(0x4001);
    eq("begins", cinder_tuner_seek_begin(98000, -1), 1);
    int cur = 0, r = 0;
    while ((r = cinder_tuner_seek_step(&cur)) == 0) {}
    eq("lands on 96.0, the next station down", r, 96000);
    eq("POWERCFG put back", chip::reg[0x02], 0x4001);
    eq("SYSCONFIG2 put back", chip::reg[0x05], 0x1220);
    eq("SYSCONFIG3 put back", chip::reg[0x06], 0x0000);
    rules("only the allowed registers were written");

    std::printf("chunked scan\n");
    chip::reset(0x4001);
    eq("begins", cinder_tuner_scan_begin(87500, 108000), 1);
    eq("muted for the sweep", chip::reg[0x02] & (1u << 14), 0);
    int pct = 0, last = 0;
    while ((pct = cinder_tuner_scan_step()) >= 0) last = pct;
    eq("progress stops short of 100", last, 99);
    int out[8] = {};
    eq("two stations", cinder_tuner_scan_collect(out, 8), 2);
    eq("strongest first", out[0], 96000);
    eq("then the other, not its shoulder", out[1], 91500);
    eq("unmuted, as found", chip::reg[0x02], 0x4001);
    eq("seek threshold is the floor + 1", regmon::g_seek_th, 6);
    rules("only the allowed registers were written");

    std::printf("a chip that never finishes a tune\n");
    chip::reset(0x4001);
    chip::stuck = true;
    cinder_tuner_scan_begin(87500, 108000);
    int n = 0;
    while (cinder_tuner_scan_step() >= 0) n++;
    eq("the scan ends after five channels", n, 4);
    eq("and finds nothing", cinder_tuner_scan_collect(out, 8), 0);

    std::printf(fails ? "tunershim_selftest: FAILED\n" : "tunershim_selftest: all passed\n");
    return fails;
}
