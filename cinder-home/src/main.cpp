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
#include <poll.h>
#include <execinfo.h>
#include <fcntl.h>
#include <dirent.h>
#include <ctime>
#include <time.h>   // clock_gettime/CLOCK_MONOTONIC (drag velocity timing)
#include <setjmp.h>
#include <pthread.h>
#include <sys/stat.h>      // stat() — the dev request-file consumer (take_req)
#include <sys/statvfs.h>
#include <sys/ioctl.h>
#include <sys/mount.h>   // umount/umount2 (we unmount /contents ourselves before the MSC handoff)
#include <cerrno>
#include <string>            // std::string — Sony's pst::base::string IS libc++ std::string
#include <vector>            // ditto for pst::base::vector (GetConnectInformation's MAC out-param)
#include <sys/socket.h>      // the USB-DAC -> LDAC bridge writes PCM to an abstract AF_UNIX socket
#include <sys/un.h>
#include <dlfcn.h>           // libasound is dlopen'd, NOT linked — see the LDAC bridge block

// The render core: the Rust Cinder UI, built as a glibc C-ABI staticlib
// (player/cinder-ffi -> libcinder_ffi.a). C ABI, so the renderer stays in Rust while
// this shell stays C++/libc++. See player/cinder-ffi/include/cinder.h.
#include "cinder.h"
#include "cinder_effects.h"
#include "cinder_tuner.h"
#include "cinder_analyzer.h"
#include "cinder_power.h"
#include "discover.h"
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
void log_fault(int sig, void* uc_, siginfo_t* si, const char* tag) {
    unsigned long pc = 0, lr = 0;
#if defined(__arm__)
    ucontext_t* uc = static_cast<ucontext_t*>(uc_);
    pc = uc->uc_mcontext.arm_pc; lr = uc->uc_mcontext.arm_lr;
#endif
    std::fprintf(stderr, "[cinder-home] *** %s : sig=%d PC=0x%08lx LR=0x%08lx addr=%p ***\n",
                 tag, sig, pc, lr, si ? si->si_addr : (void*)0);
    void* bt[24];
    int n = backtrace(bt, 24);
    std::fprintf(stderr, "--- backtrace (%d frames) ---\n", n);
    backtrace_symbols_fd(bt, n, 2 /*stderr*/);
    std::fflush(stderr);
    dump_maps();
}

// ── crash/hang GUARD ────────────────────────────────────────────────────────────────────
// A crash (SIGSEGV/BUS/…) OR a watchdog timeout (SIGALRM) that happens INSIDE a guarded call
// (run_guarded) is RECOVERED via siglongjmp — the subsystem is skipped and the UI keeps
// running. Outside a guard it's FATAL (_exit → appmgr reboots → the launcher's bad-boot counter
// accumulates → auto-revert to stock). This is what makes a blocking/buggy Sony service unable
// to brick the device: worst case is "UI runs without audio/library", never a hung boot screen.
sigjmp_buf g_guard_jb;
volatile sig_atomic_t g_in_guard = 0;
// The guard's sigsetjmp is thread-specific: siglongjmp is only valid on the thread that set it
// up. Sony libraries (PlayerService/effect/analyzer) run IPC threads of their own, and a fault on
// one of THOSE while the pump thread is inside run_guarded must NOT siglongjmp with the pump's
// jump buffer (cross-thread longjmp = stack corruption / crash-loop). So we record the guard owner
// and only recover when the faulting thread IS the owner; any other thread's fault is fatal (the
// bad-boot counter then reverts — far safer than corrupting the stack).
volatile pthread_t g_guard_owner = 0;

// Latch the bad-boot counter to MAXBAD so the NEXT boot reverts to stock, then never runs us again
// until a newer binary is installed (the launcher self-heals on that).
//
// WHY THIS EXISTS (2026-07-26, learned the hard way): the counter is cleared once we've painted a
// frame and survived ~8 s. A crash AFTER that point therefore leaves a *cleared* counter — so the
// next boot starts from zero, crashes again, clears again… The counter can never accumulate and
// rung 1 of the escape ladder is silently dead. That is exactly what happened with the per-frame
// canvas OOM: it aborted seconds after "healthy: bad-boot counter cleared", and the device
// logo-looped with no automatic way out (only the cable-at-boot escape saved it).
//
// Absence of health is not evidence of a crash. So the crash records itself.
// async-signal-safe only: open/write/close/fsync are all on the POSIX AS-safe list.
void latch_bad_boot_counter() {
    int fd = open("/data/cinder/bootcount", O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd < 0) return;                       // no /data → nothing we can do from a signal handler
    ssize_t w = write(fd, "4", 1); (void)w;   // MAXBAD in deploy/install_cinderhome.sh
    fsync(fd);                                // must survive the reboot we're about to take
    close(fd);
}

void fault_handler(int sig, siginfo_t* si, void* uc_) {
    // SIGABRT is NEVER recoverable — not even inside a guard. glibc raises it when it detects
    // heap corruption *inside malloc with the arena lock held*; a siglongjmp "recovery" leaves
    // that lock held forever, so the next allocation anywhere (including this handler's own
    // fprintf/backtrace) deadlocks silently and even the watchdog can't fire (observed on device
    // 2026-07-02: recovered EQ-apply abort → wedged in the next guarded call, log went dark).
    // So: async-signal-safe write() only (stderr is the log file), then die → reboot → counter.
    if (sig == SIGABRT) {
        static const char m[] = "[cinder-home] *** FATAL SIGABRT (heap corruption / library abort)"
                                " — no recovery possible, latching bad-boot + exiting ***\n";
        ssize_t w = write(2, m, sizeof m - 1); (void)w;
        latch_bad_boot_counter();   // a post-health crash must still auto-revert
        _exit(42);
    }
    if (g_in_guard && pthread_equal(pthread_self(), (pthread_t)g_guard_owner)) {
        log_fault(sig, uc_, si, "GUARDED CALL FAULTED — skipping that subsystem, UI continues");
        g_in_guard = 0;
        alarm(0);
        siglongjmp(g_guard_jb, 1);   // unwind back to run_guarded (same thread that set it up)
    }
    log_fault(sig, uc_, si,
              sig == SIGALRM ? "WATCHDOG (un-guarded hang)" : "FATAL SIGNAL");
    // Same reasoning as the SIGABRT path: if we already declared ourselves healthy, only the
    // crash itself can put the counter back, so record it before dying.
    latch_bad_boot_counter();
    _exit(42);  // die fast -> appmgr reboots -> bad-boot counter reverts to stock
}

void install_diagnostics() {
    struct sigaction sa; std::memset(&sa, 0, sizeof sa);
    sa.sa_sigaction = fault_handler; sa.sa_flags = SA_SIGINFO | SA_NODEFER;
    sigemptyset(&sa.sa_mask);
    for (int s : {SIGSEGV, SIGBUS, SIGABRT, SIGILL, SIGFPE, SIGALRM}) sigaction(s, &sa, nullptr);
    // SIGPIPE MUST NOT BE ABLE TO KILL THE HOME APP. Its default disposition is termination, and it
    // fires on any write to a socket or pipe whose peer has gone — an entirely routine event for the
    // LDAC transmitter socket and for every popen'd helper. Measured 2026-08-11: the first session
    // that actually carried audio to the transmitter ended with rc=141 (128 + SIGPIPE) the moment
    // the service closed its end, and because appmgr does not respawn the launcher, the device was
    // left with NO Home app. Individual sites still use MSG_NOSIGNAL where they can; this is the
    // backstop, and it is the correct disposition for any long-lived process that writes to fds it
    // does not own.
    signal(SIGPIPE, SIG_IGN);
    // Pre-warm backtrace(): its first call mallocs (libgcc unwinder init). Doing it here means a
    // later in-handler backtrace after a *hang* (SIGALRM) doesn't allocate at a moment the heap
    // lock might be held by the interrupted thread.
    void* warm[4]; backtrace(warm, 4);
    alarm(20);  // construction watchdog: if we never paint, _exit before appmgr's own timeout
}

// Run `fn` under the crash+hang guard with a `timeout`-second watchdog. Returns 0 on success,
// -1 if it crashed or hung (in which case it was skipped and the process is still alive).
// Saves/restores any OUTER alarm so it composes when called inside the per-frame watchdog
// (e.g. carry_out -> EQ apply, which runs inside input_pump's alarm()).
//
// Logging: each label is traced the FIRST time only (the boot sequence stays readable in
// cinderhome.log), plus every RECOVERY. Per-tick calls ("pump: poll now-playing" ran once a
// second) no longer flood the log. Labels are string literals → identity compare is enough.
int run_guarded(const char* what, unsigned timeout, void (*fn)()) {
    static const char* seen[64];
    static int nseen = 0;
    bool first = true;
    for (int i = 0; i < nseen; ++i)
        if (seen[i] == what) { first = false; break; }
    if (first) {
        if (nseen < (int)(sizeof seen / sizeof *seen)) seen[nseen++] = what;
        clog_(what);
    }
    unsigned prev = alarm(0);     // pause + capture the outer watchdog's remaining time
    g_guard_owner = pthread_self();  // only THIS thread may siglongjmp back to the buffer below
    g_in_guard = 1;
    if (sigsetjmp(g_guard_jb, 1) == 0) {
        alarm(timeout);
        fn();
        alarm(0);
        g_in_guard = 0;
        if (prev) alarm(prev);    // resume the outer watchdog
        return 0;
    }
    // returned here via siglongjmp from fault_handler — always name the recovered call (the
    // fault dump itself can't know the label)
    g_in_guard = 0;
    alarm(0);
    if (prev) alarm(prev);
    char m[128];
    std::snprintf(m, sizeof m, "GUARD RECOVERED: %s", what);
    clog_(m);
    return -1;
}

bool g_settings_loaded = false; // did cinder_settings_load find a saved file? (→ re-apply EQ/sound)
bool g_volume_restored = false; // did it restore a persisted volume level? (→ push to hw, not seed)
void set_backlight(int night); // night=minimal/day=normal; boot forces day, toggle matches theme
void recompute_day_level();   // map the UI's 0..5 brightness onto the node's day level (no write)
void brightness_wake_on_input(); // leave a level-0 blank on the next input (defined with the backlight)
extern bool g_screen_on;      // panel lit? (defined with the screen state, below)
extern long g_last_input_ms;  // idle-screen-off clock; seeded by render_up, defined with the input state
long now_ms();                // CLOCK_MONOTONIC ms (defined with the touch state, below)
bool g_render_ready = false;   // framebuffer/renderer open? (pump must not tick before this)
easel::CuiAppModule* g_cui = nullptr;   // the UI module — used to drive the pump (OnPumpTrigger)
easel::ApplicationBase* g_app = nullptr; // the app — for StopBootAnimation() (stop the boot-anim overlay)
volatile bool g_pump_ticker_run = false; // is the pump-driver ticker thread active?
pthread_t g_pump_ticker = 0;   // the ticker thread handle (joined at finalize)
bool g_deferred_done = false;  // slow/blocking init (DB + PlayerService) finished?
time_t g_healthy_since = 0;    // when deferred init completed (for the proven-healthy reset)
bool g_counter_reset = false;  // have we cleared the launcher bad-boot counter this boot?
time_t g_first_paint_at = 0;   // when the FIRST frame hit the panel (the bad-boot health signal)
int g_screenshot_sync = 0;     // countdown: sync /contents a few ticks after a screenshot is taken

// ── watchdog summary ────────────────────────────────────────────────────────────────────
// SIGALRM (alarm) + fault_handler give two layers: (1) `run_guarded` wraps the blocking Sony-IPC
// calls (DB load, PlayerService connect, now-playing poll) and RECOVERS via siglongjmp (skip that
// subsystem, keep the UI alive); (2) a per-frame `alarm(8)` around render/input + the construction
// `alarm(20)` are FATAL on timeout (_exit → appmgr reboots → the launcher's bad-boot counter
// accumulates → auto-revert to stock). Together: a blocking/buggy Sony service can't hang the boot
// (the 2026-06-26 wbrt incident), and a hang/crash in our own code reverts instead of soft-bricking.

// ── render driver (Option-B pivot, 2026-07-01) ──────────────────────────────────────────────
// The non-Qt CuiAppModule's OnForeground blocks in Sony's event/JobQueue framework: its second
// sub-module vtable call ([this+0x60]->+0x18) waits on the CuiAppModule condition_variable for a
// flag that Sony's pst::core::JobQueue / event-muxer would drive — infrastructure that libeaselqt
// runs for the *Qt* app but which we don't (CuiAppModule has NO other user on the device, so it's
// unproven standalone). Driving OnPumpTrigger did NOT help (different wait). But render_up already
// opened the framebuffer, so we DRIVE OUR OWN render loop here instead of relying on easel's pump.
// This paints the panel from a thread we control, independent of the (blocked) easel main thread.
// SAFETY: the launcher's per-boot bad-boot counter (cleared only when we mark healthy) still
// auto-reverts to stock after 2 boots even if we disarm the in-process watchdog below.
void start_pump_ticker();   // full worker defined just before main(), after the frame helpers

// ── boot animation / display handover (2026-07-02, disasm of xbin/icx_bootanimation) ──────
// The REAL mechanism, superseding all earlier "kill timing" theories: mtkfb does NOT scan the
// framebuffer continuously — pixels only reach the panel when a process issues
// FBIOPUT_VSCREENINFO with activate|=FB_ACTIVATE_FORCE (the anim's per-frame flip, disasm
// @0x1fae). The anim has NO signal handlers (no signal/sigaction imports) — SIGTERM kills it
// dead at any moment, and whatever frame was pushed last stays latched on the glass until
// somebody else flips. Historically the anim was the ONLY flipper, so every kill timing was a
// coin flip on whose pixels were on-glass ("hit and miss" frozen boot image), and our UI only
// ever appeared when ITS flips happened to push OUR memory. Since cinder-ffi's blit now ends
// with the same trigger ioctl, our first painted frame after its death always takes the panel —
// kill at first paint, no timing sensitivity. Forced repaints (cinder_force_dirty) still cover
// external scribbles into the fb pages themselves.

// FAST foreground bring-up: open the framebuffer ONLY, so we can paint immediately and the
// appmgr Foreground handshake completes promptly. No DB, no IPC here (those can be slow/block).
bool gadget_in_dac_mode();   // defined with the USB-DAC block below
static int bt_status();      // defined with the Bluetooth block below
static bool bt_radio_up(int st);
static void apply_pump_interval();  // defined with poll_now_playing — audio-pump rate for the current state
void refresh_bt_route();     // ditto — points the volume rocker at whichever output is live
static void bt_reconnect_rearm();  // ditto — "the user wants a link"; clears the auto-reconnect latch
extern bool g_bt_radio_seen_up;  // ditto — last GetBtStatus said the radio was up (poll back-off)
void apply_bt_codec();       // ditto — pushes the codec choice to the radio (not just the conf file)
static void bt_apply_enhanced_mode(const char* why); // ditto — Sony's "Use Enhanced Mode" (absolute volume)
static void refresh_bt_connected();  // ditto — names the linked device for the Bluetooth screen
void refresh_bt_paired();    // ditto — reads the radio's pairing table for the Devices screen
void apply_bt_scan();        // ditto — starts/stops discovery (SetSearchMode + the listener)
void apply_bt_prompt_reply(bool accept); // ditto — answers a numeric-comparison / SSP prompt
void apply_bt_pair_device(); // ditto — pairs with a device the scan turned up

void render_up() {
    if (g_render_ready) return;
    clog_("render_up: cinder_render_init");
    if (cinder_render_init() != 0) { clog_("render_up: render init FAILED"); return; }
    g_render_ready = true;
    // Restore persisted UI preferences (theme/visualiser/EQ/sound) so the first paint reflects them.
    // Pure file read (no Sony service) — safe on the boot path; a missing file just leaves defaults.
    // If a file was loaded, deferred_up re-applies the saved EQ/sound to the DSP once audio is up.
    int sl = cinder_settings_load("/contents/cinder_settings.conf");
    g_settings_loaded = (sl & 1) != 0;
    g_volume_restored = (sl & 2) != 0;
    // Reconcile the USB-DAC toggle with the gadget's ACTUAL mode. The toggle is our intent; the
    // property is the fact, and they diverge whenever USB mode is changed outside Cinder. Without
    // this, Settings can report MASS STORAGE while the hardware sits in `uac`, and the only way out
    // is to toggle all the way through DAC and back — measured 2026-07-29, and it stranded the
    // device with no USB. Pure property read, no Sony service, safe on the boot path.
    cinder_set_usb_dac(gadget_in_dac_mode() ? 1 : 0);
    // Boot ALWAYS at DAY backlight, even if night theme is persisted — the night dim is NOT resumed
    // across boots. Otherwise a daytime boot into persisted night could come up at ~3% backlight and
    // you couldn't see the screen to turn it back up. The night dim is a deliberate per-session action
    // (toggle Theme→night). Pure sysfs write (no Sony service); no-op if no backlight node found.
    // Persisted brightness applies at boot; the theme does NOT (always day — see above). Level 1
    // is 15% of max, so even the dimmest persisted setting comes up readable.
    recompute_day_level();
    set_backlight(0);
    // Start the idle clock NOW. It defaults to 0, which reads as "last input was at time zero", so
    // a persisted screen-off timeout would otherwise blank the panel on the first housekeeping tick,
    // before the user has touched anything.
    g_last_input_ms = now_ms();
    clog_("render_up: DONE (renderer ready)");
    // From here the render_driver worker thread owns SIGALRM (per-frame watchdog + run_guarded).
    // Block SIGALRM on THIS (main) thread first: main is about to get stuck in easel's OnForeground
    // (CuiAppModule's blocking pump path), so a guard alarm raised by the worker must never be
    // delivered here (it would fail the g_guard_owner check and _exit us). The worker unblocks
    // SIGALRM on itself. (The construction alarm(20) is a process-wide timer; once armed it will be
    // delivered to the worker, which supersedes it with its own per-frame alarm on the first tick.)
    sigset_t sa; sigemptyset(&sa); sigaddset(&sa, SIGALRM);
    pthread_sigmask(SIG_BLOCK, &sa, nullptr);
    // Nothing drives the easel pump for a non-Qt CuiAppModule, so run our own render+input loop.
    start_pump_ticker();
}

// Is the optional real-spectrum visualiser enabled? Reads /contents/cinder_viz.conf for
// `analyzer=0`. Absent/unset => ON. Kept deliberately dumb (substring match) so a malformed file
// can't do anything but disable it.
//
// CACHED. This is polled by viz_analyzer_tick() at 1 Hz for the entire runtime of the device, and
// it used to open + read + close a file on /contents EVERY call: ~86k opens a day, on the fragile
// vfat partition, to re-answer a question whose answer can only change if the user rewrites the
// file — which needs USB-MSC, which means leaving the app. The cache is dropped when a mass-storage
// session ends (viz_conf_invalidate, called from the MSC exit path), which is the only moment the
// file can have changed under us without a reboot.
bool g_viz_conf_known = false;
bool g_viz_conf_on = true;
void viz_conf_invalidate() { g_viz_conf_known = false; }

bool viz_analyzer_enabled() {
    if (g_viz_conf_known) return g_viz_conf_on;
    // DEFAULT ON now. It used to default off because a Sony-service connect on the BOOT PATH is a
    // real risk — but it is no longer started at boot, only on demand once Now Playing is up and
    // playing, by which point the app has painted and cleared the bad-boot counter. And with the
    // synthetic fallback gone the visualiser simply cannot appear without it, so defaulting off
    // would mean shipping a feature that never works. `analyzer=0` in the file turns it off.
    bool on = true;
    FILE* f = std::fopen("/contents/cinder_viz.conf", "r");
    if (f) {
        char buf[256] = {0};
        size_t got = std::fread(buf, 1, sizeof buf - 1, f);
        std::fclose(f);
        if (got > 0) on = std::strstr(buf, "analyzer=0") == nullptr;
    }
    g_viz_conf_on = on;
    g_viz_conf_known = true;
    clog_(on ? "viz: analyzer enabled (cinder_viz.conf)" : "viz: analyzer disabled (cinder_viz.conf)");
    return on;
}

void report_storage();  // defined below (with the other sysfs readers); called from deferred_up
void fx_cache_drop();    // defined below: forget what the sound service was last told
void apply_eq_fn();      // defined below (carry_out helpers); re-applied from deferred_up on restore
void apply_sound_fn();   // ditto (apply_backlight is forward-declared earlier, before render_up)
void write_bt_pref();    // defined below (carry_out helpers); published once at boot from deferred_up
extern bool g_np_poll_now;  // defined with the pump state: force the next now-playing poll (see below)
extern bool g_house_due;    // defined with the pump state: run the ~1 Hz housekeeping on the next frame
void sync_volume_from_hw(); // defined below (volume backend); seeds the UI level from the mixer
void bt_resync_volume(const char* why); // ditto — re-asserts the UI level if the mixer drifted
void apply_volume();        // defined below (volume backend); writes the UI level to the mixer

// Start/stop Sony's spectrum analyzer to match what the screen is actually showing.
//
// The visualiser only draws when REAL spectrum data is arriving (cinder-ffi hides it otherwise), so
// the analyzer is the thing that makes it exist at all — and running it when nothing displays it is
// pure waste: an FFT plus an IPC callback stream, for pixels nobody sees.
//
// Conditions, all required: the panel is ON, the user hasn't disabled the visualiser, Now Playing
// is the current screen, and audio is actually playing. Anything else stops the stream. Guarded,
// because Start/Stop are Sony-service calls; failure just means no visualiser.
void viz_analyzer_tick() {
    bool want = g_screen_on && viz_analyzer_enabled() && cinder_viz_wants_analyzer();
    bool running = cinder_analyzer_is_running() != 0;
    if (want == running) return;
    if (want) {
        run_guarded("viz: analyzer start", 10,
                    []() { cinder_analyzer_start(CINDER_ANALYZER_SPECTRUM, 20.0f, 0); });
    } else {
        run_guarded("viz: analyzer stop", 6, []() { cinder_analyzer_stop(); });
    }
}

// Stop the analyzer stream (guarded — Stop() is a Sony-service call). No-op if it was never
// started, so it's safe to call unconditionally from the lifecycle hooks.
void stop_analyzer() {
    if (cinder_analyzer_is_running())
        run_guarded("analyzer stop", 6, []() { cinder_analyzer_stop(); });
}

// DEFERRED bring-up: the slow/blocking parts (library DB load + scrobbler + PlayerService
// connect). Run from the pump AFTER the first frame is painted, each under the hang watchdog
// so a blocking Sony-IPC can't stall the device. Idempotent, one-shot.
void deferred_up() {
    if (g_deferred_done) return;
    // Each guarded: a crash/hang in the library load or the PlayerService connect is caught and
    // that subsystem is skipped — the UI keeps running (empty library / no playback) rather than
    // hanging the boot. (db load is slow on a big DB, so a generous 25s; the IPC connect 12s.)
    run_guarded("deferred_up: cinder_db_open + build library", 25,
                []() { cinder_db_open("/db/MTPDB.dat"); });   // path: confirm on device
    // Bring back what was playing when the device was last powered off — the sequence, the user's
    // own queued picks and the position inside the track. Must follow the DB open: the files hold
    // object ids and the rows come from the library. Pure file IO + id lookups (no Sony service is
    // touched and nothing starts playing), but guarded anyway because it runs during bring-up and
    // a corrupt file must not be able to cost the user their Home app.
    //
    // /data/cinder, NOT /contents: the resume files are machine-written, they change while the PC
    // holds the MSC volume, and /contents is unmounted for the whole of that.
    run_guarded("deferred_up: restore playback context", 10, []() {
        int rr = cinder_resume_load("/data/cinder/queue.conf", "/data/cinder/resume.conf");
        clog_(rr == 1 ? "deferred_up: resume — context restored (plays on the first press)"
                      : "deferred_up: resume — nothing saved");
    });
    clog_("deferred_up: cinder_scrobble_open(/contents/.scrobbler.log)");
    cinder_scrobble_open("/contents/.scrobbler.log", "Cinder NW-A55 0.1");
    // THE FRAMEWORK PUMP — must come BEFORE cinder_audio_init. Sony's PlayerService client is
    // asynchronous: replies are dispatched by pst::core::Framework's event looper. Nothing drove
    // it (easel's pump never fires for our non-Qt CuiAppModule — see the render-driver note
    // above), so until 2026-07-27 every PlayerService call returned uninitialised stack and
    // playback silently did nothing. Safe to call here and not earlier: app.run() has already
    // constructed easel::Framework (which calls StartForApplication) by the time OnForeground
    // fires, and deferred_up runs after that — GetReference() on an unstarted Framework segfaults.
    clog_("deferred_up: cinder_audio_pump_start (pst::core::Framework event looper)");
    run_guarded("deferred_up: audio pump start", 8, []() { cinder_audio_pump_start(20); });
    run_guarded("deferred_up: cinder_audio_init (PlayerService connect)", 12,
                []() {
                    if (cinder_audio_init("cinder") != 0)
                        clog_("deferred_up: audio_init FAILED — transport + progress unavailable");
                });
    // Sync the Settings "Battery care" toggle to the device's real Itawari state (guarded — the
    // PowerMgrServiceClient ctor connects to the power service). Unavailable (-1) leaves the UI default.
    run_guarded("deferred_up: read battery care state", 8,
                []() { cinder_set_battery_care(cinder_power_get_battery_care()); });
    // Same treatment for the Bluetooth switch: sync it to the RADIO's real state instead of letting
    // it default to on. Statuses are 2 (on, idle) and 3 (connected) for a live radio; 7 is OFF and 0
    // reads as unknown. Anything that is not 2 or 3 reads as off, so the switch never claims a radio
    // that is not actually up. Deferred, not in render_up, because this is Sony IPC and needs the
    // framework pump started just above.
    run_guarded("deferred_up: read Bluetooth radio state", 8,
                []() {
                    int st = bt_status();
                    cinder_set_bt_on(bt_radio_up(st) ? 1 : 0);
                    // Same read decides where the volume rocker points, so headphones that were
                    // already connected at launch get the rocker from the very first press.
                    refresh_bt_route();
                    // …and names whatever is already linked, so the Bluetooth screen is correct on
                    // first open rather than after the first 3 s poll.
                    refresh_bt_connected();
                    char m[96];
                    std::snprintf(m, sizeof m, "deferred_up: BT radio status=%d -> switch %s",
                                  st, bt_radio_up(st) ? "ON" : "OFF");
                    clog_(m);
                });
    // Push the saved codec preference at the radio too. Same reasoning as the EQ re-apply below: a
    // preference that only lives in a file is a preference the hardware never hears about.
    run_guarded("deferred_up: apply saved BT codec", 6, apply_bt_codec);
    // Same for "Use Enhanced Mode" — the radio boots with whatever stock last left, and a stale OFF
    // makes every absolute-volume call a silent no-op (see bt_apply_enhanced_mode).
    run_guarded("deferred_up: apply saved BT enhanced mode", 6,
                []() { bt_apply_enhanced_mode("boot"); });
    // Re-apply the user's SAVED EQ + sound effects to the DSP (only if a settings file was restored —
    // no point pushing defaults on a fresh install). Guarded, like every effect-shim call.
    if (g_settings_loaded) {
        fx_cache_drop();   // boot: nothing is known about what the stock player left behind
        run_guarded("deferred_up: re-apply saved EQ", 6, apply_eq_fn);
        run_guarded("deferred_up: re-apply saved sound", 6, apply_sound_fn);
        // Repeat-one is sticky inside the audio shim and applied to every sequence it builds, so
        // pushing the restored value once here is enough — nothing is playing yet at this point.
        run_guarded("deferred_up: re-apply saved repeat", 4,
                    []() { cinder_audio_set_repeat_one(cinder_get_repeat_one()); });
    }
    // Real storage usage for Settings (statvfs — read-only, no Sony service; guarded for parity).
    run_guarded("deferred_up: report storage", 6, report_storage);
    // Volume at boot, stock-style: a PERSISTED level wins (restore it to the hardware — fixes the
    // "boots near-mute at hw level 1" failure); otherwise seed the UI from the mixer, and if that
    // reads back effectively mute (< 5/120 — the boot-default case, not a deliberate user 0),
    // apply the UI's modest audible default instead.
    if (g_volume_restored) {
        run_guarded("deferred_up: restore saved volume to hw", 6, apply_volume);
    } else {
        run_guarded("deferred_up: sync volume from hw", 6, sync_volume_from_hw);
        if (cinder_get_volume() < 5) {
            cinder_set_volume(15);
            run_guarded("deferred_up: apply default volume (hw was mute)", 6, apply_volume);
        }
    }
    // Publish the device-wide BT codec preference file (consumed by normal BT + the LDAC bridge), so
    // it reflects the persisted choice from first boot. Pure file IO; safe to call unguarded.
    write_bt_pref();
#ifdef CINDER_DEV
    // DEV CHANNEL: copy the real library DB out to user-visible storage (once per boot, only if
    // missing or the size changed — ~a few MB). Pull it via USB-MSC/flash.sh to close the
    // album-art schema question offline (images.value TEXT-path vs inline BLOB vs bmpfile).
    // Copy the library DB out to /contents so it's reachable over USB-MSC for offline schema work.
    // HARDENED 2026-07-25: the previous version silently no-op'd (the copy never landed on device).
    // Now it forces a sane PATH (the easel-launched context has a minimal one — `arecord`/`stat`
    // resolved inconsistently) and LOGS the outcome (OK+size / FAILED+error / SRC-missing) to
    // cinderhome.log, so one log pull tells us exactly what happened. Once adb is up (dev),
    // `adb pull /db/MTPDB.dat` is the primary route and this is just a fallback.
    run_guarded("deferred_up: copy MTPDB.dat to /contents (dev)", 15, []() {
        std::system(
            "export PATH=/system/bin:/system/xbin:/xbin:/bin:/sbin:/usr/bin:$PATH; "
            "SRC=/db/MTPDB.dat; DST=/contents/MTPDB_copy.dat; "
            "if [ -f \"$SRC\" ]; then "
            "  if cp \"$SRC\" \"$DST.tmp\" 2>/tmp/cpdberr && mv \"$DST.tmp\" \"$DST\" 2>>/tmp/cpdberr; then "
            "    echo \"[cinder-home] db-copy: OK $(stat -c %s \\\"$DST\\\" 2>/dev/null) bytes\"; "
            "  else echo \"[cinder-home] db-copy: FAILED: $(cat /tmp/cpdberr 2>/dev/null)\"; fi; "
            "else echo \"[cinder-home] db-copy: SRC $SRC not found\"; fi; "
            "sync");
    });
    // DEV CHANNEL: auto-capture the read-only device discovery (volume/ALSA/sysfs/usb + PlayStatus)
    // so the data needed to wire the device-gated features lands in /contents/cinder_discovery.txt
    // just by flashing dev — no separate probe run. with_input=0: the pump owns the input nodes, so
    // raw key codes are logged by input_pump instead (press buttons → cinderhome.log). Guarded.
    clog_("deferred_up: DEV channel — capturing device discovery (read-only)");
    run_guarded("deferred_up: discovery dump (dev)", 20,
                []() { cinder_run_discovery("/contents/cinder_discovery.txt", 1, 0); });
#endif
#ifdef CINDER_DEV
    // DEV CHANNEL ONLY (build.sh dev): best-effort enable adb for push-and-run iteration. Guarded,
    // so a failure is harmless — the player runs exactly like stable, just without adb. Touches NO
    // boot-critical files.
    //
    // CORRECTED 2026-07-25 (from the on-device discovery dump): the previous approach only did
    // `setprop ctl.start adbd`, on the assumption that "the boot-default gadget already carries the
    // adb function." The discovery dump DISPROVED that — at normal boot the gadget reads
    // `functions=mass_storage` (NO adb interface), so adbd ran but the PC never saw an adb device.
    // Fix: COMPOSE adb into the android_usb gadget ourselves (the same sysfs pattern the MSC path
    // uses below): disable, set functions=mass_storage,adb (keep storage so the log/DB stay
    // reachable), set the MSC+adb composite PID 0B8D, re-enable, THEN start adbd. Safe here because
    // a dev boot only reaches this code with USB DISCONNECTED at launch (USB-at-launch boots stock),
    // so no PC is holding the gadget when we bounce it. Still fully guarded + best-effort.
    clog_("deferred_up: DEV channel — composing adb into the USB gadget + starting adbd (guarded)");
    run_guarded("deferred_up: enable adb (dev)", 10, []() {
        std::system(
            "echo 0 > /sys/class/android_usb/android0/enable 2>/dev/null; "
            "echo mass_storage,adb > /sys/class/android_usb/android0/functions 2>/dev/null; "
            "echo 0B8D > /sys/class/android_usb/android0/idProduct 2>/dev/null; "
            "echo 1 > /sys/class/android_usb/android0/enable 2>/dev/null; "
            "setprop ctl.start adbd 2>/dev/null");
        // Verification snapshot ~3 s later, straight into cinderhome.log: service state per init,
        // the live process, and the gadget functions (now expected to read `mass_storage,adb`).
        // One log pull answers "is adb enumerated, or is it just the Windows driver?" (docs/adb_setup.md).
        std::system("sleep 3; echo \"[cinder-home] adb: init.svc.adbd=$(getprop init.svc.adbd) "
                    "proc=$(ps | grep -c '[a]dbd') "
                    "functions=$(cat /sys/class/android_usb/android0/functions 2>/dev/null) "
                    "idProduct=$(cat /sys/class/android_usb/android0/idProduct 2>/dev/null) "
                    "state=$(getprop sys.usb.state)\"");
    });
#endif
    // The real-spectrum visualiser is NOT started here any more. It is now started ON DEMAND (see
    // viz_analyzer_tick in the pump) only while its output is actually on screen, which keeps a
    // Sony-service connect off the boot path entirely — by the time it can run, the app has already
    // painted and cleared the bad-boot counter.
    g_deferred_done = true;
    g_healthy_since = std::time(nullptr);
    clog_("deferred_up: DONE");
}

// Clear the launcher's bad-boot counter once this boot is proven good. The launcher only
// INCREMENTS the counter; only we reset it, so a hang (which never reaches here) leaves it
// climbing → auto-revert.
//
// HEALTH BAR = "first frame painted, and still alive N seconds later" — deliberately NOT
// "deferred_up() finished". Rationale (2026-07-26): the counter exists to catch a boot that
// WEDGES — one that never satisfies appmgr's foreground handshake or never paints, leaving a
// black screen with no adb and no way in. Once a frame is on the glass the device is usable and
// recoverable, which is exactly the condition the safety net cares about. deferred_up() is
// FEATURE init (DB + PlayerService + battery + EQ + storage + volume, and on dev additionally
// the MTPDB copy, discovery dump and adb-enable); its guarded budgets sum to ~69 s on stable and
// ~104 s more on dev, so gating the reset behind it meant a perfectly healthy boot could stay
// "unproven" for over two minutes. Any reboot inside that window left the counter set, and with
// MAXBAD=2 the very next boot latched cinderhome_off PERMANENTLY (the launcher checks that flag
// before it counts, and nothing running ever clears it) → every subsequent boot ran stock until
// the flags were cleared by hand over USB-MSC. That is precisely what happened during the GPU
// work: three quick staged reboots put the device into a permanent stock-boot state.
// The counter moved from /contents to /data on 2026-07-26. /contents is vfat AND is the partition
// handed to the PC for USB-MSC, so it is both corruptible and periodically absent — when it failed
// to mount, this write went nowhere, the launcher's increment went nowhere too, and the device
// looped on the boot logo with the safety net silently disabled. /data is ext4 and USB-MSC never
// touches it. The path MUST stay in step with $BOOTCOUNT in deploy/install_cinderhome.sh.
void mark_healthy_maybe() {
    if (g_counter_reset || g_first_paint_at == 0) return;
    if (std::time(nullptr) - g_first_paint_at >= 8) {
        FILE* f = std::fopen("/data/cinder/bootcount", "w");
        if (!f) { clog_("healthy: FAILED to open /data/cinder/bootcount"); return; }  // retry next tick
        std::fputc('0', f);
        std::fclose(f);
        ::sync();
        g_counter_reset = true;
        clog_("healthy: bad-boot counter cleared");
    }
}

// The render thread calls mark_healthy_maybe() from its ~1/sec housekeeping block — but that block
// sits AFTER the `if (!g_deferred_done) { … deferred_up(); continue; }` gate, and deferred_up()
// BLOCKS that thread for as long as its guarded budgets allow (~69 s stable, ~173 s dev worst case).
// So the reset could not fire until feature init finished, which is the whole bug described above.
// This detached one-shot keeps ticking while the render thread is blocked, so "painted a frame and
// survived 8 s" clears the counter on time regardless of how slow the Sony services are.
void* healthy_timer(void*) {
    sleep(9);
    mark_healthy_maybe();
    return nullptr;
}

// DEV channel: an adb connection makes the gadget report CONFIGURED, which looks exactly like a PC
// data-host — so auto-MSC fires, hands /contents to the PC and UNMOUNTS it. That breaks adb-based
// development in a very confusing way: adb drops (the gadget re-enumerates), /contents reads come
// back empty, and files written there (logs, screenshots, the bad-boot counter) vanish mid-session.
// Observed 2026-07-26 while building the screenshot loop. On dev, auto-MSC is therefore OFF by
// default; create /contents/cinder_automsc_on to restore it. The Settings ▸ USB mode row still
// enters MSC manually on every channel, and stable is unchanged (auto-MSC is a real feature there).
bool dev_skip_auto_msc() {
#ifdef CINDER_DEV
    return ::access("/contents/cinder_automsc_on", F_OK) != 0;
#else
    return false;
#endif
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
static const uint16_t EV_SYN_ = 0x00; // SYN_REPORT (code 0) delimits an input frame
static const uint16_t EV_KEY_ = 0x01;
static const uint16_t EV_SW_  = 0x05; // switches (Hold/lock reports here on some configs)
static const uint16_t EV_ABS_ = 0x03;
static const uint16_t ABS_X_  = 0x00;
static const uint16_t ABS_Y_  = 0x01;
static const uint16_t ABS_MT_POSITION_X_ = 0x35;
static const uint16_t ABS_MT_POSITION_Y_ = 0x36;
static const uint16_t BTN_TOUCH_ = 0x14a;

static int  g_keymap[768];
static int  g_evfds[16];
static int  g_evn = 0;
static bool g_input_started = false;

// Touchscreen navigation (the NW-A55 has no d-pad). A contact is bracketed by BTN_TOUCH down/up;
// ABS_X/Y (or the MT variants) give its position. A mostly-VERTICAL move past a small slop
// becomes a LIVE DRAG: every position update streams pixel deltas into cinder_touch_drag so the
// list tracks the finger, and the release velocity feeds cinder_touch_fling for momentum —
// stock-smooth scrolling instead of the old one-jump-per-gesture row scroll. Everything else is
// classified at release in UI coordinates:
//   • left-edge → rightward swipe   = Back
//   • little movement               = TAP  → cinder_tap(x,y)
//   • horizontal swipe              = cinder_swipe (onboarding/NP skip/queue)
// The panel's reported ranges (from EVIOCGABS at open) map raw → UI (480×800), so this works
// regardless of whether the panel reports pixels or a raw range.
static int  g_touch_min_x = 0,  g_touch_max_x = 480;
static int  g_touch_min_y = 0,  g_touch_max_y = 800;
static bool g_touch_down   = false;
static int  g_touch_start_x = -1, g_touch_start_y = -1;
static int  g_touch_cur_x   = -1, g_touch_cur_y   = -1;
// Live-drag state: active flag, last dispatched UI y, smoothed velocity (px/s, UI scroll sign:
// positive = show later rows), and the time of the last movement (staleness check at release —
// hold-still-then-lift must not fling).
static bool  g_drag_active = false;
static int   g_drag_last_uy = 0;
static float g_drag_vel = 0.0f;
static long  g_drag_last_ms = 0;
// Drag-to-seek on the Now Playing progress rail. Decided at finger-DOWN (cinder_scrub_hit) and,
// once set, this contact belongs entirely to the scrub: no tap, no list drag, no swipe. That
// exclusivity matters — the rail band overlaps the horizontal-swipe area that skips tracks, so a
// scrub gesture would otherwise also fire a Next/Prev on release.
static bool  g_scrub_active = false;
static bool  g_scrub_tested = false;   // has this contact been offered to cinder_scrub_hit yet?
// Live ROW SWIPE (swipe-to-queue). Like the scrub, this is decided once and then owns the contact:
// a row that is following the finger must not also start scrolling the list under it. Set only if
// cinder_swipe_track reports that a TRACK row actually took the gesture — on an artist row, or the
// empty space below a list, the contact stays a normal drag.
static bool  g_hswipe_active = false;
// Up Next queue reorder. Vertical counterpart of g_hswipe_active: once a contact lands on a queue
// row's grab handle it owns that contact for the rest of its life, so the list must not also
// scroll under it.
static bool  g_reorder_active = false;
// Scrollbar drag: the bar at the right edge owns the contact, like the reorder handle does.
static bool  g_sbar_active = false;
long now_ms() {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (long)ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
}
// The actual touchscreen fd. Other input nodes report ABS_X/ABS_Y too (e.g. `m_batch_input`, a
// sensor-batch device — ABS=13f), so we MUST only treat ABS/BTN_TOUCH from THIS fd as touch, or
// the sensor stream overwrites the real finger coordinates and every tap lands at garbage.
static int  g_touch_fd = -1;
static bool g_touch_is_mt = false;    // g_touch_fd is a real MT node (first MT node wins, keeps it)
static bool g_touch_saw_pos = false;  // did the current SYN frame carry a position? (type-A lift)
static char g_touch_path[64] = {0};   // /dev/input/eventN of the touch node (for the holder scan)

// EVIOCGABS(abs) ioctl number = _IOR('E', 0x40+abs, struct input_absinfo). input_absinfo is 6×int32
// (24 bytes) on this 32-bit ABI. Defined by hand to avoid pulling linux/input.h into the 2.23 build.
struct cinder_absinfo { int32_t value, minimum, maximum, fuzz, flat, resolution; };
static unsigned eviocgabs(unsigned abs) {
    return (2u << 30) | (24u << 16) | ((unsigned)'E' << 8) | (0x40u + abs);
}
// EVIOCGNAME(64) = _IOC(READ, 'E', 0x06, 64) — the device's human name (diagnostics).
static const unsigned EVIOCGNAME_64 = (2u << 30) | (64u << 16) | ((unsigned)'E' << 8) | 0x06;
// EVIOCGRAB = _IOW('E', 0x90, int). grab(1) fails EBUSY if ANOTHER process holds the grab — and a
// foreign grab is the one condition that silently diverts ALL events away from our fd. We probe
// every node (grab+release) to log that condition, and HOLD the grab on the touchscreen (we are
// the Home app — the exclusive touch consumer — and holding locks out late grabbers).
static const unsigned EVIOCGRAB_ = (1u << 30) | (4u << 16) | ((unsigned)'E' << 8) | 0x90;
// EVIOCGKEY(len) = _IOR('E', 0x18, len) — the CURRENT pressed/closed state of every key code, as a
// bitmap. This is the only way to learn a switch's state without waiting for it to move.
//
// EVIOCGKEY and not EVIOCGSW, which is what a Hold switch would normally want: on this unit the
// Hold switch is reported as an ordinary KEY. `getevent -p /dev/input/event4` lists icx_key's
// capabilities as `KEY (0001): 001c 0023 …` — 0x23 = 35, our hold code — and /proc/bus/input/devices
// gives icx_key `EV=3` (SYN|KEY). NO node on this device advertises EV_SW at all, so the switch
// bitmap would be empty on every one of them.
static unsigned eviocgkey(unsigned len) {
    return (2u << 30) | (len << 16) | ((unsigned)'E' << 8) | 0x18;
}
// Map a raw touch coordinate to the UI's 0..480 / 0..800 space.
static int touch_ui_x(int rx) {
    int span = g_touch_max_x - g_touch_min_x; if (span <= 0) span = 1;
    long v = (long)(rx - g_touch_min_x) * 480 / span;
    return v < 0 ? 0 : (v > 479 ? 479 : (int)v);
}
static int touch_ui_y(int ry) {
    int span = g_touch_max_y - g_touch_min_y; if (span <= 0) span = 1;
    long v = (long)(ry - g_touch_min_y) * 800 / span;
    return v < 0 ? 0 : (v > 799 ? 799 : (int)v);
}
// Transport state. This is our INTENT (the last play/pause we sent); poll_now_playing replaces it
// with the service's own view once the grace window below has passed. Both are needed: the intent
// keeps the glyph correct immediately after a press, and the service's view is how a track ending
// or PlayerService pausing itself reaches the UI.
static bool g_playing = true;
static long g_transport_at = 0;              // when we last set g_playing ourselves (now_ms)
static const long TRANSPORT_GRACE_MS = 3000; // > the ~1 s onPlayTimeUpdated period + its 2.5 s
                                             // movement tolerance, so the lag can't fight a press
// Set the transport intent AND start the grace window. Always use this rather than assigning
// g_playing directly, or the service's lagging view will immediately overwrite the new state.
static void set_transport(bool playing) {
    g_playing = playing;
    g_transport_at = now_ms();
}

static int keymap_size() { return (int)(sizeof g_keymap / sizeof *g_keymap); }

// Default map: the NW-A50's REAL side-button codes (wampy glfw.patch, confirmed against the
// icx_key driver): the transport keys report as plain keyboard codes, NOT media codes —
// play=28 (KEY_ENTER), FF/next=106 (KEY_RIGHT), REW/prev=105 (KEY_LEFT), vol−=114, vol+=115,
// power=116 (on event0), hold switch=35. They map to GLOBAL transport buttons (the device has
// no d-pad, so nothing else wants these codes). Standard media codes stay mapped too for the
// qemu/sim path. Override any of it via /contents/cinder_keymap.conf.
static void keymap_defaults() {
    for (int i = 0; i < keymap_size(); ++i) g_keymap[i] = -1;
    auto set = [](int code, int btn) { if (code >= 0 && code < keymap_size()) g_keymap[code] = btn; };
    set(28,  CINDER_BTN_PLAY);                                  // side play/pause (KEY_ENTER)
    set(106, CINDER_BTN_NEXT);   set(105, CINDER_BTN_PREV);     // side FF / REW
    set(115, CINDER_BTN_VOLUP);  set(114, CINDER_BTN_VOLDOWN);  // volume rocker
    set(116, CINDER_BTN_POWER);                                 // power (event0)
    set(35,  CINDER_BTN_HOLD);                                  // hold/lock switch
    set(164, CINDER_BTN_PLAY);   set(200, CINDER_BTN_PLAY);     // PLAYPAUSE / PLAYCD (sim/qemu)
    set(163, CINDER_BTN_NEXT);   set(165, CINDER_BTN_PREV);     // NEXTSONG / PREVIOUSSONG
    set(158, CINDER_BTN_BACK);   set(1,   CINDER_BTN_BACK);     // BACK / ESC (sim/qemu)
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
        if (code >= 0 && code < keymap_size() && btn >= 0 && btn <= CINDER_BTN_PREV)
            g_keymap[code] = (int)btn;
    }
    std::fclose(f);
    clog_("input: applied /contents/cinder_keymap.conf overrides");
}

void touch_set_sleep(int slp);   // defined with screen_toggle below (himax sleep-node driver)

// Adopt the Hold switch's ACTUAL position instead of waiting for it to move. Defined further down,
// with the lock/screen state it writes (g_held).
static void sync_hold_from_hw();

static void input_open() {
    keymap_defaults();
    keymap_load_overrides();
    // WAKE the himax touch controller FIRST — nothing else does in our boot (the stock Qt app is
    // what normally writes the driver's sleep node), and asleep it produces zero events while the
    // event node still opens and answers ioctls. See touch_set_sleep() for the full story.
    touch_set_sleep(0);
    g_evn = 0;
    DIR* d = opendir("/dev/input");
    if (!d) { clog_("input: /dev/input missing"); return; }
    struct dirent* de;
    while ((de = readdir(d)) && g_evn < (int)(sizeof g_evfds / sizeof *g_evfds)) {
        if (std::strncmp(de->d_name, "event", 5) != 0) continue;
        char path[64];
        std::snprintf(path, sizeof path, "/dev/input/%s", de->d_name);
        int fd = open(path, O_RDONLY | O_NONBLOCK);
        if (fd >= 0) {
            g_evfds[g_evn++] = fd;
            // Identify the ACTUAL touchscreen and learn its x/y ranges (raw → UI mapping). The real
            // panel exposes ABS_MT_POSITION (himax); other nodes carry ABS_X/Y but aren't touch
            // (m_batch_input sensor). The FIRST MT node wins (a later virtual/uinput MT clone must
            // not displace the real panel); only fall back to a plain ABS_X/Y node if NO MT
            // touchscreen exists — and gate touch reads to g_touch_fd either way.
            struct cinder_absinfo ai, aiy;
            if (!g_touch_is_mt
                    && ioctl(fd, eviocgabs(ABS_MT_POSITION_X_), &ai) == 0 && ai.maximum > ai.minimum) {
                g_touch_fd = fd; g_touch_is_mt = true;
                std::snprintf(g_touch_path, sizeof g_touch_path, "%s", path);
                g_touch_min_x = ai.minimum; g_touch_max_x = ai.maximum;
                if (ioctl(fd, eviocgabs(ABS_MT_POSITION_Y_), &aiy) == 0 && aiy.maximum > aiy.minimum) {
                    g_touch_min_y = aiy.minimum; g_touch_max_y = aiy.maximum;
                }
            } else if (g_touch_fd < 0
                       && ioctl(fd, eviocgabs(ABS_X_), &ai) == 0 && ai.maximum > ai.minimum
                       && ioctl(fd, eviocgabs(ABS_Y_), &aiy) == 0 && aiy.maximum > aiy.minimum) {
                g_touch_fd = fd;   // tentative single-touch panel (no MT node seen yet)
                std::snprintf(g_touch_path, sizeof g_touch_path, "%s", path);
                g_touch_min_x = ai.minimum;  g_touch_max_x = ai.maximum;
                g_touch_min_y = aiy.minimum; g_touch_max_y = aiy.maximum;
            }
        }
    }
    closedir(d);
    char msg[160];
    std::snprintf(msg, sizeof msg, "input: opened %d node(s), touch=%s x[%d..%d] y[%d..%d]",
                  g_evn, g_touch_path[0] ? g_touch_path : "NONE",
                  g_touch_min_x, g_touch_max_x, g_touch_min_y, g_touch_max_y);
    clog_(msg);

    // ── DIAGNOSTIC PASS (zero events seen on device 2026-07-02): name every node + probe for a
    // foreign EVIOCGRAB. A grab by ANOTHER process is the one condition that silently diverts all
    // events away from our fds — exactly the observed symptom (8 nodes open, nothing ever read).
    for (int i = 0; i < g_evn; ++i) {
        char name[64] = {0};
        if (ioctl(g_evfds[i], EVIOCGNAME_64, name) < 0) std::snprintf(name, sizeof name, "?");
        int grab = ioctl(g_evfds[i], EVIOCGRAB_, (void*)1);
        int gerr = (grab < 0) ? errno : 0;
        if (grab == 0 && g_evfds[i] != g_touch_fd)
            ioctl(g_evfds[i], EVIOCGRAB_, (void*)0);   // probe only — release non-touch nodes
        // HOLD the grab on the touchscreen: we are the Home app (the exclusive touch consumer),
        // and holding it locks out any late-grabbing daemon from stealing the stream.
        std::snprintf(msg, sizeof msg, "input: node %d '%s'%s grab=%s%s", i, name,
                      (g_evfds[i] == g_touch_fd) ? " [TOUCH]" : "",
                      grab == 0 ? "ok" : (gerr == 16 /*EBUSY*/ ? "EBUSY(foreign grab!)" : "err"),
                      (grab == 0 && g_evfds[i] == g_touch_fd) ? " (held)" : "");
        clog_(msg);
    }
#ifdef CINDER_DEV
    // DEV: name every process that also has the touch node open (the grab holder is among them).
    if (g_touch_path[0]) {
        DIR* pd = opendir("/proc");
        struct dirent* pe;
        while (pd && (pe = readdir(pd))) {
            if (pe->d_name[0] < '0' || pe->d_name[0] > '9') continue;
            char fdd[64];
            std::snprintf(fdd, sizeof fdd, "/proc/%s/fd", pe->d_name);
            DIR* fdir = opendir(fdd);
            struct dirent* fe;
            while (fdir && (fe = readdir(fdir))) {
                char lp[96], tgt[96];
                std::snprintf(lp, sizeof lp, "%s/%s", fdd, fe->d_name);
                ssize_t l = readlink(lp, tgt, sizeof tgt - 1);
                if (l <= 0) continue;
                tgt[l] = 0;
                if (std::strcmp(tgt, g_touch_path) != 0) continue;
                char comm[48] = {0}, cp[64];
                std::snprintf(cp, sizeof cp, "/proc/%s/comm", pe->d_name);
                FILE* cf = std::fopen(cp, "r");
                if (cf) { if (std::fgets(comm, sizeof comm, cf)) comm[std::strcspn(comm, "\n")] = 0; std::fclose(cf); }
                std::snprintf(msg, sizeof msg, "input: %s also open in pid %s (%s)",
                              g_touch_path, pe->d_name, comm[0] ? comm : "?");
                clog_(msg);
                break;   // one hit per process is enough
            }
            if (fdir) closedir(fdir);
        }
        if (pd) closedir(pd);
    }
#endif
    sync_hold_from_hw();
}

// ── Sound-chain write cache ─────────────────────────────────────────────────────────────────
// WHY THIS EXISTS. Every effect setter here is an IPC round-trip to the sound service, and the UI
// re-applies the WHOLE chain on any change: ten EQ bands plus ~fifteen effect calls. That was fine
// while the controls were toggles you press once. It stopped being fine the moment the EQ and Tone
// Control band fields became DRAGGABLE (2026-08-17) — a drag emits a change per motion event, so
// the shell was doing twenty-five round-trips per frame and the audio stuttered while you moved a
// slider. The user's report, verbatim: "reduce the amount of stuttering and glitches when
// adjusting eq".
//
// So: remember what was last handed to the service and skip the calls whose value has not moved.
// A drag then costs ONE call — the band under the finger.
//
// The cache is a claim about the service's state, so anything that can invalidate that claim has
// to say so. `fx_cache_drop()` is called at boot and after a Bluetooth reconnect, which are the
// two places the chain is re-asserted precisely BECAUSE something else may have moved it.
static int  g_fx_last[40];
static bool g_fx_have = false;
static int  g_eq_last[10];
static bool g_eq_have = false;

void fx_cache_drop() { g_fx_have = false; g_eq_have = false; }

// True if slot `i` changed (and records the new value). Slots are private to each apply_* below.
static inline bool fx_dirty(int i, int val) {
    if (!g_fx_have || g_fx_last[i] != val) { g_fx_last[i] = val; return true; }
    return false;
}

// Read the UI's EQ bands and push them to the device DSP. Run ONLY via run_guarded (below): the
// EffectCtrlDmp connect can crash/hang if the sound service is down → caught, UI continues.
void apply_eq_fn() {
    signed char bands[10];
    cinder_get_eq_bands(bands);
    if (!g_eq_have) {
        // First apply, or the cache was dropped: assert the whole curve, and the 10-band's own
        // on-switch with it. cinder_effects_set_eq does both.
        cinder_effects_set_eq(bands, 10);
        for (int i = 0; i < 10; i++) g_eq_last[i] = bands[i];
        g_eq_have = true;
        return;
    }
    for (int i = 0; i < 10; i++) {
        if (g_eq_last[i] == bands[i]) continue;
        g_eq_last[i] = bands[i];
        cinder_effects_set_eq_band(i, bands[i]);
    }
}

// Apply the Sound screen's effect toggles to the DSP. Run ONLY via run_guarded (the EffectCtrlDmp
// connect can crash/hang). The bitmask matches cinder_get_sound_flags(): bit0 DSEE · bit1 Vinyl ·
// bit2 VPT · bit3 DC-Phase · bit4 Normalizer · bit5 ClearAudio+. VPT's ROOM is not a bit — it is
// an enum read separately via cinder_get_vpt_mode() and applied with SetVptMode. DC-Phase's filter
// TYPE is still on/off only; it is the same shape of change and the next one to make.
// See analysis/RE_dsp_effects_surface.md.
// HIGH GAIN OUTPUT — CUT 2026-08-17, and this note is here so it is not re-added.
//
// Sony's sid_4116. It is the CXD3778GF's own S-Master gain mode, not an EffectCtrlDmp effect:
//
//   numid=28 'headphone smaster gain mode'      ENUMERATED  normal | high
//   numid=29 'headphone smaster se gain mode'   ENUMERATED  normal | high   (single-ended)
//   numid=30 'headphone smaster btl gain mode'  ENUMERATED  normal | high   (balanced)
//
// Both 28 and 29 accepted `high`, read back 1, and the value survived a reboot. The write lands.
// The CODEC IGNORES IT — measured by ear on device, no audible change on any headphones. The A50's
// output stage does not have the high-gain hardware the ZX/WM1 series does; these controls come
// from the shared CXD3778GF driver and are inert on this model. (30 was never written: the NW-A55
// has only a stereo-mini jack, so `btl` describes hardware that isn't there.)
//
// THE LESSON, which is the reason this comment survives the code: on this device a mixer control
// accepting a write, reading the value back, and persisting it is NOT evidence the feature works.
// Only a measurable change in the output is. Bit 6 of the sound flags used to carry this and is now
// permanently clear; cinder-ui has a test that keeps it that way, because an older shell would
// still apply a set bit.

// L/R BALANCE — Sony's sid_0514, and the other Sound row that is a codec control rather than a DSP
// effect. `l balance volume` / `r balance volume`, INTEGER 0..88, and both are ATTENUATION: 0 is
// "no cut". So panning LEFT means turning the RIGHT channel down, not turning the left one up.
//
// Position 0..=100 with 50 = centre — a continuous drag slider since 2026-08-17 (it was 7 discrete
// stops). Both controls are always written, so moving back toward centre RELEASES the previous cut
// instead of leaving it applied on top of the new one.
//
// The control's range is 0..88 and its unit is the HALF-DECIBEL — the same unit the EQ bands were
// measured to use (analysis/RE_playerservice_sound.md). The original `away * 88 / 3` curve put the
// outermost stop at 88 raw = -44 dB, which is not a pan, it is a mute: reported 2026-08-16 as "it
// fully cuts the other side". A balance control exists to nudge a stereo image a few dB for an
// asymmetric pair of ears or headphones, so full deflection is -12 dB and you can still hear the
// quiet channel.
//
// Linear in dB across the travel, which is what makes a slider predictable — half way out is half
// the cut in dB, not half the power. 50 slider steps map onto 24 raw values, so most steps do not
// change the mixer at all and the cache below swallows them; a full sweep is at most 24 writes per
// side however fast the finger moves.
static const int BAL_MAX_ATT = 24;   // raw half-dB at full deflection = -12.0 dB

static void apply_balance(int pos) {
    const int CENTRE = 50;
    if (pos < 0) pos = 0;
    if (pos > 100) pos = 100;
    const int away = pos > CENTRE ? pos - CENTRE : CENTRE - pos;          // 0..50
    const int cut  = (away * BAL_MAX_ATT + CENTRE / 2) / CENTRE;          // rounded, 0..24
    const int l_att = pos > CENTRE ? cut : 0;   // panning right cuts the LEFT
    const int r_att = pos < CENTRE ? cut : 0;   // panning left cuts the RIGHT

    // Skip a write that changes nothing. apply_balance rides apply_sound_fn, so it used to run on
    // EVERY sound change, boot and BT reconnect — each one two shell forks and two codec writes for
    // a value that had not moved. It now ALSO runs on every motion event of a live drag, where the
    // same cache is what keeps the slider from forking a shell per pixel. -1 is "unknown", so the
    // first call always writes.
    static int s_l = -1, s_r = -1;
    if (l_att == s_l && r_att == s_r) return;

    // ONE shell, ONE amixer. It was two `system()` calls, i.e. two forks, two dynamic loads and two
    // separate codec writes with a scheduling gap between them — which is what the user heard as
    // "a bit of stutter when it does". amixer's batch mode (-s) reads commands from stdin, so both
    // channels move inside a single process. Names, not numids: a numid is a firmware-build detail
    // and this control's name is stable.
    char cmd[288];
    std::snprintf(cmd, sizeof cmd,
                  "{ echo \"cset name='l balance volume' %d\"; "
                  "echo \"cset name='r balance volume' %d\"; } "
                  "| amixer -c 0 -s >/dev/null 2>&1",
                  l_att, r_att);
    const int rc = std::system(cmd);
    if (rc == 0) { s_l = l_att; s_r = r_att; }
    char m[160];
    std::snprintf(m, sizeof m, "sound: balance pos=%d -> l_att=%d r_att=%d (raw half-dB) rc=%d",
                  pos, l_att, r_att, rc);
    clog_(m);
}

void apply_sound_fn() {
    // Slot numbers are local to this function and only have to be unique; see fx_dirty's note for
    // why the calls are skipped at all. Order is unchanged — only the "has it moved" guard is new.
    enum { S_DSEE, S_VINYL, S_VPT, S_VPTMODE, S_DC, S_DCTYPE, S_CLEARPH, S_DSEEAI, S_DSEECUST,
           S_DSEEMODE, S_TONE, S_TONE0, S_TONE1, S_TONE2, S_VINYLTYPE, S_SRCDIRECT,
           S_NORM, S_CLEARAUDIO };
    int f = cinder_get_sound_flags();
    if (fx_dirty(S_DSEE, (f >> 0) & 1)) cinder_effects_set_dsee_hx((f >> 0) & 1);
    if (fx_dirty(S_VINYL, (f >> 1) & 1)) cinder_effects_set_vinylizer((f >> 1) & 1);
    if (fx_dirty(S_VPT, (f >> 2) & 1)) cinder_effects_set_vpt((f >> 2) & 1);
    // Which room. Sent unconditionally rather than only when VPT is on: stock leaves a mode behind
    // with VPT off (it read back 1 = Club), so keeping the service's idea of the room in step with
    // ours means turning VPT on later cannot land in a room the UI is not showing.
    if (fx_dirty(S_VPTMODE, cinder_get_vpt_mode())) cinder_effects_set_vpt_mode(cinder_get_vpt_mode());
    if (fx_dirty(S_DC, (f >> 3) & 1)) cinder_effects_set_dc_phase((f >> 3) & 1);
    // Which filter. Sent unconditionally, same reasoning as the VPT room above.
    if (fx_dirty(S_DCTYPE, cinder_get_dc_type())) cinder_effects_set_dc_phase_type(cinder_get_dc_type());

    // Sound ▸ Advanced. Same read-flags-then-apply shape as the rows above.
    // NOTE ON ORDER: Source Direct is applied LAST of the bypass-class controls, after the effects
    // it hides have been set. The service keeps each effect's own state regardless, so this only
    // decides what is in the path — but setting the chain first and the bypass after means the
    // chain is already correct the moment the bypass is lifted.
    int af = cinder_get_adv_flags();
    if (fx_dirty(S_CLEARPH, (af >> 1) & 1)) cinder_effects_set_clear_phase((af >> 1) & 1);
    if (fx_dirty(S_DSEEAI, (af >> 2) & 1)) cinder_effects_set_dsee_ai((af >> 2) & 1);
    if (fx_dirty(S_DSEECUST, (af >> 3) & 1)) cinder_effects_set_dsee_hx_custom((af >> 3) & 1);
    if (fx_dirty(S_DSEEMODE, cinder_get_dsee_mode())) cinder_effects_set_dsee_hx_mode(cinder_get_dsee_mode());
    if (fx_dirty(S_TONE, (af >> 4) & 1)) cinder_effects_set_tone_control((af >> 4) & 1);
    // The three Tone Control band gains. Raw HALF-DECIBELS, Sony order BASS/MIDDLE/TREBLE — both
    // measured on device (cinder-probe --tone), not assumed. Sent whenever the chain is applied,
    // like the VPT room and the DC Phase filter, so switching Tone Control on later cannot land on
    // gains the UI is not showing.
    {
        signed char tb[3] = { 0, 0, 0 };
        int n = cinder_get_tone_bands(tb);
        for (int i = 0; i < n && i < 3; i++)
            if (fx_dirty(S_TONE0 + i, tb[i])) cinder_effects_set_tone_value(i, tb[i]);
    }
    // …AND SELECT WHICH TONE SYSTEM IS IN THE PATH. This was the hole: Sony's Equalizer and Tone
    // Control are ALTERNATIVES, SetSelectUsingEq picks between them, and NOTHING IN CINDER HAD EVER
    // CALLED IT. The device sat on 1 = the 6-band EQ, which Cinder does not even expose — so every
    // band the EQ screen has written since June was stored by the service and never in the path.
    // Measured, not inferred: the sound service logs `isproc is 1` for exactly one of the three
    // under each value (see effect_abi.hpp EqType, and `cinder-probe --inpath`).
    // NOT CACHED, and for the same reason SetBtAudioSoundEffect below is not: this call's entire
    // job is to make sure the right tone system is IN THE PATH, so skipping it because "we already
    // set it once" is skipping the assertion, not an optimisation.
    //
    // It is not hypothetical. MEASURED 2026-08-17 (`cinder-probe --userpreset`): Sony's three saved
    // setups each hold `SelectUsingEq = 1` — the SIX-band, which Cinder does not expose. Anything
    // that loads one (the stock UI, a stray LoadUserPreset) silently takes Cinder's EQ out of the
    // path, and a cached selector would keep quietly agreeing that everything was fine. That is the
    // exact bug this whole selector fix exists to kill; re-introducing it through a cache would be
    // a poor trade for ~3 ms on an apply that already only runs when something changed.
    cinder_effects_set_tone_system(((af >> 4) & 1) ? CINDER_TONE_SYS_TONE : CINDER_TONE_SYS_EQ10);
    // Sent unconditionally: the device read back Vinylizer type=7, which is not a member of the
    // four-value enum at all, because nothing had ever set it.
    if (fx_dirty(S_VINYLTYPE, cinder_get_vinyl_type())) cinder_effects_set_vinylizer_type(cinder_get_vinyl_type());
    if (fx_dirty(S_SRCDIRECT, (af >> 0) & 1)) cinder_effects_set_source_direct((af >> 0) & 1);
    if (fx_dirty(S_NORM, (f >> 4) & 1)) cinder_effects_set_dynamic_normalizer((f >> 4) & 1);
    if (fx_dirty(S_CLEARAUDIO, (f >> 5) & 1)) cinder_effects_set_clearaudio_plus((f >> 5) & 1);
    // …AND LET THE WHOLE CHAIN REACH BLUETOOTH. Project goal #7, and until now a hole: the shim has
    // exported this since the effects were first wired and NOTHING EVER CALLED IT, so over A2DP the
    // EQ, DSEE HX, VPT and Vinyl were applied or bypassed according to whatever the stock player had
    // last left the flag at. Every toggle on the Sound screen was potentially a lie the moment you
    // put headphones on — the same class of defect as the codec preference that only ever reached a
    // conf file (see apply_bt_codec).
    //
    // Unconditional rather than a setting: the user already chose these effects, and "apply them,
    // except silently not over Bluetooth" is not a choice anyone would make on purpose. It rides
    // apply_sound_fn so it is re-asserted everywhere the chain is — at boot, on the user's change,
    // and on every reconnect (the radio does not carry it across a link, exactly like the codec
    // preference and the volume).
    cinder_effects_set_bt_audio_effect(1);
    // NOT cached, deliberately: this is the one call whose whole job is to be re-asserted after
    // something else may have cleared it (a reconnect, the stock player). Caching it would make
    // the re-assert a no-op, which is the bug it was added to fix.
    g_fx_have = true;
    apply_balance(cinder_get_balance());
}

// ── Volume backend ──────────────────────────────────────────────────────────────────────────
// Configured by /contents/cinder_volume.conf, populated from the discovery report (the amixer
// control name / CXD3778GF sysfs node + range). Until that file exists, the volume keys stay a
// no-op (the HUD still shows) — so dropping the config ACTIVATES hardware volume with no rebuild.
//   backend=sysfs   path=<node>             min=<n> max=<n>   → write the scaled value to the node
//   backend=amixer  control=<name> card=<n> min=<n> max=<n>   → amixer -c<card> cset name='<name>' <v>
struct VolCfg { int valid = 0, amixer = 0, card = 0, min = 0, max = 0; char path[256] = {0}, control[128] = {0}; };
VolCfg g_vol;
bool g_vol_read = false;

void load_vol_cfg() {
    g_vol_read = true;
    FILE* f = std::fopen("/contents/cinder_volume.conf", "r");
    if (!f) {
        // No conf: default to the device-DISCOVERED hardware control (2026-07-02 discovery dump:
        // card 0, numid=10 `'master volume'` INTEGER 0..120 — the CXD3778GF master, matching the
        // stock 120-step volume; `amixer` confirmed present, it produced that dump). The conf
        // file, when present, fully overrides this. Wrong-hardware safety: if the control name
        // doesn't exist, `amixer cset` just fails → the keys stay HUD-only, same as before.
        g_vol.amixer = 1; g_vol.card = 0; g_vol.min = 0; g_vol.max = 120;
        std::snprintf(g_vol.control, sizeof g_vol.control, "master volume");
        g_vol.valid = 1;
        clog_("volume: using built-in default (amixer card0 'master volume' 0..120)");
        return;
    }
    char line[256];
    while (std::fgets(line, sizeof line, f)) {
        if (line[0] == '#' || line[0] == '\n') continue;
        char* eq = std::strchr(line, '=');
        if (!eq) continue;
        *eq = 0;
        char* k = line; char* v = eq + 1;
        char* nl = std::strpbrk(v, "\r\n"); if (nl) *nl = 0;
        if      (!std::strcmp(k, "backend")) g_vol.amixer = !std::strcmp(v, "amixer");
        else if (!std::strcmp(k, "path"))    std::strncpy(g_vol.path, v, sizeof g_vol.path - 1);
        else if (!std::strcmp(k, "control")) {
            // This value is interpolated into a /bin/sh command inside single quotes, and the file
            // it comes from lives on /contents — which the user can write over USB-MSC. A single
            // quote in the name would break out of the quoting, so anything outside the character
            // set real ALSA control names use is rejected outright rather than escaped: a name we
            // don't recognise is far more likely to be a typo than something worth running.
            bool safe = v[0] != 0;
            for (const char* q = v; *q && safe; ++q) {
                safe = (*q >= 'a' && *q <= 'z') || (*q >= 'A' && *q <= 'Z') ||
                       (*q >= '0' && *q <= '9') || *q == ' ' || *q == '_' || *q == '-' ||
                       *q == '.' || *q == ',' || *q == '(' || *q == ')' || *q == '/';
            }
            if (safe) std::strncpy(g_vol.control, v, sizeof g_vol.control - 1);
            else clog_("volume: control name has unsafe characters — IGNORED (check cinder_volume.conf)");
        }
        else if (!std::strcmp(k, "card"))    g_vol.card = (int)std::strtol(v, nullptr, 10);
        else if (!std::strcmp(k, "min"))     g_vol.min = (int)std::strtol(v, nullptr, 10);
        else if (!std::strcmp(k, "max"))     g_vol.max = (int)std::strtol(v, nullptr, 10);
    }
    std::fclose(f);
    g_vol.valid = (g_vol.max > g_vol.min) && (g_vol.amixer ? g_vol.control[0] : g_vol.path[0]);
}

// Read the device's CURRENT hardware volume and seed the UI level from it (once, at boot), so the
// first Vol± press nudges from the real level instead of jumping the hardware to the UI default.
// amixer backend only (cget + parse "values="); sysfs backend reads the node directly. Guarded.
// Read the raw hardware volume through the configured backend. -1 = unreadable (no backend, the
// node/control is missing, or the value parsed out of range). Split out of sync_volume_from_hw so
// the reconnect resync can VERIFY the mixer before it writes anything to it.
long read_volume_hw() {
    if (!g_vol_read) load_vol_cfg();
    if (!g_vol.valid) return -1;
    long val = -1;
    if (g_vol.amixer) {
        char cmd[384];
        std::snprintf(cmd, sizeof cmd, "amixer -c %d cget name='%s' 2>/dev/null", g_vol.card, g_vol.control);
        FILE* p = popen(cmd, "r");
        if (!p) return -1;
        char line[256];
        while (std::fgets(line, sizeof line, p)) {
            const char* v = std::strstr(line, ": values=");
            if (v) { val = std::strtol(v + 9, nullptr, 10); break; }
        }
        pclose(p);
    } else {
        FILE* f = std::fopen(g_vol.path, "r");
        if (!f) return -1;
        char buf[32] = {0};
        if (std::fgets(buf, sizeof buf, f)) val = std::strtol(buf, nullptr, 10);
        std::fclose(f);
    }
    if (val < g_vol.min || val > g_vol.max) return -1;   // parse failed / out of range
    return val;
}

// Raw hardware value -> the UI's 0..120 step level. Guarded against a degenerate conf range
// (min == max), which would otherwise divide by zero.
int volume_hw_to_level(long val) {
    long span = g_vol.max - g_vol.min;
    if (span <= 0) return 0;
    return (int)((val - g_vol.min) * 120 / span);
}

void sync_volume_from_hw() {
    long val = read_volume_hw();
    if (val < 0) return;   // unreadable — keep the UI's default
    // UI level is the stock 0..120 scale; with the default backend (min 0, max 120) this is 1:1.
    int level = volume_hw_to_level(val);
    cinder_set_volume(level);
    char m[96];
    std::snprintf(m, sizeof m, "volume: hw %ld -> UI level %d/120", val, level);
    clog_(m);
}

// Re-assert the UI's volume level on the 3.5 mm mixer IF the hardware has drifted from it.
//
// Report: "Bluetooth volume can become disconnected after it reconnects." Cinder OWNS the
// hardware volume — Vol± writes the UI's 0..120 level to the CXD3778GF master control — but that
// was a one-shot write with nothing ever re-asserting it. When an output is torn down and
// re-opened (a headphone drops and reconnects, the panel wakes), the mixer no longer necessarily
// holds what the UI believes it holds, so the on-screen level is a lie and the first Vol± press
// either does nothing visible or jumps.
//
// VERIFY-FIRST is what makes this safe to call often: read the mixer back and only write when it
// disagrees with the UI. A resync can therefore never fight a level the user just set, and costs
// one read when everything is already in step.
void bt_resync_volume(const char* why) {
    if (!g_vol_read) load_vol_cfg();
    if (!g_vol.valid) return;
    long hw = read_volume_hw();
    if (hw < 0) return;
    int have = volume_hw_to_level(hw);
    int want = cinder_get_volume();
    if (have == want) return;   // already in step — nothing to do
    char m[128];
    std::snprintf(m, sizeof m, "volume: resync after %s — hw %d/120 != UI %d/120, re-applying",
                  why, have, want);
    clog_(m);
    apply_volume();
}

// Apply the UI's 0..120 volume level to the device via the configured backend (1:1 with the
// default amixer 'master volume' 0..120; rescaled only for a conf-overridden range). No-op if
// unconfigured. Called guarded (system()/sysfs write). Read-on-first-use.
// Volume writes are COALESCED. The rocker auto-repeats a step every 120 ms while held, and the
// amixer backend costs a fork+exec of /bin/sh AND of amixer per step — on a single-core ARMv7 that
// is tens of milliseconds, eight times a second, competing with the render thread for the only
// core. During a ramp only the FINAL value matters, so a step marks the level pending and the
// actual write happens at most every VOL_WRITE_EVERY_MS, with a trailing flush so the value the
// user stopped on is always the one that lands.
const long VOL_WRITE_EVERY_MS = 150;
int  g_vol_pending = -1;      // level waiting to be written (-1 = nothing pending)
int  g_vol_written = -1;      // last level actually written (dedupe)
long g_vol_write_ms = 0;

// Do the write. Split out so both the rate-limited path and the trailing flush share it.
void volume_write_now(int level) {
    if (!g_vol.valid) return;
    int val = g_vol.min + (g_vol.max - g_vol.min) * level / 120;
    if (g_vol.amixer) {
        char cmd[384];
        std::snprintf(cmd, sizeof cmd, "amixer -c %d cset name='%s' %d >/dev/null 2>&1",
                      g_vol.card, g_vol.control, val);
        std::system(cmd);
    } else {
        FILE* f = std::fopen(g_vol.path, "w");
        if (f) { std::fprintf(f, "%d", val); std::fclose(f); }
    }
    g_vol_written = level;
    g_vol_write_ms = now_ms();
}

void apply_volume() {
    if (!g_vol_read) load_vol_cfg();
    if (!g_vol.valid) return;
    int level = cinder_get_volume();
    if (level < 0) level = 0;
    if (level > 120) level = 120;
    if (level == g_vol_written) { g_vol_pending = -1; return; }   // nothing changed
    if (now_ms() - g_vol_write_ms >= VOL_WRITE_EVERY_MS) {
        g_vol_pending = -1;
        volume_write_now(level);
    } else {
        g_vol_pending = level;   // flushed by volume_flush() below
    }
}

// Trailing flush: writes the level the user actually stopped on. Called every pump iteration, so
// the last step of a ramp lands within VOL_WRITE_EVERY_MS of the button coming up.
void volume_flush() {
    if (g_vol_pending < 0) return;
    if (now_ms() - g_vol_write_ms < VOL_WRITE_EVERY_MS) return;
    int level = g_vol_pending;
    g_vol_pending = -1;
    volume_write_now(level);
}

// ── Backlight (night = minimal light) ───────────────────────────────────────────────────────
// The night/day theme drives the PANEL BACKLIGHT: night mode dims it to a minimal level. The node
// is auto-detected (the common Android/MTK paths) and overridable via /contents/cinder_backlight.conf
// (path, night, day raw values). If no node is writable, it's a no-op (the device keeps its own
// brightness). Levels default to a tiny fraction of max_brightness for night, ~70% for day.
// `day_pinned` = the conf gave an explicit `day=`. It exists so recompute_day_level() knows to
// leave the value alone: the file is the documented escape hatch for a device whose auto-detected
// node or max_brightness scale is wrong, and an escape that a later feature silently overwrites is
// not an escape.
struct BlCfg { int valid = 0, night = -1, day = -1, max = 255, day_pinned = 0, night_pinned = 0; char path[256] = {0}; };
BlCfg g_bl;
bool g_bl_read = false;

void bl_max_sibling(const char* path, char* out, size_t cap) {
    std::strncpy(out, path, cap - 1);
    char* slash = std::strrchr(out, '/');
    if (slash) std::snprintf(slash + 1, cap - (size_t)(slash + 1 - out), "max_brightness");
}

void load_bl_cfg() {
    g_bl_read = true;
    char cfg_path[256] = {0};
    int cfg_night = -1, cfg_day = -1;
    FILE* f = std::fopen("/contents/cinder_backlight.conf", "r");
    if (f) {
        char line[256];
        while (std::fgets(line, sizeof line, f)) {
            if (line[0] == '#' || line[0] == '\n') continue;
            char* eq = std::strchr(line, '='); if (!eq) continue; *eq = 0;
            char* k = line; char* v = eq + 1;
            char* nl = std::strpbrk(v, "\r\n"); if (nl) *nl = 0;
            if      (!std::strcmp(k, "path"))  std::strncpy(cfg_path, v, sizeof cfg_path - 1);
            else if (!std::strcmp(k, "night")) cfg_night = (int)std::strtol(v, nullptr, 10);
            else if (!std::strcmp(k, "day"))   cfg_day   = (int)std::strtol(v, nullptr, 10);
        }
        std::fclose(f);
    }
    // Resolve a writable backlight node: config path, then the common Android LED, then scan
    // /sys/class/backlight/*. (access(W_OK) so we don't pick a read-only/stub node.)
    if (cfg_path[0] && access(cfg_path, W_OK) == 0) {
        std::strncpy(g_bl.path, cfg_path, sizeof g_bl.path - 1);
    } else if (access("/sys/class/leds/lcd-backlight/brightness", W_OK) == 0) {
        std::strncpy(g_bl.path, "/sys/class/leds/lcd-backlight/brightness", sizeof g_bl.path - 1);
    } else {
        DIR* d = opendir("/sys/class/backlight");
        if (d) {
            struct dirent* e;
            while ((e = readdir(d)) != nullptr) {
                if (e->d_name[0] == '.') continue;
                char p[256];
                std::snprintf(p, sizeof p, "/sys/class/backlight/%s/brightness", e->d_name);
                if (access(p, W_OK) == 0) { std::strncpy(g_bl.path, p, sizeof g_bl.path - 1); break; }
            }
            closedir(d);
        }
    }
    if (!g_bl.path[0]) return;
    // max_brightness (for the percentage defaults)
    char mp[280]; bl_max_sibling(g_bl.path, mp, sizeof mp);
    FILE* mf = std::fopen(mp, "r");
    if (mf) { char b[16] = {0}; if (std::fread(b, 1, sizeof b - 1, mf) > 0) { int m = (int)std::strtol(b, nullptr, 10); if (m > 0) g_bl.max = m; } std::fclose(mf); }
    // Night = a tiny fraction (minimal light, floored to 1 so it's not fully off); Day ~70%.
    g_bl.night = (cfg_night >= 0) ? cfg_night : (g_bl.max * 3 / 100 > 0 ? g_bl.max * 3 / 100 : 1);
    g_bl.day   = (cfg_day   >= 0) ? cfg_day   : (g_bl.max * 70 / 100);
    g_bl.day_pinned = (cfg_day >= 0) ? 1 : 0;
    // An explicit `night=` in the conf still wins over the derived value, same as `day=`.
    g_bl.night_pinned = (cfg_night >= 0) ? 1 : 0;
    g_bl.valid = 1;
    // Say so in the log: with `day=` pinned the Settings Brightness row still cycles 1..5 and
    // persists, but it no longer moves the panel. That is the override working as intended, and
    // this line is what tells the next person reading cinderhome.log why the row looks dead.
    if (g_bl.day_pinned) clog_("backlight: day= pinned by cinder_backlight.conf — Settings Brightness will not move the panel");
}

// Write the panel backlight: night → minimal, day → normal. No-op if no node / level unset.
void set_backlight(int night) {
    if (!g_bl_read) load_bl_cfg();
    if (!g_bl.valid) return;
    int level = night ? g_bl.night : g_bl.day;
    // BACKLIGHT OFF BEATS THE THEME. This line used to be the whole of the branch above, which
    // meant brightness level 0 only reached the panel in the DAY theme: in night theme the
    // ternary picked g_bl.night (3% of max = 7 raw) and the "off" setting looked identical to the
    // lowest normal one — exactly what it was reported as. Level 0 is a deliberate, transient
    // state; nothing may override it.
    if (cinder_get_brightness() == 0) level = 0;
    if (level < 0) return;
    FILE* f = std::fopen(g_bl.path, "w");
    if (f) { std::fprintf(f, "%d", level); std::fclose(f); }
}

// Apply the UI's brightness level (1..5) by recomputing the DAY level, then re-writing the panel.
// The level is a percentage of the node's own max_brightness, so it works whatever the raw scale is.
//
// LEVEL 1 IS 15%, NOT 0. The lowest setting reachable from the UI has to stay readable: if it
// blanked the panel, the Settings screen needed to turn it back up would be invisible, and the
// brightness row is persisted — so a single tap could make the device look bricked across reboots.
// (Same reasoning as the boot-always-day rule for night dimming.) An explicit `day=` in
// /contents/cinder_backlight.conf still wins, exactly as before, so the file stays the escape hatch.
// Night theme as a percentage of the current day level. 8% of the level-1 day value (38 of 255)
// lands on 3 raw units — dim enough to read in the dark without lighting the room, and well below
// the old fixed 7.
static const int NIGHT_PCT = 8;

void recompute_day_level() {
    if (!g_bl_read) load_bl_cfg();
    if (!g_bl.valid) return;
    // The conf's `day=` really does win — it did not before this check, which made the escape hatch
    // the comment above promises a no-op: load_bl_cfg parsed the value and then this function
    // overwrote it on the very next line of render_up. That matters most in the case the file
    // exists for: an auto-detected node or max_brightness that produces an unreadable panel, where
    // the UI you would use to fix it is the thing that is unreadable.
    if (g_bl.day_pinned) return;
    static const int pct[5] = { 15, 30, 50, 70, 100 };
    int lvl = cinder_get_brightness();          // 0..5 from cinder-ffi
    // LEVEL 0 = BACKLIGHT FULLY OFF, with the app still running and still taking input.
    //
    // Every other level is floored at 1 raw unit for the reason the comment above gives: a
    // *persisted* setting that blanks the panel would hide the Settings screen you need to undo
    // it, across reboots. Level 0 sidesteps that instead of ignoring it — it is TRANSIENT. It is
    // never written to cinder_settings.conf (cinder-ffi persists `brightness_restore`), and the
    // next input event restores the previous level (see the g_bl_zero_at handling in input_pump).
    // So the escape does not depend on being able to see anything.
    if (lvl == 0) { g_bl.day = 0; g_bl.night = 0; return; }
    if (lvl < 1 || lvl > 5) lvl = 4;
    g_bl.day = g_bl.max * pct[lvl - 1] / 100;
    if (g_bl.day < 1) g_bl.day = 1;             // never fully dark
    // NIGHT FOLLOWS THE BRIGHTNESS ROW. It used to be a fixed 3% of max_brightness computed once
    // in load_bl_cfg(), which had two consequences: the Brightness row did nothing at all while
    // the night theme was on, and the floor (7 raw of 255) was brighter than a dark room wants.
    // Deriving it from the day level fixes both — the row works in either theme, and the darkest
    // reachable lit state is now 1 raw unit instead of 7.
    //
    // Still floored at 1 rather than 0: this one IS persisted, and a persisted setting that blanks
    // the panel would hide the Settings screen needed to undo it. Level 0 is the transient escape
    // for true darkness, and it now works in night theme too (see set_backlight).
    if (!g_bl.night_pinned) {
        g_bl.night = g_bl.day * NIGHT_PCT / 100;
        if (g_bl.night < 1) g_bl.night = 1;
    }
}

// When the backlight was last taken to 0 by the brightness row. The restore is debounced against
// this: the very gesture that SELECTS level 0 also generates a touch release, and restoring on
// that would make the setting impossible to reach.
long g_bl_zero_at = 0;

// Next input after a level-0 blank: come back. Cheap and unconditional — cinder_brightness_wake
// returns 0 immediately unless the UI is actually in the transient state.
void brightness_wake_on_input() {
    if (g_bl_zero_at == 0) return;
    if (now_ms() - g_bl_zero_at < 400) return;   // debounce the selecting gesture's own release
    g_bl_zero_at = 0;
    if (cinder_brightness_wake()) {
        clog_("backlight: input -> restoring from BACKLIGHT OFF");
        recompute_day_level();
        set_backlight(cinder_get_night());
    }
}

// Live change from the Settings row: recompute, then write at the CURRENT theme's level.
void apply_brightness() {
    recompute_day_level();
    set_backlight(cinder_get_night());
    // Arm (or disarm) the "any input brings it back" escape.
    g_bl_zero_at = (cinder_get_brightness() == 0) ? now_ms() : 0;
    if (g_bl_zero_at) clog_("backlight: BACKLIGHT OFF (transient — next input restores it)");
}

// ── Boot to stock (Settings ▸ Boot to stock, after the row's two-tap confirm) ─────────────────
// Arms the launcher's ONE-SHOT flag and then restarts into Sony's player.
//
// One-shot, not the persistent $OFF latch: this is the ONLY escape a user can reach with no USB
// cable, and every other route back to Cinder (cinderhome_clear over USB-MSC, or installing a newer
// binary) needs one. A persistent flag here would let someone leave Cinder without a cable and then
// be unable to return without one. cinderhome-launch.sh consumes the flag on the boot it fires, so
// the boot after that is Cinder again.
//
// Written to BOTH filesystems on purpose: /data is ext4 and journaled (the launcher's real home for
// state), /contents is vfat but visible over USB-MSC, so the user can see and delete the flag from
// a PC if anything goes wrong. Either one alone is enough for the launcher.
//
// The restart itself needs no root: appmgr watches the Home app and calls android_reboot when it
// dies (analysis/F_appmgr_home/RE_findings.md §2). So _exit() IS the reboot. The flag is synced
// first, so it survives regardless of how abrupt that reboot turns out to be.
void boot_to_stock() {
    bool armed = false;
    for (const char* p : { "/data/cinder/once_stock", "/contents/cinderhome_once" }) {
        FILE* f = std::fopen(p, "w");
        if (f) { std::fclose(f); armed = true; clog_(p); }
    }
    ::sync();
    if (!armed) {
        // Nothing written => a restart now would just come back to Cinder. Say so and stay put,
        // rather than rebooting the device for no reason.
        clog_("boot-to-stock: could NOT arm the flag on either filesystem — staying on Cinder");
        return;
    }
    clog_("boot-to-stock: armed; exiting so appmgr restarts the device into the Sony player");
    // ORDER MATTERS: the flag is written and sync()'d ABOVE, before anything else runs. Keep it
    // that way. cinder_render_shutdown joins the present thread, which blocks if the display driver
    // has wedged — so if this ever hangs, the user power-cycles and STILL lands on stock, because
    // the flag was already durable. Doing the sync after the shutdown would make the cable-free
    // escape depend on a healthy present path, i.e. on more than it is there to rescue.
    cinder_render_shutdown();   // release the framebuffer so the reboot isn't fighting our mapping
    std::fflush(nullptr);
    ::sync();
    _exit(0);
}

// Power off / Restart. Goes through the setuid-root cinder-power helper (reboot(2)), NOT through
// PowerMgrServiceClient — measured 2026-07-28, Sony's Reboot() froze the player and
// SetStatus(PowerOff) only slept it, because shutdown is a two-phase barrier across every
// registered service and Cinder-as-Home-app never acknowledges its phase. See src/cinder-power.c.
//
// NOT run_guarded: on success this call never returns (the machine goes down inside it), and the
// guard's whole job is to _exit on a call that does not return — which would trip the launcher's
// bad-boot counter on the one path that is SUPPOSED to take the device away. If the helper is
// missing or lost its setuid bit, system() returns promptly and we log a real cause and stay up.
void power_action(bool restart) {
    const char* verb = restart ? "restart" : "off";
    char m[160];
    std::snprintf(m, sizeof m, "power: %s confirmed — exec cinder-power %s", verb, verb);
    clog_(m);
    // Write the resume state at its true value before the machine goes down. The 1 Hz saver is
    // rate-limited to 5 s, so without this a deliberate power-off would come back up to five
    // seconds behind where the user actually stopped.
    cinder_resume_flush();
    // Sync BEFORE the helper, not only inside it: /contents is vfat holding the settings file and
    // this log, and the helper's own remount-ro can legitimately fail with EBUSY while we still
    // hold the log open. Two syncs cost nothing next to a shutdown.
    std::fflush(nullptr);
    ::sync();
    std::snprintf(m, sizeof m, "/system/vendor/unknown321/bin/cinder-power %s", verb);
    int rc = std::system(m);
    // Reached only on failure — the helper does not return when reboot(2) succeeds.
    std::snprintf(m, sizeof m,
                  "power: %s FAILED (cinder-power rc=%d) — helper missing or setuid bit lost; staying up",
                  verb, rc);
    clog_(m);
}

// Settings > Date & time > SET CLOCK. Hands the epoch to the setuid helper, which is the only
// thing on this device that can move either clock: settimeofday(2) and RTC_SET_TIME both want
// CAP_SYS_TIME, and cinder-home runs as uid 100 (system) with an empty capability set.
//
// The epoch is an INTEGER from the UI, already clamped by cinder-ui to the same 2001..2038 range
// the helper enforces independently — so unlike apply_volume's control name (which comes from a
// user-writable file on /contents) there is no string here to character-check. It is still printed
// with %lld into a fixed buffer and nothing else from the UI reaches the command line.
static void apply_clock() {
    long long epoch = cinder_get_clock_epoch();
    char cmd[128];
    std::snprintf(cmd, sizeof cmd,
                  "/system/vendor/unknown321/bin/cinder-clock set %lld", epoch);
    int rc = std::system(cmd);
    // system() returns a wait status; the helper's own code is in the low byte of the exit status.
    int code = (rc == -1) ? -1 : ((rc >> 8) & 0xff);
    char m[192];
    const char *what;
    switch (code) {
        case 0:  what = "system clock + RTC set"; break;
        case 5:  what = "system clock set, RTC WRITE FAILED (will not survive a power cycle)"; break;
        case 2:  what = "REJECTED: epoch outside 2001..2038 or malformed"; break;
        case 3:  what = "REFUSED: helper is not setuid root"; break;
        case 4:  what = "settimeofday failed"; break;
        default: what = "unknown result"; break;
    }
    std::snprintf(m, sizeof m, "clock: set %lld -> rc=%d (%s)", epoch, code, what);
    clog_(m);
    // Push the new time straight back, so the Settings row and the status bar do not wait for the
    // next housekeeping tick to stop showing the old one.
    if (code == 0 || code == 5) cinder_set_clock_epoch((long long)::time(nullptr));
}

// For the LIVE theme toggle: match the backlight to the current theme.
void apply_backlight() { set_backlight(cinder_get_night()); }

// Power button = screen on/off. OFF writes backlight 0 (panel dark; the app keeps rendering so
// playback/Hold-state continue); ON restores the current theme's level. Pure sysfs write, no Sony
// service. Locking is independent (the Hold switch) — waking the screen never unlocks the touch.
bool g_screen_on = true;      // see the fwd decl above
// True while we are dropping the remainder of the contact that woke the panel. See the touch pump:
// waking must never also press whatever the finger landed on.
static bool g_touch_wake_swallow = false;
// The himax touch controller has a driver sysfs SLEEP switch — and something must write it, or
// the controller never scans and /dev/input/event1 stays silent forever (opens fine, EVIOCGABS
// answers, zero events — the exact 2026-07-02 symptom). The stock Qt app is what normally wakes
// it; with cinder-home as the Home app nothing did. Wampy has the same problem and the same fix
// (write "0" = awake, "1" = sleep): src/connector/hagoromo.cpp enableTouchscreen(). Paths are
// the A50-family node with the WM1Z variant as fallback (Wampy's pair, verbatim).
// Sony's touch-panel power switch, reached by dlopen so cinder-home gains no DT_NEEDED for it.
// Returns true if the service accepted the call; false means "not available, use the node path".
// The client is built once and kept — CreateInstance is a binder handshake, and this is called on
// every screen toggle.
static bool touch_panel_service(bool valid) {
    enum { VIDX_SetTouchPanelValidate = 13 };
    static void* cli = nullptr;
    static bool tried = false;
    if (!tried) {
        tried = true;
        void* h = dlopen("libDisplayService.so", RTLD_NOW);
        if (!h) { clog_("touch: libDisplayService.so unavailable — falling back to the sysfs node"); return false; }
        typedef void* (*mk)(void);
        mk f = (mk)dlsym(h, "_ZN3pst8services27DisplayServiceClientFactory14CreateInstanceEv");
        if (!f) { clog_("touch: DisplayServiceClientFactory symbol missing"); return false; }
        try { cli = f(); } catch (...) { cli = nullptr; }
        clog_(cli ? "touch: DisplayServiceClient ready (SetTouchPanelValidate)"
                  : "touch: DisplayServiceClientFactory returned null");
    }
    if (!cli) return false;
    try {
        typedef int (*fnb)(void*, const bool*);
        void** vt = *(void***)cli;
        ((fnb)vt[VIDX_SetTouchPanelValidate])(cli, &valid);
    } catch (...) { clog_("touch: SetTouchPanelValidate threw"); return false; }
    char m[96];
    std::snprintf(m, sizeof m, "input: touch %s via DisplayService::SetTouchPanelValidate(%d)",
                  valid ? "WAKE" : "SLEEP", (int)valid);
    clog_(m);
    return true;
}

void touch_set_sleep(int slp) {
    static const char* paths[] = {
        "/sys/devices/platform/mt-i2c.1/i2c-1/1-0048/sleep",  // nw-a50/40/30/zx300
        "/sys/devices/platform/mt-i2c.1/i2c-1/1-0020/sleep",  // wm1z
    };
    auto write_node = [&](const char* p) -> bool {
        FILE* f = std::fopen(p, "w");
        if (!f) return false;
        std::fputc(slp ? '1' : '0', f); std::fputc('\n', f);
        std::fclose(f);
        char m[128];
        std::snprintf(m, sizeof m, "input: touch %s via %s", slp ? "SLEEP" : "WAKE", p);
        clog_(m);
        return true;
    };
    for (const char* p : paths)
        if (write_node(p)) return;
    // Wampy's two known paths aren't on this fw (observed 2026-07-02) — scan the i2c bus for any
    // device exposing a `sleep` attribute (on this platform only the touch controller has one).
    DIR* d = opendir("/sys/bus/i2c/devices");
    if (d) {
        struct dirent* de;
        bool hit = false;
        while ((de = readdir(d))) {
            if (de->d_name[0] == '.') continue;
            char p[160];
            std::snprintf(p, sizeof p, "/sys/bus/i2c/devices/%s/sleep", de->d_name);
            if (write_node(p)) hit = true;   // write every match (in practice: exactly one)
        }
        closedir(d);
        if (hit) return;
    }
    // No node on this unit — measured 2026-08-11, and the log line below has been printing on
    // every screen toggle since 2026-07-02, which means the touch controller has NEVER slept. It
    // is a capacitive panel scanning continuously with the screen off, so this was a real standby
    // cost hiding behind a "harmless" message.
    //
    // Sony does it through a service, not a node: DisplayService::SetTouchPanelValidate(bool),
    // vtable slot 13 (recovered from libDisplayService.so's .data.rel.ro relocations; see
    // cinder-probe --disp, which also proved the round trip reverses — GetTouchPanelValidate went
    // 1 → 0 → 1 across a --dispoff window on device).
    //
    // dlopen, NOT a link. A DT_NEEDED on the Home app for a path this thin is how you get a device
    // that boots to nothing (the libNfcService rule), and there is nothing to gain: if the handle
    // or the symbol is missing we simply fall through to the old no-op, exactly as today.
    //
    // Escape ladder: this only ever runs from screen_toggle() — the POWER-BUTTON blank — never
    // from the idle blank, which must keep touch alive to be woken by it. So the thing that
    // rescues a sleeping touch panel is a key event, which depends on strictly less than touch.
    // apply_touch_panel_valid(true) additionally runs on every wake and once at boot, so a stuck
    // "invalid" cannot outlive the next Power press or the next reboot.
    if (touch_panel_service(!slp)) return;
    if (!slp) clog_("input: touch WAKE — no touch sleep node found (harmless: the held evdev grab keeps events flowing; screen-off tap-drop is handled in input_pump)");
}

// ── Idle screen-off (opt-in, Settings > Screen-off timer) ─────────────────────────────────────
// Blanks the BACKLIGHT after N seconds with no input, to save power (goal #1). Two rules make this
// safe enough to ship without a device to test on:
//
//  1. It is OFF by default. Nothing changes unless the user picks a duration, so the worst case is
//     opt-in rather than inflicted on every boot.
//  2. The auto-off path does NOT sleep the touch controller, unlike the Power-button path. A
//     sleeping controller reports nothing, so wake-on-touch would be impossible and a dark panel
//     would look like a dead device. Keeping it awake costs a little current; the backlight is the
//     dominant draw anyway.
//
// And the escape ladder still holds: even if wake-on-touch fails entirely, the physical Power
// button restores the screen. That escape depends on strictly less than the thing it rescues (a
// key event vs. the whole touch stack).
static bool g_screen_auto_off = false;   // dark because of the idle timer (not the Power button)
static bool g_held = false;              // Hold/lock switch engaged (mirrors cinder_set_hold)

// Adopt the Hold switch's ACTUAL position, rather than waiting for it to move.
//
// evdev delivers state CHANGES. The only place cinder_set_hold() was ever called is the event
// branch in input_pump, so a device booted with Hold ALREADY engaged came up UNLOCKED — and stayed
// that way until the switch was flicked twice, with the UI's idea of the lock INVERTED relative to
// the physical switch in between. Reported 2026-08-16. A pocket-safe device that boots unlocked
// when the switch says locked is the wrong way round for this bug to fail.
//
// Asserts BOTH ways on purpose. This also runs on any path that reopens the nodes, where the switch
// may have been released while we were not listening — so "no hold code is down" has to actively
// clear the state, not merely decline to set it.
//
// It is also the groundwork for a planned escape: a quick Hold ON→OFF flick as the failsafe out of
// a fully-dark backlight (see the backlight-off work). That escape is only trustworthy if the lock
// state can be read rather than inferred, which is what this gives it.
static void sync_hold_from_hw() {
    // WHICH codes mean Hold is a question for the LIVE keymap, not for a hardcoded 35:
    // cinder_keymap.conf can move it, and a device whose conf remaps the switch must still come up
    // locked. Collect them first so each node is only walked once.
    int codes[16];
    int ncodes = 0;
    for (int c = 0; c < keymap_size() && ncodes < (int)(sizeof codes / sizeof *codes); ++c)
        if (g_keymap[c] == CINDER_BTN_HOLD) codes[ncodes++] = c;
    if (ncodes == 0) return;   // nothing is mapped to Hold — nothing to read

    // One bit per code, so the buffer must cover the highest code we will test (keymap_size()).
    unsigned char bits[(768 + 7) / 8];
    bool held = false;
    int on_node = -1, on_code = -1;
    for (int i = 0; i < g_evn && !held; ++i) {
        std::memset(bits, 0, sizeof bits);
        if (ioctl(g_evfds[i], eviocgkey(sizeof bits), bits) < 0) continue;   // node carries no keys
        for (int k = 0; k < ncodes; ++k) {
            const int c = codes[k];
            if (c < 0 || c >= (int)(sizeof bits) * 8) continue;
            if (bits[c / 8] & (1u << (c % 8))) { held = true; on_node = i; on_code = c; break; }
        }
    }

    // Only speak when it changes something. At boot g_held is already false, so the ordinary case
    // (switch off) writes nothing and logs nothing.
    if (held == g_held) return;
    g_held = held;
    cinder_set_hold(held ? 1 : 0);
    char m[128];
    if (held)
        std::snprintf(m, sizeof m,
                      "input: Hold switch is ALREADY ENGAGED (node %d, code %d) — locking",
                      on_node, on_code);
    else
        std::snprintf(m, sizeof m, "input: Hold switch reads released — unlocking");
    clog_(m);
}
long g_last_input_ms = 0;                // when we last saw ANY input event (see the fwd decl above)

// Blank the panel WITHOUT sleeping the touch controller, so a touch can wake it.
static void screen_auto_off() {
    if (!g_screen_on) return;
    g_screen_on = false;
    g_screen_auto_off = true;
    // Drop any in-flight contact so the waking touch starts a fresh gesture.
    g_touch_down = false; g_touch_start_x = -1; g_touch_start_y = -1; g_touch_saw_pos = false;
    g_drag_active = false; g_drag_vel = 0.0f;
    g_scrub_active = false; g_scrub_tested = false; g_hswipe_active = false; g_reorder_active = false; g_sbar_active = false;
    if (!g_bl_read) load_bl_cfg();
    if (g_bl.valid) {
        FILE* f = std::fopen(g_bl.path, "w");
        if (f) { std::fputc('0', f); std::fclose(f); }
    }
    apply_pump_interval();   // nothing on screen needs 50 Hz IPC latency
    clog_("screen: idle timeout -> panel off (touch or Power wakes it)");
}

// Wake from an idle blank. No-op unless WE turned it off: a Power-button blank must stay off until
// Power is pressed again (that is the pocket-safe case, and the Hold switch's job otherwise).
static void screen_auto_wake() {
    if (!g_screen_auto_off) return;
    g_screen_auto_off = false;
    g_screen_on = true;
    // Self-heal: the idle blank never sleeps the touch panel (it is what wakes us), but a Power
    // blank does now — and if the user Power-blanked, woke with Power, and the panel is dark again
    // by the idle timer, re-validating here costs one call and closes the only window where a
    // stuck "invalid" could survive.
    touch_set_sleep(0);
    apply_backlight();
    cinder_force_dirty();   // the render loop skipped painting while dark — repaint immediately
    apply_pump_interval();
    clog_("screen: woken by input");
}

void screen_toggle() {
    g_screen_auto_off = false;   // an explicit Power press takes ownership of the panel state
    g_screen_on = !g_screen_on;
    touch_set_sleep(g_screen_on ? 0 : 1);   // stock behaviour: TS sleeps with the panel (battery)
    // Drop any in-flight contact: the sleeping controller never sends its lift, and a stale
    // "down" would make the next touch classify as a drag from the old start point.
    g_touch_down = false; g_touch_start_x = -1; g_touch_start_y = -1; g_touch_saw_pos = false;
    apply_pump_interval();
    if (g_screen_on) {
        apply_backlight();
        cinder_force_dirty();
        // Waking is the other moment the mixer can have drifted from the level the UI is about to
        // show — the panel comes back with a number on it, and that number had better be true.
        // Verify-first, so this costs one read when nothing has changed.
        bt_resync_volume("screen wake");
        return;
    }
    if (!g_bl_read) load_bl_cfg();
    if (g_bl.valid) {
        FILE* f = std::fopen(g_bl.path, "w");
        if (f) { std::fputc('0', f); std::fclose(f); }
    }
}

// Persist the device-wide BT transmit codec preference to /contents/cinder_bt.conf, so every BT
// path uses the same choice: normal playback AND the USB-DAC→LDAC bridge (ldac-run.sh) read it. The
// LIVE apply via BtTransmitterService (SetLdac/SetAptxHD/SetSbc + SetLdacSoundQuality) is device-
// gated (the BT client shim, same boundary as ldac-bridge); this config write is the always-safe
// half (pure file IO, no Sony service). Names mirror the cinder.h codec/quality indices.
void write_bt_pref() {
    static const char* codecs[] = { "ldac", "aptxhd", "aptx", "sbc" };
    static const char* quals[]  = { "auto", "990", "660", "330" };
    int ci = cinder_get_bt_codec();        if (ci < 0 || ci > 3) ci = 0;
    int qi = cinder_get_bt_ldac_quality(); if (qi < 0 || qi > 3) qi = 0;
    FILE* f = std::fopen("/contents/cinder_bt.conf", "w");
    if (f) {
        std::fprintf(f, "codec=%s\nldac_quality=%s\n", codecs[ci], quals[qi]);
        std::fclose(f);
    }
}

// USB-DAC → LDAC (the headline feature): engage USB-DAC input and route it to 3.5mm + BT/LDAC at
// once, WITHOUT tearing down Bluetooth (we simply never call IBtTransmitterService::Request-
// Disconnection, which is what stock does). Engaging = start the LDAC bridge supervisor (it watches
// /contents/ldac_on; see deploy/ldac-run.sh) + switch the USB gadget to UAC. The setprop USB-mode
// switch is device-gated (disruptive; validate live) — run_guarded + best-effort so it can't wedge
// the UI. The codec/quality the bridge uses comes from /contents/cinder_bt.conf (write_bt_pref).
// Engage / release USB-DAC mode (the Walkman as a USB sound card for a PC).
//
// THROUGH THE SETUID HELPER, NOT setprop DIRECTLY. This used to run `setprop sys.sony.config uac`
// from here — and cinder-home is uid `system`, whose setprop the property service REFUSES. The
// shell still returns 0, so the property silently stayed "adb", init's `on
// property:sys.sony.config=uac` block never ran, the gadget was never reconfigured, and no PC ever
// saw a sound card — while this function logged "usb-dac: engaged". Exactly the failure that made
// USB-MSC look like a race for weeks (see cinder-msc.c's header), in exactly the same place.
//
// cinder-msc now owns the verb, reads sys.usb.state back, and says which way it went.
// ── Bluetooth: make the switch drive the RADIO ───────────────────────────────────────────────
//
// Until 2026-07-29 the Settings switch raised `Action::BtToggle`, cinder-ffi dropped it ("UI-only"),
// and nothing ever reached the hardware. So the switch and the radio were independent, and paired
// headphones never reconnected — which is what "Bluetooth doesn't connect automatically" actually
// was. Two clients are involved, and they are different services:
//
//   BtCommonServiceClient      slot 3 GetBtStatus, slot 4 SetRfOnOff(const bool*)   — the RADIO
//   BtTransmitterServiceClient slot 7 RequestLastDeviceConnection()  (genuinely zero-arg)
//
// THE STATUS ENUM: 7 means the radio is OFF. 2 is on/idle, 3 is connected.
//
// This was initially misread as "the stack is wedged", because a probe run saw 7, sent a connect,
// and watched it vanish. The device log settles it — turning the radio OFF makes the next read
// report 7:
//     bt: toggle OFF (GetBtStatus=2)          <- was on
//     bt: toggle ON  (GetBtStatus=7)          <- reads 7 immediately after being switched off
// So SetRfOnOff(false) PRODUCES 7. There was never a wedge: the radio was simply off, Cinder's
// switch claimed otherwise, and a connect against a powered-down radio is accepted and silently
// dropped (the service logs "last device found [MAC]" and nothing more — MTK's stack logs nothing
// to ANY logcat buffer, so there is no failure to observe). The original probe only called
// SetRfOnOff when status was 0, so it never powered the radio up at all.
//
// Hence: anything that is not 2 or 3 is off, and the fix is one SetRfOnOff(true) — no power cycle.
extern "C" void* _ZN3pst8services28BtCommonServiceClientFactory14CreateInstanceEv(void);
extern "C" void* _ZN3pst8services33BtTransmitterServiceClientFactory14CreateInstanceEv(void);

static void* g_bt_common = nullptr;
static void* g_bt_xmit   = nullptr;

static void* bt_slot(void* obj, int idx) {
    void** vptr = *reinterpret_cast<void***>(obj);
    return vptr[idx];
}

// Is this status a radio that is actually up? 2 = on/idle, 3 = connected; everything else (7 = off,
// 0 = unknown/error, -1 = no client) is not. Single definition so the toggle, the startup reconcile
// and the log all agree on what "on" means.
static bool bt_radio_up(int st) { return st == 2 || st == 3; }

// Current radio status, or -1 if the client could not be built.
static int bt_status() {
    enum { VIDX_GetBtStatus = 3 };
    try {
        if (!g_bt_common) g_bt_common = _ZN3pst8services28BtCommonServiceClientFactory14CreateInstanceEv();
        if (!g_bt_common) return -1;
        typedef int (*fn0)(void*);
        return ((fn0)bt_slot(g_bt_common, VIDX_GetBtStatus))(g_bt_common);
    } catch (...) { return -1; }
}

static void bt_set_rf(bool on) {
    enum { VIDX_SetRfOnOff = 4 };
    if (!g_bt_common) return;
    typedef void (*fnb)(void*, const bool*);
    ((fnb)bt_slot(g_bt_common, VIDX_SetRfOnOff))(g_bt_common, &on);
}

// Apply the Settings switch to the radio. Run ONLY via run_guarded — every call here is Sony IPC.
void apply_bt_toggle() {
    bool want = cinder_get_bt_on() != 0;
    // Either direction is the user taking charge of the radio: ON obviously re-arms, and OFF should
    // not leave a stale "they disconnected on purpose" latch to suppress the reconnect after the
    // next ON.
    bt_reconnect_rearm();
    int st = bt_status();
    char m[160];
    std::snprintf(m, sizeof m, "bt: toggle %s (GetBtStatus=%d%s)",
                  want ? "ON" : "OFF", st, bt_radio_up(st) ? " up" : " off");
    clog_(m);
    if (st < 0) { clog_("bt: BtCommonServiceClient unavailable — switch is UI-only this session"); return; }

    if (!want) { bt_set_rf(false); return; }

    // Power the radio up if it is not already. KEEP THIS SHORT: it runs on the render/input thread,
    // so every millisecond here is a frozen UI. The first version polled for up to 5 s, and the
    // freeze read as "the switch doesn't work" — the user tapped again, and the second tap turned
    // Bluetooth straight back off. Poll briefly, then proceed regardless: the connect below is
    // asynchronous anyway, and the reconcile on the next Settings entry will show the true state.
    if (!bt_radio_up(st)) {
        bt_set_rf(true);
        for (int i = 0; i < 6 && !bt_radio_up(st); i++) { usleep(150000); st = bt_status(); }
        std::snprintf(m, sizeof m, "bt: radio after SetRfOnOff(true) = %d%s", st,
                      bt_radio_up(st) ? "" : "  (not up yet — connect may be dropped)");
        clog_(m);
    }

    // Codec BEFORE the connect, not after: A2DP negotiates the codec during connection setup, so a
    // preference applied to an already-established link doesn't take until the next one.
    apply_bt_codec();

    // Radio is up — reconnect whatever was last paired. Zero-arg: the service looks the address up
    // itself (confirmed both by decompiling the stub, which makes no TransactionParam::Set* call,
    // and by its own log line naming the MAC).
    enum { VIDX_RequestLastDeviceConnection = 7 };
    try {
        if (!g_bt_xmit) g_bt_xmit = _ZN3pst8services33BtTransmitterServiceClientFactory14CreateInstanceEv();
        if (g_bt_xmit) {
            typedef int (*fn0)(void*);
            ((fn0)bt_slot(g_bt_xmit, VIDX_RequestLastDeviceConnection))(g_bt_xmit);
            clog_("bt: RequestLastDeviceConnection() sent");
        }
    } catch (...) {
        clog_("bt: RequestLastDeviceConnection threw");
    }
    refresh_bt_route();
}

// ── Bluetooth volume ────────────────────────────────────────────────────────────────────────
// The volume rocker has to go somewhere ELSE once audio leaves the jack. The 3.5 mm level is the
// CXD3778GF codec master (ALSA card0 'master volume', 0..120) — that attenuator is downstream of
// nothing the A2DP encoder touches, so turning it up while headphones are connected does exactly
// what the user reported: nothing.
//
// The Bluetooth attenuator lives at the far end, in the headphones, reached over AVRCP. Two ways to
// drive it, and the good one is conditional on the sink:
//
//   * ABSOLUTE — `SetCurrentVolume(const uint8_t&)` (slot 34), gated on `IsSupportedAbsoluteVolume()`
//     (slot 33). Preferred wherever it works, because it is CLOSED LOOP: the UI level is the level,
//     so a saved level restores exactly and repeated presses can't accumulate drift.
//   * STEPS — `SetVolumeUp` / `SetVolumeDown` (slots 17/16), one sink step per call. Open loop, the
//     fallback for a sink that doesn't do absolute volume. The UI count is then only a belief about
//     where the far end actually is.
//
// Signature safety (the rule from GetConnectInformation, which crashed twice on a bogus out-param):
// these libraries carry their own demangled signatures as __PRETTY_FUNCTION__ log literals, so they
// are READ, not inferred — `strings` gives `virtual bool ...SetVolumeUp()` and
// `virtual bool ...SetCurrentVolume(const uint8_t &)` outright. The marshalling agrees: every stub
// costs a base 3×Alloc(4), and the ARGUMENT shows up as extra Allocs sized to it — SetVolumeUp has
// exactly the base 3 (no args), SetCurrentVolume has a 4th of size 1 (the uint8_t). None of them
// call GetStr, so no std::string comes back and a plain scalar reply is safe.
static void* bt_xmit() {
    if (!g_bt_xmit) g_bt_xmit = _ZN3pst8services33BtTransmitterServiceClientFactory14CreateInstanceEv();
    return g_bt_xmit;
}

// Last answer we LOGGED, so the transition is reported once rather than on every press. It is not
// a cache — nothing reads it to make a decision (see bt_abs_volume_supported).
static int g_bt_abs_vol = -1;

// Ask GetSoundStatus on the NEXT frame rather than waiting out its throttle. Set on the two events
// that can actually change what A2DP has agreed on: a link coming up (it renegotiates) and a codec
// preference being written. Same pattern as g_np_poll_now — see bt_poll_sound_status.
static volatile sig_atomic_t g_bt_sound_poll_now = 0;

// ── Bluetooth listener: the sink's OWN volume ────────────────────────────────────────────────────
// The headphones are the authority on their own level, and until now Cinder never asked. It kept an
// absolute 0..CINDER_BT_VOL_MAX counter and assumed the sink followed it — which is only true while
// SetCurrentVolume is being accepted. On a link that refuses absolute volume (measured: CMF Buds
// Pro 2 refusing EVERY press) each press becomes a blind relative step, the counter and the sink
// drift apart, and the UI ends up reading mute while audio plays. That was the report.
//
// Recovered + PROVEN on device 2026-08-11 with `cinder-probe --btvollisten`:
//   * `AddListener` / `RemoveListener` are client slots 39 / 40 — immediately after the last
//     service method (38, SetEnableLowLatency), the same rule BtCommon and Nfc both follow.
//   * `OnNotifyChangeVolume(const uint8_t&)` is listener slot 10, after the virtual destructor and
//     eight earlier callbacks. Verified by instrumenting every slot and moving the volume: only 10
//     fired, carrying 19 -> 15 -> 19 -> 15 against two up and two down steps.
//   * Sony's own relative step is 4 units of AVRCP's 0..127.
//
// The callback runs on the FRAMEWORK LOOPER, so it only stores a byte; the render loop applies it.
static volatile sig_atomic_t g_bt_vol_notify = -1;   // 0..127 from the sink, -1 = nothing new

struct CinderBtVolListener {
    // Virtual destructor first: Itanium ABI puts D1/D0 in slots 0/1 and the callbacks at 2+.
    virtual ~CinderBtVolListener() {}
    virtual void OnNotifyAvSrcConnectionStatus(const void*, const void*, const void*) {}
    virtual void OnNotifyAvrcpConnectionStatus(const void*, const void*, const void*) {}
    virtual void OnNotifyConnectInformation(const void*, const void*, const void*) {}
    virtual void OnNotifyAvrcpGetPlayStatus(const void*, const void*, const void*) {}
    virtual void OnNotifyAvrcpGetMediaAttribute(const void*, const void*, const void*) {}
    virtual void OnNotifyError(const void*, const void*, const void*) {}
    virtual void NotifyConfigiration(const void*, const void*, const void*) {}
    virtual void OnNotifySoundStatus(const void*, const void*, const void*) {}
    // Slot 10 — the one that matters. Everything above is a placeholder whose only job is to put
    // this at the right index; none of them dereference their arguments, because their real
    // signatures carry containers and a wrong guess would be a crash rather than a no-op.
    virtual void OnNotifyChangeVolume(const unsigned char& v) {
        g_bt_vol_notify = (sig_atomic_t)v;
    }
    virtual void OnNotifyUpdateCapabilities(const void*, const void*, const void*) {}
};
// MUST be static: AddListener stores a RAW, unowned pointer (see CinderBtListener above).
static CinderBtVolListener g_bt_vol_listener;
static bool g_bt_vol_listener_on = false;

// Subscribe once. Cheap to call repeatedly — it no-ops after the first success.
static void bt_vol_listener_start() {
    enum { VIDX_AddListener = 39 };
    if (g_bt_vol_listener_on) return;
    try {
        void* x = bt_xmit();
        if (!x) return;
        typedef int (*fnadd)(void*, void*, const std::string*);
        std::string key;                    // "" — a NotifyListeners FILTER KEY, not a label
        int rc = ((fnadd)bt_slot(x, VIDX_AddListener))(x, (void*)&g_bt_vol_listener, &key);
        g_bt_vol_listener_on = (rc == 0);
        char m[96];
        std::snprintf(m, sizeof m, "bt-vol: volume listener AddListener rc=%d (%s)", rc,
                      rc == 0 ? "registered" : "FAILED — the UI level stays a belief");
        clog_(m);
    } catch (...) { clog_("bt-vol: AddListener threw"); }
}

// Apply whatever the sink last reported. Render-thread only. Returns true if the UI changed.
static bool bt_vol_drain_notify() {
    int v = g_bt_vol_notify;
    if (v < 0) return false;
    g_bt_vol_notify = -1;
    if (v > 127) v = 127;
    // 0..127 -> 0..CINDER_BT_VOL_MAX, rounded rather than truncated so the top of the sink's scale
    // reaches the top of ours instead of stopping one step short.
    int lvl = (v * CINDER_BT_VOL_MAX + 63) / 127;
    if (lvl == cinder_get_bt_volume()) return false;
    cinder_set_bt_volume(lvl);
    char m[96];
    std::snprintf(m, sizeof m, "bt-vol: sink reports %d/127 -> UI level %d", v, lvl);
    clog_(m);
    return true;
}

// Does the CONNECTED sink accept absolute volume right now? ASKED EVERY TIME, never cached.
//
// Both directions of caching have now been wrong on real hardware:
//   * Caching the first answer cached a NO. GetBtStatus hits 3 at the START of connection setup,
//     before the sink's AVRCP capabilities are readable, so the first ask reliably said "no" and
//     the session then used the step path forever.
//   * Caching only the YES was just as wrong. Measured 2026-08-11 on CMF Buds Pro 2 inside one
//     session: IsSupportedAbsoluteVolume read 1, then 0, on the same unbroken link (and
//     IsAvrcpTgVolumeSupported with it). With the YES sticky, Cinder kept calling SetCurrentVolume
//     into a service that refuses it — the call returns false and transmits nothing — and never
//     fell back. The UI level moved and the headphones did not, which is what was reported.
//
// So there is no cacheable answer here: support is a property of the AVRCP session, which
// renegotiates. It is one cheap IPC read per volume press, which is a rounding error next to the
// press itself, and `apply_bt_volume` treats even a YES as provisional.
static bool bt_abs_volume_supported() {
    enum { VIDX_IsSupportedAbsoluteVolume = 33 };
    int sup = 0;
    try {
        void* x = bt_xmit();
        if (!x) return false;
        typedef int (*fn0)(void*);
        sup = ((fn0)bt_slot(x, VIDX_IsSupportedAbsoluteVolume))(x) ? 1 : 0;
    } catch (...) { clog_("bt-vol: IsSupportedAbsoluteVolume threw"); return false; }
    if (sup != g_bt_abs_vol) {                   // log the transition, not every press
        g_bt_abs_vol = sup;
        clog_(sup ? "bt-vol: sink takes ABSOLUTE volume (SetCurrentVolume)"
                  : "bt-vol: sink is step-only (SetVolumeUp/Down)");
    }
    cinder_set_bt_enhanced_supported(sup == 1);
    return sup == 1;
}

// Absolute volume needs BOTH: a sink that supports it, and Sony's "Use Enhanced Mode" preference
// set — the service checks the preference itself before transmitting.
static bool bt_use_absolute_volume() {
    return cinder_get_bt_enhanced() != 0 && bt_abs_volume_supported();
}

// ── "Use Enhanced Mode" (Sony's name; message 230077, help text 230079) ─────────────────────────
// The AVRCP absolute-volume switch. Sony's own SetCurrentVolume refuses to transmit unless this
// preference is set — libBtTransmitterService.so logs "Not control absolute volume mode" and
// returns — so IsSupportedAbsoluteVolume() alone was never enough: Cinder's absolute path was a
// no-op whenever stock had last left the preference off.
//
// The radio does not carry the preference across a link, so this is re-applied on every connect.
static void bt_apply_enhanced_mode(const char* why) {
    enum { VIDX_SetControlAbsoluteVolume = 31, VIDX_GetControlAbsoluteVolume = 32 };
    try {
        void* x = bt_xmit();
        if (!x) { clog_("bt-enh: BtTransmitterServiceClient unavailable"); return; }
        const bool want = cinder_get_bt_enhanced() != 0;
        typedef void (*fnb)(void*, const bool*);
        typedef int  (*fn0)(void*);
        ((fnb)bt_slot(x, VIDX_SetControlAbsoluteVolume))(x, &want);
        // Judge by side effect, not by the setter's return: read it straight back.
        const bool got = ((fn0)bt_slot(x, VIDX_GetControlAbsoluteVolume))(x) != 0;
        char buf[128];
        std::snprintf(buf, sizeof buf, "bt-enh: enhanced mode %s (%s) -> readback %s",
                      want ? "ON" : "OFF", why, got ? "ON" : "OFF");
        clog_(buf);
    } catch (...) { clog_("bt-enh: SetControlAbsoluteVolume threw"); }
}

// Push the UI's Bluetooth level at the sink. `up` only matters on the step fallback — with absolute
// volume the UI has already moved its level and we just send where it landed.
static void apply_bt_volume(bool up) {
    enum { VIDX_SetVolumeDown = 16, VIDX_SetVolumeUp = 17, VIDX_SetCurrentVolume = 34 };
    try {
        void* x = bt_xmit();
        if (!x) { clog_("bt-vol: BtTransmitterServiceClient unavailable"); return; }
        // `sent` is decided by the SERVICE, not by the capability read above it. SetCurrentVolume
        // is `virtual bool` and returns false when it declines to transmit — which it does, on a
        // sink whose support flipped between the read and the call. Falling through to the step
        // call in the same press is what makes a flapping link still track the rocker.
        bool sent = false;
        if (bt_use_absolute_volume()) {
            // UI steps -> the AVRCP 0..127 scale. Integer maths, and the top step must land on 127
            // exactly or full volume would be unreachable.
            int lvl = cinder_get_bt_volume();
            if (lvl < 0) lvl = 0;
            if (lvl > CINDER_BT_VOL_MAX) lvl = CINDER_BT_VOL_MAX;
            unsigned char v = (unsigned char)(lvl * 127 / CINDER_BT_VOL_MAX);
            typedef int (*fnu)(void*, const unsigned char*);
            sent = ((fnu)bt_slot(x, VIDX_SetCurrentVolume))(x, &v) != 0;
            if (!sent) clog_("bt-vol: SetCurrentVolume REFUSED — falling back to a step");
        }
        if (!sent) {
            typedef int (*fn0)(void*);
            ((fn0)bt_slot(x, up ? VIDX_SetVolumeUp : VIDX_SetVolumeDown))(x);
        }
    } catch (...) {
        clog_("bt-vol: volume call threw");
    }
}

// ── Bluetooth codec ─────────────────────────────────────────────────────────────────────────
// The Settings codec row used to be write_bt_pref() only — it recorded the choice in
// /contents/cinder_bt.conf for the LDAC bridge to read, and never told the radio. So picking LDAC
// changed a file and nothing else, which is the same shape of defect as the BT switch that never
// called SetRfOnOff.
//
// The codec toggles are three independent bools rather than one selector, so the exclusive choice
// the UI presents has to be expressed as "enable the chosen one, disable the others". SBC is the
// A2DP mandatory baseline and has no toggle — it is what is left when all three are off.
//
// Signatures are read, not guessed: `virtual bool ...SetLdac(const bool &)`,
// `SetAptxClassic(const bool &)`, `SetAptxHD(const bool &)`,
// `SetLdacSoundQuality(const IBtTransmitterService::BtLdacSoundQuality &)` all appear verbatim in
// the library's strings, and the marshalling matches (base 3×Alloc(4) plus one Alloc sized 1 for
// each bool, 4 for the quality enum).
void apply_bt_codec() {
    enum { VIDX_SetLdacSoundQuality = 18, VIDX_SetLdac = 20,
           VIDX_SetAptxClassic = 21, VIDX_SetAptxHD = 22 };
    int ci = cinder_get_bt_codec();        if (ci < 0 || ci > 3) ci = 0;   // 0 ldac 1 aptxhd 2 aptx 3 sbc
    int qi = cinder_get_bt_ldac_quality(); if (qi < 0 || qi > 3) qi = 0;   // 0 auto 1 990 2 660 3 330
    try {
        void* x = bt_xmit();
        if (!x) { clog_("bt-codec: BtTransmitterServiceClient unavailable"); return; }
        typedef int (*fnb)(void*, const bool*);
        bool ldac = (ci == 0), aptxhd = (ci == 1), aptx = (ci == 2);
        ((fnb)bt_slot(x, VIDX_SetLdac))(x, &ldac);
        ((fnb)bt_slot(x, VIDX_SetAptxHD))(x, &aptxhd);
        ((fnb)bt_slot(x, VIDX_SetAptxClassic))(x, &aptx);
        if (ldac) {
            // The enum's numeric values are NOT recovered — the UI order (Auto/990/660/330) mirrors
            // Sony's own menu, so declaration order is the reasonable assumption, and it is only an
            // assumption. It is safe to be wrong: this is a by-value scalar, so a bad value picks
            // the wrong bitrate or gets rejected, it cannot corrupt memory. The service logs the
            // value it received as `ldac quality:%d`, so one look at logcat while cycling the row
            // settles it — see task #21.
            unsigned q = (unsigned)qi;
            typedef int (*fne)(void*, const unsigned*);
            ((fne)bt_slot(x, VIDX_SetLdacSoundQuality))(x, &q);
        }
        char m[128];
        std::snprintf(m, sizeof m, "bt-codec: ldac=%d aptxhd=%d aptx=%d quality=%d (0=sbc baseline)",
                      (int)ldac, (int)aptxhd, (int)aptx, qi);
        clog_(m);
        // What we asked for is a PREFERENCE; go and read what the link actually agreed on, without
        // waiting out the poll's throttle. This is the whole point of GetSoundStatus existing.
        g_bt_sound_poll_now = 1;
    } catch (...) {
        clog_("bt-codec: apply threw");
    }
}

// ── who is connected ────────────────────────────────────────────────────────────────────────
// `GetConnectInformation` takes TWO out-params, both by reference:
//
//     bool GetConnectInformation(pst::base::vector<uint8_t>& addr, pst::base::string& name)
//
// Recovered from the stub's prologue (arg1 lands in sl, arg2 in r8) plus what each is used for: r8
// goes to TransactionParam::GetStr, while sl is walked as {begin,end,cap} and grown a byte at a time
// by a Get(1) loop counted by a preceding Get(4) — a MAC being push_back'd. Two earlier attempts
// passed a SINGLE pointer and crashed at an IDENTICAL address both times; that identity was the
// clue, because a merely-wrong buffer moves and a missing argument does not.
//
// `pst::base::vector`/`string` are typedefs for the libc++ containers (the mangled forms exist
// nowhere in the vendor tree, and the marshaller's own PLT entry names std::__1::basic_string), and
// this file is compiled against the libc++ 3.9.0 headers matching the device runtime — so real
// containers go straight across. Verified on device: returns a 6-byte MAC and the device name.
// Did the last read actually yield a device? The link comes up in stages — the service logs
// `AVSRC status change to` 3, then 4, then 5 — and `GetBtStatus` reaches 3 at the FIRST of those,
// while `GetConnectInformation` only has an address to give near the last. So the read that fires on
// the route change can legitimately come back empty, and something has to ask again.
static bool g_bt_have_name = false;
// The CONNECTED peer's BD address, or empty. Needed because "is the thing I just tapped the thing
// I am already listening to?" cannot be answered from the name alone — two headphones can share a
// model name, and the name is what the sink chose to advertise, not an identity.
static std::vector<unsigned char> g_bt_connected_addr;

// Auto-reconnect state. Declared here, beside the link state it reasons about, because the
// Disconnect path below runs before the reconnect logic further down. See bt_reconnect_tick.
static bool g_bt_user_disconnected = false;
// When to try next, and how long to wait after that. 0 = nothing scheduled.
static long g_bt_reconnect_at = 0;
static int  g_bt_reconnect_wait_s = 0;

static void refresh_bt_connected() {
    enum { VIDX_GetConnectInformation = 5 };
    try {
        void* x = bt_xmit();
        if (!x) return;
        typedef int (*fn2)(void*, std::vector<unsigned char>*, std::string*);
        std::vector<unsigned char> addr;
        std::string name;
        ((fn2)bt_slot(x, VIDX_GetConnectInformation))(x, &addr, &name);
        // THE ADDRESS IS THE SIGNAL, NOT THE RETURN VALUE. Measured 2026-07-30 with a WH-1000XM4
        // connected and playing: `cinder-probe --btwho` reported
        //   GetBtStatus=3  AvSrc=5  Avrcp=2
        //   GetConnectInformation rc=0 addr=AC:80:0A:56:A9:91 name='WH-1000XM4'
        // — a filled address and name alongside a ZERO return. The client stub's int is a transaction
        // status (0 = OK), not the service method's `bool`. This code used to gate on
        // `rc && !addr.empty()`, so it threw away a perfectly good name on every real connection and
        // the Bluetooth screen stayed on "No device connected" while audio played.
        const std::vector<unsigned char> prev_addr = g_bt_connected_addr;
        cinder_set_bt_connected(!addr.empty() ? name.c_str() : "");
        g_bt_have_name = !addr.empty();
        g_bt_connected_addr = addr;

        // The negotiated codec is NOT read here. bt_poll_sound_status() already owns that call and
        // throttles it to 2 s with an event bypass; a second reader in this function meant two
        // synchronous IPC round trips for one fact, and two paths that could disagree about it.
        // A link coming up sets g_bt_sound_poll_now, so the throttle costs nothing here.
        // A peer we did not have last time is a NEW link, and the codec is negotiated during its
        // setup — so ask for a fresh read rather than waiting out the throttle.
        if (!addr.empty() && addr != prev_addr) g_bt_sound_poll_now = 1;
    } catch (...) {
        clog_("bt: GetConnectInformation threw");
    }
}

// Hang up on the current device, radio untouched. RequestDisconnection is slot 8 and takes no
// arguments (`virtual bool RequestDisconnection()` straight out of the library's own strings).
void apply_bt_disconnect() {
    enum { VIDX_RequestDisconnection = 8 };
    // The user said no. Park the auto-reconnect, or the link comes straight back and the button
    // looks broken.
    g_bt_user_disconnected = true;
    g_bt_reconnect_at = 0;
    g_bt_reconnect_wait_s = 0;
    try {
        void* x = bt_xmit();
        if (!x) { clog_("bt: no transmitter client — cannot disconnect"); return; }
        typedef int (*fn0)(void*);
        int rc = ((fn0)bt_slot(x, VIDX_RequestDisconnection))(x);
        char m[96];
        std::snprintf(m, sizeof m, "bt: RequestDisconnection() rc=%d (radio stays on)", rc);
        clog_(m);
    } catch (...) { clog_("bt: RequestDisconnection threw"); }
    refresh_bt_connected();
    refresh_bt_route();
}

// ── paired devices (the Devices screen) ──────────────────────────────────────────────────────
// `virtual bool GetPairedDeviceInfo(pst::base::vector<BtPairedDeviceInformation> &)` — slot 20 on
// BtCommonServiceClient. The element layout was recovered on device 2026-07-29 and then confirmed
// the hard way: the 10-char name arrived as a libc++ SSO string and the 14-char one as a heap
// string, and BOTH decoded through this one declaration, which is only possible if the container
// really is std::__1::string. `bytes/48` matched the typed count exactly for two real pairings.
struct BtPairedDeviceInformation {
    std::vector<unsigned char> addr;    // +0   BD address, 6 bytes, MSB first
    unsigned                   cod;     // +12  class of device
    std::vector<unsigned char> key;     // +16  link key (16 B)
    std::string                name;    // +28  friendly name
    unsigned char              f0, f1;  // +40  flags — 1,1 on both real pairings
    unsigned char              pad[6];  // +42 -> 48
};
static_assert(sizeof(BtPairedDeviceInformation) == 48, "paired-device stride is not 48");

// The BD addresses, in the SAME ORDER they were pushed into the UI. A row index is the only handle
// the UI ever holds, so this vector is the other half of that agreement: index in, address out.
static std::vector<std::vector<unsigned char>> g_bt_paired;

// ── AUTO-RECONNECT ──────────────────────────────────────────────────────────────────────────────
// Walk out of range and back and the link stayed down: nothing ever retried. The radio reconnects
// on a deliberate toggle (apply_bt_toggle) and when the sink itself initiates, but a link that
// DROPS — range, the headphones idling off, a competing host stealing them — left the player
// silently on the 3.5 mm jack until the user noticed and toggled Bluetooth off and on.
//
// THE FLAG IS THE WHOLE DESIGN. An auto-reconnect that ignores an explicit Disconnect is worse than
// none: the button would appear broken, because the link would come straight back. So a user
// disconnect (the Bluetooth screen's Disconnect, or a second NFC tap on the connected device) parks
// the retry until the user asks for a link again — a toggle, a Devices row, a tap, or a pairing.
// First wait after noticing a drop, and the ceiling. A failed attempt is not free — it occupies the
// radio and the MTK stack is the single most expensive thing on this device when it is busy (~13.5%
// of a core with a link up) — so this backs off hard rather than hammering. The cap is generous
// because the common case is the headphones being switched off deliberately, where the right
// behaviour is to cost nothing until they come back.
#define BT_RECONNECT_FIRST_S 10
#define BT_RECONNECT_MAX_S   300

// Called wherever the user asks for a link, so the retry stops being parked.
static void bt_reconnect_rearm() {
    g_bt_user_disconnected = false;
    g_bt_reconnect_at = 0;
    g_bt_reconnect_wait_s = 0;
}

// Runs from the 1 Hz housekeeping. Cheap: the common paths are two boolean tests.
static void bt_reconnect_tick() {
    if (!cinder_get_bt_on() || g_bt_user_disconnected) return;
    if (g_bt_have_name) {                     // connected — reset so the next drop starts fresh
        if (g_bt_reconnect_wait_s) {
            clog_("bt-reconnect: link is up again — retry disarmed");
            g_bt_reconnect_at = 0;
            g_bt_reconnect_wait_s = 0;
        }
        return;
    }
    // Nothing to reconnect TO is not a failure; it is a device that has never been paired.
    if (g_bt_paired.empty()) return;
    // The radio itself has to be up. If it is not, apply_bt_toggle owns that, not us.
    if (!g_bt_radio_seen_up) return;

    const long now = now_ms();
    if (g_bt_reconnect_at == 0) {             // first notice of the drop: schedule, don't act
        g_bt_reconnect_wait_s = BT_RECONNECT_FIRST_S;
        g_bt_reconnect_at = now + (long)g_bt_reconnect_wait_s * 1000;
        char m[128];
        std::snprintf(m, sizeof m, "bt-reconnect: link is down — first retry in %ds",
                      g_bt_reconnect_wait_s);
        clog_(m);
        return;
    }
    if (now < g_bt_reconnect_at) return;

    // Zero-arg: the service looks up the last device itself. Preferred over RequestConnection with
    // an address because it is what Sony's own reconnect does, and it needs no guess about WHICH of
    // several paired devices the user meant.
    enum { VIDX_RequestLastDeviceConnection = 7 };
    apply_bt_codec();     // A2DP negotiates during setup, so the preference must precede the link
    int rc = -1;
    try {
        void* x = bt_xmit();
        if (x) {
            typedef int (*fn0)(void*);
            rc = ((fn0)bt_slot(x, VIDX_RequestLastDeviceConnection))(x);
        }
    } catch (...) { clog_("bt-reconnect: RequestLastDeviceConnection threw"); }

    if (g_bt_reconnect_wait_s < BT_RECONNECT_MAX_S) {
        g_bt_reconnect_wait_s *= 2;
        if (g_bt_reconnect_wait_s > BT_RECONNECT_MAX_S) g_bt_reconnect_wait_s = BT_RECONNECT_MAX_S;
    }
    g_bt_reconnect_at = now + (long)g_bt_reconnect_wait_s * 1000;
    char m[160];
    std::snprintf(m, sizeof m, "bt-reconnect: RequestLastDeviceConnection rc=%d — next retry in %ds",
                  rc, g_bt_reconnect_wait_s);
    clog_(m);
}


static void* bt_common() {
    if (!g_bt_common) g_bt_common = _ZN3pst8services28BtCommonServiceClientFactory14CreateInstanceEv();
    return g_bt_common;
}

// A short, honest descriptor from the class-of-device word. Only the major class is trusted plus the
// handful of audio minor classes that are unambiguous — a wrong-but-specific label ("Speaker" on a
// pair of headphones) is worse than none, and an empty string simply draws nothing.
static const char* bt_kind_from_cod(unsigned cod) {
    unsigned major = (cod >> 8) & 0x1F;
    unsigned minor = (cod >> 2) & 0x3F;
    if (major == 4) {                       // Audio / Video
        switch (minor) {
            case 1: case 2: return "Headset";
            case 5:         return "Speaker";
            case 6:         return "Headphones";
            case 7:         return "Portable audio";
            case 8:         return "Car audio";
            case 0x0A:      return "Hi-Fi audio";
            default:        return "Audio device";
        }
    }
    if (major == 2) return "Phone";
    if (major == 1) return "Computer";
    return "";
}

// Read the radio's pairing table and push it into the UI. Run ONLY via run_guarded.
void refresh_bt_paired() {
    enum { VIDX_GetPairedDeviceInfo = 20, VIDX_GetConnectInformation = 5 };
    std::vector<BtPairedDeviceInformation> list;
    // Which one is live, so the row can say CONNECTED instead of offering to connect it again.
    std::vector<unsigned char> live;
    try {
        void* x = bt_xmit();
        if (x) {
            typedef int (*fn2)(void*, std::vector<unsigned char>*, std::string*);
            std::string nm;
            // Same rule as refresh_bt_connected: the filled address decides, not the return value,
            // which is 0 even on a live link. Clearing `live` on a zero return marked NOTHING as
            // connected in this list, every time.
            ((fn2)bt_slot(x, VIDX_GetConnectInformation))(x, &live, &nm);
        }
    } catch (...) { clog_("bt-paired: GetConnectInformation threw"); live.clear(); }

    try {
        void* c = bt_common();
        if (!c) { clog_("bt-paired: no BtCommonServiceClient — list stays empty"); return; }
        typedef int (*fnv)(void*, std::vector<BtPairedDeviceInformation>*);
        int rc = ((fnv)bt_slot(c, VIDX_GetPairedDeviceInfo))(c, &list);
        if (!rc) { clog_("bt-paired: GetPairedDeviceInfo returned false"); return; }
    } catch (...) {
        clog_("bt-paired: GetPairedDeviceInfo threw — list left as it was");
        return;
    }

    // Only replace what is on screen once the read succeeded: a failed poll must not blank the list
    // the user is looking at, because the FORGET rows are the only way out of a bad pairing.
    g_bt_paired.clear();
    cinder_bt_paired_clear();
    for (size_t i = 0; i < list.size(); i++) {
        const BtPairedDeviceInformation& d = list[i];
        bool connected = !live.empty() && d.addr == live;
        cinder_bt_paired_add(d.name.c_str(), bt_kind_from_cod(d.cod), connected ? 1 : 0);
        g_bt_paired.push_back(d.addr);
    }
    char m[96];
    std::snprintf(m, sizeof m, "bt-paired: %u device(s)%s", (unsigned)list.size(),
                  live.empty() ? "" : ", one connected");
    clog_(m);
}

// ── Bluetooth listener: scan results ─────────────────────────────────────────────────────────────
// Recovered + PROVEN on device 2026-07-30 (analysis/G_bt_nfc/RE_findings.md round b, and
// `cinder-probe --btscan`). Three facts this code depends on, each measured rather than assumed:
//
//   * We do NOT implement `IBinderObject`. `AddListener` (client vtable slot 30) allocates the binder
//     proxy itself and stores a RAW pointer to our object, so a plain C++ class is the whole listener.
//     Because the pointer is raw and unowned, the listener MUST have static storage duration — see
//     `g_bt_listener` below. A stack or heap listener would be a use-after-free on the next
//     notification.
//   * `AddListener(listener, name)` returns **0 on success** (1 = bad argument, 4 = no service). It
//     does not return an id, which is why unregistering takes the LISTENER POINTER as its `unsigned`
//     handle — verified with a negative control: an identical stimulus fired callbacks while
//     registered and was silent after `RemoveListener`.
//   * The `name` argument is a `NotifyListeners` FILTER KEY, not a label. `""` works; a wrong key
//     would give a listener that never fires while looking perfectly healthy.
//
// Everything here runs on the framework looper, NOT the render thread. So the callbacks only append to
// a mutex-guarded list and set a flag; the main loop is what pushes anything into the UI.
struct BtFound {
    std::vector<unsigned char> addr;
    unsigned                   cod;
    std::string                name;
};
static pthread_mutex_t g_bt_found_mx = PTHREAD_MUTEX_INITIALIZER;
static std::vector<BtFound> g_bt_found;                 // guarded by g_bt_found_mx
static volatile sig_atomic_t g_bt_found_dirty  = 0;
static volatile sig_atomic_t g_bt_pairing_done = 0;
// The radio says its connection state moved. Set from the listener, drained by the render loop,
// which then re-reads the route on the NEXT FRAME instead of waiting out the 3 s poll.
//
// This is why Sony connects visibly faster than Cinder did. The link itself is the MTK stack's and
// takes exactly as long either way — what differed is how long we took to NOTICE. Cinder learned
// about a connection only from `refresh_bt_route`'s `GetBtStatus` poll, which runs every 3 s (and
// every 15 s while the radio reads down), so on average ~1.5 s and at worst 3 s of doing nothing
// before the route flipped — and everything that hangs off the connect edge (the device name, the
// volume re-assert, the codec apply, the EQ chain) queues up behind that. Stock is event-driven.
//
// Polling faster is the wrong fix: each poll is a synchronous IPC round trip on the render thread.
// The listener costs nothing until something happens.
static volatile sig_atomic_t g_bt_state_dirty  = 0;
// The addresses actually PUSHED to the UI, in UI row order — see flush_bt_found for why this is not
// the same list as g_bt_found. Main-loop thread only, so it needs no lock.
static std::vector<std::vector<unsigned char>> g_bt_found_ui;
// A pairing we asked for, and the re-read schedule that waits for the radio to admit it happened.
// `OnNotifyPairingComplete` fires BEFORE the link key is visible to GetPairedDeviceInfo (measured
// 2026-07-30: the read right after the callback still returned the old count), so one immediate
// refresh is not enough — the device only appeared when the user tapped PAIR a second time.
static std::vector<unsigned char> g_bt_pairing_addr;
// A pairing prompt the radio is waiting on. Recovered by hand 2026-07-30 (round e): these three
// callbacks carry real arguments, so unlike the placeholder versions this code DOES dereference them —
// which is only safe because each signature was read off the handler that builds the stack objects.
//   slot 3  OnNotifyNumericComparison(const vector<uint8_t>&, const uint32_t&, const uint32_t&, const string&)
//   slot 5  OnNotifyPasskey(const vector<uint8_t>&, const uint32_t&, const string&)
//   slot 14 OnNotifySspRequest(const vector<uint8_t>&, const string&, const uint32_t&, const uint32_t&, const uint32_t&)
//
// SSPVARIANT IS SETTLED (2026-08-11): the service's own range check says so. In
// `FireOnNotifySspRequest` (libBtCommonService.so @0x86f4) the incoming variant is tested
// `if (v > 3) log "NotifySspRequest : SspVariant value out of range!!"` and otherwise passed through
// a 4-entry `tbb` jump table that maps 0->0, 1->1, 2->2, 3->3. So the enum has exactly four
// enumerators, 0..3, and Sony's own translation is the identity. Echoing back the value we were
// handed — which is what RequestSspReply does below — is therefore provably correct rather than a
// guess, and no name table is needed to be right about it.
enum { BT_PROMPT_NONE = 0, BT_PROMPT_NUMERIC = 1, BT_PROMPT_PASSKEY = 2, BT_PROMPT_SSP = 3 };
struct BtPrompt {
    int                        kind = BT_PROMPT_NONE;
    std::vector<unsigned char> addr;
    std::string                name;
    unsigned                   code = 0;          // the digits to show the user
    unsigned                   v1 = 0, v2 = 0;    // the other raw values, echoed back on reply
};
static pthread_mutex_t g_bt_prompt_mx = PTHREAD_MUTEX_INITIALIZER;
static BtPrompt g_bt_prompt;                        // guarded by g_bt_prompt_mx
static volatile sig_atomic_t g_bt_prompt_dirty = 0;
static long g_bt_paired_recheck_at   = 0;
static int  g_bt_paired_recheck_left = 0;

struct CinderBtListener {
    // Virtual destructor FIRST: with the Itanium ABI that puts D1/D0 in slots 0/1 and the methods
    // below at 2..17, which is the layout the library dispatches through.
    virtual ~CinderBtListener() {}
    // Stash a prompt for the main loop to show. Runs on the framework looper, so it touches nothing
    // but the guarded struct.
    static void push_prompt(int kind, const std::vector<unsigned char>& addr, const std::string& name,
                            unsigned code, unsigned v1, unsigned v2) {
        pthread_mutex_lock(&g_bt_prompt_mx);
        g_bt_prompt.kind = kind;
        g_bt_prompt.addr = addr;
        g_bt_prompt.name = name;
        g_bt_prompt.code = code;
        g_bt_prompt.v1   = v1;
        g_bt_prompt.v2   = v2;
        pthread_mutex_unlock(&g_bt_prompt_mx);
        g_bt_prompt_dirty = 1;
    }
    // The three callbacks that mean "the link changed". Each only raises a flag — the arguments are
    // deliberately left uninterpreted (the render loop re-reads GetBtStatus, which is the value
    // refresh_bt_route already trusts), so this needs no argument RE to be correct. Whatever these
    // carry, the fact that they FIRED is the whole signal.
    virtual void OnNotifyBtStatus(const void*, const void*, const void*) { g_bt_state_dirty = 1; }
    virtual void OnNotifyNumericComparison(const std::vector<unsigned char>& addr,
                                           const unsigned& a, const unsigned& b,
                                           const std::string& name) {
        // Which of the two words is the six digits the other device shows is still not settled, but
        // the field list now is. libBtCompIf.so keeps demangled prototypes for the layer below:
        //
        //   BtCommonComponentIfReceiver::NotifyNumericComparison(btmw_bdaddr_t*, unsigned int,
        //                                unsigned char, unsigned char*, unsigned int)
        //   BtCommonComponentIfReceiver::NotifySspRequest(btmw_bdaddr_t*, unsigned char,
        //                                unsigned char*, unsigned int, btmw_ssp_variant_t,
        //                                unsigned int)
        //
        // The `unsigned char*` is the device name and the `unsigned char` its length, which leaves
        // exactly two uint32s here: a class-of-device word and the comparison value. That is what
        // the test below actually discriminates — a CoD is a packed 24-bit field (0x240404 for
        // headphones, i.e. 2360324) while a numeric comparison value is six decimal digits, so
        // "under a million" separates them. Both are still logged, and getting it wrong shows the
        // wrong number without being able to corrupt the pairing, because the reply is a yes/no.
        char m[128];
        std::snprintf(m, sizeof m, "bt-scan: NumericComparison '%s' a=%u b=%u", name.c_str(), a, b);
        clog_(m);
        unsigned shown = (a && a < 1000000u) ? a : b;
        push_prompt(BT_PROMPT_NUMERIC, addr, name, shown, a, b);
    }
    virtual void OnNotifyPairingComplete(const void*, const void*, const void*) {
        g_bt_pairing_done = 1;
    }
    virtual void OnNotifyPasskey(const std::vector<unsigned char>& addr,
                                 const unsigned& passkey, const std::string& name) {
        char m[112];
        std::snprintf(m, sizeof m, "bt-scan: Passkey '%s' %06u", name.c_str(), passkey);
        clog_(m);
        // Display only: this is the code the REMOTE device asks its user to enter, so there is nothing
        // for Cinder to reply. The panel is dismissable and pairing completes on its own.
        push_prompt(BT_PROMPT_PASSKEY, addr, name, passkey, 0, 0);
    }
    virtual void OnNotifySearchedDevice(const std::vector<unsigned char>& addr,
                                        const unsigned& cod,
                                        const std::string& name) {
        if (addr.size() != 6) return;                   // not an address we can do anything with
        pthread_mutex_lock(&g_bt_found_mx);
        bool dup = false, renamed = false;
        for (size_t i = 0; i < g_bt_found.size(); i++) {
            if (g_bt_found[i].addr == addr) {
                // A device is reported repeatedly during a scan, and the name often arrives empty the
                // first time and filled later. Keep the better one rather than adding a second row —
                // and mark the list dirty for that too, or the row stays "(unnamed)" for the whole
                // scan even though the radio told us the name a moment later.
                if (g_bt_found[i].name.empty() && !name.empty()) {
                    g_bt_found[i].name = name;
                    renamed = true;
                }
                dup = true;
                break;
            }
        }
        if (!dup) {
            BtFound f;
            f.addr = addr;
            f.cod  = cod;
            f.name = name;
            g_bt_found.push_back(f);
        }
        pthread_mutex_unlock(&g_bt_found_mx);
        if (!dup || renamed) g_bt_found_dirty = 1;
    }
    virtual void OnNotifyDisconnectEnd(const void*, const void*, const void*) { g_bt_state_dirty = 1; }
    virtual void OnNotifyCoexistenceBtWifiRatio(const void*, const void*, const void*) {}
    virtual void OnNotifyUpdateSupportProfile(const void*, const void*, const void*) {}
    virtual void OnNotifyUpdateOSInfo(const void*, const void*, const void*) {}
    virtual void OnNotifyRssi(const void*, const void*, const void*) {}
    virtual void OnNotifyStartSwitchDevice(const void*, const void*, const void*) {}
    virtual void OnNotifyAclStateChanged(const void*, const void*, const void*) { g_bt_state_dirty = 1; }
    virtual void OnNotifySspRequest(const std::vector<unsigned char>& addr, const std::string& name,
                                   const unsigned& x, const unsigned& y, const unsigned& z) {
        char m[144];
        std::snprintf(m, sizeof m, "bt-scan: SspRequest '%s' x=%u y=%u z=%u", name.c_str(), x, y, z);
        clog_(m);
        // `RequestSspReply(addr, SspVariant, bool accept, uint32)` wants the variant and value back, so
        // keep the raw words rather than interpreting them — echoing what arrived is the one choice
        // that cannot be wrong about an enum we have not decoded.
        push_prompt(BT_PROMPT_SSP, addr, name, y, x, z);
    }
    virtual void OnNotifyServiceUuids(const void*, const void*, const void*) {}
    virtual void OnNotifyServiceResume(const void*, const void*, const void*) {}
    virtual void OnNotifyError(const void*, const void*, const void*) {
        clog_("bt-scan: OnNotifyError from BtCommonService");
    }
};

// STATIC on purpose — the binder proxy keeps a raw pointer to this object for as long as the
// registration lives. Never make this a local or a `new` that anything can free.
static CinderBtListener g_bt_listener;
static bool g_bt_listener_on = false;

// Register once and stay registered for the life of the process. Churning registration per screen
// would buy nothing and add a window where a notification races the removal.
static bool bt_listener_register() {
    enum { VIDX_AddListener = 30 };
    if (g_bt_listener_on) return true;
    void* c = bt_common();
    if (!c) return false;
    try {
        std::string key("");     // the filter key — "" is the one measured to work
        typedef int (*fnadd)(void*, void*, const std::string*);
        int rc = ((fnadd)bt_slot(c, VIDX_AddListener))(c, (void*)&g_bt_listener, &key);
        g_bt_listener_on = (rc == 0);
        char m[112];
        std::snprintf(m, sizeof m, "bt-scan: AddListener rc=%d (%s)", rc,
                      g_bt_listener_on ? "registered" : "FAILED — 1 = bad arg, 4 = no service");
        clog_(m);
    } catch (...) { clog_("bt-scan: AddListener threw"); }
    return g_bt_listener_on;
}

// Start/stop discovery. Run ONLY via run_guarded.
void apply_bt_scan() {
    enum { VIDX_SetSearchMode = 14 };
    bool want = cinder_get_bt_scanning() != 0;
    if (want && !bt_listener_register()) {
        // Without the listener a scan is a lie: the radio would search and nothing would ever arrive.
        clog_("bt-scan: no listener, so refusing to pretend — scan switched back off");
        cinder_set_bt_scanning(0);
        return;
    }
    if (want) {
        pthread_mutex_lock(&g_bt_found_mx);
        g_bt_found.clear();
        pthread_mutex_unlock(&g_bt_found_mx);
        cinder_bt_found_clear();
    }
    try {
        void* c = bt_common();
        if (!c) return;
        // `SetSearchMode(const bool&, const uint16_t&)`. The second argument is a duration; 30 is a
        // guess at seconds that behaves sensibly, and the radio stopping on its own is harmless
        // because the UI's scan state is reconciled from the found-list poll, not from this call.
        bool on = want;
        unsigned short dur = 30;
        typedef int (*fnsearch)(void*, const bool*, const unsigned short*);
        int rc = ((fnsearch)bt_slot(c, VIDX_SetSearchMode))(c, &on, &dur);
        char m[96];
        std::snprintf(m, sizeof m, "bt-scan: SetSearchMode(%s, %u) rc=%d", want ? "on" : "off", dur, rc);
        clog_(m);
    } catch (...) { clog_("bt-scan: SetSearchMode threw"); }
}

// Push whatever the listener has collected into the UI. Called from the main loop — never from a
// callback, which runs on the looper thread.
//
// Devices that are ALREADY PAIRED are dropped here rather than shown twice. A scan reports them like
// anything else, so without this the just-paired device sat in the FOUND section still offering "TAP
// TO PAIR" while also appearing under PAIRED — which is exactly what made a working pairing look
// broken on 2026-07-30, because the obvious response is to tap PAIR again.
//
// The filtering is why `g_bt_found_ui` exists: the UI's row index must address the FILTERED list, or
// row 2 on screen would pair with whatever the unfiltered list happens to hold at index 2.
static void flush_bt_found() {
    g_bt_found_dirty = 0;
    pthread_mutex_lock(&g_bt_found_mx);
    std::vector<BtFound> snap = g_bt_found;
    pthread_mutex_unlock(&g_bt_found_mx);
    g_bt_found_ui.clear();
    cinder_bt_found_clear();
    unsigned hidden = 0;
    for (size_t i = 0; i < snap.size(); i++) {
        bool paired = false;
        for (size_t j = 0; j < g_bt_paired.size(); j++)
            if (g_bt_paired[j] == snap[i].addr) { paired = true; break; }
        if (paired) { hidden++; continue; }
        cinder_bt_found_add(snap[i].name.c_str(), bt_kind_from_cod(snap[i].cod));
        g_bt_found_ui.push_back(snap[i].addr);
    }
    char m[112];
    std::snprintf(m, sizeof m, "bt-scan: %u device(s) found%s", (unsigned)g_bt_found_ui.size(),
                  hidden ? " (already-paired ones hidden)" : "");
    clog_(m);
}

// Answer a pairing prompt. `accept` false = decline, which is also what a Cancel tap sends.
//
// Both replies pass the SAME address the notification carried, not anything the UI chose — the UI only
// ever says yes or no. Signatures come from the library's own strings:
//   bool SetNumericComparison(const pst::base::vector<uint8_t>&, const bool&)                   slot 9
//   bool RequestSspReply(const pst::base::vector<uint8_t>&, const SspVariant&, const bool&,
//                        const uint32_t&)                                                      slot 28
void apply_bt_prompt_reply(bool accept) {
    enum { VIDX_SetNumericComparison = 9, VIDX_CancelPairing = 8, VIDX_RequestSspReply = 28 };
    BtPrompt p;
    pthread_mutex_lock(&g_bt_prompt_mx);
    p = g_bt_prompt;
    g_bt_prompt = BtPrompt();
    pthread_mutex_unlock(&g_bt_prompt_mx);
    cinder_bt_prompt_clear();
    if (p.kind == BT_PROMPT_NONE || p.addr.size() != 6) return;
    try {
        void* c = bt_common();
        if (!c) return;
        char m[128];
        if (p.kind == BT_PROMPT_NUMERIC) {
            typedef int (*fnn)(void*, const std::vector<unsigned char>*, const bool*);
            int rc = ((fnn)bt_slot(c, VIDX_SetNumericComparison))(c, &p.addr, &accept);
            std::snprintf(m, sizeof m, "bt-scan: SetNumericComparison(%s) rc=%d", accept ? "yes" : "no", rc);
            clog_(m);
        } else if (p.kind == BT_PROMPT_SSP) {
            // v1 = the variant word as received, code = the value word. Echoed back rather than
            // interpreted, because `SspVariant`'s enumerators are not decoded.
            unsigned variant = p.v1, value = p.code;
            typedef int (*fns)(void*, const std::vector<unsigned char>*, const unsigned*, const bool*,
                              const unsigned*);
            int rc = ((fns)bt_slot(c, VIDX_RequestSspReply))(c, &p.addr, &variant, &accept, &value);
            std::snprintf(m, sizeof m, "bt-scan: RequestSspReply(variant=%u, %s, value=%u) rc=%d",
                          variant, accept ? "yes" : "no", value, rc);
            clog_(m);
        } else if (!accept) {
            // A passkey panel has nothing to confirm, so Cancel is the only meaningful answer and it
            // means "stop trying".
            typedef int (*fn0)(void*);
            int rc = ((fn0)bt_slot(c, VIDX_CancelPairing))(c);
            std::snprintf(m, sizeof m, "bt-scan: CancelPairing() rc=%d", rc);
            clog_(m);
        }
    } catch (...) { clog_("bt-scan: prompt reply threw"); }
}

// Push a pending prompt into the UI. Main loop only.
static void flush_bt_prompt() {
    g_bt_prompt_dirty = 0;
    pthread_mutex_lock(&g_bt_prompt_mx);
    BtPrompt p = g_bt_prompt;
    pthread_mutex_unlock(&g_bt_prompt_mx);
    if (p.kind == BT_PROMPT_NONE) { cinder_bt_prompt_clear(); return; }
    cinder_bt_prompt_set(p.kind, p.name.c_str(), p.code);
}

// Pair with a device the scan turned up (Devices ▸ a SCAN row).
void apply_bt_pair_device() {
    enum { VIDX_Pairing = 7 };
    int i = cinder_pending_bt_device();
    // Index into the list the UI was SHOWN (g_bt_found_ui), not the raw scan list — they differ
    // whenever an already-paired device was filtered out.
    std::vector<unsigned char> addr;
    if (i >= 0 && (size_t)i < g_bt_found_ui.size()) addr = g_bt_found_ui[(size_t)i];
    if (addr.size() != 6) { clog_("bt-scan: pair for an unknown row"); return; }
    g_bt_pairing_addr = addr;
    try {
        void* c = bt_common();
        if (!c) return;
        typedef int (*fna)(void*, const std::vector<unsigned char>*);
        int rc = ((fna)bt_slot(c, VIDX_Pairing))(c, &addr);
        char m[96];
        std::snprintf(m, sizeof m, "bt-scan: Pairing(row %d) rc=%d", i, rc);
        clog_(m);
    } catch (...) { clog_("bt-scan: Pairing threw"); }
}

// ── the codec that was actually NEGOTIATED ──────────────────────────────────────────────────────
//
// Everything the UI shows today is the user's PREFERENCE — SetLdac / SetAptxHD / SetAptxClassic,
// applied before RequestConnection. But A2DP negotiates, so a sink that cannot do LDAC lands on SBC
// while the screen still says LDAC. `BtTransmitterServiceClient` slot 26 is the only call that
// knows:
//
//     virtual void GetSoundStatus(BtSoundCodec&, BtSoundFrequency&, BtSoundChannel&, bool&)
//
// Four OUT params, all scalars, which is what makes it safe to call blind — unlike GetCapabilities
// (takes a pst::base::vector) or GetConnectInformation (holds a std::string), the two that crashed
// before the container layout was recovered. Exercised with the radio off via `--btwho`: all four
// were written, so the ABI is right.
//
// THE ENUMERATORS ARE NOT MAPPED, and this does not guess them — the rule BtLdacSoundQuality earned.
// With nothing connected every field reads 0, so 0 means "none/unset", not SBC. What this does is
// publish the raw word and log all four whenever they change, in the service's own format: one
// session with a known headphone connected settles the map for free. Until then the UI shows a
// neutral "BLUETOOTH" rather than a codec name it cannot stand behind.
static void bt_poll_sound_status() {
    enum { VIDX_GetSoundStatus = 26 };
    static unsigned last = 0xffffffffu;
    // THROTTLED, and it has to be. This is a synchronous IPC round trip on the RENDER thread, and the
    // call site gates it only on the route being Bluetooth — so before this it ran on EVERY FRAME,
    // ~60 a second, for as long as headphones were connected. The comment there claimed it was
    // "self-limiting"; that was wrong. The `key == last` test below suppresses the LOG, not the call,
    // and the call is the entire cost. This is exactly the shape of the poll storm that caused the
    // audio stutter in task #26.
    //
    // A2DP negotiates once per link, so the answer changes on the order of once a session. 2 s is
    // still far more often than it can move, and the two events that CAN move it (a link coming up,
    // a codec preference being written) set g_bt_sound_poll_now and skip the wait entirely — so the
    // throttle costs nothing in responsiveness.
    static long last_ms = 0;
    const long now = now_ms();
    if (!g_bt_sound_poll_now && now - last_ms < 2000) return;
    g_bt_sound_poll_now = 0;
    last_ms = now;
    void* x = bt_xmit();
    if (!x) return;
    unsigned codec = 0, freq = 0, chan = 0;
    bool ok = false;
    try {
        typedef void (*fn4)(void*, unsigned*, unsigned*, unsigned*, bool*);
        ((fn4)bt_slot(x, VIDX_GetSoundStatus))(x, &codec, &freq, &chan, &ok);
    } catch (...) { return; }
    const unsigned key = (codec & 0xff) | ((freq & 0xff) << 8) | ((chan & 0xff) << 16)
                       | ((unsigned)ok << 24);
    if (key == last) return;
    last = key;
    char m[192];
    std::snprintf(m, sizeof m, "bt-sound: codec:0x%02x channel:0x%02x frequency:0x%02x flag:%d "
                  "(enumerators still unmapped — this line is the map)", codec, chan, freq, ok);
    clog_(m);
    cinder_set_bt_negotiated_codec((int)codec);
}

// ── NFC tap-to-pair ─────────────────────────────────────────────────────────────────────────────
//
// PROVEN ON DEVICE 2026-08-11, with a WH-1000XM4 held against the rear panel. Until that tap the
// Home app deliberately had no NFC dependency at all (the libNfcService rule: no DT_NEEDED for a
// path whose payload read has never executed). It has now executed, so this wires it — still by
// dlopen, because the rule's second half stands: a library this optional must never be able to stop
// the launcher.
//
// WHAT THE PREVIOUS ROUNDS GOT WRONG. `Start(0)` was called for a fortnight and read as working
// because it "returned 1" and the controller appeared in logcat. `NfcService::Start`
// (libNfcService.so @0x7a40) says otherwise:
//
//     if (state == 3) return 3;                  // already started
//     ret = 1;                                   // the DEFAULT is failure
//     if (mode==1) nf=1; else if (mode==2) nf=2; else if (mode==3) nf=0; else goto out;
//     NF_start2(.., nf); state = 2; ret = 0;
//
// Valid modes are 1, 2, 3 and nothing else; 0 falls straight to the failure exit. The controller
// powering up was `Open`'s NF_initialize, not Start. Mode 1 is the one that reads tags: measured
// rc=0 with GetCurrentMode moving 0 -> 1, and a tag callback within seconds.
//
// The payload, recovered from that tap rather than guessed:
//
//     +0x00  std::vector<uint8_t> addr    AC:80:0A:56:A9:91
//     +0x0c  uint32               cod     0x240404 (class-of-device: headphones)
//     +0x10  std::vector<uint8_t>         16 bytes — the OOB block
//     +0x1c  std::string          name    "WH-1000XM4"
//
// Only `addr` and `name` are read here. The 16-byte block at +0x10 is almost certainly the OOB
// pairing material, but nothing needs it: `Pairing(addr)` is the same call the Devices screen's
// FOUND rows already use, and it is proven. Reading a field we have no use for would be risk
// without benefit.
static pthread_mutex_t g_nfc_mx = PTHREAD_MUTEX_INITIALIZER;
static std::vector<unsigned char> g_nfc_addr;      // guarded by g_nfc_mx
static std::string                g_nfc_name;      // guarded by g_nfc_mx
static volatile sig_atomic_t      g_nfc_tapped = 0;

struct CinderNfcListener {
    virtual ~CinderNfcListener() {}                                  // slots 0, 1
    virtual void OnBluetoothOob(const void* oob) {                   // slot 2
        if (!oob) return;
        const unsigned char* b = (const unsigned char*)oob;
        const std::vector<unsigned char>* addr =
            reinterpret_cast<const std::vector<unsigned char>*>(oob);
        const std::string* name = reinterpret_cast<const std::string*>(b + 0x1c);
        // Runs on the framework looper: copy under the lock and get out. Everything that talks to a
        // pst client happens on the render thread, same rule as the BT listener.
        pthread_mutex_lock(&g_nfc_mx);
        try {
            g_nfc_addr = *addr;
            g_nfc_name = *name;
        } catch (...) { g_nfc_addr.clear(); g_nfc_name.clear(); }
        pthread_mutex_unlock(&g_nfc_mx);
        g_nfc_tapped = 1;
    }
    virtual void OnUnknownTag(const void*) {                         // slot 3
        clog_("nfc: a tag was read but it is not a Bluetooth OOB record — ignoring");
    }
    virtual void OnHostCardEmulation(const void*) {}                 // slot 4
};
static CinderNfcListener g_nfc_listener;   // static: the proxy keeps a RAW pointer
static void* g_nfc_client = nullptr;
static bool  g_nfc_running = false;
static int   g_nfc_arm_tries = 0;   // bounds the retry; see the render loop

enum { VIDX_NfcOpen = 4, VIDX_NfcStart = 5, VIDX_NfcStop = 7, VIDX_NfcClose = 8,
       VIDX_NfcGetCurrentMode = 9, VIDX_NfcAddListener = 10, VIDX_NfcRemoveListener = 11 };

// Bring the reader up. Safe to call repeatedly; returns false and stays quiet on a device where the
// library is missing, which is the whole reason it is dlopen'd.
static bool nfc_start() {
    if (g_nfc_running) return true;
    static bool tried = false;
    if (!g_nfc_client && !tried) {
        tried = true;
        void* h = dlopen("libNfcService.so", RTLD_NOW);
        if (!h) { clog_("nfc: libNfcService.so unavailable — tap-to-pair off"); return false; }
        typedef void* (*mk)(void);
        mk f = (mk)dlsym(h, "_ZN3pst8services23NfcServiceClientFactory14CreateInstanceEv");
        if (!f) { clog_("nfc: NfcServiceClientFactory symbol missing"); return false; }
        try { g_nfc_client = f(); } catch (...) { g_nfc_client = nullptr; }
    }
    if (!g_nfc_client) return false;
    void** vt = *(void***)g_nfc_client;
    typedef int (*fn0)(void*);
    typedef int (*fnu)(void*, const unsigned*);
    typedef int (*fnadd)(void*, void*, const std::string*);
    try {
        std::string key("");
        ((fnadd)vt[VIDX_NfcAddListener])(g_nfc_client, (void*)&g_nfc_listener, &key);
        ((fn0)vt[VIDX_NfcOpen])(g_nfc_client);
        unsigned mode = 1;                      // the tag-reading mode; see the note above
        int rc = ((fnu)vt[VIDX_NfcStart])(g_nfc_client, &mode);
        int md = ((fn0)vt[VIDX_NfcGetCurrentMode])(g_nfc_client);
        char m[128];
        std::snprintf(m, sizeof m, "nfc: Start(1) rc=%d mode=%d%s", rc, md,
                      (rc == 0 || rc == 3) ? " — tap-to-pair armed" : " — NOT armed");
        clog_(m);
        g_nfc_running = (rc == 0 || rc == 3);
    } catch (...) { clog_("nfc: bring-up threw"); g_nfc_running = false; }
    return g_nfc_running;
}

static void nfc_stop() {
    if (!g_nfc_client || !g_nfc_running) return;
    void** vt = *(void***)g_nfc_client;
    typedef int (*fn0)(void*);
    typedef int (*fnrem)(void*, unsigned);
    try {
        ((fn0)vt[VIDX_NfcStop])(g_nfc_client);
        ((fn0)vt[VIDX_NfcClose])(g_nfc_client);
        ((fnrem)vt[VIDX_NfcRemoveListener])(g_nfc_client, (unsigned)(uintptr_t)&g_nfc_listener);
    } catch (...) {}
    g_nfc_running = false;
    clog_("nfc: reader stopped");
}

// The address a tap asked us to connect once the pairing table catches up. Empty = nothing pending.
// A pairing does NOT bring up A2DP by itself, and OnNotifyPairingComplete fires before the link key
// is even visible (see analysis/G_bt_nfc/RE_findings.md, round 2026-07-30d) — so the connect has to
// wait for the paired-list recheck rather than follow the Pairing call.
static std::vector<unsigned char> g_nfc_connect_after_pair;

// Ask the radio to bring up a link to `addr`. Shared by the Devices rows and NFC, so there is one
// place that knows the codec has to be pushed BEFORE the connect — A2DP negotiates during
// connection setup, so a codec set afterwards applies to the next link, not this one.
static int bt_request_connection(const std::vector<unsigned char>& addr, const char* who) {
    enum { VIDX_RequestConnection = 6 };
    int rc = -1;
    try {
        void* x = bt_xmit();
        if (!x) { clog_("bt: no transmitter client — cannot connect"); return -1; }
        apply_bt_codec();
        typedef int (*fna)(void*, const std::vector<unsigned char>*);
        rc = ((fna)bt_slot(x, VIDX_RequestConnection))(x, &addr);
        char m[128];
        std::snprintf(m, sizeof m, "%s: RequestConnection rc=%d", who, rc);
        clog_(m);
    } catch (...) { clog_("bt: RequestConnection threw"); }
    return rc;
}

// Called from the render loop when a tap landed.
//
// A TAP IS NOT ALWAYS A PAIRING. This used to call `Pairing` unconditionally, which is right only
// for a device the player has never seen. Tapping headphones that are ALREADY BONDED re-runs the
// bond and does not bring up A2DP — so the feature looked like it worked while the audio link was
// actually the headphones' own auto-reconnect on power-up, arriving at the same moment by
// coincidence. Reported 2026-08-17: "the nfc turns the headphones on but i think they connect
// because they previously were, not because of the tap." Correct, and the log agreed:
//
//   nfc: tapped AC:80:0A:56:A9:91 'WH-1000XM4' — pairing
//   nfc: Pairing rc=1                      <-- rc=1 IS success; the call was simply the wrong verb
//
// So the tap now dispatches on what the device actually is:
//
//   connected already  -> disconnect. Sony's own NFC behaviour on this hardware ("touch the device
//                         again to disconnect"), and the only way to make one gesture a toggle.
//   bonded, not linked -> RequestConnection. This is the common case and the one that was broken.
//   unknown            -> Pairing, then connect once the link key shows up (see the recheck loop).
static void nfc_service_tap() {
    enum { VIDX_Pairing = 7, VIDX_RequestDisconnection = 8 };
    std::vector<unsigned char> addr;
    std::string name;
    pthread_mutex_lock(&g_nfc_mx);
    addr.swap(g_nfc_addr);
    name.swap(g_nfc_name);
    pthread_mutex_unlock(&g_nfc_mx);
    if (addr.size() != 6) { clog_("nfc: tap with no usable address"); return; }
    char m[192];

    // Fresh reads: the tap may be the first thing that has ever touched Bluetooth this session, in
    // which case both the paired list and the connected peer are empty for want of asking, not
    // because they are really empty. Getting this wrong turns a connect into a redundant re-pair.
    refresh_bt_paired();
    refresh_bt_connected();

    bool bonded = false;
    for (size_t i = 0; i < g_bt_paired.size(); i++)
        if (g_bt_paired[i] == addr) { bonded = true; break; }
    const bool linked = (addr == g_bt_connected_addr);

    std::snprintf(m, sizeof m, "nfc: tapped %02X:%02X:%02X:%02X:%02X:%02X '%s' — %s",
                  addr[0], addr[1], addr[2], addr[3], addr[4], addr[5], name.c_str(),
                  linked ? "already linked, disconnecting" : bonded ? "bonded, connecting"
                                                                    : "unknown, pairing");
    clog_(m);

    if (linked) {
        // Same reasoning as the Disconnect button: a deliberate hang-up must not be undone by the
        // auto-reconnect a few seconds later.
        g_bt_user_disconnected = true;
        g_bt_reconnect_at = 0;
        g_bt_reconnect_wait_s = 0;
        try {
            void* x = bt_xmit();
            if (!x) { clog_("nfc: no transmitter client — cannot disconnect"); return; }
            typedef int (*fn0)(void*);
            int rc = ((fn0)bt_slot(x, VIDX_RequestDisconnection))(x);
            std::snprintf(m, sizeof m, "nfc: RequestDisconnection rc=%d (radio stays on)", rc);
            clog_(m);
        } catch (...) { clog_("nfc: RequestDisconnection threw"); }
        refresh_bt_connected();
        refresh_bt_route();
        return;
    }

    if (bonded) {
        bt_reconnect_rearm();
        bt_request_connection(addr, "nfc");
        // The link comes up asynchronously; the route poll notices it and refreshes the name.
        return;
    }

    // Genuinely new. Pair, and remember to connect once the link key appears — pairing alone leaves
    // you bonded and silent, which is not what someone who just tapped their headphones wanted.
    bt_reconnect_rearm();
    g_bt_pairing_addr = addr;
    g_nfc_connect_after_pair = addr;
    try {
        void* c = bt_common();
        if (!c) return;
        typedef int (*fna)(void*, const std::vector<unsigned char>*);
        int rc = ((fna)bt_slot(c, VIDX_Pairing))(c, &addr);
        std::snprintf(m, sizeof m, "nfc: Pairing rc=%d (1 = accepted)", rc);
        clog_(m);
    } catch (...) { clog_("nfc: Pairing threw"); }
    // Same recheck the scan flow uses: the callback runs ahead of the pairing table, so poll for
    // the side effect rather than trusting the return.
    g_bt_paired_recheck_left = 8;
    g_bt_paired_recheck_at   = now_ms() + 700;
}

// Connect one specific paired device (Devices ▸ row). `RequestConnection` takes the BD address by
// const reference — unlike RequestLastDeviceConnection, which takes nothing and picks for itself.
void apply_bt_connect_device() {
    int i = cinder_pending_bt_device();
    if (i < 0 || (size_t)i >= g_bt_paired.size()) { clog_("bt-paired: connect for an unknown row"); return; }
    const std::vector<unsigned char> addr = g_bt_paired[(size_t)i];
    bt_reconnect_rearm();          // an explicit connect re-arms the retry
    char who[48];
    std::snprintf(who, sizeof who, "bt-paired: row %d", i);
    bt_request_connection(addr, who);
    // The connection completes asynchronously; the 3 s route poll notices it and refreshes the name.
    refresh_bt_paired();
}

// Drop a device's link key. Nothing here is recoverable from Cinder — re-pairing needs a scan, which
// waits on the BtCommonService listener — so the UI confirms with a second tap before we get here.
void apply_bt_forget_device() {
    enum { VIDX_DeleteLinkkey = 15 };
    int i = cinder_pending_bt_device();
    if (i < 0 || (size_t)i >= g_bt_paired.size()) { clog_("bt-paired: forget for an unknown row"); return; }
    const std::vector<unsigned char> addr = g_bt_paired[(size_t)i];
    try {
        void* c = bt_common();
        if (!c) { clog_("bt-paired: no common client — cannot forget"); return; }
        typedef int (*fna)(void*, const std::vector<unsigned char>*);
        int rc = ((fna)bt_slot(c, VIDX_DeleteLinkkey))(c, &addr);
        char m[96];
        std::snprintf(m, sizeof m, "bt-paired: DeleteLinkkey(row %d) rc=%d", i, rc);
        clog_(m);
    } catch (...) { clog_("bt-paired: DeleteLinkkey threw"); }
    refresh_bt_paired();
    refresh_bt_connected();
}

// Which output owns the rocker. GetBtStatus == 3 is the measured "connected" value (see
// reference_bt_radio_wedge: 7 = off, 2 = on/idle, 3 = connected), and a connected transmitter is
// where the audio is going, so that is the whole test. Pushed into the UI, which uses it to pick
// WHICH of its two stored levels the next press moves — it never moves either one by itself.
// Last radio state this poll observed (7 = off). Read by the render loop to decide how often to
// come back — see the back-off there. Starts `true` so a first poll always happens promptly.
bool g_bt_radio_seen_up = true;

void refresh_bt_route() {
    int st = bt_status();
    g_bt_radio_seen_up = bt_radio_up(st);
    int on = (st == 3) ? 1 : 0;
    if (on == cinder_get_bt_route()) {
        // No route change — but if we are on Bluetooth and still have no device NAME, ask again. The
        // first read happens the instant GetBtStatus hits 3, which is the START of connection setup
        // (`AVSRC status change to (3)`), and the address only becomes readable a couple of stages
        // later. Without this retry a single early empty read left the Bluetooth screen saying "No
        // device connected" for the whole session while audio played into the headphones.
        if (on && !g_bt_have_name) refresh_bt_connected();
        return;                                   // otherwise: don't log every poll
    }
    cinder_set_bt_route(on);
    // A new link renegotiates the codec, so the previous answer is stale the moment this fires. Read
    // it on the next frame rather than up to 2 s later — a connect is exactly when the USB-DAC panel
    // is being looked at.
    g_bt_sound_poll_now = 1;
    // Absolute-volume support is a property of the SINK, so a new connection has to re-ask. Clearing
    // it here rather than caching once is what makes swapping between two different pairs of
    // headphones pick the right mechanism for each.
    g_bt_abs_vol = -1;   // re-log the sink's answer on the new link
    // The link changed, so the name on the CONNECTED card is stale. This poll is the only place that
    // notices a device connecting or dropping on its own, so it owns refreshing the card too.
    refresh_bt_connected();
    char m[128];
    std::snprintf(m, sizeof m, "bt-vol: rocker now drives %s (GetBtStatus=%d)",
                  on ? "BLUETOOTH" : "the 3.5 mm jack", st);
    clog_(m);
    // On CONNECT, push the level the user last used on Bluetooth, so a session resumes where it left
    // off instead of at whatever the headphones happen to remember. Only meaningful with absolute
    // volume — there is no way to command a level with up/down steps, so a step-only sink just keeps
    // its own and the UI count stays a belief until the next press.
    // …but first re-assert "Use Enhanced Mode". The radio does not carry the preference across a
    // link, and Sony's SetCurrentVolume checks it before transmitting, so pushing the level before
    // this would be a silent no-op on a fresh connection.
    if (on) bt_apply_enhanced_mode("bt connect");
    // Subscribe to the sink's own volume reports. Done on CONNECT because that is when there is a
    // sink to report, and it is what keeps the UI honest on a link that refuses absolute volume.
    if (on) run_guarded("bt: volume listener", 6, bt_vol_listener_start);
    if (on && bt_use_absolute_volume()) apply_bt_volume(true);
    // PROVOKE THE FIRST REPORT. There is no getter for the sink's level — OnNotifyChangeVolume is
    // the only route — so until the sink volunteers one, the bar is still the stale persisted
    // belief from the last session. Reported as "3 was mute until I went to mute, then it worked":
    // going to mute forced a change, the sink reported, and the bar corrected itself.
    //
    // One step up and straight back down is a net-zero change to what the user hears, and it makes
    // the sink report its real level within a second of connecting. Only needed on the step path;
    // when absolute volume is accepted the push above already tells us where it landed.
    if (on && !bt_use_absolute_volume()) {
        run_guarded("bt: provoke a volume report", 6, []() {
            enum { VIDX_SetVolumeDown = 16, VIDX_SetVolumeUp = 17 };
            try {
                void* x = bt_xmit();
                if (!x) return;
                typedef int (*fn0)(void*);
                ((fn0)bt_slot(x, VIDX_SetVolumeUp))(x);
                usleep(150000);
                ((fn0)bt_slot(x, VIDX_SetVolumeDown))(x);
                clog_("bt-vol: nudged the sink to report its level (net zero)");
            } catch (...) { clog_("bt-vol: volume nudge threw"); }
        });
    }

    // ── RECONNECT EDGE: re-assert everything a re-opened output can have reset ────────────────
    // Report: "Bluetooth volume can become disconnected after it reconnects." Cinder pushes the
    // volume, the codec preference and the DSP chain when the USER changes them — which meant
    // exactly twice per boot for most of them. Tearing the sink down and bringing it back up is
    // the one event that can undo all three behind our back, and nothing was watching for it.
    //
    // This is the right place to watch from: `GetBtStatus`/`GetConnectInformation` are the only
    // link source this firmware has. There is no /sys/class/bluetooth, no hcitool and no
    // /var/lib/bluetooth on it (checked on device 2026-08-10 — all three absent), so a sysfs or
    // BlueZ-shaped detector would simply never fire here.
    if (on) {
        // Guarded individually: each is either a shell-out or a Sony IPC call, on the render
        // thread. Verify-first, so a resync can never fight a level the user just set.
        run_guarded("bt: resync volume after reconnect", 8,
                    []() { bt_resync_volume("bt reconnect"); });
        write_bt_pref();
        fx_cache_drop();   // a reconnect is exactly when the service may have moved under us
        run_guarded("bt: re-apply EQ after reconnect", 6, apply_eq_fn);
        run_guarded("bt: re-apply sound after reconnect", 6, apply_sound_fn);
    }
}

// ── the USB gadget's OTHER owner ─────────────────────────────────────────────────────────────
// The gadget is written by TWO subsystems that do not know about each other, and Cinder was only
// driving one of them:
//
//   * init (/init.usbcfg.rc), on property:sys.sony.config={adb,uac,msc} — writes functions,
//     idVendor/idProduct (0B8B/0B8C/0B8D), AND f_mass_storage/lun/file, and is the ONLY thing that
//     runs unmount_msc1 (umount /contents) / mount_msc1 (mount_partition contents). This is the
//     half `cinder-msc` drives.
//   * UsbMgrServiceFw (UsbMgrImplWmport) — SetUsbFunction -> UpdateUsbFunction -> SetUac()/SetMsc(),
//     writing idVendor/idProduct (from DmpFeature, hence the 0ca0 that matches none of init's PIDs),
//     functions and MaxPower. It NEVER touches lun/file or the mount.
//
// Measured 2026-08-11 with `cinder-probe --usbmgr`, idle: the service reported GetUsbFunction=2
// (MSC) while sys.sony.config read `adb`, the gadget sat at mass_storage,adb 054c:0ca0, and
// lun/file was EMPTY. So the two owners were already disagreeing before anyone touched anything.
//
// Why that broke USB-DAC: setting the property alone leaves the service still believing MSC, and
// ANY of its own triggers — cable insert, Resume(), OnDeviceConnectedChanged — re-runs
// UpdateUsbFunction and rewrites the gadget back to mass storage. The PC then never enumerates a
// sound card, or enumerates one that disappears. Telling the service too is what makes the switch
// stick.
//
// dlopen, NOT a link: a DT_NEEDED on the Home app for a path this thin is the libNfcService
// boot-to-nothing rule. cinder-probe links it properly; here it is optional by construction.
enum { USBFN_UAC = 1, USBFN_MSC = 2 };   // read from the cmp #1/#2 switch in UpdateUsbFunction

static bool usb_set_function(unsigned fn) {
    enum { VIDX_SetUsbFunction = 3, VIDX_GetUsbFunction = 4 };
    static void* cli = nullptr;
    static bool tried = false;
    if (!tried) {
        tried = true;
        void* h = dlopen("libUsbMgrServiceFw.so", RTLD_NOW);
        if (!h) { clog_("usb-fn: libUsbMgrServiceFw.so unavailable — property-only switch"); return false; }
        typedef void* (*mk)(void);
        mk f = (mk)dlsym(h, "_ZN3pst8services28UsbMgrServiceFwClientFactory14CreateInstanceEv");
        if (!f) { clog_("usb-fn: UsbMgrServiceFwClientFactory symbol missing"); return false; }
        try { cli = f(); } catch (...) { cli = nullptr; }
        clog_(cli ? "usb-fn: UsbMgrServiceFwClient ready" : "usb-fn: factory returned null");
    }
    if (!cli) return false;
    // ReqMsg_SetUsbFunction is { uint32 function } (SizeOfReqMsg = 4, WriteReqMsg does one Alloc(4)
    // and copies one word from offset 0). The GET reply is EIGHT bytes — { uint32 result; uint32
    // value } — so its answer lives at offset 4, not 0. That cost a wasted read: the first probe
    // run printed "function 0 (??)" because it trusted rsp[0].
    typedef void (*fnrr)(void*, const void*, void*);
    unsigned req[8], rsp[8];
    void** vt = *(void***)cli;
    std::memset(req, 0, sizeof req); std::memset(rsp, 0, sizeof rsp);
    try { ((fnrr)vt[VIDX_GetUsbFunction])(cli, req, rsp); }
    catch (...) { clog_("usb-fn: GetUsbFunction threw"); }
    const unsigned had = rsp[1];
    if (had == fn) return true;                    // already believes it; don't provoke a rewrite
    std::memset(req, 0, sizeof req); std::memset(rsp, 0, sizeof rsp);
    req[0] = fn;
    try { ((fnrr)vt[VIDX_SetUsbFunction])(cli, req, rsp); }
    catch (...) { clog_("usb-fn: SetUsbFunction threw"); return false; }
    char m[128];
    std::snprintf(m, sizeof m, "usb-fn: SetUsbFunction %u -> %u (%s) result=%u", had, fn,
                  fn == USBFN_UAC ? "UAC" : "MSC", rsp[0]);
    clog_(m);
    return true;
}

// ── the THIRD owner, and the only complete one ───────────────────────────────────────────────
// Recovered 2026-08-11 by RE of libUsbDeviceConnectionService.so, after `--uacgate` measured the
// gate shut on device. `UsbDeviceConnectionServiceClient::SetDeviceType` (vtable slot 5) is what
// stock actually drives; UsbMgrServiceFw only records the mode. SetDeviceType does, in order:
//
//   SetDeviceTypeUac()  @0xaed5 : stop adbd -> read sys.usb.vid -> write functions=audio_func,adb
//                                 + idVendor/idProduct + enable -> start adbd -> start mount_msc1
//   SetDeviceTypeMsc(b) @0xac5d : start unmount_msc1 -> stop adbd -> read sys.usb.msc1 and write it
//                                 into f_mass_storage/lun/file -> functions=mass_storage,adb + ids
//                                 -> start adbd
//   SetDeviceTypeAdb()  @0xb0ed : stop adbd -> ids -> start adbd -> start mount_msc1
//
// Two whole bugs collapse into that listing:
//
//   * mass storage. `unmount_msc1` + writing sys.usb.msc1 (=/emmc@contents) into lun/file is the
//     MEDIUM, and NOTHING outside this service does it. UsbMgrServiceFw sets the descriptor only,
//     which is exactly the "drive with no disk" the probe kept reporting.
//   * USB-DAC silence. The gadget rewrite makes the kernel re-enumerate; the monitor thread turns
//     that uevent into NotifyUacConnectEvnet, which ConnMgrServiceFw publishes as device 7
//     (UacHost) enabled. And device 7 is the gate: UsbAudioPlayerCore::NotifyChangeConnectionStatus
//     (@0x16c44) opens UsbAudioStreamMonitor ONLY on status==1, and the stream monitor is the sole
//     owner of the netlink socket (AF_NETLINK proto 24, group 1) the kernel announces
//     ACTION/FORMAT/FREQ/BITWIDTH on. No connect event -> no socket -> kFormatNone forever, no
//     matter how often we call Start().
//
// Measured before this landed (`cinder-probe --uacgate`, gadget at mass_storage,adb): device 6
// (MscHost) enabled=1 connected=1, device 7 (UacHost) enabled=0 connected=0, and
// socket(AF_NETLINK, SOCK_DGRAM, 24) returned "Protocol not supported" — the proto only exists
// while the UAC function is loaded. Every link of the chain was dark.
//
// Note we do NOT need root for any of this: `start mount_msc1` runs inside the service.
enum { USBDT_ADB = 1, USBDT_MSC = 2, USBDT_UAC = 3 };   // from the cmp #1/#2/#3 dispatch @0xab98

static bool usb_set_device_type(unsigned t) {
    enum { VIDX_SetDeviceType = 5 };   // vtable recovered from R_ARM_ABS32 relocations, slot 5;
                                       // AddListener/RemoveListener are 10/11 — the last two, as
                                       // in every other pst service.
    static void* cli = nullptr;
    static bool tried = false;
    if (!tried) {
        tried = true;
        void* h = dlopen("libUsbDeviceConnectionService.so", RTLD_NOW);
        if (!h) { clog_("usb-dt: libUsbDeviceConnectionService.so unavailable"); return false; }
        typedef void* (*mk)(void);
        mk f = (mk)dlsym(h, "_ZN3pst8services39UsbDeviceConnectionServiceClientFactory14CreateInstanceEv");
        if (!f) { clog_("usb-dt: UsbDeviceConnectionServiceClientFactory symbol missing"); return false; }
        try { cli = f(); } catch (...) { cli = nullptr; }
        clog_(cli ? "usb-dt: UsbDeviceConnectionServiceClient ready" : "usb-dt: factory returned null");
    }
    if (!cli) return false;
    // Not the Req/Rsp pattern: the proxy takes the enum BY CONST REFERENCE, i.e. a pointer to one
    // word, the same shape as BtTransmitter's setters.
    typedef int (*fnr)(void*, const unsigned*);
    void** vt = *(void***)cli;
    int rc = -1;
    try { rc = ((fnr)vt[VIDX_SetDeviceType])(cli, &t); }
    catch (...) { clog_("usb-dt: SetDeviceType threw"); return false; }
    char m[160];
    std::snprintf(m, sizeof m, "usb-dt: SetDeviceType(%u = %s) rc=%d%s", t,
                  t == USBDT_UAC ? "Uac" : t == USBDT_MSC ? "Msc" : "Adb", rc,
                  t == USBDT_MSC ? "  [unmounts /contents and attaches the medium]" : "");
    clog_(m);
    return true;
}

// ── FuncMode: the thing that actually gates USB-DAC ──────────────────────────────────────────
// Everything above drives the USB gadget, and the gadget turns out not to be what the audio
// service is watching. From libConnMgrServiceFw.so, ConnGlueUsbHost::CnvStatus (@0x19f24) — the
// function that decides what connmgr publishes for a device:
//
//     if (!connected)     out = { 2, 0 };
//     else if (dev != 7)  out = { 2, 1 };
//     else                out = { 2, (FuncMode == 1) };        // device 7 == UacHost
//
// and UsbAudioConnectionMonitor::Open (@0x1e1d0) does GetDeviceStatus(Device{7}, st) and believes
// it only when the word it reads is 1. So the gate on USB-DAC is FuncMode == 1 (UsbDac), and no
// amount of descriptor rewriting reaches it. Measured on device 2026-08-11 with the gadget healthy
// and adb up: FuncMode = 0 (MediaPlay), device 7 = 0/0. Never set, therefore never open.
//
// The enum is read off funcarch::GetName (@0x7e00, libFuncMgrServiceFw.so), which builds a
// std::map<FuncMode,const char*> inline with eight keys 0..7 over the contiguous .rodata run at
// 0xba69 — not guessed. GetCurrentFuncMode returns 9 of its own accord on binder failure, which is
// why "Invalid" is a fallback string rather than a key.
//
// EnterFuncMode is what stock runs when the user picks USB-DAC in Settings, and it is three calls
// under one mutex (FuncMgrServiceServiceImpl::EnterFuncMode @0x7fb4):
//     FireRequireExitFuncMode(current)                    listeners may veto
//     usbmgr::UsbMgrService::SetUsbFunction(...)          <- the only step we were doing
//     connmgr::ConnMgrService::SetDeviceHandleRules(...)  <- publishes device 7
//     pathmgr::PathMgrService::SetPath(...)               <- the audio routing path
// It early-outs when the requested mode already equals the current one, so calling it twice is
// harmless. That also means SetUsbFunction from apply_usb_dac below is now redundant — kept only
// because it is the proven-harmless half and costs one call.
enum { FM_MEDIAPLAY = 0, FM_USBDAC = 1 };

static bool usb_enter_func_mode(int mode) {
    typedef bool (*enterfn)(void*, const int*);
    static enterfn enter = nullptr;
    static bool tried = false;
    if (!tried) {
        tried = true;
        // dlopen, not a DT_NEEDED: the libNfcService rule. A link-time dependency on a library this
        // build has never exercised is a boot risk for the Home app, and nothing here runs at boot.
        void* h = dlopen("libFuncMgrService.so", RTLD_NOW);
        if (!h) { clog_("func-mode: libFuncMgrService.so unavailable"); return false; }
        enter = (enterfn)dlsym(h,
            "_ZN3pst8services8funcarch7funcmgr14FuncMgrService13EnterFuncModeERKNS1_8FuncModeE");
        if (!enter) { clog_("func-mode: EnterFuncMode symbol missing"); return false; }
        clog_("func-mode: FuncMgrService ready");
    }
    if (!enter) return false;
    // Stateless wrapper, exactly like funcarch::connmgr::ConnMgrService: the ctor at 0x5d84 only
    // reads the stack guard, and every method re-fetches the client via
    // Framework::GetServiceClient("FuncMgrServiceFw"). A dummy `this` is legitimate.
    char self[64];
    bool ok = false;
    try { ok = enter(self, &mode); }
    catch (...) { clog_("func-mode: EnterFuncMode threw"); return false; }
    // The bool is NOT a result — measured 2026-08-11, it came back false for a switch and a restore
    // that both demonstrably worked (FuncMode, connmgr device 7, the gadget descriptor and
    // /proc/asound all moved). Logged for the record; nothing branches on it.
    char m[128];
    std::snprintf(m, sizeof m, "func-mode: EnterFuncMode(%d = %s) [returned %s — not meaningful]",
                  mode, mode == FM_USBDAC ? "UsbDac" : "MediaPlay", ok ? "true" : "false");
    clog_(m);
    return true;
}

// run_guarded takes a bare function pointer (its jmp-buf guard cannot carry a closure), so each
// caller gets a thunk rather than a capturing lambda.
static void usb_fn_uac() { usb_set_function(USBFN_UAC); }
static void usb_fn_msc() { usb_set_function(USBFN_MSC); }
static void usb_dt_uac() { usb_set_device_type(USBDT_UAC); }
static void usb_dt_adb() { usb_set_device_type(USBDT_ADB); }
static void usb_fm_usbdac()    { usb_enter_func_mode(FM_USBDAC); }
static void usb_fm_mediaplay() { usb_enter_func_mode(FM_MEDIAPLAY); }

// ── the render half of USB-DAC ───────────────────────────────────────────────────────────────
// Flipping the gadget to `uac` only makes the PC ENUMERATE a sound card. It does not make sound
// come out of the 3.5 mm jack, because nothing has told Sony's player service to open the render
// path — which is why DAC mode was "recognised in audio, no output". That call is
// `UsbDeviceAudioPlayerServiceClient::Start` (vtable slot 4).
//
// Signature is not guesswork; the service side is exported in full:
//     UsbDeviceAudioPlayerService::Start(IUsbDeviceAudioPlayerService::stream_info_t&)
//     UsbDeviceAudioPlayerService::GetStatus(IUsbDeviceAudioPlayerService::stream_info_t&)
// The ref is NON-const, so stream_info_t is an OUT param. The client stub (0x235a4) unpacks the
// reply with six plain TransactionParam::Get calls and NO GetStr, so every field is a scalar and a
// zeroed buffer is safe to pass. That check is the whole ballgame on this platform: the same
// assumption applied to BtTransmitterServiceClient::GetConnectInformation crashed twice, because
// THAT struct does hold a std::string and the write landed at a garbage offset.
//
// cinder-home is an easel app, so the framework/looper is already running and these calls get
// their replies — no Pump() thread needed here (cf. reference: pst clients return uninitialised
// stack when nothing drives the looper).
extern "C" void* _ZN3pst8services40UsbDeviceAudioPlayerServiceClientFactory14CreateInstanceEv(void);

static void* g_uac_client = nullptr;

static void* uac_slot(void* obj, int idx) {
    void** vptr = *reinterpret_cast<void***>(obj);
    return vptr[idx];
}

// Build the client on demand. This exists because the first version created it ONLY inside
// uac_render(), while uac_listener_start() bailed on a null client — and the engage path called the
// listener first. So the listener registered nothing, silently, and the whole OnChangedFormat fix
// never ran: the device logged a clean UAC switch and a kFormatNone Start(), then nothing forever.
// Anything that needs the client asks for it here instead of assuming someone else built it.
static void* uac_client() {
    if (!g_uac_client) {
        try {
            g_uac_client =
                _ZN3pst8services40UsbDeviceAudioPlayerServiceClientFactory14CreateInstanceEv();
        } catch (...) { g_uac_client = nullptr; }
    }
    return g_uac_client;
}

// Has a real stream been opened since we entered DAC mode? Cleared on entry/exit. Used to stop the
// backstop poll below once the render path is genuinely up.
static bool g_uac_playing = false;

// The host's current stream format word, as last seen by the RENDER THREAD's GetStatus poll.
// 0 = kFormatNone = the PC is not streaming.
//
// This is a published value rather than something the LDAC bridge reads for itself, and that is the
// point: every pst client here is asynchronous, replies land on the framework looper, and
// g_uac_client is built and driven by the render thread. Calling GetStatus on it from the bridge
// thread as well would mean two threads in one client's transaction state — the exact shape of bug
// that cost weeks on PlayerService. One reader, one writer, one word.
static volatile sig_atomic_t g_uac_host_fmt = 0;

// The other two words of the same stream_info_t, published on the same terms and for the same
// reader. `freq` matters: the bridge has to open the capture at the rate the HOST chose and put
// that same rate in the transmitter handshake, and until 2026-08-11 it hardcoded 44100 in both
// places. That happened to be right for the PC it was proven on, and would have played ~9% sharp on
// any of the many hosts that default to 48 kHz. Sony's own service reads these to configure its
// renderer, so they are the intended source; the bridge still prefers what ALSA reports about the
// live PCM when it can get it, because that is the hardware's own answer rather than a service's.
static volatile sig_atomic_t g_uac_host_freq = 0;
static volatile sig_atomic_t g_uac_host_bits = 0;

// One-shot latch for the "what does a WORKING capture look like?" dump — see the render loop.
static bool g_uac_ref_dumped = false;

// Which output the USB-DAC session is currently rendering to: 1 = the LDAC bridge, 0 = the jack,
// -1 = not in DAC mode. Kept because the route can change WITHOUT the toggle being touched — the
// headphones can be switched off, run flat, or walk out of range in the middle of a session — and
// `apply_usb_dac` only ever ran on the toggle. Before this, turning the headphones off during
// USB-DAC left the bridge owning the capture with nowhere to send it and the jack silent until the
// user toggled the mode off and on again.
static int  g_uac_route = -1;
static bool g_uac_want_jack = false;   // route fell back to the jack; waiting for the bridge to let go

// Returns false if the client could not be built; caller logs. Never throws out.
static bool uac_render(bool start) {
    enum { VIDX_UacStart = 4, VIDX_UacStop = 5 };
    try {
        if (!uac_client()) return false;
        // Start takes an out-param; Stop takes NOTHING. Passing one to Stop was harmless under
        // AAPCS (the callee just ignores r1) but it was still a lie about the signature — the
        // recovered vtable reads `Stop()`, no arguments.
        typedef void (*fnp)(void*, void*);
        typedef void (*fn0)(void*);
        if (!start) {
            ((fn0)uac_slot(g_uac_client, VIDX_UacStop))(g_uac_client);
            clog_("usb-dac: Stop()");
            return true;
        }
        // Oversized + zeroed: the field COUNT is known (six reads), the struct's true size is not,
        // and over-allocating an out-param buffer is free while under-allocating smashes the stack.
        unsigned si[32];
        std::memset(si, 0, sizeof si);
        ((fnp)uac_slot(g_uac_client, VIDX_UacStart))(g_uac_client, si);
        // stream_info_t is THREE WORDS — { stream_type_t format; u32 freq; u32 bitwidth; } — and
        // there is NO action field. Read off the tail of
        // UsbAudioPlayerCore::GetStreamInfo(IUsbDeviceAudioPlayerService::stream_info_t&) @0x16ab0,
        // which is the function that fills the struct that goes over the wire:
        //
        //     str  r3, [r9, #0]      ; stream_type_t, looked up through an internal->IPC map
        //     ldr  r0, [r8, #72]  ->  str r0, [r9, #4]     ; freq
        //     ldrb r0, [r8, #76]  ->  str r0, [r9, #8]     ; bitwidth (a byte, widened)
        //
        // That also explains the listener signature OnChangedFormat(const u32&, const u32(&)[3]):
        // three words, not four. The earlier reading here had an invented leading `action` field, so
        // every label was shifted by one and the kFormatNone test was looking at the FREQUENCY. It
        // happened to behave (44100 is nonzero when playing, 0 when stopped) but it was wrong, and
        // the first real capture printed "format=44100 freq=32", which is what exposed it.
        //
        // Measured live 2026-08-11 with a PC streaming: { 1, 44100, 32 } — PCM, 44.1 kHz, 32-bit,
        // which is exactly the S32_LE/44100 the capture thread below already assumes.
        char m[192];
        std::snprintf(m, sizeof m, "usb-dac: Start() -> format=%u freq=%u bits=%u%s",
                      si[0], si[1], si[2],
                      si[0] == 0 ? "  <-- kFormatNone: the host is not streaming yet" : "");
        clog_(m);
        if (si[0] != 0) g_uac_playing = true;    // a real format: the render path is open
        return true;
    } catch (...) {
        clog_(start ? "usb-dac: Start() threw" : "usb-dac: Stop() threw");
        return false;
    }
}

// ── USB-DAC: Start() has to wait for the HOST, not for the user ─────────────────────────────────
// Why DAC mode was "recognised in audio, no output": Start(stream_info_t&) takes its parameter by
// NON-const reference — it is an OUT param. The service does not take the format from us; it learns
// it from a hotplug socket (UsbAudioStreamMonitor::UacInitHotplugSock / RecvUACEvent /
// ParseStreamInfo) and only then can UsbAudioPlayerCore::StartPlaying open the capture (OpenInhal)
// and the AudioTrack to the 3.5 mm chain (OpenExhal).
//
// Cinder called Start() exactly once, at the instant the user flipped the switch — before any PC
// was streaming. At that moment there is no valid stream_info, StartPlaying has nothing to open,
// and NOTHING EVER RETRIED. The switch reported success and the jack stayed silent.
//
// The service publishes precisely the event needed. Recovered from the dispatcher
// (UsbDeviceAudioPlayerServiceListenerProxy::OnChangedFormatBase): it loads the listener object,
// takes vptr[2], and calls it with r1 = one word and r2 = a three-word struct. So OnChangedFormat
// is LISTENER SLOT 2, and AddListener is client slot 6 — immediately after the last service method
// (5, Stop), the same placement BtCommon(30), BtTransmitter(39) and UsbMgr(12) all use.
//
// Same discipline as the BT volume listener: this runs on the framework looper, so it only sets a
// flag; the render loop is what calls Start().
static volatile sig_atomic_t g_uac_format_notify = 0;

struct CinderUacListener {
    virtual ~CinderUacListener() {}
    virtual void OnChangedFormat(const unsigned& a, const unsigned (&b)[3]) {
        // Read NOTHING beyond these four words — that is exactly what the dispatcher wrote.
        // `b` is the stream_info_t: { format, freq, bitwidth } (see uac_render). Deliberately not
        // trusted here — this only raises a flag and lets the Start()/GetStatus path do the read,
        // so there is exactly one decoder to get wrong.
        (void)a; (void)b;
        g_uac_format_notify = 1;
    }
};
// MUST be static: AddListener stores a RAW, unowned pointer (same rule as every other listener here).
static CinderUacListener g_uac_listener;
static bool g_uac_listener_on = false;

static void uac_listener_start() {
    enum { VIDX_UacAddListener = 6 };
    if (g_uac_listener_on) return;
    if (!uac_client()) { clog_("usb-dac: no client — cannot register OnChangedFormat"); return; }
    try {
        typedef int (*fnadd)(void*, void*, const std::string*);
        std::string key;                 // "" — a NotifyListeners FILTER KEY, not a label
        int rc = ((fnadd)uac_slot(g_uac_client, VIDX_UacAddListener))(
                     g_uac_client, (void*)&g_uac_listener, &key);
        g_uac_listener_on = (rc == 0);
        char m[128];
        std::snprintf(m, sizeof m, "usb-dac: OnChangedFormat listener rc=%d (%s)", rc,
                      rc == 0 ? "registered" : "FAILED — relying on the status poll");
        clog_(m);
    } catch (...) { clog_("usb-dac: AddListener threw"); }
}

// Render-thread only. The host started (or changed) its stream — NOW Start() can succeed.
static void uac_drain_format() {
    if (!g_uac_format_notify) return;
    g_uac_format_notify = 0;
    if (cinder_get_usb_dac() == 0) return;          // left DAC mode in the meantime
    if (cinder_get_bt_route()) return;              // the bridge owns the capture PCM, not us
    clog_("usb-dac: host changed the stream format — (re)opening the render path");
    uac_render(true);
}

// BACKSTOP: ask the service what the stream looks like, once a second while in DAC mode.
//
// The listener is the right mechanism, but it only fires on a CHANGE. Two cases it cannot cover:
// a host that was already streaming before the user toggled DAC mode (no change to report), and a
// registration that failed. Both end in permanent silence with no way to notice, which is precisely
// the failure this whole exercise is about — so it gets a cheap, self-limiting poll rather than
// trust.
//
// GetStatus (slot 3) is a pure read of the same stream_info_t Start() fills in, so this costs one
// IPC per second and stops the moment a real format has been opened.
// The UAC gadget registers its own ALSA card while the gadget is in audio_func mode, at whatever
// index is free (4 on this unit, but that is not guaranteed — the ldac bridge scans for it too).
// Its capture substream tells us whether ANYONE has opened the host's PCM: "closed" means nobody,
// which is the kernel-side view of the same silence.
static void uac_capture_state(char* out, size_t n) {
    std::snprintf(out, n, "no UAC card");
    for (int c = 1; c <= 8; c++) {
        char p[64];
        std::snprintf(p, sizeof p, "/proc/asound/card%d/pcm0c/sub0/status", c);
        FILE* f = std::fopen(p, "r");
        if (!f) continue;
        char buf[96] = {0};
        if (std::fgets(buf, sizeof buf, f)) {
            size_t l = std::strlen(buf);
            while (l && (buf[l-1] == '\n' || buf[l-1] == '\r')) buf[--l] = 0;
            std::snprintf(out, n, "card%d %s", c, buf);
        }
        std::fclose(f);
        return;
    }
}

static void uac_poll_status() {
    enum { VIDX_UacGetStatus = 3 };
    static int ticks = 0;
    if (cinder_get_usb_dac() == 0) { ticks = 0; g_uac_host_fmt = 0; return; }
    // When the LDAC bridge owns the capture there is no local render path to open, but the bridge
    // still needs to know when the host starts and stops — so keep polling for it. Otherwise the old
    // rule holds: once a real format has been opened, stop looking.
    const bool bridging = cinder_get_bt_route() != 0;
    if (!bridging && g_uac_playing) return;
    // THROTTLED, for the same reason as bt_poll_sound_status: GetStatus is a synchronous IPC round
    // trip on the render thread and the call site runs it every frame. With the LDAC bridge up, that
    // was TWO per-frame round trips on the thread that also paints.
    //
    // The rates are picked from what actually consumes the answer, not from taste:
    //   * bridging — ldac_wait_for_host() polls this word on a 200 ms sleep, so publishing faster
    //     than 200 ms cannot be noticed by anyone. Same figure, 1/12th the calls.
    //   * otherwise — 1 s, which also FIXES the heartbeat below. `ticks` was written as a count of
    //     seconds ("every 5 s for the first two minutes") but incremented once per frame, so at
    //     60 Hz it printed its 24 lines inside the first two SECONDS and then went silent for good —
    //     destroying the one diagnostic it exists to provide. At 1 Hz the comment becomes true.
    static long last_ms = 0;
    const long every = bridging ? 200 : 1000;
    const long now = now_ms();
    if (now - last_ms < every) return;
    last_ms = now;
    if (!uac_client()) return;
    unsigned si[32];
    std::memset(si, 0, sizeof si);
    try {
        typedef void (*fnp)(void*, void*);
        ((fnp)uac_slot(g_uac_client, VIDX_UacGetStatus))(g_uac_client, si);
    } catch (...) { return; }
    // si[0] is the format word — see the layout note in uac_render(). This used to test si[1], the
    // FREQUENCY, which only ever behaved because a live stream has a nonzero rate.
    g_uac_host_fmt  = (sig_atomic_t)si[0];          // publish for the bridge thread
    g_uac_host_freq = (sig_atomic_t)si[1];
    g_uac_host_bits = (sig_atomic_t)si[2];
    // And publish it to the UI, so the USB-DAC panel reads out the real stream instead of a
    // generic line. Guarded on the freq word looking like Hz: if it turns out to be an enum we
    // have not decoded, the panel keeps its honest generic line rather than printing "0.0 KHZ".
    // Channels are not in stream_info_t, so nothing is claimed about them.
    cinder_set_usb_dac_format(si[0] && si[1] >= 8000 && si[1] <= 384000 ? si[1] : 0, si[2], 0);
    if (bridging) {
        // Say what the host actually picked, once per change. The bridge acts on this, so when a
        // session comes out at the wrong pitch the log has to already contain the reason.
        static unsigned last = 0;
        unsigned now = si[0] ? si[1] : 0;
        if (now != last) {
            last = now;
            char m[160];
            std::snprintf(m, sizeof m, "usb-dac: host stream format=%u freq=%u bits=%u",
                          si[0], si[1], si[2]);
            clog_(m);
        }
        ticks = 0;
        return;                                     // the bridge opens the PCM, not us
    }
    if (si[0] == 0) {
        // HEARTBEAT. Silence in the log is ambiguous — it reads the same whether the host never
        // streamed or the service never noticed, and that ambiguity has already cost two test
        // rounds. Print what is actually true, every 5 s for the first two minutes, so the log
        // distinguishes "nobody played anything" from "the PC is streaming and Sony's service is
        // not seeing it". Cheap, bounded, and it stops the moment a format appears.
        ticks++;
        if (ticks <= 120 && (ticks % 5) == 1) {
            char cap[96];
            uac_capture_state(cap, sizeof cap);
            char m[192];
            std::snprintf(m, sizeof m, "usb-dac: waiting for the host — t=%ds GetStatus format=0, "
                                       "capture=%s", ticks, cap);
            clog_(m);
        }
        return;                                     // kFormatNone — host still not streaming
    }
    char m[160];
    std::snprintf(m, sizeof m, "usb-dac: GetStatus sees a live stream (format=%u freq=%u bits=%u) "
                               "without a notify — opening the render path", si[0], si[1], si[2]);
    clog_(m);
    uac_render(true);
}

// Read the gadget's ACTUAL mode rather than trusting our own toggle.
//
// This exists because the two states genuinely diverge. `apply_usb_dac` only ever writes the
// gadget; nothing read it back, so anything that changed USB mode outside Cinder — a probe, a
// crash mid-toggle, a stock-side change — left Settings reporting the opposite of the hardware,
// with no way to correct it except toggling all the way through. Measured 2026-07-29: the gadget
// sat in `uac` while the UI read MASS STORAGE, and the only route back out was DAC-then-off.
bool gadget_in_dac_mode() {
    FILE* f = ::popen("/system/bin/getprop sys.sony.config 2>/dev/null", "r");
    if (!f) return false;
    char buf[64] = {0};
    if (!std::fgets(buf, sizeof buf, f)) { ::pclose(f); return false; }
    ::pclose(f);
    return std::strncmp(buf, "uac", 3) == 0;
}

// ── USB-DAC → LDAC bridge ───────────────────────────────────────────────────────────────────
// The headline feature: PCM arriving from the PC over USB is re-encoded to LDAC and sent to the
// headphones, which stock refuses to do (its block is pure app policy — a "disconnect Bluetooth"
// overlay plus an explicit RequestDisconnection; we simply never do either).
//
// Data path, all of it proven on device 2026-07-29 by `cinder-probe --ldac`:
//
//   PC --USB--> UAC gadget ALSA capture card --> this thread --> abstract AF_UNIX socket
//               --> BtTransmitterService (LDAC encode) --> MTK BT chip --> headphones
//
// Control plane: SetLdac(true) -> SetLdacSoundQuality -> SetCurrentSource(true) makes the service
// bind and listen on the socket, then GetSocketName(std::string&) names it. Two traps, both already
// paid for and both documented at their call sites below: GetSocketName takes its string BY
// REFERENCE and is not sret, and the abstract socket's addrlen is part of its NAME.
//
// WHY THIS LIVES IN CINDER-HOME and not in ldac-bridge, which was written for it: these are
// `pst::services::*` clients, so every call is asynchronous and the reply arrives on
// pst::core::Framework's looper. A standalone daemon pumps nothing, so its calls do not fail — they
// return uninitialised stack, which is the trap that cost weeks on PlayerService. cinder-home is an
// easel app with a live framework and an already-working BtTransmitterServiceClient, so it is the
// only process here that can drive this correctly.
//
// THREADING: the capture/write loop blocks on ALSA, so it gets its OWN pthread. It must never run on
// the render thread — a stalled USB host would freeze the UI and then trip the frame watchdog into a
// fatal _exit. Nothing here touches the renderer or the navigator; the only shared state is two
// flags and the log.
static volatile sig_atomic_t g_ldac_run   = 0;   // 1 = the bridge should keep going
static volatile sig_atomic_t g_ldac_alive = 0;   // 1 = the thread exists (don't spawn a second)

// libasound is dlopen'd, NOT linked. This is a boot-safety decision, not a style one: cinder-home is
// the HOME app, so a DT_NEEDED entry that fails to resolve does not disable a feature — it stops the
// only user-facing process on the device from starting at all, and the device then boots to nothing.
// The same reasoning as the escape-ladder rule (an escape must depend on less than what it rescues):
// the UI must not depend on the audio library that one optional feature wants. Resolved lazily on
// first use, so a device without libasound simply reports the bridge unavailable and keeps running.
//
// Types and constants are declared here rather than pulled from <alsa/asoundlib.h> so this needs no
// armhf libasound2-dev on the host. The values are ALSA-ABI-stable.
typedef struct _snd_pcm snd_pcm_t;
enum { SND_PCM_STREAM_CAPTURE = 1 };
enum { SND_PCM_ACCESS_RW_INTERLEAVED = 3 };
enum { SND_PCM_FORMAT_S32_LE = 10 };

struct AlsaApi {
    void* h = nullptr;
    int  (*open)(snd_pcm_t**, const char*, int, int) = nullptr;
    int  (*set_params)(snd_pcm_t*, int, int, unsigned, unsigned, int, unsigned) = nullptr;
    long (*readi)(snd_pcm_t*, void*, unsigned long) = nullptr;
    int  (*prepare)(snd_pcm_t*) = nullptr;
    int  (*close)(snd_pcm_t*) = nullptr;
    const char* (*strerr)(int) = nullptr;
    // Diagnostics + explicit start. All OPTIONAL — ok() deliberately does not require them, so a
    // libasound missing any of these still gives a working bridge, just a quieter one.
    int  (*state)(snd_pcm_t*) = nullptr;
    int  (*start)(snd_pcm_t*) = nullptr;
    long (*avail_update)(snd_pcm_t*) = nullptr;
    int  (*wait)(snd_pcm_t*, int) = nullptr;
    // Rate/channel interrogation. Also optional: without these the bridge falls back to the rate
    // Sony's service reports, and then to 44100.
    size_t (*hwp_sizeof)(void) = nullptr;
    int  (*hwp_any)(snd_pcm_t*, void*) = nullptr;
    int  (*hwp_rate_min)(const void*, unsigned*, int*) = nullptr;
    int  (*hwp_rate_max)(const void*, unsigned*, int*) = nullptr;
    int  (*hwp_chan_min)(const void*, unsigned*) = nullptr;
    bool ok() const { return open && set_params && readi && prepare && close; }
    bool can_query() const { return hwp_sizeof && hwp_any && hwp_rate_min && hwp_rate_max; }
};
static AlsaApi g_alsa;

static bool alsa_load() {
    if (g_alsa.h) return g_alsa.ok();
    // Try the versioned SONAME first: that is what actually exists on a stock rootfs, while the bare
    // .so is a dev symlink that may not be shipped.
    const char* cands[] = { "libasound.so.2", "libasound.so", "/lib/libasound.so.2",
                            "/lib/libasound.so", "/system/lib/libasound.so" };
    for (const char* c : cands) {
        g_alsa.h = dlopen(c, RTLD_NOW | RTLD_LOCAL);
        if (g_alsa.h) break;
    }
    if (!g_alsa.h) { clog_("ldac: dlopen(libasound) FAILED — bridge unavailable"); return false; }
    g_alsa.open       = (int  (*)(snd_pcm_t**, const char*, int, int))dlsym(g_alsa.h, "snd_pcm_open");
    g_alsa.set_params = (int  (*)(snd_pcm_t*, int, int, unsigned, unsigned, int, unsigned))
                        dlsym(g_alsa.h, "snd_pcm_set_params");
    g_alsa.readi      = (long (*)(snd_pcm_t*, void*, unsigned long))dlsym(g_alsa.h, "snd_pcm_readi");
    g_alsa.prepare    = (int  (*)(snd_pcm_t*))dlsym(g_alsa.h, "snd_pcm_prepare");
    g_alsa.close      = (int  (*)(snd_pcm_t*))dlsym(g_alsa.h, "snd_pcm_close");
    g_alsa.strerr     = (const char* (*)(int))dlsym(g_alsa.h, "snd_strerror");
    g_alsa.state        = (int  (*)(snd_pcm_t*))dlsym(g_alsa.h, "snd_pcm_state");
    g_alsa.start        = (int  (*)(snd_pcm_t*))dlsym(g_alsa.h, "snd_pcm_start");
    g_alsa.avail_update = (long (*)(snd_pcm_t*))dlsym(g_alsa.h, "snd_pcm_avail_update");
    g_alsa.wait         = (int  (*)(snd_pcm_t*, int))dlsym(g_alsa.h, "snd_pcm_wait");
    g_alsa.hwp_sizeof   = (size_t (*)(void))dlsym(g_alsa.h, "snd_pcm_hw_params_sizeof");
    g_alsa.hwp_any      = (int (*)(snd_pcm_t*, void*))dlsym(g_alsa.h, "snd_pcm_hw_params_any");
    g_alsa.hwp_rate_min = (int (*)(const void*, unsigned*, int*))
                          dlsym(g_alsa.h, "snd_pcm_hw_params_get_rate_min");
    g_alsa.hwp_rate_max = (int (*)(const void*, unsigned*, int*))
                          dlsym(g_alsa.h, "snd_pcm_hw_params_get_rate_max");
    g_alsa.hwp_chan_min = (int (*)(const void*, unsigned*))
                          dlsym(g_alsa.h, "snd_pcm_hw_params_get_channels_min");
    if (!g_alsa.ok()) { clog_("ldac: libasound is missing a PCM symbol — bridge unavailable"); return false; }
    return true;
}

// snd_strerror is optional (only used for messages), so don't let a missing one break anything.
static const char* alsa_err(int e) {
    return g_alsa.strerr ? g_alsa.strerr(e) : "(errno)";
}

// The handshake that turns the connection into a PCM pipe. Returns true if the service kept it.
//
// THIS IS THE WHOLE FEATURE, AND SKIPPING IT IS WHAT REBOOTED THE DEVICE TWICE ON 2026-08-11.
// The socket starts in frame-parsing mode: `4-byte type | 4-byte length | new[](length) | payload`,
// with type 0 => len 0, type 1 => len 28, type 2 => len 12 and anything else closed. PCM written
// before the handshake is read as a type and a length, and a garbage length reaches `operator new[]`
// inside a core service — hence the reboots.
//
// A *type-1* frame does not merely configure the stream, it hands the connection over.
// `BtTransmitterExHal::OnEvent` (libBtTransmitterService.so @0x9fc0) ends that path with
//
//     r1 = p[4]; p[4] = 0;   old = this->[12];   this->[12] = r1;   this->[0x2c] = 1;
//     pthread_create(stream thread)
//
// and the thread it starts (@0xa714) is a pump: `reader->Read(pcm_buf, pcm_size, &got)` then
// `BtAvSrcComponentIf::SendData(got, pcm_buf)`. `this->[12]` is *our* connection, so from that
// point the same fd carries raw PCM and Sony does the LDAC encoding (blueangel links libldacBTBC)
// and the AVDTP bookkeeping. `WriteSilent()` is the identical loop over a zeroed buffer.
//
// Measured end to end 2026-08-11 with `cinder-probe --btopen tone`: accepted, and 10 s of audio
// took 9.8 s of wall clock to write — the peer drains at exactly rate x channels x 2 bytes, which
// is both the proof that it is a live A2DP stream and the proof that the wire format is S16_LE.
// The tone was audible in the headphones.
static bool ldac_handshake(int fd, unsigned rate, unsigned chans) {
    unsigned char frame[8 + 28];
    std::memset(frame, 0, sizeof frame);
    unsigned type = 1, len = 28;
    std::memcpy(frame + 0, &type, 4);
    std::memcpy(frame + 4, &len,  4);
    // Only the three fields OnEvent actually reads. The rest stays zero deliberately: the handler
    // never touches payload +0/+8/+12/+16, and inventing values for fields we have not identified
    // is exactly how the previous round went wrong.
    std::memcpy(frame + 8 + 4,  &chans, 4);   // channel count: 1 stays 1, anything else becomes 2
    frame[8 + 20] = 1;                        // the u8 flag at payload+20
    std::memcpy(frame + 8 + 24, &rate,  4);   // Hz, checked against the negotiated BtSoundFrequency
                                              // (.rodata 0x1c130: 1->44100 2->48000 4->88200 8->96000)
    if (send(fd, frame, sizeof frame, MSG_NOSIGNAL) != (ssize_t)sizeof frame) {
        clog_("ldac: handshake send failed");
        return false;
    }

    // VERIFY BEFORE SENDING A SINGLE SAMPLE. A rejected frame gets the connection closed; an
    // accepted one leaves it open with the ExHal's stream thread blocked reading it. Those are
    // distinguishable from here, and the cost of being wrong is a device reboot, so pay the second.
    for (int i = 0; i < 20; i++) {
        struct pollfd p = { fd, POLLIN, 0 };
        int pr = poll(&p, 1, 100);
        if (pr > 0) {
            if (p.revents & (POLLHUP | POLLERR)) {
                clog_("ldac: handshake REJECTED — the transmitter closed the connection; not "
                      "sending audio");
                return false;
            }
            char b[64];
            ssize_t r = recv(fd, b, sizeof b, MSG_DONTWAIT);
            if (r == 0) {
                clog_("ldac: handshake REJECTED — the transmitter closed the connection; not "
                      "sending audio");
                return false;
            }
        }
    }
    clog_("ldac: handshake accepted — the socket is now a PCM pipe");
    return true;
}

// Connect to the transmitter's PCM socket and complete the handshake at the host's actual format.
// Returns a ready fd, or -1.
static int ldac_connect_socket(unsigned rate, unsigned chans) {
    enum { VIDX_GetSocketName = 29 };
    void* x = bt_xmit();
    if (!x) { clog_("ldac: BtTransmitterServiceClient unavailable"); return -1; }

    // void GetSocketName(pst::base::string&) — READ from the library's own __PRETTY_FUNCTION__
    // literal, not inferred. It is NOT sret: calling it as `fn(sret, this)` hands a stack buffer to
    // the function as `this`, which is what used to die at PC=0 with libcxxrt reporting "Fatal error
    // during phase 1 unwinding" — a swapped argument list that got misread as evidence about the
    // Bluetooth link. `pst::base::string` is a typedef for libc++ std::string (the mangled form
    // N3pst4base6stringE exists nowhere in the vendor tree, and the marshaller's own PLT entry is
    // TransactionParam::GetStr(std::__1::basic_string<char,...>&)); this file compiles against the
    // libc++ 3.9.0 headers matching the device runtime, so a real std::string is ABI-correct.
    std::string name;
    try {
        typedef void (*fns)(void*, std::string*);
        ((fns)bt_slot(x, VIDX_GetSocketName))(x, &name);
    } catch (...) { clog_("ldac: GetSocketName threw"); return -1; }
    if (name.empty()) { clog_("ldac: GetSocketName returned empty — source not open"); return -1; }

    char m[192];
    std::snprintf(m, sizeof m, "ldac: socket name '%s'", name.c_str());
    clog_(m);

    // ADDRLEN IS PART OF THE NAME. An abstract AF_UNIX address is a byte string of length
    // (addrlen - offsetof(sun_path)), compared exactly, trailing NULs included. The service binds
    // with the FULL sockaddr_un (addrlen 110), so its real name is the 35-character string followed
    // by 72 NULs; sizing addrlen to strlen() asks for a different name and earns ECONNREFUSED from a
    // socket that /proc/net/unix plainly shows as listening (flags 00010000 = SO_ACCEPTCON).
    // The bind is also ASYNC after SetCurrentSource, hence the retry.
    for (int i = 0; i < 30 && g_ldac_run; i++) {
        int fd = socket(AF_UNIX, SOCK_STREAM, 0);
        if (fd < 0) return -1;
        struct sockaddr_un a;
        std::memset(&a, 0, sizeof a);
        a.sun_family = AF_UNIX;
        size_t n = name.size();
        if (n > sizeof a.sun_path - 1) n = sizeof a.sun_path - 1;
        std::memcpy(a.sun_path + 1, name.data(), n);   // abstract: sun_path[0] stays NUL
        if (connect(fd, (struct sockaddr*)&a, (socklen_t)sizeof a) == 0) {
            // The rate here MUST be the one the capture is opened at — the service checks it
            // against the negotiated BtSoundFrequency, and a mismatch is rejected rather than
            // silently mishandled, which is the behaviour we want.
            if (ldac_handshake(fd, rate, chans)) return fd;
            close(fd);
            return -1;      // rejected is a real answer — retrying the same frame gets the same one
        }
        close(fd);
        usleep(100000);
    }
    clog_("ldac: connect to the transmitter socket FAILED");
    return -1;
}

// Find the UAC gadget's capture PCM. The gadget registers a SEPARATE ALSA card only while it is in
// UAC mode, and the kernel gives it the next free index — so "hw:4,0" is a guess, not a fact. Scan
// for a capture-capable card that is not the built-in codec instead.
static bool ldac_find_capture(char* out, size_t outn) {
    FILE* f = std::fopen("/proc/asound/cards", "r");
    if (!f) return false;
    char line[256];
    bool found = false;
    while (!found && std::fgets(line, sizeof line, f)) {
        int idx; char id[64];
        if (std::sscanf(line, " %d [%63[^]]", &idx, id) != 2) continue;
        char* e = id + std::strlen(id);
        while (e > id && e[-1] == ' ') *--e = 0;              // trim the pad spaces
        if (std::strcmp(id, "sonysoccard") == 0) continue;    // built-in codec, not the gadget
        for (int d = 0; d < 8 && !found; ++d) {
            char p[64];
            std::snprintf(p, sizeof p, "/proc/asound/card%d/pcm%dc", idx, d);
            if (access(p, F_OK) == 0) {
                std::snprintf(out, outn, "hw:%d,%d", idx, d);
                found = true;
            }
        }
    }
    std::fclose(f);
    return found;
}

// Is the HOST streaming? Returns the format word; 0 = kFormatNone = it is not.
//
// GetStatus (client slot 3) is a pure read of the same stream_info_t Start() fills in —
// { format, freq, bitwidth } — so unlike Start() it does NOT take the capture PCM the bridge needs,
// which is what makes it usable as a gate at all. The render loop does that read once a second and
// publishes the result here (see g_uac_host_fmt); the bridge only ever reads the word.
static unsigned uac_stream_format() {
    return (unsigned)g_uac_host_fmt;
}

// ── bridge diagnostics ──────────────────────────────────────────────────────────────────────────
// A blocking snd_pcm_readi that never returns tells you nothing at all, and that is exactly what the
// first on-device run produced: "streaming", then ~30 s of silence, then -EIO with zero frames. The
// kernel already publishes everything needed to tell "the stream never started" apart from "the
// stream started and starved" — so read it and put it in the log rather than inferring.
static const char* pcm_state_name(int s) {
    static const char* n[] = { "OPEN", "SETUP", "PREPARED", "RUNNING", "XRUN",
                               "DRAINING", "PAUSED", "SUSPENDED", "DISCONNECTED" };
    return (s >= 0 && s < (int)(sizeof n / sizeof n[0])) ? n[s] : "?";
}

// Log the first `max` non-empty lines of a /proc file, prefixed. Cheap and bounded.
static void log_proc_file(const char* path, const char* tag, int max) {
    FILE* f = std::fopen(path, "r");
    if (!f) {
        char m[192];
        std::snprintf(m, sizeof m, "ldac: %s %s — not readable", tag, path);
        clog_(m);
        return;
    }
    char line[160];
    int n = 0;
    while (n < max && std::fgets(line, sizeof line, f)) {
        size_t l = std::strlen(line);
        while (l && (line[l-1] == '\n' || line[l-1] == '\r')) line[--l] = 0;
        if (!l) continue;
        char m[224];
        std::snprintf(m, sizeof m, "ldac: %s | %s", tag, line);
        clog_(m);
        n++;
    }
    std::fclose(f);
}

// Dump what the driver thinks of the capture substream. `dev` is "hw:<card>,<device>".
static void ldac_dump_pcm(const char* dev, const char* when) {
    int card = 0, device = 0;
    if (std::sscanf(dev, "hw:%d,%d", &card, &device) != 2) return;
    char p[128], tag[64];
    std::snprintf(tag, sizeof tag, "%s hw_params", when);
    std::snprintf(p, sizeof p, "/proc/asound/card%d/pcm%dc/sub0/hw_params", card, device);
    log_proc_file(p, tag, 12);
    std::snprintf(tag, sizeof tag, "%s status", when);
    std::snprintf(p, sizeof p, "/proc/asound/card%d/pcm%dc/sub0/status", card, device);
    log_proc_file(p, tag, 10);
}

static void ldac_log_state(snd_pcm_t* pcm, const char* when) {
    if (!g_alsa.state) return;
    int  st = g_alsa.state(pcm);
    long av = g_alsa.avail_update ? g_alsa.avail_update(pcm) : -1;
    char m[160];
    std::snprintf(m, sizeof m, "ldac: %s state=%d (%s) avail=%ld", when, st, pcm_state_name(st), av);
    clog_(m);
}

// Block until Sony's service reports a live host stream. Returns the format word, or 0 if the
// bridge was stopped while waiting.
//
// WAIT FOR THE HOST BEFORE OPENING THE PCM. The capture card appears as soon as the gadget enters
// UAC mode — long before the PC has selected the Walkman as its output and pressed play. Opening
// then yields a PCM with no stream behind it, and measured on device 2026-08-11 that PCM is
// UNRECOVERABLE: the first read returns -EIO and every read afterwards returns -EBADFD ("file
// descriptor in bad state") no matter how many times snd_pcm_prepare is called. Retrying harder was
// the previous fix and it could not have worked — the object was already poisoned by being opened
// too early.
//
// Sony's service already knows when the host starts: it learns the format from the kernel's netlink
// announcement, and GetStatus reads it back without touching the PCM. So gate on that.
static unsigned ldac_wait_for_host() {
    for (int i = 0; g_ldac_run; i++) {
        unsigned fmt = uac_stream_format();
        if (fmt) return fmt;
        if (i % 150 == 0)          // ~30 s between notes at 200 ms
            clog_("ldac: waiting for the host to start streaming (GetStatus format=0)");
        usleep(200000);
    }
    return 0;
}

// What rate is the host ACTUALLY running the gadget at?
//
// Ask ALSA before configuring anything: on a freshly opened PCM, snd_pcm_hw_params_any gives the
// endpoint's full capability set, and a USB gadget whose host has selected an altsetting reports a
// single rate (min == max). That is the hardware's own answer. Once set_params has run the query
// only reads back whatever we ourselves asked for, so this must happen first.
//
// Returns 0 if it cannot tell, in which case the caller falls back to Sony's reported freq and then
// to 44100. `chans_out` is advisory and only ever set to something this bridge can carry.
static unsigned ldac_capture_rate(snd_pcm_t* pcm, unsigned* chans_out) {
    if (chans_out) *chans_out = 2;
    if (!g_alsa.can_query()) return 0;
    std::vector<unsigned char> hwp(g_alsa.hwp_sizeof(), 0);
    if (g_alsa.hwp_any(pcm, hwp.data()) < 0) return 0;
    unsigned lo = 0, hi = 0;
    int dir = 0;
    if (g_alsa.hwp_rate_min(hwp.data(), &lo, &dir) < 0) return 0;
    dir = 0;
    if (g_alsa.hwp_rate_max(hwp.data(), &hi, &dir) < 0) return 0;
    if (chans_out && g_alsa.hwp_chan_min) {
        unsigned c = 0;
        if (g_alsa.hwp_chan_min(hwp.data(), &c) == 0 && c >= 1) *chans_out = c;
    }
    char m[160];
    std::snprintf(m, sizeof m, "ldac: capture advertises rate %u..%u Hz, %u ch", lo, hi,
                  chans_out ? *chans_out : 2);
    clog_(m);
    // A pinned endpoint says one rate. A range means the gadget has not committed and the query
    // cannot answer the question — say so rather than picking an end of the range at random.
    return (lo == hi) ? lo : 0;
}

// Sony's stream_info_t freq word, if it looks like a rate in Hz rather than an enum we have not
// decoded. Deliberately conservative: a value outside audio range is treated as "no answer".
static unsigned uac_host_rate_hint() {
    unsigned f = (unsigned)g_uac_host_freq;
    return (f >= 8000 && f <= 384000) ? f : 0;
}

// The transmitter negotiates one of exactly four rates — the table at libBtTransmitterService.so
// .rodata 0x1c130, indexed by BtSoundFrequency: 1->44100, 2->48000, 4->88200, 8->96000. There is no
// resampler anywhere in this path, so a host at any other rate cannot be carried: sending 32000 Hz
// audio through a stream negotiated at 44100 plays it sharp, which is worse than not playing it.
static bool ldac_rate_supported(unsigned r) {
    return r == 44100 || r == 48000 || r == 88200 || r == 96000;
}

// Why a capture session ended. The distinction is what lets the bridge survive a PC that stops and
// restarts playback: only a dead socket (or an explicit stop) ends the thread.
enum ldac_end {
    LDAC_END_STOPPED,       // g_ldac_run cleared — the user toggled USB-DAC off
    LDAC_END_HOST_GONE,     // the PC stopped streaming; go back to waiting
    LDAC_END_PCM_DEAD,      // the PCM is unusable; drop it and reopen
    LDAC_END_SOCKET_GONE    // the transmitter closed — the bridge is finished
};

// The pump. 512 frames × 8 bytes = 4 KB a go, which is ~11.6 ms of audio — small enough to keep
// latency sane, large enough that the syscall rate stays negligible on one ARMv7 core.
static ldac_end ldac_pump(int fd, snd_pcm_t* pcm, const char* dev, unsigned rate,
                          unsigned long long* frames_out) {
    static unsigned char buf[512 * 8];
    unsigned long long frames = 0;
    int err_streak   = 0;      // consecutive recoverable errors in this stall
    int stall_rounds = 0;      // ~5 s stalls survived while the host still claims to be streaming
    int dry_waits    = 0;      // consecutive 1 s waits that produced no data at all
    ldac_end why = LDAC_END_STOPPED;

    while (g_ldac_run) {
        // NEVER BLOCK INDEFINITELY IN readi. The first on-device run sat inside one blocking read
        // for over half a minute and produced a single line of log at the end of it, which is the
        // least informative possible outcome. Waiting with a timeout first costs nothing when audio
        // is flowing (the fd is already readable) and turns a silent hang into a measurement.
        if (g_alsa.wait) {
            int w = g_alsa.wait(pcm, 1000);
            if (w == 0) {                                   // timeout: no data for a whole second
                if (++dry_waits == 3 || dry_waits % 15 == 0) {
                    char m[160];
                    std::snprintf(m, sizeof m, "ldac: no capture data for %d s", dry_waits);
                    clog_(m);
                    ldac_log_state(pcm, "dry");
                    ldac_dump_pcm(dev, "dry");
                }
                if (dry_waits >= 5 && uac_stream_format() == 0) {
                    clog_("ldac: the host stopped streaming — closing the capture and waiting");
                    why = LDAC_END_HOST_GONE;
                    break;
                }
                continue;
            }
            if (w < 0) {
                // -EPIPE etc. from the wait itself — fall through to the read, which reports it
                // through the single error path below rather than duplicating the handling here.
                if (w != -EPIPE && w != -EINTR) {
                    char m[128];
                    std::snprintf(m, sizeof m, "ldac: snd_pcm_wait -> %s", alsa_err(w));
                    clog_(m);
                }
            }
            dry_waits = 0;
        }
        long got = g_alsa.readi(pcm, buf, 512);
        if (got == -EAGAIN || got == -EINTR) continue;

        if (got == -EPIPE || got == -ESTRPIPE || got == -EIO) {
            // Overrun, suspend, or a momentary gap. snd_pcm_prepare fixes all three *when there is
            // still a stream behind the endpoint*; when there is not, it fixes nothing, so cap how
            // long we are willing to sit here before dropping back to the waiting state.
            if (err_streak == 0) {
                char m[128];
                std::snprintf(m, sizeof m, "ldac: capture stalled (%s) — recovering",
                              alsa_err((int)got));
                clog_(m);
                ldac_log_state(pcm, "stalled");
                ldac_dump_pcm(dev, "stalled");
            }
            g_alsa.prepare(pcm);
            usleep(20000);
            if (++err_streak >= 250) {                  // ~5 s
                err_streak = 0;
                if (uac_stream_format() == 0) {
                    clog_("ldac: the host stopped streaming — closing the capture and waiting");
                    why = LDAC_END_HOST_GONE;
                    break;
                }
                if (++stall_rounds >= 3) {              // ~15 s stalled with a live host: poisoned
                    clog_("ldac: capture will not recover though the host is streaming — reopening");
                    why = LDAC_END_PCM_DEAD;
                    break;
                }
                clog_("ldac: still stalled but the host is streaming — retrying");
            }
            continue;
        }

        if (got == -ENODEV || got == -ENXIO) {
            // The gadget's ALSA card went away: the cable was pulled, or USB-DAC was switched off.
            // That is a normal end of session, not a failure — measured 2026-08-11, a 14.8 s bridge
            // session ended exactly here when the mode was toggled off, and calling it "capture
            // stalled" in the log made a clean shutdown read like a fault.
            clog_("ldac: the UAC card went away (cable out, or DAC mode off) — waiting for it");
            why = LDAC_END_HOST_GONE;
            break;
        }
        if (got < 0) {
            // -EBADFD lands here: the PCM object itself is wrong-state, and no amount of prepare()
            // will move it. Reopening is the only cure, so say so rather than ending the bridge.
            char m[128];
            std::snprintf(m, sizeof m, "ldac: readi -> %s%s", alsa_err((int)got),
                          got == -EBADFD ? "  (PCM wrong-state — reopening)" : "");
            clog_(m);
            why = LDAC_END_PCM_DEAD;
            break;
        }

        if (err_streak || stall_rounds) {
            clog_("ldac: capture recovered");
            err_streak = stall_rounds = 0;
        }
        if (frames == 0) {
            char m[128];
            std::snprintf(m, sizeof m, "ldac: FIRST CAPTURE READ ok — %ld frames from the host", got);
            clog_(m);
        }

        // S32_LE in, S16_LE out. The capture side is 32-bit because that is what the UAC gadget
        // presents; the transmitter's reader is 16-bit, which is not a guess — the peer drained the
        // probe's tone at exactly rate x channels x 2 bytes per second, and a 16-bit sine came out
        // of the headphones as a clean 440 Hz tone rather than noise. Taking the top 16 bits is the
        // whole conversion: the gadget's low half is padding on a 24-bit-in-32 container.
        static short out16[512 * 2];
        {
            const int* s32 = (const int*)buf;
            for (long i = 0; i < got * 2; i++) out16[i] = (short)(s32[i] >> 16);
        }
        size_t want = (size_t)got * 4;
        const unsigned char* p = (const unsigned char*)out16;
        bool broken = false;
        while (want && !broken) {
            // send(MSG_NOSIGNAL), NOT write(). A write() to a socket whose peer has closed raises
            // SIGPIPE, whose default disposition is to KILL THE PROCESS — so the EPIPE branch below
            // could never run, and the first time this bridge actually carried audio it took the
            // Home app down with it (rc=141 = 128+SIGPIPE, measured 2026-08-11: the transmitter
            // closed its end and cinder-home died mid-session, leaving the launcher to hand back to
            // appmgr and the device with no Home app at all). MSG_NOSIGNAL turns that into the
            // errno it should always have been.
            ssize_t w = send(fd, p, want, MSG_NOSIGNAL);
            if (w < 0) {
                if (errno == EINTR) continue;
                // The transmitter closed its end — headphones dropped, or the source was released.
                // EPIPE here is normal shutdown, not a bug.
                char m[160];
                std::snprintf(m, sizeof m, "ldac: %s after %llu frames (%llu s)",
                              errno == EPIPE ? "transmitter closed the socket"
                                             : "socket write failed",
                              frames, frames / (rate ? rate : 44100));
                clog_(m);
                broken = true;
                break;
            }
            p += w;
            want -= (size_t)w;
        }
        if (broken) { why = LDAC_END_SOCKET_GONE; break; }
        frames += (unsigned long long)got;
    }

    *frames_out = frames;
    return why;
}

static void* ldac_thread(void*) {
    g_ldac_alive = 1;
    clog_("ldac: bridge thread up");

    // THE SOCKET IS OPENED LATE, AND THAT ORDER IS LOAD-BEARING. The handshake declares the stream
    // format, so it cannot be sent until the capture has told us what the host actually chose. This
    // used to connect first and hardcode 44100 in both the handshake and set_params, which was right
    // only for a host that happened to pick 44.1 kHz and would have played ~9% sharp on a 48 kHz one.
    int fd = -1;
    unsigned fd_rate = 0;      // the rate the current connection was handshaken at

    {
        // Wait for the gadget's capture card. It only appears once the gadget is in UAC mode, so
        // "not there yet" is the normal state for the first seconds after the toggle — poll rather
        // than failing.
        char dev[32] = {0};
        for (int i = 0; i < 100 && g_ldac_run && !ldac_find_capture(dev, sizeof dev); i++)
            usleep(100000);

        if (dev[0] == '\0') {
            clog_("ldac: no UAC capture card appeared — is the gadget actually in UAC mode?");
        } else {
            char m[160];
            std::snprintf(m, sizeof m, "ldac: capture device %s", dev);
            clog_(m);

            // SESSION LOOP. A PC pausing, switching output device, or sleeping ends the capture
            // stream but not the bridge: close the PCM, go back to waiting on GetStatus, and open a
            // fresh one when audio returns. Previously the first stream ending killed the thread,
            // which meant the bridge worked exactly once per USB-DAC toggle.
            int reopens = 0, reconnects = 0;
            unsigned long long total = 0;
            while (g_ldac_run) {
                unsigned fmt = ldac_wait_for_host();
                if (!fmt) break;                        // stopped while waiting
                std::snprintf(m, sizeof m, "ldac: host is streaming (format=%u) — opening %s",
                              fmt, dev);
                clog_(m);

                snd_pcm_t* pcm = nullptr;
                int rc = g_alsa.open(&pcm, dev, SND_PCM_STREAM_CAPTURE, 0);
                if (rc == -ENOENT || rc == -ENODEV) {
                    // The card index is not there any more. The gadget's card is created and
                    // destroyed with the USB connection, and its index is whatever the kernel had
                    // free, so a replug can land on a different one — rescan instead of ending the
                    // bridge, which is what used to happen and meant a cable pull cost you the
                    // feature until you toggled USB-DAC off and on again.
                    clog_("ldac: the capture card is gone — rescanning");
                    dev[0] = '\0';
                    for (int i = 0; i < 100 && g_ldac_run && !ldac_find_capture(dev, sizeof dev); i++)
                        usleep(100000);
                    if (dev[0] == '\0') {
                        clog_("ldac: no capture card came back — ending the bridge");
                        break;
                    }
                    std::snprintf(m, sizeof m, "ldac: capture device is now %s", dev);
                    clog_(m);
                    continue;
                }
                if (rc < 0) {
                    std::snprintf(m, sizeof m, "ldac: snd_pcm_open(%s) -> %s%s", dev, alsa_err(rc),
                                  rc == -EBUSY ? "  (Sony's UsbDeviceAudioPlayerService holds it — "
                                                 "this is contention, not RE)" : "");
                    clog_(m);
                    break;
                }
                // RATE FIRST, on the untouched handle: hw_params_any reports what the endpoint can
                // do, and once set_params has run it only reports back what we asked for.
                unsigned chans = 2;
                unsigned rate = ldac_capture_rate(pcm, &chans);
                if (!rate) {
                    rate = uac_host_rate_hint();
                    if (rate) {
                        std::snprintf(m, sizeof m, "ldac: ALSA would not name a single rate — using "
                                      "the service's freq (%u Hz)", rate);
                        clog_(m);
                    }
                }
                if (!rate) {
                    rate = 44100;
                    clog_("ldac: no rate from ALSA or the service — assuming 44100");
                }
                // Only stereo is carried. The gadget is stereo and the conversion buffer below is
                // sized for it; quietly reinterpreting some other channel count would come out as
                // noise, so fix it here and say so.
                if (chans != 2) {
                    std::snprintf(m, sizeof m, "ldac: capture reports %u channels — forcing stereo",
                                  chans);
                    clog_(m);
                    chans = 2;
                }
                if (!ldac_rate_supported(rate)) {
                    // Nothing in this path resamples: not the gadget, not us, not the transmitter,
                    // which negotiates one of 44100/48000/88200/96000 at connection setup. Carrying
                    // the audio anyway would mean playing it at the wrong pitch.
                    std::snprintf(m, sizeof m, "ldac: host is at %u Hz and the transmitter only "
                                  "negotiates 44100/48000/88200/96000 — cannot bridge this stream "
                                  "without resampling, so it is not being carried", rate);
                    clog_(m);
                    g_alsa.close(pcm);
                    break;
                }
                // soft_resample = 0. We do not want a rate conversion — the gadget runs at whatever
                // the host negotiated and the transmitter is told that same rate in the handshake —
                // and asking a bare hw: device for resampling only adds a way for this call to
                // half-succeed.
                rc = g_alsa.set_params(pcm, SND_PCM_FORMAT_S32_LE, SND_PCM_ACCESS_RW_INTERLEAVED,
                                       chans, rate, 0, 100000);
                if (rc < 0) {
                    std::snprintf(m, sizeof m, "ldac: set_params(%u ch, %u Hz) -> %s", chans, rate,
                                  alsa_err(rc));
                    clog_(m);
                    g_alsa.close(pcm);
                    break;
                }

                // Now — and only now — is there something true to tell the transmitter. A rate
                // change between sessions means the old handshake described a different stream, so
                // the connection is torn down and remade rather than reused.
                if (fd >= 0 && fd_rate != rate) {
                    std::snprintf(m, sizeof m, "ldac: host changed rate %u -> %u Hz — re-opening "
                                  "the transmitter stream", fd_rate, rate);
                    clog_(m);
                    close(fd);
                    fd = -1;
                }
                if (fd < 0) {
                    fd = ldac_connect_socket(rate, chans);
                    if (fd < 0) { g_alsa.close(pcm); break; }
                    fd_rate = rate;
                }
                ldac_dump_pcm(dev, "after set_params");
                ldac_log_state(pcm, "after set_params");

                // START EXPLICITLY. snd_pcm_set_params leaves start_threshold at 1, so the first
                // read is *supposed* to start the stream — but that is a library-side convention and
                // this is a gadget driver we have no source for. Starting by hand costs one syscall
                // and turns "did it ever start?" from a guess into a logged return value.
                if (g_alsa.start) {
                    int s = g_alsa.start(pcm);
                    std::snprintf(m, sizeof m, "ldac: snd_pcm_start -> %s",
                                  s == 0 ? "ok" : alsa_err(s));
                    clog_(m);
                    ldac_log_state(pcm, "after start");
                }

                std::snprintf(m, sizeof m, "ldac: streaming %u Hz %u ch", rate, chans);
                clog_(m);
                unsigned long long frames = 0;
                ldac_end why = ldac_pump(fd, pcm, dev, rate, &frames);
                g_alsa.close(pcm);
                total += frames;
                std::snprintf(m, sizeof m, "ldac: session ended after %llu frames (%llu s at %u Hz)",
                              frames, frames / rate, rate);
                clog_(m);

                if (why == LDAC_END_STOPPED) break;
                // A session that actually carried audio earns back the error budgets. Without this
                // the counters are cumulative over the life of the bridge, so a long listening
                // session with a few pauses in it would exhaust `reconnects` and give up on a
                // feature that had just been working for an hour.
                if (frames > 0) { reconnects = 0; reopens = 0; }
                if (why == LDAC_END_HOST_GONE && fd >= 0) {
                    // GIVE THE SOCKET BACK WHEN THE MUSIC STOPS. While our connection is the ExHal's
                    // PCM reader, its stream thread is blocked in Read on our fd and Sony's own
                    // WriteSilent() is NOT running — so a paused PC leaves the A2DP stream with no
                    // data at all rather than with silence, which is what the sink expects and what
                    // keeps the link from stalling. Closing hands the stream back; the next session
                    // re-handshakes, which is cheap and exactly what the probe did repeatedly.
                    clog_("ldac: host paused — handing the transmitter stream back");
                    close(fd);
                    fd = -1;
                    fd_rate = 0;
                }
                if (why == LDAC_END_SOCKET_GONE) {
                    // The transmitter dropped us. That is normal at the end of a stream, so try to
                    // get back in rather than ending the feature — bounded, because a service that
                    // refuses us repeatedly is telling us something and a reconnect spin would just
                    // bury it in the log.
                    close(fd);
                    if (++reconnects > 3) {
                        clog_("ldac: transmitter keeps closing the socket — giving up");
                        fd = -1;
                        break;
                    }
                    clog_("ldac: reconnecting to the transmitter");
                    usleep(300000);
                    fd = ldac_connect_socket(rate, chans);
                    if (fd < 0) break;
                    fd_rate = rate;
                    continue;
                }
                if (why == LDAC_END_PCM_DEAD && ++reopens > 8) {
                    clog_("ldac: too many failed reopens — giving up on this session");
                    break;
                }
                usleep(200000);
            }
            std::snprintf(m, sizeof m, "ldac: bridge carried %llu frames in total", total);
            clog_(m);
        }
        if (fd >= 0) close(fd);        // the reconnect path may already have closed and given up
    }

    clog_("ldac: bridge thread down");
    g_ldac_alive = 0;
    g_ldac_run = 0;
    return nullptr;
}

// Bring the bridge up. Returns immediately — everything slow happens on the thread.
// ── FM → BLUETOOTH ──────────────────────────────────────────────────────────────────────────
// The same bridge as USB-DAC → LDAC, with a different source. Everything on the transmit side is
// reused: ldac_connect_socket (which does the type-1 handshake), and ldac_pump.
//
// WHY THIS IS POSSIBLE AT ALL. FM audio looked un-bridgeable at first because no ALSA device opens
// while the tuner plays — the Si4708 is analogue into the codec. But the codec can digitise it:
// `analog input device` has a `tuner` item and `hw:0,1` is a real capture PCM. So the chain is
//
//     Si4708 --analogue--> CXD3778GF ADC --> hw:0,1 --> LDAC/SBC encode --> BtTransmitterService
//
// and the aerial and the output are INDEPENDENT: the cable only has to be plugged in, not listened
// to. Cable in the jack for reception, audio to the headphones over Bluetooth.
//
// NEVER write PCM to that socket before the handshake — it reboots the device, measured twice.
// ldac_connect_socket is the only thing that may open it, which is why this reuses it rather than
// opening its own.
//
// The USB-DAC bridge and this one share `g_ldac_run`/`g_ldac_alive` deliberately: they are the same
// resource (one BT PCM socket, one bridge thread) and `g_ldac_alive` already refuses a second
// thread, so the two modes cannot fight over it.
static void* fmbt_thread(void*) {
    g_ldac_alive = 1;
    clog_("fm-bt: bridge thread up (source hw:0,1, the codec ADC)");

    // The ADC's own format. Fixed, unlike the UAC bridge — there is no host to negotiate with.
    const unsigned RATE = 44100, CHANS = 2;
    snd_pcm_t* pcm = nullptr;
    if (!alsa_load()) { clog_("fm-bt: libasound unavailable"); g_ldac_alive = 0; g_ldac_run = 0; return nullptr; }
    int rc = g_alsa.open(&pcm, "hw:0,1", /*CAPTURE*/1, 0);
    if (rc < 0) {
        char m[160];
        std::snprintf(m, sizeof m, "fm-bt: cannot open hw:0,1: rc=%d%s", rc,
                      rc == -EBUSY ? " (AudioInPlayerService still holds it — stop FM audio first)"
                                   : "");
        clog_(m);
        g_ldac_alive = 0; g_ldac_run = 0;
        return nullptr;
    }
    rc = g_alsa.set_params(pcm, /*S16_LE*/2, /*RW_INTERLEAVED*/3, CHANS, RATE, 1, 200000);
    if (rc < 0) {
        char m[128];
        std::snprintf(m, sizeof m, "fm-bt: set_params failed: rc=%d", rc);
        clog_(m);
        g_alsa.close(pcm);
        g_ldac_alive = 0; g_ldac_run = 0;
        return nullptr;
    }
    g_alsa.start(pcm);   // this capture does not start on its own

    int fd = ldac_connect_socket(RATE, CHANS);
    if (fd < 0) {
        clog_("fm-bt: no transmitter socket — is a device connected?");
        g_alsa.close(pcm);
        g_ldac_alive = 0; g_ldac_run = 0;
        return nullptr;
    }
    unsigned long long frames = 0;
    ldac_end why = ldac_pump(fd, pcm, "hw:0,1", RATE, &frames);
    char m[192];
    std::snprintf(m, sizeof m, "fm-bt: pump ended (%d) after %llu frames", (int)why, frames);
    clog_(m);
    ::close(fd);
    g_alsa.close(pcm);
    g_ldac_alive = 0;
    g_ldac_run = 0;
    return nullptr;
}

// Start the radio-to-Bluetooth bridge. The tuner must already be running; the FM AUDIO path
// (AudioInPlayerService) must NOT be, because it owns hw:0,1 and this needs it.
static void fmbt_start() {
    if (g_ldac_alive) { clog_("fm-bt: a bridge is already running"); return; }
    g_ldac_run = 1;
    pthread_attr_t at;
    pthread_attr_init(&at);
    pthread_attr_setdetachstate(&at, PTHREAD_CREATE_DETACHED);
    pthread_t th;
    if (pthread_create(&th, &at, fmbt_thread, nullptr) != 0) {
        g_ldac_run = 0;
        clog_("fm-bt: pthread_create FAILED");
    }
    pthread_attr_destroy(&at);
}

static void ldac_start() {
    // Disabled between 2026-08-11 and the same evening, on the belief that this socket was a control
    // channel that could never carry audio. Half of that was right: it *starts* as one, and PCM
    // written before the handshake reboots the device. The other half was wrong — a type-1 frame
    // hands the connection to the ExHal as its PCM reader. See ldac_handshake() for the recovered
    // code path and the on-device measurement. The rule that survives is: never write a sample to
    // this fd that ldac_handshake() has not first cleared.
    if (g_ldac_alive) { clog_("ldac: already running"); return; }
    // Resolve libasound BEFORE touching the control plane. If the capture side can't work there is no
    // point declaring a source to the transmitter and then having to walk it back.
    if (!alsa_load()) return;
    // Control plane on THIS thread: it is the one with the live framework, and it is three quick IPC
    // calls. The codec choice comes from Settings via apply_bt_codec, which also runs SetLdac — this
    // adds only the source declaration, which is what makes the service open its socket.
    enum { VIDX_SetCurrentSource = 12 };
    apply_bt_codec();
    try {
        void* x = bt_xmit();
        if (x) {
            bool t = true;
            typedef int (*fnb)(void*, const bool*);
            ((fnb)bt_slot(x, VIDX_SetCurrentSource))(x, &t);
            clog_("ldac: SetCurrentSource(true)");
        }
    } catch (...) { clog_("ldac: SetCurrentSource threw"); return; }

    g_ldac_run = 1;
    pthread_t th;
    pthread_attr_t at;
    pthread_attr_init(&at);
    pthread_attr_setdetachstate(&at, PTHREAD_CREATE_DETACHED);   // fire and forget; it self-reaps
    pthread_attr_setstacksize(&at, 256 * 1024);
    if (pthread_create(&th, &at, ldac_thread, nullptr) != 0) {
        g_ldac_run = 0;
        clog_("ldac: pthread_create FAILED");
    }
    pthread_attr_destroy(&at);
}

// Ask the bridge to stop. Deliberately does NOT join: this runs on the render thread under a
// watchdog, and the thread can be parked in a blocking ALSA read. Clearing the flag makes it unwind
// on its own within one buffer period (~12 ms), and g_ldac_alive keeps a restart from racing it.
static void ldac_stop() {
    if (!g_ldac_alive) return;
    g_ldac_run = 0;
    clog_("ldac: stop requested");
    enum { VIDX_SetCurrentSource = 12 };
    try {
        void* x = bt_xmit();
        if (x) {
            bool f = false;
            typedef int (*fnb)(void*, const bool*);
            ((fnb)bt_slot(x, VIDX_SetCurrentSource))(x, &f);
        }
    } catch (...) { clog_("ldac: SetCurrentSource(false) threw"); }
}

void apply_usb_dac() {
    bool on = cinder_get_usb_dac() != 0;

    // RELEASE THE RENDERER BEFORE ASKING FOR THE DAC.
    //
    // This is why DAC mode produced a sound card on the PC and silence at the jack, while local
    // playback carried on regardless. `RendererDmpMaster` keeps a mode field at +0x118 —
    // IsUsbDacMode() is `field == 1`, IsA2dpSnkMode() is `field == 2` — and it only enters DAC mode
    // when a UAC track takes the renderer. It cannot, because our own playback still holds
    // SoundService's single "Music" track; libSoundServiceFw's own log line is "Cannot create
    // multiple tracks that have same type". So Start() was accepted and returned an all-zero
    // stream_info: no stream was ever established.
    //
    // Pause is NOT enough (same lesson as USB-MSC, one layer down): a paused PlayerService still
    // owns the track. Stop + drop the pinned sequence, then ClosePlayer, which is documented to
    // release "SoundService's single Music track" — that is the thing standing in the way.
    //
    // Called directly, not via a nested run_guarded: this already runs under carry_out's guard,
    // and the guard's jmp buffer does not nest.
    if (on) {
        set_transport(false);
        cinder_audio_release_sequence();
        int cp = cinder_audio_close_player();
        char cm[96];
        std::snprintf(cm, sizeof cm, "usb-dac: released Music track for the DAC (ClosePlayer rc=%d)",
                      cp);
        clog_(cm);
    }

    // TELL THE OTHER OWNER FIRST. The gadget has two independent writers — init (via the property,
    // which is what cinder-msc drives) and UsbMgrServiceFw — and ORDER DECIDES WHO WINS, because
    // each one rewrites `functions` wholesale.
    //
    // The service goes first on purpose. It ends up believing the right function, so its own later
    // triggers (cable insert, Resume, OnDeviceConnectedChanged) compare equal and change nothing —
    // which is the revert that made a UAC switch look like it worked and then silently undo itself.
    // Meanwhile init's write lands LAST, so the live descriptor is init's `audio_func,adb` rather
    // than the service's audio-only string, and the adb interface survives DAC mode.
    //
    // Doing it the other way round (property first, service second) leaves the service's string
    // live and takes adb down with it — correct for the radio, useless for anyone debugging.
    // FUNCMODE FIRST, AND BEFORE THE PROPERTY WRITE. EnterFuncMode is the supported entry point and
    // a superset of the line below — it calls SetUsbFunction itself, then SetDeviceHandleRules
    // (which is what publishes connmgr device 7) and SetPath (the audio route). It has to run
    // before init's property write for the same reason SetUsbFunction does: its inner
    // SetUsbFunction installs the service's audio-only descriptor string, and letting init's
    // `audio_func,adb` land afterwards is what keeps adb alive through DAC mode.
    //
    // PROVEN ON HARDWARE 2026-08-11 (`cinder-probe --funcmode 1 120 45`), which is why this is no
    // longer behind a marker file. One run, every link of the chain moved together:
    //
    //     FuncMode          0 MediaPlay        -> 1 UsbDac
    //     connmgr device 7  enabled=0 conn=0   -> enabled=1 connected=1
    //     gadget            mass_storage,adb   -> audio_func, idProduct 0ca0 -> 0b8c
    //     netlink proto 24  ENOPROTOOPT        -> bound
    //     /proc/asound      card0 only         -> card4 pcm0c present  (hw:4,0)
    //
    // held stable across a 45 s window, and EnterFuncMode(MediaPlay) put all five back. `hw:4,0` is
    // the device UsbAudioPlayerInhal hardcodes and it does not exist in any other mode.
    //
    // The run recorded zero netlink FORMAT events, which is the environment and not the chain: the
    // player was attached to WSL through usbipd, so Windows never enumerated it as a sound card and
    // nothing was ever streaming. A real host is needed to close that last link.
    //
    // IGNORE THE RETURN VALUE. EnterFuncMode returned false for BOTH the switch and the restore on
    // the run where all five readings above confirm it worked — the same trap as SetDeviceType's
    // rc. usb_enter_func_mode logs it, and nothing branches on it.
    run_guarded("usb-dac: EnterFuncMode", 20, on ? usb_fm_usbdac : usb_fm_mediaplay);

    run_guarded("usb-dac: SetUsbFunction", 6, on ? usb_fn_uac : usb_fn_msc);

    int rc = std::system(on ? "/system/vendor/unknown321/bin/cinder-msc dac-on"
                            : "/system/vendor/unknown321/bin/cinder-msc dac-off");
    char m[128];
    std::snprintf(m, sizeof m, "usb-dac: %s -> cinder-msc dac-%s rc=%d%s",
                  on ? "engage" : "release", on ? "on" : "off", rc,
                  rc == 0 ? "" : "  (FAILED — is the helper setuid root?)");
    clog_(m);

    // AND THE OWNER THAT ACTUALLY COMPLETES IT, LAST. The two writers above only move descriptor
    // bytes. This one re-enumerates the gadget in a way UsbDeviceConnectionMonitor is watching, so
    // it fires NotifyUacConnectEvnet, which is the event half of what device 7 needs.
    //
    // CORRECTION (2026-08-11): the event half was never the whole story, which is why this call did
    // nothing when it was first tried. ConnGlueUsbHost::CnvStatus ANDs the connect event with
    // FuncMode == 1 before device 7 reports connected — see usb_enter_func_mode above. A connect
    // event delivered while FuncMode is still MediaPlay is discarded, so SetDeviceType alone can
    // never open the audio path no matter how cleanly the gadget re-enumerates.
    //
    // Off goes to Adb, not Msc: Msc unmounts /contents, which would take the music library out from
    // under the player. Mass storage is a separate, explicit user action (see usb_set_device_type).
    // OPT-IN ONLY, AND DELIBERATELY OFF BY DEFAULT (2026-08-11).
    //
    // SetDeviceType is the right call on paper — it is the only owner that does the whole job — but
    // the first on-device run of it did two things at once: the gadget did NOT change (functions
    // stayed mass_storage,adb through a 30 s window of 5 s samples, device 7 stayed disabled, and
    // netlink proto 24 never appeared), and a couple of minutes later the DEVICE REBOOTED. The
    // reboot recovered onto Cinder by itself, but a Home-app toggle that can reboot the player is
    // strictly worse than one that does nothing, so this stays behind a marker file until the
    // no-op-plus-reboot is understood. Use `cinder-probe --usbdt` for that; it is opt-in there too.
    //
    //     adb shell 'touch /contents/cinder-usbdt.on'     # to try it
    if (::access("/contents/cinder-usbdt.on", F_OK) == 0)
        run_guarded("usb-dac: SetDeviceType", 8, on ? usb_dt_uac : usb_dt_adb);

    // AND CHECK THE GADGET IS STILL ON THE BUS. Every writer does `enable 0 -> ids -> functions ->
    // enable 1`, so one that dies or blocks in the middle leaves enable=0 — no adb, no MSC, no UAC,
    // nothing visible to the host at all. That happened on 2026-08-11 and cost a power-cycle. The
    // rescue is a no-op when enable is already 1, and it deliberately goes through init's property
    // rather than the service that just wedged.
    (void)std::system("/system/vendor/unknown321/bin/cinder-msc usb-rescue");
    // WHERE THE DAC AUDIO GOES. Only ever decided when the gadget actually came up — arming any of
    // this for a UAC mode that never engaged is how the last round of this looked like it worked.
    //
    // Two destinations, and they are mutually exclusive because a capture PCM substream is exclusive:
    // either Sony's UsbDeviceAudioPlayerService owns the gadget's capture and renders it to the
    // 3.5 mm codec, or OUR bridge owns it and re-encodes to LDAC. Running both means one of them gets
    // -EBUSY. So the route is chosen by where the audio should be heard: headphones connected ->
    // LDAC, otherwise the jack.

    if (rc == 0) {
        if (on) {
            if (cinder_get_bt_route()) {
                // Headphones connected: bridge the capture to them. The bridge owns the capture PCM
                // for the whole session, so Sony's own renderer must NOT be started as well — two
                // readers on one gadget capture is the -EBUSY case, and it is the bridge that loses.
                clog_("usb-dac: headphones connected — bridging the capture to LDAC");
                g_uac_playing = false;
                g_uac_ref_dumped = false;
                g_uac_route = 1;
                uac_listener_start();
                ldac_start();
            } else {
                // Subscribe FIRST, then try once. The single Start() here only succeeds if a host
                // is already streaming; the listener is what covers the normal case, where the PC
                // starts playing seconds or minutes later. Before this, that case never opened.
                //
                // uac_listener_start() builds the client itself — it used to bail on a null one,
                // and since it runs BEFORE the first uac_render() that meant it registered nothing
                // at all, silently. The log said "SetUsbFunction ok, Start() kFormatNone" and then
                // went quiet forever, which read like a service problem and was ours.
                g_uac_playing = false;
                g_uac_ref_dumped = false;
                g_uac_route = 0;
                uac_listener_start();
                if (!uac_render(true)) {
                    clog_("usb-dac: gadget is up but Start() failed — expect silence at the jack");
                }
            }
        } else {
            ldac_stop();
            uac_render(false);
            g_uac_route = -1;
            g_uac_want_jack = false;
        }
    } else if (!on) {
        ldac_stop();
    }

    // Leaving DAC mode: reclaim the player we closed on the way in. Without this the device is
    // silent AFTER a DAC session too — we released the Music track and nothing ever took it back,
    // which would turn one broken feature into two. Playback itself resumes on the next
    // play_tracks; this only re-establishes the controller + listener.
    if (!on) {
        int ri = cinder_audio_init("cinder");
        char rm[96];
        std::snprintf(rm, sizeof rm, "usb-dac: reclaimed the player after DAC (init rc=%d)%s",
                      ri, ri == 0 ? "" : "  (local playback may need a restart)");
        clog_(rm);
    }
}

// ── USB mass storage (hand /contents to the PC) ──────────────────────────────────────────────
// Stock init's `sys.sony.config=msc` runs `unmount_msc1` (= umount /contents) BEFORE pointing the
// gadget LUN at the partition — and umount fails EBUSY if anything holds an fd there. OUR
// stdout/stderr ARE /contents/cinderhome.log (the launcher's redirect), so entering MSC without
// moving them silently breaks mass storage (this was the "mass storage bugs it" failure). So:
// move fds 1+2 to /dev/null first (mirrors cinder-device's redirect_fds), flip the mode, and on
// exit flip back to `adb` (the stock boot default — its init block runs `mount_msc1` to remount),
// wait for the remount, then point the log back at /contents/cinderhome.log.
static bool g_msc_active = false;   // between enter and exit (gates /contents writers + watcher)
static bool g_msc_seen_usb = false; // saw the cable while in MSC → unplug ends the session
// THE USER SAID NO. Set when mass storage is left by hand (the Turn Off button, or Back) while the
// PC is still plugged in; cleared when the data host goes away.
//
// Without it the exit is a LOOP, and it was: leaving MSC clears g_msc_active, the very next
// watcher tick sees usb_data_host() still true, and two ticks later auto-MSC hands the volume
// straight back to the PC. Reported 2026-08-17 as "usb mass storage mode can't be turned off when
// the usb is in, the screen keeps coming up" — the modal was not reappearing, it was being
// re-entered, once every couple of seconds, forever.
//
// Latched rather than debounced on purpose: no timeout makes "off" mean off, and the one event
// that can plausibly mean the user changed their mind is unplugging and replugging the cable.
//
// RELEASING IT IS THE SUBTLE HALF, and the first attempt got it wrong (reported 2026-08-17: "the
// msc fix didn't stay"). The obvious release condition — "the data host went away" — is WRONG,
// because leaving mass storage switches the gadget from `mass_storage` back to `adb`, and that
// RE-ENUMERATES: `android0/state` leaves CONFIGURED for a moment, so `usb_data_host()` reads false
// mid-exit, the latch cleared, enumeration finished, and auto-MSC walked straight back in. The
// symptom was identical to having no fix at all.
//
// So the release is keyed on the CABLE, via the broad `usb_connected()` probe (which also reads
// the power-supply nodes and therefore stays true across a gadget re-bind), and it is debounced:
// the cable has to be gone for several consecutive ticks. Both halves matter — the broad probe
// survives the re-enumeration, the debounce survives anything that makes it flicker.
static bool g_msc_user_off = false;
static int  g_msc_off_gone = 0;      // consecutive ticks with no cable at all
static const int MSC_OFF_RELEASE_TICKS = 3;
static int  g_usb_hi = 0;           // debounce: consecutive host-present samples while NOT in MSC

// Point stdout/stderr at `path`. Returns false only if even /dev/null could not be opened.
//
// THE FALLBACK IS THE POINT. Entering USB-MSC redirects the log OFF /contents precisely because an
// open fd under /contents makes init's `umount /contents` fail EBUSY — the LUN write then fails and
// the PC sees a reader with no medium. The old version silently did nothing when the open failed,
// leaving fds 1 and 2 still on /contents/cinderhome.log: mass storage would break, and the reason
// could not appear in any log, because the log was the thing breaking it. That is not theoretical —
// a whole MSC debugging session was blinded this way when /tmp/cinder_msc.log turned out never to
// have been created.
//
// So: on failure, fall back to /dev/null. Losing the log is a bad outcome; silently failing to
// release /contents is a worse one. The failure is reported BEFORE the switch, while stderr still
// points at the old destination, so the explanation survives in the previous log.
bool redirect_fds(const char* path, int flags) {
    std::fflush(stdout); std::fflush(stderr);
    int fd = open(path, flags, 0644);
    if (fd < 0) {
        std::fprintf(stderr, "[cinder-home] redirect_fds: cannot open %s (errno=%d) — falling back "
                             "to /dev/null so /contents is released\n", path, errno);
        std::fflush(stderr);
        fd = open("/dev/null", O_WRONLY);
        if (fd < 0) {
            std::fprintf(stderr, "[cinder-home] redirect_fds: /dev/null failed too (errno=%d) — "
                                 "fds still on the old target; umount may fail EBUSY\n", errno);
            std::fflush(stderr);
            return false;
        }
    }
    dup2(fd, 1); dup2(fd, 2);
    if (fd > 2) close(fd);
    return true;
}
bool contents_mounted() {
    FILE* f = std::fopen("/proc/mounts", "r");
    if (!f) return false;
    char line[512]; bool found = false;
    while (std::fgets(line, sizeof line, f))
        if (std::strstr(line, " /contents ")) { found = true; break; }
    std::fclose(f);
    return found;
}
// Same probe as the launcher's usb_connected(): gadget state / power-supply online.
bool usb_connected() {
    static const char* paths[] = { "/sys/class/android_usb/android0/state",
                                   "/sys/class/power_supply/usb/online",
                                   "/sys/class/power_supply/usb/present" };
    for (const char* p : paths) {
        FILE* f = std::fopen(p, "r");
        if (!f) continue;
        char buf[64] = {};
        (void)!std::fread(buf, 1, sizeof buf - 1, f);
        std::fclose(f);
        if (std::strstr(buf, "CONFIGURED") || buf[0] == '1') return true;
    }
    return false;
}
// STRICT probe: is a real USB DATA HOST (a PC) attached? Only the gadget's own enumeration state
// says that. `usb_connected()` above is deliberately broad — it answers "is a cable in?", which is
// what the launcher's recovery escape wants (fail toward recovery) — but it returns true for a
// plain wall charger too, because power_supply/usb/{online,present} read 1 on any 5 V source.
//
// Using the broad probe to decide auto-MSC was a real bug: on the stable channel, plugging the
// device into a CHARGER would, after the ~2 s debounce, unmount /contents and hand it over as mass
// storage — the library would vanish mid-charge and the "connected to PC" modal would take over the
// screen. It never showed up in testing because the dev channel disables auto-MSC by default
// (dev_skip_auto_msc), which is exactly the channel this has been developed on.
//
// Unreadable node => false: no auto-MSC. That fails toward "nothing happens" (Settings ▸ USB mode
// still enters MSC by hand), never toward yanking the filesystem away on a charger.
bool usb_data_host() {
    FILE* f = std::fopen("/sys/class/android_usb/android0/state", "r");
    if (!f) return false;
    char buf[64] = {};
    (void)!std::fread(buf, 1, sizeof buf - 1, f);
    std::fclose(f);
    return std::strstr(buf, "CONFIGURED") != nullptr;
}

// While /contents is away we log to tmpfs; the file is spliced back into cinderhome.log on exit,
// so the whole MSC session (including failures) is visible afterwards.
static const char* MSC_TMP = "/tmp/cinder_msc.log";

// Log every process holding an fd under /contents — the umount-EBUSY culprits. Pure /proc walk,
// no shell. Output goes to whatever stderr currently is (the tmpfs log during an MSC attempt).
void log_contents_holders() {
    DIR* pd = opendir("/proc");
    if (!pd) return;
    while (dirent* pe = readdir(pd)) {
        if (pe->d_name[0] < '0' || pe->d_name[0] > '9') continue;
        char fdp[64]; std::snprintf(fdp, sizeof fdp, "/proc/%s/fd", pe->d_name);
        DIR* fdd = opendir(fdp);
        if (!fdd) continue;
        bool named = false;
        while (dirent* fe = readdir(fdd)) {
            char lp[128], tgt[256];
            std::snprintf(lp, sizeof lp, "%s/%s", fdp, fe->d_name);
            ssize_t n = readlink(lp, tgt, sizeof tgt - 1);
            if (n <= 0) continue;
            tgt[n] = 0;
            if (std::strncmp(tgt, "/contents", 9) != 0) continue;
            if (!named) {
                char cp[64], comm[64] = {};
                std::snprintf(cp, sizeof cp, "/proc/%s/comm", pe->d_name);
                if (FILE* cf = std::fopen(cp, "r")) {
                    if (std::fgets(comm, sizeof comm, cf)) comm[std::strcspn(comm, "\n")] = 0;
                    std::fclose(cf);
                }
                std::fprintf(stderr, "[cinder-home]   holder pid %s (%s):\n", pe->d_name, comm);
                named = true;
            }
            std::fprintf(stderr, "[cinder-home]     fd -> %s\n", tgt);
        }
        closedir(fdd);
    }
    closedir(pd);
    std::fflush(stderr);
}

// Belt-and-braces after /contents is unmounted: guarantee the gadget's mass-storage LUN actually
// backs /emmc@contents. Stock init's msc trigger normally points it, BUT the dev-channel adb
// compose (or an enumeration race) can leave the LUN file empty — which makes the PC enumerate a
// reader with NO MEDIUM (the "modal shows but no drive appears" symptom). Only acts when the LUN
// is empty, so it's a safe no-op on the paths that already pointed it. Bounces the gadget so the
// host re-enumerates and picks up the freshly-backed disk.
// Read a system property and compare it. popen rather than __system_property_get: this is a glibc
// process, not bionic, so the property API is not linkable here — the shell's getprop is.
static bool prop_equals(const char* name, const char* want) {
    char cmd[160];
    std::snprintf(cmd, sizeof cmd, "getprop %s 2>/dev/null", name);
    FILE* p = popen(cmd, "r");
    if (!p) return false;
    char buf[160] = {0};
    bool got = std::fgets(buf, sizeof buf, p) != nullptr;
    pclose(p);
    if (!got) return false;
    buf[std::strcspn(buf, "\r\n")] = 0;
    return std::strcmp(buf, want) == 0;
}

static void ensure_msc_lun() {
    const char* lunf = "/sys/class/android_usb/android0/f_mass_storage/lun/file";
    char cur[128] = {};
    if (FILE* f = std::fopen(lunf, "r")) {
        if (std::fgets(cur, sizeof cur, f)) cur[std::strcspn(cur, "\n")] = 0;
        std::fclose(f);
    }
    const char* s = cur;
    while (*s == ' ' || *s == '\t') ++s;
    if (*s != 0) return; // already backed — nothing to do
    // The gadget is ALREADY enabled with functions=mass_storage,adb (the sys.sony.config=msc
    // trigger set that). The mass_storage LUN is REMOVABLE, so writing its backing file is a
    // media-INSERT event: the host sees the disk appear with NO re-enumeration. On-device RE
    // (adb, 2026-07-25) proved a bare `echo /emmc@contents > lun/file` — with /contents already
    // unmounted — makes the PC enumerate the full 55.9 GB WALKMAN drive (host: sdf 55.9G vfat).
    // Do NOT enable-cycle here: `enable 0`→`enable 1` re-creates the mass_storage instance and
    // CLEARS lun/file back to empty — THAT was the "modal shows but the LUN stays empty / PC sees
    // a 0-byte reader with NO MEDIUM" bug this function was meant to cure. Write + confirm instead;
    // retry a few times in case a holder is mid-close (EBUSY leaves the write silently empty).
    for (int i = 0; i < 8; ++i) {
        std::system("echo /emmc@contents > "
                    "/sys/class/android_usb/android0/f_mass_storage/lun/file 2>/dev/null");
        char rb[128] = {};
        if (FILE* f = std::fopen(lunf, "r")) {
            if (std::fgets(rb, sizeof rb, f)) rb[std::strcspn(rb, "\n")] = 0;
            std::fclose(f);
        }
        const char* t = rb;
        while (*t == ' ' || *t == '\t') ++t;
        if (*t != 0) {
            // Read back again after a settle. An immediate readback only proves the write reached
            // the node, not that it SURVIVED — anything still reconfiguring the gadget behind us
            // clears it, and reporting success on a LUN that is about to be wiped is worse than
            // reporting nothing, because it sends the next reader looking in the wrong place.
            usleep(400000);
            char rb2[128] = {};
            if (FILE* f2 = std::fopen(lunf, "r")) {
                if (std::fgets(rb2, sizeof rb2, f2)) rb2[std::strcspn(rb2, "\n")] = 0;
                std::fclose(f2);
            }
            const char* t2 = rb2;
            while (*t2 == ' ' || *t2 == '\t') ++t2;
            if (*t2 == 0) {
                clog_("usb-msc: LUN was cleared again right after binding — retrying");
                continue;
            }
            clog_("usb-msc: LUN was empty — bound /emmc@contents (host medium inserted, no re-enum)");
            return;
        }
        usleep(250000); // holder still dropping — retry the media-insert
    }
    clog_("usb-msc: LUN STILL empty after retries — host will see a reader with NO medium");
}

// Forward decl: the MSC entry needs to dismiss its own modal if the handoff is refused, and
// carry_out is defined below (it dispatches every navigator action, including this one).
void carry_out(int act);

void enter_usb_msc() {
    clog_("usb-msc: entering (session log -> /tmp/cinder_msc.log, spliced back on exit)");
    // 1) release OUR storage users. Pause is NOT enough: a paused PlayerService keeps the
    //    current track's file open under /contents, which alone makes unmount_msc1 fail EBUSY.
    //    Stop + drop the pinned sequence so the service closes the media file. Called directly
    //    (NOT via a nested run_guarded — the guard's jmp buffer doesn't nest): this whole
    //    function already runs under carry_out's "enter USB MSC" guard, which covers the IPC.
    set_transport(false);
    cinder_audio_release_sequence();
    (void)chdir("/");
    // 2) move our log fds (1+2 ARE /contents/cinderhome.log via the launcher redirect). A failure
    //    here falls back to /dev/null rather than leaving them on /contents — see redirect_fds; an
    //    fd there is exactly what makes the umount below fail EBUSY.
    if (!redirect_fds(MSC_TMP, O_WRONLY | O_CREAT | O_APPEND))
        clog_("usb-msc: log fds could NOT be moved off /contents — umount will likely fail EBUSY");
    // 3) UNMOUNT /contents FIRST, via the setuid-root helper, THEN flip the gadget. On-device RE
    //    (adb, 2026-07-25) found TWO things: (a) the stock `sys.sony.config=msc` trigger is RACY —
    //    it `start unmount_msc1` (an async fork of `umount /contents`) then IMMEDIATELY writes
    //    lun/file, so the gadget often binds a STILL-MOUNTED block device and the LUN comes up EMPTY
    //    (PC sees a 0-byte reader with NO MEDIUM); and (b) cinder-home runs as uid 100 with an EMPTY
    //    capability set (appmgr strips them), so it CANNOT umount(2) itself (EPERM) — that's why the
    //    earlier in-process umount always failed. Fix: cinder-umount (chmod 4755, owner root) regains
    //    caps on exec and unmounts (verified: a uid-100 caller unmounts /contents rc 0). With
    //    /contents already gone, the trigger's lun bind lands on a FREE device. Retry for a holder.
    // 3) EVERYTHING PRIVILEGED HAPPENS IN cinder-msc, AS ROOT, IN ONE GO.
    //
    // This used to be a sequence of unprivileged steps here, and MEASURED on device 2026-07-28,
    // BOTH of the ones that matter are root-only — cinder-home is uid `system` with an empty
    // capability set, so neither has ever worked:
    //
    //   * Writing the LUN backing file makes the KERNEL open the block device in OUR credentials.
    //     /dev/block/mmcblk0p29 is `brw------- root root`, so it is EACCES and the sysfs write
    //     silently fails. The sysfs node is 0666 system:system, so it LOOKS writable and `echo`
    //     returns 0 regardless. Hence "LUN STILL empty after retries", eighteen times, and a host
    //     that saw a reader with no medium.
    //   * `setprop sys.sony.config msc` is refused for uid system, so the property never left
    //     "adb" and init's msc block never ran at all. The old "init never reported
    //     sys.usb.state=mass_storage,adb" line was reporting precisely that, and it was read as a
    //     timeout to wait out rather than a refusal.
    //
    // Every previous fix here (trigger ordering, the enable-cycle, the exit remount) was aimed
    // downstream of that and could not have worked. As root it binds first time.
    int rc = std::system("/system/vendor/unknown321/bin/cinder-msc on");
    if (rc != 0) {
        char m[128];
        std::snprintf(m, sizeof m, "usb-msc: cinder-msc on FAILED rc=%d — not entering", rc);
        clog_(m);
        // NAME THE CULPRIT. The overwhelmingly likely cause is that something still holds an fd
        // under /contents, and rc alone cannot say which process. Our own fds are already off
        // /contents at this point (redirect_fds above), so anything this prints is a genuine third
        // party rather than us. stderr is still the tmpfs log, which gets spliced back below, so
        // the diagnosis survives into cinderhome.log.
        clog_("usb-msc: processes still holding /contents:");
        log_contents_holders();
        // The helper unmounts nothing it cannot hand over, so a failure leaves /contents mounted
        // and the gadget untouched. Put the log back and stay out of MSC rather than sitting in a
        // modal over a handoff that did not happen.
        redirect_fds("/contents/cinderhome.log", O_WRONLY | O_CREAT | O_APPEND);
        std::system("cat /tmp/cinder_msc.log 2>/dev/null; rm -f /tmp/cinder_msc.log 2>/dev/null");
        int back = cinder_input(CINDER_BTN_BACK);
        if (back != CINDER_ACT_NONE && back != CINDER_ACT_EXIT_USB_MSC) carry_out(back);
        return;
    }
    clog_("usb-msc: handed over (cinder-msc on)");
    g_msc_active = true;
    g_msc_seen_usb = false;
}
void exit_usb_msc() {
    // Mirror of the entry: releasing the LUN and remounting /contents are both root-only, and the
    // helper does them in the one order that is safe — media released BEFORE the remount, so the
    // host and the kernel never hold the same vfat at once. It also falls back to mounting
    // /contents itself, because init's mount_msc1 is `oneshot` and will not re-run within a boot.
    int rc = std::system("/system/vendor/unknown321/bin/cinder-msc off");
    if (rc != 0) {
        char m[128];
        std::snprintf(m, sizeof m, "usb-msc: cinder-msc off rc=%d", rc);
        clog_(m);
    }
    redirect_fds("/contents/cinderhome.log", O_WRONLY | O_CREAT | O_APPEND);
    g_msc_active = false;
    // splice the away-session log back in (cat writes to fd 1 = cinderhome.log again)
    std::system("cat /tmp/cinder_msc.log 2>/dev/null; rm -f /tmp/cinder_msc.log 2>/dev/null");
    // The PC may have rewritten anything under /contents while it held the volume, so any config
    // we cache from there is now suspect. Only cinder_viz.conf is cached; drop it.
    viz_conf_invalidate();
    clog_(contents_mounted() ? "usb-msc: exited (/contents remounted; log restored)"
                             : "usb-msc: exited but /contents did NOT remount within 5 s");
}

// Drain Cinder's pending URI sequence into PlayerService. Queue edits use the same proven
// NodeTrackSequence path as a normal play action, but restore the current position afterwards so
// replacing the sequence does not turn a "play next" gesture into a restart.
static void play_pending_sequence(const char* label, bool restore_position) {
    int n = cinder_pending_play_count();
    if (n <= 0) return;
    if (n > 512) n = 512;
    static char bufs[512][512];
    static const char* ptrs[512];
    int start = cinder_pending_play_start();
    int kept = 0;
    for (int i = 0; i < n; ++i) {
        int len = cinder_pending_play_uri(i, bufs[kept], sizeof bufs[kept]);
        if (len > 0 && len < (int)sizeof bufs[kept]) {
            ptrs[kept] = bufs[kept];
            ++kept;
        } else {
            if (len >= (int)sizeof bufs[kept])
                std::fprintf(stderr, "[cinder] %s: URI %d truncated (%d B) — skipped\n", label, i, len);
            if (i < start) --start;
        }
    }
    if (kept == 0) return;
    if (start < 0 || start >= kept) start = 0;
    int resume_ms = restore_position ? cinder_play_position_ms() : 0;
    int rc = cinder_audio_play_tracks(ptrs, kept, start);
    if (rc != 0) {
        std::fprintf(stderr, "[cinder] %s: play_tracks(%d tracks, start %d) failed rc=%d\n",
                     label, kept, start, rc);
        return;
    }
    if (restore_position && resume_ms > 0) {
        cinder_audio_seek_ms(resume_ms);
        cinder_notify_seek_ms(resume_ms);
    }
    set_transport(true);
    g_np_poll_now = true;
    g_house_due = true;
}


// ── FM radio ────────────────────────────────────────────────────────────────────────────────
// The tuner shim (cinder-audio/src/tuner_shim.cpp) owns the two Sony services and the ADC route;
// this is only the glue that keeps the UI in step and paints while the slow parts run.
//
// SEEK AND SCAN ARE SLOW ON PURPOSE. Sony's own auto-tune finds nothing on this hardware and
// GetSignalLevel reads 1 at every frequency, so a station can only be found by MEASURING the
// audio — about 0.14 s per 100 kHz step for a seek, 0.45 s for a scan. Both paint every step so
// the dial visibly sweeps rather than freezing.
static bool g_fm_on = false;

// Called from inside the shim for every frequency a seek passes through.
static void fm_on_step(int khz) {
    cinder_fm_report_khz(khz);
    if (g_screen_on) cinder_render_tick();      // the sweep has to be VISIBLE
}
static void fm_on_progress(int pct) {
    cinder_fm_report_scan_progress(pct);
    if (g_screen_on) cinder_render_tick();
}

static void fm_power_fn() {
    const bool want = cinder_fm_playing() != 0;
    if (want) {
        // The aerial is the headphone cable. Report it so the screen can say "no aerial" instead
        // of looking broken when the jack is empty — they are indistinguishable by ear.
        FILE* af = std::fopen("/sys/class/switch/cxd3778gf_antenna/state", "r");
        int ant = 0;
        if (af) { if (std::fscanf(af, "%d", &ant) != 1) ant = 0; std::fclose(af); }
        cinder_fm_report_antenna(ant);
        // RELEASE THE MUSIC TRACK, do not merely pause it.
        //
        // SoundServiceFw allows only ONE track of a given type ("Cannot create multiple tracks that
        // have same type"), and AudioInPlayerService creates one to carry the radio. Pausing leaves
        // the player's track alive — cinder_audio.h says so outright: after play_tracks the graph
        // sits in OMX_StatePause "with the SoundService track created but silent". So a paused
        // player still owns the slot, AudioIn::Play() returns 1, and FM is silent with every other
        // call reporting success. Measured 2026-08-18: `AudioIn rc=1 state=0` with the tuner in
        // state 2 and the ADC routed.
        //
        // close_player releases it. Playback comes back through the normal play path, which issues
        // a fresh SetTrackSequence — the same thing the USB-MSC handover does for the same reason.
        //
        // FM also MIXES with playback rather than replacing it (measured — both audible at once),
        // so this is needed twice over: for the track slot, and so the radio is not playing over
        // an album.
        if (g_playing) set_transport(false);
        cinder_audio_release_sequence();
        cinder_audio_close_player();
        int rc = cinder_tuner_start(cinder_fm_khz());
        g_fm_on = (rc == 0);
        char m[128];
        std::snprintf(m, sizeof m, "fm: start(%d kHz) rc=%d antenna=%d tunerState=%d",
                      cinder_fm_khz(), rc, ant, cinder_tuner_state());
        clog_(m);
        cinder_fm_report_playing(g_fm_on ? 1 : 0);
        if (g_fm_on) cinder_fm_report_khz(cinder_tuner_get_khz());
    } else {
        cinder_tuner_stop();
        g_fm_on = false;
        cinder_fm_report_playing(0);
        // The player was released on the way in; re-init so the next ▶ has a controller to talk to.
        cinder_audio_init("cinder");
    }
}

static void fm_tune_fn() {
    if (!g_fm_on) return;
    cinder_tuner_set_khz(cinder_fm_khz());
    // Report what the radio HOLDS: SetFrequency rejects out-of-band values and keeps the previous
    // one, so echoing the request back would show a frequency that is not tuned.
    cinder_fm_report_khz(cinder_tuner_get_khz());
}

static void fm_seek_fn() {
    if (!g_fm_on) return;
    int found = cinder_tuner_seek(cinder_fm_khz(), cinder_fm_seek_dir(), fm_on_step);
    cinder_fm_report_khz(found ? found : cinder_tuner_get_khz());
}

static void fm_scan_fn() {
    // A scan needs the capture PCM, which AudioInPlayerService holds while playing — so stop
    // first, sweep, then bring the radio back up on whatever it was tuned to.
    const bool was = g_fm_on;
    if (was) { cinder_tuner_stop(); g_fm_on = false; }
    cinder_tuner_set_progress_cb(fm_on_progress);
    int found[8] = {0};
    int n = cinder_tuner_scan(87500, 108000, found, 8);
    cinder_tuner_set_progress_cb(nullptr);
    cinder_fm_report_stations(found, n);
    if (was) {
        int rc = cinder_tuner_start(cinder_fm_khz());
        g_fm_on = (rc == 0);
        cinder_fm_report_playing(g_fm_on ? 1 : 0);
    }
}


// Radio out over Bluetooth. The tuner keeps playing; what changes is where the audio goes.
//
// FM audio is analogue into the codec, so it can only leave over Bluetooth by being DIGITISED
// first — `analog input device` = tuner puts it on the ADC, and hw:0,1 is where it lands. From
// there it is the same bridge USB-DAC uses, with a different source.
//
// The aerial and the output are INDEPENDENT: the cable only has to be in the jack to receive, not
// to be listened to. Cable in for reception, audio on LDAC.
//
// AudioInPlayerService and the bridge both want hw:0,1, so they cannot both run. Switching BT
// output on stops the local audio path; switching it off brings it back.
static void fm_btout_fn() {
    const bool want = cinder_fm_bt_out() != 0;
    if (!g_fm_on) { cinder_fm_report_bt_out(0); clog_("fm-bt: radio is off — nothing to send"); return; }
    if (want) {
        cinder_tuner_audio_stop();      // release hw:0,1 from AudioInPlayerService
        fmbt_start();
        cinder_fm_report_bt_out(g_ldac_alive ? 1 : 0);
        clog_(g_ldac_alive ? "fm-bt: bridging radio to Bluetooth" : "fm-bt: bridge failed to start");
    } else {
        ldac_stop();
        cinder_tuner_audio_start();     // hand hw:0,1 back to the local path
        cinder_fm_report_bt_out(0);
        clog_("fm-bt: back to the headphone jack");
    }
}

// Carry out a navigator action via the audio/effect shims. Volume goes to the configured
// backend (built-in CXD3778GF defaults, overridable by conf); play-by-index hands PlayerService
// a NodeTrackSequence built from the pending-play list the UI resolved.
void carry_out(int act) {
    // PAINT THE UI'S ANSWER BEFORE DOING THE SLOW PART.
    //
    // The navigator has already updated itself by the time we get here — cinder_tap/cinder_input
    // flip the state and set the dirty flag, then hand back an action code. But the frame is not
    // painted until cinder_render_tick() runs, which is BELOW input_pump in the render loop, so
    // every control that triggers a Sony IPC call showed its new state only after that call
    // returned. Reported 2026-08-16 as "a bit of delay on the shuffle toggle showing": shuffle
    // re-issues the track sequence, and a SetTrackSequence + seek is a measured 360–450 ms round
    // trip. The toggle was already on; the glass just could not say so yet.
    //
    // One extra tick, and it is nearly free: the renderer is dirty-flag gated, so this costs a real
    // frame exactly when something changed (which is the case worth paying for) and returns
    // immediately otherwise — including on the housekeeping-driven calls into here, where nothing
    // has. Skipped entirely while the panel is dark, where there is nothing to show.
    //
    // The USB-MSC path already did this by hand for its own 8 s blocking call; this generalises it
    // to every action rather than to the one that was slow enough to notice first.
    if (g_screen_on) cinder_render_tick();

    // Every transport call is a PlayerService IPC call → guard it (same invariant as the EQ apply
    // and the now-playing poll): a hung/crashing PlayerService then skips that one action and the UI
    // keeps running, instead of tripping the per-frame watchdog into a fatal _exit/reboot. The
    // lambdas are non-capturing (g_playing is global) so they convert to run_guarded's fn pointer.
    switch (act) {
        case CINDER_ACT_PLAYPAUSE:
            set_transport(!g_playing);
            run_guarded("carry_out: play/pause", 12, []() {
                // FIRST ▶ AFTER A BOOT, with a context restored from the last session:
                // PlayerService has never been told about that sequence, so a plain Play() would
                // start whatever it happens to hold (usually nothing). Hand it over here rather
                // than at boot — this is the press where a ~400 ms SetTrackSequence is expected,
                // and it is the only point at which the user has actually asked for sound.
                // restore_position picks up the saved offset, which cinder_resume_load has
                // already put in cinder_play_position_ms().
                if (g_playing && cinder_resume_take_pending()) {
                    play_pending_sequence("resume", true);
                    return;
                }
                if (g_playing) cinder_audio_play(); else cinder_audio_pause();
            });
            break;
        case CINDER_ACT_NEXT:       run_guarded("carry_out: next",  6, []() { cinder_audio_next_track(); }); break;
        case CINDER_ACT_PREV:
            // ◁ WITH A REWIND. The report was "no rewind in some queue situations", and this
            // button is the whole of it. It used to be an unconditional PrevTrack(), which has
            // two failure modes that look opposite but are the same missing rule:
            //   • at the HEAD of a sequence (single-track queue, or track 1 of an album you just
            //     tapped) there is nowhere to step back to, so ◁ did NOTHING;
            //   • mid-track it jumped to the previous track when the user meant "start this over".
            // Both are handled here: past the 3 s grace window ◁ restarts the track, and if
            // PrevTrack itself fails we restart rather than silently no-op. cinder_audio_seek_ms
            // is the proven path (it pauses, seeks and resumes — the engine refuses to seek while
            // streaming), and notify_seek re-anchors the bar so it doesn't drift back for a second.
            run_guarded("carry_out: prev", 8, []() {
                if (cinder_prev_means_restart()) {
                    cinder_notify_seek_ms(0);
                    cinder_audio_seek_ms(0);
                    clog_("transport: prev -> restart current track (past the 3 s grace)");
                    return;
                }
                if (cinder_prepare_previous_play()) {
                    play_pending_sequence("previous from Cinder history", false);
                    clog_("transport: prev -> Cinder playback history");
                    return;
                }
                int rc = cinder_audio_prev_track();
                if (rc != 0) {
                    cinder_notify_seek_ms(0);
                    cinder_audio_seek_ms(0);
                    char m[96];
                    std::snprintf(m, sizeof m, "transport: PrevTrack rc=%d (head of sequence) -> restart", rc);
                    clog_(m);
                }
            });
            break;
        case CINDER_ACT_NEXT_ALBUM: run_guarded("carry_out: next album", 6, []() { cinder_audio_next_group(); }); break;
        case CINDER_ACT_PREV_ALBUM: run_guarded("carry_out: prev album", 6, []() { cinder_audio_prev_group(); }); break;
        case CINDER_ACT_VOLUP:
        case CINDER_ACT_VOLDOWN:
            // The rocker drives whichever output is actually carrying audio, and the two levels are
            // kept apart end to end (separate UI fields, separate persisted keys). On Bluetooth the
            // codec master is left exactly where the jack had it, so unplugging headphones doesn't
            // dump the headphone level into your ears.
            if (cinder_get_bt_route()) {
                // Not coalesced the way the amixer path is. That optimisation exists because an
                // amixer write costs a fork+exec of /bin/sh; this is a single IPC message, and on
                // the step fallback the calls are RELATIVE, so dropping one during a ramp loses a
                // step outright rather than being superseded by the next write.
                if (act == CINDER_ACT_VOLUP) run_guarded("carry_out: BT volume up",   4, []() { apply_bt_volume(true);  });
                else                         run_guarded("carry_out: BT volume down", 4, []() { apply_bt_volume(false); });
            } else {
                // apply the new UI volume to the hardware via the configured backend (guarded).
                // Defaults to the discovered control (amixer card0 'master volume' 0..120) with no
                // conf present; /contents/cinder_volume.conf overrides it.
                run_guarded("carry_out: volume", 4, apply_volume);
            }
            break;
        case CINDER_ACT_PLAY_INDEX:
            // Play the tapped track inside its album context: drain the pending-play URI list the
            // UI resolved (cinder_pending_play_*), hand PlayerService a NodeTrackSequence, start
            // at the tapped index. Guarded: JSON->Node + SetTrackSequence are Sony-service calls.
            run_guarded("carry_out: play selected track", 10, []() {
                // The user chose their own sequence, so the one the last boot left behind is dead.
                // Without this a resume armed at boot would still be waiting, and the next ▶ after
                // a pause would drag playback back to wherever the previous session stopped.
                cinder_resume_cancel();
                play_pending_sequence("play", false);
            });
            break;
        case CINDER_ACT_QUEUE_CHANGED:
            run_guarded("queue: apply now", 10, []() {
                if (cinder_take_queue_flush()) play_pending_sequence("queue", true);
            });
            break;
        case CINDER_ACT_EQ_CHANGED:
            // apply EQ to the DSP, guarded (a sound-service fault skips it, UI keeps running)
            run_guarded("carry_out: apply EQ to DSP", 6, apply_eq_fn);
            break;
        case CINDER_ACT_BATTERY_CARE_CHANGED:
            // apply the new battery-care (Itawari) state to PowerMgrServiceClient, guarded.
            run_guarded("carry_out: apply battery care", 6,
                        []() { cinder_power_set_battery_care(cinder_get_battery_care()); });
            break;
        case CINDER_ACT_SOUND_CHANGED:
            // apply the Sound screen's effect toggles to the DSP, guarded.
            run_guarded("carry_out: apply sound effects", 6, apply_sound_fn);
            break;
        case CINDER_ACT_CLOCK_SET:
            run_guarded("carry_out: set clock", 8, apply_clock);
            break;
        case CINDER_ACT_BALANCE_CHANGED:
            // The balance slider, live under a finger. Just the two mixer writes — NOT
            // apply_sound_fn, which would run six EffectCtrlDmp round trips per motion event and
            // turn a drag into the poll storm that caused the audio stutter in task #26.
            run_guarded("carry_out: balance", 4, []() { apply_balance(cinder_get_balance()); });
            break;
        case CINDER_ACT_SOUND_BYPASS:
            // A/B compare: bypass or re-enable the whole effect chain, guarded.
            run_guarded("carry_out: A/B bypass", 6,
                        []() { cinder_effects_set_bypass(cinder_get_sound_bypass()); });
            break;
        case CINDER_ACT_THEME_CHANGED:
            // night/day toggled -> set the panel backlight (night = minimal light), guarded.
            run_guarded("carry_out: backlight (theme)", 4, apply_backlight);
            break;
        case CINDER_ACT_BOOT_TO_STOCK:
            run_guarded("carry_out: boot to stock", 8, boot_to_stock);
            break;
        case CINDER_ACT_SCREEN_OFF_CHANGED:
            // Nothing to apply now — the countdown lives in the pump and reads the value each tick.
            // Just treat the change itself as activity so picking a short timeout doesn't blank the
            // screen a moment later while the user is still reading the row.
            g_last_input_ms = now_ms();
            break;
        case CINDER_ACT_RESTART:  power_action(true);  break;
        case CINDER_ACT_POWER_OFF: power_action(false); break;
        case CINDER_ACT_REPEAT_CHANGED:
            // Repeat-one → NodeTrackSequence::SetOneTrackMode on the pinned sequence. Guarded:
            // it is a call into Sony-constructed object layout, and a wedge here must not take
            // the frame loop with it.
            run_guarded("repeat: set one-track mode", 4,
                        []() { cinder_audio_set_repeat_one(cinder_get_repeat_one()); });
            break;
        case CINDER_ACT_BRIGHTNESS_CHANGED:
            // Settings Brightness row cycled 1..5 → recompute the day level + rewrite the node.
            run_guarded("carry_out: backlight (brightness)", 4, apply_brightness);
            break;
        case CINDER_ACT_SETTINGS_RESET:
            // Everything moved at once, so re-apply the whole chain rather than the one control a
            // normal change would touch. Same order and the same functions the boot path uses
            // (deferred_up), because "reset" should leave the device in the state a first boot
            // with this build would have produced.
            //
            // Each step is separately guarded, as everywhere else: a fault inside one Sony service
            // must not stop the remaining ones from being restored — a half-reset chain is worse
            // than either end of it.
            clog_("carry_out: settings reset — re-applying the whole chain");
            run_guarded("reset: EQ", 6, apply_eq_fn);
            run_guarded("reset: sound effects + high gain + balance", 8, apply_sound_fn);
            run_guarded("reset: volume", 4, apply_volume);
            run_guarded("reset: backlight", 4, apply_brightness);
            run_guarded("reset: repeat-one", 4,
                        []() { cinder_audio_set_repeat_one(cinder_get_repeat_one()); });
            run_guarded("reset: battery care", 6,
                        []() { cinder_power_set_battery_care(cinder_get_battery_care()); });
            // The reset also cleared the screen-off timeout back to its default; treat the change
            // as activity so the panel does not blank while the user is still reading the toast.
            g_last_input_ms = now_ms();
            break;
        case CINDER_ACT_BT_CODEC_CHANGED:
            // device-wide codec/quality changed → persist it for every BT path (file IO, safe)…
            write_bt_pref();
            // …and apply it to the live radio, which the conf file alone never did.
            run_guarded("carry_out: BT codec apply", 6, apply_bt_codec);
            break;
        case CINDER_ACT_BT_ENHANCED_CHANGED:
            // "Use Enhanced Mode" toggled → hand the preference to the radio, then re-push the
            // level so the change is audible immediately rather than at the next volume press.
            run_guarded("carry_out: BT enhanced mode", 6, []() {
                bt_apply_enhanced_mode("user toggle");
                if (bt_use_absolute_volume()) apply_bt_volume(true);
            });
            break;
        case CINDER_ACT_USBDAC_LDAC:
            // 18 s, not 6: this used to be one setprop via cinder-msc, but it now also builds a
            // UsbDeviceAudioPlayerServiceClient and makes a Start()/Stop() IPC round trip on top of
            // the gadget re-enumerating. A budget sized for the old body would fire the guard
            // mid-call and _exit a perfectly healthy app.
            run_guarded("carry_out: USB-DAC/LDAC toggle", 18, apply_usb_dac);
            break;
        case CINDER_ACT_BT_TOGGLE:
            // 8 s: SetRfOnOff + at most ~0.9 s of polling + the connect request. Deliberately not
            // generous — this runs on the render thread, and a long budget here buys a frozen UI.
            run_guarded("carry_out: Bluetooth toggle", 8, apply_bt_toggle);
            break;
        case CINDER_ACT_BT_DISCONNECT:
            // One no-arg IPC call plus two status reads — nothing here polls, so 6 s is plenty.
            run_guarded("carry_out: Bluetooth disconnect", 6, apply_bt_disconnect);
            break;
        case CINDER_ACT_BT_PAIRED_REFRESH:
            run_guarded("carry_out: Bluetooth paired list", 6, refresh_bt_paired);
            break;
        case CINDER_ACT_BT_CONNECT_DEVICE:
            // Codec apply + one connect request, then a list re-read. The connect itself is async, so
            // this does not wait for the link — 8 s matches the toggle path for the same reason.
            run_guarded("carry_out: Bluetooth connect device", 8, apply_bt_connect_device);
            break;
        case CINDER_ACT_BT_FORGET_DEVICE:
            run_guarded("carry_out: Bluetooth forget device", 6, apply_bt_forget_device);
            break;
        case CINDER_ACT_BT_SCAN_TOGGLE:
            // Registers the listener on first use, then one SetSearchMode call — nothing waits here.
            run_guarded("carry_out: Bluetooth scan", 6, apply_bt_scan);
            break;
        case CINDER_ACT_BT_PAIR_DEVICE:
            run_guarded("carry_out: Bluetooth pair device", 8, apply_bt_pair_device);
            break;
        case CINDER_ACT_BT_PROMPT_CONFIRM:
            run_guarded("carry_out: Bluetooth prompt confirm", 6, []() { apply_bt_prompt_reply(true); });
            break;
        case CINDER_ACT_BT_PROMPT_CANCEL:
            run_guarded("carry_out: Bluetooth prompt cancel", 6, []() { apply_bt_prompt_reply(false); });
            break;
        // Power = panel on/off (not lock). GUARDED: waking now also verifies the mixer against the
        // UI level, which on the amixer backend is a popen — a wedged amixer must not stall the
        // render/input thread with the panel half-woken.
        case CINDER_ACT_SLEEP:
            run_guarded("carry_out: screen toggle", 8, screen_toggle);
            break;
        case CINDER_ACT_ENTER_USB_MSC:
            // device-gated USB-mode switch (hands storage to the PC; disruptive — validate live).
            // 25 s budget: Stop IPC + the 5 s umount verify + recovery + the 3 s gadget-state
            // settle all run under this ONE guard (run_guarded doesn't nest).
            // Entering by hand clears the refusal — asking for mass storage IS changing your mind,
            // and leaving the latch set would make the auto path stay dead after this session ends.
            g_msc_user_off = false;
            g_msc_off_gone = 0;
            run_guarded("carry_out: enter USB MSC", 25, enter_usb_msc);
            break;
        case CINDER_ACT_FM_POWER:
            // 20 s: bringing the tuner up opens two Sony services and re-routes the codec.
            run_guarded("carry_out: FM power", 20, fm_power_fn);
            break;
        case CINDER_ACT_FM_TUNE:
            run_guarded("carry_out: FM tune", 8, fm_tune_fn);
            break;
        case CINDER_ACT_FM_SEEK:
            // A full band traversal at ~0.14 s a step is ~30 s worst case; budget generously,
            // because the guard firing mid-seek would leave the tuner holding the capture PCM.
            run_guarded("carry_out: FM seek", 45, fm_seek_fn);
            break;
        case CINDER_ACT_FM_BT_OUT:
            run_guarded("carry_out: FM Bluetooth out", 20, fm_btout_fn);
            break;
        case CINDER_ACT_FM_SCAN:
            // ~0.45 s per step across 206 steps.
            run_guarded("carry_out: FM scan", 150, fm_scan_fn);
            break;
        case CINDER_ACT_EXIT_USB_MSC:
            // Back on the modal (or the unplug watcher) → remount /contents + restore the log.
            // Latch the refusal FIRST: exit_usb_msc takes seconds, and a watcher tick landing in
            // the middle of it would re-enter before the flag was set.
            // `usb_connected()`, not `usb_data_host()`: by the time this runs the gadget may
            // already be mid-switch, and the question being asked is "is the cable in", not "is the
            // gadget enumerated right now".
            if (usb_connected()) {
                g_msc_user_off = true;
                g_msc_off_gone = 0;
                clog_("usb-msc: left by hand with the cable still in — auto-MSC is off until unplug");
            }
            run_guarded("carry_out: exit USB MSC", 10, exit_usb_msc);
            break;
        default: break;
    }
}

// Drain pending input from every node; map raw code -> logical button -> navigator -> action.
// Classify a finished contact (finger-up), from BTN_TOUCH=0 or a type-A empty-frame lift.
// A settings-slider drag can have work for us on EVERY motion event, not just on release — the
// balance slider writes the codec's two attenuators as the finger moves, because the point of
// dragging a balance control is hearing the image move. The progress rail reports nothing here; it
// returns its seek target from cinder_scrub_end() instead.
//
// AND IT IS RATE-LIMITED, because MEASURED 2026-08-17 (`cinder-probe --fxtime`) an effect write is
// not cheap:
//
//     get_eq_band          median      1 us      <- reads are free
//     set_eq_band          median 10304 us      <- writes are 10 ms, changed value or not
//     set_tone_value       median  6956 us
//     set_vpt_mode         median  2981 us
//
// A UI frame is 16 ms. One EQ band write is most of a frame; the ten the shell used to send on
// every change were 103 ms, six frames, which is exactly the stutter the user reported when
// dragging the EQ. Two fixes, and both are needed:
//
//   1. `fx_dirty` (see apply_sound_fn) skips the calls whose value has not moved — a drag then
//      touches ONE band instead of twenty-five controls. Note the probe line above: writing the
//      SAME value costs the same 10 ms, so the service does not short-circuit and the caller must.
//   2. this throttle, because even one 10 ms write per motion event eats 60% of the frame budget.
//      During a drag the write happens at most every SCRUB_APPLY_MS; in between, the UI still
//      follows the finger at full rate because the value lives in the navigator, not the DSP.
//
// Correctness of dropping writes rests on one thing: the scrub's RELEASE always emits the action
// again (nav's `scrub_end` re-emits EqChanged/SoundChanged for exactly this reason), so the final
// value is never the one that got skipped.
static const long SCRUB_APPLY_MS = 60;
static long long g_scrub_last_apply_ms = 0;

static long long mono_ms() {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (long long)ts.tv_sec * 1000LL + ts.tv_nsec / 1000000LL;
}

static void scrub_carry() {
    int act = cinder_scrub_action();
    if (act == CINDER_ACT_NONE) return;
    if (g_scrub_active && g_touch_down) {
        const long long now = mono_ms();
        if (now - g_scrub_last_apply_ms < SCRUB_APPLY_MS) return;   // release will re-emit it
        g_scrub_last_apply_ms = now;
    }
    carry_out(act);
}

static void touch_release() {
    if (g_touch_down && g_scrub_active) {
        // Drag-to-seek ends: ask cinder-ffi where the finger left the rail and seek there.
        // -1 means "not the rail" (a settings slider) OR "no controller"; either way, no seek.
        int ms = cinder_scrub_end();
        g_scrub_last_apply_ms = 0;   // the FINAL value must never be the one the throttle drops
        scrub_carry();
        if (ms >= 0) {
            // -1 means "no controller"; 0 only means the request was SENT. SeekTime is void, so
            // there is no acceptance to report — the old "seek REJECTED" line here was reading a
            // leftover register and saying something it could not know. Where the seek actually
            // landed is answered by the /tmp/cinder_seek.req dev probe, not from this call.
            // PAINT THE NEW POSITION BEFORE BLOCKING. cinder_scrub_end has already re-anchored the
            // bar on the target, but the seek itself takes ~700 ms — and MEASURED 2026-07-28 that
            // is almost entirely Sony's two ChangePlayState round trips (pause 190-254 ms, play
            // 440-503 ms; the SeekTime in the middle is 7-35 ms). The engine will not seek while it
            // is streaming, so the pause is not optional.
            //
            // Nothing here can make the audio resume faster, but the render thread does not have to
            // sit on a stale frame for the whole transaction. One tick costs ~16 ms and puts the
            // bar where the finger left it immediately, so the wait reads as the audio catching up
            // rather than as the UI having ignored the gesture.
            //
            // (The full fix is to run the sequence off the render thread. Not done: it would mean
            // concurrent PlayController IPC with whatever carry_out is doing, and the client's
            // thread-safety is unknown — a real risk for a ~700 ms input-blocking window that only
            // occurs on a deliberate drag.)
            cinder_render_tick();
            int rc = cinder_audio_seek_ms(ms);
            char m[80];
            std::snprintf(m, sizeof m, "touch: seek -> %d ms (sent=%s)", ms, rc == 0 ? "yes" : "NO CONTROLLER");
            clog_(m);
        }
    } else if (g_touch_down && g_sbar_active) {
        // Scrollbar drag ends. Its own branch for the same reason the reorder has one: falling
        // through would re-read a short bar drag as a TAP on the A-Z rail and jump to a letter.
        cinder_sbar_release();
    } else if (g_touch_down && g_reorder_active) {
        // Queue reorder ends: drop the row where it sits. Must be its OWN branch — falling through
        // to the classifier below would re-read the gesture as a tap (a short drag) or a swipe and
        // start playing the track the user was only trying to move.
        cinder_reorder_release();
    } else if (g_touch_down && g_drag_active) {
        // Live drag ends: hand the measured velocity to the fling (unless the finger held
        // still before lifting — stale velocity must not fling).
        if (now_ms() - g_drag_last_ms <= 120 && (g_drag_vel > 220.0f || g_drag_vel < -220.0f))
            cinder_touch_fling((int)g_drag_vel);
    } else if (g_touch_down && g_touch_start_x >= 0) {
        int sx = touch_ui_x(g_touch_start_x), cx = touch_ui_x(g_touch_cur_x);
        int sy = (g_touch_start_y >= 0) ? touch_ui_y(g_touch_start_y) : 0;
        int cy = (g_touch_cur_y >= 0) ? touch_ui_y(g_touch_cur_y) : sy;
        int dx = cx - sx, dy = cy - sy;
        int adx = dx < 0 ? -dx : dx, ady = dy < 0 ? -dy : dy;
        if (sx <= 38 && dx >= 120) {                 // left-edge → rightward = Back
            int act = cinder_input(CINDER_BTN_BACK);
            if (act != CINDER_ACT_NONE) carry_out(act);
        } else if (adx < 26 && ady < 26) {           // ~stationary = tap (26: sloppy thumbs drift)
            int act = cinder_tap(cx, cy);
            if (act != CINDER_ACT_NONE) carry_out(act);
        } else if (adx > ady && adx >= 60) {         // horizontal swipe: onboarding pages, NP skip,
            // rightward-on-a-song-row = add to queue (start point picks the row). Edge-back
            // already won above, so the two rightward gestures coexist: from the left edge it's
            // Back, anywhere else on a list row it's queue.
            int act = cinder_swipe(dx < 0 ? -1 : 1, sx, sy);
            if (act != CINDER_ACT_NONE) carry_out(act);
        }
        // (vertical drags never reach here — they became a live drag at ~12px of movement)
    }
    // Whatever the classification decided, a row that was following the finger has to be let go:
    // this animates it home. Ordered AFTER cinder_swipe so the queue action (and its toast) is
    // already in flight when the row starts travelling back.
    if (g_hswipe_active) cinder_swipe_release();
    g_touch_down = false; g_touch_start_x = -1; g_touch_start_y = -1;
    g_drag_active = false; g_drag_vel = 0.0f;
    g_scrub_active = false; g_scrub_tested = false; g_hswipe_active = false; g_reorder_active = false; g_sbar_active = false;
}

// Called on every touch position update while the contact is down: promote a mostly-vertical
// move past a small slop into a LIVE DRAG, then stream pixel deltas + track velocity.
static void touch_drag_motion() {
    if (!g_touch_down || g_touch_start_x < 0 || g_touch_start_y < 0 || g_touch_cur_y < 0) return;
    int uy = touch_ui_y(g_touch_cur_y);
    // Classify ONCE per contact, on the first motion after both coordinates are known (X and Y
    // arrive as separate events, so finger-down alone doesn't have a position yet). The hit test
    // uses the START point: a scrub only begins if the finger LANDED on the rail, so a list drag
    // that happens to pass over the rail is unaffected.
    if (!g_scrub_tested) {
        g_scrub_tested = true;
        if (cinder_scrub_hit(touch_ui_x(g_touch_start_x), touch_ui_y(g_touch_start_y))) {
            g_scrub_active = true;
            cinder_scrub_to(touch_ui_x(g_touch_cur_x), touch_ui_y(g_touch_cur_y));  // a tap on a slider also sets it
            scrub_carry();
            return;
        }
    }
    if (g_scrub_active) {
        // The control owns this contact. Never promote to a list drag — wander during a scrub must
        // not start scrolling the screen underneath, and that matters MORE now that some sliders
        // are vertical: an EQ band drag is a vertical gesture over a screen that does not scroll,
        // and without this it would read as a list drag the moment it left the field.
        cinder_scrub_to(touch_ui_x(g_touch_cur_x), touch_ui_y(g_touch_cur_y));
        scrub_carry();
        return;
    }
    if (g_hswipe_active) {
        // The row owns this contact: stream its travel so it keeps tracking the finger.
        cinder_swipe_track(touch_ui_x(g_touch_cur_x) - touch_ui_x(g_touch_start_x),
                           touch_ui_y(g_touch_start_y));
        return;
    }
    if (g_reorder_active) {
        // Same deal vertically: the lifted queue row follows the finger, the list does not scroll.
        cinder_reorder_track(uy - touch_ui_y(g_touch_start_y));
        return;
    }
    if (g_sbar_active) {
        cinder_sbar_track(uy - touch_ui_y(g_touch_start_y));
        return;
    }
    if (!g_drag_active) {
        int dyt = uy - touch_ui_y(g_touch_start_y);
        int dxt = touch_ui_x(g_touch_cur_x) - touch_ui_x(g_touch_start_x);
        int adyt = dyt < 0 ? -dyt : dyt, adxt = dxt < 0 ? -dxt : dxt;
        if (adyt > 12 && adyt > adxt) {
            // A vertical drag that STARTED on an Up Next grab handle reorders that row instead of
            // scrolling the list. Offered before the scroll, and decided on the START point — the
            // same ownership rule as the scrub rail, so a drag begun elsewhere keeps scrolling even
            // when it wanders across the handle column.
            if (cinder_reorder_begin(touch_ui_x(g_touch_start_x), touch_ui_y(g_touch_start_y))) {
                g_reorder_active = true;
                cinder_reorder_track(dyt);
                return;
            }
            // Then the scrollbar, at the right edge. AFTER the reorder, so where the two strips
            // overlap on Up Next the queue's grab handle wins.
            if (cinder_sbar_begin(touch_ui_x(g_touch_start_x), touch_ui_y(g_touch_start_y))) {
                g_sbar_active = true;
                cinder_sbar_track(dyt);
                return;
            }
            g_drag_active = true;
            g_drag_last_uy = uy;
            g_drag_last_ms = now_ms();
            g_drag_vel = 0.0f;
            return;
        }
        // Mostly-horizontal past the same 12px slop: offer it to the UI as a live row swipe. The
        // left-edge Back gesture is NOT offered — it starts at x<=38 and owns that strip, and a row
        // that slid under an edge-back would promise a queue that release never performs.
        if (adxt > 12 && adxt > adyt && touch_ui_x(g_touch_start_x) > 38) {
            g_hswipe_active = cinder_swipe_track(dxt, touch_ui_y(g_touch_start_y)) != 0;
        }
        return;
    }
    int delta = uy - g_drag_last_uy;
    if (delta == 0) return;
    cinder_touch_drag(-delta);   // finger up (delta<0) = show later rows (positive scroll)
    long now = now_ms();
    long dt = now - g_drag_last_ms;
    if (dt > 0) {
        float inst = (float)(-delta) * 1000.0f / (float)dt;
        g_drag_vel = 0.75f * g_drag_vel + 0.25f * inst;   // EMA smooths sensor jitter
    }
    g_drag_last_uy = uy;
    g_drag_last_ms = now;
}

// ── Volume rocker auto-repeat ───────────────────────────────────────────────────────────────
// Holding the rocker did nothing past the first step. The `val == 2` path in input_pump only ramps
// when the KERNEL generates key repeats, and these evdev nodes don't set EV_REP for the side keys —
// a held button delivers one press and then silence until the release.
//
// So we synthesize the repeat: remember which volume key is down and re-issue its action on a
// timer. Any kernel repeat that does arrive pushes `g_vol_last_ms` forward, so on a unit/firmware
// where EV_REP is set the two can't stack into a double-speed ramp — whichever ticks first wins.
//
// The dead-man cap matters more than it looks: every other button ignores releases (input_pump
// drops `val == 0`), so if a release event is ever lost the ramp would otherwise run forever with
// no finger on the device.
int g_vol_btn = -1;      // CINDER_BTN_VOLUP / _VOLDOWN while held, else -1
long g_vol_down_ms = 0;  // when the press landed
long g_vol_last_ms = 0;  // last volume action emitted (kernel repeat OR synthetic)

const long VOL_REPEAT_DELAY_MS = 350;    // hold this long before the ramp starts…
const long VOL_REPEAT_EVERY_MS = 120;    // …then one step this often (~8/s, full scale in ~4 s)
const long VOL_REPEAT_MAX_MS = 15000;    // dead-man: give up if a release was missed

void vol_repeat_tick() {
    if (g_vol_btn < 0) return;
    long now = now_ms();
    if (now - g_vol_down_ms > VOL_REPEAT_MAX_MS) { g_vol_btn = -1; return; }
    if (now - g_vol_down_ms < VOL_REPEAT_DELAY_MS) return;
    if (now - g_vol_last_ms < VOL_REPEAT_EVERY_MS) return;
    g_vol_last_ms = now;
    int act = cinder_input(g_vol_btn);
    if (act != CINDER_ACT_NONE) carry_out(act);
}

// ── Power button: HOLD opens the power menu, TAP toggles the screen ──────────────────────────
// Sony's own gesture, and the one the user asked for: hold Power for about a second and a menu
// comes up. Cinder used to act on the PRESS (screen toggle), so no matter how long you held it the
// only outcomes were "screen off" and — past the PMIC's own ~8 s threshold, which is hardware and
// nothing to do with us — a forced reset.
//
// So the press now only starts a clock. Which action fires is decided later:
//   held >= POWER_HOLD_MS  -> open the menu, and the release does NOTHING
//   released before that   -> the screen toggle, exactly as before
// The menu therefore has to open on a TIMER rather than on an event: mtk-kpd's key repeats are not
// something to depend on for a safety-relevant gesture, so power_hold_tick() is called every frame
// from the end of input_pump(), the same way the volume ramp is driven.
// THE ONE ASSUMPTION HERE IS THAT KEY_POWER REPORTS ITS RELEASE. Everything else in this file
// deliberately ignores releases ("releases never act"), and mtk-kpd has never been observed
// reporting one because nothing ever looked. If it does not, deferring the screen toggle to the
// release would make a short Power press do NOTHING — a core function, silently dead.
//
// So the toggle is only deferred once a release has actually been seen. Until then Power behaves
// exactly as it did before (toggle on the press), and the hold menu is disabled. The first real
// release flips this permanently, on the very first press of a boot, and the dev log records both
// edges so the answer is in cinderhome.log rather than in anyone's assumption.
static bool g_power_hold_ok = false;  // have we ever seen a KEY_POWER release on this unit?
static long g_power_down_ms = 0;      // when Power went down (0 = not down)
static bool g_power_consumed = false; // has this press already produced the menu?
static const long POWER_HOLD_MS = 1000;

void power_hold_tick() {
    if (!g_power_hold_ok || g_power_down_ms == 0 || g_power_consumed) return;
    if (now_ms() - g_power_down_ms < POWER_HOLD_MS) return;
    g_power_consumed = true;   // set FIRST: whatever happens next, this press is spent
    if (cinder_power_held()) {
        clog_("input: Power held — power menu");
        // Waking here matters: holding Power with the panel already dark must show the menu, not
        // put up a dialog nobody can see. The blank is ours (backlight only), so this is a write.
        if (!g_screen_on) screen_toggle();
        g_last_input_ms = now_ms();
    }
    // If the navigator refused (Hold engaged, or a modal already up) the press stays consumed
    // anyway — a refused hold must not fall through to a screen toggle on release either, or
    // holding Power in a pocket would blank/unblank the panel.
}

// Consume a dev request-file: true if one was waiting. Handles the two ways /tmp defeats us —
// cinder-home is uid `system`, /tmp is sticky (drwxrwxrwt) and `adb shell echo >` creates files
// root-owned 0600, so an unlink() by a non-owner is EPERM and a request that cannot be removed
// re-fires on every housekeeping tick forever (observed 2026-07-28: one echo, fifty seek probes).
// Falls back to truncating, and treats an empty file as "nothing to do", so a request is consumed
// exactly once whether or not we own it. `out` receives the first line, if the caller wants it.
static bool take_req(const char* path, char* out, size_t cap) {
    struct stat st;
    if (::stat(path, &st) != 0 || st.st_size == 0) return false;
    bool read_ok = false;
    if (out && cap) {
        out[0] = 0;
        if (FILE* f = std::fopen(path, "r")) {
            if (std::fgets(out, (int)cap, f)) read_ok = true;
            std::fclose(f);
        }
    } else {
        read_ok = true;   // caller only cares that the file was there
    }
    if (::unlink(path) != 0) {
        if (FILE* t = std::fopen(path, "w")) std::fclose(t);   // truncate = consumed
    }
    if (!read_ok) {
        char m[160];
        std::snprintf(m, sizeof m, "req: %s exists but is UNREADABLE by us (chmod 666 it) — ignored", path);
        clog_(m);
    }
    return read_ok;
}

void input_pump() {
    ev_event evs[32];
    static long g_ev_total = 0;   // events ever seen (any node) — for the silent-input heartbeat
    static long g_pump_calls = 0;
    const long ev_before = g_ev_total;
    // HEARTBEAT: if the input system is silent (foreign grab / dead driver), say so in the log
    // every ~15 s instead of leaving "no events" indistinguishable from "nobody touched it".
    if (g_ev_total == 0 && ++g_pump_calls % 450 == 0)
        clog_("input: still ZERO events from every node (foreign grab? see node diagnostics above)");
    for (int i = 0; i < g_evn; ++i) {
        for (;;) {
            ssize_t n = read(g_evfds[i], evs, sizeof evs);
            if (n < 0 && errno != EAGAIN && errno != EWOULDBLOCK) {
                static bool logged_err[16] = {};
                if (i < 16 && !logged_err[i]) {
                    logged_err[i] = true;
                    char m[80];
                    std::snprintf(m, sizeof m, "input: read fd%d failed errno=%d%s", i, errno,
                                  (g_evfds[i] == g_touch_fd) ? " [TOUCH]" : "");
                    clog_(m);
                }
            }
            if (n <= 0) break;
            int cnt = (int)(n / (ssize_t)sizeof(ev_event));
            g_ev_total += cnt;
            for (int k = 0; k < cnt; ++k) {
                uint16_t type = evs[k].type, code = evs[k].code;
                int val = evs[k].value;
#ifdef CINDER_DEV
                // DEV: log the first ~200 raw input events (with the touch fd marked) so the exact
                // touchscreen protocol is visible in cinderhome.log if gestures still misbehave.
                {
                    static int g_evlog = 0;
                    if (g_evlog < 200) {
                        char m[80];
                        std::snprintf(m, sizeof m, "input: ev fd%d%s type=0x%x code=0x%x val=%d", i,
                                      (g_evfds[i] == g_touch_fd) ? "*TS" : "", type, code, val);
                        clog_(m); ++g_evlog;
                    }
                }
#endif
                // ── Touchscreen navigation (no d-pad on the NW-A55) — ONLY from the real touch fd,
                // so the sensor-batch node's ABS_X/Y can't masquerade as finger coordinates. Robust
                // to BTN_TOUCH panels AND type-A MT (contact begins on the first position; a lift is
                // BTN_TOUCH=0 or a SYN_REPORT frame that carried no position).
                if (g_evfds[i] == g_touch_fd) {
                    // Panel dark: taps must not navigate invisibly. Stock sleeps the controller;
                    // this fw has no sleep node (see touch_set_sleep), so the events keep coming —
                    // drop them and any in-flight contact until screen-on.
                    //
                    // EXCEPT when the blank was the idle timer's: then this touch is the wake
                    // gesture. It wakes the panel and is CONSUMED (not delivered), so waking can
                    // never also activate whatever happens to be under the finger. A Power-button
                    // blank is left alone — that one stays off until Power is pressed again.
                    if (!g_screen_on) {
                        // Hold engaged = in a pocket: do NOT let stray contact wake the panel, or
                        // an idle blank would be undone by the first thing it brushes against and
                        // the battery saving is lost exactly when it matters most. Keys still wake
                        // (the buttons are usable under Hold by design), and so does the switch
                        // itself. Note this deliberately does not refresh the idle clock either.
                        if (!g_held) {
                            g_last_input_ms = now_ms();
                            if (g_screen_auto_off) screen_auto_wake();
                        }
                        g_touch_down = false; g_touch_start_x = -1; g_touch_start_y = -1;
                        g_touch_saw_pos = false;
                        g_drag_active = false; g_drag_vel = 0.0f;
                        g_scrub_active = false; g_scrub_tested = false; g_hswipe_active = false; g_reorder_active = false; g_sbar_active = false;
                        // AND SWALLOW THE REST OF THIS CONTACT. Clearing the state above is not
                        // enough: the finger is still on the glass and the panel keeps streaming
                        // positions, but `screen_auto_wake` has already set g_screen_on — so the
                        // NEXT position event falls through to the handler below, finds
                        // !g_touch_down, and synthesises a brand-new contact mid-gesture. The lift
                        // then reads as a genuine tap wherever the finger happened to be, which is
                        // exactly the "tap to wake also presses the button underneath" report
                        // (2026-08-17). The latch clears on the real lift, so the NEXT touch works
                        // normally.
                        g_touch_wake_swallow = true;
                        continue;
                    }
                    // Still inside the contact that woke the panel: drop everything until it lifts.
                    if (g_touch_wake_swallow) {
                        if (type == EV_ABS_ && (code == ABS_X_ || code == ABS_MT_POSITION_X_
                                             || code == ABS_Y_ || code == ABS_MT_POSITION_Y_)) {
                            g_touch_saw_pos = true;
                            continue;
                        }
                        // A lift is BTN_TOUCH=0, or (type-A) a SYN frame that carried no position.
                        if (type == EV_KEY_ && code == BTN_TOUCH_ && !val) {
                            g_touch_wake_swallow = false;
                            g_touch_saw_pos = false;
                            continue;
                        }
                        if (type == EV_SYN_ && code == 0) {
                            if (!g_touch_saw_pos) g_touch_wake_swallow = false;
                            g_touch_saw_pos = false;
                            continue;
                        }
                        continue;
                    }
                    // Live touch = activity, so the idle blank holds off — but NOT while Hold is
                    // engaged: those contacts are pocket noise (nav ignores them anyway), and
                    // letting them count would keep the Lock screen lit in a pocket indefinitely.
                    if (!g_held) g_last_input_ms = now_ms();
                    if (type == EV_ABS_ && (code == ABS_X_ || code == ABS_MT_POSITION_X_)) {
                        g_touch_cur_x = val; g_touch_saw_pos = true;
                        if (!g_touch_down) {
                            g_touch_down = true; g_touch_start_x = val; g_touch_start_y = -1;
                            g_scrub_active = false; g_scrub_tested = false; g_hswipe_active = false; g_reorder_active = false; g_sbar_active = false;
                            cinder_touch_down();   // finger down stops an in-flight fling
                        } else if (g_touch_start_x < 0) g_touch_start_x = val;
                        // Also drive the classifier from HERE, not only from ABS_Y: a panel that
                        // reports Y before X in a contact's first frame would otherwise never get
                        // classified (at Y time g_touch_down is still false, so touch_drag_motion
                        // returns early, and nothing calls it again for a stationary tap). Safe to
                        // call twice per frame — it no-ops unless the contact has both coordinates,
                        // and the drag branch ignores a zero y-delta.
                        touch_drag_motion();
                        continue;
                    }
                    if (type == EV_ABS_ && (code == ABS_Y_ || code == ABS_MT_POSITION_Y_)) {
                        g_touch_cur_y = val; g_touch_saw_pos = true;
                        if (g_touch_down && g_touch_start_y < 0) g_touch_start_y = val;
                        touch_drag_motion();       // live drag: stream deltas + velocity
                        continue;
                    }
                    if (type == EV_KEY_ && code == BTN_TOUCH_) {
                        if (val) {
                            if (!g_touch_down) {
                                g_touch_down = true; g_touch_start_x = -1; g_touch_start_y = -1;
                                g_scrub_active = false; g_scrub_tested = false; g_hswipe_active = false; g_reorder_active = false; g_sbar_active = false;
                                cinder_touch_down();
                            }
                        } else {
                            touch_release();
                        }
                        continue;
                    }
                    if (type == EV_SYN_ && code == 0) {   // SYN_REPORT: empty frame while down = lift (type-A)
                        if (g_touch_down && !g_touch_saw_pos) touch_release();
                        g_touch_saw_pos = false;
                        continue;
                    }
                }

                // ── Hold/lock SWITCH ── sustained state, both edges (val 1 = locked, 0 = off).
                // Reported as EV_KEY or EV_SW depending on the unit; we match by the code the keymap
                // labels CINDER_BTN_HOLD. The switch is the ONLY thing that unlocks (Power just
                // toggles the screen). Default keymap doesn't know the code — drop `<code> 12` into
                // /contents/cinder_keymap.conf from the dev keycode log.
                if ((type == EV_KEY_ || type == EV_SW_) && code < keymap_size()
                        && g_keymap[code] == CINDER_BTN_HOLD) {
                    cinder_set_hold(val ? 1 : 0);
                    g_held = (val != 0);     // the auto-wake path needs it (pocket-safety, below)
                    g_last_input_ms = now_ms();   // flicking the switch is deliberate activity
                    g_vol_btn = -1;          // locking mid-hold must not keep ramping
                    continue;
                }

                // ── Buttons ──
                // The volume rocker is the one button whose RELEASE means something: it ends the
                // synthesized ramp (see vol_repeat_tick). Checked before the release filter below.
                if (type == EV_KEY_ && val == 0 && code < keymap_size()
                        && (g_keymap[code] == CINDER_BTN_VOLUP
                            || g_keymap[code] == CINDER_BTN_VOLDOWN)) {
                    if (g_vol_btn == g_keymap[code]) g_vol_btn = -1;
                    continue;
                }
                // POWER RELEASE — the second button whose release means something. A press that
                // was NOT long enough to open the menu becomes the screen toggle here, on the way
                // up. Doing it on the way up is the whole mechanism: on the way down we cannot yet
                // know whether this is a tap or a hold.
                if (type == EV_KEY_ && val == 0 && code < keymap_size()
                        && g_keymap[code] == CINDER_BTN_POWER) {
                    bool was_down = g_power_down_ms != 0, spent = g_power_consumed;
                    // Was the toggle ALREADY deferred to the release when this press went down?
                    // On the very first Power press of a boot it was not — the press itself
                    // toggled the screen (the fallback below), so toggling again here would undo
                    // it and Power would appear dead exactly once per boot.
                    bool deferred = g_power_hold_ok;
                    g_power_hold_ok = true;   // this unit DOES report the release — see below
#ifdef CINDER_DEV
                    {
                        char m[80];
                        std::snprintf(m, sizeof m, "input: POWER release after %ld ms (spent=%d)",
                                      was_down ? now_ms() - g_power_down_ms : -1L, (int)spent);
                        clog_(m);
                    }
#endif
                    g_power_down_ms = 0; g_power_consumed = false;
                    if (deferred && was_down && !spent) {
                        int act = cinder_input(CINDER_BTN_POWER);   // -> CINDER_ACT_SLEEP
                        if (act != CINDER_ACT_NONE) carry_out(act);
                    }
                    continue;
                }
                if (type != EV_KEY_ || val == 0) continue; // releases never act
                int kc = code;
                // Key REPEATS (val=2) only ramp the volume rocker; for everything else a held
                // button is ONE action (a held FF must not machine-gun track skips).
                if (val == 2 && kc < keymap_size()
                        && g_keymap[kc] != CINDER_BTN_VOLUP && g_keymap[kc] != CINDER_BTN_VOLDOWN)
                    continue;
#ifdef CINDER_DEV
                // DEV CHANNEL: log every key code (mapped or not) so the physical-button → keycode map
                // can be read straight from cinderhome.log — press each button and watch the log.
                if (val == 1) {
                    char m[64];
                    std::snprintf(m, sizeof m, "input: KEY code=%d (0x%x) -> btn=%d", kc, kc,
                                  (kc >= 0 && kc < keymap_size()) ? g_keymap[kc] : -2);
                    clog_(m);
                }
#endif
                if (kc < 0 || kc >= keymap_size()) continue;
                int btn = g_keymap[kc];
                if (btn < 0) continue;
                // Any mapped key press counts as activity for the idle blank. A key ALSO wakes an
                // idle blank — and unlike a touch it is still delivered: the physical buttons do
                // the same thing whether or not you can see the screen (transport, volume), so
                // swallowing them would make the first press after an idle blank feel broken.
                // Power is the exception and needs no special case: cinder_input maps it to
                // CINDER_ACT_SLEEP -> screen_toggle, which takes ownership of the panel state.
                if (val == 1) {
                    g_last_input_ms = now_ms();
                    if (g_screen_auto_off && btn != CINDER_BTN_POWER) screen_auto_wake();
                }
                if (btn == CINDER_BTN_VOLUP || btn == CINDER_BTN_VOLDOWN) {
                    if (val == 1) { g_vol_btn = btn; g_vol_down_ms = now_ms(); }
                    g_vol_last_ms = now_ms();   // also swallows a kernel repeat's slot
                }
                // POWER PRESS starts the hold clock and does NOTHING ELSE. The screen toggle now
                // happens on the release (above), and the menu on the timer (power_hold_tick), so
                // acting here as well would fire both. Kernel repeats are ignored — the clock is
                // already running and re-arming it would push the hold threshold out forever.
                if (btn == CINDER_BTN_POWER) {
                    if (val == 1 && g_power_down_ms == 0) {
                        g_power_down_ms = now_ms();
                        g_power_consumed = false;
                    }
                    // Until a release has been seen on this unit, fall through and toggle the
                    // screen on the PRESS exactly as before — see g_power_hold_ok. This costs the
                    // hold gesture on the first press of a boot and nothing else.
                    if (g_power_hold_ok) continue;
                }
                int act = cinder_input(btn);
                if (act != CINDER_ACT_NONE) carry_out(act);
            }
            if (n < (ssize_t)sizeof evs) break; // drained this node
        }
    }
    // Held rocker: the events are all drained, so anything still down is a genuine hold.
    vol_repeat_tick();
    // Same idea for Power: the menu opens on elapsed time, not on an event, so it needs a tick.
    power_hold_tick();
    // ANY input at all cancels a level-0 (backlight off) blank. Checked here — once, after the
    // drain — rather than at each of the seven places that stamp g_last_input_ms, so no input
    // path can be added later that forgets to provide the escape. It reads a single global and
    // returns immediately unless the blank is actually armed.
    if (g_ev_total != ev_before) brightness_wake_on_input();
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

// Is the battery being charged? Read alongside the level, because every low-battery decision below
// is wrong on a charger: 3% and climbing is a device you just plugged in, not one about to die.
// "Full" counts as charging — that is what a topped-up device on a cable reports.
static bool battery_charging() {
    static const char* paths[] = {
        "/sys/class/power_supply/battery/status",
        "/sys/class/power_supply/Battery/status",
        "/sys/class/power_supply/bat/status",
    };
    for (const char* p : paths) {
        FILE* f = std::fopen(p, "r");
        if (!f) continue;
        char buf[24] = {0};
        size_t got = std::fread(buf, 1, sizeof buf - 1, f);
        std::fclose(f);
        if (got == 0) continue;
        // Anything that is not clearly "Discharging" is treated as charging. An unreadable or
        // unfamiliar status must NOT be read as "on battery" — that direction ends in a shutdown
        // nobody asked for, and this file's job is to prevent surprise power loss, not cause it.
        return std::strncmp(buf, "Discharging", 11) != 0;
    }
    return true;   // no status node at all: assume charging, i.e. never auto-shut-down
}

// Low battery: warn once, then shut down cleanly before the hardware browns out.
//
// This is not a power SAVING — it is protection. `/contents` is vfat with no journal, it holds the
// music library, the settings and the scrobbler log, and Cinder writes to it while playing. A flat
// battery mid-write is a corruption path on the exact partition whose corruption bricked this device
// on 2026-07-26. Sony has LowBatteryAlert and a critical shutdown; Cinder had neither.
//
// Called from the ~10 s battery gauge, so the level is already in hand.
static void battery_guard(int pct) {
    static bool warned = false;
    const bool charging = battery_charging();
    if (charging || pct > 10) {
        warned = false;              // re-arm, so the next discharge warns again
        return;
    }
    if (pct > 3) {
        if (!warned) {
            warned = true;
            char m[96];
            std::snprintf(m, sizeof m, "Battery %d%% — charge soon", pct);
            cinder_toast(m);
            clog_(m);
        }
        return;
    }
    // CRITICAL. Go down deliberately while there is still enough charge to finish the writes,
    // rather than being cut off mid-write. power_action already syncs before handing over.
    char m[128];
    std::snprintf(m, sizeof m, "power: battery %d%% and discharging — shutting down to protect /contents", pct);
    clog_(m);
    cinder_toast("Battery critical — shutting down");
    cinder_render_tick();            // let the toast reach the glass before the panel goes
    power_action(false);
}

// Real internal-storage usage for the Settings ▸ Storage row, via statvfs (read-only — no Sony
// service). Formats "used / total GB" and pushes it. 64-bit math (f_blocks*frsize overflows 32-bit
// at this capacity). Tries the music mount first, then sensible fallbacks.
void report_storage() {
    static const char* mounts[] = { "/contents", "/mnt/media0", "/data", "/" };
    for (const char* m : mounts) {
        struct statvfs st;
        if (statvfs(m, &st) != 0) continue;
        unsigned long frsize = st.f_frsize ? st.f_frsize : st.f_bsize;
        unsigned long long total = (unsigned long long)st.f_blocks * frsize;
        if (total == 0) continue;
        unsigned long long used = (unsigned long long)(st.f_blocks - st.f_bfree) * frsize;
        const double g = 1024.0 * 1024.0 * 1024.0;
        char buf[48];
        std::snprintf(buf, sizeof buf, "%.1f / %.0f GB", (double)used / g, (double)total / g);
        cinder_set_storage(buf);
        return;
    }
}

// Poll the now-playing URI; on change, push it to the UI (cinder-ffi resolves title/artist/
// codec from the library DB). Then push the REAL position/duration/state from the
// PlayEventListener every tick — that is what drives the progress bar, and unlike the old local
// estimate it survives seeks, mid-track starts and wrong tag durations.
// Set when the user has just started something, to make the very next poll ask the service
// immediately instead of waiting for the listener to tick. See g_np_poll_now's use below.
bool g_np_poll_now = false;  // fwd-declared above
// Makes the ~1 Hz housekeeping block run on the NEXT frame instead of waiting out its interval.
// Set alongside g_np_poll_now so a freshly-started track reaches the screen in one frame rather
// than up to a second later.
bool g_house_due = false;    // fwd-declared above

// How fast the audio shim's Framework::Pump() loop spins. It exists to deliver binder replies, so
// the right rate is "as slow as the thing waiting on a reply can tolerate".
//   awake                 20 ms — the UI shows a moving progress bar and answers presses.
//   dark, just pressed   100 ms — a transport press while the screen is off still wants its reply
//                                 promptly; bounded by TRANSPORT_GRACE_MS.
//   dark, otherwise      250 ms — nothing is waiting on it faster than that. Measured 2026-08-11:
//                                 the pump thread was the joint-largest source of cinder-home's
//                                 standby wakeups (10.2 ctxt/s of 20.9 total), and it is the half
//                                 the render loop's poll() change does not touch.
//
// DARK + PLAYING USED TO BE 100 ms, and that was the wrong rate for the longest-lived state a music
// player has — screen off, in a pocket, playing for hours. The pump exists only to deliver binder
// REPLIES to our own client calls; the audio is decoded by SoundServiceFw and does not ride on it at
// all. What does ride on it is the position callback (~1/s) and the track boundary, both consumed by
// 1 Hz housekeeping — so a 250 ms delivery window is invisible to every consumer, and it drops 6
// wakeups a second in the state the device spends most of its life in.
static void apply_pump_interval() {
    // g_playing is INTENT and it starts `true`, so on a boot where nothing has been played it
    // stays true forever and the pump never backed off at all (measured: 10.3 ctxt/s on a dark,
    // idle device that had played nothing). Hence the grace window rather than the intent flag.
    const bool just_pressed = now_ms() - g_transport_at < TRANSPORT_GRACE_MS;
    cinder_audio_pump_set_interval(g_screen_on ? 20 : (just_pressed ? 100 : 250));
}

void poll_now_playing() {
    static char last[1024];
    static unsigned last_events = 0;
    // The URI read is a BINDER ROUND TRIP into hagodaemon (GetCurrentStatus, plus a std::string
    // copied out) and it ran every single second whether or not anything was playing. The
    // PlayEventListener already tells us when something happened: onPlayTimeUpdated fires ~1x/sec
    // while playing and not at all while stopped, and a track change always comes with events. So
    // only ask the service when its callback count has moved. Idle now costs zero IPC; playing is
    // unchanged at one call per second.
    unsigned events = cinder_audio_listener_events();
    // …EXCEPT right after the user pressed play. The listener has not necessarily fired yet at that
    // point, so waiting for it means the screen keeps showing the previous track — for up to a
    // second on the housekeeping tick, plus however long the first callback takes. That delay is
    // most of what "playing a song from the library feels laggy" actually is: the audio starts, and
    // the UI just sits there.
    bool force = g_np_poll_now;
    g_np_poll_now = false;
    if (force || events != last_events) {
        last_events = events;
        char uri[1024];
        int n = cinder_audio_current_uri(uri, sizeof uri);
        if (n > 0 && std::strcmp(uri, last) != 0) {
            std::strncpy(last, uri, sizeof last - 1);
            last[sizeof last - 1] = 0;
            cinder_set_now_playing_uri(uri, 0.0f, g_playing ? 1 : 0, read_battery());
        }
    }
    // The service is the authority on position, and eventually on whether it is really playing —
    // which is how a track ending, or PlayerService pausing for its own reasons, reaches the UI.
    //
    // But cinder_audio_is_playing() is derived from the position having MOVED recently, and that
    // lags a transport command in both directions: just after Play the position hasn't moved yet
    // (~1 s until the next onPlayTimeUpdated), and just after Pause it looks like it moved 2.5 s ago.
    // Adopting it immediately therefore flickered the transport glyph back to the old state right
    // after every press. So for a short grace window our own intent wins; after that the service's
    // view takes over.
    int cur = -1, tot = -1;
    if (cinder_audio_position(&cur, &tot)) {
        if (now_ms() - g_transport_at > TRANSPORT_GRACE_MS)
            g_playing = cinder_audio_is_playing() != 0;
        cinder_set_play_position(cur, tot, g_playing ? 1 : 0);
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
    void OnForeground() override     { clog_("app:OnForeground");     render_up(); easel::ApplicationBase::OnForeground(); }
    void OnBackground() override     { clog_("app:OnBackground");     stop_analyzer(); easel::ApplicationBase::OnBackground(); }
    void OnInactivate() override     { clog_("app:OnInactivate");     easel::ApplicationBase::OnInactivate(); }
    void OnFinalize() override       { clog_("app:OnFinalize");       stop_analyzer(); cinder_render_shutdown(); easel::ApplicationBase::OnFinalize(); }
    void StopBootAnimation() override{ clog_("app:StopBootAnimation");easel::ApplicationBase::StopBootAnimation(); }
};

// ── the render+input worker (Option-B) ──────────────────────────────────────────────────────
// Runs everything the (blocked) easel pump would have: paint, stop the boot-anim overlay, the
// deferred DB/PlayerService/adb init, touch + button input, and periodic housekeeping — at ~60fps
// on its own thread. Mirrors the (now-dead) `pump` lambda body; SIGALRM is delivered to THIS thread.
void* render_driver(void*) {
    // This thread owns the watchdog now — UNBLOCK SIGALRM (render_up blocked it on the stuck main
    // thread). The per-frame alarm(8) and run_guarded (in deferred_up / carry_out) fire here.
    sigset_t s; sigemptyset(&s); sigaddset(&s, SIGALRM);
    pthread_sigmask(SIG_UNBLOCK, &s, nullptr);
    long n = 0;
    bool first_painted = false, boot_anim_stopped = false;
    while (g_pump_ticker_run) {
        if (!g_render_ready) { usleep(16000); continue; }

        // FORCED REPAINTS: the renderer is dirty-flag gated, so once our first blit lands it would
        // never paint again until state changes — and anything an external process scribbled on the
        // framebuffer after that blit (the boot animation's last video frame survives its kill)
        // would sit on screen forever. Repaint every frame for the first ~10 s, then 1×/s for life.
        // Forced repaint, as insurance against an EXTERNAL process scribbling into the framebuffer
        // pages (the boot animation's last video frame survives its kill, and the dirty-flag
        // renderer would otherwise never paint over it).
        //
        // Dense during the first ~10 s, which is the only window where anything else is actually
        // writing to fb0 — then RARE. It used to stay at 1 Hz forever, which meant a full UI raster
        // plus a 4.6 MB blit every single second on a completely static screen, for the life of the
        // process. Nothing writes to fb0 after boot, so that was expensive insurance against an
        // event that can no longer happen; 5 s keeps the safety net at a twentieth of the cost.
        // (While the panel is dark the paint is skipped entirely, so this costs nothing there.)
        static long last_forced_ms = 0;
        if (n < 600) {
            cinder_force_dirty();
            last_forced_ms = now_ms();
        } else if (now_ms() - last_forced_ms >= 5000) {
            cinder_force_dirty();
            last_forced_ms = now_ms();
        }

        long frame_start = now_ms();

        // ── INPUT IS READ BEFORE THE PAINT ────────────────────────────────────────────────────
        // It used to run *after* cinder_render_tick(), and that cost a whole frame on every single
        // interaction: a tap read at the END of frame N could only reach the glass in frame N+1,
        // and a drag always painted the finger's PREVIOUS position. On device a scrolling frame
        // measures ~31 ms (cinder-probe --bench), so that was ~31 ms of pure, avoidable lag on
        // everything you touch — the "clunky" feel in Settings, which has no album art to blame.
        // Reading first means the frame we are about to paint already reflects the finger.
        //
        // Gated on g_deferred_done to preserve the OLD ordering exactly: the deferred-init block
        // below `continue`s before the input call ever ran, so input has never been pumped before
        // Sony's services are up and must not start now (carry_out would drive uninitialised audio).
        if (g_deferred_done) {
            if (!g_input_started) { input_open(); g_input_started = true; }
            alarm(8); input_pump(); alarm(0);  // touch + buttons -> navigator -> actions -> carry_out
            volume_flush();                    // trailing write of a coalesced volume ramp

            // Headphones can connect or drop without Cinder doing anything — the user powers them
            // on, or walks out of range, and the sink the volume rocker should be driving changes
            // underneath us. Poll it slowly so the rocker follows. It has to be ahead of the press
            // rather than resolved during it: the UI moves its level when the button goes down, so
            // a route learned only at carry_out time would always be one press stale.
            // 3 s is chosen against the cost — this is a synchronous IPC round trip on the render
            // thread, and it is guarded like every other one.
            //
            // …and back off hard when the RADIO IS OFF. Nothing can connect while it is down, and
            // every path that powers it up (CINDER_ACT_BT_TOGGLE, deferred_up) refreshes the route
            // itself — so a 3 s IPC round trip there was buying nothing at all, forever, for the
            // majority of users who leave Bluetooth off. `g_bt_radio_seen_up` is the last answer this
            // poll got, so the first poll after a power-up snaps straight back to 3 s.
            //
            // …and the RADIO NOW TELLS US, so the poll is a safety net rather than the mechanism.
            // OnNotifyBtStatus / OnNotifyAclStateChanged / OnNotifyDisconnectEnd fire on the
            // framework looper the moment the link moves and set g_bt_state_dirty; this reads the
            // route on the very next frame instead of up to 3 s (or 15 s from a radio that last
            // read down) later. That latency is the whole of why stock felt faster to connect —
            // the link itself is the MTK stack's either way.
            static long last_route_ms = 0;
            const long route_every = g_bt_radio_seen_up ? 3000 : 15000;
            if (g_bt_state_dirty || now_ms() - last_route_ms >= route_every) {
                const bool by_event = g_bt_state_dirty != 0;
                g_bt_state_dirty = 0;
                last_route_ms = now_ms();
                run_guarded("loop: BT route poll", 4, refresh_bt_route);
                // A connect edge also wants the device NAME, which refresh_bt_route only re-asks for
                // on its own next tick. Logged once per event so a slow link is visible in the log
                // rather than being guessed at.
                if (by_event) clog_("bt: link state changed (listener) — route re-read this frame");
            }
            // Scan results. The listener runs on the framework looper and only appends to a guarded
            // list; THIS is where they reach the UI, on the thread that owns it.
            if (g_bt_found_dirty) {
                run_guarded("loop: BT found list", 4, flush_bt_found);
            }
            // The sink told us its real level (its own buttons, or an echo of ours). Adopting it
            // is the whole fix for the UI and the headphones disagreeing.
            if (g_bt_vol_notify >= 0 && bt_vol_drain_notify()) cinder_force_dirty();
            // The USB host started or changed its stream. Start() can only succeed once a format
            // exists, so this — not the moment the user flipped the switch — is when the DAC's
            // render path actually opens.
            if (g_uac_format_notify) run_guarded("loop: USB-DAC format", 8, uac_drain_format);
            // …and the backstop, for a host that was already streaming when DAC mode was entered
            // (no change, so no notify) or a listener that failed to register. Returns immediately
            // once a real format has been opened, or when not in DAC mode — and is throttled inside
            // (200 ms bridging, 1 s otherwise) for the frames where neither of those is true.
            if (cinder_get_usb_dac()) run_guarded("loop: USB-DAC status", 8, uac_poll_status);
            // REFERENCE DUMP. When the DAC renders to the jack, Sony's own service is holding the
            // same capture substream our bridge fails on — so this is the known-good configuration,
            // printed once per session. Diffing it against the bridge's "after set_params" dump is
            // the shortest path to whatever we are setting differently. One shot, and only in the
            // non-bridging case, where it costs a couple of small /proc reads.
            if (g_uac_playing && !cinder_get_bt_route() && !g_uac_ref_dumped) {
                g_uac_ref_dumped = true;
                run_guarded("loop: USB-DAC reference dump", 6, []() {
                    char dev[32] = {0};
                    if (ldac_find_capture(dev, sizeof dev)) ldac_dump_pcm(dev, "SONY-REFERENCE");
                });
            }
            // What A2DP actually agreed on. Called every frame but THROTTLED INSIDE (2 s, or at once
            // on a connect / codec write) — it is a synchronous IPC round trip, not a scalar read,
            // and the earlier "self-limiting" claim here confused suppressing the log with
            // suppressing the call.
            if (cinder_get_bt_route()) run_guarded("loop: BT sound status", 6, bt_poll_sound_status);
            // A dropped link never came back on its own. Guarded like every other pst call, and
            // cheap in the common cases: connected, or radio off, is two boolean tests.
            run_guarded("loop: BT reconnect", 8, bt_reconnect_tick);
            // THE OUTPUT ROUTE CAN CHANGE WITHOUT THE TOGGLE. Headphones get switched off, run flat,
            // or walk out of range mid-session. `apply_usb_dac` only runs when the user flips the
            // switch, so before this the session was stuck on whichever route it started with: turn
            // the headphones off during USB-DAC and the bridge kept the capture with nowhere to send
            // it while the jack stayed silent, until the mode was toggled off and on.
            if (g_uac_route >= 0 && cinder_get_usb_dac()) {
                const int now = cinder_get_bt_route() ? 1 : 0;
                if (now != g_uac_route) {
                    g_uac_route = now;
                    if (now == 1) {
                        clog_("usb-dac: headphones connected mid-session — moving to the LDAC bridge");
                        run_guarded("loop: DAC route -> LDAC", 8, []() {
                            uac_render(false);      // let go of the capture before the bridge wants it
                            g_uac_playing = false;
                            ldac_start();
                        });
                    } else {
                        clog_("usb-dac: headphones gone — falling back to the 3.5 mm jack");
                        run_guarded("loop: DAC route -> jack", 8, []() { ldac_stop(); });
                        // ldac_stop() only asks; the thread closes the PCM on its own schedule, and
                        // opening the capture before it lets go is the -EBUSY case. So defer.
                        g_uac_want_jack = true;
                    }
                }
            }
            // The bridge has let go of the capture, so Sony's renderer can have it.
            if (g_uac_want_jack && !g_ldac_alive) {
                g_uac_want_jack = false;
                run_guarded("loop: DAC jack handover", 8, []() {
                    g_uac_playing = false;
                    g_uac_ref_dumped = false;
                    if (!uac_render(true))
                        clog_("usb-dac: jack handover — Start() failed, waiting for the next format "
                              "notify");
                });
            }
            // NFC tap-to-pair. The reader is armed whenever the radio is on and stopped when it is
            // off — a tap is only useful if there is a Bluetooth stack to hand the address to, and
            // leaving the NFC controller running with the radio down would burn power for nothing.
            // Both calls are cheap no-ops once they have taken effect.
            {
                const bool want_nfc = cinder_get_bt_on() != 0;
                if (want_nfc != g_nfc_running) {
                    if (!want_nfc) {
                        run_guarded("loop: NFC disarm", 8, nfc_stop);
                    } else if (g_nfc_arm_tries < 5) {
                        // BOUNDED. `nfc_start()` leaves g_nfc_running false when it fails, and the
                        // test above is "want != have" — so a permanent failure (no libNfcService,
                        // a service that refuses every mode) would re-run three IPC calls on EVERY
                        // frame, which is the poll-storm shape that caused the audio stutter in
                        // task #26. Five attempts is enough to ride out a service that is still
                        // coming up at boot; after that it stays off until the radio is toggled.
                        g_nfc_arm_tries++;
                        run_guarded("loop: NFC arm", 8, []() { nfc_start(); });
                        if (!g_nfc_running && g_nfc_arm_tries >= 5)
                            clog_("nfc: reader would not start — tap-to-pair off for this session");
                    }
                }
                // A radio toggle is the user asking again, so it re-earns the attempts.
                if (!want_nfc) g_nfc_arm_tries = 0;
            }
            // Something was tapped. The callback ran on the framework looper and only copied the
            // address out; the Pairing call happens here, on the thread that owns the clients.
            if (g_nfc_tapped) {
                g_nfc_tapped = 0;
                run_guarded("loop: NFC tap", 8, nfc_service_tap);
            }
            // A pairing prompt arrived on the looper; show it from the thread that owns the UI.
            if (g_bt_prompt_dirty) {
                run_guarded("loop: BT pairing prompt", 4, flush_bt_prompt);
            }
            // A pairing finished (OnNotifyPairingComplete). Re-read the paired list — the new device
            // belongs in it now — and end the scan, since what you came to do is done.
            if (g_bt_pairing_done) {
                g_bt_pairing_done = 0;
                run_guarded("loop: BT pairing complete", 8, []() {
                    clog_("bt-scan: pairing complete — scan off, waiting for the link key to appear");
                    cinder_set_bt_scanning(0);
                    apply_bt_scan();
                    refresh_bt_paired();
                    // The callback runs ahead of the pairing table, so schedule re-reads instead of
                    // trusting that one. Stops early the moment the new address shows up.
                    g_bt_paired_recheck_left = 8;
                    g_bt_paired_recheck_at   = now_ms() + 700;
                });
            }
            // Waiting for a just-paired device to appear in GetPairedDeviceInfo. ~700 ms apart, up to
            // 8 tries (~5.5 s), then give up quietly — the list is also re-read whenever the screen is
            // opened, so a slow radio costs a stale row and not a wrong one.
            if (g_bt_paired_recheck_left > 0 && now_ms() >= g_bt_paired_recheck_at) {
                g_bt_paired_recheck_at = now_ms() + 700;
                g_bt_paired_recheck_left--;
                run_guarded("loop: BT paired recheck", 6, []() {
                    refresh_bt_paired();
                    bool there = false;
                    for (size_t i = 0; i < g_bt_paired.size(); i++)
                        if (g_bt_paired[i] == g_bt_pairing_addr) { there = true; break; }
                    if (there) {
                        g_bt_paired_recheck_left = 0;
                        g_bt_pairing_addr.clear();
                        // It is paired now, so it must stop offering "TAP TO PAIR" in the FOUND list.
                        flush_bt_found();
                        clog_("bt-scan: the new device is in the paired list");
                        // A tap-to-pair asked for a link, not just a bond. NOW is the first moment
                        // RequestConnection can work: before the link key is visible the radio has
                        // nothing to connect with. Cleared either way so it fires exactly once.
                        if (!g_nfc_connect_after_pair.empty()) {
                            std::vector<unsigned char> want;
                            want.swap(g_nfc_connect_after_pair);
                            bt_request_connection(want, "nfc: paired, now connecting");
                        }
                    } else if (g_bt_paired_recheck_left == 0) {
                        g_nfc_connect_after_pair.clear();
                        clog_("bt-scan: paired list never showed the new device — leaving it to the "
                              "next refresh rather than inventing a row");
                    }
                });
            }
        }

        // Panel dark => skip the PAINT ONLY. Nobody can see the frame, and with the visualiser
        // running while playing this is a full repaint + 4.6 MB blit every 16 ms — the cost the
        // screen-off timer exists to avoid, so blanking the backlight alone left the win on the
        // table. The forced-dirty calls above still run, so the flag is set when we resume, and
        // both wake paths force a repaint as well.
        //
        // MUST NOT `continue` here. input_pump() is BELOW this point, and so is the housekeeping
        // block: skipping the rest of the iteration would stop reading input while the panel is
        // dark, and then NOTHING could wake it — not a touch, and not the Power button either,
        // since that arrives through input_pump too. A reboot would be the only way out.
        if (g_screen_on) {
            // PER-FRAME WATCHDOG around OUR paint: a real render hang -> _exit -> launcher
            // counter -> stock.
            alarm(8);
            cinder_render_tick();
            alarm(0);
        }
        // "First frame painted" gates on cinder_frames_presented(), NOT on tick returning: the
        // present runs on its own thread now, so tick returns once the frame is SUBMITTED. The
        // health signal (and StopBootAnimation) must mean pixels actually went to the glass —
        // counting submission would re-open the frozen-panel-marked-healthy hole. Costs at most
        // one extra 16 ms loop iteration after boot.
        if (!first_painted && cinder_frames_presented() > 0) {
            first_painted = true;
            g_first_paint_at = std::time(nullptr);   // starts the bad-boot "proven good" clock
            clog_("render_driver: first frame painted (our own loop)");
            // Detached: deferred_up() blocks THIS thread, so the counter reset needs its own timer.
            { pthread_t t; if (pthread_create(&t, nullptr, healthy_timer, nullptr) == 0) pthread_detach(t); }
            // Kill the boot animation at first paint. Timing does NOT matter (all the earlier
            // "coin flip" theories were wrong): disasm shows icx_bootanimation installs NO signal
            // handlers at all — SIGTERM just drops it dead at any point. The frozen-boot-image
            // failure was never about its cleanup; mtkfb only pushes pixels to the panel on a
            // FBIOPUT_VSCREENINFO(FB_ACTIVATE_FORCE) trigger, and the anim was the only process
            // issuing them. Our renderer now flips after every blit (cinder-ffi Framebuffer::blit),
            // so the instant the anim dies OUR next frame owns the glass — deterministically.
            if (g_app && !boot_anim_stopped) {
                g_app->StopBootAnimation();
                boot_anim_stopped = true;
                clog_("render_driver: StopBootAnimation() (first paint; our flips own the glass now)");
            }
        }

        // Paint continuously for the first ~0.5s BEFORE the slow deferred init runs (which blocks
        // this thread for several seconds). Re-issue StopBootAnimation afterward as insurance
        // against a race with init (re)starting bootanimation.
        if (!g_deferred_done) {
            if (n < 30) { ++n; usleep(16000); continue; }   // warm-up paints first
            deferred_up();
            if (g_app) g_app->StopBootAnimation();          // re-kill in case init respawned it
            ++n; usleep(16000); continue;
        }
        // Straggler sweep: if the anim somehow survived (or respawned), re-kill at ~15 s and
        // ~30 s after render start. killall on a dead process is a harmless no-op.
        if (n == 900 || n == 1800) {
            if (g_app) g_app->StopBootAnimation();
        }

        // (input_pump + volume_flush now run at the TOP of the loop, before the paint — see there)
        // ~1x/sec housekeeping, paced by the WALL CLOCK rather than an iteration count. It used to
        // be `n % 60`, which silently assumed the loop always runs at 60 Hz — no longer true now
        // that a dark panel drops it to 10 Hz, where `n % 60` would mean once every SIX seconds and
        // would have delayed the sleep timer and the USB-host debounce by that much.
        static long last_house_ms = 0;
        long house_now = now_ms();
        if (g_house_due || house_now - last_house_ms >= 1000) {
            g_house_due = false;
            last_house_ms = house_now;
            cinder_clock_tick();
            run_guarded("pump: poll now-playing", 8, poll_now_playing);
            // Scrobble writes /contents — skip while it's handed to the PC (stale mountpoint).
            if (!g_msc_active) cinder_scrobble_tick(g_playing ? 1 : 0);
            // g_playing changes on its own (a track ends, the sleep timer pauses), and the screen
            // transitions are not the only way into "dark + idle" — so re-evaluate here too.
            apply_pump_interval();
            if (cinder_sleep_should_pause()) {
                clog_("sleep timer expired -> pausing");
                set_transport(false);
                run_guarded("pump: sleep-timer pause", 6, []() { cinder_audio_pause(); });
            }
            // Idle screen-off. Only ever blanks the panel; playback and every background job keep
            // running (the app renders regardless — same as the Power-button blank). Never fires
            // while already dark, and never while the USB-MSC modal is up: that screen is the only
            // indication the volume is handed to the PC, so blanking it would be actively confusing.
            // Nor while a confirmation dialog is up: blanking a "Power off?" prompt out from under
            // the finger about to answer it would leave the device dark with a live modal on it.
            {
                int idle_s = cinder_get_screen_off_s();
                if (idle_s > 0 && g_screen_on && !g_msc_active && !cinder_modal_open() &&
                    now_ms() - g_last_input_ms >= (long)idle_s * 1000) {
                    screen_auto_off();
                }
            }
            // AUTO POWER-OFF. Sony has this (sid_4118) and Cinder did not, so a paused device with
            // the screen dark ran until the battery was flat — the largest battery item in the
            // 2026-08-16 audit. Off by default; the row has to be set before this can ever fire.
            //
            // Four guards, and each one is a way this could otherwise switch the device off in
            // somebody's hand:
            //   * NOT WHILE PLAYING. Idle means idle. Music playing with the screen off for an hour
            //     is the normal way this device is used, and cutting it off would be a bug, not a
            //     saving. `g_playing` is only our intent, so the SERVICE has to agree too.
            //   * not during a USB-MSC session — the PC is mid-transfer and owns /contents.
            //   * not over a modal — a "Power off?" prompt answering itself is absurd.
            //   * not while charging, for the same reason the low-battery guard checks it: a device
            //     on a cable is parked on a desk, and shutting it down there just means it is off
            //     when you come back to it.
            {
                const int off_min = cinder_get_auto_off_min();
                const bool audible = g_playing && cinder_audio_is_playing() != 0;
                if (off_min > 0 && !audible && !g_msc_active && !cinder_modal_open() &&
                    !battery_charging() &&
                    now_ms() - g_last_input_ms >= (long)off_min * 60000L) {
                    char m[128];
                    std::snprintf(m, sizeof m,
                                  "power: %d min idle and nothing playing — auto power off", off_min);
                    clog_(m);
                    power_action(false);
                }
            }
            // USB mass-storage is fully automatic — no menu dive:
            //  • NOT in MSC + a PC data-host appears (debounced ~2 s so enumeration flicker doesn't
            //    bounce us in) → raise the modal and hand /contents to the PC. This MUST use
            //    usb_data_host() (gadget state == CONFIGURED), not usb_connected(): the latter also
            //    reads the power-supply nodes, which a dumb wall charger sets, and entering MSC on a
            //    charger unmounts the user's library mid-charge.
            //  • IN MSC + the cable is pulled → inject Back so the modal pops AND the navigator emits
            //    ExitUsbMsc (single exit path: remount /contents + restore the USB mode + log).
            if (g_msc_active) {
                g_usb_hi = 0;
                if (usb_connected()) {
                    g_msc_seen_usb = true;
                    // A host tool (flash.sh's unmount, Windows "safely remove") EJECTS the medium
                    // mid-session — SCSI START STOP UNIT clears lun/file, so the NEXT host op sees a
                    // reader with no medium (the "worked once, then 0B" symptom across back-to-back
                    // flash.sh calls). /contents is still unmounted here, so re-insert it: ensure_msc_lun
                    // is a no-op while the LUN stays backed and re-binds the instant it goes empty.
                    ensure_msc_lun();
                }
                else if (g_msc_seen_usb) {
                    int act = cinder_input(CINDER_BTN_BACK);
                    if (act != CINDER_ACT_NONE) carry_out(act);
                }
            } else if (usb_data_host() && !dev_skip_auto_msc() && !cinder_get_usb_dac()
                       && !g_msc_user_off) {
                // USB-DAC EXCLUDES auto-MSC, and that is why plain DAC mode never worked.
                // `sys.sony.config=uac` reconfigures the gadget to `audio_func,adb`, which
                // enumerates — so `usb_data_host()` goes true within ~2 s and this branch handed
                // /contents to the PC instead, flipping the property to `msc` and then back to
                // `adb` on exit. Measured 2026-07-29: dac-on reported `sys.usb.state=audio_func,adb`
                // and twelve seconds later the device was on `adb` with `functions=mass_storage,adb`.
                // The user asked for a sound card; offering a disk drive instead is not a fallback.
                if (++g_usb_hi >= 2) {
                    clog_("usb-msc: PC host detected — auto-entering mass storage");
                    cinder_show_usb_storage();            // UI reflects the handoff (same modal as the tap)
                    cinder_render_tick();                 // PAINT the modal now — enter_usb_msc blocks ~8 s
                    carry_out(CINDER_ACT_ENTER_USB_MSC);  // flip gadget + hand /contents to the PC
                }
            } else {
                g_usb_hi = 0;
                // Cable actually out, for long enough that it is not a re-enumeration: the
                // refusal has served its purpose, so the next connection behaves normally again.
                if (g_msc_user_off) {
                    if (usb_connected()) {
                        g_msc_off_gone = 0;
                    } else if (++g_msc_off_gone >= MSC_OFF_RELEASE_TICKS) {
                        g_msc_user_off = false;
                        g_msc_off_gone = 0;
                        clog_("usb-msc: cable out — auto-MSC re-armed");
                    }
                }
            }
            // QUEUE FLUSH — the pending user-queue edit, applied at a track boundary. cinder-ffi
            // only raises this when the track has just changed, which is the one moment
            // SetTrackSequence is free: the position is already ~0, so the reset it causes is
            // invisible. Applying it any other time restarts the music (measured on device).
            if (cinder_take_queue_flush()) {
                run_guarded("queue: flush at track boundary", 10, []() {
                    play_pending_sequence("queue", true);
                });
            }
            viz_analyzer_tick();              // analyzer runs only while its output is visible
            mark_healthy_maybe();             // clear the bad-boot counter once proven good
            // Screenshot-on-demand: drop /tmp/cinder_screenshot.req and the next frame is written
            // to /tmp/cinder_screen.png. Same polled-flag idiom as ldac_on above (no new IPC
            // primitive — the safety model here is best-effort polled file I/O).
            //   /tmp, NOT /contents: /tmp is tmpfs, so it survives USB-MSC (which unmounts
            //   /contents and hands it to the PC), needs no sync (/contents is vfat — unsynced
            //   writes are lost), and costs no eMMC wear for a throwaway debug artifact.
            //   Also accept the /contents trigger, for setting it over USB-MSC with no adb.
#ifdef CINDER_DEV
            // DEV PROBE: `touch /tmp/cinder_reissue.req` re-hands PlayerService the current
            // sequence WHILE IT PLAYS, changing nothing else. It answers whether an Apple-style
            // "Play Next" can alter the queue without interrupting the track — PlayerService has no
            // insert, so a fresh SetTrackSequence is the only way to change a queue, and if that is
            // not transparent the feature has to defer inserts to a track boundary instead.
            //
            // It has to run HERE, inside the app, rather than from cinder-probe: SoundService
            // allows exactly one track of a type, cinder-home holds it, and a probe run alongside
            // therefore never gets audio to interrupt. The position either side is logged so the
            // answer is in the log rather than in someone's judgement of what they heard.
            if (::access("/tmp/cinder_reissue.req", F_OK) == 0) {
                ::unlink("/tmp/cinder_reissue.req");
                int c0 = -1, t0 = -1;
                cinder_audio_position(&c0, &t0);
                int playing0 = cinder_audio_is_playing();
                run_guarded("reissue probe", 8, []() { cinder_audio_reissue_sequence(1); });
                usleep(1500000);
                int c1 = -1, t1 = -1;
                cinder_audio_position(&c1, &t1);
                std::fprintf(stderr,
                    "[cinder-home] reissue: before pos=%d playing=%d | after 1.5s pos=%d playing=%d"
                    " => %s\n",
                    c0, playing0, c1, cinder_audio_is_playing(),
                    (c1 > c0) ? "CONTINUED (transparent — Play Next is buildable)"
                              : "INTERRUPTED (inserts must wait for a track boundary)");
                std::fflush(stderr);
            }
            // DEV PROBE: `echo "<origin> <ms>" > /tmp/cinder_seek.req` seeks the PLAYING track and
            // logs where it actually landed. Drag-to-seek moves the bar but the audio does not
            // follow (reported 2026-07-28), and there is exactly one unverified value in that path:
            // media_origin_t. The enum is a guess — playerservice_abi.hpp says "Begin = 0,
            // Current = 1 ... calibrate exact values on device" — and SeekTime is void, so a
            // rejected request looks identical to an accepted one from the caller's side.
            //
            // Same reasoning as the reissue probe for why it lives in the app: SoundService allows
            // one track per type and cinder-home holds it, so a seek from cinder-probe would have
            // nothing playing to seek within. Sweeping origin 0 then 1 against a known target and
            // reading the resulting position off the log settles it in two runs.
            char seekreq[64];
            if (take_req("/tmp/cinder_seek.req", seekreq, sizeof seekreq)) {
                int origin = 0, mode = 0; long target = 60000;
                std::sscanf(seekreq, "%d %ld %d", &origin, &target, &mode);
                int c0 = -1, t0 = -1;
                cinder_audio_position(&c0, &t0);
                // run_guarded takes a plain function pointer (it must survive a longjmp out of a
                // signal handler), so the probe's inputs travel in statics rather than a capture.
                // Single-threaded here — this block runs on the render thread only.
                //
                // MODE is the real question now. Origin 0..11, milliseconds, seconds and an offset
                // of ZERO were all rejected with the same "Bad parameter. ignored" from
                // MediaEnginePlayer.cc:221 — and a seek to the start of the track cannot be a bad
                // parameter, so the parameters were never it. The stock app wraps every seek in a
                // state machine (dmpapp::AudioPlayerImplStateSeek carries a PlayState alongside the
                // origin and offset), which says the engine wants to be in a particular state
                // first. These are the two ways to put it there, both already in the shim:
                //   0 = seek outright (the original, known-rejected path — the control)
                //   1 = Suspend -> seek -> Resume   (engine-level pause: OMX Executing -> Pause)
                //   2 = Pause   -> seek -> Play     (transport-level, via ChangePlayState)
                //   3 = cinder_audio_seek_ms        (the SHIPPING path, so what gets tested is
                //                                    what runs — no 200 ms settle around the
                //                                    state change, because drag-to-seek releases
                //                                    on the render thread and cannot block there)
                // ANSWER (2026-07-28): mode 2 lands, modes 0 and 1 do not. Mode 3 exists to prove
                // the production path lands too, without the probe's artificial settles.
                static int s_origin, s_ms, s_rc, s_mode;
                s_origin = origin; s_ms = (int)target; s_rc = -99; s_mode = mode;
                run_guarded("seek probe", 8, []() {
                    if (s_mode == 3) { s_rc = cinder_audio_seek_ms(s_ms); return; }
                    if (s_mode == 1) cinder_audio_suspend();
                    else if (s_mode == 2) cinder_audio_pause();
                    if (s_mode) usleep(200000);      // let the state transition land before seeking
                    s_rc = cinder_audio_seek_ms_origin(s_origin, s_ms);
                    if (s_mode) usleep(200000);
                    if (s_mode == 1) cinder_audio_resume();
                    else if (s_mode == 2) cinder_audio_play();
                });
                int rc = s_rc;
                usleep(1200000);
                int c1 = -1, t1 = -1;
                cinder_audio_position(&c1, &t1);
                // "Landed" is generous by 3 s: the position is polled about once a second and the
                // track keeps advancing after the seek, so an exact match would fail on a correct
                // seek. What it must NOT look like is "carried on from where it was".
                long drift = (long)c1 - target;
                if (drift < 0) drift = -drift;
                std::fprintf(stderr,
                    "[cinder-home] seek probe: origin=%d target=%ld mode=%d rc=%d | before pos=%d"
                    " dur=%d | after 1.2s pos=%d dur=%d => %s\n",
                    origin, target, mode, rc, c0, t0, c1, t1,
                    (t1 != t0)    ? "TRACK CHANGED mid-probe — rerun, this result means nothing"
                    : (drift <= 3000) ? "LANDED (this mode/origin works)"
                                      : "MISSED (still refused)");
                std::fflush(stderr);
            }

            // DEV PROBE: `echo go > /tmp/cinder_msc.req` runs the SAME path as Settings ▸ USB mass
            // storage. The manual row is the one the user actually uses and it has never produced a
            // log anyone could read: the whole session logs to tmpfs (/contents is away) and is
            // spliced back on exit, so a failure that also breaks the exit erases its own evidence.
            // Driving it from adb means the attempt can be made, watched and dissected without
            // anyone having to be holding the device.
            if (take_req("/tmp/cinder_msc.req", nullptr, 0)) {
                clog_("usb-msc: entering ON REQUEST (/tmp/cinder_msc.req) — same path as the Settings row");
                cinder_show_usb_storage();
                cinder_render_tick();                 // paint the modal; enter_usb_msc blocks ~8 s
                carry_out(CINDER_ACT_ENTER_USB_MSC);
            }
#endif
            if (::access("/tmp/cinder_screenshot.req", F_OK) == 0) {
                ::unlink("/tmp/cinder_screenshot.req");
                cinder_request_screenshot("/tmp/cinder_screen.png");
            } else if (!g_msc_active && ::access("/contents/cinder_screenshot.req", F_OK) == 0) {
                ::unlink("/contents/cinder_screenshot.req");
                cinder_request_screenshot("/contents/cinder_screen.png");
                g_screenshot_sync = 3;        // vfat: sync a few ticks later, once the PNG is written
            }
            if (g_screenshot_sync > 0 && --g_screenshot_sync == 0) ::sync();
#ifdef CINDER_DEV
            // DEV: push the /contents page cache to eMMC every ~2 s so device-written files
            // (cinderhome.log, cinder_discovery.txt, MTPDB_copy.dat, cinder_settings.conf) are
            // readable over USB-MSC without a reboot. The host reads the raw block device, which
            // lags the device's live rw mount until a sync — hence the earlier 0-byte/short reads.
            if (n % 120 == 0) ::sync();
#endif
        }
        // Battery gauge: every ~10 s, wall-clock paced for the same reason as the housekeeping block
        // above (an iteration count means something different at 60 Hz and at 10 Hz). Reading it is
        // two small sysfs reads, so 10 s is already conservative.
        static long last_batt_ms = 0;
        if (house_now - last_batt_ms >= 10000) {
            last_batt_ms = house_now;
            const int pct = read_battery();
            cinder_set_battery(pct);
            // Warn low, shut down before the hardware browns out mid-write. Not guarded: it is pure
            // sysfs reads plus, at the very end, the same power path the Settings row uses.
            battery_guard(pct);
        }
        ++n;
        // FRAME PACING: sleep only the REMAINDER of the 16 ms budget, not a flat 16 ms on top of
        // however long the frame took. The old comment here assumed "the blit+flip is ~2 ms"; on
        // device it is ~15.6 ms, and a scrolling frame costs ~31 ms all in (cinder-probe --bench,
        // 2026-07-26). Adding a full 16 ms to that turned a 32 fps ceiling into ~21 fps — the
        // scrolling choppiness was half render cost and half this sleep.
        //   An idle frame still costs ~nothing (the dirty flag skips the work) and sleeps the full
        // budget, so this does not spin the CPU when nothing is moving. A frame that overruns
        // yields 1 ms rather than 0, so the input/housekeeping threads always get scheduled.
        // ── Frame pacing, and the single biggest battery lever in the app ────────────────────
        // Awake: ~60 Hz, for touch tracking (a drag has to follow the finger).
        // Panel DARK: 10 Hz. Nothing is being drawn, so the only thing this rate buys is input
        // latency, and the only input that matters while dark is the one that wakes the device —
        // where 100 ms is imperceptible. It is worth a lot: input_pump() does a non-blocking read()
        // on EVERY input node EVERY iteration (8 nodes here), so 60 Hz is ~480 syscalls/second plus
        // 60 thread wakeups, sustained. Dark is also the LONGEST-lived state on a music player —
        // screen off, in a pocket, playing for hours — so this is where the cost actually adds up.
        // 60 -> 10 Hz cuts it to ~80 syscalls/second.
        //
        // …and the sleep is a poll() ON THE INPUT NODES, not a usleep. That decouples the two
        // things the budget used to conflate — how often we wake, and how fast we answer a touch.
        // With poll(), an event returns immediately at ANY budget, so the dark budget can be a
        // full second without costing wake latency: the finger that wakes the device now lands in
        // the very next iteration instead of up to 100 ms later. Measured on device 2026-08-11,
        // dark + idle: the render thread was doing 10.2 context switches/s (one per 100 ms tick)
        // and this takes it to ~1/s, with better touch response, not worse.
        //   Housekeeping is what sets the floor: it runs at 1 Hz and owns the sleep timer, the
        // scrobbler and the USB-host debounce, so 1000 ms is the longest we may ever sleep. The
        // awake budget stays 16 ms — a drag has to follow the finger, and there poll() only helps.
        long left;
        if (g_screen_on) {
            left = 16 - (now_ms() - frame_start);
        } else {
            // Sleep to the NEXT housekeeping deadline, not a flat second — a flat 1000 ms would
            // drift past the 1 Hz check and silently halve the housekeeping rate (the sleep timer
            // and the scrobbler both ride on it).
            left = last_house_ms + 1000 - now_ms();
            if (left > 1000) left = 1000;
        }
        if (left < 1) left = 1;
        if (g_evn > 0 && g_deferred_done) {
            struct pollfd pfd[16];
            for (int i = 0; i < g_evn; ++i) { pfd[i].fd = g_evfds[i]; pfd[i].events = POLLIN; pfd[i].revents = 0; }
            poll(pfd, g_evn, (int)left);   // returns early on ANY input; EINTR is fine, we just loop
        } else {
            usleep(left * 1000);           // pre-init, or no input nodes at all: nothing to wait on
        }
    }
    return nullptr;
}
void start_pump_ticker() {
    if (g_pump_ticker_run) return;   // start exactly once
    g_pump_ticker_run = true;
    if (pthread_create(&g_pump_ticker, nullptr, render_driver, nullptr) != 0) {
        g_pump_ticker_run = false;
        clog_("render_up: WARN render-driver thread failed to start (UI will not paint)");
    } else {
        clog_("render_up: render driver started (~60fps, our own loop)");
    }
}

} // namespace

int main(int argc, char** argv) {
    clog_("main: start");
    install_diagnostics();   // crash/hang handler -> logs the exact PC of the stall

    // SELF-HEAL THE GADGET, BEFORE ANYTHING ELSE WANTS IT. A USB-mode switch that died mid-sequence
    // leaves /sys/class/android_usb/android0/enable at 0, and then nothing — not adb, not mass
    // storage, not USB-DAC — is on the host's bus until someone writes it back. A no-op when the
    // gadget is healthy, which is every normal boot.
    (void)std::system("/system/vendor/unknown321/bin/cinder-msc usb-rescue");

    CinderApp app;

    // The CuiAppModule callbacks (named per the RE'd ctor). They map to lifecycle phases;
    // each is traced. onForeground brings up the renderer; the pump ticks it.
    auto cbInit    = []() { clog_("cb:onInitialize"); };
    auto cbPostI   = []() { clog_("cb:onPostInitialize"); };
    auto cbActivate= []() { clog_("cb:onActivate"); };
    auto cbForeg   = []() { clog_("cb:onForeground"); render_up(); };
    auto cbFinal   = []() {
        clog_("cb:onFinalize");
        // Stop the pump ticker BEFORE the module is destroyed so it can't poke a freed object.
        g_pump_ticker_run = false;
        if (g_pump_ticker) { pthread_join(g_pump_ticker, nullptr); g_pump_ticker = 0; }
        cinder_render_shutdown();
    };
    auto pump      = []() -> bool {
        // INERT BY DESIGN. The frame loop (render tick, deferred_up, input, housekeeping) lives
        // on our own worker thread (render_driver, started by render_up) because easel's pump
        // never runs for us: the main thread parks forever inside OnForeground's module CV wait
        // (Sony's JobQueue only ticks under libeaselqt — see STATUS.md). If a future fw ever DID
        // fire this callback it would run on the MAIN thread concurrently with the worker —
        // racing g_deferred_done/g_input_started/touch state and arming alarm() on a thread with
        // SIGALRM blocked. So this must stay a stub: log once, keep-pumping, do nothing.
        static bool logged = false;
        if (!logged) { logged = true; clog_("cb:pump fired (unexpected — worker owns the frame loop; staying inert)"); }
        return true;
    };
    auto cb7       = []() { clog_("cb:cb7"); };

    clog_("main: constructing CuiAppModule");
    auto* cui = new easel::CuiAppModule(app, argc, argv,
            cbInit, cbPostI, cbActivate, cbForeg, cbFinal,
            pump, cb7);
    g_cui = cui;   // keep a raw handle so the ticker can drive OnPumpTrigger (see pump_ticker)
    g_app = &app;  // for StopBootAnimation() — kill the boot-anim overlay once we paint
    auto module = std::unique_ptr<easel::ModuleBaseInterface>(cui);

    clog_("main: calling app.run()");
    app.run(argc, argv, "HgrmMediaPlayerApp", std::move(module));
    clog_("main: app.run() returned");
    return 0;
}
