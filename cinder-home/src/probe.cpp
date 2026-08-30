// cinder-probe — standalone, ZERO-BOOT-RISK diagnostic.
//
// It runs ONLY the suspect cinder-home init calls (framebuffer open, library DB load,
// PlayerService connect, a render+poll loop) in ISOLATION — it does NOT do the easel/appmgr
// lifecycle, so it does NOT register as the Home app and CANNOT affect boot. The stock UI keeps
// running; the probe just briefly touches /dev/graphics/fb0 (cosmetic flicker) and connects to
// PlayerService as an extra client. Every call is watchdog-bounded: on a hang it logs the exact
// PC + backtrace + maps and exits, so we learn precisely which call blocks WITHOUT a flash.
//
// Run it from a shell on the device, e.g. over adb (/tmp is the only writable exec mount —
// /data and /contents are noexec; toolbox chmod needs octal):
//   adb push cinder-home/dist/dev/cinder-probe /tmp/cinder-probe
//   adb shell 'chmod 755 /tmp/cinder-probe && \
//     LD_LIBRARY_PATH=/system/vendor/sony/lib:/system/vendor/unknown321/lib:/system/lib \
//     /tmp/cinder-probe'
// Watch the printed trace: the LAST line before it stops is the call that hangs.
#include "cinder.h"
#include "cinder_audio.h"
#include "cinder_analyzer.h"
#include "cinder_tuner.h"
#include "discover.h"
#include <cstdio>
#include <cstdlib>
#include <csignal>
#include <cstring>
// uintptr_t, used in seven places to turn a listener's address into the `unsigned` handle
// RemoveListener wants. It compiled only because the device toolchain's headers happened to pull
// stdint in transitively — a header reorder or a toolchain bump would have broken the build with
// no source change. Found 2026-08-23 by the first host syntax check this file has ever had
// (tools/host_syntax_check.sh); main.cpp already had the include.
#include <cstdint>
#include <string>
#include <vector>
#include <ucontext.h>
#include <unistd.h>
#include <poll.h>
#include <fcntl.h>
#include <dirent.h>
#include <sys/ioctl.h>
#include <dlfcn.h>   // --btinfo checks the same lazy dlopen(libasound) the LDAC bridge relies on
#include <execinfo.h>
#include <initializer_list>
#include <pthread.h>
#include <functional>
#include <cerrno>
#include <stddef.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <alsa/asoundlib.h>
#include <linux/videodev2.h>
#include <sys/ioctl.h>
#include <cmath>   // --btopen tone: a sine is the only way to hear whether the PCM really arrived

static void clog_(const char* m) { std::fprintf(stderr, "[cinder-probe] %s\n", m); std::fflush(stderr); }

// ── pst::core::Framework (libpstcore.so) — the missing event loop ────────────────────────────
// Hypothesis under test (2026-07-27): every PlayerService call returns UNINITIALISED STACK
// (Connect rc=0xb6xxxxxx, IsConnected true-from-garbage, SetTrackSequence "99") and the service
// side logs NOTHING to /dev/log/main — so the binder transaction never completes. Sony's client
// proxies are async: the reply is delivered by pst::core::Framework's event looper, which for a
// Qt app libeaselqt pumps and which NOTHING in Cinder pumps (main.cpp:229 documents the dead
// pump but never linked it to the dead IPC). If that is the cause, driving Framework::Pump()
// makes the out-params fill in.
// Declared by hand (no SDK headers) purely to link against the device .so — return types are
// absent from the Itanium mangling, so only the names/params below have to match:
//   _ZN3pst4core9Framework12GetReferenceEv   _ZN3pst4core9Framework4PumpEb
//   _ZN3pst4core9Framework18GetBinderLastErrorEv
// GetReference() is a static Meyers singleton; StartForApplication is a NON-static member —
// easel::Framework's ctor (libeaselcore @0x5c18..0x5c46) does exactly
// `GetReference().StartForApplication(job, /*bool*/ true)`, r0 = the singleton. That call is
// how cinder-home's own Framework comes up today (ApplicationBase::run -> easel::Framework).
namespace pst { namespace core {
class Framework {
public:
    static Framework& GetReference();
    static int  GetBinderLastError();
    int  StartForApplication(std::function<void()> job, bool flag);
    void StopForApplication();
    bool Pump(bool short_timeout);
};
} }

static volatile bool g_pump_run = false;
static volatile unsigned g_pump_ticks = 0;
static void* pump_thread(void* fwp) {
    pst::core::Framework* fw = static_cast<pst::core::Framework*>(fwp);
    // YIELD BETWEEN PUMPS. This was a bare spin — `while (run) { Pump(true); }` — which pegs a core
    // for as long as the probe lives. Invisible on a two-second read; audible the moment a probe
    // mode HOLDS, because SoundServiceFw wants ~34% of a core while playing and a spinning thread
    // starves it. Reported 2026-08-17 as "very stuttery" during a 25 s VPT hold. Same shape as the
    // render-loop poll storm measured in docs/DEVICE_TESTS.md section 7, in a different process.
    //
    // 2 ms is 500 pumps a second — orders of magnitude more than any IPC reply needs, and it takes
    // the thread from 100% of a core to nothing measurable.
    while (g_pump_run) { fw->Pump(true); ++g_pump_ticks; usleep(2000); }
    return nullptr;
}

static int  g_pump_argc = 0;
static char** g_pump_argv = nullptr;
static void pump_job();      // the actual play test, run once the framework is up (below)
static void pump_finish() { std::fprintf(stderr, "[cinder-probe] pump: finish-job fired\n"); }

static void dump_maps() {
    FILE* f = std::fopen("/proc/self/maps", "r");
    if (!f) return;
    char line[512];
    std::fprintf(stderr, "--- /proc/self/maps (exec regions) ---\n");
    while (std::fgets(line, sizeof line, f))
        if (std::strstr(line, "r-xp")) std::fprintf(stderr, "%s", line);
    std::fclose(f);
    std::fflush(stderr);
}

static void crash_handler(int sig, siginfo_t* si, void* uc_) {
    unsigned long pc = 0, lr = 0;
#if defined(__arm__)
    ucontext_t* uc = static_cast<ucontext_t*>(uc_);
    pc = uc->uc_mcontext.arm_pc; lr = uc->uc_mcontext.arm_lr;
#else
    (void)uc_;   // only the ARM mcontext has a PC/LR to read; host builds still take the param
#endif
    std::fprintf(stderr, "[cinder-probe] *** %s : PC=0x%08lx LR=0x%08lx addr=%p ***\n",
                 (sig == SIGALRM ? "WATCHDOG (this call HUNG — this is the culprit)" : "FATAL SIGNAL"),
                 pc, lr, si ? si->si_addr : (void*)0);
    void* bt[24];
    int n = backtrace(bt, 24);
    std::fprintf(stderr, "--- backtrace (%d frames) ---\n", n);
    backtrace_symbols_fd(bt, n, 2);
    std::fflush(stderr);
    dump_maps();
    _exit(42);
}

static void install_diagnostics() {
    struct sigaction sa; std::memset(&sa, 0, sizeof sa);
    sa.sa_sigaction = crash_handler; sa.sa_flags = SA_SIGINFO;
    sigemptyset(&sa.sa_mask);
    for (int s : {SIGSEGV, SIGBUS, SIGABRT, SIGILL, SIGFPE, SIGALRM}) sigaction(s, &sa, nullptr);
}

static void wd_arm(unsigned sec) { alarm(sec); }
static void wd_disarm() { alarm(0); }

// --analyzer [mode]: validate Sony's AudioAnalyzerService spectrum path in ISOLATION before it is
// ever enabled in the boot shell. Connects, starts the stream, and reports whether frames flow +
// the raw band values (so spectrum::from_bands can be calibrated). Play audio while running this.
// `mode` defaults to 1 (SPECTRUM); pass 0 to try LEVEL if SPECTRUM yields no frames.
static int g_an_mode = 1;

// The analyzer body, run INSIDE Framework::StartForApplication with a Pump() thread going — see
// analyzer_probe below for why that is not optional.
static int analyzer_job(int mode) {
    std::fprintf(stderr, "[cinder-probe] analyzer: cinder_analyzer_start(mode=%d, 20Hz, default) …\n", mode);
    std::fflush(stderr);
    wd_arm(12);
    int rc = cinder_analyzer_start(mode, 20.0f, 0);
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] analyzer: start returned %d (%s)\n", rc,
        rc == 0 ? "OK" :
        rc == -1 ? "dlopen failed (lib absent)" :
        rc == -2 ? "missing symbol (dlsym)" :
        rc == -3 ? "GetInstance returned NULL" :
        rc == -4 ? "already started" : "?");
    std::fflush(stderr);
    if (rc != 0) return 1;

    clog_("analyzer: watching for spectrum frames (8s) — play audio now …");
    int vals[16];
    for (int i = 0; i < 16; ++i) {
        // Just wait — no render tick. See analyzer_job_entry: this process must not draw.
        int frames = cinder_analyzer_frames();
        if (i % 2 == 0) {
            int n = cinder_analyzer_last(vals, 16);
            std::fprintf(stderr, "[cinder-probe] analyzer: frames=%d bands=%d  vals[0..7]= %d %d %d %d %d %d %d %d\n",
                frames, n, vals[0], vals[1], vals[2], vals[3], vals[4], vals[5], vals[6], vals[7]);
            std::fflush(stderr);
        }
        usleep(500000);
    }
    int total = cinder_analyzer_frames();
    wd_arm(8); cinder_analyzer_stop(); wd_disarm();
    if (total > 0) {
        clog_("analyzer: PASS — frames flowed. The visualiser will work; note the printed band range "
              "(spectrum::from_bands auto-detects dBFS vs linear, so it should need no change). The "
              "shell enables the analyzer BY DEFAULT — cinder_viz.conf only turns it OFF.");
        return 0;
    }
    clog_("analyzer: started but NO frames — try the other mode (--analyzer 0), confirm audio is "
          "playing, or the service emits only while its own screen is foregrounded.");
    return 2;
}

static void analyzer_job_entry() {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    std::fprintf(stderr, "[cinder-probe] analyzer: job running (fw=%p) — starting Pump() thread\n",
                 (void*)&fw);
    g_pump_run = true;
    pthread_t th;
    if (pthread_create(&th, nullptr, pump_thread, &fw) != 0) {
        clog_("analyzer: pthread_create FAILED"); _exit(1);
    }
    usleep(300000);
    std::fprintf(stderr, "[cinder-probe] analyzer: %u pump ticks before connect\n", g_pump_ticks);
    // DELIBERATELY NO cinder_render_init(). The Home app is normally running while this probe is
    // used, and it owns the framebuffer: opening fb0 a second time and then calling
    // cinder_render_tick() would paint THIS process's (blank, Lock-screen) UI over the live app.
    // The frame counter lives in the listener and increments whether or not a renderer exists —
    // cinder_set_spectrum simply returns early when there is none — so the diagnostic loses
    // nothing. The old version did init a renderer "so set_spectrum has a target", which was true
    // and unnecessary.
    _exit(analyzer_job(g_an_mode));
}

// --analyzer [mode] — THE ENTRY POINT.
//
// This MUST run with pst::core::Framework started and pumped, and for a long time it did not.
// AudioAnalyzerService is a Sony service client exactly like PlayerService: the call marshals a
// request and the REPLY is dispatched by the framework's event looper. With no looper the
// out-params stay uninitialised and the service never does anything — which is the entire reason
// playback appeared broken for weeks (Connect "returned" a pointer, IsConnected read true from
// stack garbage, and the service logged nothing at all).
//
// So the old version of this probe would have reported "started but NO frames" on a perfectly good
// device, and the obvious conclusion — that the SetPassband fix did not work — would have been
// wrong. Same shape as the bug it was meant to help diagnose.
static int analyzer_probe(int mode) {
    g_an_mode = mode;
    install_diagnostics();
    clog_("analyzer: Framework::GetReference() …");
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    std::fprintf(stderr, "[cinder-probe] analyzer: got Framework=%p BinderLastError=%d\n",
                 (void*)&fw, pst::core::Framework::GetBinderLastError());
    clog_("analyzer: StartForApplication(finish_job, true) …");
    int sr = fw.StartForApplication(std::function<void()>(&pump_finish), true);
    std::fprintf(stderr, "[cinder-probe] analyzer: StartForApplication returned %d\n", sr);
    analyzer_job_entry();
    return 0; // unreachable: analyzer_job_entry _exit()s with the real status
}

// The --pump job: runs INSIDE pst::core::Framework::StartForApplication, i.e. with the framework
// up, plus our own Pump() thread so binder replies get dispatched even though (unlike a Sony app)
// nothing else is driving the loop. Everything else is identical to --play, so a difference in
// outcome isolates exactly one variable: the event loop.
static void pump_job() {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    std::fprintf(stderr, "[cinder-probe] pump: job running (fw=%p) — starting Pump() thread\n",
                 (void*)&fw);
    g_pump_run = true;
    pthread_t th;
    if (pthread_create(&th, nullptr, pump_thread, &fw) != 0) {
        clog_("pump: pthread_create FAILED"); _exit(1);
    }
    usleep(300000);
    std::fprintf(stderr, "[cinder-probe] pump: %u ticks before connect\n", g_pump_ticks);
    clog_("pump: cinder_audio_init(\"cinderprobe\") …");
    wd_arm(12);
    int ai = cinder_audio_init("cinderprobe");
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] pump: audio_init=%d BinderLastError=%d\n",
                 ai, pst::core::Framework::GetBinderLastError());
    int waited = 0;
    while (!cinder_audio_is_connected() && waited < 50) { usleep(100000); ++waited; }
    std::fprintf(stderr, "[cinder-probe] pump: IsConnected=%d after %d ms (%u ticks)\n",
                 cinder_audio_is_connected(), waited * 100, g_pump_ticks);
    // Reclaim a "Music" track leaked into hagodaemon by an earlier session that died without
    // shutting down (SoundService allows exactly one per type). Harmless when nothing is open.
    wd_arm(8);
    cinder_audio_close_player();
    wd_disarm();
    wd_arm(15);
    int pr = cinder_audio_play_tracks(
        const_cast<const char* const*>(g_pump_argv + 2), g_pump_argc - 2, 0);
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] pump: play_tracks=%d BinderLastError=%d\n",
                 pr, pst::core::Framework::GetBinderLastError());
    char uri[512];
    bool resumed = false;
    for (int i = 1; i <= 14; ++i) {
        sleep(1);
        int cur = -1, tot = -1;
        cinder_audio_position(&cur, &tot);
        wd_arm(8);
        int n = cinder_audio_current_uri(uri, sizeof uri);
        wd_disarm();
        std::fprintf(stderr,
                     "[cinder-probe] pump: t+%ds ticks=%u events=%u pos=%d/%d uri(%d)=%s\n",
                     i, g_pump_ticks, cinder_audio_listener_events(), cur, tot,
                     n, n > 0 ? uri : "(none)");
        // The graph reaches OMX_StatePause and stays there: position never moves. Escalate ONCE
        // to Resume() — the engine-level unpause — and keep watching the same counters, so the
        // log shows unambiguously whether Resume is the missing Play transition.
        if (!resumed && i >= 3 && cur <= 0) {
            resumed = true;
            clog_("pump: position not advancing — trying Resume() …");
            wd_arm(8); cinder_audio_resume(); wd_disarm();
        }
        // Which PCM device did Sony's stack actually open? This is the CPU-vs-hardware-DAC
        // answer: hw:0,4 = cxd3778gf-icx-lowpower (the low-power S-Master DAC path),
        // 0 = hires-out, 1 = standard, 2/3 = DSD.
        if (i == 5 || i == 12) {
            for (int d = 0; d <= 5; ++d) {
                char p[96];
                std::snprintf(p, sizeof p, "/proc/asound/card0/pcm%dp/sub0/status", d);
                FILE* f = std::fopen(p, "r");
                if (!f) continue;
                char st[64] = {0};
                if (std::fgets(st, sizeof st, f)) {
                    char* nl = std::strchr(st, '\n'); if (nl) *nl = 0;
                    if (std::strcmp(st, "closed") != 0)
                        std::fprintf(stderr, "[cinder-probe] pump:   ALSA pcm%dp = %s\n", d, st);
                }
                std::fclose(f);
            }
        }
        std::fflush(stderr);
    }
    clog_("pump: DONE — is audio playing?");
    // Release the player before dying, or the next run inherits a stuck Music track.
    // CINDER_KEEPPLAYING=1 leaves it running (to listen to the result over the headphones).
    const char* keep = getenv("CINDER_KEEPPLAYING");
    if (!(keep && keep[0] == '1')) { wd_arm(8); cinder_audio_close_player(); wd_disarm(); }
    std::fflush(stderr);
    // _exit for the same reason as --play: static teardown runs through Sony's stale vtables.
    _exit(pr == 0 ? 0 : 1);
}

// ── --transport : the Phase-3 playback unknowns, settled by measurement not by eye ──────────────
//
// Four items in DEVICE_CHECKLIST Phase 3 share a shape — a value RE recovered that nothing on the
// device ever confirmed — and all four are visible in the position/URI the listener already
// reports, so none of them actually needs a finger on the glass:
//
//   3a play-by-index   play_tracks(paths, n, start=1) must start on the SECOND path
//   3c drag-to-seek    media_origin_t::Begin == 0 — seek to a known ms, read the position back
//   3e repeat-one      OneTrackMode::On (== 2, measured 2026-08-26) — park near the end, see if
//                      the SAME uri restarts
//   3f queue end       no repeat-all primitive is known; this records what the state DOES
//
// Runs inside StartForApplication with the pump going, exactly like --pump; without that every
// value below is uninitialised stack.
//   --transport <trackA> <trackB>
static const char* base_(const char* p) {
    const char* s = std::strrchr(p, '/');
    return s ? s + 1 : p;
}

// Wait for the position to be reported at all (the listener needs one update), then settle.
static int tpos_(int* cur, int* tot, int tries) {
    for (int i = 0; i < tries; ++i) {
        sleep(1);
        *cur = -1; *tot = -1;
        if (cinder_audio_position(cur, tot) && *tot > 0) return 1;
    }
    return 0;
}

// The engine REJECTS SeekTime while it is streaming — MediaEnginePlayer.cc:221 logs
// "SeekTime(): Bad parameter. ignored" for every origin and every offset, including 0.
// cinder_audio_seek_ms() already knows this and wraps the call in a transport-level
// pause/resume; cinder_audio_seek_ms_origin() is the RAW call and does not. Driving the raw
// one mid-playback is what made 3c and 3e read INCONCLUSIVE on 2026-08-26 — the seek never
// happened, so 3e never reached the end either. Pause around it, exactly like the shipping path.
static int seek_origin_paused(int origin, int ms) {
    const bool resume = cinder_audio_is_playing() != 0;
    if (resume) cinder_audio_pause();
    int rc = cinder_audio_seek_ms_origin(origin, ms);
    if (resume) cinder_audio_play();
    return rc;
}

// ── --repeatsweep : find the real OneTrackMode::On ───────────────────────────────────────────
// {Off=0,On=1} is an assumption in playerservice_abi.hpp that the device refused on 2026-08-26:
// repeat-one applied BEFORE SetTrackSequence still let the track run to the end and stop. The
// value is a plain int serialised inside the sequence, so sweep it and watch for a wrap. Each
// value costs one seek to 6 s from the end plus ~10 s of watching.
static void repeatsweep_job() {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    g_pump_run = true;
    pthread_t th;
    if (pthread_create(&th, nullptr, pump_thread, &fw) != 0) {
        clog_("repeatsweep: pthread_create FAILED"); _exit(1);
    }
    usleep(300000);
    int ai = cinder_audio_init("cinder-probe-repeatsweep");
    std::fprintf(stderr, "[cinder-probe] repeatsweep: audio_init=%d IsConnected=%d\n",
                 ai, cinder_audio_is_connected());
    if (ai != 0) { clog_("repeatsweep: not connected"); _exit(1); }

    const char* T = g_pump_argv[2];
    int lo = 0, hi = 7;
    if (g_pump_argc > 3) lo = hi = atoi(g_pump_argv[3]);
    int cur = 0, tot = 0, n = 0; char uri[512];

    for (int v = lo; v <= hi; ++v) {
        std::fprintf(stderr, "[cinder-probe] repeatsweep: ── OneTrackMode = %d ──\n", v);
        cinder_audio_set_one_track_raw(v);
        const char* solo[1] = { T };
        wd_arm(15); cinder_audio_play_tracks(solo, 1, 0); wd_disarm();
        char base0[512] = {0};
        wd_arm(8); cinder_audio_current_uri(base0, sizeof base0); wd_disarm();
        if (!tpos_(&cur, &tot, 8) || tot < 12000) { clog_("repeatsweep: no position/duration"); continue; }
        wd_arm(10); seek_origin_paused(0, tot - 6000); wd_disarm();
        int wrapped = 0;
        for (int i = 0; i < 12; ++i) {
            sleep(1);
            cinder_audio_position(&cur, &tot);
            wd_arm(8); n = cinder_audio_current_uri(uri, sizeof uri); wd_disarm();
            int playing = cinder_audio_is_playing();
            std::fprintf(stderr, "[cinder-probe] repeatsweep: v=%d t+%ds pos=%d/%d playing=%d uri=%s\n",
                         v, i + 1, cur, tot, playing, n > 0 ? base_(uri) : "(none)");
            if (cur >= 0 && cur < 5000 && playing) { wrapped = 1; break; }
        }
        std::fprintf(stderr, "[cinder-probe] repeatsweep: v=%d -> %s\n",
                     v, wrapped ? "REPEATED (this is On)" : "stopped at end");
        if (wrapped) {
            std::fprintf(stderr, "[cinder-probe] repeatsweep: RESULT OneTrackMode::On == %d\n", v);
            break;
        }
    }
    cinder_audio_set_one_track_raw(-1);
    cinder_audio_release_sequence();
}

static void transport_job() {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    std::fprintf(stderr, "[cinder-probe] transport: job running (fw=%p) — starting Pump()\n",
                 (void*)&fw);
    g_pump_run = true;
    pthread_t th;
    if (pthread_create(&th, nullptr, pump_thread, &fw) != 0) {
        clog_("transport: pthread_create FAILED"); _exit(1);
    }
    usleep(300000);
    wd_arm(12); int ai = cinder_audio_init("cindertrans"); wd_disarm();
    int waited = 0;
    while (!cinder_audio_is_connected() && waited < 50) { usleep(100000); ++waited; }
    std::fprintf(stderr, "[cinder-probe] transport: audio_init=%d IsConnected=%d\n",
                 ai, cinder_audio_is_connected());
    if (ai != 0) { clog_("transport: not connected — nothing below would mean anything"); _exit(1); }
    wd_arm(8); cinder_audio_close_player(); wd_disarm();

    const char* A = g_pump_argv[2];
    const char* B = g_pump_argv[3];
    char uri[512];
    int cur = -1, tot = -1;
    int pass3a = 0, pass3c = 0, pass3e = 0;

    // ── 3a — play-by-index ───────────────────────────────────────────────────────────────────
    clog_("transport: [3a] play_tracks({A,B}, start=1) — expect the SECOND track");
    const char* two[2] = { A, B };
    wd_arm(15);
    int pr = cinder_audio_play_tracks(two, 2, 1);
    wd_disarm();
    tpos_(&cur, &tot, 8);
    wd_arm(8); int n = cinder_audio_current_uri(uri, sizeof uri); wd_disarm();
    std::fprintf(stderr, "[cinder-probe] transport: [3a] play_tracks=%d pos=%d/%d uri=%s\n",
                 pr, cur, tot, n > 0 ? base_(uri) : "(none)");
    if (n > 0 && std::strcmp(base_(uri), base_(B)) == 0) {
        pass3a = 1; clog_("transport: [3a] PASS — start index selected the second path");
    } else {
        std::fprintf(stderr, "[cinder-probe] transport: [3a] FAIL — wanted %s\n", base_(B));
    }

    // ── 3c — seek origin: is media_origin_t::Begin really 0? ─────────────────────────────────
    // Begin=0 means the ms is measured from the START of the track, so seeking to 60 s must land
    // at ~60 s whatever the current position is. If 0 were really Current, it would land at
    // now+60 s instead — which is the 2026-07-28 bug: the bar follows the finger, audio does not.
    if (tot > 70000) {
        clog_("transport: [3c] seek_ms_origin(origin=0, 60000) — expect ~60000, not now+60000");
        // Let the engine settle first. Seeking at pos=73 ms — before the demuxer has really
        // started — read back 0/0 and made this INCONCLUSIVE on an otherwise good run.
        for (int w = 0; w < 20 && cur < 1000; ++w) { usleep(200000); cinder_audio_position(&cur, &tot); }
        int before = cur;
        wd_arm(10); int sr = seek_origin_paused(0, 60000); wd_disarm();
        sleep(3);
        cinder_audio_position(&cur, &tot);
        std::fprintf(stderr, "[cinder-probe] transport: [3c] seek rc=%d before=%d after=%d/%d\n",
                     sr, before, cur, tot);
        if (cur > 56000 && cur < 70000) {
            pass3c = 1;
            clog_("transport: [3c] PASS — origin 0 is BEGIN (absolute); drag-to-seek lands where dropped");
        } else if (cur > before + 50000) {
            clog_("transport: [3c] FAIL — origin 0 behaves as CURRENT (relative). Begin is NOT 0.");
        } else {
            clog_("transport: [3c] INCONCLUSIVE — position did not move as either origin predicts");
        }
    } else {
        clog_("transport: [3c] SKIPPED — track shorter than 70 s, pick a longer one");
    }

    // ── 3e — repeat-one: does the OneTrackMode::On we ship actually repeat? ──────────────────
    // Park 6 s from the end with repeat-one ON. If On==1 the SAME uri restarts near zero; if the
    // enum is wrong the sequence advances to the next track instead.
    clog_("transport: [3e] set_repeat_one(1), then park 6 s from the end …");
    wd_arm(8); int rr = cinder_audio_set_repeat_one(1); wd_disarm();
    std::fprintf(stderr, "[cinder-probe] transport: [3e] set_repeat_one=%d (0=applied live)\n", rr);
    char before_uri[512] = {0};
    wd_arm(8); cinder_audio_current_uri(before_uri, sizeof before_uri); wd_disarm();
    if (tot > 12000) {
        wd_arm(10); seek_origin_paused(0, tot - 6000); wd_disarm();
        int wrapped = 0, changed = 0;
        for (int i = 0; i < 14; ++i) {
            sleep(1);
            cinder_audio_position(&cur, &tot);
            wd_arm(8); n = cinder_audio_current_uri(uri, sizeof uri); wd_disarm();
            if (n > 0 && std::strcmp(base_(uri), base_(before_uri)) != 0) changed = 1;
            if (cur >= 0 && cur < 5000) wrapped = 1;
            std::fprintf(stderr, "[cinder-probe] transport: [3e] t+%ds pos=%d/%d uri=%s\n",
                         i + 1, cur, tot, n > 0 ? base_(uri) : "(none)");
            if (wrapped || changed) break;
        }
        if (wrapped && !changed) {
            pass3e = 1;
            clog_("transport: [3e] PASS — same track restarted: the OneTrackMode::On we ship (2) repeats");
        } else if (changed) {
            clog_("transport: [3e] FAIL — the sequence ADVANCED with repeat-one on: wrong On value");
        } else {
            clog_("transport: [3e] INCONCLUSIVE — never reached the end inside the window");
        }
    }

    // ── 3e-sticky — the SAME question, asked down the path that actually works ───────────────
    // 3e above sets repeat-one on a sequence the service is ALREADY pulling from. player_shim's
    // own comment flags that as the unsynchronised case: SetOneTrackMode there is a store into an
    // object we own, made after SetTrackSequence has already handed the service its copy. If that
    // is the reason 3e did not repeat, then the enum is fine and only the LIVE toggle is broken —
    // a different bug with a different fix. cinder_audio_play_tracks applies the sticky flag
    // BEFORE SetTrackSequence, so setting it first and building a fresh single-track sequence
    // asks purely "is the shipped OneTrackMode::On right", with no live-toggle in the way.
    clog_("transport: [3e-sticky] set_repeat_one(1) FIRST, then a fresh single-track sequence …");
    wd_arm(8); int rs = cinder_audio_set_repeat_one(1); wd_disarm();
    std::fprintf(stderr, "[cinder-probe] transport: [3e-sticky] set_repeat_one=%d (1=sticky only, no live seq)\n", rs);
    {
        const char* solo[1] = { B };
        wd_arm(15); cinder_audio_play_tracks(solo, 1, 0); wd_disarm();
        char s_uri[512] = {0};
        wd_arm(8); cinder_audio_current_uri(s_uri, sizeof s_uri); wd_disarm();
        if (tpos_(&cur, &tot, 8) && tot > 12000) {
            wd_arm(10); seek_origin_paused(0, tot - 6000); wd_disarm();
            int wrapped = 0, changed = 0;
            for (int i = 0; i < 14; ++i) {
                sleep(1);
                cinder_audio_position(&cur, &tot);
                wd_arm(8); n = cinder_audio_current_uri(uri, sizeof uri); wd_disarm();
                if (n > 0 && std::strcmp(base_(uri), base_(s_uri)) != 0) changed = 1;
                if (cur >= 0 && cur < 5000) wrapped = 1;
                std::fprintf(stderr,
                             "[cinder-probe] transport: [3e-sticky] t+%ds pos=%d/%d playing=%d uri=%s\n",
                             i + 1, cur, tot, cinder_audio_is_playing(), n > 0 ? base_(uri) : "(none)");
                if (wrapped || changed) break;
            }
            if (wrapped && !changed) {
                pass3e = 1;
                clog_("transport: [3e-sticky] PASS — the shipped OneTrackMode::On IS correct; the live "
                      "toggle in cinder_audio_set_repeat_one is what does not reach the service.");
            } else if (changed) {
                clog_("transport: [3e-sticky] FAIL — advanced even on the sticky path: wrong On value");
            } else {
                clog_("transport: [3e-sticky] FAIL — parked at the end and stopped. Repeat-one does "
                      "not repeat on either path; wrong On value, or the mode needs more than this enum.");
            }
        }
    }

    // ── 3f — what happens when the queue runs out (no repeat-all primitive is known) ──────────
    // Not a pass/fail: the checklist asks for an OBSERVATION of the play state at the boundary.
    clog_("transport: [3f] repeat-one OFF, single-track sequence, park 6 s from the end …");
    wd_arm(8); cinder_audio_set_repeat_one(0); wd_disarm();
    const char* one[1] = { A };
    wd_arm(15); cinder_audio_play_tracks(one, 1, 0); wd_disarm();
    if (tpos_(&cur, &tot, 8) && tot > 12000) {
        wd_arm(10); seek_origin_paused(0, tot - 6000); wd_disarm();
        for (int i = 0; i < 14; ++i) {
            sleep(1);
            cinder_audio_position(&cur, &tot);
            wd_arm(8); n = cinder_audio_current_uri(uri, sizeof uri); wd_disarm();
            std::fprintf(stderr,
                         "[cinder-probe] transport: [3f] t+%ds pos=%d/%d playing=%d state=%u uri=%s\n",
                         i + 1, cur, tot, cinder_audio_is_playing(), cinder_audio_play_state(),
                         n > 0 ? base_(uri) : "(none)");
        }
        clog_("transport: [3f] OBSERVATION recorded above — read the state/playing columns at the "
              "boundary; that is what a repeat-all would have to override.");
    }

    std::fprintf(stderr, "[cinder-probe] transport: SUMMARY 3a=%s 3c=%s 3e=%s (3f is an observation)\n",
                 pass3a ? "PASS" : "FAIL", pass3c ? "PASS" : "FAIL/INCONCLUSIVE",
                 pass3e ? "PASS" : "FAIL/INCONCLUSIVE");
    const char* keep = getenv("CINDER_KEEPPLAYING");
    if (!(keep && keep[0] == '1')) { wd_arm(8); cinder_audio_close_player(); wd_disarm(); }
    g_pump_run = false;
    std::fflush(nullptr);
    _exit((pass3a && pass3c && pass3e) ? 0 : 2);
}

// ── --ldac : USB-DAC -> LDAC bring-up, the two questions ldac-bridge/TEST.md asks ────────────
//
// WHY IT LIVES HERE AND NOT IN ldac-bridge. The standalone bridge cannot answer either question
// as written: `libBtTransmitterService` is a `pst::services::*` client, so like every other one on
// this device its calls are ASYNC and their replies are delivered by pst::core::Framework's event
// looper. `ldac-bridge/main.c` starts no framework and pumps nothing, so SetLdac/SetCurrentSource
// never leave the process and GetSocketName returns UNINITIALISED STACK. That failure looks exactly
// like TEST.md's third outcome ("control plane assumption wrong -> go re-do Ghidra"), which is the
// same trap that cost weeks on PlayerService (Connect "returned" 0xb6xxxxxx; IsConnected
// true-from-garbage). Answering it here reuses the framework + pump + watchdog + backtrace that are
// already proven in this binary, and it needs no .UPG flash and no reboot — just an adb push.
//
//   adb push cinder-home/dist/dev/cinder-probe /tmp/cinder-probe
//   adb shell 'chmod 755 /tmp/cinder-probe && LD_LIBRARY_PATH=/system/vendor/sony/lib:/system/lib \
//              /tmp/cinder-probe --ldac'
//
// Q1: does the control plane make BtTransmitterService open its audio socket?
// Q2: can we open the USB-DAC capture PCM, or does Sony's UAC service hold it (-EBUSY)?
// The two are independent, and the whole point is to come back with an answer to EACH — a run that
// dies on Q1 still reports what Q2 would have said.
extern "C" {
// Exported factory. Ghidra typed it void; it returns the client*.
void* _ZN3pst8services33BtTransmitterServiceClientFactory14CreateInstanceEv(void);
}

// Vtable indices into the BtTransmitterServiceClient primary vtable, from
// analysis/E_usbdac_ldac/ghidra/DumpVtable.java (vptr = group_base+8, confirmed against
// CreateInstance; slot 0 = first virtual after the [0,typeinfo] header). Same table
// ldac-bridge/src/btclient.c uses — keep the two in step.
enum {
    VIDX_SetCurrentSource    = 12,   // SetCurrentSource(const bool&)
    VIDX_SetLdacSoundQuality = 18,   // SetLdacSoundQuality(const enum&)
    VIDX_SetLdac             = 20,   // SetLdac(const bool&)
    VIDX_GetSocketName       = 29,   // void GetSocketName(std::string&)  — NOT sret; see below
};

static void* vslot(void* obj, int idx) { return (*(void***)obj)[idx]; }

static int ldac_probe() {
    install_diagnostics();

    // 1. The framework FIRST. Without it every call below reads back stack garbage, and the pump
    //    thread must exist before the first request or its reply has nobody to deliver it.
    clog_("ldac: Framework::GetReference() …");
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    std::fprintf(stderr, "[cinder-probe] ldac: Framework=%p BinderLastError=%d\n",
                 (void*)&fw, pst::core::Framework::GetBinderLastError());
    clog_("ldac: StartForApplication(finish_job, true) …");
    wd_arm(15);
    int sr = fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] ldac: StartForApplication returned %d\n", sr);
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    // WAIT for the looper to actually be turning before the first client call. The first run of
    // this reported `pump ticks so far 0` at CreateInstance — the thread had not been scheduled
    // yet — and a pst client call made with a dead looper returns uninitialised stack, which is
    // the exact trap this probe exists to rule out.
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);
    std::fprintf(stderr, "[cinder-probe] ldac: pump running (%u ticks)\n", g_pump_ticks);

    // 2. Control plane. Each call is watchdog-bounded, so a HANG names itself instead of looking
    //    like a silent failure.
    clog_("ldac: BtTransmitterServiceClientFactory::CreateInstance() …");
    wd_arm(12);
    void* bt = _ZN3pst8services33BtTransmitterServiceClientFactory14CreateInstanceEv();
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] ldac: client=%p (pump ticks so far %u)\n",
                 bt, g_pump_ticks);
    if (!bt) { clog_("ldac: CreateInstance returned NULL — STOP"); return 1; }

    typedef void (*fn_b)(void*, const bool*);
    typedef void (*fn_i)(void*, const int*);
    bool t = true, f = false;
    int  q = 0;   // BtLdacSoundQuality::Auto
    clog_("ldac: SetLdac(true) …");
    wd_arm(12); ((fn_b)vslot(bt, VIDX_SetLdac))(bt, &t); wd_disarm();
    clog_("ldac: SetLdacSoundQuality(Auto) …");
    wd_arm(12); ((fn_i)vslot(bt, VIDX_SetLdacSoundQuality))(bt, &q); wd_disarm();
    clog_("ldac: SetCurrentSource(true) …");
    wd_arm(12); ((fn_b)vslot(bt, VIDX_SetCurrentSource))(bt, &t); wd_disarm();

    // CORRECTED 2026-07-29. This used to be called as if it returned a std::string BY VALUE —
    // `fn(s, bt)`, hidden sret pointer first, `this` second. It does not. The library states its
    // own prototype in .rodata (see reference: __PRETTY_FUNCTION__ literals):
    //
    //     void pst::services::BtTransmitterService::GetSocketName(pst::base::string &)
    //
    // Void return, string by REFERENCE. So the old call handed a 12-byte stack array to the
    // function as `this` and the client object as the out-param — which is why it "threw" with
    // libcxxrt reporting "Fatal error during phase 1 unwinding" and died at PC=0. That was a
    // swapped argument list, NOT evidence about the Bluetooth link, and the previous conclusion
    // ("this call throws unless a link is up") should not be trusted.
    //
    // `pst::base::string` is a typedef, not a distinct class: the mangled form N3pst4base6stringE
    // appears in no symbol anywhere in the vendor tree, while the marshaller's own PLT entry is
    //     TransactionParam::GetStr(std::__1::basic_string<char, ...>&)
    // so it is plain libc++ std::string. This translation unit is compiled against the libc++
    // 3.9.0 headers that match the device runtime, so a real std::string is ABI-correct here and
    // there is nothing left to hand-decode.
    clog_("ldac: GetSocketName(std::string&) …");
    typedef void (*fn_s)(void*, std::string*);
    std::string sock_name;
    bool got_name = false;
    wd_arm(12);
    try {
        ((fn_s)vslot(bt, VIDX_GetSocketName))(bt, &sock_name);
        got_name = true;
    } catch (...) {
        clog_("ldac: GetSocketName THREW");
    }
    wd_disarm();
    char name[128] = {0};
    if (got_name) {
        size_t n = sock_name.size();
        if (n >= sizeof name) n = sizeof name - 1;
        std::memcpy(name, sock_name.data(), n);
        name[n] = '\0';
    }
    std::fprintf(stderr, "[cinder-probe] ldac: Q1 socket name = '%s' (len %zu, pump ticks %u)\n",
                 name, std::strlen(name), g_pump_ticks);
    if (g_pump_ticks == 0)
        clog_("ldac: *** pump never ticked — the framework is NOT running; nothing below is "
              "trustworthy ***");

    // 3. Q1 proper: can we connect to the socket the server should now be listening on? The open is
    //    async after SetCurrentSource, so retry ~2 s before calling it a no.
    int sock = -1;
    if (!got_name) {
        clog_("ldac: Q1 INCONCLUSIVE — GetSocketName threw. With the argument order now corrected "
              "this is NO LONGER the expected outcome, so treat it as a real fault: check the pump "
              "tick count above before blaming the link.");
    } else if (name[0] == '\0') {
        clog_("ldac: Q1 FAIL — GetSocketName returned EMPTY with a live pump and a link up, so "
              "this really is the control plane: re-check the open trigger in FUN_00019aa0's "
              "callers");
    } else {
        // ADDRLEN IS PART OF THE NAME. An abstract AF_UNIX address is a BYTE STRING of length
        // (addrlen - offsetof(sun_path)), not a C string — the kernel compares those bytes exactly,
        // trailing NULs included. BtTransmitterService binds with the FULL sockaddr_un (addrlen 110),
        // so its real name is "pst::services::bttransmitterservice" followed by 72 NULs, and a
        // connect() sized to strlen() asks for a *different* name and gets ECONNREFUSED.
        //
        // This is invisible in /proc/net/unix, which prints the name up to the first NUL and pads the
        // column — the entry looks like an exact match. `od -c` on that line is what shows it: 107
        // bytes after the '@', i.e. 110 - sizeof(sa_family_t) - 1. Measured on device 2026-07-29.
        for (int i = 0; i < 20 && sock < 0; i++) {
            sock = socket(AF_UNIX, SOCK_STREAM, 0);
            struct sockaddr_un a; std::memset(&a, 0, sizeof a);
            a.sun_family = AF_UNIX;
            size_t n = std::strlen(name);
            if (n > sizeof a.sun_path - 1) n = sizeof a.sun_path - 1;
            std::memcpy(a.sun_path + 1, name, n);   // abstract namespace: sun_path[0] stays NUL
            socklen_t len = (socklen_t)sizeof a;    // 110 — match the server's bind exactly
            if (connect(sock, (struct sockaddr*)&a, len) < 0) { close(sock); sock = -1; usleep(100000); }
        }
        if (sock >= 0) clog_("ldac: Q1 PASS — connected to the transmitter's audio socket");
        else std::fprintf(stderr, "[cinder-probe] ldac: Q1 FAIL — connect(@%s): %s\n",
                          name, std::strerror(errno));
    }

    // 4. Q2, asked REGARDLESS of Q1's answer — the two failures need different fixes and one run
    //    should classify both. Card index is dynamic (card4 only exists in UAC mode), so walk the
    //    capture PCMs rather than assuming hw:4,0.
    clog_("ldac: Q2 — probing USB-DAC capture PCMs …");
    bool opened = false;
    for (int card = 0; card < 8 && !opened; card++) {
        char dev[32];
        std::snprintf(dev, sizeof dev, "hw:%d,0", card);
        snd_pcm_t* pcm = nullptr;
        int rc = snd_pcm_open(&pcm, dev, SND_PCM_STREAM_CAPTURE, SND_PCM_NONBLOCK);
        if (rc == 0) {
            std::fprintf(stderr, "[cinder-probe] ldac: Q2 PASS — %s opened for capture\n", dev);
            snd_pcm_close(pcm);
            opened = true;
        } else if (rc == -EBUSY) {
            std::fprintf(stderr, "[cinder-probe] ldac: Q2 FAIL — %s is BUSY (Sony's "
                         "UsbDeviceAudioPlayerService owns it; the fix is contention, not RE)\n", dev);
            opened = true;   // classified: stop walking
        } else if (rc != -ENOENT && rc != -ENODEV) {
            std::fprintf(stderr, "[cinder-probe] ldac: %s -> %s\n", dev, snd_strerror(rc));
        }
    }
    if (!opened)
        clog_("ldac: Q2 INCONCLUSIVE — no capture PCM at all. Is the gadget in UAC mode "
              "(sys.sony.config uac) with a PC actually feeding audio?");

    if (sock >= 0) close(sock);
    clog_("ldac: releasing the source …");
    wd_arm(12); ((fn_b)vslot(bt, VIDX_SetCurrentSource))(bt, &f); wd_disarm();
    g_pump_run = false;
    std::fprintf(stderr, "[cinder-probe] ldac: done (%u pump ticks)\n", g_pump_ticks);
    std::fflush(nullptr);
    _exit(0);   // same reason as eq_probe: do not unwind with the pump thread live
}

// ── --btopen : send the ONE frame that turns the transmitter socket into a PCM pipe ─────────────
//
// Round o concluded that `@pst::services::bttransmitterservice` is a control channel and nothing
// else. That was half right and the wrong half mattered. The frame grammar is real —
//
//     recv 4 -> type | recv 4 -> length | new[](length) | recv payload
//     type 0 => len 0 | 1 => 28 | 2 => 12 | anything else => close
//
// — but a type-1 frame does not merely *configure* the stream, it HANDS THE CONNECTION OVER.
// `BtTransmitterExHal::OnEvent` (libBtTransmitterService.so @0x9fc0) ends the type-1 path with:
//
//     r1 = p[4]; p[4] = 0;          // the accepted connection, moved out of the event
//     old = this->[12];
//     this->[12] = r1;              // it becomes the ExHal's PCM READER
//     if (old) old->release();
//     this->[0x2c] = 1;             // streaming = true
//     pthread_create(stream thread)
//
// and that thread (@0xa714) is a plain pump:
//
//     while (streaming) {
//         if (reader->Read(pcm_buf, pcm_size, &got)) continue;   // vtable slot +8
//         if (!got) break;
//         src->SendData((uint16)got, pcm_buf);                   // BtAvSrcComponentIf, slot +0x2c
//     }
//
// So after the handshake the SAME fd carries raw PCM, and Sony does the LDAC encoding
// (libbluetooth.blueangel.so links libldacBTBC.so) and the AVDTP bookkeeping. `WriteSilent()` is
// the identical loop over a zeroed buffer — that is all the "silence keeper" ever was.
//
// WHY THIS MODE SENDS NO AUDIO BY DEFAULT. Writing PCM while the connection is still in
// frame-parsing mode is what rebooted the device twice on 2026-08-11: two audio samples get read as
// a type and a length, and a garbage length reaches `operator new[]` inside a core service. So the
// default run sends the handshake and then does nothing but watch the socket. A peer that keeps the
// connection open accepted it; a peer that closes rejected it. Only `--btopen silence` follows up
// with zeros, and only if the connection actually stayed open.
//
//   adb push cinder-home/dist/dev/cinder-probe /tmp/cinder-probe
//   adb shell 'chmod 755 /tmp/cinder-probe && LD_LIBRARY_PATH=/system/vendor/sony/lib:/system/lib \
//              /tmp/cinder-probe --btopen'            # handshake only
//   ... --btopen silence                              # handshake, then 3 s of zeros
//   ... --btopen tone [rate] [chans] [secs]           # handshake, then an audible 440 Hz sine
//
// Headphones must be connected first: the handler requires (avsrc_status & ~1) == 4.
static int btopen_probe(bool send_silence, bool send_tone, unsigned rate, unsigned chans,
                        unsigned secs) {
    install_diagnostics();

    clog_("btopen: Framework::GetReference() …");
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);
    if (g_pump_ticks == 0) {
        clog_("btopen: pump never ticked — every call below would read back stack garbage. STOP");
        return 1;
    }

    wd_arm(12);
    void* bt = _ZN3pst8services33BtTransmitterServiceClientFactory14CreateInstanceEv();
    wd_disarm();
    if (!bt) { clog_("btopen: CreateInstance returned NULL — STOP"); return 1; }

    typedef void (*fn_b)(void*, const bool*);
    typedef void (*fn_i)(void*, const int*);
    bool t = true, f = false;
    int  q = 0;   // BtLdacSoundQuality::Auto
    clog_("btopen: SetLdac(true) / SetLdacSoundQuality(Auto) / SetCurrentSource(true) …");
    wd_arm(12); ((fn_b)vslot(bt, VIDX_SetLdac))(bt, &t); wd_disarm();
    wd_arm(12); ((fn_i)vslot(bt, VIDX_SetLdacSoundQuality))(bt, &q); wd_disarm();
    wd_arm(12); ((fn_b)vslot(bt, VIDX_SetCurrentSource))(bt, &t); wd_disarm();

    typedef void (*fn_s)(void*, std::string*);
    std::string sock_name;
    wd_arm(12);
    try { ((fn_s)vslot(bt, VIDX_GetSocketName))(bt, &sock_name); }
    catch (...) { clog_("btopen: GetSocketName THREW — STOP"); return 1; }
    wd_disarm();
    if (sock_name.empty()) { clog_("btopen: GetSocketName returned EMPTY — STOP"); return 1; }
    std::fprintf(stderr, "[cinder-probe] btopen: socket '%s'\n", sock_name.c_str());

    // addrlen 110, not strlen — see the note in ldac_probe: the server binds the full sockaddr_un
    // and an abstract name is a byte string, trailing NULs included.
    int sock = -1;
    for (int i = 0; i < 20 && sock < 0; i++) {
        sock = socket(AF_UNIX, SOCK_STREAM, 0);
        struct sockaddr_un a; std::memset(&a, 0, sizeof a);
        a.sun_family = AF_UNIX;
        size_t n = sock_name.size();
        if (n > sizeof a.sun_path - 1) n = sizeof a.sun_path - 1;
        std::memcpy(a.sun_path + 1, sock_name.data(), n);
        if (connect(sock, (struct sockaddr*)&a, (socklen_t)sizeof a) < 0) {
            close(sock); sock = -1; usleep(100000);
        }
    }
    if (sock < 0) {
        std::fprintf(stderr, "[cinder-probe] btopen: connect failed: %s — STOP\n",
                     std::strerror(errno));
        return 1;
    }
    clog_("btopen: connected");

    // The frame. Eight bytes of header then exactly 28 bytes of payload — the length word is what
    // the service allocates on, so it is written as a constant and never computed from anything
    // that could be short.
    unsigned char frame[8 + 28];
    std::memset(frame, 0, sizeof frame);
    unsigned type = 1, len = 28;
    std::memcpy(frame + 0, &type, 4);
    std::memcpy(frame + 4, &len,  4);
    // Payload fields OnEvent actually reads. Everything else stays zero: the handler does not touch
    // +0/+8/+12/+16, and inventing values for fields we have not identified is how the last round
    // went wrong.
    std::memcpy(frame + 8 + 4,  &chans, 4);   // channel count: 1 stays 1, anything else becomes 2
    frame[8 + 20] = 1;                        // the u8 flag at payload+20
    std::memcpy(frame + 8 + 24, &rate,  4);   // sample rate in Hz, checked against BtSoundFrequency
    std::fprintf(stderr, "[cinder-probe] btopen: sending type=1 len=28 chans=%u rate=%u\n",
                 chans, rate);
    ssize_t w = send(sock, frame, sizeof frame, MSG_NOSIGNAL);
    if (w != (ssize_t)sizeof frame) {
        std::fprintf(stderr, "[cinder-probe] btopen: send returned %zd (%s) — STOP\n",
                     w, std::strerror(errno));
        close(sock);
        return 1;
    }

    // Verdict by observation. An accepted frame leaves the connection open with the stream thread
    // blocked in Read(); a rejected one gets closed by the service. Three seconds is far longer
    // than either decision takes.
    bool alive = true;
    for (int i = 0; i < 30 && alive; i++) {
        struct pollfd p = { sock, POLLIN, 0 };
        int pr = poll(&p, 1, 100);
        if (pr > 0) {
            if (p.revents & (POLLHUP | POLLERR)) { alive = false; break; }
            char b[64];
            ssize_t r = recv(sock, b, sizeof b, MSG_DONTWAIT);
            if (r == 0) { alive = false; break; }
            if (r > 0)
                std::fprintf(stderr, "[cinder-probe] btopen: peer sent %zd bytes back\n", r);
        }
    }

    if (!alive) {
        clog_("btopen: REJECTED — the service closed the connection. The payload layout or the "
              "AvSrc state is wrong; do NOT send PCM on this shape of frame.");
        close(sock);
        g_pump_run = false;
        std::fflush(nullptr);
        _exit(2);
    }
    clog_("btopen: ACCEPTED — connection still open 3 s after the handshake, which means the ExHal "
          "took it as its PCM reader and its stream thread is now blocked reading this fd.");

    if (send_silence || send_tone) {
        // Zeros are exactly what WriteSilent() already pushes, so `silence` adds no failure mode
        // beyond "was the handshake really accepted" — which the gate above just answered. The tone
        // is the same write path with audible content, because a silent run cannot tell "the bytes
        // reached the headphones" apart from "the bytes went into a bit bucket". S16_LE stereo is
        // assumed: it is the only interleaved 16-bit layout the ExHal's chunk sizes make sense for,
        // and if the assumption is wrong the tone comes out as noise, which is still an answer.
        std::fprintf(stderr, "[cinder-probe] btopen: writing %u s of %s (S16_LE, %u ch, %u Hz) …\n",
                     secs, send_tone ? "440 Hz tone" : "silence", chans, rate);
        std::vector<unsigned char> buf(4096, 0);
        const size_t frame_bytes = (size_t)chans * 2;
        const size_t total = (size_t)rate * frame_bytes * secs;
        size_t sent = 0;
        unsigned phase = 0;
        while (sent < total) {
            size_t want = buf.size() - (buf.size() % frame_bytes);
            if (want > total - sent) want = total - sent;
            if (send_tone) {
                for (size_t i = 0; i + frame_bytes <= want; i += frame_bytes) {
                    // 440 Hz at a third of full scale — loud enough to hear, quiet enough not to
                    // hurt if the headphones are already on someone's head.
                    double th = 2.0 * 3.14159265358979 * 440.0 * (double)phase / (double)rate;
                    short s = (short)(10000.0 * sin(th));
                    phase++;
                    for (unsigned c = 0; c < chans; c++)
                        std::memcpy(&buf[i + c * 2], &s, 2);
                }
            }
            ssize_t n = send(sock, buf.data(), want, MSG_NOSIGNAL);
            if (n <= 0) {
                std::fprintf(stderr, "[cinder-probe] btopen: send stopped after %zu bytes: %s\n",
                             sent, std::strerror(errno));
                break;
            }
            sent += (size_t)n;
        }
        std::fprintf(stderr, "[cinder-probe] btopen: wrote %zu of %zu bytes\n", sent, total);
    }

    // Closing the fd is the documented stop: the stream thread's Read returns 0 and the loop ends.
    close(sock);
    clog_("btopen: closed — releasing the source");
    wd_arm(12); ((fn_b)vslot(bt, VIDX_SetCurrentSource))(bt, &f); wd_disarm();
    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

// ── --btinfo : confirm on device what static analysis says about the container-shaped calls ─────
//
// Three things were recovered by reading the libraries rather than by running them. This mode is the
// device half — each one either prints a plausible value or faults, and a fault here costs a probe
// process rather than the Home app.
//
// 1. `GetConnectInformation` takes TWO out-params, both by reference:
//
//        bool GetConnectInformation(pst::base::vector<uint8_t>& addr, pst::base::string& name)
//
//    Recovered from the stub's prologue (`sl = r1`, `r8 = r2`) plus what each register is then used
//    for: `r8` is handed to `TransactionParam::GetStr`, while `sl` is walked as {begin,end,cap} and
//    grown one byte at a time by a `Get(1)` loop counted by a preceding `Get(4)` — a push_back of a
//    MAC address. Every earlier attempt passed a SINGLE pointer (an int*, then a std::string*), so
//    the push_back wrote through whatever followed it. That is why the fault address was IDENTICAL
//    across two runs: not a bad buffer, a missing second argument.
//
// 2. `pst::base::vector<T>` is libc++ `std::__1::vector<T>` and `pst::base::string` is libc++
//    `std::string` — both typedefs, not distinct classes. Evidence: the mangled forms
//    `N3pst4base6stringE` / `N3pst4base6vectorI…E` appear in NO symbol anywhere in the vendor tree
//    (a real class would mangle as itself), the marshaller's own PLT entry is
//    `TransactionParam::GetStr(std::__1::basic_string<char,…>&)`, and the push_back above touches
//    exactly three pointers at +0/+4/+8, which is libc++'s vector layout. This file compiles against
//    the libc++ 3.9.0 headers matching the device runtime, so real containers are ABI-correct.
//
// 3. `BtLdacSoundQuality`'s numeric values are still unknown. Cinder passes its own UI index
//    (0 Auto, 1 990, 2 660, 3 330); the service logs whatever it receives as `ldac quality:%d`, so
//    this walks 0..3 and the answer comes from logcat.
// Declared here as well as at --bt's own block below: this mode comes first in the file and needs it.
extern "C" void* _ZN3pst8services28BtCommonServiceClientFactory14CreateInstanceEv(void);
extern "C" void* _ZN3pst8services23NfcServiceClientFactory14CreateInstanceEv(void);

// The element of `GetPairedDeviceInfo`'s vector, recovered from the raw dump this mode prints (2
// devices in 96 bytes → a 48-byte stride) and confirmed by both libc++ string forms decoding through
// it. File-scope because --btconnect needs the same declaration; cinder-home carries its own copy.
struct BtPairedDeviceInformation {
    std::vector<unsigned char> addr;    // +0   end-begin == 6 → the MAC
    unsigned                   cod;     // +12  Bluetooth Class of Device (0x240404 = A/V headset)
    std::vector<unsigned char> key;     // +16  16 bytes — link key; the UI never needs it
    std::string                name;    // +28  SSO on one real device, heap on the other
    unsigned char              f0, f1;  // +40  flags — 1,1 on both
    unsigned char              pad[6];  // +42 -> 48
};
// HOST SYNTAX CHECK: this is a fact about the DEVICE's 32-bit libc++ 3.9 layout (vector 12 B,
// string 12 B), so it cannot hold on a 64-bit libstdc++ host, where the same struct is 96 bytes.
// tools/host_syntax_check.sh defines CINDER_HOST_SYNTAX_ONLY to skip it; every real build — which
// is the only one whose answer means anything here — still asserts it.
#ifndef CINDER_HOST_SYNTAX_ONLY
static_assert(sizeof(BtPairedDeviceInformation) == 48, "paired-device stride is not 48");
#endif

// Format a BD address for the log. Empty in → "(none)".
static void mac_str(const std::vector<unsigned char>& a, char* out, size_t cap) {
    if (a.empty()) { std::snprintf(out, cap, "(none)"); return; }
    size_t n = 0;
    for (size_t b = 0; b < a.size() && n + 4 < cap; b++)
        n += std::snprintf(out + n, cap - n, b ? ":%02X" : "%02X", a[b]);
}

enum { BL_GetAvSrc = 3, BL_GetAvrcp = 4, BL_GetConnInfo = 5, BL_RequestConnection = 6,
       BL_RequestLastDeviceConnection = 7, BL_RequestDisconnection = 8,
       BL_RequestStartConnectWait = 10, BL_RequestStopConnectWait = 11,
       BL_SetConnectRetryMode = 27, BL_GetConnectRetryMode = 28 };
enum { BL_GetBtStatus = 3, BL_SetRfOnOff = 4, BL_GetPairedDeviceInfo = 20,
       BL_GetRssi = 25, BL_SetHciLogEnabled = 26,
       BL_CmnAddListener = 30, BL_CmnRemoveListener = 31 };

static double bl_now() {
    struct timeval tv;
    gettimeofday(&tv, nullptr);
    return (double)tv.tv_sec + tv.tv_usec / 1e6;
}

// One line of "where is the link right now", cheap enough to run four times a second.
struct BlState {
    int bt = -1, avsrc = -1, avrcp = -1;
    std::vector<unsigned char> addr;
    std::string name;
    bool linked() const { return !addr.empty(); }
};

static void bl_read(void* x, void* cmn, BlState& s) {
    typedef int (*fn0)(void*);
    typedef int (*fn2)(void*, std::vector<unsigned char>*, std::string*);
    s.addr.clear();
    s.name.clear();
    try {
        if (cmn) s.bt    = ((fn0)vslot(cmn, BL_GetBtStatus))(cmn);
        if (x)   s.avsrc = ((fn0)vslot(x, BL_GetAvSrc))(x);
        if (x)   s.avrcp = ((fn0)vslot(x, BL_GetAvrcp))(x);
        if (x)   ((fn2)vslot(x, BL_GetConnInfo))(x, &s.addr, &s.name);
    } catch (...) { clog_("btlink: a status read threw"); }
}

static void bl_print(const BlState& s, double t0, const char* tag) {
    char mac[24] = "-";
    if (s.addr.size() == 6) mac_str(s.addr, mac, sizeof mac);
    std::fprintf(stderr, "[cinder-probe] btlink: t+%6.2fs  bt=%d avsrc=%d avrcp=%d  %s '%s'  %s\n",
                 bl_now() - t0, s.bt, s.avsrc, s.avrcp, mac, s.name.c_str(), tag ? tag : "");
}

// Poll until a link appears (or `secs` runs out), printing only when something CHANGES — a status
// line every 250 ms is unreadable, and the transitions are the measurement.
static bool bl_wait_link(void* x, void* cmn, double t0, int secs, const char* what) {
    BlState prev;
    bl_read(x, cmn, prev);
    bl_print(prev, t0, "(before)");
    const double deadline = bl_now() + secs;
    while (bl_now() < deadline) {
        usleep(250000);
        BlState s;
        bl_read(x, cmn, s);
        if (s.bt != prev.bt || s.avsrc != prev.avsrc || s.avrcp != prev.avrcp ||
            s.addr != prev.addr) {
            bl_print(s, t0, s.linked() ? "<== LINK" : "");
            prev = s;
            if (s.linked()) {
                std::fprintf(stderr, "[cinder-probe] btlink: *** %s produced a link in %.2f s ***\n",
                             what, bl_now() - t0);
                return true;
            }
        }
    }
    std::fprintf(stderr, "[cinder-probe] btlink: %s — NO link after %d s\n", what, secs);
    return false;
}

// ── --fontchain : what one codepoint costs, on the device, in real memory ──────────────────────
//
// `text::resolve` walks the Sony fallback chain for any codepoint the bundled fonts lack, LOADING
// each font in turn until one covers it. For a codepoint nothing covers, that loads all five —
// including DFPGothicPW5 (10 MB on disk, far more once parsed) — on a 467 MB device. The host
// tests cannot see this: on a desktop it is free.
//
// Written 2026-08-19 chasing "the device crashes on the 3rd page of the welcome screens". That
// page is the only one whose text contains U+25B8 (the "Settings ▸ Theme" arrow), and U+25B8 is in
// NONE of the bundled fonts and NONE of the five Sony fallbacks — the worst case above.
//
// Read-only and self-contained: it renders nothing, touches no service, and prints VmRSS around
// each character so the cost lands on the exact codepoint that caused it.
extern "C" int cinder_font_probe(unsigned cp);

static long fc_rss_kb() {
    FILE* f = std::fopen("/proc/self/status", "r");
    if (!f) return -1;
    char line[256];
    long kb = -1;
    while (std::fgets(line, sizeof line, f))
        if (std::strncmp(line, "VmRSS:", 6) == 0) { kb = std::atol(line + 6); break; }
    std::fclose(f);
    return kb;
}

static int fontchain_probe(int argc, char** argv) {
    struct { unsigned cp; const char* what; } probes[] = {
        { 0x0041, "'A'  ASCII — never walks the chain" },
        { 0x00B7, "'.'  U+00B7 middle dot — bundled" },
        { 0x2014, "'-'  U+2014 em dash — bundled" },
        { 0x25C1, "     U+25C1 white left triangle — JetBrains Mono has it (Controls page)" },
        { 0x25B8, "     U+25B8 black right small triangle — NOTHING has it (Features page)" },
        { 0x65E5, "     U+65E5 CJK 'day' — a real fallback hit" },
    };
    unsigned only = 0;
    if (argc > 2) only = (unsigned)std::strtoul(argv[2], nullptr, 0);

    std::fprintf(stderr, "[cinder-probe] fontchain: VmRSS at start = %ld kB\n", fc_rss_kb());
    for (unsigned i = 0; i < sizeof probes / sizeof probes[0]; i++) {
        if (only && probes[i].cp != only) continue;
        long before = fc_rss_kb();
        int id = cinder_font_probe(probes[i].cp);
        long after = fc_rss_kb();
        std::fprintf(stderr, "[cinder-probe] fontchain: U+%04X %-62s -> font id %3d   "
                     "VmRSS %ld -> %ld kB  (%+ld)\n",
                     probes[i].cp, probes[i].what, id, before, after, after - before);
    }
    std::fprintf(stderr, "[cinder-probe] fontchain: VmRSS at end = %ld kB  "
                 "(font id 255 = nothing covered it and the WHOLE chain was loaded)\n", fc_rss_kb());
    return 0;
}

static int btinfo_probe() {
    install_diagnostics();

    // Framework + pump FIRST, and wait for the looper to actually turn — same invariant as every
    // other mode here. A pst client call made with a dead looper returns uninitialised stack, so a
    // "plausible" answer from an unpumped process is worth nothing.
    clog_("btinfo: Framework::GetReference() + StartForApplication …");
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    int sr = fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] btinfo: StartForApplication returned %d\n", sr);
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);
    std::fprintf(stderr, "[cinder-probe] btinfo: pump running (%u ticks)\n", g_pump_ticks);

    // dlopen(libasound) — the same call cinder-home's LDAC bridge makes. Checked here because
    // cinder-home resolves it LAZILY (deliberately: a DT_NEEDED on the Home app would turn a missing
    // library into a device that boots to nothing), so nothing else proves it can be found.
    {
        const char* cands[] = { "libasound.so.2", "libasound.so", "/lib/libasound.so.2",
                                "/lib/libasound.so", "/system/lib/libasound.so" };
        bool got = false;
        for (const char* c : cands) {
            void* h = dlopen(c, RTLD_NOW | RTLD_LOCAL);
            if (h) {
                std::fprintf(stderr, "[cinder-probe] btinfo: dlopen('%s') OK, snd_pcm_open=%p\n",
                             c, dlsym(h, "snd_pcm_open"));
                got = true;
                break;
            }
        }
        if (!got) clog_("btinfo: dlopen(libasound) FAILED on every candidate — the LDAC bridge "
                        "would report itself unavailable");
    }

    void* xmit = _ZN3pst8services33BtTransmitterServiceClientFactory14CreateInstanceEv();
    void* cmn  = _ZN3pst8services28BtCommonServiceClientFactory14CreateInstanceEv();
    std::fprintf(stderr, "[cinder-probe] btinfo: xmit=%p cmn=%p\n", xmit, cmn);

    // (1) GetConnectInformation — the two-out-param call.
    if (xmit) {
        enum { VIDX_GetConnectInformation = 5 };
        typedef int (*fn2)(void*, std::vector<unsigned char>*, std::string*);
        std::vector<unsigned char> addr;
        std::string name;
        int rc = -1;
        wd_arm(12);
        try {
            rc = ((fn2)vslot(xmit, VIDX_GetConnectInformation))(xmit, &addr, &name);
        } catch (...) { clog_("btinfo: GetConnectInformation threw"); }
        wd_disarm();
        char mac[64] = {0};
        for (size_t i = 0; i < addr.size() && i * 3 + 3 < sizeof mac; i++)
            std::snprintf(mac + i * 3, 4, "%02X:", addr[i]);
        size_t l = std::strlen(mac);
        if (l) mac[l - 1] = '\0';
        std::fprintf(stderr, "[cinder-probe] btinfo: GetConnectInformation rc=%d addr=[%s] (%zu bytes) name='%s'\n",
                     rc, mac, addr.size(), name.c_str());
        if (addr.size() == 6)
            clog_("btinfo: *** CONFIRMED — 6-byte MAC via vector<uint8_t>, so the two-out-param "
                  "signature and the libc++ container ABI are both right ***");
    }

    // (2) GetPairedDeviceInfo — the call the pairing screen needs. The ELEMENT type
    // (BtPairedDeviceInformation) is still unrecovered, so this deliberately passes a
    // vector<unsigned char> and reports raw sizes: the service resizes through the vector's own
    // three-pointer header, which is element-type independent, so begin/end tell us the total byte
    // count even though we cannot yet name the fields. That is the next thing to decode, and doing it
    // from real bytes beats guessing.
    if (cmn) {
        enum { VIDX_GetPairedDeviceInfo = 20 };
        typedef int (*fn1)(void*, std::vector<unsigned char>*);
        std::vector<unsigned char> raw;
        int rc = -1;
        wd_arm(12);
        try {
            rc = ((fn1)vslot(cmn, VIDX_GetPairedDeviceInfo))(cmn, &raw);
        } catch (...) { clog_("btinfo: GetPairedDeviceInfo threw"); }
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] btinfo: GetPairedDeviceInfo rc=%d bytes=%zu (=%zu "
                     "devices at the 48-byte stride)\n", rc, raw.size(), raw.size() / 48);
        // Hex the first 128 bytes so the element stride is readable by eye (repeating string
        // headers / MACs stand out).
        char line[3 * 16 + 1];
        for (size_t off = 0; off < raw.size() && off < 128; off += 16) {
            line[0] = 0;
            for (size_t i = 0; i < 16 && off + i < raw.size(); i++)
                std::snprintf(line + i * 3, 4, "%02x ", raw[off + i]);
            std::fprintf(stderr, "[cinder-probe] btinfo:   +%03zu  %s\n", off, line);
        }
    }

    // (2b) The same call again, but TYPED — the payoff. If the layout below is right this prints real
    // MACs and names; if it is wrong it faults here in a throwaway process instead of in the Home app.
    //
    // Recovered from the raw dump above (2 devices, 96 bytes, so a 48-byte stride):
    //   +0   vector<uint8_t> addr    end-begin == 6  -> the MAC
    //   +12  uint32                  0x00240404      -> Bluetooth Class of Device (A/V headset)
    //   +16  vector<uint8_t>         16 bytes        -> link key / UUID (not needed by the UI)
    //   +28  std::string name        first record was SSO ("\x14" = 10 chars, "WH-1000XM4"); the
    //                                second was a LONG string (cap 0x11 has libc++'s long bit set,
    //                                size 0x0e) — both forms present, which is the strongest single
    //                                confirmation that this really is libc++ std::string
    //   +40  two bytes of flags, then padding to 48
    if (cmn) {
        enum { VIDX_GetPairedDeviceInfo = 20 };
        typedef int (*fnv)(void*, std::vector<BtPairedDeviceInformation>*);
        std::vector<BtPairedDeviceInformation> devs;
        int rc = -1;
        wd_arm(12);
        try {
            rc = ((fnv)vslot(cmn, VIDX_GetPairedDeviceInfo))(cmn, &devs);
        } catch (...) { clog_("btinfo: typed GetPairedDeviceInfo threw"); }
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] btinfo: typed rc=%d count=%zu\n", rc, devs.size());
        for (size_t i = 0; i < devs.size(); i++) {
            const BtPairedDeviceInformation& d = devs[i];
            char mac[32] = {0};
            for (size_t b = 0; b < d.addr.size() && b * 3 + 3 < sizeof mac; b++)
                std::snprintf(mac + b * 3, 4, "%02X:", d.addr[b]);
            size_t l = std::strlen(mac);
            if (l) mac[l - 1] = '\0';
            std::fprintf(stderr, "[cinder-probe] btinfo:   [%zu] %s  '%s'  cod=0x%06x key=%zuB "
                         "flags=%u,%u\n", i, mac, d.name.c_str(), d.cod, d.key.size(), d.f0, d.f1);
        }
        if (!devs.empty() && devs[0].addr.size() == 6 && !devs[0].name.empty())
            clog_("btinfo: *** CONFIRMED — paired-device list decodes: 6-byte MAC + non-empty name. "
                  "pst::base::vector/string ARE libc++, and the pairing UI is unblocked ***");
    }

    // (3) LDAC quality enum — walk the values and let the service name them in logcat.
    if (xmit) {
        enum { VIDX_SetLdacSoundQuality = 18 };
        typedef int (*fne)(void*, const unsigned*);
        for (unsigned q = 0; q < 4; q++) {
            std::fprintf(stderr, "[cinder-probe] btinfo: SetLdacSoundQuality(%u) — expect "
                                 "'ldac quality:' in logcat\n", q);
            wd_arm(10);
            try { ((fne)vslot(xmit, VIDX_SetLdacSoundQuality))(xmit, &q); }
            catch (...) { clog_("btinfo: SetLdacSoundQuality threw"); }
            wd_disarm();
            usleep(300000);
        }
        // Put it back to Auto. Walking to 3 and leaving it there would quietly pin the radio at
        // 330 kbps for whoever uses the device next — a probe should not change what it measures.
        unsigned back = 0;
        wd_arm(10);
        try { ((fne)vslot(xmit, VIDX_SetLdacSoundQuality))(xmit, &back); }
        catch (...) {}
        wd_disarm();
        clog_("btinfo: LDAC quality restored to 0 (Auto)");
    }

    g_pump_run = false;
    std::fprintf(stderr, "[cinder-probe] btinfo: done (%u pump ticks)\n", g_pump_ticks);
    std::fflush(nullptr);
    _exit(0);
}

// ── --nfc : tap-to-pair. Does the NFC listener fire, and what does the OOB tag carry? ────────────
//
// Recovered 2026-07-30 the same way as the BT listener (analysis/G_bt_nfc/RE_findings.md round f):
//   * `NfcServiceClient` vtable: 3/4 `Open` (two overloads), 5/6 `Start`, 7 `Stop`, 8 `Close`,
//     9 `GetCurrentMode`, then **10 `AddListener`, 11 `RemoveListener`** — registration sits right
//     after the last service method, exactly as it does on BtCommonServiceClient.
//   * `NfcServiceListener` vtable: 2 `OnBluetoothOob`, 3 `OnUnknownTag`, 4 `OnHostCardEmulation`.
//   * `OnBluetoothOob` takes ONE argument: a pointer to a struct whose first field is the peer's BD
//     address as a `vector<uint8_t>` (+0), followed by a uint32 (+0xC) and strings. Only the prefix is
//     read here — declaring a tail that has not been verified is how you get a crash for no benefit,
//     and the address is the only field tap-to-pair needs.
//
// What this mode CANNOT settle by reading: which `Open`/`Start` overload to call. The alloc histogram
// says `Open` slot 4 and `Stop`/`Close`/`GetCurrentMode` marshal no arguments, while `Start` slot 5
// marshals one and slot 6 marshals two. So it tries the no-argument `Open` first and passes 0 for
// `Start`'s single argument (0 being the likeliest "default mode"), and reports every return code.
// A tap is expensive to arrange — somebody has to physically hold a device against the rear panel
// while this runs — so ONE tap has to answer everything the round-f notes left open, not just
// "did the callback fire". The struct is only known as far as its prefix:
//
//     { vector<uint8_t> addr;  /* +0x00 */  ... uint32 at +0x0C ... strings from +0x10 }
//
// so this dumps the raw bytes as well as the decoded prefix. Reading a fixed 64 bytes off an object
// of unknown size is exactly the kind of thing that belongs in a probe and nowhere near the Home
// app: the fault handler in install_diagnostics() catches it, and the cost of being wrong is this
// process rather than the launcher.
static void nfc_dump_payload(const void* p, const char* what) {
    if (!p) { clog_("nfc: null payload"); return; }
    const unsigned char* b = (const unsigned char*)p;
    for (int row = 0; row < 4; row++) {
        char line[128];
        int n = std::snprintf(line, sizeof line, "[cinder-probe] nfc: %s +%02x:", what, row * 16);
        for (int i = 0; i < 16; i++)
            n += std::snprintf(line + n, sizeof line - n, " %02x", b[row * 16 + i]);
        std::fprintf(stderr, "%s\n", line);
    }
    unsigned w = 0;
    std::memcpy(&w, b + 0x0c, 4);
    std::fprintf(stderr, "[cinder-probe] nfc: %s +0c = %u (0x%06x) — expected class-of-device\n",
                 what, w, w & 0xffffff);
    // Any libc++ std::string in the tail: SSO keeps (len<<1) in byte 0 and the text inline from
    // byte 1; the long form keeps size at +4 and a heap pointer at +8. Both are checked for sanity
    // before printing, because the whole point is to find out whether these ARE strings.
    for (unsigned off : { 0x10u, 0x1cu, 0x28u }) {
        const unsigned char* s = b + off;
        char out[64] = {0};
        if (!(s[0] & 1)) {
            unsigned len = s[0] >> 1;
            if (len > 0 && len <= 22) { std::memcpy(out, s + 1, len); out[len] = 0; }
        } else {
            unsigned len = 0; const char* ptr = nullptr;
            std::memcpy(&len, s + 4, 4);
            std::memcpy(&ptr, s + 8, 4);
            if (ptr && len > 0 && len < 60) { std::memcpy(out, ptr, len); out[len] = 0; }
        }
        bool printable = out[0] != 0;
        for (const char* q = out; *q && printable; q++) printable = (*q >= 0x20 && *q < 0x7f);
        if (printable)
            std::fprintf(stderr, "[cinder-probe] nfc: %s +%02x looks like a string: '%s'\n",
                         what, off, out);
    }
}

struct ProbeNfcListener {
    virtual ~ProbeNfcListener() {}                                  // slots 0, 1
    virtual void OnBluetoothOob(const void* oob) {                  // slot 2
        taps++;
        if (!oob) { clog_("nfc: OnBluetoothOob with a null payload"); return; }
        // +0 is a libc++ vector<uint8_t>; reading its three pointers is safe whatever follows.
        const std::vector<unsigned char>* addr =
            reinterpret_cast<const std::vector<unsigned char>*>(oob);
        char mac[24];
        mac_str(*addr, mac, sizeof mac);
        std::fprintf(stderr, "[cinder-probe] nfc: *** BLUETOOTH OOB TAG — addr=%s (%zu bytes) ***\n",
                     mac, addr->size());
        nfc_dump_payload(oob, "oob");
        // Kept for --nfctap, which measures tap -> link. The callback runs on the framework looper,
        // so this is written there and read by the main thread; a word-sized flag published last is
        // enough ordering for a probe (cinder-home does the same thing under a mutex).
        try { last_addr = *addr; } catch (...) { last_addr.clear(); }
        t_tap = bl_now();
        tapped = 1;
    }
    virtual void OnUnknownTag(const void* tag) {                    // slot 3
        taps++;
        // Worth as much as the OOB case: headphones that present a non-OOB record land here, and
        // knowing THAT is what says whether tap-to-pair needs a different record parser.
        clog_("nfc: OnUnknownTag — a tag was read but it is not a Bluetooth OOB record");
        nfc_dump_payload(tag, "tag");
    }
    virtual void OnHostCardEmulation(const void*) {                 // slot 4
        taps++;
        clog_("nfc: OnHostCardEmulation");
    }
    int taps = 0;
    std::vector<unsigned char> last_addr;
    double t_tap = 0;
    volatile sig_atomic_t tapped = 0;
};

static int nfc_probe(int secs, unsigned want_mode) {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    void* nfc = _ZN3pst8services23NfcServiceClientFactory14CreateInstanceEv();
    std::fprintf(stderr, "[cinder-probe] nfc: client=%p\n", nfc);
    if (!nfc) { clog_("nfc: NfcServiceClientFactory returned NULL"); _exit(1); }

    enum { VIDX_Open1 = 3, VIDX_Open2 = 4, VIDX_Start1 = 5, VIDX_Start2 = 6,
           VIDX_Stop = 7, VIDX_Close = 8, VIDX_GetCurrentMode = 9,
           VIDX_AddListener = 10, VIDX_RemoveListener = 11 };
    typedef int (*fn0)(void*);
    typedef int (*fnu)(void*, const unsigned*);
    typedef int (*fnadd)(void*, void*, const std::string*);
    typedef int (*fnrem)(void*, unsigned);

    static ProbeNfcListener listener;   // static: the proxy keeps a RAW pointer
    std::string key("");
    int reg = -1;
    wd_arm(12);
    try { reg = ((fnadd)vslot(nfc, VIDX_AddListener))(nfc, (void*)&listener, &key); }
    catch (...) { clog_("nfc: AddListener THREW"); }
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] nfc: AddListener -> %d (0 = registered, as on BtCommon)\n", reg);

    int mode0 = -1;
    wd_arm(10);
    try { mode0 = ((fn0)vslot(nfc, VIDX_GetCurrentMode))(nfc); } catch (...) {}
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] nfc: GetCurrentMode (before) = %d\n", mode0);

    int rc_open = -1;
    wd_arm(12);
    try { rc_open = ((fn0)vslot(nfc, VIDX_Open2))(nfc); }
    catch (...) { clog_("nfc: Open (slot 4) threw"); }
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] nfc: Open() slot4 rc=%d\n", rc_open);

    // MODE 0 IS NOT A MODE, and that is why every previous run saw zero taps.
    // `NfcService::Start` (libNfcService.so @0x7a40) reads:
    //
    //     if (state == 3) return 3;                 // already started
    //     ret = 1;                                  // <-- the DEFAULT is failure
    //     if (mode==1) nf=1; else if (mode==2) nf=2; else if (mode==3) nf=0; else goto out;
    //     puts("calling NF_start2()..."); NF_start2(.., nf); state = 2; ret = 0;
    //
    // so the only valid arguments are 1, 2 and 3, the return is 0 for success, and the `rc=1` that
    // the 2026-07-30 round recorded (and read as ambiguous) was a REJECTED call — the reader was
    // never started at all. The NFC controller coming up in logcat that day was `Open`'s
    // NF_initialize, not this.
    //
    // Which of the three is tag-reading is not settled, so sweep: take the first mode that both
    // returns 0 and leaves a nonzero GetCurrentMode. An explicit mode argument overrides.
    int rc_start = -1;
    unsigned used_mode = 0;
    for (unsigned mode = (want_mode ? want_mode : 1);
         mode <= (want_mode ? want_mode : 3); mode++) {
        int rc = -1;
        wd_arm(12);
        try { rc = ((fnu)vslot(nfc, VIDX_Start1))(nfc, &mode); }
        catch (...) { clog_("nfc: Start (slot 5) threw"); }
        wd_disarm();
        int md = -1;
        wd_arm(10);
        try { md = ((fn0)vslot(nfc, VIDX_GetCurrentMode))(nfc); } catch (...) {}
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] nfc: Start(%u) rc=%d%s -> GetCurrentMode=%d\n",
                     mode, rc,
                     rc == 0 ? " (ok)" : rc == 1 ? " (REJECTED — bad mode)"
                                                 : rc == 3 ? " (already started)" : "",
                     md);
        if (rc == 0 || rc == 3) { rc_start = rc; used_mode = mode; break; }
        // A rejected Start changed nothing, so the next mode can be tried on the same handle.
    }
    if (rc_start != 0 && rc_start != 3) {
        clog_("nfc: no Start mode was accepted — nothing below can fire. Check that NFC is enabled "
              "in Sony's settings.");
    } else {
        std::fprintf(stderr, "[cinder-probe] nfc: started in mode %u\n", used_mode);
    }

    std::fprintf(stderr, "[cinder-probe] nfc: TAP A DEVICE ON THE REAR PANEL NOW (%d s) …\n", secs);
    for (int i = 0; i < secs; i++) {
        usleep(1000000);
        if (i % 5 == 4) std::fprintf(stderr, "[cinder-probe] nfc:   t+%ds taps=%d\n", i + 1, listener.taps);
    }

    if (listener.taps == 0)
        clog_("nfc: no tag callbacks. Either Open/Start needs the other overload, the radio is off in "
              "settings, or nothing was tapped — check GetCurrentMode above before blaming the ABI");
    else
        clog_("nfc: *** the NFC listener FIRES — tap-to-pair is reachable ***");

    wd_arm(12);
    try { ((fn0)vslot(nfc, VIDX_Stop))(nfc); ((fn0)vslot(nfc, VIDX_Close))(nfc); } catch (...) {}
    wd_disarm();
    wd_arm(12);
    try { ((fnrem)vslot(nfc, VIDX_RemoveListener))(nfc, (unsigned)(uintptr_t)&listener); } catch (...) {}
    wd_disarm();
    clog_("nfc: stopped + closed + unregistered");

    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

// ── --btwho : read-only snapshot of who is connected right now ───────────────────────────────────
//
// Added 2026-07-30 to diagnose "headphones are playing but the Bluetooth screen says nothing is
// connected". Deliberately makes NO setter call of any kind: the user may be listening while this
// runs, and --btinfo's LDAC-quality walk would be audible. Four reads, then out.
//
// The point is to separate two very different causes: `GetConnectInformation` returning false (the
// shell asked correctly and the service said no) versus the shell asking at the wrong moment.
// ── --btscan : the listener ABI, exercised end to end ────────────────────────────────────────────
//
// Recovered 2026-07-30 (analysis/G_bt_nfc/RE_findings.md round b): Cinder does NOT have to implement
// `IBinderObject`. `BtCommonServiceClient::AddListener` (vtable slot 30) allocates the binder proxy
// itself and stores a RAW pointer to our object at proxy+0x24; dispatch is then a plain vtable call.
// So a C++ class with the virtuals in the right order is the whole listener.
//
// Two things this mode is here to settle, both of which would be expensive to discover inside the
// Home app:
//   1. Is the slot order right? Every method logs its own name, so a one-off error shows up as the
//      WRONG name printing rather than as a crash.
//   2. What is `AddListener`'s name argument? The notify side is
//      `NotifyListeners(id, param, bool, const string&, bool(*filter)(const string&, const string&))`,
//      so that string is a FILTER KEY — register the wrong one and the listener never fires while
//      looking perfectly healthy. This tries "" first and a service-name candidate second.
//
// The vtable layout relies on the Itanium ABI: with a virtual destructor declared FIRST, the address
// point is [D1, D0, then the virtuals in declaration order] — which puts OnNotifyBtStatus at 2 and
// OnNotifySearchedDevice at 6, matching the disassembly.
struct ProbeBtListener {
    virtual ~ProbeBtListener() {}                                            // slots 0, 1
    // Only SearchedDevice's signature is recovered; the rest take three word-sized args and are
    // never dereferenced, which is ABI-safe on armhf (surplus registers are simply ignored) and keeps
    // a mis-slotted call from turning into a wild pointer read.
    virtual void OnNotifyBtStatus(const void*, const void*, const void*)            { hit("BtStatus"); }
    virtual void OnNotifyNumericComparison(const void*, const void*, const void*)   { hit("NumericComparison"); }
    virtual void OnNotifyPairingComplete(const void*, const void*, const void*)     { hit("PairingComplete"); }
    virtual void OnNotifyPasskey(const void*, const void*, const void*)             { hit("Passkey"); }
    virtual void OnNotifySearchedDevice(const std::vector<unsigned char>& addr,
                                        const unsigned& cod,
                                        const std::string& name) {
        char mac[24];
        mac_str(addr, mac, sizeof mac);
        std::fprintf(stderr, "[cinder-probe] btscan: *** FOUND %s  '%s'  cod=%#x (%zu addr bytes) ***\n",
                     mac, name.c_str(), cod, addr.size());
        found++;
    }
    virtual void OnNotifyDisconnectEnd(const void*, const void*, const void*)       { hit("DisconnectEnd"); }
    virtual void OnNotifyCoexistenceBtWifiRatio(const void*, const void*, const void*) { hit("CoexistenceBtWifiRatio"); }
    virtual void OnNotifyUpdateSupportProfile(const void*, const void*, const void*){ hit("UpdateSupportProfile"); }
    virtual void OnNotifyUpdateOSInfo(const void*, const void*, const void*)        { hit("UpdateOSInfo"); }
    virtual void OnNotifyRssi(const void*, const void*, const void*)                { hit("Rssi"); }
    virtual void OnNotifyStartSwitchDevice(const void*, const void*, const void*)   { hit("StartSwitchDevice"); }
    virtual void OnNotifyAclStateChanged(const void*, const void*, const void*)     { hit("AclStateChanged"); }
    virtual void OnNotifySspRequest(const void*, const void*, const void*)          { hit("SspRequest"); }
    virtual void OnNotifyServiceUuids(const void*, const void*, const void*)        { hit("ServiceUuids"); }
    virtual void OnNotifyServiceResume(const void*, const void*, const void*)       { hit("ServiceResume"); }
    virtual void OnNotifyError(const void*, const void*, const void*)               { hit("Error"); }

    int found = 0, calls = 0;
    void hit(const char* what) {
        calls++;
        std::fprintf(stderr, "[cinder-probe] btscan: callback %s\n", what);
    }
};

// ── --btrx : Bluetooth RECEIVER mode (Walkman as an A2DP SINK) ───────────────────────────────
// Sony's "other" Bluetooth mode. The transmitter path (BtTransmitterService) sends audio OUT to
// headphones; BtPlayerService is the mirror image — a phone streams TO the Walkman, and the
// CXD3778GF DAC/amp drives whatever is in the 3.5 mm jack. That is a real use for this hardware:
// the amp is the expensive part and a phone has nowhere near it.
//
// Client vtable recovered 2026-08-10 from `_ZTVN3pst8services21BtPlayerServiceClientE` at 0x313dc
// (.data.rel.ro is relocated at load, so the file words are zero — the slot map comes from the
// R_ARM_ABS32 entries covering that range, not from the raw bytes). It confirms the same rule the
// NFC and BtCommon clients follow: AddListener/RemoveListener are the LAST two methods.
//
// This mode is deliberately read-mostly and REVERSIBLE: it enters connect-wait, watches, then
// stops sound, disconnects and leaves connect-wait again. It never touches the transmitter side,
// so headphones that are already playing are unaffected.
struct ProbeRxListener {
    virtual ~ProbeRxListener() {}                                        // slots 0, 1
    // Slot order mirrors the listener names exported by libBtPlayerService.so, in declaration
    // order. Unrecovered signatures take three word-sized args and are never dereferenced —
    // ABI-safe on armhf (surplus registers are ignored) and a mis-slot can't become a wild read.
    virtual void OnNotifyAvSnkConnectionStatus(const void* a, const void*, const void*) { hit("AvSnkConnectionStatus", a); }
    virtual void OnNotifyAvrcpConnectionStatus(const void* a, const void*, const void*) { hit("AvrcpConnectionStatus", a); }
    virtual void OnNotifyConnectInformation(const void*, const void*, const void*)      { hit("ConnectInformation", nullptr); }
    virtual void OnNotifyReceiveMedia(const void*, const void*, const void*)            { hit("ReceiveMedia", nullptr); }
    virtual void OnNotifyPlayStatus(const void* a, const void*, const void*)            { hit("PlayStatus", a); }
    virtual void OnNotifyTrackNumber(const void* a, const void*, const void*)           { hit("TrackNumber", a); }
    virtual void OnNotifyVolumeDown(const void*, const void*, const void*)              { hit("VolumeDown", nullptr); }
    virtual void OnNotifyVolumeUp(const void*, const void*, const void*)                { hit("VolumeUp", nullptr); }
    virtual void OnNotifyChangeVolume(const void* a, const void*, const void*)          { hit("ChangeVolume", a); }
    virtual void OnNotifyRemoteVersion(const void*, const void*, const void*)           { hit("RemoteVersion", nullptr); }
    virtual void OnNotifySoundStatus(const void*, const void*, const void*)             { hit("SoundStatus", nullptr); }
    virtual void OnNotifyBitrate(const void* a, const void*, const void*)               { hit("Bitrate", a); }
    virtual void OnNotifyError(const void* a, const void*, const void*)                 { hit("Error", a); }
    virtual void OnNotifyAudioSetting(const void*, const void*, const void*)            { hit("AudioSetting", nullptr); }
    virtual void OnNotifyReceiveMediaComplete(const void*, const void*, const void*)    { hit("ReceiveMediaComplete", nullptr); }
    virtual void OnNotifyAudioState(const void* a, const void*, const void*)            { hit("AudioState", a); }
    virtual void OnNotifyRegisterForAbsVolume(const void*, const void*, const void*)    { hit("RegisterForAbsVolume", nullptr); }

    int calls = 0;
    // `a` is printed as a WORD, not dereferenced: for the callbacks whose first parameter really is
    // a `const uint32&` this is the value; for the rest it is a pointer we make no claim about.
    void hit(const char* what, const void* a) {
        calls++;
        if (a) std::fprintf(stderr, "[cinder-probe] btrx: <- On%s  arg0(as word)=%u\n", what, *(const unsigned*)a);
        else   std::fprintf(stderr, "[cinder-probe] btrx: <- On%s\n", what);
    }
};

extern "C" void* _ZN3pst8services28BtPlayerServiceClientFactory14CreateInstanceEv(void);

// ── --nfctap : how long does tap-to-pair actually take, end to end? ────────────────────────────
//
// "Make the tap work and make it faster" needs numbers before it needs code, and cinder-home's own
// path has none — its log has no timestamps. This arms the reader exactly as the Home app does
// (Open, Start(1), listener) and then times the three stages a user experiences as one:
//
//   arm -> OnBluetoothOob        the NFC read itself (hardware + Sony's stack)
//   callback -> RequestConnection  what the app adds
//   RequestConnection -> link      the radio
//
// Safe to run while cinder-home is up: NfcService accepts more than one listener, and a Start on an
// already-started reader returns 3 ("already") rather than disturbing it.
static int nfctap_probe(int secs) {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    void* nfc = _ZN3pst8services23NfcServiceClientFactory14CreateInstanceEv();
    void* x   = _ZN3pst8services33BtTransmitterServiceClientFactory14CreateInstanceEv();
    void* cmn = _ZN3pst8services28BtCommonServiceClientFactory14CreateInstanceEv();
    if (!nfc || !x || !cmn) { clog_("nfctap: a client factory returned NULL"); _exit(1); }

    enum { N_Open = 4, N_Start = 5, N_Stop = 7, N_Close = 8, N_GetMode = 9,
           N_AddListener = 10, N_RemoveListener = 11 };
    typedef int (*fn0)(void*);
    typedef int (*fnu)(void*, const unsigned*);
    typedef int (*fnadd)(void*, void*, const std::string*);
    typedef int (*fnrem)(void*, unsigned);
    typedef int (*fna)(void*, const std::vector<unsigned char>*);
    typedef int (*fnpaired)(void*, std::vector<BtPairedDeviceInformation>*);

    const double t0 = bl_now();
    static ProbeNfcListener listener;   // static: the proxy keeps a RAW pointer
    std::string key("");
    wd_arm(12);
    try { ((fnadd)vslot(nfc, N_AddListener))(nfc, (void*)&listener, &key); }
    catch (...) { clog_("nfctap: AddListener threw"); }
    wd_disarm();
    int rc_open = -1, rc_start = -1, md = -1;
    unsigned mode = 1;
    wd_arm(12);
    try {
        rc_open  = ((fn0)vslot(nfc, N_Open))(nfc);
        rc_start = ((fnu)vslot(nfc, N_Start))(nfc, &mode);
        md       = ((fn0)vslot(nfc, N_GetMode))(nfc);
    } catch (...) { clog_("nfctap: bring-up threw"); }
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] nfctap: Open rc=%d  Start(1) rc=%d (0 ok / 3 already)  "
                 "mode=%d  armed in %.2f s\n", rc_open, rc_start, md, bl_now() - t0);

    std::vector<BtPairedDeviceInformation> paired;
    try { ((fnpaired)vslot(cmn, BL_GetPairedDeviceInfo))(cmn, &paired); } catch (...) {}

    std::fprintf(stderr, "[cinder-probe] nfctap: TAP THE HEADPHONES ON THE REAR PANEL NOW (%d s)\n", secs);
    const double t_arm = bl_now();
    const double deadline = t_arm + secs;
    while (bl_now() < deadline && !listener.tapped) usleep(50000);

    if (!listener.tapped) {
        std::fprintf(stderr, "[cinder-probe] nfctap: no tag in %d s — nothing to time\n", secs);
    } else {
        std::vector<unsigned char> addr = listener.last_addr;
        char mac[24] = "-";
        mac_str(addr, mac, sizeof mac);
        std::fprintf(stderr, "[cinder-probe] nfctap: TAP  %s  at t+%.2f s (%.2f s after the reader "
                     "was armed)\n", mac, listener.t_tap - t0, listener.t_tap - t_arm);
        bool bonded = false;
        for (size_t i = 0; i < paired.size(); i++) if (paired[i].addr == addr) bonded = true;
        std::fprintf(stderr, "[cinder-probe] nfctap: %s\n",
                     bonded ? "already bonded -> RequestConnection (the common case)"
                            : "NOT bonded -> a real pairing would be needed; only timing the read");
        if (bonded && addr.size() == 6) {
            const double t_req = bl_now();
            int rc = -1;
            wd_arm(12);
            try { rc = ((fna)vslot(x, BL_RequestConnection))(x, &addr); }
            catch (...) { clog_("nfctap: RequestConnection threw"); }
            wd_disarm();
            std::fprintf(stderr, "[cinder-probe] nfctap: RequestConnection rc=%d, %.0f ms after the "
                         "callback\n", rc, (t_req - listener.t_tap) * 1000.0);
            bl_wait_link(x, cmn, listener.t_tap, 30, "tap -> RequestConnection");
        }
    }

    wd_arm(12);
    try {
        // Leave the reader as we found it: if it was already running (rc 3) it belongs to
        // cinder-home, and stopping it would disarm the Home app's tap-to-pair for the rest of the
        // boot.
        if (rc_start == 0) { ((fn0)vslot(nfc, N_Stop))(nfc); ((fn0)vslot(nfc, N_Close))(nfc); }
        ((fnrem)vslot(nfc, N_RemoveListener))(nfc, (unsigned)(uintptr_t)&listener);
    } catch (...) {}
    wd_disarm();

    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

static int btrx_probe(int secs) {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    void* rx  = _ZN3pst8services28BtPlayerServiceClientFactory14CreateInstanceEv();
    void* cmn = _ZN3pst8services28BtCommonServiceClientFactory14CreateInstanceEv();
    if (!rx) { clog_("btrx: no BtPlayerServiceClient"); _exit(1); }
    std::fprintf(stderr, "[cinder-probe] btrx: client=%p\n", rx);

    enum { RX_GetAvSnkConnectionStatus = 5, RX_GetAvrcpConnectionStatus = 6,
           RX_RequestDisconnection = 11, RX_RequestStartConnectWait = 13,
           RX_RequestStopConnectWait = 14, RX_StartSound = 15, RX_StopSound = 16,
           RX_GetPlayStatus = 19, RX_GetTrackCodec = 26, RX_GetTrackFreq = 27,
           RX_GetTrackChannel = 28, RX_GetTrackScmst = 29, RX_GetBitrate = 30,
           RX_AddListener = 31, RX_RemoveListener = 32 };
    enum { VIDX_GetBtStatus = 3, VIDX_SetRfOnOff = 4, VIDX_SetDiscoverableMode = 6 };
    typedef int  (*fn0)(void*);
    typedef void (*fnb)(void*, const bool*);
    typedef int  (*fnadd)(void*, void*, const std::string*);
    typedef int  (*fnrem)(void*, unsigned);

    // Radio up if it is not already. Restored at the end only if WE powered it.
    int st = -1;
    try { if (cmn) st = ((fn0)vslot(cmn, VIDX_GetBtStatus))(cmn); } catch (...) {}
    bool we_powered = false;
    if (cmn && st != 2 && st != 3) {
        bool on = true;
        wd_arm(10);
        try { ((fnb)vslot(cmn, VIDX_SetRfOnOff))(cmn, &on); we_powered = true; } catch (...) {}
        wd_disarm();
        for (int i = 0; i < 20; i++) { usleep(200000);
            try { st = ((fn0)vslot(cmn, VIDX_GetBtStatus))(cmn); } catch (...) {}
            if (st == 2 || st == 3) break; }
    }
    std::fprintf(stderr, "[cinder-probe] btrx: radio status=%d%s\n", st, we_powered ? " (we powered it)" : "");

    static ProbeRxListener listener;   // STATIC: the proxy keeps a RAW pointer to it
    std::string key("");
    int add = -1;
    wd_arm(12);
    try { add = ((fnadd)vslot(rx, RX_AddListener))(rx, (void*)&listener, &key); }
    catch (...) { clog_("btrx: AddListener THREW"); }
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] btrx: AddListener -> %d (0 = registered, as on BtCommon)\n", add);

    int snk0 = -1, avrcp0 = -1;
    try { snk0   = ((fn0)vslot(rx, RX_GetAvSnkConnectionStatus))(rx); } catch (...) {}
    try { avrcp0 = ((fn0)vslot(rx, RX_GetAvrcpConnectionStatus))(rx); } catch (...) {}
    std::fprintf(stderr, "[cinder-probe] btrx: before: AvSnk=%d Avrcp=%d\n", snk0, avrcp0);

    // Become connectable AND discoverable, then wait. RequestStartConnectWait is the sink-side
    // "listen for a phone"; SetDiscoverableMode is what makes the Walkman show up in the phone's
    // Bluetooth list at all if it has never been paired as a sink.
    int rcw = -1;
    wd_arm(10);
    try { rcw = ((fn0)vslot(rx, RX_RequestStartConnectWait))(rx); } catch (...) { clog_("btrx: StartConnectWait threw"); }
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] btrx: RequestStartConnectWait() rc=%d\n", rcw);
    if (cmn) {
        bool disc = true;
        wd_arm(8);
        try { ((fnb)vslot(cmn, VIDX_SetDiscoverableMode))(cmn, &disc); } catch (...) {}
        wd_disarm();
        clog_("btrx: SetDiscoverableMode(true) — pair the Walkman from your PHONE now");
    }

    std::fprintf(stderr, "[cinder-probe] btrx: waiting %ds — connect from a phone and PLAY something\n", secs);
    for (int i = 0; i < secs * 4; i++) {
        usleep(250000);
        if (i % 8 != 7) continue;                       // report ~2 s apart
        int snk = -1, codec = -1, freq = -1, chan = -1, rate = -1, play = -1;
        try { snk   = ((fn0)vslot(rx, RX_GetAvSnkConnectionStatus))(rx); } catch (...) {}
        if (snk == snk0 && listener.calls == 0) continue;   // nothing has changed yet
        try { play  = ((fn0)vslot(rx, RX_GetPlayStatus))(rx);  } catch (...) {}
        try { codec = ((fn0)vslot(rx, RX_GetTrackCodec))(rx);  } catch (...) {}
        try { freq  = ((fn0)vslot(rx, RX_GetTrackFreq))(rx);   } catch (...) {}
        try { chan  = ((fn0)vslot(rx, RX_GetTrackChannel))(rx);} catch (...) {}
        try { rate  = ((fn0)vslot(rx, RX_GetBitrate))(rx);     } catch (...) {}
        // Raw values on purpose: the enumerators are NOT recovered, and this run is how the map
        // gets built. The service's own log prints codec/channel/frequency as 0x%02x.
        std::fprintf(stderr, "[cinder-probe] btrx: AvSnk=%d play=%d codec=0x%02x freq=0x%02x chan=0x%02x bitrate=%d\n",
                     snk, play, codec, freq, chan, rate);
    }

    // Put everything back. StopSound before StopConnectWait, and disconnect whatever attached, so
    // the device is not left silently discoverable or holding a sink link.
    wd_arm(12);
    try { ((fn0)vslot(rx, RX_StopSound))(rx); } catch (...) {}
    try { ((fn0)vslot(rx, RX_RequestDisconnection))(rx); } catch (...) {}
    try { ((fn0)vslot(rx, RX_RequestStopConnectWait))(rx); } catch (...) {}
    if (cmn) { bool off = false; try { ((fnb)vslot(cmn, VIDX_SetDiscoverableMode))(cmn, &off); } catch (...) {} }
    try { ((fnrem)vslot(rx, RX_RemoveListener))(rx, (unsigned)(uintptr_t)&listener); } catch (...) {}
    if (we_powered && cmn) { bool off = false; try { ((fnb)vslot(cmn, VIDX_SetRfOnOff))(cmn, &off); } catch (...) {} }
    wd_disarm();

    std::fprintf(stderr, "[cinder-probe] btrx: %d listener callback(s) total%s\n", listener.calls,
                 listener.calls ? "" : "  <== nothing arrived: either no phone connected, or the sink path is not live");
    clog_("btrx: restored (sound stopped, disconnected, connect-wait off, discoverable off)");
    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

static int btscan_probe(int secs) {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    void* cmn = _ZN3pst8services28BtCommonServiceClientFactory14CreateInstanceEv();
    if (!cmn) { clog_("btscan: no BtCommonServiceClient"); _exit(1); }

    enum { VIDX_GetBtStatus = 3, VIDX_SetRfOnOff = 4, VIDX_SetSearchMode = 14,
           VIDX_AddListener = 30, VIDX_RemoveListener = 31 };
    typedef int  (*fn0)(void*);
    typedef void (*fnb)(void*, const bool*);
    typedef int  (*fnadd)(void*, void*, const std::string*);
    typedef int  (*fnrem)(void*, unsigned);
    typedef int  (*fnsearch)(void*, const bool*, const unsigned short*);

    // Radio up, restored at the end if we powered it.
    int st = -1;
    try { st = ((fn0)vslot(cmn, VIDX_GetBtStatus))(cmn); } catch (...) {}
    bool we_powered = false;
    if (st != 2 && st != 3) {
        bool on = true;
        wd_arm(10);
        try { ((fnb)vslot(cmn, VIDX_SetRfOnOff))(cmn, &on); we_powered = true; } catch (...) {}
        wd_disarm();
        for (int i = 0; i < 20; i++) { usleep(200000);
            try { st = ((fn0)vslot(cmn, VIDX_GetBtStatus))(cmn); } catch (...) {}
            if (st == 2 || st == 3) break; }
    }
    std::fprintf(stderr, "[cinder-probe] btscan: radio status=%d\n", st);

    static ProbeBtListener listener;   // must outlive the registration — the proxy keeps a RAW pointer
    const char* keys[] = { "", "BtCommonService" };
    int total = 0;
    for (int k = 0; k < 2 && total == 0; k++) {
        std::string key(keys[k]);
        int id = -1;
        wd_arm(12);
        try { id = ((fnadd)vslot(cmn, VIDX_AddListener))(cmn, (void*)&listener, &key); }
        catch (...) { clog_("btscan: AddListener THREW"); }
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] btscan: AddListener(key='%s') -> %d  "
                     "(1 = bad arg, 4 = no service; 0 is either failure or an OK-status)\n", keys[k], id);
        if (id == 1 || id == 4) continue;   // documented failures; anything else, carry on and MEASURE
        // Whether 0 means "registered" is exactly what the disassembly could not settle, so ask the
        // radio for something that answers over a NOTIFICATION rather than a return value. `GetRssi()`
        // (slot 25) is the ideal probe: it returns bool and its actual answer arrives as
        // OnNotifyRssi, so one callback proves the whole path without needing a discoverable device
        // in the room — and unlike a scan it cannot disturb audio that is already playing.
        {
            enum { VIDX_GetRssi = 25 };
            int rr = -1;
            wd_arm(10);
            try { rr = ((fn0)vslot(cmn, VIDX_GetRssi))(cmn); } catch (...) { clog_("btscan: GetRssi threw"); }
            wd_disarm();
            std::fprintf(stderr, "[cinder-probe] btscan: GetRssi() rc=%d — expecting OnNotifyRssi\n", rr);
            for (int i = 0; i < 20 && listener.calls == 0; i++) usleep(100000);
            std::fprintf(stderr, "[cinder-probe] btscan: after GetRssi: %d callback(s)%s\n",
                         listener.calls,
                         listener.calls ? "  <== REGISTRATION WORKS" : "  (no callback yet)");
        }

        bool on = true;
        unsigned short dur = (unsigned short)secs;
        int rc = -1;
        wd_arm(12);
        try { rc = ((fnsearch)vslot(cmn, VIDX_SetSearchMode))(cmn, &on, &dur); }
        catch (...) { clog_("btscan: SetSearchMode THREW"); }
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] btscan: SetSearchMode(true, %u) rc=%d — put a device in "
                     "pairing mode NOW\n", dur, rc);

        for (int i = 0; i < secs; i++) {
            usleep(1000000);
            if (i % 5 == 4) std::fprintf(stderr, "[cinder-probe] btscan:   t+%ds callbacks=%d found=%d\n",
                                         i + 1, listener.calls, listener.found);
        }
        bool off = false;
        wd_arm(12);
        try { ((fnsearch)vslot(cmn, VIDX_SetSearchMode))(cmn, &off, &dur); } catch (...) {}
        wd_disarm();
        total = listener.calls + listener.found;
        std::fprintf(stderr, "[cinder-probe] btscan: key='%s' -> %d callbacks, %d devices\n",
                     keys[k], listener.calls, listener.found);

        // UNREGISTRATION, and it matters more than it looks: the proxy holds a RAW pointer to our
        // object, so a listener that outlives its registration is a use-after-free in the Home app
        // the moment a notification arrives.
        //
        // AddListener hands back no id (0 = OK), yet the client's RemoveListener takes an `unsigned`
        // and rejects 0 with rc 1 — so that argument can only be the LISTENER POINTER itself. Test it
        // instead of assuming: remove, then toggle the scan again (which reliably produced an
        // OnNotifyBtStatus above) and require the callback count to stop moving.
        {
            unsigned handle = (unsigned)(uintptr_t)&listener;
            int rrc = -1;
            wd_arm(12);
            try { rrc = ((fnrem)vslot(cmn, VIDX_RemoveListener))(cmn, handle); }
            catch (...) { clog_("btscan: RemoveListener threw"); }
            wd_disarm();
            std::fprintf(stderr, "[cinder-probe] btscan: RemoveListener(%#x /* &listener */) rc=%d\n",
                         handle, rrc);
            int before = listener.calls + listener.found;
            bool on2 = true, off2 = false;
            unsigned short d2 = 2;
            wd_arm(12);
            try {
                ((fnsearch)vslot(cmn, VIDX_SetSearchMode))(cmn, &on2, &d2);
                usleep(1500000);
                ((fnsearch)vslot(cmn, VIDX_SetSearchMode))(cmn, &off2, &d2);
            } catch (...) {}
            wd_disarm();
            usleep(500000);
            int after = listener.calls + listener.found;
            std::fprintf(stderr, "[cinder-probe] btscan: post-remove callbacks %d -> %d\n", before, after);

            // NEGATIVE CONTROL. Silence after a remove proves nothing unless the same stimulus DOES
            // produce a callback while registered — otherwise "no callbacks" might just mean the
            // toggle changed no state. Re-register and repeat the identical toggle.
            std::string k2("");
            int id2 = -1;
            wd_arm(12);
            try { id2 = ((fnadd)vslot(cmn, VIDX_AddListener))(cmn, (void*)&listener, &k2); } catch (...) {}
            wd_disarm();
            int mid = listener.calls + listener.found;
            wd_arm(12);
            try {
                ((fnsearch)vslot(cmn, VIDX_SetSearchMode))(cmn, &on2, &d2);
                usleep(1500000);
                ((fnsearch)vslot(cmn, VIDX_SetSearchMode))(cmn, &off2, &d2);
            } catch (...) {}
            wd_disarm();
            usleep(500000);
            int again = listener.calls + listener.found;
            std::fprintf(stderr, "[cinder-probe] btscan: re-registered (rc=%d) same toggle: %d -> %d\n",
                         id2, mid, again);
            if (again > mid && after == before)
                clog_("btscan: *** RemoveListener CONFIRMED — the unsigned IS the listener pointer: "
                      "identical stimulus fired while registered and was silent while removed ***");
            else if (after > before)
                clog_("btscan: RemoveListener did NOT unregister — that argument is not the pointer");
            else
                clog_("btscan: inconclusive — the toggle produced no callback even while registered, "
                      "so the earlier silence proves nothing. Needs a different stimulus.");
            wd_arm(12);
            try { ((fnrem)vslot(cmn, VIDX_RemoveListener))(cmn, handle); } catch (...) {}
            wd_disarm();
        }
    }

    if (total == 0)
        clog_("btscan: NO callbacks on either key. Either the filter key is something else again, or "
              "nothing was discoverable nearby — rerun with a phone in pairing mode before blaming "
              "the ABI");
    else
        clog_("btscan: *** the listener ABI WORKS — raw pointer + vtable slots, no IBinderObject ***");

    if (we_powered) {
        bool off = false;
        wd_arm(10);
        try { ((fnb)vslot(cmn, VIDX_SetRfOnOff))(cmn, &off); } catch (...) {}
        wd_disarm();
        clog_("btscan: radio powered back OFF (it was off when this started)");
    }
    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

// ── --btlink : the reconnect path, measured — and the two Sony calls Cinder never makes ─────────
//
// Task: "reconnecting to paired devices" and "tap-to-pair, faster". Both are already implemented in
// cinder-home, so the question is not whether the calls exist but WHAT THE RADIO DOES WITH THEM and
// how long it takes. This measures that without a rebuild or a reboot: every subcommand is one
// request plus a 250 ms status poll with timestamps.
//
// Two transmitter methods are in the recovered vtable and are called by NOTHING in this project
// (grep: only the sink-side BtPlayerService equivalents are used):
//
//   slot 10  RequestStartConnectWait()                       — accept an INCOMING connection.
//   slot 27  SetConnectRetryMode(const bool&, const u32&, const u32&) — the service's own retry
//            worker (`BtTransmitterService::ConnectRetryWorkThread(const uint32_t&, const uint32_t&)`
//            exists in the same binary, so the two u32s are that thread's interval and count).
//
// If connect-wait is what makes a headphone's own power-on reconnect land, that is the whole
// "reconnect is slow" complaint: Cinder's backoff starts at 10 s and doubles to 300 s, so a
// headphone switched on during the long part of that curve waits minutes for a link the radio could
// have accepted immediately.
//
// Everything here is reversible and does nothing a user could not do from the Bluetooth screen:
// connect, disconnect, and two mode flags that are restored unless `keep` is passed.
static int btlink_probe(const char* sub, const char* a1, int a2, int a3, bool keep) {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    void* x   = _ZN3pst8services33BtTransmitterServiceClientFactory14CreateInstanceEv();
    void* cmn = _ZN3pst8services28BtCommonServiceClientFactory14CreateInstanceEv();
    if (!x || !cmn) { clog_("btlink: no transmitter/common client"); _exit(1); }

    typedef int (*fn0)(void*);
    typedef void (*fnb)(void*, const bool*);
    typedef int (*fnretry)(void*, const bool*, const unsigned*, const unsigned*);
    typedef int (*fnpaired)(void*, std::vector<BtPairedDeviceInformation>*);

    const double t0 = bl_now();

    // The radio has to be up for any of this to mean anything, and 7 is OFF (not a wedge —
    // reference_bt_radio_wedge). Powering it up here is the same call the Settings switch makes.
    int st = -1;
    try { st = ((fn0)vslot(cmn, BL_GetBtStatus))(cmn); } catch (...) {}
    if (st != 2 && st != 3) {
        bool on = true;
        clog_("btlink: radio is not up — SetRfOnOff(true)");
        wd_arm(10);
        try { ((fnb)vslot(cmn, BL_SetRfOnOff))(cmn, &on); } catch (...) {}
        wd_disarm();
        for (int i = 0; i < 25; i++) {
            usleep(200000);
            try { st = ((fn0)vslot(cmn, BL_GetBtStatus))(cmn); } catch (...) {}
            if (st == 2 || st == 3) break;
        }
        std::fprintf(stderr, "[cinder-probe] btlink: radio status now %d after %.2f s\n",
                     st, bl_now() - t0);
    }

    // The paired table, because every addressed subcommand indexes into it.
    std::vector<BtPairedDeviceInformation> paired;
    try { ((fnpaired)vslot(cmn, BL_GetPairedDeviceInfo))(cmn, &paired); }
    catch (...) { clog_("btlink: GetPairedDeviceInfo threw"); }
    for (size_t i = 0; i < paired.size(); i++) {
        char mac[24] = "-";
        if (paired[i].addr.size() == 6) mac_str(paired[i].addr, mac, sizeof mac);
        std::fprintf(stderr, "[cinder-probe] btlink: paired[%zu] %s '%s'\n",
                     i, mac, paired[i].name.c_str());
    }

    // ── status: read-only. Also the only place GetConnectRetryMode has ever been called.
    if (!sub || std::strcmp(sub, "status") == 0) {
        int retry = -1;
        wd_arm(10);
        try { retry = ((fn0)vslot(x, BL_GetConnectRetryMode))(x); } catch (...) { clog_("btlink: GetConnectRetryMode threw"); }
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] btlink: GetConnectRetryMode() = %d\n", retry);
        BlState s;
        bl_read(x, cmn, s);
        bl_print(s, t0, "(status)");
        g_pump_run = false;
        std::fflush(nullptr);
        _exit(0);
    }

    // ── last: the zero-argument reconnect cinder-home's backoff timer uses. (An ADDRESSED connect
    //    is --btconnect <row>, which already exists and is not duplicated here.)
    if (std::strcmp(sub, "last") == 0) {
        int rc = -1;
        wd_arm(12);
        try { rc = ((fn0)vslot(x, BL_RequestLastDeviceConnection))(x); }
        catch (...) { clog_("btlink: RequestLastDeviceConnection threw"); }
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] btlink: RequestLastDeviceConnection() rc=%d\n", rc);
        bl_wait_link(x, cmn, t0, a2 > 0 ? a2 : 30, "RequestLastDeviceConnection");
    }
    // ── wait S: become connectable and let the HEADPHONES do the connecting. Switch them on
    //    during the window. If this lands, incoming reconnect is a radio feature Cinder is simply
    //    not enabling, and the whole backoff ladder is the wrong mechanism.
    else if (std::strcmp(sub, "wait") == 0) {
        int rc = -1;
        wd_arm(12);
        try { rc = ((fn0)vslot(x, BL_RequestStartConnectWait))(x); }
        catch (...) { clog_("btlink: RequestStartConnectWait threw"); }
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] btlink: RequestStartConnectWait() rc=%d — "
                     "SWITCH THE HEADPHONES ON NOW (%d s)\n", rc, a2 > 0 ? a2 : 45);
        bl_wait_link(x, cmn, t0, a2 > 0 ? a2 : 45, "connect-wait (incoming)");
        if (!keep) {
            wd_arm(12);
            try { ((fn0)vslot(x, BL_RequestStopConnectWait))(x); } catch (...) {}
            wd_disarm();
            clog_("btlink: RequestStopConnectWait() — connectable window closed again");
        } else {
            clog_("btlink: connect-wait LEFT ON (keep)");
        }
    }
    // ── retry on|off [interval_s] [count]: the service-side retry worker.
    else if (std::strcmp(sub, "retry") == 0) {
        bool on = a1 && (std::strcmp(a1, "on") == 0 || std::strcmp(a1, "1") == 0);
        unsigned iv = a2 > 0 ? (unsigned)a2 : 5, cnt = a3 > 0 ? (unsigned)a3 : 10;
        int before = -1, rc = -1, after = -1;
        wd_arm(10);
        try { before = ((fn0)vslot(x, BL_GetConnectRetryMode))(x); } catch (...) {}
        wd_disarm();
        wd_arm(12);
        try { rc = ((fnretry)vslot(x, BL_SetConnectRetryMode))(x, &on, &iv, &cnt); }
        catch (...) { clog_("btlink: SetConnectRetryMode threw"); }
        wd_disarm();
        wd_arm(10);
        try { after = ((fn0)vslot(x, BL_GetConnectRetryMode))(x); } catch (...) {}
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] btlink: SetConnectRetryMode(%s, %u, %u) rc=%d  "
                     "GetConnectRetryMode %d -> %d\n", on ? "true" : "false", iv, cnt, rc,
                     before, after);
        // With retry ON, a request that fails should be re-tried BY THE SERVICE. Watch for that.
        if (on && a2 >= 0) bl_wait_link(x, cmn, t0, 40, "retry-mode window");
    }
    // ── hci on|off: Sony's own HCI trace. mtkbt writes /tmp/hci_sniffer_log_<stamp>.cfa — the
    //    failure channel this stack otherwise does not have.
    else if (std::strcmp(sub, "hci") == 0) {
        bool on = a1 && (std::strcmp(a1, "on") == 0 || std::strcmp(a1, "1") == 0);
        int rc = -1;
        wd_arm(12);
        try { ((fnb)vslot(cmn, BL_SetHciLogEnabled))(cmn, &on); rc = 0; }
        catch (...) { clog_("btlink: SetHciLogEnabled threw"); }
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] btlink: SetHciLogEnabled(%s)%s — look for "
                     "/tmp/hci_sniffer_log_*.cfa\n", on ? "true" : "false",
                     rc == 0 ? "" : " THREW");
        DIR* d = opendir("/tmp");
        if (d) {
            struct dirent* de;
            while ((de = readdir(d))) {
                if (std::strncmp(de->d_name, "hci_sniffer_log", 15) != 0) continue;
                char p[256];
                std::snprintf(p, sizeof p, "/tmp/%s", de->d_name);
                struct stat sb;
                if (stat(p, &sb) == 0)
                    std::fprintf(stderr, "[cinder-probe] btlink:   %s  %ld bytes\n",
                                 p, (long)sb.st_size);
            }
            closedir(d);
        }
    }
    // ── rssi S: GetRssi (BtCommon slot 25) answers over OnNotifyRssi, so this needs the listener.
    //    Untested until now with a device actually CONNECTED — the 2026-07-30 round called it on an
    //    idle radio and read the silence as "wrong reply path".
    else if (std::strcmp(sub, "rssi") == 0) {
        static ProbeBtListener listener;
        typedef int (*fnadd)(void*, void*, const std::string*);
        std::string key("");
        int id = -1;
        wd_arm(12);
        try { id = ((fnadd)vslot(cmn, BL_CmnAddListener))(cmn, (void*)&listener, &key); }
        catch (...) { clog_("btlink: AddListener threw"); }
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] btlink: AddListener -> %d\n", id);
        const int secs = a2 > 0 ? a2 : 10;
        for (int i = 0; i < secs; i++) {
            int rr = -1;
            wd_arm(10);
            try { rr = ((fn0)vslot(cmn, BL_GetRssi))(cmn); } catch (...) {}
            wd_disarm();
            BlState s;
            bl_read(x, cmn, s);
            std::fprintf(stderr, "[cinder-probe] btlink: GetRssi rc=%d  (callbacks so far %d)  "
                         "avsrc=%d link='%s'\n", rr, listener.calls, s.avsrc, s.name.c_str());
            usleep(1000000);
        }
        typedef int (*fnrem)(void*, unsigned);
        wd_arm(12);
        try { ((fnrem)vslot(cmn, BL_CmnRemoveListener))(cmn, (unsigned)(uintptr_t)&listener); } catch (...) {}
        wd_disarm();
    }
    // ── drop: disconnect, so a reconnect test starts from a known state.
    else if (std::strcmp(sub, "drop") == 0) {
        int rc = -1;
        wd_arm(12);
        try { rc = ((fn0)vslot(x, BL_RequestDisconnection))(x); }
        catch (...) { clog_("btlink: RequestDisconnection threw"); }
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] btlink: RequestDisconnection() rc=%d\n", rc);
        BlState s;
        for (int i = 0; i < 12; i++) { usleep(500000); bl_read(x, cmn, s); if (!s.linked()) break; }
        bl_print(s, t0, s.linked() ? "(still linked)" : "(dropped)");
    }
    else {
        clog_("btlink: usage: --btlink status | conn <row> [secs] | last [secs] | wait [secs] [keep] "
              "| retry on|off [interval] [count] | hci on|off | rssi [secs] | drop");
    }

    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}


// ── --pollnodes : which /dev/input node makes poll() spin? ──────────────────────────────────────
// The render loop's dark sleep became poll() on every input node (2026-08-11) and cinder-home's
// standby cost went the WRONG way — 0.25% of a core to 1.90%, system context switches 354/s to
// 1337/s, with the render thread's time 23 sys : 5 user. That ratio is a syscall storm, i.e. poll()
// returning immediately every iteration. Either a node is streaming events, or one is reporting
// POLLERR/POLLHUP — which poll() delivers whether or not you asked for it, forever, on every call.
// This says which, per node, without a rebuild-and-reboot cycle.
// EVIOCGNAME(64) = _IOC(READ, 'E', 0x06, 64) — same constant main.cpp derives, repeated here so
// the probe stays standalone.
static const unsigned PROBE_EVIOCGNAME_64 =
    (2u << 30) | (64u << 16) | ((unsigned)'E' << 8) | 0x06;

static int pollnodes_probe(int secs) {
    struct { int fd; char name[64]; char path[32]; long ready; long bytes; int err; } n[16];
    int cnt = 0;
    DIR* d = opendir("/dev/input");
    if (!d) { clog_("pollnodes: /dev/input missing"); return 1; }
    struct dirent* de;
    while ((de = readdir(d)) && cnt < 16) {
        if (std::strncmp(de->d_name, "event", 5) != 0) continue;
        std::snprintf(n[cnt].path, sizeof n[cnt].path, "/dev/input/%s", de->d_name);
        int fd = open(n[cnt].path, O_RDONLY | O_NONBLOCK);
        if (fd < 0) continue;
        n[cnt].fd = fd; n[cnt].ready = 0; n[cnt].bytes = 0; n[cnt].err = 0;
        std::snprintf(n[cnt].name, sizeof n[cnt].name, "?");
        ioctl(fd, PROBE_EVIOCGNAME_64, n[cnt].name);
        cnt++;
    }
    closedir(d);

    // Poll each node ON ITS OWN with a 50 ms timeout, exactly as the render loop would, and drain
    // whatever it offers. A node that answers instantly every time and yields no bytes is the one.
    const long deadline = (long)secs * 1000;
    long spent = 0;
    unsigned char buf[4096];
    while (spent < deadline) {
        for (int i = 0; i < cnt; i++) {
            struct pollfd p; p.fd = n[i].fd; p.events = POLLIN; p.revents = 0;
            if (poll(&p, 1, 0) > 0) {           // 0 ms: "is it ready RIGHT NOW"
                n[i].ready++;
                n[i].err |= (p.revents & ~POLLIN);
                ssize_t r = read(n[i].fd, buf, sizeof buf);
                if (r > 0) n[i].bytes += r;
            }
        }
        usleep(50000);
        spent += 50;
    }
    std::fprintf(stderr, "[cinder-probe] pollnodes: %d nodes, %d s\n", cnt, secs);
    for (int i = 0; i < cnt; i++) {
        const char* verdict = "quiet (poll blocks — good)";
        if (n[i].ready && n[i].bytes == 0)  verdict = "*** READY WITH NO DATA — this is the spinner";
        else if (n[i].ready)                verdict = "streaming events";
        std::fprintf(stderr, "[cinder-probe] pollnodes: %-20s %-24s ready=%-5ld bytes=%-6ld revents_extra=0x%x  %s\n",
                     n[i].path, n[i].name, n[i].ready, n[i].bytes, n[i].err, verdict);
    }
    for (int i = 0; i < cnt; i++) close(n[i].fd);
    std::fflush(nullptr);
    return 0;
}

// ── --uaccap : IS THE HOST ACTUALLY STREAMING? ──────────────────────────────────────────────────
// The one question two rounds of testing could not answer. cinder-home's log says
// `GetStatus format=0, capture=cardN closed` for the whole session — but "closed" describes OUR
// side of the ALSA link, so it is silent about whether the PC is sending anything at all. The
// service reporting no format is equally ambiguous: no host audio, or a host that IS streaming and
// a Sony service that never noticed.
//
// So ask the kernel directly, bypassing Sony entirely: open the UAC gadget's capture PCM and read
// it. Data with a non-zero peak proves the host is streaming and that the capture is available to
// us — which, if the service is still reporting kFormatNone, means the render path can be built
// WITHOUT UsbDeviceAudioPlayerService (capture card N -> write hw:0,4), exactly the shape the LDAC
// bridge already has.
//
// Run it detached while in DAC mode and read the log afterwards — adb cannot survive the gadget
// switch on a usbipd passthrough:
//     /tmp/cinder-probe --uaccap 20 > /contents/uaccap.log 2>&1
static int uaccap_probe(int secs, int delay) {
    install_diagnostics();
    if (secs <= 0) secs = 15;

    // ARM IT AND LET GO. The measurement has to happen while the gadget is in UAC mode with the PC
    // streaming — and that is exactly the state in which adb cannot exist here: usbipd attaches the
    // WHOLE device to WSL, so a device attached for adb is a device Windows cannot use as a sound
    // card. The two are mutually exclusive over this passthrough.
    //
    // So the caller starts this from a normal adb shell, it returns immediately, and the child does
    // the work `delay` seconds later — after the user has toggled DAC on and started playback.
    // setsid + SIG_IGN first, for the same reason the --usbmgr restore child needs them: the shell
    // that launched us dies with the gadget switch, and a child in that session dies with it.
    if (delay > 0) {
        pid_t kid = fork();
        if (kid != 0) {
            std::fprintf(stderr, "[cinder-probe] uaccap: armed (pid %d) — measuring %ds of capture "
                                 "in %ds. Toggle USB-DAC on and start playback on the PC now; read "
                                 "/contents/uaccap.log afterwards.\n", (int)kid, secs, delay);
            std::fflush(nullptr);
            return 0;
        }
        signal(SIGHUP, SIG_IGN);
        signal(SIGINT, SIG_IGN);
        signal(SIGTERM, SIG_IGN);
        setsid();
        sleep((unsigned)delay);
    }

    char dev[32] = {0};
    snd_pcm_t* pcm = nullptr;
    int rc = -ENODEV;
    // card0 is the built-in codec; the UAC gadget lands on whatever index is free.
    for (int card = 1; card < 8; card++) {
        std::snprintf(dev, sizeof dev, "hw:%d,0", card);
        rc = snd_pcm_open(&pcm, dev, SND_PCM_STREAM_CAPTURE, 0);
        if (rc == 0) break;
        if (rc == -EBUSY) {
            std::fprintf(stderr, "[cinder-probe] uaccap: %s is BUSY — something already owns the "
                                 "capture (that would be good news: Sony's service took it)\n", dev);
            return 0;
        }
    }
    if (rc != 0) {
        std::fprintf(stderr, "[cinder-probe] uaccap: no UAC capture PCM could be opened (%s). "
                             "Is the gadget in UAC mode?\n", snd_strerror(rc));
        return 1;
    }
    std::fprintf(stderr, "[cinder-probe] uaccap: opened %s for capture\n", dev);

    // Let ALSA pick what the gadget supports rather than imposing a rate — the host decides the
    // format here, and forcing one is how you get a spurious EINVAL that reads like "no audio".
    unsigned rate = 48000;
    int dir = 0;
    rc = snd_pcm_set_params(pcm, SND_PCM_FORMAT_S16_LE, SND_PCM_ACCESS_RW_INTERLEAVED,
                            2, rate, 1, 200000);
    if (rc < 0) {
        std::fprintf(stderr, "[cinder-probe] uaccap: set_params(S16_LE 48k stereo) -> %s; "
                             "retrying S32_LE 44.1k\n", snd_strerror(rc));
        rate = 44100;
        rc = snd_pcm_set_params(pcm, SND_PCM_FORMAT_S32_LE, SND_PCM_ACCESS_RW_INTERLEAVED,
                                2, rate, 1, 200000);
        if (rc < 0) {
            std::fprintf(stderr, "[cinder-probe] uaccap: set_params failed: %s\n", snd_strerror(rc));
            snd_pcm_close(pcm); return 1;
        }
    }
    (void)dir;
    snd_pcm_prepare(pcm);   // readi auto-starts a prepared stream; snd_pcm_start isn't in the
                            // ALSA subset this probe declares, and isn't needed for RW_INTERLEAVED

    short buf[2048];
    long  frames_total = 0, reads_ok = 0, reads_err = 0;
    int   peak = 0;
    time_t end = time(nullptr) + secs;
    while (time(nullptr) < end) {
        wd_arm(10);
        snd_pcm_sframes_t got = snd_pcm_readi(pcm, buf, sizeof buf / (2 * sizeof(short)));
        wd_disarm();
        if (got < 0) {
            reads_err++;
            if (got == -EPIPE) { snd_pcm_prepare(pcm); continue; }
            if (reads_err > 200) break;
            usleep(5000);
            continue;
        }
        reads_ok++;
        frames_total += got;
        for (long i = 0; i < got * 2; i++) {
            int v = buf[i] < 0 ? -buf[i] : buf[i];
            if (v > peak) peak = v;
        }
    }
    snd_pcm_close(pcm);

    std::fprintf(stderr, "[cinder-probe] uaccap: %s  reads_ok=%ld reads_err=%ld frames=%ld "
                         "peak=%d/32767\n", dev, reads_ok, reads_err, frames_total, peak);
    if (frames_total > 0 && peak > 64) {
        clog_("uaccap: >>> THE HOST IS STREAMING AUDIO and the capture is ours to read. If "
              "cinder-home still logs kFormatNone, Sony's UsbDeviceAudioPlayerService is the "
              "broken link — bypass it: capture this PCM and write hw:0,4 directly.");
    } else if (frames_total > 0) {
        clog_("uaccap: frames arrived but they are SILENT (peak ~0). The host has the device "
              "selected but is playing nothing, or is muted.");
    } else {
        clog_("uaccap: NO frames. The host is not streaming to this gadget at all — check the PC's "
              "output device selection.");
    }
    std::fflush(nullptr);
    return 0;
}

// ── --usbmgr : WHO OWNS THE USB GADGET ──────────────────────────────────────────────────────────
// Reported 2026-08-11: mass storage misbehaves, and USB-DAC may never have produced audio. Both
// come from the same thing — the gadget has TWO owners that do not know about each other:
//
//   * init (/init.usbcfg.rc), on property:sys.sony.config={adb,uac,msc}. Writes functions,
//     idVendor, idProduct (hardcoded 0B8B/0B8C/0B8D), AND f_mass_storage/lun/file, and it is the
//     ONLY thing that runs unmount_msc1 (umount /contents) / mount_msc1 (mount_partition contents).
//   * UsbMgrServiceFw (UsbMgrImplWmport). SetUsbFunction -> UpdateUsbFunction -> SetUac()/SetMsc(),
//     which write idVendor/idProduct (from DmpFeature, hence the 0ca0 nobody could explain),
//     functions and MaxPower — and NEVER touch lun/file or the mount.
//
// So MSC ends up advertising a mass-storage device with no medium, and a UAC switch made only
// through the property gets reverted the next time UsbMgr reconfigures (cable insert, resume,
// OnDeviceConnectedChanged...). Full evidence: analysis/E_usbdac_ldac/RE_findings.md, 2026-08-11.
//
// READ-ONLY by default, and worth running exactly as-is first: it asks the service what function it
// believes is active and prints that next to what the gadget actually says. If those disagree, the
// diagnosis above is confirmed on this unit rather than inferred from the disassembly.
//
// `--usbmgr uac|msc` is the experiment. It bounces adbd (UpdateUsbFunction stops it around the
// switch), so it forks a restore child FIRST that puts the previous function back after the window
// no matter what happens to the parent — same rule as --dispoff, and the reason this is safe to run
// over adb at all.
extern "C" void* _ZN3pst8services28UsbMgrServiceFwClientFactory14CreateInstanceEv(void);

// Read from the dispatch switch in UsbMgrImplWmport::UpdateUsbFunction (cmp #2 -> SetMsc,
// cmp #1 -> SetUac), not guessed.
enum { USBFN_UAC = 1, USBFN_MSC = 2 };

// Whole vtable recovered from R_ARM_ABS32 relocations — every slot is named, nothing inferred.
enum { VIDX_SetUsbFunction = 3, VIDX_GetUsbFunction = 4,
       VIDX_GetCurrentPowerSuppliedMode = 9, VIDX_GetAdbEnabled = 11 };

static const char* usbfn_name(unsigned f) {
    return f == USBFN_UAC ? "UAC (USB-DAC)" : f == USBFN_MSC ? "MSC (mass storage)" : "??";
}

static void slurp_(const char* path, char* out, size_t n) {
    out[0] = 0;
    FILE* f = std::fopen(path, "r");
    if (!f) { std::snprintf(out, n, "<absent>"); return; }
    if (!std::fgets(out, (int)n, f)) std::snprintf(out, n, "<empty>");
    std::fclose(f);
    size_t l = std::strlen(out);
    while (l && (out[l-1] == '\n' || out[l-1] == '\r')) out[--l] = 0;
    if (!out[0]) std::snprintf(out, n, "<empty>");
}

static void getprop_(const char* key, char* out, size_t n) {
    char cmd[128];
    std::snprintf(cmd, sizeof cmd, "/system/bin/getprop %s", key);
    out[0] = 0;
    FILE* f = ::popen(cmd, "r");
    if (!f) { std::snprintf(out, n, "<popen failed>"); return; }
    if (!std::fgets(out, (int)n, f)) out[0] = 0;
    ::pclose(f);
    size_t l = std::strlen(out);
    while (l && (out[l-1] == '\n' || out[l-1] == '\r')) out[--l] = 0;
    if (!out[0]) std::snprintf(out, n, "<empty>");
}

static void usbmgr_dump_gadget(void) {
    char fn[128], vid[64], pid[64], lun[256], cfg[64], st[64], msc1[128], en[32];
    slurp_("/sys/class/android_usb/android0/functions", fn, sizeof fn);
    slurp_("/sys/class/android_usb/android0/idVendor", vid, sizeof vid);
    slurp_("/sys/class/android_usb/android0/idProduct", pid, sizeof pid);
    slurp_("/sys/class/android_usb/android0/enable", en, sizeof en);
    slurp_("/sys/class/android_usb/android0/f_mass_storage/lun/file", lun, sizeof lun);
    getprop_("sys.sony.config", cfg, sizeof cfg);
    getprop_("sys.usb.state", st, sizeof st);
    getprop_("sys.usb.msc1", msc1, sizeof msc1);
    std::fprintf(stderr,
        "[cinder-probe] usbmgr: gadget   functions=%s enable=%s %s:%s\n"
        "[cinder-probe] usbmgr: msc      lun/file=%s  (sys.usb.msc1=%s)\n"
        "[cinder-probe] usbmgr: init     sys.sony.config=%s  sys.usb.state=%s\n",
        fn, en, vid, pid, lun, msc1, cfg, st);
    // The medium check is the whole mass-storage bug in one line.
    if (std::strstr(fn, "mass_storage") && (std::strcmp(lun, "<empty>") == 0 ||
                                            std::strcmp(lun, "<absent>") == 0)) {
        std::fprintf(stderr, "[cinder-probe] usbmgr: >>> the gadget advertises MASS STORAGE with NO "
                             "BACKING MEDIUM — the host sees a drive with no disk. lun/file is "
                             "init's job (sys.sony.config=msc), and it has not run.\n");
    }
}

static unsigned g_usbmgr_prev = 0;   // what the service believed before we changed it

static int usbmgr_probe(unsigned want, int window_secs) {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    void* c = _ZN3pst8services28UsbMgrServiceFwClientFactory14CreateInstanceEv();
    if (!c) { clog_("usbmgr: UsbMgrServiceFwClientFactory returned null"); g_pump_run = false; _exit(1); }

    // Req/Rsp are 4-byte structs (SizeOfReqMsg_SetUsbFunction returns 4; WriteReqMsg does one
    // Alloc(4) and copies one word from offset 0). Oversized + zeroed anyway: over-allocating an
    // out-param is free, under-allocating smashes the stack.
    typedef void (*fnrr)(void*, const void*, void*);
    unsigned req[8], rsp[8];

    std::memset(req, 0, sizeof req); std::memset(rsp, 0, sizeof rsp);
    wd_arm(10);
    try { ((fnrr)vslot(c, VIDX_GetUsbFunction))(c, req, rsp); }
    catch (...) { clog_("usbmgr: GetUsbFunction threw"); }
    wd_disarm();
    // The GET responses carry TWO words — SizeOfRspMsg_GetUsbFunction returns 8 while
    // SizeOfRspMsg_SetUsbFunction returns 4 — so the layout is { uint32_t result; uint32_t value; }
    // and the answer is at offset 4, NOT 0. Measured on device before it was read back out of the
    // size functions: the first run printed `rsp 0 2 0 0` with the gadget in mass storage, i.e.
    // result=0 (ok) and value=2 (MSC). Reading rsp[0] here would report "0 (??)" forever.
    g_usbmgr_prev = rsp[1];
    std::fprintf(stderr, "[cinder-probe] usbmgr: service  GetUsbFunction = %u  (%s)  "
                         "[result=%u  raw %u %u %u %u]\n",
                 rsp[1], usbfn_name(rsp[1]), rsp[0], rsp[0], rsp[1], rsp[2], rsp[3]);

    std::memset(req, 0, sizeof req); std::memset(rsp, 0, sizeof rsp);
    wd_arm(10);
    try { ((fnrr)vslot(c, VIDX_GetAdbEnabled))(c, req, rsp); }
    catch (...) { clog_("usbmgr: GetAdbEnabled threw"); }
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] usbmgr: service  GetAdbEnabled  = %u  [result=%u]\n",
                 rsp[1], rsp[0]);

    usbmgr_dump_gadget();

    if (want == 0) {
        clog_("usbmgr: read-only. If the service's function disagrees with `functions` above, the "
              "two owners are fighting — that is the bug. Re-run as --usbmgr uac|msc [secs] to "
              "switch through the service (arms a restore child first).");
        g_pump_run = false;
        std::fflush(nullptr);
        _exit(0);
    }

    // THE ESCAPE FIRST. UpdateUsbFunction stops adbd around the switch, so if this probe loses the
    // shell we still need the gadget put back. The child holds no client of its own until it needs
    // one, and does nothing except restore — it depends on strictly less than what it rescues.
    if (window_secs <= 0) window_secs = 60;
    unsigned prev = g_usbmgr_prev ? g_usbmgr_prev : USBFN_MSC;
    pid_t kid = fork();
    if (kid == 0) {
        // DETACH FIRST, before anything else can go wrong.
        //
        // Learned the hard way 2026-08-11: the first version of this child stayed in the parent's
        // process group and session. Switching to UAC re-enumerates the gadget, `adb` drops, adbd
        // SIGHUPs the group — and the restore child died with the very shell it existed to rescue,
        // leaving the device in UAC with no way back in. That is precisely the escape-ladder rule
        // being broken: an escape must depend on STRICTLY LESS than the thing it rescues, and a
        // child sharing the dying session depends on exactly as much.
        //
        // setsid() puts it in its own session with no controlling terminal, and SIG_IGN on HUP/INT
        // means even a group-wide signal cannot take it.
        signal(SIGHUP, SIG_IGN);
        signal(SIGINT, SIG_IGN);
        signal(SIGTERM, SIG_IGN);
        setsid();
        // Child: no framework yet — build one, wait out the window, put the old function back.
        sleep((unsigned)window_secs);
        pst::core::Framework& cfw = pst::core::Framework::GetReference();
        cfw.StartForApplication(std::function<void()>(&pump_finish), true);
        void* cc = _ZN3pst8services28UsbMgrServiceFwClientFactory14CreateInstanceEv();
        if (cc) {
            unsigned rq[8], rp[8];
            std::memset(rq, 0, sizeof rq); std::memset(rp, 0, sizeof rp);
            rq[0] = prev;
            try { ((fnrr)vslot(cc, VIDX_SetUsbFunction))(cc, rq, rp); } catch (...) {}
        }
        _exit(0);
    }
    std::fprintf(stderr, "[cinder-probe] usbmgr: restore child pid=%d armed — puts function %u (%s) "
                         "back in %ds\n", (int)kid, prev, usbfn_name(prev), window_secs);

    std::memset(req, 0, sizeof req); std::memset(rsp, 0, sizeof rsp);
    req[0] = want;
    wd_arm(15);
    try { ((fnrr)vslot(c, VIDX_SetUsbFunction))(c, req, rsp); }
    catch (...) { clog_("usbmgr: SetUsbFunction threw"); }
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] usbmgr: SetUsbFunction(%u = %s) -> rsp %u\n",
                 want, usbfn_name(want), rsp[0]);

    sleep(2);                       // let UpdateUsbFunction finish its stop-adbd/rewrite/start dance
    usbmgr_dump_gadget();
    if (want == USBFN_UAC) {
        clog_("usbmgr: if `functions` now reads audio_func,adb the service owns the gadget and the "
              "PC should enumerate a sound card. Audio still needs the host to STREAM and "
              "UsbDeviceAudioPlayerServiceClient::Start (slot 4) to be called once the format "
              "arrives — see OnChangedFormat, listener slot 2.");
    } else {
        clog_("usbmgr: MSC through the service sets the DESCRIPTOR ONLY. Until lun/file points at "
              "/emmc@contents and /contents is unmounted device-side (init's half), the host still "
              "sees a drive with no medium.");
    }
    std::fflush(nullptr);
    g_pump_run = false;
    return 0;
}

// ── --uacgate : WHY DOES UsbDeviceAudioPlayerService SAY kFormatNone FOREVER? ───────────────────
// Recovered 2026-08-11 by static RE of libUsbDeviceAudioPlayerService.so; this probe exists to
// measure the three links of the chain the RE exposed, all at once and all read-only.
//
// The service does NOT learn the stream format from ALSA, and it does not poll. The chain is:
//
//   1. connmgr says the device is enabled.  UsbAudioConnectionMonitor::Open (0x1e1d0) calls
//      funcarch::connmgr::ConnMgrService::GetDeviceStatus(Device=7, DeviceStatus&) and registers a
//      DeviceListener. Every status change lands in UsbAudioPlayerCore::NotifyChangeConnectionStatus
//      (0x16c44) — and THAT function is the gate: `if (status == 1) UsbAudioStreamMonitor::Open()`,
//      else ClearStreamInfo + StopPlaying. Nothing else opens the monitor.
//   2. the stream monitor's socket.  UsbAudioStreamMonitor::UacInitHotplugSock (0x1ebf4) is,
//      instruction for instruction:
//          socket(AF_NETLINK=16, SOCK_DGRAM=2, 24)          // proto 24 — an MTK/Sony private one
//          setsockopt(fd, SOL_SOCKET, SO_RCVBUFFORCE=33, 2048)
//          bind(fd, {nl_family=16, nl_pid=getpid(), nl_groups=1}, 12)
//   3. the kernel sends the format.  RecvUACEvent (0x1ef00) recvmsg()s into a 2048-byte buffer,
//      SKIPS THE FIRST 16 BYTES (the nlmsghdr), then splits the rest on '\n'/'\r' and hands each
//      line to ParseStreamInfo, which matches "ACTION=", "FORMAT=", "FREQ=", "BITWIDTH=" with
//      values from {STOP,PLAY,NONE} and the frequency table 32000..11289600.
//
// So `GetStatus format=0` has exactly three possible causes, and this probe separates them:
//   * connmgr device 7 never goes enabled -> link 1 is broken, the socket is never even opened;
//   * the socket is open but the kernel never sends -> link 3, i.e. the host is not streaming (or
//     f_audio_func never armed), and no amount of calling Start() will help;
//   * events DO arrive -> the service is being told and is failing to act, and cinder-home should
//     stop asking it: read the netlink event ourselves and bridge hw:4,0 capture straight out.
//
// Nothing here writes anything. Netlink group 1 is multicast, so listening alongside Sony's own
// service is invisible to it.
//
//   /tmp/cinder-probe --uacgate [secs] [delay] > /contents/uacgate.log 2>&1
// `delay` forks a detached child that starts measuring that many seconds later — the same trick
// --uaccap needs, because attaching the gadget for adb is exactly what stops the PC using it as a
// sound card.
extern "C" {
// libConnMgrService.so — the funcarch wrapper. It is stateless (the ctor at 0x60bc only touches the
// stack guard), so a dummy `this` is legitimate: every method re-fetches the client by name via
// Framework::GetServiceClient("ConnMgrServiceFw").
int _ZN3pst8services8funcarch7connmgr14ConnMgrService15GetDeviceStatusERKNS1_6DeviceERNS2_12DeviceStatusE(
        void* self, const int* device, void* status_out);
int _ZN3pst8services8funcarch7connmgr14ConnMgrService19GetUsbHostSuspendedEv(void* self);
// libUsbDeviceConnectionService.so — the THIRD gadget owner, and the only complete one. See the
// long note in main.cpp next to usb_set_device_type(): SetDeviceType (client vtable slot 5) is what
// rewrites the gadget AND attaches/detaches the mass-storage medium AND makes the connect event
// fire that opens the audio service's netlink socket.
void* _ZN3pst8services39UsbDeviceConnectionServiceClientFactory14CreateInstanceEv(void);
}
enum { USBDT_ADB = 1, USBDT_MSC = 2, USBDT_UAC = 3 };   // cmp #1/#2/#3 dispatch @0xab98

static const char* usbdt_name(unsigned t) {
    return t == USBDT_UAC ? "Uac" : t == USBDT_MSC ? "Msc" : t == USBDT_ADB ? "Adb" : "??";
}

static bool usbdt_set(unsigned t) {
    enum { VIDX_SetDeviceType = 5 };
    void* cli = nullptr;
    try { cli = _ZN3pst8services39UsbDeviceConnectionServiceClientFactory14CreateInstanceEv(); }
    catch (...) { cli = nullptr; }
    if (!cli) { clog_("usbdt: factory returned null"); return false; }
    typedef int (*fnr)(void*, const unsigned*);
    int rc = -1;
    wd_arm(20);
    try { rc = ((fnr)vslot(cli, VIDX_SetDeviceType))(cli, &t); }
    catch (...) { wd_disarm(); clog_("usbdt: SetDeviceType threw"); return false; }
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] usbdt: SetDeviceType(%u = %s) rc=%d\n", t, usbdt_name(t), rc);
    std::fflush(nullptr);
    return true;
}

// The names ConnMgrServiceFw prints for Device (its own dump: "device[%u:%s]"), in the order they
// sit in .rodata. The first index is NOT confirmed — the strings are contiguous so the pointer
// table could not be recovered statically — which is the other reason this probe enumerates the
// whole range and prints it: one run on a device with a jack plugged in pins the mapping.
static const char* connmgr_device_guess(int d) {
    static const char* n[] = { "LineIn", "BtlHeadphone", "SeHeadphone", "LineOut", "UacDevice",
                               "A2dpSink", "MscHost", "UacHost", "AvrcpTg", "HostCable",
                               "SdCard0", "SdCard1", "Invalid" };
    return (d >= 0 && d < (int)(sizeof n / sizeof n[0])) ? n[d] : "?";
}

// DeviceStatus is 8 bytes: funcarch::GetDeviceStatus copies exactly one d-register (vst1.8 {d16})
// out of the reply at offset 4, and UsbAudioConnectionMonitor reads word 0 and compares it to 1.
struct ConnDeviceStatus { unsigned enabled; unsigned connected; };

static void uacgate_dump_connmgr(const char* when) {
    char self[64];                                  // stateless: any address will do as `this`
    std::fprintf(stderr, "[cinder-probe] uacgate: connmgr device status (%s)\n", when);
    for (int d = 0; d <= 12; d++) {
        ConnDeviceStatus st;
        std::memset(&st, 0xEE, sizeof st);
        int rc = -1;
        wd_arm(8);
        try {
            rc = _ZN3pst8services8funcarch7connmgr14ConnMgrService15GetDeviceStatusERKNS1_6DeviceERNS2_12DeviceStatusE(
                     self, &d, &st);
        } catch (...) { rc = -2; }
        wd_disarm();
        if (rc != 0) {
            std::fprintf(stderr, "[cinder-probe] uacgate:   device %2d (%-12s) rc=%d\n",
                         d, connmgr_device_guess(d), rc);
            continue;
        }
        std::fprintf(stderr, "[cinder-probe] uacgate:   device %2d (%-12s) enabled=%u connected=%u%s\n",
                     d, connmgr_device_guess(d), st.enabled, st.connected,
                     d == 7 ? "   <-- THE GATE: UsbAudioConnectionMonitor watches this one" : "");
    }
    int susp = -1;
    wd_arm(8);
    try { susp = _ZN3pst8services8funcarch7connmgr14ConnMgrService19GetUsbHostSuspendedEv(self); }
    catch (...) {}
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] uacgate:   GetUsbHostSuspended = %d\n", susp);
}

static void uacgate_dump_uacsysfs(void) {
    static const char* n[] = { "f_allow", "f_valid", "f_start", "f_thresh", "f_plus", "f_minus" };
    char line[512];
    int  off = std::snprintf(line, sizeof line, "[cinder-probe] uacgate: f_audio_func ");
    for (size_t i = 0; i < sizeof n / sizeof n[0]; i++) {
        char p[128], v[64];
        std::snprintf(p, sizeof p, "/sys/class/android_usb/android0/f_audio_func/%s", n[i]);
        slurp_(p, v, sizeof v);
        off += std::snprintf(line + off, sizeof line - off, "%s=%s ", n[i], v);
        if (off >= (int)sizeof line - 40) break;
    }
    std::fprintf(stderr, "%s\n", line);
}

static void uacgate_dump_cards(void) {
    char v[128];
    slurp_("/proc/asound/cards", v, sizeof v);
    std::fprintf(stderr, "[cinder-probe] uacgate: /proc/asound/cards[0] = %s\n", v);
    for (int c = 1; c <= 8; c++) {
        char p[96];
        std::snprintf(p, sizeof p, "/proc/asound/card%d/pcm0c/sub0/status", c);
        FILE* f = std::fopen(p, "r");
        if (!f) continue;
        char st[96] = {0};
        if (std::fgets(st, sizeof st, f)) {
            size_t l = std::strlen(st);
            while (l && (st[l-1] == '\n' || st[l-1] == '\r')) st[--l] = 0;
        }
        std::fclose(f);
        std::fprintf(stderr, "[cinder-probe] uacgate: capture card%d pcm0c = %s%s\n", c, st,
                     c == 4 ? "   (hw:4,0 — the device Sony's UsbAudioPlayerInhal hardcodes)" : "");
    }
}

// Netlink proto 24 only EXISTS while the UAC gadget function is loaded — with the gadget at
// mass_storage,adb this returns ENOPROTOOPT ("Protocol not supported"), which is itself a clean
// yes/no on whether the UAC function is live. So it gets retried after engaging.
static int uacgate_open_nl(bool quiet) {
    int fd = socket(16 /*AF_NETLINK*/, SOCK_DGRAM, 24);
    if (fd < 0) {
        if (!quiet)
            std::fprintf(stderr, "[cinder-probe] uacgate: socket(AF_NETLINK, SOCK_DGRAM, 24) "
                                 "failed: %s  (the UAC function is not loaded)\n", std::strerror(errno));
        return -1;
    }
    int rcvbuf = 2048;
    setsockopt(fd, SOL_SOCKET, 33 /*SO_RCVBUFFORCE*/, &rcvbuf, sizeof rcvbuf);
    // struct sockaddr_nl, laid out by hand so this file needs no linux/netlink.h.
    struct { unsigned short family; unsigned short pad; unsigned pid; unsigned groups; } sa;
    std::memset(&sa, 0, sizeof sa);
    sa.family = 16;
    sa.pid    = (unsigned)getpid();
    sa.groups = 1;
    if (bind(fd, (struct sockaddr*)&sa, 12) < 0) {
        std::fprintf(stderr, "[cinder-probe] uacgate: bind(nl_groups=1) failed: %s\n",
                     std::strerror(errno));
        close(fd);
        return -1;
    }
    clog_("uacgate: netlink proto 24 group 1 bound — the same socket Sony's stream monitor uses");
    return fd;
}

static int uacgate_probe(int secs, int delay, int engage) {
    install_diagnostics();
    if (secs <= 0) secs = 30;

    if (delay > 0) {
        pid_t kid = fork();
        if (kid != 0) {
            std::fprintf(stderr, "[cinder-probe] uacgate: armed (pid %d) — listening for %ds of UAC "
                                 "netlink in %ds. Put the player in USB-DAC mode and start playback "
                                 "on the PC now.\n", (int)kid, secs, delay);
            std::fflush(nullptr);
            return 0;
        }
        signal(SIGHUP, SIG_IGN);
        signal(SIGINT, SIG_IGN);
        signal(SIGTERM, SIG_IGN);
        setsid();
        sleep((unsigned)delay);
    }

    // Open the socket FIRST, before any of the slow service calls: a format event that arrives
    // while we are still enumerating connmgr is an event we would otherwise miss.
    int fd = uacgate_open_nl(false);

    uacgate_dump_uacsysfs();
    usbmgr_dump_gadget();
    uacgate_dump_cards();

    // connmgr needs the framework pump, so it comes after the socket is already listening.
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);
    uacgate_dump_connmgr("before");

    // ── optional: DO the switch, so the whole chain can be measured in one run ──────────────────
    // THE ESCAPE FIRST, as always. Switching to Uac re-enumerates the gadget; over a usbipd
    // passthrough that detaches the binding and adb goes with it, so the thing that puts the gadget
    // back must not depend on this shell, this process group, or this process surviving.
    if (engage) {
        pid_t kid = fork();
        if (kid == 0) {
            signal(SIGHUP, SIG_IGN);
            signal(SIGINT, SIG_IGN);
            signal(SIGTERM, SIG_IGN);
            setsid();
            sleep((unsigned)(secs + 30));
            pst::core::Framework& cfw = pst::core::Framework::GetReference();
            cfw.StartForApplication(std::function<void()>(&pump_finish), true);
            usbdt_set(USBDT_ADB);
            _exit(0);
        }
        std::fprintf(stderr, "[cinder-probe] uacgate: restore child pid=%d armed — SetDeviceType(Adb) "
                             "in %ds\n", (int)kid, secs + 30);
        usbdt_set(USBDT_UAC);
        sleep(3);                       // let adbd bounce and the uevent land
        uacgate_dump_uacsysfs();
        usbmgr_dump_gadget();
        uacgate_dump_cards();
        uacgate_dump_connmgr("after SetDeviceType(Uac)");
        if (fd < 0) fd = uacgate_open_nl(false);   // proto 24 should exist now
    }

    long events = 0;
    time_t end = time(nullptr) + secs;
    time_t next_beat = time(nullptr) + 5;
    while (time(nullptr) < end) {
        struct pollfd pfd;
        pfd.fd = fd; pfd.events = POLLIN; pfd.revents = 0;
        int pr = (fd >= 0) ? poll(&pfd, 1, 1000) : (usleep(200000), 0);
        if (pr > 0 && (pfd.revents & POLLIN)) {
            char buf[2048];
            ssize_t got = recv(fd, buf, sizeof buf - 1, 0);
            if (got > 16) {
                buf[got] = 0;
                events++;
                // Sony skips 16 bytes of nlmsghdr, so we do too; the payload is NUL/newline
                // separated key=value text.
                std::fprintf(stderr, "[cinder-probe] uacgate: EVENT #%ld (%d bytes payload): ",
                             events, (int)(got - 16));
                for (ssize_t i = 16; i < got; i++) {
                    unsigned char ch = (unsigned char)buf[i];
                    if (ch == 0 || ch == '\n' || ch == '\r') std::fputc('|', stderr);
                    else if (ch >= 32 && ch < 127)           std::fputc(ch, stderr);
                    else                                     std::fprintf(stderr, "\\x%02x", ch);
                }
                std::fputc('\n', stderr);
                std::fflush(stderr);
            }
        }
        if (time(nullptr) >= next_beat) {
            next_beat = time(nullptr) + 5;
            uacgate_dump_uacsysfs();
            uacgate_dump_cards();
            if (fd < 0) fd = uacgate_open_nl(true);   // the function may load late
        }
    }
    if (fd >= 0) close(fd);
    uacgate_dump_connmgr("after");
    if (engage) {
        usbdt_set(USBDT_ADB);           // don't make the user wait out the restore child
        sleep(2);
        usbmgr_dump_gadget();
    }

    std::fprintf(stderr, "[cinder-probe] uacgate: %ld netlink event(s) in %ds\n", events, secs);
    if (events > 0) {
        clog_("uacgate: >>> the kernel IS announcing the stream. If cinder-home still logs "
              "kFormatNone, UsbDeviceAudioPlayerService is not acting on it — read this socket "
              "ourselves and bridge hw:4,0 capture directly, the same shape as the LDAC bridge.");
    } else {
        clog_("uacgate: no events. Compare the two dumps above: connmgr device 7 never enabled "
              "means the service never even opened this socket (link 1); device 7 enabled with "
              "f_valid/f_start still 0 means the HOST is not streaming (link 3).");
    }
    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);   // like every other probe mode: returning unwinds through the live pump thread and
                // dies with "Fatal error during phase 1 unwinding" AFTER all output, which reads
                // like a probe failure when it is only a teardown race.
}

// ── --funcmode : FuncMode, the thing that actually gates USB-DAC ────────────────────────────────
// Every previous round drove the USB gadget directly (sys.sony.config, UsbMgrServiceFw::
// SetUsbFunction, UsbDeviceConnectionService::SetDeviceType) and the audio service stayed silent.
// It stayed silent because the gadget is not what it is watching. The real chain, recovered from
// libConnMgrServiceFw.so:
//
//   ConnGlueUsbHost::CnvStatus(Device, connected, FuncMode, DeviceStatus& out)   @0x19f24
//       if (!connected)      out = { 2, 0 };
//       else if (dev != 7)   out = { 2, 1 };
//       else                 out = { 2, (FuncMode == 1) };     <-- device 7 == UacHost
//
// and UsbAudioConnectionMonitor::Open (@0x1e1d0, libUsbDeviceAudioPlayerService.so) does
// GetDeviceStatus(Device{7}, st) and believes it only when the word it reads is 1. So the gate on
// USB-DAC is not the descriptor, not the connect event, not the netlink socket — it is
// **FuncMode == 1**, and nothing we have ever called changed FuncMode.
//
// The names come from a std::map<FuncMode,const char*> that funcarch::GetName builds inline
// (@0x7e00, libFuncMgrServiceFw.so): eight keys, 0..7, against the contiguous .rodata run at
// 0xba69 — so the enum below is read off the binary, not guessed. GetCurrentFuncMode returns 9 of
// its own accord when the binder call fails, which is why "Invalid" is a fallback and not a key.
//
// FuncMgrServiceServiceImpl::EnterFuncMode (@0x7fb4) is what stock runs when the user picks
// USB-DAC in Settings, and it is three calls under one mutex, in this order:
//       FireRequireExitFuncMode(current)                 (listeners may veto)
//       usbmgr::UsbMgrService::SetUsbFunction(...)        <-- the only step we were doing
//       connmgr::ConnMgrService::SetDeviceHandleRules(...)  <-- publishes device 7
//       pathmgr::PathMgrService::SetPath(...)             <-- the audio routing path
// It also early-outs if the requested mode already equals the current one, so a no-op result is
// meaningful rather than a failure.
extern "C" {
// libFuncMgrService.so — the funcarch wrapper, stateless exactly like ConnMgrService above: the
// ctor at 0x5d84 only reads the stack guard, and each method re-fetches the client by name via
// Framework::GetServiceClient("FuncMgrServiceFw"). A dummy `this` is therefore legitimate.
int  _ZN3pst8services8funcarch7funcmgr14FuncMgrService18GetCurrentFuncModeEv(void* self);
bool _ZN3pst8services8funcarch7funcmgr14FuncMgrService13EnterFuncModeERKNS1_8FuncModeE(
        void* self, const int* mode);
}

enum { FM_MEDIAPLAY = 0, FM_USBDAC = 1, FM_A2DPSINK = 2, FM_FM = 3,
       FM_DIRECTREC = 4, FM_DMR = 5, FM_DMS = 6, FM_INITIAL = 7, FM_INVALID = 9 };

static const char* funcmode_name(int m) {
    static const char* n[] = { "MediaPlay", "UsbDac", "A2dpSink", "Fm",
                               "DirectRec", "Dmr", "Dms", "Initial" };
    return (m >= 0 && m < (int)(sizeof n / sizeof n[0])) ? n[m] : "Invalid";
}

static int funcmode_get(void) {
    char self[64];
    int m = -1;
    wd_arm(8);
    try { m = _ZN3pst8services8funcarch7funcmgr14FuncMgrService18GetCurrentFuncModeEv(self); }
    catch (...) { m = -2; }
    wd_disarm();
    return m;
}

static void funcmode_report(const char* when) {
    int m = funcmode_get();
    std::fprintf(stderr, "[cinder-probe] funcmode: current = %d (%s)   [%s]\n",
                 m, funcmode_name(m), when);
    std::fflush(nullptr);
}

// The bool this returns is NOT a result. Measured 2026-08-11: both EnterFuncMode(1) and
// EnterFuncMode(0) returned false on a run where GetCurrentFuncMode, connmgr device 7, the gadget
// descriptor and /proc/asound all confirmed the transition happened. Same trap as
// UsbDeviceConnectionServiceClient::SetDeviceType's rc — judge by read-back, never by return value.
static bool funcmode_enter(int mode) {
    char self[64];
    wd_arm(30);
    try { (void)_ZN3pst8services8funcarch7funcmgr14FuncMgrService13EnterFuncModeERKNS1_8FuncModeE(
                   self, &mode); }
    catch (...) { wd_disarm(); clog_("funcmode: EnterFuncMode threw"); return false; }
    wd_disarm();
    int now = funcmode_get();
    std::fprintf(stderr, "[cinder-probe] funcmode: EnterFuncMode(%d = %s) — read-back = %d (%s) %s\n",
                 mode, funcmode_name(mode), now, funcmode_name(now),
                 now == mode ? "OK" : "DID NOT TAKE");
    std::fflush(nullptr);
    return now == mode;
}

// EnterFuncMode installs the SERVICE's descriptor, and neither UsbDac (`audio_func`) nor MediaPlay
// (`mass_storage`) carries the adb interface — so a probe that switches modes and stops there
// leaves the box with no adb at all, recoverable only by a reboot. That happened on the first run.
// Re-driving init's adb block is the same lever `cinder-msc usb-rescue` uses, and it is a no-op if
// adb is already composed in.
static void funcmode_recompose_adb(void) {
    clog_("funcmode: re-composing adb into the gadget (setprop sys.sony.config adb)");
    (void)std::system("setprop sys.sony.config adb");
    for (int i = 0; i < 60; i++) {
        char v[128];
        v[0] = 0;
        getprop_("sys.usb.state", v, sizeof v);
        if (std::strstr(v, "adb")) break;
        usleep(500000);
    }
    usbmgr_dump_gadget();
}

// want < 0 => READ-ONLY. Always run that form first; it cannot change anything.
static int funcmode_probe(int want, int restore_secs, int watch_secs) {
    install_diagnostics();
    if (watch_secs   <= 0) watch_secs   = 30;
    if (restore_secs <= 0) restore_secs = watch_secs + 60;

    // Detach before doing anything that touches the gadget. SetUsbFunction re-enumerates, adbd
    // bounces, and this process — a child of adbd — gets SIGHUP'd mid-experiment otherwise, which
    // would take the in-process restore with it. Read-only runs stay in the foreground.
    if (want >= 0) {
        pid_t self = fork();
        if (self != 0) {
            std::fprintf(stderr, "[cinder-probe] funcmode: detached (pid %d) — EnterFuncMode(%d = %s), "
                                 "watch %ds, restore child at %ds. Output is going to the log file, "
                                 "not this shell.\n",
                         (int)self, want, funcmode_name(want), watch_secs, restore_secs);
            std::fflush(nullptr);
            return 0;
        }
        signal(SIGHUP,  SIG_IGN);
        signal(SIGINT,  SIG_IGN);
        signal(SIGTERM, SIG_IGN);
        setsid();
    }

    int fd = (want == FM_USBDAC) ? uacgate_open_nl(true) : -1;

    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    funcmode_report("before");
    uacgate_dump_connmgr("before");
    usbmgr_dump_gadget();
    uacgate_dump_uacsysfs();
    uacgate_dump_cards();

    if (want < 0) {
        clog_("funcmode: read-only run. device 7 connected=1 requires FuncMode==1 (UsbDac); "
              "re-run as `--funcmode 1` to make the transition.");
        g_pump_run = false;
        std::fflush(nullptr);
        _exit(0);
    }

    // THE ESCAPE FIRST. EnterFuncMode calls SetUsbFunction, which re-enumerates the gadget; over a
    // usbipd passthrough that detaches the binding and takes adb with it. So the thing that puts
    // the player back into MediaPlay must not depend on this shell, this process group, or this
    // process still being alive — same rule as the boot ladder.
    pid_t kid = fork();
    if (kid == 0) {
        signal(SIGHUP, SIG_IGN);
        signal(SIGINT, SIG_IGN);
        signal(SIGTERM, SIG_IGN);
        setsid();
        sleep((unsigned)restore_secs);
        pst::core::Framework& cfw = pst::core::Framework::GetReference();
        cfw.StartForApplication(std::function<void()>(&pump_finish), true);
        funcmode_enter(FM_MEDIAPLAY);
        funcmode_recompose_adb();        // or the box comes back with no adb at all
        _exit(0);
    }
    std::fprintf(stderr, "[cinder-probe] funcmode: restore child pid=%d armed — "
                         "EnterFuncMode(0 = MediaPlay) in %ds\n", (int)kid, restore_secs);
    std::fflush(nullptr);

    funcmode_enter(want);
    sleep(3);                            // let the three inner calls land and the uevent settle
    funcmode_report("after EnterFuncMode");
    uacgate_dump_connmgr("after EnterFuncMode");
    usbmgr_dump_gadget();
    uacgate_dump_uacsysfs();
    uacgate_dump_cards();
    if (fd < 0 && want == FM_USBDAC) fd = uacgate_open_nl(false);   // proto 24 should exist now

    long events = 0;
    time_t end = time(nullptr) + watch_secs;
    time_t next_beat = time(nullptr) + 5;
    while (time(nullptr) < end) {
        struct pollfd pfd;
        pfd.fd = fd; pfd.events = POLLIN; pfd.revents = 0;
        int pr = (fd >= 0) ? poll(&pfd, 1, 1000) : (usleep(200000), 0);
        if (pr > 0 && (pfd.revents & POLLIN)) {
            char buf[2048];
            ssize_t got = recv(fd, buf, sizeof buf - 1, 0);
            if (got > 16) {
                buf[got] = 0;
                events++;
                std::fprintf(stderr, "[cinder-probe] funcmode: EVENT #%ld (%d bytes payload): ",
                             events, (int)(got - 16));
                for (ssize_t i = 16; i < got; i++) {
                    unsigned char ch = (unsigned char)buf[i];
                    if (ch == 0 || ch == '\n' || ch == '\r') std::fputc('|', stderr);
                    else if (ch >= 32 && ch < 127)           std::fputc(ch, stderr);
                    else                                     std::fprintf(stderr, "\\x%02x", ch);
                }
                std::fputc('\n', stderr);
                std::fflush(stderr);
            }
        }
        if (time(nullptr) >= next_beat) {
            next_beat = time(nullptr) + 5;
            uacgate_dump_uacsysfs();
            uacgate_dump_cards();
            uacgate_dump_connmgr("beat");
            if (fd < 0 && want == FM_USBDAC) fd = uacgate_open_nl(true);
        }
    }
    if (fd >= 0) close(fd);

    funcmode_enter(FM_MEDIAPLAY);        // don't make the user wait out the restore child
    sleep(2);
    funcmode_report("restored");
    uacgate_dump_connmgr("restored");
    funcmode_recompose_adb();            // MediaPlay's descriptor has no adb either — put it back

    std::fprintf(stderr, "[cinder-probe] funcmode: %ld netlink event(s) in %ds\n",
                 events, watch_secs);
    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

// ── --disp / --dispoff : the PANEL POWER path (battery) ─────────────────────────────────────────
// Measured 2026-08-11, screen dark + idle + not playing: the system does ~354 context switches a
// second, and 230 of them belong to two MediaTek kernel threads —
//   /proc/<pid>/wchan = disp_ovl_engine_rdma0_update_kthread   (~120/s)
//   /proc/<pid>/wchan = _DISP_ConfigUpdateKThread              (~109/s)
// i.e. the display pipeline is still running with the backlight at 0, because Cinder's screen-off
// writes /sys/class/leds/lcd-backlight/brightness and nothing else. `echo 4 > fb0/blank` does NOT
// help: the write is accepted (rc=0) but `fb0/state` stays 0 and the two kthreads keep the same
// CPU (measured +5 jiffies/15 s blanked vs +7 unblanked — noise).
//
// Sony's own panel switch is a service call, not a node: DisplayService::SetLCDValidate(bool).
// Vtable recovered from the R_ARM_ABS32 relocations covering libDisplayService.so's .data.rel.ro
// (the words on disk are zero — see the same technique in RE_findings.md round g):
//   8 SetLCDValidate  9 SetLCDValidateGradually  10 GetLCDValidate
//  11 SetLCDBacklightBrightness  12 GetLCDBacklightBrightness
//  13 SetTouchPanelValidate  14 GetTouchPanelValidate  15 SetDimmer
//
// SetTouchPanelValidate matters too: the log line "input: touch WAKE — no touch sleep node found"
// means cinder-home's himax sleep-node path is a NO-OP on this unit, so the touch controller has
// never actually slept. This is the service that does it.
//
// TWO MODES ON PURPOSE. `--disp` is READ-ONLY and always safe. `--dispoff` is the experiment, and
// it can leave the panel dark if the service refuses to bring it back — so it forks a restore
// child FIRST. That child re-validates the LCD and the touch panel after the window regardless of
// what happens to the parent (crash, watchdog _exit, adb drop). Same rule as the boot ladder: the
// escape must depend on less than the thing it rescues, and a forked child that only calls
// SetLCDValidate(true) depends on less than the probe that turned it off.
extern "C" void* _ZN3pst8services27DisplayServiceClientFactory14CreateInstanceEv(void);

enum { VIDX_SetLCDValidate = 8, VIDX_GetLCDValidate = 10,
       VIDX_SetLCDBacklightBrightness = 11, VIDX_GetLCDBacklightBrightness = 12,
       VIDX_SetTouchPanelValidate = 13, VIDX_GetTouchPanelValidate = 14 };

// ── --displight <level> [restore_secs] : the backlight control that actually OWNS the panel ──
//
// Writing 0 to /sys/class/leds/lcd-backlight/brightness does NOT turn the panel off on this
// firmware: measured 2026-08-19 with the node reading 0 while the panel was lit and
// DisplayService reporting `backlight=2`. The LED node is not the effective control — Sony's
// DisplayService owns the level and re-asserts it — so the switch is slot 11,
// `SetLCDBacklightBrightness(const uint32_t&)`.
//
// Backlight only: the LCD and the touch panel stay VALID, unlike --dispoff. A restore child is
// armed first regardless, because the whole point of the test is a screen you cannot read.
static int displight_probe(unsigned level, int restore_secs) {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    void* d = _ZN3pst8services27DisplayServiceClientFactory14CreateInstanceEv();
    if (!d) { clog_("displight: DisplayServiceClientFactory returned null"); g_pump_run = false; _exit(1); }
    typedef void (*fnru)(void*, unsigned*);
    typedef void (*fnwu)(void*, const unsigned*);

    unsigned before = 0xDEADu;
    wd_arm(10);
    try { ((fnru)vslot(d, VIDX_GetLCDBacklightBrightness))(d, &before); } catch (...) {}
    wd_disarm();
    char node[16] = "?";
    {
        FILE* nf = std::fopen("/sys/class/leds/lcd-backlight/brightness", "r");
        if (nf) {
            if (!std::fgets(node, sizeof node, nf)) std::strcpy(node, "?");
            std::fclose(nf);
            char* nl = std::strpbrk(node, "\r\n");
            if (nl) *nl = 0;
        }
    }
    std::fprintf(stderr, "[cinder-probe] displight: before=%u (service)  node=%s\n", before, node);

    if (restore_secs > 0) {
        pid_t kid = fork();
        if (kid == 0) {
            signal(SIGHUP, SIG_IGN); signal(SIGINT, SIG_IGN); signal(SIGTERM, SIG_IGN);
            setsid();
            for (int i = 0; i < restore_secs; i++) sleep(1);
            void* d2 = _ZN3pst8services27DisplayServiceClientFactory14CreateInstanceEv();
            const unsigned back = (before == 0xDEADu || before == 0) ? 5u : before;
            if (d2) { try { ((fnwu)vslot(d2, VIDX_SetLCDBacklightBrightness))(d2, &back); } catch (...) {} }
            FILE* f = std::fopen("/sys/class/leds/lcd-backlight/brightness", "w");
            if (f) { std::fputs("5", f); std::fclose(f); }
            _exit(0);
        }
        std::fprintf(stderr, "[cinder-probe] displight: restore child pid=%d armed (+%ds)\n",
                     (int)kid, restore_secs);
    }

    wd_arm(10);
    try { ((fnwu)vslot(d, VIDX_SetLCDBacklightBrightness))(d, &level); }
    catch (...) { clog_("displight: SetLCDBacklightBrightness threw"); }
    wd_disarm();
    sleep(1);
    unsigned after = 0xDEADu;
    wd_arm(10);
    try { ((fnru)vslot(d, VIDX_GetLCDBacklightBrightness))(d, &after); } catch (...) {}
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] displight: SetLCDBacklightBrightness(%u) -> reads %u  "
                         "(LOOK AT THE PANEL NOW)\n", level, after);
    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

static int disp_probe(int off_secs) {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    void* d = _ZN3pst8services27DisplayServiceClientFactory14CreateInstanceEv();
    if (!d) { clog_("disp: DisplayServiceClientFactory returned null"); g_pump_run = false; _exit(1); }
    typedef void (*fnrb)(void*, bool*);
    typedef int  (*frtb)(void*, bool*);      // Set/GetTouchPanelValidate return bool
    typedef void (*fnwb)(void*, const bool*);
    typedef int  (*fwtb)(void*, const bool*);
    typedef void (*fnru)(void*, unsigned*);

    bool lcd = false, tp = false;
    unsigned bl = 0xDEADu;
    wd_arm(10);
    try {
        ((fnrb)vslot(d, VIDX_GetLCDValidate))(d, &lcd);
        ((frtb)vslot(d, VIDX_GetTouchPanelValidate))(d, &tp);
        ((fnru)vslot(d, VIDX_GetLCDBacklightBrightness))(d, &bl);
    } catch (...) { clog_("disp: a read threw"); }
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] disp: GetLCDValidate=%d  GetTouchPanelValidate=%d  "
                         "backlight=%u%s\n", (int)lcd, (int)tp, bl,
                 bl == 0xDEADu ? "  <-- UNTOUCHED: the service wrote nothing" : "");
    if (off_secs <= 0) {
        clog_("disp: read-only. Re-run as --dispoff <secs> to measure the panel actually powered "
              "down (that mode arms a restore child first).");
        g_pump_run = false;
        std::fflush(nullptr);
        _exit(0);
    }

    // ARM THE ESCAPE BEFORE TAKING THE RISK. This child holds its own client, so it does not share
    // a single object (or a single crash) with the parent.
    pid_t kid = fork();
    if (kid == 0) {
        for (int i = 0; i < off_secs + 10; i++) sleep(1);
        void* d2 = _ZN3pst8services27DisplayServiceClientFactory14CreateInstanceEv();
        const bool on = true;
        if (d2) {
            try { ((fnwb)vslot(d2, VIDX_SetLCDValidate))(d2, &on); } catch (...) {}
            try { ((fwtb)vslot(d2, VIDX_SetTouchPanelValidate))(d2, &on); } catch (...) {}
        }
        // Belt and braces: the backlight node is not a service call and cannot fail the same way.
        FILE* f = std::fopen("/sys/class/leds/lcd-backlight/brightness", "w");
        if (f) { std::fputs("5", f); std::fclose(f); }
        _exit(0);
    }
    std::fprintf(stderr, "[cinder-probe] disp: restore child pid=%d armed (+%ds)\n", (int)kid,
                 off_secs + 10);

    const bool off = false;
    wd_arm(10);
    try { ((fnwb)vslot(d, VIDX_SetLCDValidate))(d, &off); } catch (...) { clog_("disp: SetLCDValidate threw"); }
    try { ((fwtb)vslot(d, VIDX_SetTouchPanelValidate))(d, &off); } catch (...) { clog_("disp: SetTouchPanelValidate threw"); }
    wd_disarm();
    clog_("disp: LCD + touch panel INVALIDATED — measure now");

    for (int i = 0; i < off_secs; i++) sleep(1);

    bool lcd2 = false, tp2 = false;
    const bool on = true;
    wd_arm(10);
    try { ((fnwb)vslot(d, VIDX_SetLCDValidate))(d, &on); } catch (...) {}
    try { ((fwtb)vslot(d, VIDX_SetTouchPanelValidate))(d, &on); } catch (...) {}
    try {
        ((fnrb)vslot(d, VIDX_GetLCDValidate))(d, &lcd2);
        ((frtb)vslot(d, VIDX_GetTouchPanelValidate))(d, &tp2);
    } catch (...) {}
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] disp: restored — GetLCDValidate=%d GetTouchPanelValidate=%d\n",
                 (int)lcd2, (int)tp2);
    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

// ── --btvollisten : which listener slot carries the sink's volume? ──────────────────────────────
// The sink is the authority on its own level: with a step-only link (SetCurrentVolume refused —
// measured on CMF Buds Pro 2, every press) Cinder's absolute 0..MAX counter has no relationship to
// what the headphones are actually doing, which is why the UI can read mute while audio plays.
// BtTransmitterServiceListener::OnNotifyChangeVolume is the sink telling us the truth.
//
// The callback ORDER is inferred from the order the `BtTransmitterServiceListener::*` strings
// appear in .rodata, which is declaration order for a vtable-heavy class but is not proof — so
// every slot is instrumented and the one that fires when the volume moves is the answer. Args are
// logged as POINTERS plus one byte, never dereferenced as a container: a wrong guess about a slot
// taking a vector or a string would otherwise be a crash rather than a measurement.
struct VolListener {
    virtual ~VolListener() {}
    static void hit(int slot, const char* name, const void* a, const void* b, const void* c) {
        unsigned first = 0xFFFFu;
        if (a) first = *(const unsigned char*)a;
        std::fprintf(stderr, "[cinder-probe] btvollisten: slot %-2d %-28s a=%p b=%p c=%p  *a=%u\n",
                     slot, name, a, b, c, first);
        std::fflush(nullptr);
    }
    virtual void s2 (const void* a, const void* b, const void* c) { hit(2,  "OnNotifyAvSrcConnStatus", a,b,c); }
    virtual void s3 (const void* a, const void* b, const void* c) { hit(3,  "OnNotifyAvrcpConnStatus", a,b,c); }
    virtual void s4 (const void* a, const void* b, const void* c) { hit(4,  "OnNotifyConnectInformation", a,b,c); }
    virtual void s5 (const void* a, const void* b, const void* c) { hit(5,  "OnNotifyAvrcpGetPlayStatus", a,b,c); }
    virtual void s6 (const void* a, const void* b, const void* c) { hit(6,  "OnNotifyAvrcpGetMediaAttr", a,b,c); }
    virtual void s7 (const void* a, const void* b, const void* c) { hit(7,  "OnNotifyError", a,b,c); }
    virtual void s8 (const void* a, const void* b, const void* c) { hit(8,  "NotifyConfigiration", a,b,c); }
    virtual void s9 (const void* a, const void* b, const void* c) { hit(9,  "OnNotifySoundStatus", a,b,c); }
    virtual void s10(const void* a, const void* b, const void* c) { hit(10, "OnNotifyChangeVolume?", a,b,c); }
    virtual void s11(const void* a, const void* b, const void* c) { hit(11, "OnNotifyUpdateCapabilities", a,b,c); }
    // Spares: if the real vtable is longer than the strings suggested, a notification landing here
    // says so instead of running off the end of the object.
    virtual void s12(const void* a, const void* b, const void* c) { hit(12, "(beyond the string list)", a,b,c); }
    virtual void s13(const void* a, const void* b, const void* c) { hit(13, "(beyond the string list)", a,b,c); }
};
// Static storage: AddListener keeps a RAW pointer (see the BtCommon listener in cinder-home).
static VolListener g_vol_listener;

static int btvollisten_probe(int secs) {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    void* x = _ZN3pst8services33BtTransmitterServiceClientFactory14CreateInstanceEv();
    if (!x) { clog_("btvollisten: factory returned null"); g_pump_run = false; _exit(1); }
    // Slots 3-38 are the service methods (RE_findings, full client vtable), so Add/Remove follow at
    // 39/40 — the same "immediately after the last method" rule BtCommon and Nfc both obeyed.
    enum { VIDX_AddListener = 39, VIDX_RemoveListener = 40,
           VIDX_SetVolumeUp = 17, VIDX_SetVolumeDown = 16 };
    typedef int (*fnadd)(void*, void*, const std::string*);
    typedef int (*fnrem)(void*, unsigned);
    typedef int (*fn0)(void*);
    std::string key;                       // "" — a NotifyListeners filter key, not a label
    int rc = -1;
    wd_arm(10);
    try { rc = ((fnadd)vslot(x, VIDX_AddListener))(x, (void*)&g_vol_listener, &key); }
    catch (...) { clog_("btvollisten: AddListener threw"); }
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] btvollisten: AddListener rc=%d (%s)\n", rc,
                 rc == 0 ? "registered" : "FAILED — 1=bad arg, 4=no service");
    if (rc != 0) { g_pump_run = false; std::fflush(nullptr); _exit(1); }

    clog_("btvollisten: registered. Nudging the volume so the sink reports back — and change it on "
          "the HEADPHONES too; that is the case only a listener can see.");
    for (int i = 0; i < secs; i++) {
        if (i == 2 || i == 6) {
            wd_arm(8);
            try { ((fn0)vslot(x, VIDX_SetVolumeUp))(x); } catch (...) {}
            wd_disarm();
        }
        if (i == 4 || i == 8) {
            wd_arm(8);
            try { ((fn0)vslot(x, VIDX_SetVolumeDown))(x); } catch (...) {}
            wd_disarm();
        }
        sleep(1);
    }
    wd_arm(10);
    try { ((fnrem)vslot(x, VIDX_RemoveListener))(x, (unsigned)(uintptr_t)&g_vol_listener); }
    catch (...) {}
    wd_disarm();
    clog_("btvollisten: unregistered");
    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

// ── --btvolslot : find the REAL SetCurrentVolume slot ───────────────────────────────────────────
// Measured 2026-08-11: with the buds connected, absolute volume enabled and all three capability
// reads returning 1, calling slot 34 with a uint8_t makes BtTransmitterService log NOTHING — while
// slots 16/17 (SetVolumeUp/Down) reliably log either "Send absolute volute(%u)" or "Not support
// absolute volume". A slot that reaches no logging branch at all is a slot that is not the method
// we think it is.
//
// The client exports only its factory, so slot names cannot be read from the symbol table, and the
// `virtual ...` prototype strings that the original table was inferred from only exist for methods
// that log — IsAvrcpTgVolumeSupported, for one, is absent from them. So the map has to be measured.
//
// ONE SLOT PER INVOCATION, deliberately: a wrong signature here can corrupt memory (GetSocketName
// takes a base::string&, and handing it a uint8_t* would have it marshal from our stack), so a
// crash must cost one slot rather than the whole scan. The caller checks the service log after each.
static int btvolslot_probe(int slot, int value) {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    void* x = _ZN3pst8services33BtTransmitterServiceClientFactory14CreateInstanceEv();
    if (!x) { clog_("btvolslot: factory returned null"); g_pump_run = false; _exit(1); }
    unsigned char v = (unsigned char)(value > 127 ? 127 : (value < 0 ? 0 : value));
    typedef int (*fnu)(void*, const unsigned char*);
    std::fprintf(stderr, "[cinder-probe] btvolslot: calling slot %d with uint8_t %u\n", slot, v);
    std::fflush(nullptr);
    int rc = -1;
    wd_arm(8);
    try { rc = ((fnu)vslot(x, slot))(x, &v); } catch (...) { clog_("btvolslot: threw"); }
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] btvolslot: slot %d rc=%d (now grep the service log for "
                         "'volute' / 'absolute volume')\n", slot, rc);
    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

// ── --btvol : drive the sink's volume directly ──────────────────────────────────────────────────
// Reported 2026-08-11: with "Use Enhanced Mode" on and the log saying "sink takes ABSOLUTE volume",
// the rocker still does not change the CMF Buds' volume. That leaves two possibilities, and this
// mode separates them: either SetCurrentVolume(uint8_t) does nothing on this link (a service/ABI
// problem), or cinder-home is calling it with a value that never moves (a UI/level problem).
//
// Sweeps a few absolute levels with a pause between each, so the answer is audible. 0..127 is the
// AVRCP scale. Prints IsAvrcpTgVolumeSupported / GetControlAbsoluteVolume / IsSupportedAbsoluteVolume
// first, because SetCurrentVolume is a no-op unless the last two are BOTH true.
static int btvol_probe(int want) {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    void* x = _ZN3pst8services33BtTransmitterServiceClientFactory14CreateInstanceEv();
    if (!x) { clog_("btvol: BtTransmitterServiceClientFactory returned null"); g_pump_run = false; _exit(1); }
    enum { VIDX_SetVolumeDown = 16, VIDX_SetVolumeUp = 17,
           VIDX_IsAvrcpTgVolumeSupported = 30, VIDX_GetControlAbsoluteVolume = 32,
           VIDX_IsSupportedAbsoluteVolume = 33, VIDX_SetCurrentVolume = 34 };
    typedef int (*fn0)(void*);
    typedef int (*fnu)(void*, const unsigned char*);

    int tg = -1, ctrl = -1, sup = -1;
    wd_arm(10);
    try {
        tg   = ((fn0)vslot(x, VIDX_IsAvrcpTgVolumeSupported))(x);
        ctrl = ((fn0)vslot(x, VIDX_GetControlAbsoluteVolume))(x);
        sup  = ((fn0)vslot(x, VIDX_IsSupportedAbsoluteVolume))(x);
    } catch (...) { clog_("btvol: a capability read threw"); }
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] btvol: IsAvrcpTgVolumeSupported=%d GetControlAbsoluteVolume=%d "
                         "IsSupportedAbsoluteVolume=%d\n", tg, ctrl, sup);
    if (ctrl != 1 || sup != 1)
        clog_("btvol: SetCurrentVolume will be REFUSED by the service in this state — expect silence");

    if (want >= 0) {
        unsigned char v = (unsigned char)(want > 127 ? 127 : want);
        wd_arm(10);
        int rc = -1;
        try { rc = ((fnu)vslot(x, VIDX_SetCurrentVolume))(x, &v); } catch (...) { clog_("btvol: SetCurrentVolume threw"); }
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] btvol: SetCurrentVolume(%u) rc=%d — judge by EAR, not rc\n", v, rc);
    } else {
        // Sweep. Deliberately capped at 55/127 (~43%): this goes straight into earbuds someone is
        // wearing, and an unmistakable change does not require a painful one.
        const unsigned char steps[] = { 15, 35, 55, 25 };
        for (unsigned i = 0; i < sizeof steps / sizeof *steps; i++) {
            unsigned char v = steps[i];
            wd_arm(10);
            int rc = -1;
            try { rc = ((fnu)vslot(x, VIDX_SetCurrentVolume))(x, &v); } catch (...) {}
            wd_disarm();
            std::fprintf(stderr, "[cinder-probe] btvol: SetCurrentVolume(%u) rc=%d\n", v, rc);
            sleep(3);
        }
        // …then the RELATIVE path, for comparison. If these move the volume and the absolute ones
        // did not, the sink is ignoring absolute volume whatever its capability bits claim.
        for (int i = 0; i < 3; i++) {
            wd_arm(10);
            int rc = -1;
            try { rc = ((fn0)vslot(x, VIDX_SetVolumeUp))(x); } catch (...) {}
            wd_disarm();
            std::fprintf(stderr, "[cinder-probe] btvol: SetVolumeUp() rc=%d\n", rc);
            sleep(2);
        }
    }
    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

static int btwho_probe() {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    void* xmit = _ZN3pst8services33BtTransmitterServiceClientFactory14CreateInstanceEv();
    void* cmn  = _ZN3pst8services28BtCommonServiceClientFactory14CreateInstanceEv();
    enum { VIDX_GetBtStatus = 3, VIDX_GetAvSrcConnectionStatus = 3, VIDX_GetAvrcpConnectionStatus = 4,
           VIDX_GetConnectInformation = 5, VIDX_GetSoundStatus = 26,
           VIDX_IsAvrcpTgVolumeSupported = 30, VIDX_GetControlAbsoluteVolume = 32,
           VIDX_IsSupportedAbsoluteVolume = 33 };
    typedef int (*fn0)(void*);
    typedef int (*fn2)(void*, std::vector<unsigned char>*, std::string*);
    // virtual void GetSoundStatus(BtSoundCodec&, BtSoundFrequency&, BtSoundChannel&, bool&)
    // — four OUT params, every one a scalar (three enums + a bool), so there is no container to
    // get wrong here. That is the whole reason this is safe to call blind, unlike GetCapabilities
    // (which takes a pst::base::vector) or GetConnectInformation (which holds a std::string).
    // Declared `unsigned` rather than the real enum types: an enum's underlying type is int-sized
    // on this ABI, and we want to print whatever arrives rather than assume the enumerators.
    typedef void (*fn4)(void*, unsigned*, unsigned*, unsigned*, bool*);

    int st = -1, avsrc = -1, avrcp = -1, rc = -1;
    std::vector<unsigned char> addr;
    std::string name;
    // Sentinels: GetSoundStatus returns void, so an untouched buffer is the only way to tell
    // "the service wrote nothing" from "the service wrote 0" (0 is a legitimate codec value).
    unsigned codec = 0xDEADu, freq = 0xDEADu, chan = 0xDEADu;
    bool scmst = false;
    // The three reads behind Sony's "Use Enhanced Mode" checkbox (firmware message 230077, help
    // text 230079 "Select this check box if you cannot change the volume"). All three take no
    // arguments and return bool, so they are as safe to call blind as GetAvSrcConnectionStatus.
    int tgvol = -1, ctrlabs = -1, supabs = -1;
    wd_arm(12);
    try {
        if (cmn)  st    = ((fn0)vslot(cmn,  VIDX_GetBtStatus))(cmn);
        if (xmit) avsrc = ((fn0)vslot(xmit, VIDX_GetAvSrcConnectionStatus))(xmit);
        if (xmit) avrcp = ((fn0)vslot(xmit, VIDX_GetAvrcpConnectionStatus))(xmit);
        if (xmit) rc    = ((fn2)vslot(xmit, VIDX_GetConnectInformation))(xmit, &addr, &name);
        if (xmit) ((fn4)vslot(xmit, VIDX_GetSoundStatus))(xmit, &codec, &freq, &chan, &scmst);
        if (xmit) tgvol   = ((fn0)vslot(xmit, VIDX_IsAvrcpTgVolumeSupported))(xmit);
        if (xmit) ctrlabs = ((fn0)vslot(xmit, VIDX_GetControlAbsoluteVolume))(xmit);
        if (xmit) supabs  = ((fn0)vslot(xmit, VIDX_IsSupportedAbsoluteVolume))(xmit);
    } catch (...) { clog_("btwho: a read threw"); }
    wd_disarm();

    char mac[24];
    mac_str(addr, mac, sizeof mac);
    std::fprintf(stderr, "[cinder-probe] btwho: GetBtStatus=%d  AvSrc=%d  Avrcp=%d\n", st, avsrc, avrcp);
    std::fprintf(stderr, "[cinder-probe] btwho: GetConnectInformation rc=%d addr=%s name='%s'\n",
                 rc, mac, name.c_str());
    // The ADDRESS decides, not `rc`. Measured on the run that motivated this mode: rc=0 came back
    // together with a valid MAC and 'WH-1000XM4', so the stub's int is a transaction status (0 = OK)
    // and not the service method's bool. Judging by rc is what made the Bluetooth screen claim
    // nothing was connected while audio was playing.
    // THE NEGOTIATED CODEC. Everything the Bluetooth screen shows today is the user's PREFERENCE
    // (SetLdac/SetAptxHD/SetAptxClassic, applied before connecting) — but A2DP negotiates, so a
    // sink that cannot do LDAC silently lands on SBC while the UI still reads "LDAC". This is the
    // call that says what actually happened. Enumerators are printed raw and NOT interpreted:
    // the service's own log line is `codec:0x%02x channel:0x%02x frequency:0x%02x`, so the map
    // gets built from a run with a known headphone rather than from a guess.
    std::fprintf(stderr, "[cinder-probe] btwho: GetSoundStatus codec=0x%02x freq=0x%02x chan=0x%02x scmst=%d%s\n",
                 codec, freq, chan, (int)scmst,
                 codec == 0xDEADu ? "   <-- UNTOUCHED: the service wrote nothing" : "");
    // VOLUME PATH. `GetControlAbsoluteVolume` is the preference behind Sony's "Use Enhanced Mode";
    // `IsSupportedAbsoluteVolume` is what the SINK can do. libBtTransmitterService.so refuses to
    // transmit unless BOTH are true — it logs "Not control absolute volume mode" for a false
    // preference and "Not support absolute volume" for an incapable sink, and only then
    // "Send absolute volute(%u)". With the preference off, volume goes out as AVRCP
    // VOLUME_UP/VOLUME_DOWN key events instead, which is what makes sinks play their own beep.
    std::fprintf(stderr,
                 "[cinder-probe] btwho: IsAvrcpTgVolumeSupported=%d  GetControlAbsoluteVolume=%d "
                 "(Sony 'Use Enhanced Mode')  IsSupportedAbsoluteVolume=%d  -> volume goes out as %s\n",
                 tgvol, ctrlabs, supabs,
                 (ctrlabs == 1 && supabs == 1) ? "ABSOLUTE (SetCurrentVolume)"
                                               : "VOLUME_UP/DOWN key events (sink may beep)");
    if (!addr.empty())
        clog_("btwho: CONNECTED — and note rc above: a zero return with a filled address means rc is "
              "NOT a connected flag. Gate on the address.");
    else
        clog_("btwho: the service reports nothing connected (so the codec fields above are stale/unset)");

    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

// ── --btconnect [row] : does a `const pst::base::vector<uint8_t>&` IN-param marshal correctly? ───
//
// The Devices screen (Bluetooth ▸ paired list) rests on two calls that pass a BD address INTO the
// service — `BtTransmitterService::RequestConnection(const vector<uint8_t>&)` and
// `BtCommonService::DeleteLinkkey(const vector<uint8_t>&)`. Everything proven so far went the other
// way: the service filled containers we handed it. An in-param exercises the opposite direction
// (`TransactionParam::Set*` over our bytes), so it is worth one throwaway process rather than finding
// out inside the Home app.
//
// RequestConnection is the safe half of the pair to test with: it is reversible (disconnect, or just
// walk away) whereas DeleteLinkkey destroys a pairing the device cannot recreate — Cinder cannot scan
// yet. So this mode tests the connect and reasons about the delete by identical signature.
//
// If the radio was OFF, it is powered up for the attempt and **put back** afterwards: a probe should
// leave the device as it found it.
static int btconnect_probe(int row) {
    install_diagnostics();

    clog_("btconnect: Framework::GetReference() + StartForApplication …");
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    int sr = fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] btconnect: StartForApplication returned %d\n", sr);
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    void* xmit = _ZN3pst8services33BtTransmitterServiceClientFactory14CreateInstanceEv();
    void* cmn  = _ZN3pst8services28BtCommonServiceClientFactory14CreateInstanceEv();
    if (!xmit || !cmn) {
        std::fprintf(stderr, "[cinder-probe] btconnect: client build failed (xmit=%p cmn=%p)\n", xmit, cmn);
        _exit(1);
    }

    enum { VIDX_GetBtStatus = 3, VIDX_SetRfOnOff = 4, VIDX_GetConnectInformation = 5,
           VIDX_RequestConnection = 6, VIDX_GetPairedDeviceInfo = 20 };
    typedef int  (*fn0)(void*);
    typedef void (*fnb)(void*, const bool*);
    typedef int  (*fnv)(void*, std::vector<BtPairedDeviceInformation>*);
    typedef int  (*fna)(void*, const std::vector<unsigned char>*);
    typedef int  (*fn2)(void*, std::vector<unsigned char>*, std::string*);

    // The list first — the row index is only meaningful against it.
    std::vector<BtPairedDeviceInformation> devs;
    wd_arm(12);
    try { ((fnv)vslot(cmn, VIDX_GetPairedDeviceInfo))(cmn, &devs); }
    catch (...) { clog_("btconnect: GetPairedDeviceInfo threw"); }
    wd_disarm();
    for (size_t i = 0; i < devs.size(); i++) {
        char mac[24];
        mac_str(devs[i].addr, mac, sizeof mac);
        std::fprintf(stderr, "[cinder-probe] btconnect:   [%zu] %s  '%s'\n", i, mac, devs[i].name.c_str());
    }
    if (row < 0 || (size_t)row >= devs.size() || devs[(size_t)row].addr.size() != 6) {
        std::fprintf(stderr, "[cinder-probe] btconnect: row %d is not a usable device (%zu paired) — "
                     "pass a row from the list above\n", row, devs.size());
        _exit(1);
    }
    const std::vector<unsigned char> addr = devs[(size_t)row].addr;

    int st0 = -1;
    wd_arm(10);
    try { st0 = ((fn0)vslot(cmn, VIDX_GetBtStatus))(cmn); } catch (...) {}
    wd_disarm();
    bool we_powered = false;
    std::fprintf(stderr, "[cinder-probe] btconnect: GetBtStatus=%d (2 idle / 3 connected / 7 off)\n", st0);
    if (st0 != 2 && st0 != 3) {
        clog_("btconnect: radio is not up — powering it on for this test, and back off at the end");
        bool on = true;
        wd_arm(10);
        try { ((fnb)vslot(cmn, VIDX_SetRfOnOff))(cmn, &on); we_powered = true; } catch (...) {}
        wd_disarm();
        for (int i = 0; i < 20; i++) {
            usleep(200000);
            int st = -1;
            try { st = ((fn0)vslot(cmn, VIDX_GetBtStatus))(cmn); } catch (...) {}
            if (st == 2 || st == 3) { std::fprintf(stderr, "[cinder-probe] btconnect: radio up (status=%d)\n", st); break; }
        }
    }

    // The call under test.
    char mac[24];
    mac_str(addr, mac, sizeof mac);
    std::fprintf(stderr, "[cinder-probe] btconnect: RequestConnection(%s) …\n", mac);
    int rc = -1;
    wd_arm(15);
    try { rc = ((fna)vslot(xmit, VIDX_RequestConnection))(xmit, &addr); }
    catch (...) { clog_("btconnect: RequestConnection THREW — the in-param marshalling is wrong"); }
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] btconnect: RequestConnection rc=%d\n", rc);

    // Connection setup is asynchronous, so watch the status for a while. A2DP + AVRCP on this stack
    // takes a couple of seconds when the headphones are awake and nothing at all when they are not.
    //
    // The link is only ATTRIBUTABLE to our request if there wasn't one already: measured 2026-07-30,
    // `SetRfOnOff(true)` makes the stack reconnect the last device by itself, so the status was
    // already 3 before RequestConnection was called. The first version of this loop printed
    // "*** LINKED after 1s ***" off exactly that pre-existing link — a diagnostic that credits the
    // call under test with something it didn't do is worse than one that reports nothing.
    bool baseline_linked = false;
    {
        std::vector<unsigned char> live;
        std::string nm;
        try { ((fn2)vslot(xmit, VIDX_GetConnectInformation))(xmit, &live, &nm); } catch (...) {}
        baseline_linked = !live.empty();
    }
    bool linked = false;
    for (int i = 0; i < 15; i++) {
        usleep(1000000);
        int st = -1;
        std::vector<unsigned char> live;
        std::string nm;
        try {
            st = ((fn0)vslot(cmn, VIDX_GetBtStatus))(cmn);
            ((fn2)vslot(xmit, VIDX_GetConnectInformation))(xmit, &live, &nm);
        } catch (...) {}
        if (!live.empty() && live == addr) {
            char who[24];
            mac_str(live, who, sizeof who);
            std::fprintf(stderr, "[cinder-probe] btconnect: %s after %ds — %s '%s' status=%d\n",
                         baseline_linked ? "linked, but it ALREADY WAS before the request"
                                         : "*** LINKED by our request ***",
                         i + 1, who, nm.c_str(), st);
            linked = true;
            break;
        }
        if (i % 3 == 0) std::fprintf(stderr, "[cinder-probe] btconnect:   t+%ds status=%d\n", i + 1, st);
    }
    if (!linked)
        clog_("btconnect: no link to THAT address inside 15 s. rc above still answers the ABI "
              "question — a rejected or ignored request is a headphones/radio outcome, whereas a "
              "THROW would have been our bug. Check logcat for 'RequestConnection [mac]': the "
              "service echoes the address it received, which is the real proof the bytes arrived.");

    // Leave the radio as we found it. Deliberately does NOT disconnect a link the user already had.
    if (we_powered) {
        bool off = false;
        wd_arm(10);
        try { ((fnb)vslot(cmn, VIDX_SetRfOnOff))(cmn, &off); } catch (...) {}
        wd_disarm();
        clog_("btconnect: radio powered back OFF (it was off when this started)");
    }

    g_pump_run = false;
    std::fprintf(stderr, "[cinder-probe] btconnect: done (%u pump ticks)\n", g_pump_ticks);
    std::fflush(nullptr);
    _exit(0);
}

// ── --eq : what units does SetEq10BandValue actually take, and is the 10-band engine SELECTED? ──
//
// Two questions RE could not close, both of which decide whether the EQ screen does what it says:
//
//  1. UNITS. `libEffectCtrlDmp` exports BOTH `GetEq10BandValue` and `GetEq10BandValuedB`. Two
//     getters means the raw value is NOT dB, so the -10..+10 the UI sends may not be the range
//     Sony expects — and a value outside it would clip silently. Setting a known ladder and
//     reading BOTH getters back answers it outright.
//     The dB getter's RETURN TYPE is unknown too (mangling does not encode it), and this is armhf,
//     so a float comes back in s0 and an int in r0 — reading the wrong one gives a plausible
//     number from the wrong register. So the same address is called through both prototypes and
//     both results printed; exactly one of them will make sense.
//
//  2. SELECTION. `SetSelectUsingEq(EqType)` / `GetSelectUsingEq()` exist and Cinder calls neither.
//     `SetEq10Band(true)` may only arm the 10-band, not make the DSP USE it. Reading what stock
//     leaves the selector at says whether that call is missing.
//
// Read-mostly: it restores every band it touched, and it does the easel lifecycle not at all, so
// it cannot affect boot.
namespace pst { namespace services { namespace sound { class EffectCtrlDmp; } } }
extern "C" {
void _ZN3pst8services5sound13EffectCtrlDmpC1Ev(void*);
void _ZN3pst8services5sound13EffectCtrlDmp11SetEq10BandEb(void*, bool);
void _ZN3pst8services5sound13EffectCtrlDmp16SetEq10BandValueENS1_8Eq10BandEi(void*, int, int);
int  _ZN3pst8services5sound13EffectCtrlDmp16GetEq10BandValueENS1_8Eq10BandE(void*, int);
void _ZN3pst8services5sound13EffectCtrlDmp18GetEq10BandValuedBENS1_8Eq10BandE(void);
int  _ZN3pst8services5sound13EffectCtrlDmp16GetSelectUsingEqEv(void*);
int  _ZN3pst8services5sound13EffectCtrlDmp12IsEq10BandOnEv(void*);
}

// The effect shim (cinder-audio/src/effect_shim.cpp) — the probe links the same objects
// cinder-home does, so these resolve without pulling in the whole header.
extern "C" {
int cinder_effects_is_vpt_on(void);
int cinder_effects_is_dsee_hx_on(void);
int cinder_effects_is_dsee_ai_on(void);
int cinder_effects_is_clearaudio_on(void);
int cinder_effects_is_bt_effect_on(void);
int cinder_effects_is_source_direct_on(void);
int cinder_effects_is_normalizer_on(void);
int cinder_effects_is_dc_phase_on(void);
int cinder_effects_is_vinylizer_on(void);
int cinder_effects_is_eq10_on(void);
int cinder_effects_is_eq6_on(void);
int cinder_effects_is_tone_on(void);
int cinder_effects_is_clear_phase_hp_on(void);
int cinder_effects_get_select_using_eq(void);
int cinder_effects_set_select_using_eq(int t);
int cinder_effects_get_eq_band(int i);
int cinder_effects_set_eq_band(int i, int gain);
int cinder_effects_get_vinylizer_type(void);
int cinder_effects_set_vpt(int on);
int cinder_effects_set_vpt_mode(int mode);
int cinder_effects_get_vpt_mode(void);
int cinder_effects_set_dc_phase_type(int type);
int cinder_effects_get_dc_phase_type(void);
float cinder_effects_get_eq_band_db(int i);
int cinder_effects_set_tone_control(int on);
int cinder_effects_set_eq6(int on);
int cinder_effects_set_tone_value(int band, int gain);
int cinder_effects_get_tone_value(int band);
float cinder_effects_get_tone_value_db(int band);
int cinder_effects_set_tone_freq(int band, int f);
int cinder_effects_get_tone_freq(int band);
int cinder_effects_set_eq6_band(int b, int gain);
int cinder_effects_set_eq6(int on2);
int cinder_effects_set_dsee_hx(int on3);
int cinder_effects_save_user_preset(int no);
int cinder_effects_load_user_preset(int no);
int cinder_effects_set_vinylizer(int on4);
int cinder_effects_set_vinylizer_type(int t2);
int cinder_effects_set_dc_phase(int on5);
int cinder_effects_set_dynamic_normalizer(int on6);
int cinder_effects_get_eq6_band(int b);
float cinder_effects_get_eq6_band_db(int b);
int cinder_effects_set_eq6_preset(int p);
int cinder_effects_get_eq6_preset(void);
}

// --eqsel : find which EqType actually puts the 10-band EQ IN THE PATH.
//
// `cinder_effects_set_eq` switches the 10-band on and writes gains, but has never called
// SetSelectUsingEq — and the device reports EQ10, EQ6 and ToneControl all "on" simultaneously with
// SelectUsingEq=1. Sony's own manual says the Equalizer and the Tone Control are ALTERNATIVES whose
// settings are saved separately, so something selects between them, and if it is not sitting on the
// 10-band then Cinder's EQ has been writing to a control that is not in the signal path. Exactly
// the shape of the ClearAudio+ override that made VPT inaudible for a fortnight.
//
// Loads a deliberately absurd curve (+10 dB bass / -10 dB treble; raw units are HALF-decibels, so
// +-20) and holds it under the chosen selector value. If the EQ is in the path this is unmissable.
// The user's own curve is read first and put back on the way out.
static int eqsel_probe(int only, int hold_s) {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    char m[224];
    const int sel0 = cinder_effects_get_select_using_eq();
    int saved[10];
    for (int i = 0; i < 10; i++) saved[i] = cinder_effects_get_eq_band(i);
    std::snprintf(m, sizeof m,
                  "eqsel: on entry SelectUsingEq=%d  curve=%d,%d,%d,%d,%d,%d,%d,%d,%d,%d",
                  sel0, saved[0], saved[1], saved[2], saved[3], saved[4],
                  saved[5], saved[6], saved[7], saved[8], saved[9]);
    clog_(m);
    std::snprintf(m, sizeof m, "eqsel: gates — ClearAudio+=%d SourceDirect=%d (either one hides this test)",
                  cinder_effects_is_clearaudio_on(), cinder_effects_is_source_direct_on());
    clog_(m);

    if (only < 0) {
        clog_("eqsel: read-only. Re-run as --eqsel <n> [secs] to load a wild curve and listen.");
        g_pump_run = false;
        return 0;
    }

    // +10 dB on the bottom three, -10 dB on the top three. Raw is half-decibels.
    static const int wild[10] = { 20, 20, 20, 0, 0, 0, 0, -20, -20, -20 };
    for (int i = 0; i < 10; i++) cinder_effects_set_eq_band(i, wild[i]);
    cinder_effects_set_select_using_eq(only);
    std::snprintf(m, sizeof m,
                  "eqsel: SelectUsingEq(%d) -> reads back %d; wild curve loaded, HOLDING %ds — "
                  "bass should be huge and treble gone if the 10-band is in the path",
                  only, cinder_effects_get_select_using_eq(), hold_s);
    clog_(m);
    for (int i = 0; i < hold_s; i++) sleep(1);

    for (int i = 0; i < 10; i++) cinder_effects_set_eq_band(i, saved[i]);
    cinder_effects_set_select_using_eq(sel0);
    std::snprintf(m, sizeof m, "eqsel: restored SelectUsingEq=%d and your curve",
                  cinder_effects_get_select_using_eq());
    clog_(m);
    g_pump_run = false;
    return 0;
}

// --fx : dump the ENTIRE effect chain's state.
//
// Exists because "I set VPT and cannot hear it" has at least three explanations and the setter's
// return value distinguishes none of them:
//   * ClearAudioPlus is ON — Sony's one-touch tuning OVERRIDES the manual EQ and DSP outright.
//   * SourceDirect is ON — bypasses the chain entirely for the purest path.
//   * BtAudioSoundEffect is OFF — the chain runs but never reaches a Bluetooth sink, which is the
//     whole of project goal #7 and the reason cinder-home asserts it unconditionally.
// Any of those makes every other setting inaudible while still reading back exactly as written.
static int fx_probe() {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    char m[256];
    std::snprintf(m, sizeof m,
                  "fx: GATES  ClearAudio+=%d  SourceDirect=%d  BtAudioSoundEffect=%d",
                  cinder_effects_is_clearaudio_on(), cinder_effects_is_source_direct_on(),
                  cinder_effects_is_bt_effect_on());
    clog_(m);
    clog_("fx:   ^ ClearAudio+ overrides EQ+DSP; SourceDirect bypasses everything;");
    clog_("fx:     BtAudioSoundEffect=0 means none of it reaches Bluetooth.");
    std::snprintf(m, sizeof m,
                  "fx: VPT=%d mode=%d   DcPhase=%d type=%d   Vinylizer=%d type=%d",
                  cinder_effects_is_vpt_on(), cinder_effects_get_vpt_mode(),
                  cinder_effects_is_dc_phase_on(), cinder_effects_get_dc_phase_type(),
                  cinder_effects_is_vinylizer_on(), cinder_effects_get_vinylizer_type());
    clog_(m);
    std::snprintf(m, sizeof m,
                  "fx: DSEE HX=%d  DSEE AI=%d  Normalizer=%d  ToneControl=%d  ClearPhaseHP=%d",
                  cinder_effects_is_dsee_hx_on(), cinder_effects_is_dsee_ai_on(),
                  cinder_effects_is_normalizer_on(), cinder_effects_is_tone_on(),
                  cinder_effects_is_clear_phase_hp_on());
    clog_(m);
    std::snprintf(m, sizeof m, "fx: EQ10=%d  EQ6=%d  SelectUsingEq=%d",
                  cinder_effects_is_eq10_on(), cinder_effects_is_eq6_on(),
                  cinder_effects_get_select_using_eq());
    clog_(m);
    g_pump_run = false;
    // _exit, not return — see --tone: the pump thread is still inside libpstcore, and unwinding
    // through static destructors while it runs faults in the BT/effect libs. Returning here is
    // what made a clean run report exit 42 with a PC=0 backtrace.
    std::fflush(nullptr);
    _exit(0);
}

// --tone : settle the Tone Control and 6-band EQ units, ranges and enumerators.
//
// WHY THIS IS MEASURABLE WITHOUT EARS. On this device a read-back does NOT bound an enum — the
// service stores whatever int it is handed (proven for VptMode and for SelectUsingEq, where 0..7
// all echoed). But there are two harder signals available here:
//
//   * a dB getter whose conversion happens INSIDE the service, against its own table, so it
//     reports the scale AND the clamp rather than the value it was handed; and
//   * `SetEq6BandPreset`, which is the rare setter with a VISIBLE SIDE EFFECT — if a preset is
//     real, the six band values move. A preset that is merely stored leaves them alone.
//
// The 10-band's dB getter is the CONTROL: its scale is already measured (raw = half-decibels,
// +-20 = +-10 dB), so if it reads sensibly in the same run then a flat 0 from one of the others
// is a fact about that control, not about the probe.
//
// PASS 1 (2026-08-17) established, with tone control OFF and eq6 preset 0:
//   * ToneType ordinals ARE 0/1/2 — the service logs `eqtone,type=N`, N in {0,1,2}, i.e. the
//     catalogue order BASS/MIDDLE/TREBLE is the enum order.
//   * Tone raw echoes -30..+30 unclamped and the dB twin reads 0 throughout.
//   * Eq6 band writes DO NOT STICK — set +-30, read back 0, every band.
//   * Eq6 presets 1..7 each load a DIFFERENT six-value curve. Preset 0 is flat.
// Pass 2 (this code) asks the two questions those answers raise: does the dB twin come alive when
// the effect is switched ON, and do band writes stick once a preset is selected?
//
// Everything written here is read first and put back on the way out.

// Ladder deliberately overshoots on both sides: the interesting reading is where dB STOPS
// following the raw value, because that is the clamp.
static const int kToneLadder[] = { -30, -24, -20, -12, -6, 0, 6, 12, 20, 24, 30 };
static const int kToneLadderN = (int)(sizeof kToneLadder / sizeof kToneLadder[0]);

// Sweep one band of one control, printing raw and dB side by side.
static void tone_sweep(const char* tag, int band,
                       int (*set)(int, int), int (*getraw)(int), float (*getdb)(int)) {
    char raw[160] = {0}, db[160] = {0}, m[256];
    int rl = 0, dl = 0;
    for (int i = 0; i < kToneLadderN; i++) {
        set(band, kToneLadder[i]);
        rl += std::snprintf(raw + rl, sizeof raw - rl, "%d ", getraw(band));
        dl += std::snprintf(db + dl, sizeof db - dl, "%.1f ", (double)getdb(band));
        if (rl >= (int)sizeof raw || dl >= (int)sizeof db) break;
    }
    std::snprintf(m, sizeof m, "tone: %-10s raw %s", tag, raw);
    clog_(m);
    std::snprintf(m, sizeof m, "tone: %-10s dB  %s", tag, db);
    clog_(m);
}

static int tone_probe() {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    char m[224];
    static const char* kBand[3] = { "BASS", "MID", "TREB" };

    // ── entry state, so every write below can be put back ────────────────────────────────────
    const int tone_on0 = cinder_effects_is_tone_on();
    const int eq6_on0  = cinder_effects_is_eq6_on();
    const int preset0  = cinder_effects_get_eq6_preset();
    int tsaved[3], fsaved[3], esaved[6], qsaved[10];
    for (int b = 0; b < 3; b++)  tsaved[b] = cinder_effects_get_tone_value(b);
    for (int b = 0; b < 3; b++)  fsaved[b] = cinder_effects_get_tone_freq(b);
    for (int b = 0; b < 6; b++)  esaved[b] = cinder_effects_get_eq6_band(b);
    for (int b = 0; b < 10; b++) qsaved[b] = cinder_effects_get_eq_band(b);
    std::snprintf(m, sizeof m, "tone: entry ToneOn=%d Eq6On=%d preset=%d tone=%d,%d,%d freq=%d,%d,%d",
                  tone_on0, eq6_on0, preset0, tsaved[0], tsaved[1], tsaved[2],
                  fsaved[0], fsaved[1], fsaved[2]);
    clog_(m);
    clog_("tone: ladder = -30 -24 -20 -12 -6 0 6 12 20 24 30");

    // ── A. the control: the 10-band, whose scale is already known ────────────────────────────
    clog_("tone: === A. 10-band EQ (CONTROL — raw is half-decibels, +-20 = +-10 dB) ===");
    tone_sweep("eq10[0]", 0, cinder_effects_set_eq_band,
               cinder_effects_get_eq_band, cinder_effects_get_eq_band_db);
    cinder_effects_set_eq_band(0, qsaved[0]);

    // ── B. Tone Control, with the effect switched ON ─────────────────────────────────────────
    // Pass 1 read a flat 0 dB with the effect off. If that was the reason, this comes alive.
    clog_("tone: === B. Tone Control, effect ON ===");
    cinder_effects_set_tone_control(1);
    for (int b = 0; b < 3; b++) {
        tone_sweep(kBand[b], b, cinder_effects_set_tone_value,
                   cinder_effects_get_tone_value, cinder_effects_get_tone_value_db);
        cinder_effects_set_tone_value(b, tsaved[b]);
    }
    cinder_effects_set_tone_control(tone_on0);

    // ── C. how far up does the preset enum go? ───────────────────────────────────────────────
    // The catalogue lists seven names (Bright, Excited, Mellow, Relaxed, Vocal, Custom 1,
    // Custom 2) and pass 1 saw eight distinct behaviours, 0..7, with 0 flat. Push past the end:
    // an out-of-range preset should stop changing the curve.
    clog_("tone: === C. eq6 preset ordinals (a REAL preset moves the six band values) ===");
    for (int p = 0; p < 16; p++) {
        cinder_effects_set_eq6_preset(p);
        std::snprintf(m, sizeof m, "tone: preset %2d -> reads %2d  bands %d,%d,%d,%d,%d,%d",
                      p, cinder_effects_get_eq6_preset(),
                      cinder_effects_get_eq6_band(0), cinder_effects_get_eq6_band(1),
                      cinder_effects_get_eq6_band(2), cinder_effects_get_eq6_band(3),
                      cinder_effects_get_eq6_band(4), cinder_effects_get_eq6_band(5));
        clog_(m);
    }

    // ── D. do 6-band writes stick, and under which preset? ───────────────────────────────────
    // Pass 1 wrote +-30 to every band under preset 0 and read back 0 every time. The obvious
    // explanation is that the band values belong to the SELECTED preset and only the two Custom
    // slots are writable — which is exactly how Sony's own UI behaves. Test the flat slot, and
    // the last two, which are the Custom candidates.
    clog_("tone: === D. eq6 band writes under different presets ===");
    static const int kTryPresets[3] = { 0, 6, 7 };
    for (int i = 0; i < 3; i++) {
        cinder_effects_set_eq6_preset(kTryPresets[i]);
        char tag[24];
        std::snprintf(tag, sizeof tag, "eq6[0]@p%d", kTryPresets[i]);
        tone_sweep(tag, 0, cinder_effects_set_eq6_band,
                   cinder_effects_get_eq6_band, cinder_effects_get_eq6_band_db);
    }

    // ── restore ──────────────────────────────────────────────────────────────────────────────
    cinder_effects_set_eq6_preset(preset0);
    for (int b = 0; b < 6; b++)  cinder_effects_set_eq6_band(b, esaved[b]);
    for (int b = 0; b < 10; b++) cinder_effects_set_eq_band(b, qsaved[b]);
    for (int b = 0; b < 3; b++) {
        cinder_effects_set_tone_value(b, tsaved[b]);
        cinder_effects_set_tone_freq(b, fsaved[b]);
    }
    cinder_effects_set_tone_control(tone_on0);
    cinder_effects_set_eq6(eq6_on0);
    std::snprintf(m, sizeof m, "tone: restored ToneOn=%d Eq6On=%d preset=%d tone=%d,%d,%d eq10[0]=%d",
                  cinder_effects_is_tone_on(), cinder_effects_is_eq6_on(),
                  cinder_effects_get_eq6_preset(),
                  cinder_effects_get_tone_value(0), cinder_effects_get_tone_value(1),
                  cinder_effects_get_tone_value(2), cinder_effects_get_eq_band(0));
    clog_(m);
    g_pump_run = false;
    // _exit, not return: the pump thread is still inside libpstcore, and unwinding through static
    // destructors while it runs faults in the BT/effect libs. Same reason --eq does it. Returning
    // here is what made this probe (and --fx) exit 42 with a PC=0 backtrace after a clean run.
    std::fflush(nullptr);
    _exit(0);
}

// --inpath <t> : which EqType value puts WHICH tone system in the signal path.
//
// The breakthrough that makes this answerable without ears is in the service's own log, not in
// any getter: `FilterChain::ExecEffectParam` is followed by `<Effect>::UpdateProcCond(bool,bool)
// ... isproc is N`, and **isproc is the service saying whether that effect is actually
// processing**. A setter's return value cannot distinguish "stored" from "in the path"; isproc
// can. So: switch all three tone systems ON, give them non-flat values, select one EqType, poke
// each system once, and read which one reports isproc 1.
//
// Run it once per candidate with the log cleared in between:
//   for t in 0 1 2 3 4; do adb shell logcat -c; cinder-probe --inpath $t;
//       adb shell logcat -d | grep -oE '(Eq10band|Eq6band|EqTone)::UpdateProcCond.*isproc is [01]'; done
//
// MEASURED 2026-08-17 with only the 10-band on: EqType **2** is the one under which Eq10band
// reports `isproc is 1`. 0, 1 and 3 never do — and the device was sitting on 1, which means the
// EQ Cinder has been writing since June was stored and never in the path.
static int inpath_probe(int t) {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    char m[224];
    const int sel0 = cinder_effects_get_select_using_eq();
    const int eq10_0 = cinder_effects_is_eq10_on();
    const int eq6_0  = cinder_effects_is_eq6_on();
    const int tone_0 = cinder_effects_is_tone_on();
    const int pre0   = cinder_effects_get_eq6_preset();
    int q0 = cinder_effects_get_eq_band(0);
    int t0 = cinder_effects_get_tone_value(0);

    // All three ON and non-flat, so "not processing" can only mean "not selected".
    cinder_effects_set_eq_band(0, 12);      // +6 dB, inside the +-20 raw range
    cinder_effects_set_tone_control(1);
    cinder_effects_set_tone_value(0, 12);
    cinder_effects_set_eq6(1);
    cinder_effects_set_eq6_preset(4);

    cinder_effects_set_select_using_eq(t);

    // Poke each system so each emits one UpdateProcCond under THIS selector.
    for (int i = 0; i < 3; i++) {
        cinder_effects_set_eq_band(0, 12 - i);
        cinder_effects_set_tone_value(0, 12 - i);
        cinder_effects_set_eq6_preset(4);
        usleep(100000);
    }
    std::snprintf(m, sizeof m, "inpath: selector %d applied (reads %d); poked eq10/eq6/tone",
                  t, cinder_effects_get_select_using_eq());
    clog_(m);
    sleep(1);

    // Restore. Selector LAST, so nothing above is re-evaluated under the old value.
    cinder_effects_set_eq6_preset(pre0);
    cinder_effects_set_eq6(eq6_0);
    cinder_effects_set_tone_value(0, t0);
    cinder_effects_set_tone_control(tone_0);
    cinder_effects_set_eq_band(0, q0);
    if (!eq10_0) { /* leave as found — the 10-band is switched on by set_eq, not here */ }
    cinder_effects_set_select_using_eq(sel0);
    std::snprintf(m, sizeof m, "inpath: restored selector=%d eq10[0]=%d tone[0]=%d preset=%d",
                  cinder_effects_get_select_using_eq(), cinder_effects_get_eq_band(0),
                  cinder_effects_get_tone_value(0), cinder_effects_get_eq6_preset());
    clog_(m);
    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

// --fxtime : how expensive IS an effect call?
//
// The Sound screen re-applies the whole chain on every change, and the band fields became
// draggable on 2026-08-17 — so a drag was emitting ~25 IPC round-trips per motion event and the
// audio stuttered. The shell now caches and writes only what moved, but "25 calls is too many"
// was an inference, not a measurement. This measures it: median and worst-case microseconds for
// each call, so the budget is a number rather than a hunch.
//
// Reads are separated from writes because they are different animals — a getter is a round-trip to
// the service, a setter is a round-trip PLUS whatever the DSP does about it downstream.
static long long usec_now() {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (long long)ts.tv_sec * 1000000LL + ts.tv_nsec / 1000;
}

static void fxtime_row(const char* name, int reps, void (*call)(int), int arg) {
    long long worst = 0, total = 0;
    long long samples[64];
    if (reps > 64) reps = 64;
    for (int i = 0; i < reps; i++) {
        long long t0 = usec_now();
        call(arg);
        long long d = usec_now() - t0;
        samples[i] = d;
        total += d;
        if (d > worst) worst = d;
    }
    // Median by insertion sort — n is tiny and this keeps the probe dependency-free.
    for (int i = 1; i < reps; i++) {
        long long v = samples[i];
        int j = i - 1;
        while (j >= 0 && samples[j] > v) { samples[j + 1] = samples[j]; j--; }
        samples[j + 1] = v;
    }
    char m[224];
    std::snprintf(m, sizeof m, "fxtime: %-22s median %6lld us   mean %6lld us   worst %6lld us",
                  name, samples[reps / 2], total / (reps ? reps : 1), worst);
    clog_(m);
}

static void c_eqband(int v)   { cinder_effects_set_eq_band(0, v); }
static void c_tone(int v)     { cinder_effects_set_tone_value(0, v); }
static void c_vptmode(int v)  { cinder_effects_set_vpt_mode(v); }
static void c_dsee(int v)     { cinder_effects_set_dsee_hx(v); }
static void c_geteq(int v)    { (void)v; (void)cinder_effects_get_eq_band(0); }
static void c_isvpt(int v)    { (void)v; (void)cinder_effects_is_vpt_on(); }

static int fxtime_probe() {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    const int q0 = cinder_effects_get_eq_band(0);
    const int t0 = cinder_effects_get_tone_value(0);
    const int vm0 = cinder_effects_get_vpt_mode();
    const int dsee0 = cinder_effects_is_dsee_hx_on();

    clog_("fxtime: 32 reps each. A UI frame is 16000 us; a drag emits one apply per frame.");
    fxtime_row("get_eq_band", 32, c_geteq, 0);
    fxtime_row("is_vpt_on", 32, c_isvpt, 0);
    fxtime_row("set_eq_band (same val)", 32, c_eqband, q0);
    fxtime_row("set_eq_band (alternating)", 32, c_eqband, 4);
    fxtime_row("set_tone_value", 32, c_tone, 4);
    fxtime_row("set_vpt_mode", 32, c_vptmode, vm0);
    fxtime_row("set_dsee_hx", 32, c_dsee, dsee0);

    cinder_effects_set_eq_band(0, q0);
    cinder_effects_set_tone_value(0, t0);
    cinder_effects_set_vpt_mode(vm0);
    cinder_effects_set_dsee_hx(dsee0);
    char m[224];
    std::snprintf(m, sizeof m, "fxtime: restored eq10[0]=%d tone[0]=%d vptmode=%d dsee=%d",
                  cinder_effects_get_eq_band(0), cinder_effects_get_tone_value(0),
                  cinder_effects_get_vpt_mode(), cinder_effects_is_dsee_hx_on());
    clog_(m);
    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

// --eq6custom : is the 6-band EQ editable, and which presets are the Custom slots?
//
// The service told us the rule itself on 2026-08-17:
//   EffectCtrlDmp.cc:534  !!! cannot set value except for UserCustom preset
// so band writes are rejected under a NAMED preset and accepted under a Custom one. The same run
// showed `!!! unknown preset. use fallback` for 11..15 but NOT for 9 and 10 — so the enum is
// 0..10, eleven slots, and 9/10 are the two Customs (both flat, because nothing has ever written
// them). This confirms it by writing and reading back under every valid preset.
static int eq6custom_probe() {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    char m[224];
    const int pre0 = cinder_effects_get_eq6_preset();
    const int on0 = cinder_effects_is_eq6_on();
    std::snprintf(m, sizeof m, "eq6: entry preset=%d Eq6On=%d", pre0, on0);
    clog_(m);
    clog_("eq6: writing 6,-6 to band0/band1 under each preset; a slot that KEEPS them is a Custom.");

    for (int p = 0; p <= 11; p++) {
        cinder_effects_set_eq6_preset(p);
        const int before0 = cinder_effects_get_eq6_band(0);
        cinder_effects_set_eq6_band(0, 6);
        cinder_effects_set_eq6_band(1, -6);
        const int a0 = cinder_effects_get_eq6_band(0);
        const int a1 = cinder_effects_get_eq6_band(1);
        std::snprintf(m, sizeof m,
                      "eq6: preset %2d  band0 %3d -> wrote 6 -> reads %3d   band1 reads %3d   %s",
                      p, before0, a0, a1,
                      (a0 == 6 && a1 == -6) ? "*** WRITABLE (UserCustom) ***" : "rejected");
        clog_(m);
    }

    // Put the user's world back: reselect the entry preset, which reloads its own curve.
    cinder_effects_set_eq6_preset(pre0);
    std::snprintf(m, sizeof m, "eq6: restored preset=%d bands %d,%d,%d,%d,%d,%d",
                  cinder_effects_get_eq6_preset(),
                  cinder_effects_get_eq6_band(0), cinder_effects_get_eq6_band(1),
                  cinder_effects_get_eq6_band(2), cinder_effects_get_eq6_band(3),
                  cinder_effects_get_eq6_band(4), cinder_effects_get_eq6_band(5));
    clog_(m);
    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

// --userpreset [--write] : settle UserPresetNo, and find out what Sony's saved setups hold.
//
// Cinder's A/B is its own thing — two `SoundSetup`s in Cinder's settings file. Sony has its own
// two-or-three saved setups ("Saved Sound Settings 1/2/3" in the catalogue) behind
// `SaveUserPreset(UserPresetNo)` / `LoadUserPreset(UserPresetNo)`, and backing A/B onto those would
// make the setups survive being edited from the stock UI. The enum's ordinals were never recovered.
//
// READ-ONLY BY DEFAULT, deliberately. `SaveUserPreset` would OVERWRITE whatever the user has in
// Sony's slots, and that is their data — however unlikely it is to matter on a device that boots
// Cinder as Home. So the default pass only LOADS: a load overwrites the LIVE chain, which Cinder
// re-asserts on its next apply anyway, and the chain is snapshotted and put back here regardless.
// What comes back after a load IS the slot's content, so this reads the store without writing it.
//
// Pass --write to additionally prove the round trip (save a marker, disturb it, load it back).
// That one DOES overwrite the slot it tests.
struct FxSnap {
    int eq[10];
    int dsee, vpt, vptmode, dc, dctype, norm, vinyl, vinyltype, tone, tv[3], eq6preset, sel;
};

static void fx_snap(FxSnap& s) {
    for (int i = 0; i < 10; i++) s.eq[i] = cinder_effects_get_eq_band(i);
    s.dsee = cinder_effects_is_dsee_hx_on();
    s.vpt = cinder_effects_is_vpt_on();
    s.vptmode = cinder_effects_get_vpt_mode();
    s.dc = cinder_effects_is_dc_phase_on();
    s.dctype = cinder_effects_get_dc_phase_type();
    s.norm = cinder_effects_is_normalizer_on();
    s.vinyl = cinder_effects_is_vinylizer_on();
    s.vinyltype = cinder_effects_get_vinylizer_type();
    s.tone = cinder_effects_is_tone_on();
    for (int i = 0; i < 3; i++) s.tv[i] = cinder_effects_get_tone_value(i);
    s.eq6preset = cinder_effects_get_eq6_preset();
    s.sel = cinder_effects_get_select_using_eq();
}

static void fx_restore(const FxSnap& s) {
    for (int i = 0; i < 10; i++) cinder_effects_set_eq_band(i, s.eq[i]);
    cinder_effects_set_dsee_hx(s.dsee);
    cinder_effects_set_vpt(s.vpt);
    cinder_effects_set_vpt_mode(s.vptmode);
    cinder_effects_set_dc_phase(s.dc);
    cinder_effects_set_dc_phase_type(s.dctype);
    cinder_effects_set_dynamic_normalizer(s.norm);
    cinder_effects_set_vinylizer(s.vinyl);
    cinder_effects_set_vinylizer_type(s.vinyltype);
    cinder_effects_set_tone_control(s.tone);
    for (int i = 0; i < 3; i++) cinder_effects_set_tone_value(i, s.tv[i]);
    cinder_effects_set_eq6_preset(s.eq6preset);
    cinder_effects_set_select_using_eq(s.sel);
}

static bool fx_same(const FxSnap& a, const FxSnap& b) {
    for (int i = 0; i < 10; i++) if (a.eq[i] != b.eq[i]) return false;
    for (int i = 0; i < 3; i++) if (a.tv[i] != b.tv[i]) return false;
    return a.dsee == b.dsee && a.vpt == b.vpt && a.vptmode == b.vptmode && a.dc == b.dc
        && a.dctype == b.dctype && a.norm == b.norm && a.vinyl == b.vinyl
        && a.vinyltype == b.vinyltype && a.tone == b.tone && a.eq6preset == b.eq6preset
        && a.sel == b.sel;
}

static void fx_log(const char* tag, const FxSnap& s) {
    char m[224];
    std::snprintf(m, sizeof m,
                  "userpreset: %-14s eq %d,%d,%d,%d,%d,%d,%d,%d,%d,%d", tag,
                  s.eq[0], s.eq[1], s.eq[2], s.eq[3], s.eq[4], s.eq[5], s.eq[6], s.eq[7],
                  s.eq[8], s.eq[9]);
    clog_(m);
    std::snprintf(m, sizeof m,
                  "userpreset: %-14s dsee=%d vpt=%d/%d dc=%d/%d norm=%d vinyl=%d/%d tone=%d "
                  "tv=%d,%d,%d eq6p=%d sel=%d", tag,
                  s.dsee, s.vpt, s.vptmode, s.dc, s.dctype, s.norm, s.vinyl, s.vinyltype,
                  s.tone, s.tv[0], s.tv[1], s.tv[2], s.eq6preset, s.sel);
    clog_(m);
}

static int userpreset_probe(bool do_write) {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    char m[224];
    FxSnap live;
    fx_snap(live);
    fx_log("LIVE (entry)", live);

    // Pass 1, read-only: what does each slot hold? A load that changes nothing is either an empty
    // slot, an out-of-range ordinal, or a slot that happens to match the live chain — the log says
    // which by showing the content.
    for (int n = 0; n < 5; n++) {
        cinder_effects_load_user_preset(n);
        FxSnap got;
        fx_snap(got);
        std::snprintf(m, sizeof m, "userpreset: --- LoadUserPreset(%d) -> %s ---", n,
                      fx_same(got, live) ? "chain UNCHANGED (empty slot, or out of range)"
                                         : "chain CHANGED (this slot holds a setup)");
        clog_(m);
        if (!fx_same(got, live)) {
            char tag[20];
            std::snprintf(tag, sizeof tag, "slot %d", n);
            fx_log(tag, got);
        }
        fx_restore(live);
    }

    if (do_write) {
        clog_("userpreset: === --write: round-tripping slot 0 and 1 (THIS OVERWRITES THEM) ===");
        for (int n = 0; n < 2; n++) {
            // A marker no real setup would be: full boost on band 0, full cut on band 9.
            cinder_effects_set_eq_band(0, 20);
            cinder_effects_set_eq_band(9, -20);
            cinder_effects_save_user_preset(n);
            // Disturb it, then load it back.
            cinder_effects_set_eq_band(0, 0);
            cinder_effects_set_eq_band(9, 0);
            cinder_effects_load_user_preset(n);
            const int b0 = cinder_effects_get_eq_band(0), b9 = cinder_effects_get_eq_band(9);
            std::snprintf(m, sizeof m,
                          "userpreset: slot %d round trip -> band0=%d band9=%d   %s", n, b0, b9,
                          (b0 == 20 && b9 == -20) ? "*** SAVE/LOAD WORKS ***" : "did not round trip");
            clog_(m);
            fx_restore(live);
        }
    }

    fx_restore(live);
    FxSnap back;
    fx_snap(back);
    std::snprintf(m, sizeof m, "userpreset: restored — chain %s the entry state",
                  fx_same(back, live) ? "MATCHES" : "DOES NOT MATCH");
    clog_(m);
    if (!fx_same(back, live)) fx_log("AFTER", back);
    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

// --tonefreq : recover the Tone Control CENTRE FREQUENCIES in Hz.
//
// `GetToneCenterFreq` echoes whatever it is handed and has no dB twin, so the ordinals cannot be
// settled by read-back the way the gains were. But the DSP prints the real numbers itself, from
// inside EffectSetParamHQEQ_ToneControl:
//
//     FS = [%u] FREQ = [%d] BFREQ1 = [%d] BFREQ2 = [%d]
//
// — and only while the effect is ACTUALLY PROCESSING, which is why this needs music playing, Tone
// Control on, and EqType 3 selected. An attempt on 2026-08-17 right after a reboot logged nothing
// because every PCM was closed.
//
// The correlation needs no marker of its own: the service logs `eqtone,type=N,centerfreq=F`
// immediately before the DSP reconfigures, so reading logcat in order pairs each ordinal with the
// Hz that follow it. Run as:
//
//     adb shell logcat -c
//     cinder-probe --tonefreq
//     adb shell logcat -d | grep -E 'centerfreq=|FREQ = \['
static int tonefreq_probe() {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    char m[224];
    const int tone0 = cinder_effects_is_tone_on();
    const int sel0 = cinder_effects_get_select_using_eq();
    int tv0[3], tf0[3];
    for (int b = 0; b < 3; b++) tv0[b] = cinder_effects_get_tone_value(b);
    for (int b = 0; b < 3; b++) tf0[b] = cinder_effects_get_tone_freq(b);
    std::snprintf(m, sizeof m, "tonefreq: entry ToneOn=%d sel=%d values=%d,%d,%d freqs=%d,%d,%d",
                  tone0, sel0, tv0[0], tv0[1], tv0[2], tf0[0], tf0[1], tf0[2]);
    clog_(m);

    // The effect has to be IN THE PATH and doing something, or it never reconfigures and never
    // logs. Non-zero gains on every band, tone control on, EqType 3 = ToneControl.
    for (int b = 0; b < 3; b++) cinder_effects_set_tone_value(b, 12);   // +6 dB
    cinder_effects_set_tone_control(1);
    cinder_effects_set_select_using_eq(3);
    clog_("tonefreq: tone control ON, EqType 3 — sweeping centre frequencies");
    sleep(1);

    for (int b = 0; b < 3; b++) {
        for (int f = 0; f < 8; f++) {
            cinder_effects_set_tone_freq(b, f);
            std::snprintf(m, sizeof m, "tonefreq: >>> band %d freq ordinal %d <<<", b, f);
            clog_(m);
            usleep(400000);   // let the DSP reconfigure and log before the next write
        }
        cinder_effects_set_tone_freq(b, tf0[b]);
    }

    for (int b = 0; b < 3; b++) {
        cinder_effects_set_tone_value(b, tv0[b]);
        cinder_effects_set_tone_freq(b, tf0[b]);
    }
    cinder_effects_set_tone_control(tone0);
    cinder_effects_set_select_using_eq(sel0);
    std::snprintf(m, sizeof m, "tonefreq: restored ToneOn=%d sel=%d values=%d,%d,%d freqs=%d,%d,%d",
                  cinder_effects_is_tone_on(), cinder_effects_get_select_using_eq(),
                  cinder_effects_get_tone_value(0), cinder_effects_get_tone_value(1),
                  cinder_effects_get_tone_value(2),
                  cinder_effects_get_tone_freq(0), cinder_effects_get_tone_freq(1),
                  cinder_effects_get_tone_freq(2));
    clog_(m);
    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

// --fm [play] : bring the FM tuner up and sweep the band.
//
// The Si4708 has never been touched by Cinder — `fm.rs` draws a hardcoded 88.6 MHz and the shell
// makes zero tuner calls. This is the first contact. Interface from analysis/RE_fm_tuner.md: the
// client vtable at 0x1789c, recovered from R_ARM_ABS32 relocations because the slots are not
// stored in the file.
//
// TWO THINGS THIS PROBE IS CAREFUL ABOUT:
//
//   * It does NOT call Play() unless asked (`--fm play`). Play routes tuner audio to the output at
//     whatever the hardware volume happens to be, and this probe gets run on a device someone may
//     have left at 120/120 with headphones on.
//   * It sweeps and REPORTS rather than asserting. `SetFrequency` takes a bare uint32 whose unit is
//     not recovered — kHz, 10 kHz and Hz are all plausible — so the sweep is also the unit test:
//     tune across the FM band under each candidate unit and see which one makes GetSignalLevel
//     move. A flat reading under every unit means either the wrong unit or no antenna.
//
// FM NEEDS THE HEADPHONE CABLE AS ITS ANTENNA. With nothing in the jack, every frequency reads
// dead and the result says nothing.
extern "C" void* _ZN3pst8services31TunerPlayerServiceClientFactory14CreateInstanceEv(void);
// The tuner chip and the AUDIO PATH for analogue sources are two different services, both hosted
// in hagoromo28. TunerPlayerService tunes; nothing is audible until AudioInPlayerService is played.
// That is why Open/SetFrequency/Play all returned 0 and the output stayed at a flat -59.8 dBFS.
extern "C" void* _ZN3pst8services33AudioInPlayerServiceClientFactory14CreateInstanceEv(void);

namespace {
// Slot numbers from analysis/RE_fm_tuner.md. Slot 0 is the first virtual after the
// [offset, typeinfo] header, same convention as `vslot` uses for the BT clients.
enum {
    VIDX_GetTunerState   = 3,
    VIDX_Open            = 4,
    VIDX_Close           = 5,
    VIDX_Play            = 6,
    VIDX_Stop            = 7,
    VIDX_GetSenseMode    = 18,
    VIDX_SetSenseMode    = 19,
    VIDX_StartAutoTuning = 21,
    VIDX_GetMuteMode     = 8,
    VIDX_SetMuteMode     = 9,
    VIDX_GetStereoState  = 12,
    VIDX_GetFrequency    = 16,
    VIDX_SetFrequency    = 17,
    VIDX_GetSignalLevel  = 20,
    VIDX_IsRunningAuto   = 23,
};
typedef int (*fn_v)(void*);
typedef int (*fn_pu)(void*, unsigned*);
typedef int (*fn_cu)(void*, const unsigned*);
typedef int (*fn_pi)(void*, int*);
typedef int (*fn_ci)(void*, const int*);
typedef int (*fn_seek)(void*, const unsigned*, const bool*, const unsigned*);

int fm_signal(void* c) {
    int lvl = -1;
    try { ((fn_pi)vslot(c, VIDX_GetSignalLevel))(c, &lvl); } catch (...) { return -1; }
    return lvl;
}
} // namespace

// --fm tune <kHz> [seconds] : hold ONE station so its audio can be heard or CAPTURED.
//
// This is the one that answers the open question in analysis/RE_fm_tuner.md — "where does the
// tuner audio go?". Open/Play return 0, but a return code proves nothing on this device (see the
// high-gain finding, and dacdat). The honest test is to record the headphone output on a PC and
// look at the level: FM audio present or not, no ears required.
//
// Wiring: the headphone cable IS the aerial, so the SAME cable can run to a PC input and still
// receive. Keep the volume LOW — a headphone output into a mic input is already a level mismatch,
// and this holds for `seconds` with no way to turn it down from here.
// Select the tuner as the codec's analogue input. MEASURED 2026-08-18: without this,
// AudioInPlayerService::Play() returns rc=1 and the audio path never opens; with it, rc=0 and the
// player state moves 0 -> 2. The ADC has to have a source before the capture side will start.
static void fm_route_on()  { std::system("amixer -c0 cset numid=26 1 >/dev/null 2>&1"); }
static void fm_route_off() { std::system("amixer -c0 cset numid=26 0 >/dev/null 2>&1"); }

// --fm audioscan <start_kHz> <end_kHz> <dwell_ms> : tune each step for a fixed dwell, playing.
//
// `GetSignalLevel` is useless for finding a station on this hardware — with the aerial in it
// returns 1 for 203 of the 206 frequencies in the band. So the scan is done the only way that
// actually discriminates: PLAY each frequency for a known dwell and measure the analogue output on
// a PC. A carrier is loud, a dead frequency is quiet, and the difference is tens of dB.
//
// The dwell is fixed and printed so the host can slice one continuous recording into per-frequency
// bins without any clock sync — bin N is frequency start + N*step. Run it as:
//
//   (host) start recording for nsteps*dwell + margin
//   (device) cinder-probe --fm audioscan 88000 91000 1500
//   (host) slice the WAV into nsteps bins, RMS each, the loud ones are stations
static int fm_audioscan(int start_khz, int end_khz, int dwell_ms) {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    char m[224];
    fm_route_on();
    void* c = _ZN3pst8services31TunerPlayerServiceClientFactory14CreateInstanceEv();
    if (!c) { clog_("fm: CreateInstance NULL — STOP"); fm_route_off(); g_pump_run = false; return 1; }
    try { ((fn_v)vslot(c, VIDX_Open))(c); } catch (...) {}
    try { ((fn_v)vslot(c, VIDX_Play))(c); } catch (...) {}
    // Open the audio path too, or this sweep is silent and there is nothing to listen to.
    void* ain = _ZN3pst8services33AudioInPlayerServiceClientFactory14CreateInstanceEv();
    if (ain) {
        int rc2 = -1, st = -1;
        try { rc2 = ((fn_v)vslot(ain, 3))(ain); } catch (...) {}
        try { st = ((fn_v)vslot(ain, 6))(ain); } catch (...) {}
        std::snprintf(m, sizeof m, "fm: AudioIn Play() rc=%d state=%d (2 = audio path open)", rc2, st);
        clog_(m);
    }

    const int step = 100;
    int nsteps = (end_khz - start_khz) / step + 1;
    std::snprintf(m, sizeof m,
                  "fm: audioscan %d..%d kHz, %d steps of %d kHz, %d ms each = %.1f s total",
                  start_khz, end_khz, nsteps, step, dwell_ms, nsteps * dwell_ms / 1000.0);
    clog_(m);
    clog_("fm: START-NOW");   // the host lines its recording up on this
    for (int i = 0; i < nsteps; i++) {
        unsigned f = (unsigned)(start_khz + i * step);
        try { ((fn_cu)vslot(c, VIDX_SetFrequency))(c, &f); } catch (...) {}
        std::snprintf(m, sizeof m, "fm:   >>> %.1f MHz", f / 1000.0);
        clog_(m);
        usleep(dwell_ms * 1000);
    }
    clog_("fm: END");
    if (ain) { try { ((fn_v)vslot(ain, 5))(ain); } catch (...) {} }
    try { ((fn_v)vslot(c, VIDX_Stop))(c); } catch (...) {}
    try { ((fn_v)vslot(c, VIDX_Close))(c); } catch (...) {}
    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

// --fm seek [from_kHz] : use Sony's own auto-tune to find a station.
//
// `GetSignalLevel` cannot discriminate (203 of 206 frequencies read 1) and an audio A/B of a known
// station against a dead frequency came out identical, so either nothing is receivable or the
// tuner needs more setup than Open/SetFrequency/Play. StartAutoTuning is the firmware's OWN seek —
// if any carrier is reachable it will stop on one, and where it stops is the answer.
//
// Signature: StartAutoTuning(const uint32_t&, const bool&, const uint32_t&). Argument roles are
// not recovered; best guess is (start frequency, direction-up, mode/threshold). Reported verbatim.
// --fm scan <start_kHz> <end_kHz> : a REAL station scanner, on-device, no PC needed.
//
// Neither of Sony's own primitives can find a station on this hardware:
//   * `GetSignalLevel` returns 1 at 203 of 206 frequencies — and still 1 everywhere once the aerial
//     is free. It is not an RSSI.
//   * `StartAutoTuning` returns within 100 ms with IsRunningAutoTuning()==0 and the frequency back
//     at its start value, in both directions — even when a station is AUDIBLY present.
// Both were verified against a station the user could hear. So a scan has to measure the AUDIO.
//
// The discriminator is spectral, not level. Unlocked FM is broadband hiss; a locked station is
// programme material with far less energy at the top of the band. So per frequency we capture from
// `hw:0,1` (the codec ADC, with `analog input device` = tuner) and compute
//
//     hf = mean(|x[n] - x[n-1]|) / mean(|x[n]|)
//
// a first-difference high-pass proxy. White noise sits near 1.4; music and speech sit well below
// it. LOW hf = station. Level alone does not work — hiss is often LOUDER than a locked carrier.
//
// AudioInPlayerService must NOT be playing during a scan: it owns hw:0,1 and the open would fail.
static int fm_scan(int start_khz, int end_khz) {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    char m[224];
    fm_route_on();
    void* c = _ZN3pst8services31TunerPlayerServiceClientFactory14CreateInstanceEv();
    if (!c) { clog_("fm: CreateInstance NULL"); fm_route_off(); g_pump_run = false; return 1; }
    try { ((fn_v)vslot(c, VIDX_Open))(c); } catch (...) {}
    try { ((fn_v)vslot(c, VIDX_Play))(c); } catch (...) {}

    snd_pcm_t* pcm = nullptr;
    int rc = snd_pcm_open(&pcm, "hw:0,1", SND_PCM_STREAM_CAPTURE, 0);
    if (rc < 0) {
        std::snprintf(m, sizeof m, "fm: cannot open hw:0,1 for capture: %s%s", snd_strerror(rc),
                      rc == -EBUSY ? "  (is AudioInPlayerService still playing?)" : "");
        clog_(m);
        try { ((fn_v)vslot(c, VIDX_Close))(c); } catch (...) {}
        fm_route_off(); g_pump_run = false; return 1;
    }
    const unsigned RATE = 44100;
    rc = snd_pcm_set_params(pcm, SND_PCM_FORMAT_S16_LE, SND_PCM_ACCESS_RW_INTERLEAVED,
                            2, RATE, 1, 200000);
    if (rc < 0) {
        std::snprintf(m, sizeof m, "fm: set_params failed: %s", snd_strerror(rc));
        clog_(m);
        snd_pcm_close(pcm);
        try { ((fn_v)vslot(c, VIDX_Close))(c); } catch (...) {}
        fm_route_off(); g_pump_run = false; return 1;
    }
    snd_pcm_start(pcm);   // the UAC lesson: this device does not start on its own

    const int step = 100, nsteps = (end_khz - start_khz) / step + 1;
    const int FR = RATE / 5;               // 200 ms of audio per frequency
    static short buf[(44100 / 5) * 2];
    std::snprintf(m, sizeof m, "fm: scanning %d..%d kHz, %d steps — LOW hf = station",
                  start_khz, end_khz, nsteps);
    clog_(m);

    struct Hit { unsigned f; double hf; double rms; };
    static Hit hits[256];
    int nh = 0;
    for (int i = 0; i < nsteps && nh < 256; i++) {
        unsigned f = (unsigned)(start_khz + i * step);
        try { ((fn_cu)vslot(c, VIDX_SetFrequency))(c, &f); } catch (...) {}
        usleep(250000);                     // let the tuner settle before believing the audio
        snd_pcm_drop(pcm); snd_pcm_prepare(pcm); snd_pcm_start(pcm);
        int got = 0;
        while (got < FR) {
            snd_pcm_sframes_t n = snd_pcm_readi(pcm, buf + got * 2, FR - got);
            if (n < 0) { snd_pcm_recover(pcm, (int)n, 1); break; }
            if (n == 0) break;
            got += (int)n;
        }
        if (got < FR / 2) { continue; }
        double sabs = 0, sdif = 0, sq = 0;
        int prev = buf[0];
        for (int k = 0; k < got; k++) {
            int v = buf[k * 2];             // left channel is enough
            sabs += (v < 0 ? -v : v);
            int d = v - prev; sdif += (d < 0 ? -d : d);
            sq += (double)v * v;
            prev = v;
        }
        double hf = sabs > 0 ? sdif / sabs : 9.9;
        double rms = 20.0 * std::log10((std::sqrt(sq / got) + 1e-9) / 32768.0);
        hits[nh].f = f; hits[nh].hf = hf; hits[nh].rms = rms; nh++;
    }
    snd_pcm_close(pcm);

    // Rank by hf ascending — the least hissy frequencies first.
    for (int a = 1; a < nh; a++) {
        Hit t = hits[a]; int b = a - 1;
        while (b >= 0 && hits[b].hf > t.hf) { hits[b + 1] = hits[b]; b--; }
        hits[b + 1] = t;
    }
    double med = nh ? hits[nh / 2].hf : 0;
    std::snprintf(m, sizeof m, "fm: median hf = %.3f (that is the no-station baseline)", med);
    clog_(m);
    clog_("fm: --- best candidates (lowest hf) ---");
    for (int i = 0; i < nh && i < 10; i++) {
        std::snprintf(m, sizeof m, "fm:   %6.1f MHz   hf %.3f   level %6.1f dBFS   %s",
                      hits[i].f / 1000.0, hits[i].hf, hits[i].rms,
                      hits[i].hf < med * 0.85 ? "<<< STATION" : "");
        clog_(m);
    }
    try { ((fn_v)vslot(c, VIDX_Stop))(c); } catch (...) {}
    try { ((fn_v)vslot(c, VIDX_Close))(c); } catch (...) {}
    fm_route_off();
    clog_("fm: scan done");
    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

// --fm autotune : drive the CHIP'S OWN SEEK properly, with a listener registered.
//
// The earlier conclusion that `StartAutoTuning` "finds nothing" was WRONG, and the symbol table
// says why: the result arrives through
//
//     OnChangedAutoTuningInfo(const ITunerPlayerService::AutoTuningState&, const uint32_t&)
//
// so the call is ASYNCHRONOUS and LISTENER-DRIVEN. Returning within 100 ms is the documented
// shape. The probe that dismissed it never registered a listener and polled IsRunningAutoTuning
// instead of waiting for the callback.
//
// THE VTABLE ORDER of the five IServiceListener callbacks is not recovered, so this does what
// CinderBtListener does in cinder-home: implement every slot with the SAME uninterpreted shape and
// let one seek reveal which fires. Nothing is dereferenced, so a wrong arity cannot fault — the
// arguments are recorded as raw words and interpreted afterwards, on paper.
//
// If this lands on a real frequency, a Sony-speed scan is available and the audio-measuring
// scanner becomes a fallback.
namespace {

struct LsnHit { int slot; unsigned long a, b, c; };
volatile int g_lsn_n = 0;
LsnHit g_lsn[64];

void lsn_note(int slot, unsigned long a, unsigned long b, unsigned long c) {
    int i = g_lsn_n;
    if (i < 64) { g_lsn[i].slot = slot; g_lsn[i].a = a; g_lsn[i].b = b; g_lsn[i].c = c; g_lsn_n = i + 1; }
}

// Virtual destructor FIRST: the Itanium ABI puts D1/D0 in slots 0/1, so the callbacks land at
// 2.. — the layout the library dispatches through. Twelve of them, comfortably more than the five
// the interface declares, so an unexpected slot lands on a recorder rather than on garbage.
struct TunerLsn {
    virtual ~TunerLsn() {}
    virtual void s0(unsigned long a, unsigned long b, unsigned long c) { lsn_note(0, a, b, c); }
    virtual void s1(unsigned long a, unsigned long b, unsigned long c) { lsn_note(1, a, b, c); }
    virtual void s2(unsigned long a, unsigned long b, unsigned long c) { lsn_note(2, a, b, c); }
    virtual void s3(unsigned long a, unsigned long b, unsigned long c) { lsn_note(3, a, b, c); }
    virtual void s4(unsigned long a, unsigned long b, unsigned long c) { lsn_note(4, a, b, c); }
    virtual void s5(unsigned long a, unsigned long b, unsigned long c) { lsn_note(5, a, b, c); }
    virtual void s6(unsigned long a, unsigned long b, unsigned long c) { lsn_note(6, a, b, c); }
    virtual void s7(unsigned long a, unsigned long b, unsigned long c) { lsn_note(7, a, b, c); }
    virtual void s8(unsigned long a, unsigned long b, unsigned long c) { lsn_note(8, a, b, c); }
    virtual void s9(unsigned long a, unsigned long b, unsigned long c) { lsn_note(9, a, b, c); }
    virtual void s10(unsigned long a, unsigned long b, unsigned long c) { lsn_note(10, a, b, c); }
    virtual void s11(unsigned long a, unsigned long b, unsigned long c) { lsn_note(11, a, b, c); }
};

} // namespace

static int fm_autotune(int from_khz) {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    char m[224];
    fm_route_on();
    void* c = _ZN3pst8services31TunerPlayerServiceClientFactory14CreateInstanceEv();
    if (!c) { clog_("fm: CreateInstance NULL"); fm_route_off(); g_pump_run = false; return 1; }
    try { ((fn_v)vslot(c, VIDX_Open))(c); } catch (...) {}
    unsigned f0 = (unsigned)from_khz;
    try { ((fn_cu)vslot(c, VIDX_SetFrequency))(c, &f0); } catch (...) {}
    try { ((fn_v)vslot(c, VIDX_Play))(c); } catch (...) {}

    // Register. AddListener is client slot 26; `name` is a notify FILTER KEY, not a label.
    TunerLsn lsn;
    enum { VIDX_AddListener = 26, VIDX_RemoveListener = 27 };
    typedef int (*fn_add)(void*, void*, const std::string*);
    typedef int (*fn_rem)(void*, void*);
    int add_rc = -1;
    {
        std::string name("cinder");
        try { wd_arm(12); add_rc = ((fn_add)vslot(c, VIDX_AddListener))(c, &lsn, &name); wd_disarm(); }
        catch (...) { clog_("fm: AddListener THREW"); }
    }
    std::snprintf(m, sizeof m, "fm: AddListener rc=%d (raw ptr, client builds the proxy)", add_rc);
    clog_(m);

    // Now the seek. Try both directions; report every callback that arrives.
    for (int dir = 1; dir >= 0; dir--) {
        g_lsn_n = 0;
        unsigned start = (unsigned)from_khz;
        bool up = dir != 0;
        unsigned arg3 = 0;
        int running_before = -1, running_after = -1;
        try { running_before = ((fn_v)vslot(c, VIDX_IsRunningAuto))(c); } catch (...) {}
        try { ((fn_seek)vslot(c, VIDX_StartAutoTuning))(c, &start, &up, &arg3); }
        catch (...) { clog_("fm: StartAutoTuning THREW"); continue; }
        // WAIT FOR THE CALLBACK, which is the whole point — do not poll IsRunningAutoTuning and
        // conclude from it. 15 s is far longer than any seek should take.
        int waited = 0;
        while (waited < 150 && g_lsn_n == 0) { usleep(100000); waited++; }
        usleep(500000);                                  // let a burst finish arriving
        try { running_after = ((fn_v)vslot(c, VIDX_IsRunningAuto))(c); } catch (...) {}
        unsigned landed = 0;
        try { ((fn_pu)vslot(c, VIDX_GetFrequency))(c, &landed); } catch (...) {}
        std::snprintf(m, sizeof m,
                      "fm: seek %s from %.1f -> %.1f MHz | callbacks=%d after %d ms | running %d->%d",
                      up ? "UP" : "DOWN", from_khz / 1000.0, landed / 1000.0,
                      (int)g_lsn_n, waited * 100, running_before, running_after);
        clog_(m);
        for (int i = 0; i < g_lsn_n && i < 12; i++) {
            std::snprintf(m, sizeof m, "fm:   callback slot %2d  args %lu, %lu, %lu",
                          g_lsn[i].slot, g_lsn[i].a, g_lsn[i].b, g_lsn[i].c);
            clog_(m);
        }
        if (g_lsn_n == 0) clog_("fm:   (no callback at all — listener not registered, or seek never ran)");
    }

    try { ((fn_rem)vslot(c, VIDX_RemoveListener))(c, &lsn); } catch (...) {}
    try { ((fn_v)vslot(c, VIDX_Stop))(c); } catch (...) {}
    try { ((fn_v)vslot(c, VIDX_Close))(c); } catch (...) {}
    fm_route_off();
    clog_("fm: autotune done");
    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

// --fm v4l2 : talk to the Si4708 DIRECTLY through the kernel's V4L2 radio node.
//
// Sony's StartAutoTuning is a stub (48 bytes, unconditionally returns 4 — see analysis/
// RE_fm_tuner.md), so their SEARCH interface does not exist. But the chip is not hidden:
//
//     /dev/radio0   "Silicon Labs. FM Tuner"   driver Si4708icx at i2c 2-0010
//
// V4L2 radio exposes exactly the two things Sony withheld: VIDIOC_G_TUNER carries a real signal
// strength and stereo flag, and VIDIOC_S_HW_FREQ_SEEK is the chip's hardware seek. If those work,
// an instant scanner and a real meter are both available — search over V4L2, audio still over
// TunerPlayerService + AudioInPlayerService.
//
// READ-ONLY except for tuning: it sets frequencies (which the tuner already lets anyone do) and
// tries one seek. Nothing here can write a register directly.
//
// The unit rule is easy to get wrong: with V4L2_TUNER_CAP_LOW the frequency field is in 1/16 kHz
// (62.5 Hz) steps, otherwise 1/16 MHz (62.5 kHz). Both are handled rather than assumed.
static int fm_v4l2(int station_khz, int dead_khz) {
    install_diagnostics();
    char m[224];

    int fd = open("/dev/radio0", O_RDWR);
    if (fd < 0) {
        std::snprintf(m, sizeof m, "v4l2: open(/dev/radio0) failed: %s%s", strerror(errno),
                      errno == EBUSY ? "  (TunerPlayerService probably holds it — stop FM first)"
                                     : "");
        clog_(m);
        return 1;
    }
    struct v4l2_capability cap;
    std::memset(&cap, 0, sizeof cap);
    if (ioctl(fd, VIDIOC_QUERYCAP, &cap) == 0) {
        std::snprintf(m, sizeof m, "v4l2: driver='%s' card='%s' caps=0x%08x",
                      cap.driver, cap.card, cap.capabilities);
        clog_(m);
        std::snprintf(m, sizeof m, "v4l2:   TUNER=%d RADIO=%d HW_FREQ_SEEK=%d RDS=%d",
                      !!(cap.capabilities & V4L2_CAP_TUNER),
                      !!(cap.capabilities & V4L2_CAP_RADIO),
                      !!(cap.capabilities & V4L2_CAP_HW_FREQ_SEEK),
                      !!(cap.capabilities & V4L2_CAP_RDS_CAPTURE));
        clog_(m);
    } else {
        std::snprintf(m, sizeof m, "v4l2: QUERYCAP failed: %s", strerror(errno));
        clog_(m);
    }

    struct v4l2_tuner tun;
    std::memset(&tun, 0, sizeof tun);
    if (ioctl(fd, VIDIOC_G_TUNER, &tun) != 0) {
        std::snprintf(m, sizeof m, "v4l2: G_TUNER failed: %s — no meter available here",
                      strerror(errno));
        clog_(m);
        close(fd);
        return 1;
    }
    const bool low = (tun.capability & V4L2_TUNER_CAP_LOW) != 0;
    const double unit_khz = low ? (1.0 / 16.0) : 62.5;   // field units expressed in kHz
    std::snprintf(m, sizeof m, "v4l2: tuner '%s' cap=0x%x CAP_LOW=%d range %.1f..%.1f MHz",
                  tun.name, tun.capability, (int)low,
                  tun.rangelow * unit_khz / 1000.0, tun.rangehigh * unit_khz / 1000.0);
    clog_(m);

    // The decisive test: does `signal` actually MOVE between a station and a dead frequency? A
    // constant is what Sony's own GetSignalLevel returns, and it is why the audio scanner exists.
    auto probe_at = [&](int khz, const char* label) {
        struct v4l2_frequency fr;
        std::memset(&fr, 0, sizeof fr);
        fr.tuner = 0;
        fr.type = V4L2_TUNER_RADIO;
        fr.frequency = (unsigned)(khz / unit_khz);
        if (ioctl(fd, VIDIOC_S_FREQUENCY, &fr) != 0) {
            std::snprintf(m, sizeof m, "v4l2: S_FREQUENCY(%.1f) failed: %s", khz / 1000.0,
                          strerror(errno));
            clog_(m);
            return;
        }
        usleep(300000);
        struct v4l2_tuner t2;
        std::memset(&t2, 0, sizeof t2);
        if (ioctl(fd, VIDIOC_G_TUNER, &t2) != 0) return;
        struct v4l2_frequency back;
        std::memset(&back, 0, sizeof back);
        back.tuner = 0; back.type = V4L2_TUNER_RADIO;
        ioctl(fd, VIDIOC_G_FREQUENCY, &back);
        std::snprintf(m, sizeof m,
                      "v4l2: %-8s %.1f MHz -> reads %.1f  signal=%5u/65535  stereo=%d",
                      label, khz / 1000.0, back.frequency * unit_khz / 1000.0,
                      t2.signal, !!(t2.rxsubchans & V4L2_TUNER_SUB_STEREO));
        clog_(m);
    };
    probe_at(station_khz, "STATION");
    probe_at(dead_khz,    "DEAD");
    probe_at(station_khz, "STATION");   // twice, so a drifting reading is visible as drift

    // And the hardware seek Sony stubbed out.
    struct v4l2_hw_freq_seek sk;
    std::memset(&sk, 0, sizeof sk);
    sk.tuner = 0;
    sk.type = V4L2_TUNER_RADIO;
    sk.seek_upward = 1;
    sk.wrap_around = 1;
    struct timespec a, b;
    clock_gettime(CLOCK_MONOTONIC, &a);
    int rc = ioctl(fd, VIDIOC_S_HW_FREQ_SEEK, &sk);
    clock_gettime(CLOCK_MONOTONIC, &b);
    long ms = (b.tv_sec - a.tv_sec) * 1000 + (b.tv_nsec - a.tv_nsec) / 1000000;
    struct v4l2_frequency landed;
    std::memset(&landed, 0, sizeof landed);
    landed.tuner = 0; landed.type = V4L2_TUNER_RADIO;
    ioctl(fd, VIDIOC_G_FREQUENCY, &landed);
    std::snprintf(m, sizeof m, "v4l2: HW_FREQ_SEEK up rc=%d (%s) in %ld ms -> %.1f MHz",
                  rc, rc == 0 ? "OK" : strerror(errno), ms,
                  landed.frequency * unit_khz / 1000.0);
    clog_(m);

    close(fd);
    clog_("v4l2: done");
    return 0;
}

// --fm v4l2scan <start_kHz> <end_kHz> : sweep the band reading the REAL signal meter.
//
// VIDIOC_G_TUNER.signal is full-scale on a carrier and zero on a dead frequency (measured
// 2026-08-18), so a scan no longer needs to demodulate anything. This characterises the meter
// across the whole band: how many steps report signal, whether it is graded or binary, and how
// long a step actually takes — which is what decides the settle time the shim should use.
static int fm_v4l2scan(int start_khz, int end_khz, int settle_ms, bool power) {
    install_diagnostics();
    char m[256];
    // POWER THE CHIP FIRST. V4L2 only READS the Si4708 — TunerPlayerService::Open() is what turns
    // it on. Measured 2026-08-18: a sweep run straight after an FM listening session (chip still
    // powered) reported signal, and the identical sweep run later, cold, reported 0 everywhere at
    // every settle time. The meter is not broken; an unpowered tuner receives nothing.
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);
    void* tc = nullptr;
    if (power) {
        fm_route_on();
        tc = _ZN3pst8services31TunerPlayerServiceClientFactory14CreateInstanceEv();
    }
    if (tc) {
        try { ((fn_v)vslot(tc, VIDX_Open))(tc); } catch (...) {}
        try { ((fn_v)vslot(tc, VIDX_Play))(tc); } catch (...) {}
    }
    std::snprintf(m, sizeof m, "v4l2: tuner powered (client %p, state %d)", tc,
                  tc ? ((fn_v)vslot(tc, VIDX_GetTunerState))(tc) : -1);
    clog_(m);

    int fd = open("/dev/radio0", O_RDWR);
    if (fd < 0) {
        std::snprintf(m, sizeof m, "v4l2: open failed: %s", strerror(errno));
        clog_(m); return 1;
    }
    struct v4l2_tuner tun;
    std::memset(&tun, 0, sizeof tun);
    if (ioctl(fd, VIDIOC_G_TUNER, &tun) != 0) { clog_("v4l2: G_TUNER failed"); close(fd); return 1; }
    const double unit_khz = (tun.capability & V4L2_TUNER_CAP_LOW) ? (1.0 / 16.0) : 62.5;

    struct timespec t0, t1;
    clock_gettime(CLOCK_MONOTONIC, &t0);
    int steps = 0, hits = 0;
    unsigned hist[9] = {0};                 // signal distribution, in eighths of full scale
    static struct { int khz; unsigned sig; } found[64];
    int nf = 0;
    for (int khz = start_khz; khz <= end_khz; khz += 100) {
        struct v4l2_frequency fr;
        std::memset(&fr, 0, sizeof fr);
        fr.tuner = 0; fr.type = V4L2_TUNER_RADIO;
        fr.frequency = (unsigned)(khz / unit_khz);
        if (ioctl(fd, VIDIOC_S_FREQUENCY, &fr) != 0) continue;
        usleep(settle_ms * 1000);
        struct v4l2_tuner t;
        std::memset(&t, 0, sizeof t);
        if (ioctl(fd, VIDIOC_G_TUNER, &t) != 0) continue;
        steps++;
        hist[t.signal * 8 / 65536]++;
        if (t.signal > 32768) {
            hits++;
            if (nf < 64) { found[nf].khz = khz; found[nf].sig = t.signal; nf++; }
        }
    }
    clock_gettime(CLOCK_MONOTONIC, &t1);
    long ms = (t1.tv_sec - t0.tv_sec) * 1000 + (t1.tv_nsec - t0.tv_nsec) / 1000000;
    std::snprintf(m, sizeof m, "v4l2: %d steps in %ld ms (%.1f ms/step, settle %d ms) — %d above half scale",
                  steps, ms, steps ? (double)ms / steps : 0.0, settle_ms, hits);
    clog_(m);
    std::snprintf(m, sizeof m, "v4l2: signal histogram (eighths): %u %u %u %u %u %u %u %u %u",
                  hist[0], hist[1], hist[2], hist[3], hist[4], hist[5], hist[6], hist[7], hist[8]);
    clog_(m);
    // Collapse adjacent steps: a transmitter lights several 100 kHz slots.
    int i = 0;
    while (i < nf) {
        int j = i;
        while (j + 1 < nf && found[j + 1].khz == found[j].khz + 100) j++;
        int centre = found[i].khz + ((found[j].khz - found[i].khz) / 200) * 100;
        std::snprintf(m, sizeof m, "v4l2:   %6.1f MHz  (%5.1f-%5.1f, %d steps)  signal=%u",
                      centre / 1000.0, found[i].khz / 1000.0, found[j].khz / 1000.0,
                      j - i + 1, found[i].sig);
        clog_(m);
        i = j + 1;
    }
    close(fd);
    if (tc) {
        try { ((fn_v)vslot(tc, VIDX_Stop))(tc); } catch (...) {}
        try { ((fn_v)vslot(tc, VIDX_Close))(tc); } catch (...) {}
        fm_route_off();
    }
    g_pump_run = false;
    return 0;
}

// --fm i2c : read the Si4708's OWN registers over /dev/i2c-2, bypassing Sony's driver.
//
// WHY. Everything above it is lossy. TunerPlayerService::GetSignalLevel returns a constant, its
// StartAutoTuning is a 48-byte stub, and the kernel driver's V4L2 meter is BINARY — the histogram
// across the band is only ever 0 or 65535, which is a "tuned" flag rather than a signal strength,
// and it flickers on marginal stations (97.3 present in one sweep, absent in the next).
//
// The chip itself has neither problem. Si470x-family register map (public):
//
//   0x02 POWERCFG    SEEK(8) SEEKUP(9) SKMODE(10)      <- the hardware seek nothing else exposes
//   0x03 CHANNEL     TUNE(15) + channel
//   0x05 SYSCONFIG2  SEEKTH(15:8) BAND(7:6) SPACE(5:4) VOLUME(3:0)
//   0x0A STATUSRSSI  RSSI(7:0) ST(8) BLERA SF/BL(13) STC(14) RDSR(15)
//   0x0B READCHAN    channel(9:0)
//
// READ PROTOCOL: the Si470x returns registers starting at 0x0A and wrapping — read 32 bytes and
// you get 0x0A..0x0F then 0x00..0x09, big-endian 16-bit each. There is no register-address byte.
//
// THIS PROBE ONLY READS. It does not write POWERCFG, because the kernel driver is bound to this
// chip and a write underneath it could desynchronise the driver's shadow of the register file.
// Reading is safe: the chip has no read side effects except STC clearing, which we do not touch.
static int fm_i2c(void) {
    install_diagnostics();
    char m[256];
    int fd = open("/dev/i2c-2", O_RDWR);
    if (fd < 0) {
        std::snprintf(m, sizeof m, "i2c: open(/dev/i2c-2) failed: %s", strerror(errno));
        clog_(m);
        return 1;
    }
    // I2C_SLAVE = 0x0703. Using the numeric constant avoids needing linux/i2c-dev.h, which is not
    // in the device sysroot.
    if (ioctl(fd, 0x0703, 0x10) < 0) {
        std::snprintf(m, sizeof m, "i2c: I2C_SLAVE 0x10 failed: %s%s", strerror(errno),
                      errno == EBUSY ? "  (the Si4708icx driver holds it — try I2C_SLAVE_FORCE)"
                                     : "");
        clog_(m);
        // 0x0706 = I2C_SLAVE_FORCE: take the address even though a driver is bound. Read-only, so
        // the risk is a confused driver shadow at worst, not a bus fault.
        if (ioctl(fd, 0x0706, 0x10) < 0) {
            clog_("i2c: I2C_SLAVE_FORCE also failed — no direct access");
            close(fd);
            return 1;
        }
        clog_("i2c: took the address with I2C_SLAVE_FORCE");
    }
    // MediaTek's adapter does not implement the simple read()/write() file ops — plain read()
    // returns EINVAL. Use I2C_RDWR with an explicit message instead, and try progressively smaller
    // transfers: MTK controllers frequently cap a single transaction well below 32 bytes.
    struct i2c_msg_ { unsigned short addr, flags; unsigned short len; unsigned char* buf; };
    struct i2c_rdwr_ { struct i2c_msg_* msgs; int nmsgs; };
    unsigned char buf[32];
    std::memset(buf, 0, sizeof buf);
    ssize_t n = -1;
    int used = 0;
    for (int want : { 32, 16, 8 }) {
        struct i2c_msg_ msg;
        msg.addr = 0x10; msg.flags = 1 /* I2C_M_RD */; msg.len = (unsigned short)want; msg.buf = buf;
        struct i2c_rdwr_ rd; rd.msgs = &msg; rd.nmsgs = 1;
        if (ioctl(fd, 0x0707 /* I2C_RDWR */, &rd) >= 0) { n = want; used = want; break; }
        std::snprintf(m, sizeof m, "i2c: I2C_RDWR read of %d B failed: %s", want, strerror(errno));
        clog_(m);
    }
    if (n < 0) {
        clog_("i2c: no direct transfer worked — the adapter refuses userspace access to this device");
        close(fd);
        return 1;
    }
    std::snprintf(m, sizeof m, "i2c: read %d bytes via I2C_RDWR", used);
    clog_(m);
    if (used < 32) {
        clog_("i2c: PARTIAL — registers past the transfer length are stale zeros, read with care");
    }
    // Registers come back starting at 0x0A and wrapping to 0x09.
    unsigned short reg[16];
    for (int i = 0; i < 16; i++) {
        int r = (0x0A + i) & 0x0F;
        reg[r] = (unsigned short)((buf[i * 2] << 8) | buf[i * 2 + 1]);
    }
    for (int r = 0; r < 16; r++) {
        std::snprintf(m, sizeof m, "i2c: reg[0x%02X] = 0x%04X", r, reg[r]);
        clog_(m);
    }
    const unsigned short st = reg[0x0A];
    std::snprintf(m, sizeof m,
                  "i2c: STATUSRSSI RSSI=%u  ST=%u  STC=%u  SF/BL=%u  RDSR=%u",
                  st & 0xFF, (st >> 8) & 1, (st >> 14) & 1, (st >> 13) & 1, (st >> 15) & 1);
    clog_(m);
    std::snprintf(m, sizeof m, "i2c: READCHAN channel=%u   POWERCFG=0x%04X   SYSCONFIG2=0x%04X",
                  reg[0x0B] & 0x03FF, reg[0x02], reg[0x05]);
    clog_(m);
    // Device ID registers confirm we are talking to the right part rather than reading noise.
    std::snprintf(m, sizeof m, "i2c: DEVICEID=0x%04X CHIPID=0x%04X  (pn=%u mfgid=0x%03X)",
                  reg[0x00], reg[0x01], (reg[0x00] >> 12) & 0xF, reg[0x00] & 0xFFF);
    clog_(m);
    close(fd);
    return 0;
}

static int fm_seek(int from_khz) {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    char m[224];
    fm_route_on();
    void* c = _ZN3pst8services31TunerPlayerServiceClientFactory14CreateInstanceEv();
    if (!c) { clog_("fm: CreateInstance NULL"); fm_route_off(); g_pump_run = false; return 1; }
    try { ((fn_v)vslot(c, VIDX_Open))(c); } catch (...) {}
    try { ((fn_v)vslot(c, VIDX_Play))(c); } catch (...) {}
    void* ain = _ZN3pst8services33AudioInPlayerServiceClientFactory14CreateInstanceEv();
    if (ain) { try { ((fn_v)vslot(ain, 3))(ain); } catch (...) {} }

    unsigned f = (unsigned)from_khz;
    try { ((fn_cu)vslot(c, VIDX_SetFrequency))(c, &f); } catch (...) {}
    // Sense mode first: if it is a sensitivity/threshold setting, the least selective value gives
    // the best chance of finding anything indoors on a short aerial.
    for (int sm = 0; sm <= 2; sm++) {
        int cur = -1;
        try { ((fn_ci)vslot(c, VIDX_SetSenseMode))(c, &sm); } catch (...) {}
        try { ((fn_pi)vslot(c, VIDX_GetSenseMode))(c, &cur); } catch (...) {}
        std::snprintf(m, sizeof m, "fm: SetSenseMode(%d) -> reads %d", sm, cur);
        clog_(m);
    }
    for (int dir = 1; dir >= 0; dir--) {
        unsigned start = (unsigned)from_khz;
        bool up = dir != 0;
        unsigned arg3 = 0;
        try { ((fn_seek)vslot(c, VIDX_StartAutoTuning))(c, &start, &up, &arg3); }
        catch (...) { clog_("fm: StartAutoTuning threw"); continue; }
        int running = 1, waited = 0;
        while (waited < 150) {
            usleep(100000); waited++;
            try { running = ((fn_v)vslot(c, VIDX_IsRunningAuto))(c); } catch (...) { break; }
            if (!running) break;
        }
        unsigned landed = 0;
        try { ((fn_pu)vslot(c, VIDX_GetFrequency))(c, &landed); } catch (...) {}
        int stereo = -1;
        try { ((fn_pi)vslot(c, VIDX_GetStereoState))(c, &stereo); } catch (...) {}
        std::snprintf(m, sizeof m,
                      "fm: seek %s from %.1f -> landed %.1f MHz after %d ms (running=%d stereo=%d)",
                      up ? "UP" : "DOWN", from_khz / 1000.0, landed / 1000.0, waited * 100,
                      running, stereo);
        clog_(m);
    }
    if (ain) { try { ((fn_v)vslot(ain, 5))(ain); } catch (...) {} }
    try { ((fn_v)vslot(c, VIDX_Stop))(c); } catch (...) {}
    try { ((fn_v)vslot(c, VIDX_Close))(c); } catch (...) {}
    fm_route_off();
    clog_("fm: seek done");
    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

static int fm_tune(int khz, int seconds, const char* ainame) {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    char m[224];
    // SELECT THE TUNER AS THE ADC'S SOURCE FIRST. Without this the capture side has nothing to
    // read and the whole path is silent even on a strong carrier — which is exactly what happened
    // on 2026-08-18: `--fm audioscan` had this call and was audible, `--fm tune` did not and played
    // 45 s of nothing on 97.3 MHz.
    fm_route_on();
    void* c = _ZN3pst8services31TunerPlayerServiceClientFactory14CreateInstanceEv();
    if (!c) { clog_("fm: CreateInstance returned NULL — STOP"); fm_route_off(); g_pump_run = false; return 1; }

    int rc = -1;
    try { wd_arm(12); rc = ((fn_v)vslot(c, VIDX_Open))(c); wd_disarm(); } catch (...) { rc = -99; }
    unsigned want = (unsigned)khz, got = 0;
    try { ((fn_cu)vslot(c, VIDX_SetFrequency))(c, &want); usleep(200000);
          ((fn_pu)vslot(c, VIDX_GetFrequency))(c, &got); } catch (...) {}
    if (got != want) {
        std::snprintf(m, sizeof m, "fm: asked for %u kHz, tuner holds %u — REJECTED, out of band?",
                      want, got);
        clog_(m);
    }
    // MUTE MODE. Never touched until 2026-08-18, when an audio sweep of 88-91 MHz came back FLAT
    // at -59.7 dBFS across all 31 steps — well above the -93 dBFS noise floor, so the amp was on,
    // but with no station discrimination whatsoever. A muted tuner looks exactly like that: you
    // hear the output stage, not the radio. Report what it holds, then explicitly clear it.
    int mute0 = -1;
    try { ((fn_pi)vslot(c, VIDX_GetMuteMode))(c, &mute0); } catch (...) {}
    {
        FILE* af = std::fopen("/sys/class/switch/cxd3778gf_antenna/state", "r");
        int ant = -1; if (af) { if (std::fscanf(af, "%d", &ant) != 1) ant = -1; std::fclose(af); }
        std::snprintf(m, sizeof m, "fm: antenna switch = %d (1 = something in the jack; it is the AERIAL)", ant);
        clog_(m);
    }
    std::snprintf(m, sizeof m, "fm: GetMuteMode = %d (clearing to 0 before Play)", mute0);
    clog_(m);
    { int off = 0; try { ((fn_ci)vslot(c, VIDX_SetMuteMode))(c, &off); } catch (...) {} }
    int mute1 = -1;
    try { ((fn_pi)vslot(c, VIDX_GetMuteMode))(c, &mute1); } catch (...) {}
    std::snprintf(m, sizeof m, "fm: MuteMode now %d", mute1);
    clog_(m);
    int play_rc = -1;
    try { wd_arm(12); play_rc = ((fn_v)vslot(c, VIDX_Play))(c); wd_disarm(); } catch (...) {}
    // ORDER MATTERS: the tuner must be STREAMING before the capture side opens. `--fm audioscan`
    // played (tuner Play, then AudioIn) and was audible; this function opened AudioIn first and was
    // SILENT even on a strong carrier. Same shape as reference_uac_capture_start — the capture PCM
    // must not be opened before the source is live.
    // …and OPEN THE ANALOGUE AUDIO PATH. Slot 3 Play(), slot 4 Play(const std::string&) — the
    // string overload names a source. Try the bare one first, then the named one, reporting the
    // player state after each so the log says which (if either) took.
    void* ain = nullptr;
    try { ain = _ZN3pst8services33AudioInPlayerServiceClientFactory14CreateInstanceEv(); } catch (...) {}
    if (!ain) {
        clog_("fm: AudioInPlayerServiceClient NULL — audio path cannot be opened");
    } else {
        int st0 = -1, r3 = -1, r4 = -1;
        try { st0 = ((fn_v)vslot(ain, 6))(ain); } catch (...) {}
        try { wd_arm(12); r3 = ((fn_v)vslot(ain, 3))(ain); wd_disarm(); } catch (...) {}
        int st1 = -1;
        try { st1 = ((fn_v)vslot(ain, 6))(ain); } catch (...) {}
        std::snprintf(m, sizeof m, "fm: AudioIn state %d -> Play() rc=%d -> state %d", st0, r3, st1);
        clog_(m);
        if (st1 == st0) {
            // The accepted names are the ones BOTH libAudioInPlayerService and libSoundServiceFw
            // carry: music, beep, hdmi, hfp, mic, mrmcloop. "tuner" is NOT among them — that was a
            // guess on 2026-08-18 and the device rebooted into stock. `Play()` with no argument
            // builds an EMPTY string (disassembled at 0xabf8) and returns rc=1, so the name is
            // required and validated.
            std::string src(ainame);
            typedef int (*fn_s)(void*, const std::string*);
            try { wd_arm(12); r4 = ((fn_s)vslot(ain, 4))(ain, &src); wd_disarm(); } catch (...) {}
            int st2 = -1;
            try { st2 = ((fn_v)vslot(ain, 6))(ain); } catch (...) {}
            std::snprintf(m, sizeof m, "fm: AudioIn Play(\"%s\") rc=%d -> state %d", ainame, r4, st2);
            clog_(m);
        }
    }
    int stereo = -1;
    try { ((fn_pi)vslot(c, VIDX_GetStereoState))(c, &stereo); } catch (...) {}

    std::snprintf(m, sizeof m,
                  "fm: Open=%d tuned %.1f MHz Play=%d signal=%d stereoState=%d — HOLDING %ds",
                  rc, got / 1000.0, play_rc, fm_signal(c), stereo, seconds);
    clog_(m);
    // Re-assert the frequency now that both players are up. `--fm audioscan` tunes AFTER the
    // plays and is audible; tuning only before them leaves room for Play() to reset it.
    try { ((fn_cu)vslot(c, VIDX_SetFrequency))(c, &want); } catch (...) {}
    clog_("fm: (capture the headphone output NOW — tools/measure_output.py on the host)");

    // Report every few seconds so a listener/capture can be lined up against the log.
    for (int t = 0; t < seconds; t++) {
        sleep(1);
        if (t % 5 == 4) {
            int st = -1;
            try { ((fn_pi)vslot(c, VIDX_GetStereoState))(c, &st); } catch (...) {}
            std::snprintf(m, sizeof m, "fm:   +%2ds  signal=%d stereoState=%d", t + 1,
                          fm_signal(c), st);
            clog_(m);
        }
    }

    if (ain) { try { ((fn_v)vslot(ain, 5))(ain); } catch (...) {} clog_("fm: AudioIn Stop()"); }
    fm_route_off();
    try { ((fn_v)vslot(c, VIDX_Stop))(c); } catch (...) {}
    try { ((fn_v)vslot(c, VIDX_Close))(c); } catch (...) {}
    fm_route_off();
    clog_("fm: Stop() + Close() — done");
    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

static int fm_probe(bool do_play) {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    char m[224];
    clog_("fm: TunerPlayerServiceClientFactory::CreateInstance() …");
    wd_arm(12);
    void* c = _ZN3pst8services31TunerPlayerServiceClientFactory14CreateInstanceEv();
    wd_disarm();
    if (!c) { clog_("fm: CreateInstance returned NULL — STOP"); g_pump_run = false; return 1; }
    std::snprintf(m, sizeof m, "fm: client %p", c);
    clog_(m);

    int st = -1;
    try { wd_arm(10); st = ((fn_v)vslot(c, VIDX_GetTunerState))(c); wd_disarm(); }
    catch (...) { clog_("fm: GetTunerState threw"); }
    std::snprintf(m, sizeof m, "fm: GetTunerState (before Open) = %d", st);
    clog_(m);

    int rc = -1;
    try { wd_arm(12); rc = ((fn_v)vslot(c, VIDX_Open))(c); wd_disarm(); }
    catch (...) { clog_("fm: Open threw — STOP"); g_pump_run = false; std::fflush(nullptr); _exit(1); }
    std::snprintf(m, sizeof m, "fm: Open() rc=%d", rc);
    clog_(m);

    try { wd_arm(10); st = ((fn_v)vslot(c, VIDX_GetTunerState))(c); wd_disarm(); } catch (...) {}
    unsigned f0 = 0;
    try { ((fn_pu)vslot(c, VIDX_GetFrequency))(c, &f0); } catch (...) {}
    std::snprintf(m, sizeof m, "fm: after Open state=%d  GetFrequency=%u  signal=%d",
                  st, f0, fm_signal(c));
    clog_(m);

    if (do_play) {
        try { wd_arm(12); rc = ((fn_v)vslot(c, VIDX_Play))(c); wd_disarm(); }
        catch (...) { rc = -99; }
        std::snprintf(m, sizeof m, "fm: Play() rc=%d  (audio is live from here)", rc);
        clog_(m);
        sleep(1);
    } else {
        clog_("fm: NOT calling Play() — pass `--fm play` for that. Sweeping muted.");
    }

    // Unit test + band sweep in one. 98.0 MHz expressed three ways; whichever the service keeps
    // (and whichever makes the signal move) is the unit.
    static const struct { const char* name; unsigned v; } kUnits[] = {
        { "kHz   (98000)",    98000u    },
        { "10kHz (9800)",     9800u     },
        { "Hz    (98000000)", 98000000u },
    };
    for (unsigned i = 0; i < sizeof kUnits / sizeof kUnits[0]; i++) {
        unsigned back = 0;
        try {
            ((fn_cu)vslot(c, VIDX_SetFrequency))(c, &kUnits[i].v);
            usleep(200000);
            ((fn_pu)vslot(c, VIDX_GetFrequency))(c, &back);
        } catch (...) { back = 0xFFFFFFFFu; }
        std::snprintf(m, sizeof m, "fm: SetFrequency %-16s -> reads %-10u signal=%d",
                      kUnits[i].name, back, fm_signal(c));
        clog_(m);
    }

    // Sweep the band in kHz — the most likely unit, and the one the read-back above will have
    // confirmed or denied. 87.5 to 108.0 MHz in 100 kHz steps is the European raster.
    clog_("fm: sweeping 87.5-108.0 MHz in 100 kHz steps (kHz units) …");
    // MEASURED 2026-08-17 with the aerial in: GetSignalLevel returns 0 or 1, not an RSSI scale.
    // So there is no "strongest station" to rank — the useful output is the LIST of frequencies
    // that come back non-zero. The first version of this kept a top-8, which with a boolean signal
    // was just the first eight ties in sweep order and told you nothing.
    unsigned hits[256]; int nhits = 0;
    int seen_min = 1 << 30, seen_max = -(1 << 30);
    for (unsigned khz = 87500; khz <= 108000; khz += 100) {
        try { ((fn_cu)vslot(c, VIDX_SetFrequency))(c, &khz); } catch (...) { break; }
        usleep(30000);
        int lvl = fm_signal(c);
        if (lvl < seen_min) seen_min = lvl;
        if (lvl > seen_max) seen_max = lvl;
        if (lvl > 0 && nhits < 256) hits[nhits++] = khz;
    }
    std::snprintf(m, sizeof m, "fm: signal across the band: min=%d max=%d", seen_min, seen_max);
    clog_(m);
    if (seen_min == seen_max) {
        clog_("fm: *** FLAT — no station discrimination. Either the unit is wrong, the tuner needs");
        clog_("fm:     Play() first, or NOTHING IS PLUGGED INTO THE HEADPHONE JACK (it is the aerial).");
    } else {
        std::snprintf(m, sizeof m, "fm: %d of 206 frequencies report signal — collapsing to carriers:",
                      nhits);
        clog_(m);
        // A real transmitter lights several adjacent 100 kHz steps, so print RUNS. The centre of a
        // run is the station; a lone isolated step is more likely noise than a broadcaster.
        int i = 0;
        while (i < nhits) {
            int j = i;
            while (j + 1 < nhits && hits[j + 1] == hits[j] + 100) j++;
            unsigned centre = hits[i] + ((hits[j] - hits[i]) / 200) * 100;
            int stereo = -1;
            try { ((fn_cu)vslot(c, VIDX_SetFrequency))(c, &centre); usleep(200000);
                  ((fn_pi)vslot(c, VIDX_GetStereoState))(c, &stereo); } catch (...) {}
            std::snprintf(m, sizeof m,
                          "fm:   %6.1f MHz  (%5.1f-%5.1f, %2d steps)  stereoState=%d  %s",
                          centre / 1000.0, hits[i] / 1000.0, hits[j] / 1000.0, j - i + 1, stereo,
                          (j - i + 1) >= 3 ? "<- looks like a station" : "");
            clog_(m);
            i = j + 1;
        }
    }

    if (do_play) { try { ((fn_v)vslot(c, VIDX_Stop))(c); } catch (...) {} clog_("fm: Stop()"); }
    if (f0) { try { ((fn_cu)vslot(c, VIDX_SetFrequency))(c, &f0); } catch (...) {} }
    try { rc = ((fn_v)vslot(c, VIDX_Close))(c); } catch (...) { rc = -99; }
    std::snprintf(m, sizeof m, "fm: Close() rc=%d", rc);
    clog_(m);
    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

// --seqtime : how long does SetTrackSequence take as the sequence grows?
//
// Chasing a HANG reported 2026-08-18: "all songs" shuffled, shuffle pressed a couple of times, then
// the device stopped responding to any input and had to be force-rebooted (the launcher's bad-boot
// counter then reverted it to stock — the cable was NOT in, so that rung did its job).
//
// The suspect path is the queue flush, which runs under `run_guarded(..., 10, ...)`. If a large
// sequence pushes SetTrackSequence past that budget, SIGALRM fires and `fault_handler` siglongjmps
// out of the call — and a siglongjmp that unwinds out of code holding a lock leaves that lock held
// FOREVER. main.cpp already documents exactly this failure for malloc's arena lock (the 2026-07-02
// SIGABRT note). The same shape applied to cinder-ffi's renderer mutex would freeze every later
// FFI entry point, which is precisely "stopped responding to inputs from anything".
//
// That is a chain of plausible steps, not a measurement. This measures the first link: the actual
// cost of SetTrackSequence against sequence length. If 512 tracks lands well inside 10 s the
// theory is wrong and the hang is elsewhere; if it approaches or exceeds it, the theory stands and
// the guard budget is the thing to change.
//
// Builds its sequences from URIs the caller supplies, repeated — the SIZE is what is under test,
// not the content.
static int seqtime_probe(const char* uri) {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);

    char m[256];
    static const int kN[] = { 1, 10, 50, 100, 200, 350, 512 };
    static const char* ptrs[512];
    for (int i = 0; i < 512; i++) ptrs[i] = uri;

    // Bring the PlayerService client up FIRST. Without this every call returns -1 immediately and
    // the timings measure nothing but an early bail-out — which is exactly what the first run did.
    clog_("seqtime: cinder_audio_init(\"cinderprobe\") …");
    wd_arm(20);
    int ai = cinder_audio_init("cinderprobe");
    wd_disarm();
    int waited = 0;
    while (!cinder_audio_is_connected() && waited < 50) { usleep(100000); ++waited; }
    std::snprintf(m, sizeof m, "seqtime: audio_init=%d connected=%d after %d ms",
                  ai, cinder_audio_is_connected(), waited * 100);
    clog_(m);
    if (!cinder_audio_is_connected()) {
        clog_("seqtime: NOT CONNECTED — timings below would be meaningless, stopping");
        g_pump_run = false; std::fflush(nullptr); _exit(1);
    }
    clog_("seqtime: the queue flush is guarded at 10 s = 10000000 us. Watch for that line.");
    for (unsigned k = 0; k < sizeof kN / sizeof kN[0]; k++) {
        const int n = kN[k];
        struct timespec a, b;
        clock_gettime(CLOCK_MONOTONIC, &a);
        int rc = cinder_audio_play_tracks(ptrs, n, 0);
        clock_gettime(CLOCK_MONOTONIC, &b);
        long long us = (long long)(b.tv_sec - a.tv_sec) * 1000000LL
                     + (b.tv_nsec - a.tv_nsec) / 1000;
        std::snprintf(m, sizeof m, "seqtime: %4d tracks -> %8lld us (%.2f s) rc=%d%s",
                      n, us, us / 1e6, rc,
                      us > 10000000LL ? "   *** PAST THE 10 s GUARD ***"
                    : us >  5000000LL ? "   <- over half the budget" : "");
        clog_(m);
        usleep(300000);
    }
    clog_("seqtime: done — playback was left wherever the last call put it; press pause in the UI.");
    g_pump_run = false;
    std::fflush(nullptr);
    _exit(0);
}

// --vpt : settle the VptMode and DcPhaseFilterType enumerators by EXPERIMENT.
//
// Both effects have more than on/off in Sony's UI (VPT: Studio / Club / Concert Hall / Matrix;
// DC Phase: Low and Standard, each A and B) but Cinder renders them as a bool, because
// `EffectCtrlDmp::SetVptMode(VptMode)` takes an enum whose VALUES were never recovered — the
// symbols give the names, not the ordinals.
//
// Unlike most of this ABI, these two are directly probeable: `GetVptMode` and
// `GetDcPhaseFilterType` are exported. So write a candidate and read it back. A value the service
// KEEPS is a valid enumerator; one that reads back as something else was rejected and clamped.
// That distinguishes the real range from a guess without decompiling anything.
//
// It is also audible, which is the other half of the test — and the high-gain lesson says the
// read-back alone is not proof. Run it with music playing and VPT ON, and listen: the modes are
// room simulations and the difference is not subtle. `--vpt <n>` parks a single mode so you can sit
// on it; with no argument it sweeps and restores whatever was set when it started.
static int vpt_probe(int only, int hold_s) {
    install_diagnostics();
    clog_("vpt: effects client + sweep. Play something first, or there is nothing to hear.");

    // FRAMEWORK PUMP FIRST. Without a turning looper a pst client's reads are not answers — the
    // first run of this probe had every value read back 0, including ones the setter had just been
    // given, which is the signature of a reply that never arrived rather than of a rejected write.
    // Same invariant as --btinfo and every other mode here.
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    int sr = fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);
    {
        char pm[96];
        std::snprintf(pm, sizeof pm, "vpt: StartForApplication=%d, pump running (%u ticks)",
                      sr, g_pump_ticks);
        clog_(pm);
    }

    const int vpt0 = cinder_effects_get_vpt_mode();
    const int dcp0 = cinder_effects_get_dc_phase_type();
    char m[192];
    std::snprintf(m, sizeof m, "vpt: on entry VptMode=%d DcPhaseFilterType=%d", vpt0, dcp0);
    clog_(m);
    if (vpt0 == -1) {
        clog_("vpt: no effects client — is the sound service up?");
        return 2;
    }

    cinder_effects_set_vpt(1);
    if (only >= 0) {
        cinder_effects_set_vpt_mode(only);
        const int rb = cinder_effects_get_vpt_mode();
        std::snprintf(m, sizeof m, "vpt: SetVptMode(%d) -> reads back %d%s",
                      only, rb, rb == only ? "  ACCEPTED" : "  REJECTED (clamped)");
        clog_(m);
        // HOLD, don't exit. The effect belongs to THIS process's EffectCtrlDmp client: returning
        // here tears the client down and the setting goes with it, so the first version was
        // unlistenable — "it doesn't last long enough for me to hear" (2026-08-17). Sit on the mode
        // with the pump still turning, re-asserting once a second because cinder-home's own
        // apply_sound_fn will overwrite it if the user touches anything on the Sound screen.
        std::snprintf(m, sizeof m, "vpt: HOLDING mode %d for %ds — listen now. Ctrl-C to stop early.",
                      only, hold_s);
        clog_(m);
        // Set ONCE and sit. The first version re-asserted every second to defend against
        // cinder-home's apply_sound_fn overwriting it — but re-applying a DSP effect mid-stream
        // reconfigures the pipeline, so the defence was itself audible. If something else does
        // steal the setting, the log below says so on the way out.
        for (int i = 0; i < hold_s; i++) sleep(1);
        const int held = cinder_effects_get_vpt_mode();
        if (held != only) {
            std::snprintf(m, sizeof m,
                          "vpt: mode was changed under us during the hold (%d -> %d) — something "
                          "else re-applied the sound chain", only, held);
            clog_(m);
        }
        cinder_effects_set_vpt_mode(vpt0);
        cinder_effects_set_vpt(0);
        std::snprintf(m, sizeof m, "vpt: hold over — restored VptMode=%d, VPT off",
                      cinder_effects_get_vpt_mode());
        clog_(m);
        g_pump_run = false;
        return 0;
    }

    for (int v = 0; v <= 7; v++) {
        cinder_effects_set_vpt_mode(v);
        const int rb = cinder_effects_get_vpt_mode();
        std::snprintf(m, sizeof m, "vpt:   VptMode %d -> %d%s", v, rb,
                      rb == v ? "   ACCEPTED" : "   rejected");
        clog_(m);
        sleep(3);   // long enough to actually hear the room change between steps
    }
    for (int t = 0; t <= 7; t++) {
        cinder_effects_set_dc_phase_type(t);
        const int rb = cinder_effects_get_dc_phase_type();
        std::snprintf(m, sizeof m, "vpt:   DcPhaseFilterType %d -> %d%s", t, rb,
                      rb == t ? "   ACCEPTED" : "   rejected");
        clog_(m);
    }
    // Put back exactly what was there, so a probe run is not a settings change.
    cinder_effects_set_vpt_mode(vpt0);
    cinder_effects_set_dc_phase_type(dcp0);
    cinder_effects_set_vpt(0);
    std::snprintf(m, sizeof m, "vpt: restored VptMode=%d DcPhaseFilterType=%d, VPT off",
                  cinder_effects_get_vpt_mode(), cinder_effects_get_dc_phase_type());
    clog_(m);
    g_pump_run = false;
    return 0;
}

static int eq_probe() {
    install_diagnostics();
    clog_("eq: Framework::GetReference() + StartForApplication …");
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    int sr = fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] eq: StartForApplication returned %d\n", sr);
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);

    // Same over-allocation care as the shim: the real object size is RE-confirmed in
    // cinder-audio/src/effect_abi.hpp, and the 2026-06-25 heap corruption came from under-sizing.
    static unsigned char obj[1024];
    std::memset(obj, 0, sizeof obj);
    clog_("eq: EffectCtrlDmp ctor …");
    wd_arm(12);
    _ZN3pst8services5sound13EffectCtrlDmpC1Ev(obj);
    wd_disarm();

    // Q2 first — it needs nothing set, so it reports the state STOCK left behind.
    wd_arm(10);
    int sel = _ZN3pst8services5sound13EffectCtrlDmp16GetSelectUsingEqEv(obj);
    int on  = _ZN3pst8services5sound13EffectCtrlDmp12IsEq10BandOnEv(obj);
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] eq: Q2 GetSelectUsingEq=%d  IsEq10BandOn=%d "
                 "(pump ticks %u)\n", sel, on, g_pump_ticks);
    if (g_pump_ticks == 0)
        clog_("eq: *** pump never ticked — nothing below is trustworthy ***");

    // Q1: set band 0 (32 Hz) across a ladder that brackets every plausible range and read back.
    // A value the DSP rejects comes back CLAMPED, which is exactly what identifies the range.
    void* dbfn = (void*)&_ZN3pst8services5sound13EffectCtrlDmp18GetEq10BandValuedBENS1_8Eq10BandE;
    typedef int   (*db_i)(void*, int);
    typedef float (*db_f)(void*, int);
    int saved = _ZN3pst8services5sound13EffectCtrlDmp16GetEq10BandValueENS1_8Eq10BandE(obj, 0);
    std::fprintf(stderr, "[cinder-probe] eq: band0 was %d — ladder (set -> raw / dB-as-int / "
                 "dB-as-float):\n", saved);
    _ZN3pst8services5sound13EffectCtrlDmp11SetEq10BandEb(obj, true);
    for (int v : { -20, -12, -10, -6, 0, 6, 10, 12, 20 }) {
        wd_arm(8);
        _ZN3pst8services5sound13EffectCtrlDmp16SetEq10BandValueENS1_8Eq10BandEi(obj, 0, v);
        int raw = _ZN3pst8services5sound13EffectCtrlDmp16GetEq10BandValueENS1_8Eq10BandE(obj, 0);
        int di  = ((db_i)dbfn)(obj, 0);
        float df = ((db_f)dbfn)(obj, 0);
        wd_disarm();
        std::fprintf(stderr, "    set %4d -> raw %4d   dB(int) %6d   dB(float) %8.3f%s\n",
                     v, raw, di, (double)df, raw == v ? "" : "   <-- CLAMPED");
    }
    wd_arm(8);
    _ZN3pst8services5sound13EffectCtrlDmp16SetEq10BandValueENS1_8Eq10BandEi(obj, 0, saved);
    wd_disarm();
    clog_("eq: band 0 restored");
    g_pump_run = false;
    std::fprintf(stderr, "[cinder-probe] eq: done (%u pump ticks)\n", g_pump_ticks);
    // _exit, not return: the pump thread is still inside libpstcore, and letting the process unwind
    // through static destructors while it is faults in the BT/effect libs. The measurement is
    // already printed by here, so the crash was pure noise on top of a good run — but noise in a
    // diagnostic's own output is exactly what makes the next one hard to read.
    std::fflush(nullptr);
    _exit(0);
}

// ── --bt : reconnect the last paired Bluetooth device ────────────────────────────────────────
//
// Headphones paired under stock stay paired under Cinder — the link key belongs to Sony's BT
// service, not to whichever app is foreground. What does NOT happen is the CONNECT: stock calls it
// at boot and Cinder calls nothing, so they sit paired and silent.
//
// `RequestLastDeviceConnection` (transmitter vtable slot 7) takes NO arguments — decompiled
// 2026-07-29, `void(this)`. So this needs no device address, no pairing UI and no
// BtCommonServiceClient. Same client the --ldac probe already proved constructs and responds.
//
// 2026-07-29 run 2: that connect returned quietly and the status never moved off 0. The pump was
// turning and the radio read ON, so the call was delivered and declined. First candidate for why:
// the --ldac path calls `SetCurrentSource(true)` and this one did not — the transmitter plausibly
// refuses to connect while it is not the current source. One extra call to test it.
//
// `GetConnectInformation(this, int*)` (slot 5, decompiled) is read-only and tells us whether a
// "last device" record exists at all; if SetCurrentSource does not fix it, that number is the
// evidence for the addressed-connect fallback (GetPairedDeviceInfo -> RequestConnection).
enum { VIDX_GetAvSrcConnectionStatus = 3, VIDX_GetConnectInformation = 5,
       VIDX_RequestLastDeviceConnection = 7, VIDX_RequestDisconnection = 8,
       VIDX_SetCurrentSourceBt = 12 };
// BtCommonServiceClient — the RADIO, which is a different service from the transmitter. Cinder's
// on/off toggle has always been UI-only (`SetRfOnOff` is never called), so the radio is in
// whatever state something else left it, and a connect against a powered-down radio would look
// exactly like the connect not working.
extern "C" void* _ZN3pst8services28BtCommonServiceClientFactory14CreateInstanceEv(void);
enum { VIDX_GetBtStatus = 3, VIDX_SetRfOnOff = 4 };

static int bt_probe(bool disconnect, bool cycle) {
    install_diagnostics();
    clog_("bt: Framework::GetReference() + StartForApplication …");
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    int sr = fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] bt: StartForApplication returned %d\n", sr);
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);
    std::fprintf(stderr, "[cinder-probe] bt: pump running (%u ticks)\n", g_pump_ticks);

    clog_("bt: BtTransmitterServiceClientFactory::CreateInstance() …");
    wd_arm(12);
    void* bt = _ZN3pst8services33BtTransmitterServiceClientFactory14CreateInstanceEv();
    wd_disarm();
    if (!bt) { clog_("bt: CreateInstance returned NULL — STOP"); return 1; }

    typedef int (*fn0)(void*);
    typedef void (*fnb)(void*, const bool*);

    // Radio first. Everything below is meaningless against a powered-down radio.
    clog_("bt: BtCommonServiceClientFactory::CreateInstance() …");
    wd_arm(12);
    void* cmn = _ZN3pst8services28BtCommonServiceClientFactory14CreateInstanceEv();
    wd_disarm();
    int rf = -1;
    if (cmn) {
        wd_arm(10); rf = ((fn0)vslot(cmn, VIDX_GetBtStatus))(cmn); wd_disarm();
        std::fprintf(stderr, "[cinder-probe] bt: GetBtStatus = %d\n", rf);
        if (rf == 0) {
            clog_("bt: radio reads OFF — SetRfOnOff(true) …");
            bool on = true;
            wd_arm(15); ((fnb)vslot(cmn, VIDX_SetRfOnOff))(cmn, &on); wd_disarm();
            for (int i = 0; i < 10; i++) {
                usleep(500000);
                wd_arm(10); rf = ((fn0)vslot(cmn, VIDX_GetBtStatus))(cmn); wd_disarm();
                if (rf != 0) break;
            }
            std::fprintf(stderr, "[cinder-probe] bt: GetBtStatus after power-on = %d\n", rf);
        }
    } else {
        clog_("bt: BtCommonServiceClient CreateInstance returned NULL");
    }

    // `cycle` exists to answer ONE question: is the BT middleware alive and acting on what we send
    // it? The connect path is silent all the way down — the service logs "last device found" and
    // then nothing, and MTK's stack (mtkbt + libBtMw, not BlueZ) logs nothing to any logcat buffer.
    // So a declined connect and a dead middleware look identical from outside.
    //
    // Powering the radio down and back up is the strongest liveness test available WITHOUT guessing
    // a new vtable signature: it reuses SetRfOnOff (slot 4) and GetBtStatus (slot 3), both already
    // exercised. If GetBtStatus moves 7 -> 0 -> 7 the middleware is provably responsive and the
    // connect failure lies with the peer or the profile, not with our call. If it does not move,
    // the stack is wedged and that is the bug.
    if (cycle && cmn) {
        typedef void (*fnb2)(void*, const bool*);
        bool off = false, on = true;
        clog_("bt: cycle: SetRfOnOff(false) …");
        wd_arm(15); ((fnb2)vslot(cmn, VIDX_SetRfOnOff))(cmn, &off); wd_disarm();
        int low = rf;
        for (int i = 0; i < 20; i++) {
            usleep(500000);
            wd_arm(10); low = ((fn0)vslot(cmn, VIDX_GetBtStatus))(cmn); wd_disarm();
            if (low != rf) break;
        }
        std::fprintf(stderr, "[cinder-probe] bt: cycle: status after off = %d%s\n", low,
                     low == rf ? "   <-- UNCHANGED: radio ignored the request" : "");
        clog_("bt: cycle: SetRfOnOff(true) …");
        wd_arm(15); ((fnb2)vslot(cmn, VIDX_SetRfOnOff))(cmn, &on); wd_disarm();
        int back = low;
        for (int i = 0; i < 30; i++) {
            usleep(500000);
            wd_arm(10); back = ((fn0)vslot(cmn, VIDX_GetBtStatus))(cmn); wd_disarm();
            if (back != low) break;
        }
        std::fprintf(stderr, "[cinder-probe] bt: cycle: status after on = %d%s\n", back,
                     back == low ? "   <-- STUCK: radio did not come back" : "");
        sleep(2);
    }

    wd_arm(10);
    int before = ((fn0)vslot(bt, VIDX_GetAvSrcConnectionStatus))(bt);
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] bt: AvSrc status before = %d\n", before);

    if (!disconnect) {
        // Candidate 1: become the current source before asking for a connection.
        clog_("bt: SetCurrentSource(true) …");
        bool t = true;
        wd_arm(15); ((fnb)vslot(bt, VIDX_SetCurrentSourceBt))(bt, &t); wd_disarm();
        usleep(500000);
    }

    if (disconnect) {
        clog_("bt: RequestDisconnection() …");
        wd_arm(15); ((fn0)vslot(bt, VIDX_RequestDisconnection))(bt); wd_disarm();
    } else {
        clog_("bt: RequestLastDeviceConnection() …");
        wd_arm(20); ((fn0)vslot(bt, VIDX_RequestLastDeviceConnection))(bt); wd_disarm();
    }
    // The connect is asynchronous — the reply and the state change both arrive on the looper, so
    // poll the status rather than reading it once and calling it a failure.
    for (int i = 0; i < 12; i++) {
        usleep(1000000);
        wd_arm(10);
        int now = ((fn0)vslot(bt, VIDX_GetAvSrcConnectionStatus))(bt);
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] bt: +%2ds status = %d%s\n", i + 1, now,
                     now != before ? "   <-- CHANGED" : "");
        if (now != before) break;
    }

    // NOT calling GetConnectInformation (slot 5). Two attempts crashed at a byte-identical fault
    // address inside `TransactionParam::GetStr(std::string&)`, which is what a *wrong out-param
    // shape* looks like, not a wrong slot. Reading the stub's marshalling settled it: 0xf7b0 makes
    // no `TransactionParam::Set*` call before the send (so it takes no input), and unpacks the
    // reply as Get, Get, GetStr, Get, Get — i.e. it fills a STRUCT containing a std::string at an
    // offset we have not recovered. Passing an `int*` or a bare `std::string*` lands the GetStr
    // write at a garbage offset inside that struct. Recover the layout before calling it again.
    //
    // It is not needed for the connect question anyway: logcat already proves the record exists —
    // `BtTransmitterService.cc:257  last device found [00:00:5E:00:53:01]`.

    g_pump_run = false;
    std::fprintf(stderr, "[cinder-probe] bt: done (%u pump ticks)\n", g_pump_ticks);
    std::fflush(nullptr);
    _exit(0);
}

// ── --dac : make the USB-DAC actually audible ────────────────────────────────────────────────
//
// The gadget half already works: `cinder-msc dac-on` flips `sys.sony.config` to `uac` and the PC
// enumerates a sound card. But nothing comes out of the 3.5 mm jack, because enumerating a gadget
// is not the same as playing through it — something has to tell Sony's player service to open the
// render path, and Cinder never did.
//
// That is `UsbDeviceAudioPlayerServiceClient::Start` (vtable slot 4). Its service-side signature is
// exported in full, which is a rare luxury here:
//
//   UsbDeviceAudioPlayerService::Start(IUsbDeviceAudioPlayerService::stream_info_t&)
//   UsbDeviceAudioPlayerService::GetStatus(IUsbDeviceAudioPlayerService::stream_info_t&)
//
// The ref is NON-const on both, so `stream_info_t` is an OUT param the service fills in. The client
// stub at 0x235a4 unpacks the reply with six plain `TransactionParam::Get` calls and NO `GetStr`,
// so every field is a scalar and a zeroed buffer is safe to hand it. (That distinction is not
// pedantry: it is exactly what made `BtTransmitterServiceClient::GetConnectInformation` crash —
// that one DOES contain a std::string, so a zeroed buffer put the GetStr write at a garbage
// offset. Check for GetStr before trusting a buffer to any out-param on this platform.)
extern "C" void* _ZN3pst8services40UsbDeviceAudioPlayerServiceClientFactory14CreateInstanceEv(void);
enum { VIDX_UacGetStatus = 3, VIDX_UacStart = 4, VIDX_UacStop = 5 };

static void dump_stream_info(const char* tag, const unsigned* si) {
    std::fprintf(stderr,
                 "[cinder-probe] dac: %s stream_info = %u %u %u %u %u %u  (%08x %08x %08x)\n",
                 tag, si[0], si[1], si[2], si[3], si[4], si[5], si[0], si[1], si[2]);
}

static int dac_probe(bool stop) {
    install_diagnostics();
    clog_("dac: Framework::GetReference() + StartForApplication …");
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    wd_arm(15);
    int sr = fw.StartForApplication(std::function<void()>(&pump_finish), true);
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] dac: StartForApplication returned %d\n", sr);
    g_pump_run = true;
    pthread_t pt;
    pthread_create(&pt, nullptr, pump_thread, &fw);
    for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);
    std::fprintf(stderr, "[cinder-probe] dac: pump running (%u ticks)\n", g_pump_ticks);
    if (g_pump_ticks == 0) {
        clog_("dac: pump never ticked — NOTHING below this line is trustworthy");
    }

    clog_("dac: UsbDeviceAudioPlayerServiceClientFactory::CreateInstance() …");
    wd_arm(12);
    void* uac = _ZN3pst8services40UsbDeviceAudioPlayerServiceClientFactory14CreateInstanceEv();
    wd_disarm();
    if (!uac) { clog_("dac: CreateInstance returned NULL — STOP"); return 1; }

    typedef void (*fnp)(void*, void*);
    // Oversized and zeroed: we know the field COUNT (six reads) but not the struct's true size,
    // and over-allocating an out-param buffer is free while under-allocating corrupts the stack.
    unsigned si[32];

    std::memset(si, 0, sizeof si);
    wd_arm(10); ((fnp)vslot(uac, VIDX_UacGetStatus))(uac, si); wd_disarm();
    dump_stream_info("before ", si);

    if (stop) {
        clog_("dac: Stop() …");
        std::memset(si, 0, sizeof si);
        wd_arm(15); ((fnp)vslot(uac, VIDX_UacStop))(uac, si); wd_disarm();
    } else {
        clog_("dac: Start() …");
        std::memset(si, 0, sizeof si);
        wd_arm(15); ((fnp)vslot(uac, VIDX_UacStart))(uac, si); wd_disarm();
        dump_stream_info("at Start", si);
    }

    for (int i = 0; i < 8; i++) {
        usleep(1000000);
        std::memset(si, 0, sizeof si);
        wd_arm(10); ((fnp)vslot(uac, VIDX_UacGetStatus))(uac, si); wd_disarm();
        std::fprintf(stderr, "[cinder-probe] dac: +%ds status = %u %u %u %u %u %u\n",
                     i + 1, si[0], si[1], si[2], si[3], si[4], si[5]);
    }

    g_pump_run = false;
    std::fprintf(stderr, "[cinder-probe] dac: done (%u pump ticks)\n", g_pump_ticks);
    std::fflush(nullptr);
    _exit(0);
}

static long fm_now_ms() {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (long)ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
}

// --fm btcap [kHz] : prove the Bluetooth bridge has a real source.
//
// The FM screen has a BT OUT button whose whole premise is that FM becomes PCM on hw:0,1, and that
// premise had never been tested — it was reasoned from the codec's `analog input device` mux having
// a `tuner` item. This is the controlled version: capture with the mux routed to the tuner, then
// with it OFF as a control, then routed again. If the route is what makes the difference, the
// middle number collapses. If all three are the same, the bridge would be transmitting whatever is
// on that ADC regardless of the radio, which is not the feature.
static int fm_btcap(int khz) {
    char m[224];
    int rc = cinder_tuner_start(khz);
    std::snprintf(m, sizeof m, "fm-btcap: start(%d kHz) rc=%d signal=%d", khz, rc,
                  cinder_tuner_signal());
    clog_(m);
    usleep(400000);

    int on1 = cinder_tuner_capture_rms(500);
    std::system("amixer -c0 cset numid=26 0 >/dev/null 2>&1");   // control: ADC source OFF
    usleep(300000);
    int off = cinder_tuner_capture_rms(500);
    std::system("amixer -c0 cset numid=26 1 >/dev/null 2>&1");   // routed again
    usleep(300000);
    int on2 = cinder_tuner_capture_rms(500);

    std::snprintf(m, sizeof m,
                  "fm-btcap: RMS  routed=%d  MUX-OFF(control)=%d  routed-again=%d", on1, off, on2);
    clog_(m);
    if (on1 > 0 && off >= 0 && on1 > off * 2 && on2 > off * 2)
        clog_("fm-btcap: VERDICT — the route is what carries it; hw:0,1 is a real FM source");
    else if (on1 <= 0)
        clog_("fm-btcap: VERDICT — capture failed to open; BT out cannot work");
    else
        clog_("fm-btcap: VERDICT — INCONCLUSIVE, control did not separate (see numbers above)");

    cinder_tuner_stop();
    return 0;
}

static int fm_regmon() {
    char m[256];
    std::snprintf(m, sizeof m, "fm-regmon: cinder_tuner_hw()=%d (1 = chip registers reachable)",
                  cinder_tuner_hw());
    clog_(m);
    if (!cinder_tuner_hw()) {
        clog_("fm-regmon: no register path — run /system/vendor/unknown321/bin/cinder-fm first");
        return 1;
    }

    // The chip has to be powered for its registers to describe anything, and Sony's Open() owns
    // that sequence — same reason cinder_tuner_scan brings the service up around a cold sweep.
    int rc = cinder_tuner_start(97300);
    std::snprintf(m, sizeof m, "fm-regmon: start rc=%d  freq=%d kHz  signal=%d  stereo=%d",
                  rc, cinder_tuner_get_khz(), cinder_tuner_signal(), cinder_tuner_stereo());
    clog_(m);

    int found[8] = {0};
    long t0 = fm_now_ms();
    int n = cinder_tuner_scan(87500, 108000, found, 8);
    long dt = fm_now_ms() - t0;
    std::snprintf(m, sizeof m, "fm-regmon: scan found %d station(s) in %ld ms", n, dt);
    clog_(m);
    for (int i = 0; i < n; i++) {
        cinder_tuner_set_khz(found[i]);
        usleep(120000);
        std::snprintf(m, sizeof m, "fm-regmon:   %6.1f MHz  signal=%2d  stereo=%d",
                      found[i] / 1000.0, cinder_tuner_signal(), cinder_tuner_stereo());
        clog_(m);
    }

    cinder_tuner_set_khz(87500);
    t0 = fm_now_ms();
    int hit = cinder_tuner_seek(87500, +1, nullptr);
    dt = fm_now_ms() - t0;
    std::snprintf(m, sizeof m, "fm-regmon: hw seek up from 87.5 -> %.1f MHz in %ld ms (0 = nothing)",
                  hit / 1000.0, dt);
    clog_(m);

    cinder_tuner_stop();
    clog_("fm-regmon: done (tuner stopped, chip left as the driver had it)");
    return 0;
}

// ── --scan : ask Sony's MediaStore to RE-SCAN the library ────────────────────────────────────
//
// WHY THIS EXISTS. `/db/MTPDB.dat` is written by Sony's MediaStoreService, and the thing that used
// to ASK it to rescan after music arrived was the stock Qt app that Cinder replaces. Cinder reads
// that SQLite file and has never called MediaStore at all, so music copied over USB-MSC stays
// invisible and deleted music stays listed. Measured 2026-08-26: 7 albums / 68 tracks pushed that
// evening were absent from a DB frozen at 16:49 the previous day, and 84 rows pointed at files
// that no longer existed. Both symptoms, one missing call.
//
// WHY IT IS SAFE TO TRY. Checklist rule 4 — never guess a vtable slot into a core Sony service —
// is satisfied outright here, because NOTHING below is guessed. `libMediaStoreServiceClient.so`
// exports the scanner as a CONCRETE class, so every entry point is a real dynamic symbol, taken
// verbatim from `nm -D`:
//
//   _ZN3pst8services10mediastore17MediaStoreService11GetInstanceEv
//   _ZN3pst8services12mediascanner12MediaScannerC1EP18IMediaStoreService
//   _ZN3pst8services12mediascanner12MediaScanner4ScanEPNS1_21IMediaScannerListenerENS_12mediascanner10language_tE
//   _ZN3pst8services12mediascanner12MediaScanner8ScanFileEPNS1_21IMediaScannerListenerERKNSt3__112basic_stringIcNS5_11char_traitsIcEENS5_9allocatorIcEEEENS_12mediascanner10language_tE
//   _ZN3pst8services12mediascanner12MediaScanner6CancelEv
//   _ZN3pst8services12mediascanner12MediaScannerD1Ev
//
// They are bound by `asm()` label rather than by re-declaring Sony's classes, deliberately: a
// hand-written class declaration has to reproduce the object LAYOUT correctly as well as the
// name, and getting that subtly wrong is how you corrupt a service's memory while every symbol
// still resolves. Binding the mangled name directly means the only thing that has to be right is
// the argument list, which the mangling states explicitly.
//
// THE ONE PIECE OF ABI THAT IS INFERRED, AND HOW IT WAS READ RATHER THAN GUESSED. A listener is
// an interface WE implement, so its vtable order is on us. It was read straight off the two call
// sites in the shipped library (objdump, addresses as-is in the 08-26 firmware):
//
//   MediaScanner::OnFinished  fed8: ldr r0,[r0,#8]   feda: cbz r0,...  fee4: ldr ip,[r2,#8]
//   MediaScanner::OnProgress  ff2c: ldr r0,[r0,#12]  ff2e: cbz r0,...  ff32: ldr ip,[r3,#12]
//
// so the user listener lives at MediaScanner+8 (finished) and +12 (progress), and is called
// through vtable byte offsets 8 and 12 — slots 2 and 3. With the standard Itanium two-slot
// destructor at 0 and 1, a class declared as { virtual ~L(); virtual OnFinished; virtual
// OnProgress; } lands exactly there, which is what Listener below is.
//
// AND THE SAFETY NET UNDER THAT: both call sites `cbz` the listener first. A NULL listener is
// explicitly handled by Sony's code, so `--scan go` can be run with no listener at all
// (`--scan go nolisten`) and the scan still happens, just silently. That is the fallback if the
// listener shape ever turns out to be wrong on another firmware.
//
// STAGING. Bare `--scan` INSPECTS ONLY: it brings the framework up, takes the singleton, and
// prints what it found without scanning anything. `--scan go` is the one that acts.
//
//   cinder-probe --scan                    inspect: singleton + inner proxy, no scan
//   cinder-probe --scan go [lang=N] [secs=N]  full library scan (default lang 0, watch 120 s)
//   cinder-probe --scan go nolisten         same, with a NULL listener (no progress output)
//   cinder-probe --scan file <path> [lang=N] rescan ONE file
//
// Cancel() is called on the way out of every path that started a scan, including the watchdog.

class IMediaStoreService;  // opaque: `P18IMediaStoreService` in the mangling is a GLOBAL-scope class

extern "C" {
void* ms_get_instance()
    asm("_ZN3pst8services10mediastore17MediaStoreService11GetInstanceEv");
void* msc_ctor(void* self, void* store)
    asm("_ZN3pst8services12mediascanner12MediaScannerC1EP18IMediaStoreService");
void  msc_dtor(void* self)
    asm("_ZN3pst8services12mediascanner12MediaScannerD1Ev");
int   msc_scan(void* self, void* listener, int lang)
    asm("_ZN3pst8services12mediascanner12MediaScanner4ScanEPNS1_21IMediaScannerListenerENS_12mediascanner10language_tE");
int   msc_scan_file(void* self, void* listener, const void* path_std_string, int lang)
    asm("_ZN3pst8services12mediascanner12MediaScanner8ScanFileEPNS1_21IMediaScannerListenerERKNSt3__112basic_stringIcNS5_11char_traitsIcEENS5_9allocatorIcEEEENS_12mediascanner10language_tE");
int   msc_cancel(void* self)
    asm("_ZN3pst8services12mediascanner12MediaScanner6CancelEv");
// Scan() itself passes an EMPTY root string to the service (the PC-relative literal at fdaa
// resolves to ""), so WHERE it scans has to come from the service's own configuration. The
// three-string GetConfig is the cheapest window onto that: all three are plain std::string
// out-params, so it can be called without modelling Sony's Config struct.
void  mss_get_config3(void* self, void* a, void* b, void* c)
    asm("_ZN3pst8services10mediastore17MediaStoreService9GetConfigERNSt3__112basic_stringIcNS3_11char_traitsIcEENS3_9allocatorIcEEEESA_SA_");
// The 3-string + category-dirs overload. Used in preference to SetConfig(Config const&) — which is
// what the stock app calls — because `Config` is an opaque struct whose layout would have to be
// reproduced exactly, and getting a member offset wrong writes through a core service's memory
// while every symbol still resolves. This overload takes the same fields as plain arguments, so
// the only thing that has to be right is the argument list, which the mangling states outright.
void  mss_set_config3(void* self, const void* a, const void* b, const void* c, const void* dirs)
    asm("_ZN3pst8services10mediastore17MediaStoreService9SetConfigERKNSt3__112basic_stringIcNS3_11char_traitsIcEENS3_9allocatorIcEEEESB_SB_RKNS3_6vectorIS9_NS7_IS9_EEEE");
}

// The user-side listener. Slot order is fixed by the two call sites quoted above; do not reorder.
namespace {
class ScanListener {
public:
    virtual ~ScanListener() {}
    virtual void OnFinished(int status) {          // vtable slot 2  (MediaScanner+8)
        std::fprintf(stderr, "[cinder-probe] scan: *** OnFinished(status=%d) ***\n", status);
        std::fflush(stderr);
        finished = true; last_status = status;
    }
    virtual void OnProgress(int done, int total) { // vtable slot 3  (MediaScanner+12)
        // Throttled to one line a second: a 3,400-track scan would otherwise emit thousands of
        // lines through a serial-speed stderr and slow down the very thing being measured.
        static long last_ms = 0;
        long now = fm_now_ms();
        ++progress_calls;
        if (now - last_ms < 1000 && done != total) return;
        last_ms = now;
        std::fprintf(stderr, "[cinder-probe] scan: progress %d/%d (%u callbacks so far)\n",
                     done, total, progress_calls);
        std::fflush(stderr);
    }
    volatile bool finished = false;
    volatile int  last_status = -1;
    volatile unsigned progress_calls = 0;
};
} // namespace

// The DB's whole observable state, so a scan that commits through a write-ahead log and leaves
// the main file's 2-second-granularity vfat mtime untouched is still seen as a change. Same
// reasoning (and the same failure) as main.cpp's db_signature().
static void scan_db_stat(const char* tag) {
    struct { const char* p; } files[] = {
        {"/db/MTPDB.dat"}, {"/db/MTPDB.dat-wal"}, {"/db/MTPDB.dat-journal"},
    };
    for (unsigned i = 0; i < sizeof files / sizeof files[0]; ++i) {
        struct stat st;
        if (::stat(files[i].p, &st) != 0) {
            if (i == 0) std::fprintf(stderr, "[cinder-probe] scan: %s %s ABSENT\n", tag, files[i].p);
            continue;
        }
        std::fprintf(stderr, "[cinder-probe] scan: %s %-22s size=%lld mtime=%ld ino=%lu\n",
                     tag, files[i].p, (long long)st.st_size, (long)st.st_mtime,
                     (unsigned long)st.st_ino);
    }
    std::fflush(stderr);
}

// --scan config: candidate MediaStore configuration to install before anything is scanned.
// SetConfig ON ITS OWN NEVER TOUCHES THE DATABASE — only Scan/ScanFile write. That is what makes
// guessing here safe to iterate: install a candidate, read it straight back with GetConfig, and
// judge it on the readback and on whether Scan stops returning 20. Nothing is scanned unless the
// command line also says `go`.
static const char* g_cfg_a = nullptr;
static const char* g_cfg_b = nullptr;
static const char* g_cfg_c = nullptr;
static const char* g_cfg_dirs = nullptr;   // comma-separated category directories
static bool g_cfg_set = false;

static int  g_scan_mode = 0;      // 0 = inspect only, 1 = full scan, 2 = single file
static int  g_scan_lang = 0;
static int  g_scan_secs = 120;
static bool g_scan_listen = true;
static const char* g_scan_path = nullptr;

static void scan_job_entry() {
    install_diagnostics();
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    std::fprintf(stderr, "[cinder-probe] scan: job running (fw=%p) — starting Pump() thread\n",
                 (void*)&fw);
    g_pump_run = true;
    pthread_t th;
    if (pthread_create(&th, nullptr, pump_thread, &fw) != 0) {
        clog_("scan: pthread_create FAILED"); _exit(1);
    }
    usleep(300000);
    std::fprintf(stderr, "[cinder-probe] scan: %u pump ticks before GetInstance\n", g_pump_ticks);

    scan_db_stat("before ");

    // MediaStoreService::GetInstance() is a lazy singleton: `new(28)` + ctor on first call, then
    // the cached pointer. It cannot fail without crashing inside Sony's code, so a null here means
    // the library did not load, not that the service is down.
    wd_arm(12);
    void* svc = ms_get_instance();
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] scan: MediaStoreService::GetInstance() = %p  "
                 "BinderLastError=%d\n", svc, pst::core::Framework::GetBinderLastError());
    if (!svc) { clog_("scan: no singleton — libMediaStoreServiceClient did not initialise"); _exit(2); }

    // MediaScanner's ctor takes the INNER binder proxy, not the singleton. Read off the singleton
    // at +4, which is where both GetMediaScanner (b842: ldr r0,[r5,#4]) and Scan itself
    // (fdd4: ldr r0,[r5,#4]) fetch the object they make the IPC call through. Taking it directly
    // avoids modelling GetMediaScanner()'s by-value (sret) return type, which the mangling does
    // not state and which would have to be guessed.
    void* proxy = *reinterpret_cast<void**>(reinterpret_cast<char*>(svc) + 4);
    std::fprintf(stderr, "[cinder-probe] scan: inner IMediaStoreService* (singleton+4) = %p\n", proxy);
    if (!proxy) { clog_("scan: inner proxy is NULL — the service side is not up"); _exit(3); }
    void* proxy_vptr = *reinterpret_cast<void**>(proxy);
    std::fprintf(stderr, "[cinder-probe] scan: proxy vptr = %p (non-null means a live C++ object)\n",
                 proxy_vptr);

    if (g_cfg_set) {
        std::string a(g_cfg_a ? g_cfg_a : ""), b(g_cfg_b ? g_cfg_b : ""), c(g_cfg_c ? g_cfg_c : "");
        std::vector<std::string> dirs;
        if (g_cfg_dirs) {
            const char* p = g_cfg_dirs;
            while (*p) {
                const char* comma = std::strchr(p, ',');
                if (comma) { dirs.push_back(std::string(p, (size_t)(comma - p))); p = comma + 1; }
                else { dirs.push_back(std::string(p)); break; }
            }
        }
        std::fprintf(stderr, "[cinder-probe] scan: SetConfig(\"%s\", \"%s\", \"%s\", %u dir(s):",
                     a.c_str(), b.c_str(), c.c_str(), (unsigned)dirs.size());
        for (size_t i = 0; i < dirs.size(); ++i) std::fprintf(stderr, " \"%s\"", dirs[i].c_str());
        std::fprintf(stderr, ") …\n");
        std::fflush(stderr);
        wd_arm(15);
        mss_set_config3(svc, &a, &b, &c, &dirs);
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] scan: SetConfig returned (void)  BinderLastError=%d\n",
                     pst::core::Framework::GetBinderLastError());
    }

    // What does the service think it is scanning? Scan() supplies no path of its own, so if these
    // come back empty the answer to "why did Scan return 20" is that nothing told MediaStore where
    // the music lives — which is a SetConfig problem, not a scanner problem.
    {
        std::string a, b, c;
        wd_arm(12);
        mss_get_config3(svc, &a, &b, &c);
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] scan: GetConfig -> [1]=\"%s\" [2]=\"%s\" [3]=\"%s\"\n",
                     a.c_str(), b.c_str(), c.c_str());
        std::fflush(stderr);
    }

    if (g_scan_mode == 0) {
        clog_("scan: INSPECT ONLY — nothing was scanned. Re-run with `--scan go` to actually scan.");
        _exit(0);
    }

    // 64 bytes for a 16-byte object (vptr, store, listener, listener — read from the ctor at
    // fd06/fd0e). Oversized on purpose and zeroed first: if a different firmware's MediaScanner
    // carries more state, it writes into slack instead of off the end of the buffer.
    static unsigned long long scanner_buf[8];
    std::memset(scanner_buf, 0, sizeof scanner_buf);
    void* scanner = scanner_buf;
    wd_arm(12);
    msc_ctor(scanner, proxy);
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] scan: MediaScanner constructed at %p (vptr=%p store=%p)\n",
                 scanner, ((void**)scanner)[0], ((void**)scanner)[1]);

    static ScanListener listener;
    void* lp = g_scan_listen ? static_cast<void*>(&listener) : nullptr;
    std::fprintf(stderr, "[cinder-probe] scan: listener = %p%s\n", lp,
                 lp ? "" : " (NULL — Sony's OnFinished/OnProgress cbz it, so this is handled)");

    int rc;
    if (g_scan_mode == 2) {
        std::string path(g_scan_path);
        std::fprintf(stderr, "[cinder-probe] scan: ScanFile(\"%s\", lang=%d) …\n",
                     path.c_str(), g_scan_lang);
        wd_arm(30);
        rc = msc_scan_file(scanner, lp, &path, g_scan_lang);
        wd_disarm();
    } else {
        std::fprintf(stderr, "[cinder-probe] scan: Scan(lang=%d) …\n", g_scan_lang);
        wd_arm(30);
        rc = msc_scan(scanner, lp, g_scan_lang);
        wd_disarm();
    }
    std::fprintf(stderr, "[cinder-probe] scan: call returned %d  BinderLastError=%d\n",
                 rc, pst::core::Framework::GetBinderLastError());

    // Watch. The call is ASYNC — it returns as soon as the request is queued, and the work lands
    // via the callbacks (or, with no listener, only in the DB). So poll the store's signature as
    // well: that is the evidence that does not depend on the listener ABI being right.
    for (int i = 1; i <= g_scan_secs && !listener.finished; ++i) {
        sleep(1);
        if (i % 10 == 0 || i == 1) {
            char tag[32];
            std::snprintf(tag, sizeof tag, "t+%-4d", i);
            scan_db_stat(tag);
            std::fprintf(stderr, "[cinder-probe] scan: t+%ds ticks=%u progress_calls=%u\n",
                         i, g_pump_ticks, listener.progress_calls);
        }
    }
    if (listener.finished)
        std::fprintf(stderr, "[cinder-probe] scan: FINISHED with status %d\n", listener.last_status);
    else
        std::fprintf(stderr, "[cinder-probe] scan: watch window (%d s) elapsed without OnFinished — "
                     "compare the signatures above to see whether the DB moved anyway\n", g_scan_secs);

    scan_db_stat("after  ");
    // Always Cancel: leaving a scan running in a core service after the probe exits is exactly the
    // kind of orphaned state that makes the NEXT test lie.
    wd_arm(10);
    msc_cancel(scanner);
    wd_disarm();
    clog_("scan: Cancel() sent; done");
    msc_dtor(scanner);
    _exit(listener.finished && listener.last_status == 0 ? 0 : 1);
}

static int scan_probe() {
    install_diagnostics();
    clog_("scan: Framework::GetReference() …");
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    std::fprintf(stderr, "[cinder-probe] scan: got Framework=%p BinderLastError=%d\n",
                 (void*)&fw, pst::core::Framework::GetBinderLastError());
    clog_("scan: StartForApplication(finish_job, true) …");
    int sr = fw.StartForApplication(std::function<void()>(&pump_finish), true);
    std::fprintf(stderr, "[cinder-probe] scan: StartForApplication returned %d\n", sr);
    scan_job_entry();
    return 0; // unreachable: scan_job_entry() _exit()s with the real status
}

int main(int argc, char** argv) {
    if (argc > 1 && std::strcmp(argv[1], "--scan") == 0) {
        // FLAT KEYWORD PARSE. Every argument after --scan is independent, so a config candidate and
        // a scan can be given in one line without the two branches disagreeing about which
        // positional slot means what. Bare `--scan` still inspects and scans NOTHING.
        for (int i = 2; i < argc; ++i) {
            if      (std::strcmp(argv[i], "go") == 0)        g_scan_mode = 1;
            else if (std::strcmp(argv[i], "nolisten") == 0)  g_scan_listen = false;
            else if (std::strcmp(argv[i], "file") == 0 && i + 1 < argc) {
                g_scan_mode = 2; g_scan_path = argv[++i];
            }
            else if (std::strncmp(argv[i], "lang=", 5) == 0) g_scan_lang = std::atoi(argv[i] + 5);
            else if (std::strncmp(argv[i], "secs=", 5) == 0) g_scan_secs = std::atoi(argv[i] + 5);
            else if (std::strncmp(argv[i], "a=", 2) == 0)    { g_cfg_a = argv[i] + 2; g_cfg_set = true; }
            else if (std::strncmp(argv[i], "b=", 2) == 0)    { g_cfg_b = argv[i] + 2; g_cfg_set = true; }
            else if (std::strncmp(argv[i], "c=", 2) == 0)    { g_cfg_c = argv[i] + 2; g_cfg_set = true; }
            else if (std::strncmp(argv[i], "dirs=", 5) == 0) { g_cfg_dirs = argv[i] + 5; g_cfg_set = true; }
            else std::fprintf(stderr, "[cinder-probe] scan: ignoring unrecognised argument \"%s\" "
                              "(use go, file <path>, lang=N, secs=N, nolisten, a= b= c= dirs=)\n",
                              argv[i]);
        }
        if (g_scan_secs <= 0) g_scan_secs = 120;
        return scan_probe();
    }
    if (argc > 1 && std::strcmp(argv[1], "--dac") == 0) {
        return dac_probe(argc > 2 && std::strcmp(argv[2], "stop") == 0);
    }
    if (argc > 1 && std::strcmp(argv[1], "--bt") == 0) {
        return bt_probe(argc > 2 && std::strcmp(argv[2], "off") == 0,
                        argc > 2 && std::strcmp(argv[2], "cycle") == 0);
    }
    if (argc > 1 && std::strcmp(argv[1], "--eqsel") == 0) {
        return eqsel_probe(argc > 2 ? std::atoi(argv[2]) : -1,
                           argc > 3 ? std::atoi(argv[3]) : 30);
    }
    if (argc > 1 && std::strcmp(argv[1], "--fx") == 0) {
        return fx_probe();
    }
    if (argc > 1 && std::strcmp(argv[1], "--tone") == 0) {
        return tone_probe();
    }
    if (argc > 1 && std::strcmp(argv[1], "--seqtime") == 0) {
        if (argc < 3) { clog_("seqtime: need a playable URI/path"); return 1; }
        return seqtime_probe(argv[2]);
    }
// --fm regmon [from_kHz] : exercise the SHIPPING code path — cinder_tuner_hw / _signal / _scan /
// _seek, i.e. exactly what the FM screen calls, rather than probe-local re-implementations.
//
// This is the on-device test for the register route: it needs /proc/regmon/Si4708icx readable, so
// run cinder-fm (or be root) first. Everything it calls degrades to the audio routes when that is
// missing, which is itself worth seeing.
    if (argc > 1 && std::strcmp(argv[1], "--fm") == 0) {
        // --fm                sweep the band, muted
        // --fm play           sweep with the tuner streaming
        // --fm tune <kHz> [s] hold ONE station so its audio can be captured/heard
        if (argc > 3 && std::strcmp(argv[2], "scan") == 0)
            return fm_scan(std::atoi(argv[3]), argc > 4 ? std::atoi(argv[4]) : 108000);
        if (argc > 2 && std::strcmp(argv[2], "i2c") == 0)
            return fm_i2c();
        if (argc > 2 && std::strcmp(argv[2], "regmon") == 0)
            return fm_regmon();
        if (argc > 2 && std::strcmp(argv[2], "btcap") == 0)
            return fm_btcap(argc > 3 ? std::atoi(argv[3]) : 98300);
        if (argc > 2 && std::strcmp(argv[2], "v4l2scan") == 0)
            return fm_v4l2scan(argc > 3 ? std::atoi(argv[3]) : 87500,
                               argc > 4 ? std::atoi(argv[4]) : 108000,
                               argc > 5 ? std::atoi(argv[5]) : 30,
                               argc > 6 && std::strcmp(argv[6], "power") == 0);
        if (argc > 2 && std::strcmp(argv[2], "v4l2") == 0)
            return fm_v4l2(argc > 3 ? std::atoi(argv[3]) : 97300,
                           argc > 4 ? std::atoi(argv[4]) : 104300);
        if (argc > 2 && std::strcmp(argv[2], "autotune") == 0)
            return fm_autotune(argc > 3 ? std::atoi(argv[3]) : 90000);
        if (argc > 2 && std::strcmp(argv[2], "seek") == 0)
            return fm_seek(argc > 3 ? std::atoi(argv[3]) : 87500);
        if (argc > 5 && std::strcmp(argv[2], "audioscan") == 0)
            return fm_audioscan(std::atoi(argv[3]), std::atoi(argv[4]), std::atoi(argv[5]));
        if (argc > 3 && std::strcmp(argv[2], "tune") == 0)
            return fm_tune(std::atoi(argv[3]), argc > 4 ? std::atoi(argv[4]) : 20,
                           argc > 5 ? argv[5] : "music");
        return fm_probe(argc > 2 && std::strcmp(argv[2], "play") == 0);
    }
    if (argc > 1 && std::strcmp(argv[1], "--tonefreq") == 0) {
        return tonefreq_probe();
    }
    if (argc > 1 && std::strcmp(argv[1], "--userpreset") == 0) {
        return userpreset_probe(argc > 2 && std::strcmp(argv[2], "--write") == 0);
    }
    if (argc > 1 && std::strcmp(argv[1], "--fxtime") == 0) {
        return fxtime_probe();
    }
    if (argc > 1 && std::strcmp(argv[1], "--eq6custom") == 0) {
        return eq6custom_probe();
    }
    if (argc > 1 && std::strcmp(argv[1], "--inpath") == 0) {
        return inpath_probe(argc > 2 ? std::atoi(argv[2]) : 2);
    }
    if (argc > 1 && std::strcmp(argv[1], "--vpt") == 0) {
        // --vpt            sweep every value, report the read-back, restore
        // --vpt <n> [secs]  HOLD mode n so it can be listened to (default 30 s)
        return vpt_probe(argc > 2 ? std::atoi(argv[2]) : -1,
                         argc > 3 ? std::atoi(argv[3]) : 30);
    }
    if (argc > 1 && std::strcmp(argv[1], "--eq") == 0) {
        return eq_probe();
    }
    if (argc > 1 && std::strcmp(argv[1], "--ldac") == 0) {
        return ldac_probe();
    }
    if (argc > 1 && std::strcmp(argv[1], "--btopen") == 0) {
        // --btopen [silence] [rate] [chans]. The default sends the handshake and nothing else;
        // "silence" is the explicit opt-in to writing PCM, and only after the accept gate passes.
        bool sil  = argc > 2 && std::strcmp(argv[2], "silence") == 0;
        bool tone = argc > 2 && std::strcmp(argv[2], "tone") == 0;
        unsigned rate  = argc > 3 ? (unsigned)std::atoi(argv[3]) : 44100u;
        unsigned chans = argc > 4 ? (unsigned)std::atoi(argv[4]) : 2u;
        unsigned secs  = argc > 5 ? (unsigned)std::atoi(argv[5]) : 3u;
        return btopen_probe(sil, tone, rate, chans, secs);
    }
    if (argc > 1 && std::strcmp(argv[1], "--fontchain") == 0) {
        return fontchain_probe(argc, argv);
    }
    if (argc > 1 && std::strcmp(argv[1], "--btinfo") == 0) {
        return btinfo_probe();
    }
    if (argc > 1 && std::strcmp(argv[1], "--nfctap") == 0) {
        return nfctap_probe(argc > 2 ? std::atoi(argv[2]) : 45);
    }
    if (argc > 1 && std::strcmp(argv[1], "--btlink") == 0) {
        // --btlink status | last [secs] | wait [secs] [keep] | retry on|off [interval] [count]
        //         | hci on|off | rssi [secs] | drop
        // An ADDRESSED connect is --btconnect <row>; this covers the calls that one does not make.
        const char* sub = argc > 2 ? argv[2] : "status";
        const char* a1  = argc > 3 ? argv[3] : nullptr;
        int a2 = argc > 4 ? std::atoi(argv[4]) : 0;
        int a3 = argc > 5 ? std::atoi(argv[5]) : 0;
        bool keep = false;
        for (int i = 3; i < argc; i++) if (std::strcmp(argv[i], "keep") == 0) keep = true;
        // "--btlink wait 45" and "--btlink rssi 20" put their seconds in argv[3], where the
        // on/off subcommands put a word; accept both without a second parser.
        if ((std::strcmp(sub, "wait") == 0 || std::strcmp(sub, "rssi") == 0 ||
             std::strcmp(sub, "last") == 0) && a1 && a2 == 0) a2 = std::atoi(a1);
        return btlink_probe(sub, a1, a2, a3, keep);
    }
    if (argc > 1 && std::strcmp(argv[1], "--uaccap") == 0) {
        return uaccap_probe(argc > 2 ? std::atoi(argv[2]) : 15,
                            argc > 3 ? std::atoi(argv[3]) : 0);
    }
    if (argc > 1 && std::strcmp(argv[1], "--uacgate") == 0) {
        // --uacgate [secs] [delay] [engage]  — engage=1 switches to Uac itself (restore child armed).
        return uacgate_probe(argc > 2 ? std::atoi(argv[2]) : 30,
                             argc > 3 ? std::atoi(argv[3]) : 0,
                             argc > 4 ? std::atoi(argv[4]) : 0);
    }
    if (argc > 1 && std::strcmp(argv[1], "--funcmode") == 0) {
        // --funcmode                      read-only: print FuncMode + connmgr + gadget. Safe.
        // --funcmode <n> [restore] [watch] EnterFuncMode(n), watch, then back to MediaPlay.
        //   n: 0 MediaPlay  1 UsbDac  2 A2dpSink  3 Fm  4 DirectRec  5 Dmr  6 Dms  7 Initial
        return funcmode_probe(argc > 2 ? std::atoi(argv[2]) : -1,
                              argc > 3 ? std::atoi(argv[3]) : 0,
                              argc > 4 ? std::atoi(argv[4]) : 0);
    }
    if (argc > 1 && std::strcmp(argv[1], "--usbdt") == 0) {
        // The raw switch, nothing else: --usbdt uac|msc|adb [restore_secs]. Read-only with no arg.
        unsigned want = 0;
        if (argc > 2 && std::strcmp(argv[2], "uac") == 0) want = USBDT_UAC;
        else if (argc > 2 && std::strcmp(argv[2], "msc") == 0) want = USBDT_MSC;
        else if (argc > 2 && std::strcmp(argv[2], "adb") == 0) want = USBDT_ADB;
        install_diagnostics();
        pst::core::Framework& fw = pst::core::Framework::GetReference();
        wd_arm(15);
        fw.StartForApplication(std::function<void()>(&pump_finish), true);
        wd_disarm();
        g_pump_run = true;
        pthread_t pt;
        pthread_create(&pt, nullptr, pump_thread, &fw);
        for (int i = 0; i < 50 && g_pump_ticks == 0; i++) usleep(10000);
        usbmgr_dump_gadget();
        if (want == 0) {
            clog_("usbdt: read-only. Re-run as --usbdt uac|msc|adb [restore_secs] to switch through "
                  "UsbDeviceConnectionService — the owner that also attaches the MSC medium and "
                  "fires the connect event the audio service waits on.");
            g_pump_run = false;
            std::fflush(nullptr);
            _exit(0);
        }
        int restore = argc > 3 ? std::atoi(argv[3]) : 90;
        if (restore > 0) {
            pid_t kid = fork();
            if (kid == 0) {
                signal(SIGHUP, SIG_IGN);
                signal(SIGINT, SIG_IGN);
                signal(SIGTERM, SIG_IGN);
                setsid();
                sleep((unsigned)restore);
                pst::core::Framework& cfw = pst::core::Framework::GetReference();
                cfw.StartForApplication(std::function<void()>(&pump_finish), true);
                usbdt_set(USBDT_ADB);
                _exit(0);
            }
            std::fprintf(stderr, "[cinder-probe] usbdt: restore child pid=%d armed — "
                                 "SetDeviceType(Adb) in %ds\n", (int)kid, restore);
        }
        usbdt_set(want);
        sleep(3);
        usbmgr_dump_gadget();
        g_pump_run = false;
        std::fflush(nullptr);
        _exit(0);
    }
    if (argc > 1 && std::strcmp(argv[1], "--usbmgr") == 0) {
        // No arg = READ-ONLY (always safe). "uac"/"msc" switch through the service and arm a
        // restore child; the optional third arg is how long before it puts the old function back.
        unsigned want = 0;
        if (argc > 2 && std::strcmp(argv[2], "uac") == 0) want = USBFN_UAC;
        else if (argc > 2 && std::strcmp(argv[2], "msc") == 0) want = USBFN_MSC;
        return usbmgr_probe(want, argc > 3 ? std::atoi(argv[3]) : 60);
    }
    if (argc > 1 && std::strcmp(argv[1], "--btvollisten") == 0) {
        return btvollisten_probe(argc > 2 ? std::atoi(argv[2]) : 12);
    }
    if (argc > 1 && std::strcmp(argv[1], "--btvolslot") == 0) {
        return btvolslot_probe(argc > 2 ? std::atoi(argv[2]) : 34,
                               argc > 3 ? std::atoi(argv[3]) : 40);
    }
    if (argc > 1 && std::strcmp(argv[1], "--btvol") == 0) {
        // No arg = sweep 20/60/100/40 absolute then three relative ups. An arg = that level once.
        return btvol_probe(argc > 2 ? std::atoi(argv[2]) : -1);
    }
    if (argc > 1 && std::strcmp(argv[1], "--pollnodes") == 0) {
        return pollnodes_probe(argc > 2 ? std::atoi(argv[2]) : 10);
    }
    if (argc > 1 && std::strcmp(argv[1], "--disp") == 0) {
        return disp_probe(0);   // read-only
    }
    if (argc > 1 && std::strcmp(argv[1], "--displight") == 0) {
        // --displight <level> [restore_secs]  — backlight only; LCD + touch stay valid.
        return displight_probe(argc > 2 ? (unsigned)std::strtoul(argv[2], nullptr, 10) : 0u,
                               argc > 3 ? std::atoi(argv[3]) : 20);
    }
    if (argc > 1 && std::strcmp(argv[1], "--dispoff") == 0) {
        // Seconds to hold the panel powered down (default 20). Arms a restore child first.
        return disp_probe(argc > 2 ? std::atoi(argv[2]) : 20);
    }
    if (argc > 1 && std::strcmp(argv[1], "--btrx") == 0) {
        return btrx_probe(argc > 2 ? std::atoi(argv[2]) : 40);
    }
    if (argc > 1 && std::strcmp(argv[1], "--btscan") == 0) {
        // Seconds to scan (default 20). Powers the radio up if needed and restores it.
        return btscan_probe(argc > 2 ? std::atoi(argv[2]) : 20);
    }
    if (argc > 1 && std::strcmp(argv[1], "--nfc") == 0) {
        // Seconds to wait for a tap (default 30).
        return nfc_probe(argc > 2 ? std::atoi(argv[2]) : 30,
                         argc > 3 ? (unsigned)std::atoi(argv[3]) : 0);
    }
    if (argc > 1 && std::strcmp(argv[1], "--btwho") == 0) {
        return btwho_probe();   // read-only; safe to run while audio is playing
    }
    if (argc > 1 && std::strcmp(argv[1], "--btconnect") == 0) {
        // Row index into the paired list (default 0). Run --btinfo first to see the rows.
        return btconnect_probe(argc > 2 ? std::atoi(argv[2]) : 0);
    }
    if (argc > 1 && std::strcmp(argv[1], "--analyzer") == 0) {
        int mode = argc > 2 ? std::atoi(argv[2]) : 1;
        return analyzer_probe(mode);
    }
    if (argc > 1 && std::strcmp(argv[1], "--discover") == 0) {
        // One-shot read-only device discovery → a report file you pull back. Inits PlayerService so
        // the PlayStatus byte dump works (play a track first), then captures everything + the keymap.
        const char* path = argc > 2 ? argv[2] : "/contents/cinder_discovery.txt";
        install_diagnostics();
        // The PlayStatus dump needs a LIVE PlayerService connection, and that needs the framework
        // pumped — same reason --pump exists and --play does not work. Without this the Connect
        // reply is never dispatched, every out-param stays uninitialised, and the dump comes back
        // all zeros. That was read for months as "nothing was playing"; it was never connected.
        clog_("discover: Framework::StartForApplication + Pump() (the dump needs a live link) …");
        pst::core::Framework& fw = pst::core::Framework::GetReference();
        int sr = fw.StartForApplication(std::function<void()>(&pump_finish), true);
        std::fprintf(stderr, "[cinder-probe] discover: StartForApplication returned %d\n", sr);
        g_pump_run = true;
        pthread_t pth;
        if (pthread_create(&pth, nullptr, pump_thread, &fw) != 0)
            clog_("discover: pump pthread_create FAILED — the PlayStatus dump will be zeros");
        usleep(300000);
        clog_("discover: connecting PlayerService (for the PlayStatus dump) …");
        // Own controller name: cinder-home is normally running and holds "cinder".
        wd_arm(12); int ai = cinder_audio_init("cinderdisc"); wd_disarm();
        int dwait = 0;
        while (!cinder_audio_is_connected() && dwait < 50) { usleep(100000); ++dwait; }
        std::fprintf(stderr, "[cinder-probe] discover: audio_init=%d IsConnected=%d after %d ms\n",
                     ai, cinder_audio_is_connected(), dwait * 100);
        // PlayStatus is PER-CONTROLLER: it reports what THIS client's controller was told to play,
        // not what the device is playing. Running a separate player alongside (--pump in another
        // shell, or the Home app) therefore dumps 128 zero bytes no matter how healthy the link is
        // — which is exactly what every dump before 2026-08-25 recorded. To get real bytes this
        // process has to own the playback, so take the media paths here and start them ourselves.
        //   --discover [report-path] [media paths…]
        if (argc > 3 && ai == 0) {
            wd_arm(8); cinder_audio_close_player(); wd_disarm();
            wd_arm(15);
            int dpr = cinder_audio_play_tracks(
                const_cast<const char* const*>(argv + 3), argc - 3, 0);
            wd_disarm();
            int dcur = -1, dtot = -1;
            sleep(2);                       // let the graph reach Play before the bytes are read
            cinder_audio_position(&dcur, &dtot);
            std::fprintf(stderr, "[cinder-probe] discover: play_tracks=%d pos=%d/%d\n",
                         dpr, dcur, dtot);
        } else if (argc <= 3) {
            clog_("discover: no media paths given — PlayStatus will be zeros. Pass them after the "
                  "report path: --discover /contents/cinder_discovery.txt /contents/MUSIC/…flac");
        }
        clog_("discover: capturing (amixer/asound/sysfs/usb/input + PlayStatus + 12s keymap) …");
        wd_arm(40);
        cinder_run_discovery(path, 1, 1);
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] discover: DONE — report at %s (pull it back)\n", path);
        // _exit, not return — see --tone/--fx: the pump thread is still inside libpstcore, and
        // unwinding through static destructors while it runs faults in the BT/effect libs.
        g_pump_run = false;
        std::fflush(nullptr);
        _exit(0);
    }
    if (argc > 1 && std::strcmp(argv[1], "--gpu") == 0) {
        // GPU present-path test in ISOLATION — no easel lifecycle, so it CANNOT trip the launcher's
        // bad-boot counter (unlike enabling the GPU in cinder-home itself, which is what wedged the
        // boot on 2026-07-26). cinder_render_init() honours CINDER_GPU=1 / /contents/cinder_gpu_on,
        // and gpu.rs refuses to enter EGL unless every required /dev node is accessible, so the
        // worst case here is a clean "GPU init failed" + software fallback.
        setenv("CINDER_GPU", "1", 1);
        install_diagnostics();
        clog_("gpu: cinder_render_init with CINDER_GPU=1 (watch for 'GPU present path active') …");
        wd_arm(20);
        int gr = cinder_render_init();
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] gpu: render_init returned %d\n", gr); std::fflush(stderr);
        if (gr != 0) { clog_("gpu: render init FAILED — see the error above"); return 1; }
        clog_("gpu: painting 120 frames (~2s at vsync) — the panel should show the Cinder UI …");
        for (int i = 0; i < 120; ++i) { wd_arm(8); cinder_render_tick(); wd_disarm(); }
        cinder_render_shutdown();
        clog_("gpu: DONE — no hang. Reboot to restore the normal UI.");
        return 0;
    }
    if (argc > 1 && std::strcmp(argv[1], "--requeue") == 0) {
        // DOES RE-ISSUING SetTrackSequence INTERRUPT THE CURRENT TRACK?
        //
        // This decides whether an Apple-style "Play Next" is even buildable. Cinder hands
        // PlayerService one NodeTrackSequence; inserting a track after the current one means
        // building a NEW sequence and calling SetTrackSequence again while audio is running. If
        // that restarts or stops the track, Play Next would stutter playback every single time and
        // the feature has to be designed completely differently (queue the insert until the track
        // ends, say). Better to know before designing than after.
        //
        // Method: play A, let it settle, sample the position; re-issue a sequence with an extra
        // track inserted after A, starting at the SAME index; sample again. A position that keeps
        // climbing means the re-issue was transparent.
        if (argc < 4) { clog_("requeue: need <playing-path> <insert-path>"); return 1; }
        g_pump_argc = argc; g_pump_argv = argv;
        install_diagnostics();
        pst::core::Framework& fw = pst::core::Framework::GetReference();
        int sr = fw.StartForApplication(std::function<void()>(&pump_finish), true);
        std::fprintf(stderr, "[cinder-probe] requeue: StartForApplication=%d\n", sr);
        g_pump_run = true;
        pthread_t th;
        if (pthread_create(&th, nullptr, pump_thread, &fw) != 0) { clog_("requeue: thread failed"); return 1; }
        usleep(300000);
        wd_arm(12); cinder_audio_init("cinderprobe"); wd_disarm();
        for (int i = 0; i < 50 && !cinder_audio_is_connected(); ++i) usleep(100000);
        wd_arm(8); cinder_audio_close_player(); wd_disarm();

        const char* first[1] = { argv[2] };
        wd_arm(15);
        int pr = cinder_audio_play_tracks(first, 1, 0);
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] requeue: initial play_tracks=%d\n", pr);
        int cur = -1, tot = -1;
        for (int i = 0; i < 16; ++i) { usleep(500000); cinder_audio_position(&cur, &tot); }
        std::fprintf(stderr, "[cinder-probe] requeue: BEFORE pos=%d/%d playing=%d\n",
                     cur, tot, cinder_audio_is_playing());
        int before = cur;

        // Re-issue: same first track, one inserted behind it. start index stays 0.
        const char* both[2] = { argv[2], argv[3] };
        wd_arm(15);
        int rr = cinder_audio_play_tracks(both, 2, 0);
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] requeue: re-issue play_tracks=%d\n", rr);
        for (int i = 0; i < 6; ++i) {
            usleep(500000);
            cinder_audio_position(&cur, &tot);
            std::fprintf(stderr, "[cinder-probe] requeue: +%.1fs pos=%d/%d playing=%d\n",
                         (i + 1) * 0.5, cur, tot, cinder_audio_is_playing());
        }
        std::fprintf(stderr,
            "[cinder-probe] requeue: VERDICT %s (before=%d after=%d)\n",
            (cur > before) ? "CONTINUED — re-issue is transparent, Play Next is buildable"
                           : "RESTARTED/STOPPED — Play Next must not re-issue mid-track",
            before, cur);
        wd_arm(8); cinder_audio_shutdown(); wd_disarm();
        _exit(0);
    }
    if (argc > 1 && std::strcmp(argv[1], "--pump") == 0) {
        // Same end-to-end playback test as --play, but with the pst::core::Framework STARTED and
        // PUMPED. --play proved the calls fail with the framework dead; if this passes, the dead
        // event loop is the root cause of "playback does nothing" and the fix goes in cinder-home.
        // The play test runs INSIDE StartForApplication's job so it executes with the framework
        // fully up, exactly like Sony's own apps.
        if (argc < 3) { clog_("pump: need at least one absolute media path"); return 1; }
        g_pump_argc = argc; g_pump_argv = argv;
        clog_("pump: Framework::GetReference() …");
        pst::core::Framework& fw = pst::core::Framework::GetReference();
        std::fprintf(stderr, "[cinder-probe] pump: got Framework=%p BinderLastError=%d\n",
                     (void*)&fw, pst::core::Framework::GetBinderLastError());
        // StartForApplication brings the framework up and RETURNS (the std::function is the
        // finish-job — cf. GetFinishJobFunc()); it is not a main loop. Driving the loop is
        // Pump()'s job, which is exactly what nothing in Cinder does today.
        clog_("pump: StartForApplication(finish_job, true) …");
        int sr = fw.StartForApplication(std::function<void()>(&pump_finish), true);
        std::fprintf(stderr, "[cinder-probe] pump: StartForApplication returned %d\n", sr);
        pump_job();
        _exit(0);
    }
    if (argc > 1 && std::strcmp(argv[1], "--repeatsweep") == 0) {
        if (argc < 3) { clog_("repeatsweep: need a media path (>20 s), optional single value"); return 1; }
        g_pump_argc = argc; g_pump_argv = argv;
        pst::core::Framework& fw = pst::core::Framework::GetReference();
        int sr = fw.StartForApplication(std::function<void()>(&pump_finish), true);
        std::fprintf(stderr, "[cinder-probe] repeatsweep: StartForApplication returned %d\n", sr);
        repeatsweep_job();
        _exit(0);
    }
    if (argc > 1 && std::strcmp(argv[1], "--transport") == 0) {
        // Phase-3 playback unknowns (play-by-index, seek origin, repeat-one, queue end). Same
        // framework-first shape as --pump — see transport_job.
        if (argc < 4) { clog_("transport: need TWO absolute media paths (>70 s each is ideal)"); return 1; }
        g_pump_argc = argc; g_pump_argv = argv;
        clog_("transport: Framework::StartForApplication(finish_job, true) …");
        pst::core::Framework& fw = pst::core::Framework::GetReference();
        int sr = fw.StartForApplication(std::function<void()>(&pump_finish), true);
        std::fprintf(stderr, "[cinder-probe] transport: StartForApplication returned %d\n", sr);
        transport_job();
        _exit(0);
    }
    if (argc > 1 && std::strcmp(argv[1], "--play") == 0) {
        // END-TO-END PLAYBACK TEST — the one thing qemu could never prove. Connects PlayerService
        // as an EXTRA client (own controller name; the service is multi-client by design — Sony's
        // own apps + scrobbler coexist) and hands it a real NodeTrackSequence built from the paths
        // on the command line. If audio comes out, the whole RE'd chain (JSON → Node →
        // NodeTrackSequence → SetTrackSequence → ChangePlayState) is verified without a flash and
        // without touching the running Home app — whose 1 Hz now-playing poll should visibly pick
        // the track up, which tests THAT path for free.
        //   --play <abs-path> [more paths…]     starts at the first path
        if (argc < 3) { clog_("play: need at least one absolute media path"); return 1; }
        install_diagnostics();
        clog_("play: cinder_audio_init(\"cinderprobe\") (PlayerService connect) …");
        wd_arm(12);
        int ai = cinder_audio_init("cinderprobe");
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] play: audio_init returned %d\n", ai);
        if (ai != 0) return 1;
        // Registration is ASYNC: the service ACKs the Connect on its own time, and calls made
        // before IsConnected flips are rejected (that was the entire first failure: rc -3 from
        // SetTrackSequence ~0 ms after Connect). Wait up to 5 s and say what happened.
        int waited = 0;
        while (!cinder_audio_is_connected() && waited < 50) { usleep(100000); ++waited; }
        std::fprintf(stderr, "[cinder-probe] play: IsConnected=%d after %d ms\n",
                     cinder_audio_is_connected(), waited * 100);
        clog_("play: SetTrackSequence + ChangePlayState(Play) …");
        wd_arm(15);
        int pr = cinder_audio_play_tracks(
            const_cast<const char* const*>(argv + 2), argc - 2, 0);
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] play: play_tracks returned %d\n", pr);
        // Poll ~6 s: URI non-empty proves the sequence was accepted; listener events + position
        // prove the RE'd PlayEventListener vtable is right (or dangerously wrong — watch for a
        // crash here); the raw dump gives bytes for further offset RE.
        char uri[512];
        for (int i = 1; i <= 6; ++i) {
            sleep(1);
            int cur = -1, tot = -1;
            cinder_audio_position(&cur, &tot);
            wd_arm(8);
            int n = cinder_audio_current_uri(uri, sizeof uri);
            wd_disarm();
            std::fprintf(stderr,
                         "[cinder-probe] play: t+%ds events=%u pos=%d/%d uri(%d)=%s\n",
                         i, cinder_audio_listener_events(), cur, tot, n, n > 0 ? uri : "(none)");
            std::fflush(stderr);
        }
        char dump[4096];
        wd_arm(8);
        if (cinder_audio_dump_status(dump, sizeof dump) > 0) std::fprintf(stderr, "%s", dump);
        wd_disarm();
        clog_("play: DONE — is audio playing? (left playing on purpose; pause from the UI)");
        std::fflush(stderr);
        // _exit, not return: static teardown (g_ctrl's deleter lives in Sony's lib) jumps through
        // a stale vtable and SIGSEGVs AFTER all results are printed. A diagnostic has nothing to
        // gain from running that teardown; the OS reclaims everything.
        _exit(pr == 0 ? 0 : 1);
    }
    if (argc > 1 && std::strcmp(argv[1], "--art") == 0) {
        // Album-art pipeline test. The art path only runs on a track change, so on a device that
        // has not played anything it leaves no trace in the log at all — this forces it.
        install_diagnostics();
        // ORDER MATTERS: cinder_db_open stores into the renderer's state and returns -2 if the
        // renderer isn't up yet.
        wd_arm(20); cinder_render_init(); wd_disarm();
        wd_arm(40); cinder_db_open("/db/MTPDB.dat"); wd_disarm();
        long long oid = argc > 2 ? std::atoll(argv[2]) : 0;
        wd_arm(30);
        int ar = cinder_art_probe(oid);
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] art: returned %d\n", ar);
        cinder_render_shutdown();
        return ar;
    }
    if (argc > 1 && std::strcmp(argv[1], "--artcache") == 0) {
        // Exercise the background cover-cache builder for N seconds (default 60) and report what
        // it managed. Standalone: it writes only /data/cinder/artcache, which the running Home app
        // also reads — a partially built cache is a valid cache, so this is safe to run live.
        int secs = argc > 2 ? std::atoi(argv[2]) : 60;
        install_diagnostics();
        wd_arm(20); cinder_render_init(); wd_disarm();
        wd_arm(60);
        int rc = cinder_db_open("/db/MTPDB.dat");   // starts the builder thread
        wd_disarm();
        if (rc != 0) { clog_("artcache: db_open FAILED"); return 1; }
        std::fprintf(stderr, "[cinder-probe] artcache: letting the builder run %d s …\n", secs);
        std::fflush(stderr);
        for (int i = 0; i < secs; ++i) { wd_arm(8); cinder_render_tick(); wd_disarm(); sleep(1); }
        clog_("artcache: DONE (count the files in /data/cinder/artcache)");
        return 0;
    }
    if (argc > 1 && std::strcmp(argv[1], "--bench") == 0) {
        // Frame-time bench, in isolation like --gpu. "Scrolling is choppy" can be a slow
        // rasterizer, a slow present, or a loop that just isn't repainting — this separates them.
        //   --bench           software present (what ships)
        //   --bench gpu       EGL present, for the A/B
        bool gpu = argc > 2 && std::strcmp(argv[2], "gpu") == 0;
        if (gpu) setenv("CINDER_GPU", "1", 1);
        install_diagnostics();
        wd_arm(20);
        int br = cinder_render_init();
        wd_disarm();
        if (br != 0) { clog_("bench: render init FAILED"); return 1; }
        // A real library makes the rasterizer do real work (rows of text + art blocks) instead of
        // an empty list. MUST come after render_init — cinder_db_open stores into the renderer's
        // state and returns -2 if it isn't up, which silently benched an empty screen.
        wd_arm(40);
        int bdb = cinder_db_open("/db/MTPDB.dat");
        wd_disarm();
        if (bdb != 0) { clog_("bench: db_open FAILED — numbers would be for an empty list"); return 1; }
        clog_(gpu ? "bench: 300 frames scrolling the library (GPU present) …"
                  : "bench: 300 frames scrolling the library (software present) …");
        wd_arm(60);
        cinder_render_bench(300, 3);
        wd_disarm();
        cinder_render_shutdown();
        clog_("bench: DONE");
        return 0;
    }
    clog_("start — isolating the suspect init calls (no easel lifecycle, no boot impact)");
    install_diagnostics();

    clog_("[1/4] cinder_render_init (open /dev/graphics/fb0) …");
    wd_arm(10);
    int r = cinder_render_init();
    wd_disarm();
    clog_(r == 0 ? "[1/4] render init OK" : "[1/4] render init FAILED (returned <0) — continuing");

    clog_("[2/4] cinder_db_open(/db/MTPDB.dat) + build library (slow on a big DB, not hung) …");
    wd_arm(40);
    int db = cinder_db_open("/db/MTPDB.dat");
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] [2/4] db_open returned %d\n", db); std::fflush(stderr);

    clog_("[3/4] cinder_audio_init(\"cinder\") (PlayerService connect — prime hang suspect) …");
    wd_arm(12);
    int au = cinder_audio_init("cinder");
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] [3/4] audio_init returned %d\n", au); std::fflush(stderr);

    clog_("[4/4] render + now-playing poll loop (6s) …");
    char uri[1024];
    for (int i = 0; i < 60; ++i) {
        wd_arm(8);
        cinder_render_tick();
        int n = cinder_audio_current_uri(uri, sizeof uri);
        wd_disarm();
        if (i == 0 || i == 30) {
            std::fprintf(stderr, "[cinder-probe] poll[%d]: current_uri len=%d uri='%s'\n",
                         i, n, n > 0 ? uri : "");
            std::fflush(stderr);
        }
        usleep(100000);
    }

    cinder_render_shutdown();
    clog_("DONE — every call returned with no hang. The hang is NOT in these calls.");
    return 0;
}
