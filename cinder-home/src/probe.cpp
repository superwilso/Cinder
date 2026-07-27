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
#include "discover.h"
#include <cstdio>
#include <cstdlib>
#include <csignal>
#include <cstring>
#include <ucontext.h>
#include <unistd.h>
#include <execinfo.h>
#include <initializer_list>
#include <pthread.h>
#include <functional>

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
    while (g_pump_run) { fw->Pump(true); ++g_pump_ticks; }
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
static int analyzer_probe(int mode) {
    install_diagnostics();
    clog_("analyzer: cinder_render_init (so set_spectrum has a target; not strictly required) …");
    wd_arm(10); cinder_render_init(); wd_disarm();

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
        wd_arm(8); cinder_render_tick(); wd_disarm();
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
    cinder_render_shutdown();
    if (total > 0) {
        clog_("analyzer: PASS — frames flowed. Calibrate from_bands to the printed range, then enable "
              "in the shell via /contents/cinder_viz.conf (analyzer=1).");
        return 0;
    }
    clog_("analyzer: started but NO frames — try the other mode (--analyzer 0), confirm audio is "
          "playing, or the service emits only while its own screen is foregrounded.");
    return 2;
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

int main(int argc, char** argv) {
    if (argc > 1 && std::strcmp(argv[1], "--analyzer") == 0) {
        int mode = argc > 2 ? std::atoi(argv[2]) : 1;
        return analyzer_probe(mode);
    }
    if (argc > 1 && std::strcmp(argv[1], "--discover") == 0) {
        // One-shot read-only device discovery → a report file you pull back. Inits PlayerService so
        // the PlayStatus byte dump works (play a track first), then captures everything + the keymap.
        const char* path = argc > 2 ? argv[2] : "/contents/cinder_discovery.txt";
        install_diagnostics();
        clog_("discover: connecting PlayerService (for the PlayStatus dump) …");
        wd_arm(12); cinder_audio_init("cinder"); wd_disarm();
        clog_("discover: capturing (amixer/asound/sysfs/usb/input + PlayStatus + 12s keymap) …");
        wd_arm(40);
        cinder_run_discovery(path, 1, 1);
        wd_disarm();
        std::fprintf(stderr, "[cinder-probe] discover: DONE — report at %s (pull it back)\n", path);
        return 0;
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
