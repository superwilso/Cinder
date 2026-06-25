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
#include <csignal>
#include <cstring>
#include <ucontext.h>
#include <unistd.h>
#include <execinfo.h>

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
    clog_("bring_up: cinder_audio_init");
    cinder_audio_init("cinder");       // PlayerService control (poll mode)
    g_render_ready = true;
    alarm(0);   // we reached foreground init -> cancel the hang watchdog
    clog_("bring_up: DONE (renderer ready)");
}

// Carry out a navigator Action (returned by cinder_input) via PlayerService.
void dispatch_action(int act) {
    switch (act) {
        case CINDER_ACT_NEXT:        cinder_audio_next_track(); break;
        case CINDER_ACT_PREV:        cinder_audio_prev_track(); break;
        case CINDER_ACT_NEXT_ALBUM:  cinder_audio_next_group(); break;
        case CINDER_ACT_PREV_ALBUM:  cinder_audio_prev_group(); break;
        default: break; // PLAYPAUSE / PLAY_INDEX / VOL* / USB_MSC / SLEEP — TODO
    }
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
        static int n = 0;
        if (n < 3) { clog_("cb:pump"); alarm(0); ++n; }   // pump runs -> cancel watchdog, then go quiet
        if (g_render_ready) cinder_render_tick();
        return true;
        // TODO(device): read /dev/input/hoge -> dispatch_action(cinder_input(button));
        // and poll PlayerService -> cinder_set_now_playing_uri(...).
        (void)dispatch_action;
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
