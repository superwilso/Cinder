// cinder-home — make Cinder a *valid* easel "Home" app so app-manager launches it
// instead of the stock Qt UI, completing the Foreground handshake so the device does
// NOT reboot (see analysis/F_appmgr_home/RE_findings.md for the protocol).
//
// Strategy: don't subclass a fragile vtable — use the non-Qt easel::CuiAppModule and
// hand it std::function callbacks. The module + run() perform the appmgr connect and
// the lifecycle ACKs; our job is to start painting the framebuffer at foreground and
// tick the renderer on the pump.
//
// STATUS (2026-06-24): builds + LOADS + enters the easel lifecycle on the real device
// (first run reached "start ToInitialize"). MUST be built with **libc++ 3.9.0 headers**
// to match the device's libcxx-3.9.0 std::function/string layout — clang-18's libc++18
// std::function is a different size (24B vs newer), so the CuiAppModule ctor (which reads
// std::function internals at fixed offsets) corrupts its sub-objects and hangs in
// OnInitialize. See build.sh + README. This file is instrumented (clog) to trace the
// lifecycle in /contents/cinderhome.log.
#include "easel_abi.hpp"
#include <memory>
#include <cstdio>
#include <cstdint>
#include <cstdlib>
#include <csignal>
#include <cstring>
#include <ucontext.h>
#include <unistd.h>
#include <execinfo.h>
#include <fcntl.h>
#include <dirent.h>

// The render core: the Rust Cinder UI, built as a glibc C-ABI staticlib
// (player/cinder-ffi -> libcinder_ffi.a). C ABI, so the renderer stays in Rust while
// this shell stays C++/libc++. See player/cinder-ffi/include/cinder.h.
#include "cinder.h"
// The playback-control shim over Sony's PlayerService (cinder-audio/player_shim.cpp).
#include "cinder_audio.h"

namespace {

// Lifecycle tracer -> /contents/cinderhome.log (the launcher redirects stdout/stderr there).
// Flushed every line so a hang/crash still leaves the last reached step on disk.
void clog_(const char* m) { std::fprintf(stderr, "[cinder-home] %s\n", m); std::fflush(stderr); }

// ---- crash + hang diagnostics ----------------------------------------------------------
// The first device runs hang inside the device's CuiAppModule::OnInitialize, which we can't
// instrument from source. This handler captures the EXACT location: on a fatal signal OR a
// watchdog SIGALRM (single-threaded, so SIGALRM interrupts the hung instruction), it logs the
// PC/LR + the executable mappings, so the PC maps to a .so+offset -> the decompiled function.
void dump_maps() {
    FILE* m = std::fopen("/proc/self/maps", "r");
    if (!m) return;
    char line[512];
    std::fprintf(stderr, "--- /proc/self/maps (exec regions) ---\n");
    while (std::fgets(line, sizeof line, m))
        if (std::strstr(line, "r-xp")) std::fprintf(stderr, "%s", line);
    std::fclose(m);
    std::fflush(stderr);
}
void crash_handler(int sig, siginfo_t* si, void* uc_) {
    unsigned long pc = 0, lr = 0;
#if defined(__arm__)
    ucontext_t* uc = static_cast<ucontext_t*>(uc_);
    pc = uc->uc_mcontext.arm_pc; lr = uc->uc_mcontext.arm_lr;
#endif
    std::fprintf(stderr, "[cinder-home] *** %s : PC=0x%08lx LR=0x%08lx addr=%p ***\n",
                 (sig == SIGALRM ? "WATCHDOG (hung ~20s)" : "FATAL SIGNAL"),
                 pc, lr, si ? si->si_addr : (void*)0);
    void* bt[24];
    int n = backtrace(bt, 24);
    std::fprintf(stderr, "--- backtrace (%d frames) ---\n", n);
    backtrace_symbols_fd(bt, n, 2 /*stderr*/);
    std::fflush(stderr);
    dump_maps();
    _exit(42);  // die fast -> appmgr reboots -> bad-boot counter reverts to stock
}
void install_diagnostics() {
    struct sigaction sa; std::memset(&sa, 0, sizeof sa);
    sa.sa_sigaction = crash_handler; sa.sa_flags = SA_SIGINFO;
    sigemptyset(&sa.sa_mask);
    for (int s : {SIGSEGV, SIGBUS, SIGABRT, SIGILL, SIGFPE, SIGALRM}) sigaction(s, &sa, nullptr);
    alarm(20);  // if not cancelled, fire the watchdog before appmgr's own timeout
}

bool g_render_ready = false;   // renderer brought up? (pump must not tick before this)

// Bring up renderer + library + audio. Called from the module's onForeground callback
// (the CUI module's foreground phase). Idempotent.
void bring_up() {
    if (g_render_ready) return;
    clog_("bring_up: cinder_render_init");
    if (cinder_render_init() != 0) { clog_("bring_up: render init FAILED"); return; }
    clog_("bring_up: cinder_db_open(/db/MTPDB.dat)");
    cinder_db_open("/db/MTPDB.dat");   // library reader (path: confirm on device)
    clog_("bring_up: cinder_scrobble_open(/contents/.scrobbler.log)");
    // Built-in scrobbler: writes the standard Audioscrobbler/1.1 log to the storage root so
    // existing uploaders (and the unknown321/scrobbler toolchain) work unchanged.
    cinder_scrobble_open("/contents/.scrobbler.log", "Cinder NW-A55 0.1");
    clog_("bring_up: cinder_audio_init");
    cinder_audio_init("cinder");       // PlayerService control (poll mode)
    g_render_ready = true;
    alarm(0);   // we reached foreground init -> cancel the hang watchdog
    clog_("bring_up: DONE (renderer ready)");
}

// ───────────────────────── input + playback glue ──────────────────────────────────────
// The pump reads /dev/input/event* each frame, maps raw evdev key CODES to logical Cinder
// buttons, feeds them to the navigator (cinder_input), and carries out the returned action via
// the audio shim. The NW-A50's buttons are GPIO keys (not a standard keyboard), so the raw
// codes are device-specific and need on-device `getevent` calibration. We ship sensible
// defaults and allow override from a plain-text file /contents/cinder_keymap.conf
// ("rawcode logicalbutton" per line) — editable over USB-MSC, no rebuild needed.

// Minimal evdev event (kernel uapi, ABI-stable). On the 32-bit MT8590 kernel `time` is two
// 32-bit longs → this struct is 16 bytes. We only read type/code/value.
struct ev_event { long tv_sec; long tv_usec; uint16_t type; uint16_t code; int32_t value; };
static const uint16_t EV_KEY_ = 0x01;

static int  g_keymap[768];
static int  g_evfds[16];
static int  g_evn = 0;
static bool g_input_started = false;
static bool g_playing = true;   // local transport state (PlayStatus playstate offset not RE'd)

static int keymap_size() { return (int)(sizeof g_keymap / sizeof *g_keymap); }

// Default map: standard Linux nav/media key codes. The NW-A50's actual GPIO codes differ —
// calibrate with getevent and drop them into /contents/cinder_keymap.conf.
static void keymap_defaults() {
    for (int i = 0; i < keymap_size(); ++i) g_keymap[i] = -1;
    auto set = [](int code, int btn) { if (code >= 0 && code < keymap_size()) g_keymap[code] = btn; };
    set(103, CINDER_BTN_UP);     set(108, CINDER_BTN_DOWN);
    set(105, CINDER_BTN_LEFT);   set(106, CINDER_BTN_RIGHT);
    set(28,  CINDER_BTN_SELECT); set(96,  CINDER_BTN_SELECT);   // ENTER / KP_ENTER
    set(158, CINDER_BTN_BACK);   set(1,   CINDER_BTN_BACK);     // BACK / ESC
    set(139, CINDER_BTN_OPTION);                                // MENU
    set(164, CINDER_BTN_PLAY);   set(200, CINDER_BTN_PLAY);     // PLAYPAUSE / PLAYCD
    set(163, CINDER_BTN_RIGHT);  set(165, CINDER_BTN_LEFT);     // NEXTSONG / PREVIOUSSONG
    set(115, CINDER_BTN_VOLUP);  set(114, CINDER_BTN_VOLDOWN);
    set(102, CINDER_BTN_HOME);   set(116, CINDER_BTN_POWER);
}

static void keymap_load_overrides() {
    // Parse with strtol (NOT sscanf): clang+modern-glibc redirect scanf to __isoc23_* symbols
    // that don't exist on the device's glibc 2.23.
    FILE* f = std::fopen("/contents/cinder_keymap.conf", "r");
    if (!f) return;
    char line[128];
    while (std::fgets(line, sizeof line, f)) {
        if (line[0] == '#' || line[0] == '\n') continue;
        char* end = nullptr;
        long code = std::strtol(line, &end, 10);
        if (end == line) continue;
        long btn = std::strtol(end, nullptr, 10);
        if (code >= 0 && code < keymap_size() && btn >= 0 && btn <= 11)
            g_keymap[code] = (int)btn;
    }
    std::fclose(f);
    clog_("input: applied /contents/cinder_keymap.conf overrides");
}

static void input_open() {
    keymap_defaults();
    keymap_load_overrides();
    g_evn = 0;
    DIR* d = opendir("/dev/input");
    if (!d) { clog_("input: /dev/input missing"); return; }
    struct dirent* de;
    while ((de = readdir(d)) && g_evn < (int)(sizeof g_evfds / sizeof *g_evfds)) {
        if (std::strncmp(de->d_name, "event", 5) != 0) continue;
        char path[64];
        std::snprintf(path, sizeof path, "/dev/input/%s", de->d_name);
        int fd = open(path, O_RDONLY | O_NONBLOCK);
        if (fd >= 0) g_evfds[g_evn++] = fd;
    }
    closedir(d);
    char msg[64];
    std::snprintf(msg, sizeof msg, "input: opened %d /dev/input/event* node(s)", g_evn);
    clog_(msg);
}

// Carry out a navigator action via the audio shim. Volume + play-by-index need services we
// haven't finished RE'ing (SoundService volume / TrackSequence) — left as TODO, harmless.
void carry_out(int act) {
    switch (act) {
        case CINDER_ACT_PLAYPAUSE:
            g_playing = !g_playing;
            if (g_playing) cinder_audio_play(); else cinder_audio_pause();
            break;
        case CINDER_ACT_NEXT:          cinder_audio_next_track(); break;
        case CINDER_ACT_PREV:          cinder_audio_prev_track(); break;
        case CINDER_ACT_NEXT_ALBUM:    cinder_audio_next_group(); break;
        case CINDER_ACT_PREV_ALBUM:    cinder_audio_prev_group(); break;
        case CINDER_ACT_VOLUP:         break; // TODO: SoundService volume (RE pending)
        case CINDER_ACT_VOLDOWN:       break; // TODO
        case CINDER_ACT_PLAY_INDEX:    break; // TODO: PlayController::SetTrackSequence (RE pending)
        case CINDER_ACT_SLEEP:         break; // appmgr owns sleep; screen blank later
        case CINDER_ACT_ENTER_USB_MSC: break; // TODO: setprop sys.sony.config msc
        default: break;
    }
}

// Drain pending input from every node; map raw code -> logical button -> navigator -> action.
void input_pump() {
    ev_event evs[32];
    for (int i = 0; i < g_evn; ++i) {
        for (;;) {
            ssize_t n = read(g_evfds[i], evs, sizeof evs);
            if (n <= 0) break;
            int cnt = (int)(n / (ssize_t)sizeof(ev_event));
            for (int k = 0; k < cnt; ++k) {
                if (evs[k].type != EV_KEY_ || evs[k].value == 0) continue; // press/repeat only
                int code = evs[k].code;
                if (code < 0 || code >= keymap_size()) continue;
                int btn = g_keymap[code];
                if (btn < 0) continue;
                int act = cinder_input(btn);
                if (act != CINDER_ACT_NONE) carry_out(act);
            }
            if (n < (ssize_t)sizeof evs) break; // drained this node
        }
    }
}

// Battery percent from sysfs (best-effort; 100 if unavailable).
int read_battery() {
    static const char* paths[] = {
        "/sys/class/power_supply/battery/capacity",
        "/sys/class/power_supply/Battery/capacity",
        "/sys/class/power_supply/bat/capacity",
    };
    for (const char* p : paths) {
        FILE* f = std::fopen(p, "r");
        if (!f) continue;
        char buf[16] = {0};
        size_t got = std::fread(buf, 1, sizeof buf - 1, f);
        std::fclose(f);
        if (got > 0) {
            int v = (int)std::strtol(buf, nullptr, 10);
            if (v < 0) v = 0;
            if (v > 100) v = 100;
            return v;
        }
    }
    return 100;
}

// Poll the now-playing URI; on change, push it to the UI (cinder-ffi resolves title/artist/
// codec from the library DB). Position/duration await PlayStatus RE → pass 0 for now.
void poll_now_playing() {
    static char last[1024];
    char uri[1024];
    int n = cinder_audio_current_uri(uri, sizeof uri);
    if (n <= 0) return;
    if (std::strcmp(uri, last) == 0) return;
    std::strncpy(last, uri, sizeof last - 1);
    last[sizeof last - 1] = 0;
    cinder_set_now_playing_uri(uri, 0.0f, g_playing ? 1 : 0, read_battery());
}

// Concrete app. The pure virtual destructor (slots 0,1) is satisfied by ~CinderApp.
// We override every lifecycle hook ONLY to trace it (each calls the base default), so the
// device log shows exactly how far the appmgr/easel handshake progresses.
class CinderApp : public easel::ApplicationBase {
public:
    ~CinderApp() override = default;
    void OnInitialize() override     { clog_("app:OnInitialize");     easel::ApplicationBase::OnInitialize(); }
    void OnPostInitialize() override { clog_("app:OnPostInitialize"); easel::ApplicationBase::OnPostInitialize(); }
    void OnActivate() override       { clog_("app:OnActivate");       easel::ApplicationBase::OnActivate(); }
    void OnForeground() override     { clog_("app:OnForeground");     bring_up(); easel::ApplicationBase::OnForeground(); }
    void OnBackground() override     { clog_("app:OnBackground");     easel::ApplicationBase::OnBackground(); }
    void OnInactivate() override     { clog_("app:OnInactivate");     easel::ApplicationBase::OnInactivate(); }
    void OnFinalize() override       { clog_("app:OnFinalize");       cinder_render_shutdown(); easel::ApplicationBase::OnFinalize(); }
    void StopBootAnimation() override{ clog_("app:StopBootAnimation");easel::ApplicationBase::StopBootAnimation(); }
};

} // namespace

int main(int argc, char** argv) {
    clog_("main: start");
    install_diagnostics();   // crash/hang handler -> logs the exact PC of the stall
    CinderApp app;

    // The CuiAppModule callbacks (named per the RE'd ctor). They map to lifecycle phases;
    // each is traced. onForeground brings up the renderer; the pump ticks it.
    auto cbInit    = []() { clog_("cb:onInitialize"); };
    auto cbPostI   = []() { clog_("cb:onPostInitialize"); };
    auto cbActivate= []() { clog_("cb:onActivate"); };
    auto cbForeg   = []() { clog_("cb:onForeground"); bring_up(); };
    auto cbFinal   = []() { clog_("cb:onFinalize"); cinder_render_shutdown(); };
    auto pump      = []() -> bool {
        static long n = -1;
        ++n;
        if (n < 3) clog_("cb:pump");
        if (n == 0) alarm(0);                 // first pump cancels the hang watchdog
        if (!g_render_ready) return true;     // wait until the renderer is brought up
        if (!g_input_started) { input_open(); g_input_started = true; }
        cinder_render_tick();                 // paint the current navigator screen
        input_pump();                         // buttons -> navigator -> playback actions
        if (n % 30 == 0) {                    // ~1x/sec housekeeping
            poll_now_playing();               //   refresh now-playing + battery
            cinder_scrobble_tick(g_playing ? 1 : 0); // accrue listen time for the scrobbler
        }
        return true;
    };
    auto cb7       = []() { clog_("cb:cb7"); };

    clog_("main: constructing CuiAppModule");
    auto module = std::unique_ptr<easel::ModuleBaseInterface>(
        new easel::CuiAppModule(app, argc, argv,
            cbInit, cbPostI, cbActivate, cbForeg, cbFinal,
            pump, cb7));

    clog_("main: calling app.run()");
    app.run(argc, argv, "HgrmMediaPlayerApp", std::move(module));
    clog_("main: app.run() returned");
    return 0;
}
