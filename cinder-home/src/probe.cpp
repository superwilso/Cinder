// cinder-probe — standalone, ZERO-BOOT-RISK diagnostic.
//
// It runs ONLY the suspect cinder-home init calls (framebuffer open, library DB load,
// PlayerService connect, a render+poll loop) in ISOLATION — it does NOT do the easel/appmgr
// lifecycle, so it does NOT register as the Home app and CANNOT affect boot. The stock UI keeps
// running; the probe just briefly touches /dev/graphics/fb0 (cosmetic flicker) and connects to
// PlayerService as an extra client. Every call is watchdog-bounded: on a hang it logs the exact
// PC + backtrace + maps and exits, so we learn precisely which call blocks WITHOUT a flash.
//
// Run it from a shell on the device in NORMAL boot (stock UI up — needed so PlayerService is
// running), e.g. over adb:
//   adb push cinder-home/dist/cinder-probe /data/local/tmp/
//   adb shell 'cd /data/local/tmp && \
//     LD_LIBRARY_PATH=/system/vendor/sony/lib:/system/vendor/unknown321/lib:/system/lib:/usr/lib:/lib \
//     ./cinder-probe'
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

static void clog_(const char* m) { std::fprintf(stderr, "[cinder-probe] %s\n", m); std::fflush(stderr); }

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
