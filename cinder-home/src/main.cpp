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
#include <ctime>
#include <time.h>   // clock_gettime/CLOCK_MONOTONIC (drag velocity timing)
#include <setjmp.h>
#include <pthread.h>
#include <sys/statvfs.h>
#include <sys/ioctl.h>
#include <sys/mount.h>   // umount/umount2 (we unmount /contents ourselves before the MSC handoff)
#include <cerrno>

// The render core: the Rust Cinder UI, built as a glibc C-ABI staticlib
// (player/cinder-ffi -> libcinder_ffi.a). C ABI, so the renderer stays in Rust while
// this shell stays C++/libc++. See player/cinder-ffi/include/cinder.h.
#include "cinder.h"
#include "cinder_effects.h"
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
void recompute_day_level();   // map the UI's 1..5 brightness onto the node's day level (no write)
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
    // Boot ALWAYS at DAY backlight, even if night theme is persisted — the night dim is NOT resumed
    // across boots. Otherwise a daytime boot into persisted night could come up at ~3% backlight and
    // you couldn't see the screen to turn it back up. The night dim is a deliberate per-session action
    // (toggle Theme→night). Pure sysfs write (no Sony service); no-op if no backlight node found.
    // Persisted brightness applies at boot; the theme does NOT (always day — see above). Level 1
    // is 15% of max, so even the dimmest persisted setting comes up readable.
    recompute_day_level();
    set_backlight(0);
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

// Is the optional real-spectrum visualiser enabled? Reads /contents/cinder_viz.conf for a line
// `analyzer=1`. Absent/unset/0 => OFF (the safe default — the synthetic visualiser still runs).
// Kept deliberately dumb (substring match) so a malformed file can't do anything but disable it.
bool viz_analyzer_enabled() {
    FILE* f = std::fopen("/contents/cinder_viz.conf", "r");
    if (!f) return false;
    char buf[256] = {0};
    size_t got = std::fread(buf, 1, sizeof buf - 1, f);
    std::fclose(f);
    if (got == 0) return false;
    return std::strstr(buf, "analyzer=1") != nullptr;
}

void report_storage();  // defined below (with the other sysfs readers); called from deferred_up
void apply_eq_fn();      // defined below (carry_out helpers); re-applied from deferred_up on restore
void apply_sound_fn();   // ditto (apply_backlight is forward-declared earlier, before render_up)
void write_bt_pref();    // defined below (carry_out helpers); published once at boot from deferred_up
void sync_volume_from_hw(); // defined below (volume backend); seeds the UI level from the mixer
void apply_volume();        // defined below (volume backend); writes the UI level to the mixer

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
    // Re-apply the user's SAVED EQ + sound effects to the DSP (only if a settings file was restored —
    // no point pushing defaults on a fresh install). Guarded, like every effect-shim call.
    if (g_settings_loaded) {
        run_guarded("deferred_up: re-apply saved EQ", 6, apply_eq_fn);
        run_guarded("deferred_up: re-apply saved sound", 6, apply_sound_fn);
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
    // OPTIONAL real-spectrum visualiser via Sony's AudioAnalyzerService. DEFAULT OFF — only started
    // if /contents/cinder_viz.conf contains `analyzer=1`, and even then behind the guard (dlopen +
    // a Sony-service connect is a fresh risk surface). Off, the visualiser still animates
    // synthetically. Validate first with `cinder-probe --analyzer` on device. The shim no-ops the
    // repaint when Now Playing isn't showing, so continuous streaming is cheap off-screen.
    if (viz_analyzer_enabled()) {
        run_guarded("deferred_up: analyzer start (AudioAnalyzerService spectrum)", 10,
                    []() { cinder_analyzer_start(CINDER_ANALYZER_SPECTRUM, 20.0f, 0); });
    }
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
static long now_ms() {
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
static bool g_playing = true;   // local transport state (PlayStatus playstate offset not RE'd)

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
}

// Read the UI's EQ bands and push them to the device DSP. Run ONLY via run_guarded (below): the
// EffectCtrlDmp connect can crash/hang if the sound service is down → caught, UI continues.
void apply_eq_fn() {
    signed char bands[10];
    cinder_get_eq_bands(bands);
    cinder_effects_set_eq(bands, 10);
}

// Apply the Sound screen's effect toggles to the DSP. Run ONLY via run_guarded (the EffectCtrlDmp
// connect can crash/hang). The bitmask matches cinder_get_sound_flags(): bit0 DSEE · bit1 Vinyl ·
// bit2 VPT · bit3 DC-Phase · bit4 Normalizer · bit5 ClearAudio+. (VPT/DC-Phase apply on/off here;
// their mode/type is a device-gated enhancement — see analysis/RE_playerservice_sound.md.)
void apply_sound_fn() {
    int f = cinder_get_sound_flags();
    cinder_effects_set_dsee_hx((f >> 0) & 1);
    cinder_effects_set_vinylizer((f >> 1) & 1);
    cinder_effects_set_vpt((f >> 2) & 1);
    cinder_effects_set_dc_phase((f >> 3) & 1);
    cinder_effects_set_dynamic_normalizer((f >> 4) & 1);
    cinder_effects_set_clearaudio_plus((f >> 5) & 1);
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
        else if (!std::strcmp(k, "control")) std::strncpy(g_vol.control, v, sizeof g_vol.control - 1);
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
void sync_volume_from_hw() {
    if (!g_vol_read) load_vol_cfg();
    if (!g_vol.valid) return;
    long val = -1;
    if (g_vol.amixer) {
        char cmd[384];
        std::snprintf(cmd, sizeof cmd, "amixer -c %d cget name='%s' 2>/dev/null", g_vol.card, g_vol.control);
        FILE* p = popen(cmd, "r");
        if (!p) return;
        char line[256];
        while (std::fgets(line, sizeof line, p)) {
            const char* v = std::strstr(line, ": values=");
            if (v) { val = std::strtol(v + 9, nullptr, 10); break; }
        }
        pclose(p);
    } else {
        FILE* f = std::fopen(g_vol.path, "r");
        if (!f) return;
        char buf[32] = {0};
        if (std::fgets(buf, sizeof buf, f)) val = std::strtol(buf, nullptr, 10);
        std::fclose(f);
    }
    if (val < g_vol.min || val > g_vol.max) return;   // parse failed / out of range — keep UI default
    // UI level is the stock 0..120 scale; with the default backend (min 0, max 120) this is 1:1.
    int level = (int)((val - g_vol.min) * 120 / (g_vol.max - g_vol.min));
    cinder_set_volume(level);
    char m[96];
    std::snprintf(m, sizeof m, "volume: hw %ld -> UI level %d/120", val, level);
    clog_(m);
}

// Apply the UI's 0..120 volume level to the device via the configured backend (1:1 with the
// default amixer 'master volume' 0..120; rescaled only for a conf-overridden range). No-op if
// unconfigured. Called guarded (system()/sysfs write). Read-on-first-use.
void apply_volume() {
    if (!g_vol_read) load_vol_cfg();
    if (!g_vol.valid) return;
    int level = cinder_get_volume();
    if (level < 0) level = 0; if (level > 120) level = 120;
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
}

// ── Backlight (night = minimal light) ───────────────────────────────────────────────────────
// The night/day theme drives the PANEL BACKLIGHT: night mode dims it to a minimal level. The node
// is auto-detected (the common Android/MTK paths) and overridable via /contents/cinder_backlight.conf
// (path, night, day raw values). If no node is writable, it's a no-op (the device keeps its own
// brightness). Levels default to a tiny fraction of max_brightness for night, ~70% for day.
struct BlCfg { int valid = 0, night = -1, day = -1, max = 255; char path[256] = {0}; };
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
    g_bl.valid = 1;
}

// Write the panel backlight: night → minimal, day → normal. No-op if no node / level unset.
void set_backlight(int night) {
    if (!g_bl_read) load_bl_cfg();
    if (!g_bl.valid) return;
    int level = night ? g_bl.night : g_bl.day;
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
void recompute_day_level() {
    if (!g_bl_read) load_bl_cfg();
    if (!g_bl.valid) return;
    static const int pct[5] = { 15, 30, 50, 70, 100 };
    int lvl = cinder_get_brightness();          // already clamped to 1..5 by cinder-ffi
    if (lvl < 1 || lvl > 5) lvl = 4;
    g_bl.day = g_bl.max * pct[lvl - 1] / 100;
    if (g_bl.day < 1) g_bl.day = 1;             // never fully dark
}

// Live change from the Settings row: recompute, then write at the CURRENT theme's level.
void apply_brightness() { recompute_day_level(); set_backlight(cinder_get_night()); }

// For the LIVE theme toggle: match the backlight to the current theme.
void apply_backlight() { set_backlight(cinder_get_night()); }

// Power button = screen on/off. OFF writes backlight 0 (panel dark; the app keeps rendering so
// playback/Hold-state continue); ON restores the current theme's level. Pure sysfs write, no Sony
// service. Locking is independent (the Hold switch) — waking the screen never unlocks the touch.
static bool g_screen_on = true;
// The himax touch controller has a driver sysfs SLEEP switch — and something must write it, or
// the controller never scans and /dev/input/event1 stays silent forever (opens fine, EVIOCGABS
// answers, zero events — the exact 2026-07-02 symptom). The stock Qt app is what normally wakes
// it; with cinder-home as the Home app nothing did. Wampy has the same problem and the same fix
// (write "0" = awake, "1" = sleep): src/connector/hagoromo.cpp enableTouchscreen(). Paths are
// the A50-family node with the WM1Z variant as fallback (Wampy's pair, verbatim).
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
    if (!slp) clog_("input: touch WAKE — no touch sleep node found (harmless: the held evdev grab keeps events flowing; screen-off tap-drop is handled in input_pump)");
}

void screen_toggle() {
    g_screen_on = !g_screen_on;
    touch_set_sleep(g_screen_on ? 0 : 1);   // stock behaviour: TS sleeps with the panel (battery)
    // Drop any in-flight contact: the sleeping controller never sends its lift, and a stale
    // "down" would make the next touch classify as a drag from the old start point.
    g_touch_down = false; g_touch_start_x = -1; g_touch_start_y = -1; g_touch_saw_pos = false;
    if (g_screen_on) { apply_backlight(); return; }
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
void apply_usb_dac() {
    if (cinder_get_usb_dac()) {
        std::system("touch /contents/ldac_on 2>/dev/null; "
                    "setprop sys.sony.config uac 2>/dev/null");
        clog_("usb-dac: engaged (UAC + LDAC bridge on; Bluetooth left connected)");
    } else {
        std::system("rm -f /contents/ldac_on 2>/dev/null");
        clog_("usb-dac: disengaged (LDAC bridge off)");
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
static int  g_usb_hi = 0;           // debounce: consecutive host-present samples while NOT in MSC

void redirect_fds(const char* path, int flags) {
    std::fflush(stdout); std::fflush(stderr);
    int fd = open(path, flags, 0644);
    if (fd >= 0) { dup2(fd, 1); dup2(fd, 2); if (fd > 2) close(fd); }
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
            clog_("usb-msc: LUN was empty — bound /emmc@contents (host medium inserted, no re-enum)");
            return;
        }
        usleep(250000); // holder still dropping — retry the media-insert
    }
    clog_("usb-msc: LUN STILL empty after retries — host will see a reader with NO medium");
}

void enter_usb_msc() {
    clog_("usb-msc: entering (session log -> /tmp/cinder_msc.log, spliced back on exit)");
    // 1) release OUR storage users. Pause is NOT enough: a paused PlayerService keeps the
    //    current track's file open under /contents, which alone makes unmount_msc1 fail EBUSY.
    //    Stop + drop the pinned sequence so the service closes the media file. Called directly
    //    (NOT via a nested run_guarded — the guard's jmp buffer doesn't nest): this whole
    //    function already runs under carry_out's "enter USB MSC" guard, which covers the IPC.
    g_playing = false;
    cinder_audio_release_sequence();
    (void)chdir("/");
    // 2) move our log fds (1+2 ARE /contents/cinderhome.log via the launcher redirect)
    redirect_fds(MSC_TMP, O_WRONLY | O_CREAT | O_APPEND);
    // 3) UNMOUNT /contents FIRST, via the setuid-root helper, THEN flip the gadget. On-device RE
    //    (adb, 2026-07-25) found TWO things: (a) the stock `sys.sony.config=msc` trigger is RACY —
    //    it `start unmount_msc1` (an async fork of `umount /contents`) then IMMEDIATELY writes
    //    lun/file, so the gadget often binds a STILL-MOUNTED block device and the LUN comes up EMPTY
    //    (PC sees a 0-byte reader with NO MEDIUM); and (b) cinder-home runs as uid 100 with an EMPTY
    //    capability set (appmgr strips them), so it CANNOT umount(2) itself (EPERM) — that's why the
    //    earlier in-process umount always failed. Fix: cinder-umount (chmod 4755, owner root) regains
    //    caps on exec and unmounts (verified: a uid-100 caller unmounts /contents rc 0). With
    //    /contents already gone, the trigger's lun bind lands on a FREE device. Retry for a holder.
    bool unmounted = false;
    for (int i = 0; i < 12; ++i) {
        if (!contents_mounted()) { unmounted = true; break; }
        std::system("/system/vendor/unknown321/bin/cinder-umount");
        if (!contents_mounted()) { unmounted = true; break; }
        if (i == 0) log_contents_holders();   // name any fd holder on the first miss
        usleep(250000);
    }
    if (!unmounted)
        clog_("usb-msc: helper could NOT unmount /contents — falling through to the stock trigger");
    // 3b) NOW flip the gadget. With /contents already unmounted the trigger's lun/file write binds a
    //     free block device cleanly (functions=mass_storage,adb, idProduct 0B8D); unmount_msc1's
    //     redundant umount is a harmless no-op. Kept over hand-rolled sysfs so adbd + idProduct stay
    //     exactly as stock expects on the way in AND out. cinder is NOT a child of adbd, so the
    //     enable-cycle inside the trigger doesn't kill this process mid-switch.
    std::system("setprop sys.sony.config msc 2>/dev/null");
    // 4) settle (let unmount_msc1/the enable-cycle catch up), then GUARANTEE the medium is present:
    //    if the LUN still reads empty, point it + bounce the gadget so the host re-enumerates a
    //    reader WITH medium. No-op when the trigger already bound it.
    for (int i = 0; i < 12 && contents_mounted(); ++i) usleep(250000);
    if (!contents_mounted()) {
        ensure_msc_lun();
    } else {
        clog_("usb-msc: /contents STILL MOUNTED after the switch — fd holders:");
        log_contents_holders();
        // /contents is writable here, so persist the failed-session diagnosis into the main log now.
        std::system("cat /tmp/cinder_msc.log >> /contents/cinderhome.log 2>/dev/null");
    }
    // 5) record the gadget state (goes to the tmpfs log; readable after exit — or already
    //    spliced above on the unrecoverable path)
    std::system("sleep 3; echo \"[cinder-home] usb-msc: state=$(getprop sys.usb.state) "
                "functions=$(cat /sys/class/android_usb/android0/functions 2>/dev/null) "
                "lun=$(cat /sys/class/android_usb/android0/f_mass_storage/lun/file 2>/dev/null)\"");
    g_msc_active = true;
    g_msc_seen_usb = false;
}
void exit_usb_msc() {
    std::system("setprop sys.sony.config adb 2>/dev/null");           // stock default; remounts
    for (int i = 0; i < 50 && !contents_mounted(); ++i) usleep(100000); // ≤5 s for mount_msc1
    redirect_fds("/contents/cinderhome.log", O_WRONLY | O_CREAT | O_APPEND);
    g_msc_active = false;
    // splice the away-session log back in (cat writes to fd 1 = cinderhome.log again)
    std::system("cat /tmp/cinder_msc.log 2>/dev/null; rm -f /tmp/cinder_msc.log 2>/dev/null");
    clog_(contents_mounted() ? "usb-msc: exited (/contents remounted; log restored)"
                             : "usb-msc: exited but /contents did NOT remount within 5 s");
}

// Carry out a navigator action via the audio/effect shims. Volume goes to the configured
// backend (built-in CXD3778GF defaults, overridable by conf); play-by-index hands PlayerService
// a NodeTrackSequence built from the pending-play list the UI resolved.
void carry_out(int act) {
    // Every transport call is a PlayerService IPC call → guard it (same invariant as the EQ apply
    // and the now-playing poll): a hung/crashing PlayerService then skips that one action and the UI
    // keeps running, instead of tripping the per-frame watchdog into a fatal _exit/reboot. The
    // lambdas are non-capturing (g_playing is global) so they convert to run_guarded's fn pointer.
    switch (act) {
        case CINDER_ACT_PLAYPAUSE:
            g_playing = !g_playing;
            run_guarded("carry_out: play/pause", 6,
                        []() { if (g_playing) cinder_audio_play(); else cinder_audio_pause(); });
            break;
        case CINDER_ACT_NEXT:       run_guarded("carry_out: next",  6, []() { cinder_audio_next_track(); }); break;
        case CINDER_ACT_PREV:       run_guarded("carry_out: prev",  6, []() { cinder_audio_prev_track(); }); break;
        case CINDER_ACT_NEXT_ALBUM: run_guarded("carry_out: next album", 6, []() { cinder_audio_next_group(); }); break;
        case CINDER_ACT_PREV_ALBUM: run_guarded("carry_out: prev album", 6, []() { cinder_audio_prev_group(); }); break;
        case CINDER_ACT_VOLUP:
        case CINDER_ACT_VOLDOWN:
            // apply the new UI volume to the hardware via the configured backend (guarded).
            // Defaults to the discovered control (amixer card0 'master volume' 0..120) with no
            // conf present; /contents/cinder_volume.conf overrides it.
            run_guarded("carry_out: volume", 4, apply_volume);
            break;
        case CINDER_ACT_PLAY_INDEX:
            // Play the tapped track inside its album context: drain the pending-play URI list the
            // UI resolved (cinder_pending_play_*), hand PlayerService a NodeTrackSequence, start
            // at the tapped index. Guarded: JSON->Node + SetTrackSequence are Sony-service calls.
            run_guarded("carry_out: play selected track", 10, []() {
                int n = cinder_pending_play_count();
                if (n <= 0) return;
                if (n > 512) n = 512;                       // sanity cap (one album, not the world)
                static char bufs[512][512];                 // static: keep the pump stack tiny;
                static const char* ptrs[512];               // 512B/URI — deep unicode paths fit
                int kept = 0;
                for (int i = 0; i < n; ++i) {
                    if (cinder_pending_play_uri(i, bufs[kept], sizeof bufs[kept]) > 0)
                        ptrs[kept] = bufs[kept], ++kept;
                }
                if (kept == 0) return;
                int start = cinder_pending_play_start();
                if (start < 0 || start >= kept) start = 0;
                int rc = cinder_audio_play_tracks(ptrs, kept, start);
                if (rc == 0) g_playing = true;
                else fprintf(stderr, "[cinder] play_tracks(%d tracks, start %d) failed rc=%d\n",
                             kept, start, rc);
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
        case CINDER_ACT_SOUND_BYPASS:
            // A/B compare: bypass or re-enable the whole effect chain, guarded.
            run_guarded("carry_out: A/B bypass", 6,
                        []() { cinder_effects_set_bypass(cinder_get_sound_bypass()); });
            break;
        case CINDER_ACT_THEME_CHANGED:
            // night/day toggled -> set the panel backlight (night = minimal light), guarded.
            run_guarded("carry_out: backlight (theme)", 4, apply_backlight);
            break;
        case CINDER_ACT_BRIGHTNESS_CHANGED:
            // Settings Brightness row cycled 1..5 → recompute the day level + rewrite the node.
            run_guarded("carry_out: backlight (brightness)", 4, apply_brightness);
            break;
        case CINDER_ACT_BT_CODEC_CHANGED:
            // device-wide codec/quality changed → persist it for every BT path (file IO, safe).
            write_bt_pref();
            break;
        case CINDER_ACT_USBDAC_LDAC:
            run_guarded("carry_out: USB-DAC/LDAC toggle", 6, apply_usb_dac);
            break;
        case CINDER_ACT_SLEEP:         screen_toggle(); break; // Power = panel on/off (not lock)
        case CINDER_ACT_ENTER_USB_MSC:
            // device-gated USB-mode switch (hands storage to the PC; disruptive — validate live).
            // 25 s budget: Stop IPC + the 5 s umount verify + recovery + the 3 s gadget-state
            // settle all run under this ONE guard (run_guarded doesn't nest).
            run_guarded("carry_out: enter USB MSC", 25, enter_usb_msc);
            break;
        case CINDER_ACT_EXIT_USB_MSC:
            // Back on the modal (or the unplug watcher) → remount /contents + restore the log.
            run_guarded("carry_out: exit USB MSC", 10, exit_usb_msc);
            break;
        default: break;
    }
}

// Drain pending input from every node; map raw code -> logical button -> navigator -> action.
// Classify a finished contact (finger-up), from BTN_TOUCH=0 or a type-A empty-frame lift.
static void touch_release() {
    if (g_touch_down && g_scrub_active) {
        // Drag-to-seek ends: ask cinder-ffi where the finger left the rail and seek there.
        int ms = cinder_scrub_end();
        if (ms >= 0) {
            std::fprintf(stderr, "[cinder-home] seek to %d ms\n", ms);
            int rc = cinder_audio_seek_ms(ms);
            if (rc != 0) clog_("touch: seek REJECTED by PlayerService");
        }
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
    g_touch_down = false; g_touch_start_x = -1; g_touch_start_y = -1;
    g_drag_active = false; g_drag_vel = 0.0f;
    g_scrub_active = false; g_scrub_tested = false;
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
            cinder_scrub_to(touch_ui_x(g_touch_cur_x));   // a tap on the rail also seeks
            return;
        }
    }
    if (g_scrub_active) {
        // Seeking: the bar tracks x only. Never promote to a list drag — vertical wander during a
        // horizontal scrub must not start scrolling the screen underneath.
        cinder_scrub_to(touch_ui_x(g_touch_cur_x));
        return;
    }
    if (!g_drag_active) {
        int dyt = uy - touch_ui_y(g_touch_start_y);
        int dxt = touch_ui_x(g_touch_cur_x) - touch_ui_x(g_touch_start_x);
        int adyt = dyt < 0 ? -dyt : dyt, adxt = dxt < 0 ? -dxt : dxt;
        if (adyt > 12 && adyt > adxt) {
            g_drag_active = true;
            g_drag_last_uy = uy;
            g_drag_last_ms = now_ms();
            g_drag_vel = 0.0f;
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

void input_pump() {
    ev_event evs[32];
    static long g_ev_total = 0;   // events ever seen (any node) — for the silent-input heartbeat
    static long g_pump_calls = 0;
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
                    // Panel dark (Power toggle): taps must not navigate invisibly. Stock sleeps
                    // the controller; this fw has no sleep node (see touch_set_sleep), so the
                    // events keep coming — drop them and any in-flight contact until screen-on.
                    if (!g_screen_on) {
                        g_touch_down = false; g_touch_start_x = -1; g_touch_start_y = -1;
                        g_touch_saw_pos = false;
                        g_drag_active = false; g_drag_vel = 0.0f;
                        g_scrub_active = false; g_scrub_tested = false;
                        continue;
                    }
                    if (type == EV_ABS_ && (code == ABS_X_ || code == ABS_MT_POSITION_X_)) {
                        g_touch_cur_x = val; g_touch_saw_pos = true;
                        if (!g_touch_down) {
                            g_touch_down = true; g_touch_start_x = val; g_touch_start_y = -1;
                            g_scrub_active = false; g_scrub_tested = false;
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
                                g_scrub_active = false; g_scrub_tested = false;
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
                if (btn == CINDER_BTN_VOLUP || btn == CINDER_BTN_VOLDOWN) {
                    if (val == 1) { g_vol_btn = btn; g_vol_down_ms = now_ms(); }
                    g_vol_last_ms = now_ms();   // also swallows a kernel repeat's slot
                }
                int act = cinder_input(btn);
                if (act != CINDER_ACT_NONE) carry_out(act);
            }
            if (n < (ssize_t)sizeof evs) break; // drained this node
        }
    }
    // Held rocker: the events are all drained, so anything still down is a genuine hold.
    vol_repeat_tick();
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
void poll_now_playing() {
    static char last[1024];
    char uri[1024];
    int n = cinder_audio_current_uri(uri, sizeof uri);
    if (n > 0 && std::strcmp(uri, last) != 0) {
        std::strncpy(last, uri, sizeof last - 1);
        last[sizeof last - 1] = 0;
        cinder_set_now_playing_uri(uri, 0.0f, g_playing ? 1 : 0, read_battery());
    }
    // The service is the authority on both position AND whether it is really playing: g_playing is
    // only our optimistic view of the last transport action we sent.
    int cur = -1, tot = -1;
    if (cinder_audio_position(&cur, &tot)) {
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
        if (n < 600) cinder_force_dirty();
        else if (n % 60 == 0) cinder_force_dirty();

        long frame_start = now_ms();
        // PER-FRAME WATCHDOG around OUR paint: a real render hang -> _exit -> launcher counter -> stock.
        alarm(8);
        cinder_render_tick();
        alarm(0);
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

        if (!g_input_started) { input_open(); g_input_started = true; }
        alarm(8); input_pump(); alarm(0);     // touch + buttons -> navigator -> actions -> carry_out
        if (n % 60 == 0) {                    // ~1x/sec housekeeping
            cinder_clock_tick();
            run_guarded("pump: poll now-playing", 8, poll_now_playing);
            // Scrobble writes /contents — skip while it's handed to the PC (stale mountpoint).
            if (!g_msc_active) cinder_scrobble_tick(g_playing ? 1 : 0);
            if (cinder_sleep_should_pause()) {
                clog_("sleep timer expired -> pausing");
                g_playing = false;
                run_guarded("pump: sleep-timer pause", 6, []() { cinder_audio_pause(); });
            }
            // USB mass-storage is fully automatic — no menu dive:
            //  • NOT in MSC + a PC data-host appears (debounced ~2 s so charger/enumeration flicker
            //    doesn't bounce us in) → raise the modal and hand /contents to the PC. usb_connected()
            //    keys on android0/state==CONFIGURED, so a dumb wall charger (CONNECTED only) never
            //    trips this; only a real PC does. Skipped while onboarding/locked can't matter here.
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
            } else if (usb_connected() && !dev_skip_auto_msc()) {
                if (++g_usb_hi >= 2) {
                    clog_("usb-msc: PC host detected — auto-entering mass storage");
                    cinder_show_usb_storage();            // UI reflects the handoff (same modal as the tap)
                    cinder_render_tick();                 // PAINT the modal now — enter_usb_msc blocks ~8 s
                    carry_out(CINDER_ACT_ENTER_USB_MSC);  // flip gadget + hand /contents to the PC
                }
            } else {
                g_usb_hi = 0;
            }
            mark_healthy_maybe();             // clear the bad-boot counter once proven good
            // Screenshot-on-demand: drop /tmp/cinder_screenshot.req and the next frame is written
            // to /tmp/cinder_screen.png. Same polled-flag idiom as ldac_on above (no new IPC
            // primitive — the safety model here is best-effort polled file I/O).
            //   /tmp, NOT /contents: /tmp is tmpfs, so it survives USB-MSC (which unmounts
            //   /contents and hands it to the PC), needs no sync (/contents is vfat — unsynced
            //   writes are lost), and costs no eMMC wear for a throwaway debug artifact.
            //   Also accept the /contents trigger, for setting it over USB-MSC with no adb.
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
        if (n % 600 == 0) cinder_set_battery(read_battery());
        ++n;
        // FRAME PACING: sleep only the REMAINDER of the 16 ms budget, not a flat 16 ms on top of
        // however long the frame took. The old comment here assumed "the blit+flip is ~2 ms"; on
        // device it is ~15.6 ms, and a scrolling frame costs ~31 ms all in (cinder-probe --bench,
        // 2026-07-26). Adding a full 16 ms to that turned a 32 fps ceiling into ~21 fps — the
        // scrolling choppiness was half render cost and half this sleep.
        //   An idle frame still costs ~nothing (the dirty flag skips the work) and sleeps the full
        // budget, so this does not spin the CPU when nothing is moving. A frame that overruns
        // yields 1 ms rather than 0, so the input/housekeeping threads always get scheduled.
        long spent = now_ms() - frame_start;
        long left = 16 - spent;
        usleep((left > 0 ? left : 1) * 1000);
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
