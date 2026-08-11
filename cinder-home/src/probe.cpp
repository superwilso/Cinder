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
#include <sys/socket.h>
#include <sys/un.h>
#include <alsa/asoundlib.h>

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
static_assert(sizeof(BtPairedDeviceInformation) == 48, "paired-device stride is not 48");

// Format a BD address for the log. Empty in → "(none)".
static void mac_str(const std::vector<unsigned char>& a, char* out, size_t cap) {
    if (a.empty()) { std::snprintf(out, cap, "(none)"); return; }
    size_t n = 0;
    for (size_t b = 0; b < a.size() && n + 4 < cap; b++)
        n += std::snprintf(out + n, cap - n, b ? ":%02X" : "%02X", a[b]);
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
    }
    virtual void OnUnknownTag(const void*) {                        // slot 3
        taps++;
        clog_("nfc: OnUnknownTag — a tag was read but it is not a Bluetooth OOB record");
    }
    virtual void OnHostCardEmulation(const void*) {                 // slot 4
        taps++;
        clog_("nfc: OnHostCardEmulation");
    }
    int taps = 0;
};

static int nfc_probe(int secs) {
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

    unsigned zero = 0;
    int rc_start = -1;
    wd_arm(12);
    try { rc_start = ((fnu)vslot(nfc, VIDX_Start1))(nfc, &zero); }
    catch (...) { clog_("nfc: Start (slot 5) threw"); }
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] nfc: Start(0) slot5 rc=%d\n", rc_start);

    int mode1 = -1;
    wd_arm(10);
    try { mode1 = ((fn0)vslot(nfc, VIDX_GetCurrentMode))(nfc); } catch (...) {}
    wd_disarm();
    std::fprintf(stderr, "[cinder-probe] nfc: GetCurrentMode (after) = %d%s\n", mode1,
                 mode1 != mode0 ? "  <== the mode CHANGED, so Open/Start took effect" : "");

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
    // `BtTransmitterService.cc:257  last device found [AC:80:0A:56:A9:91]`.

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

int main(int argc, char** argv) {
    if (argc > 1 && std::strcmp(argv[1], "--dac") == 0) {
        return dac_probe(argc > 2 && std::strcmp(argv[2], "stop") == 0);
    }
    if (argc > 1 && std::strcmp(argv[1], "--bt") == 0) {
        return bt_probe(argc > 2 && std::strcmp(argv[2], "off") == 0,
                        argc > 2 && std::strcmp(argv[2], "cycle") == 0);
    }
    if (argc > 1 && std::strcmp(argv[1], "--eq") == 0) {
        return eq_probe();
    }
    if (argc > 1 && std::strcmp(argv[1], "--ldac") == 0) {
        return ldac_probe();
    }
    if (argc > 1 && std::strcmp(argv[1], "--btinfo") == 0) {
        return btinfo_probe();
    }
    if (argc > 1 && std::strcmp(argv[1], "--pollnodes") == 0) {
        return pollnodes_probe(argc > 2 ? std::atoi(argv[2]) : 10);
    }
    if (argc > 1 && std::strcmp(argv[1], "--disp") == 0) {
        return disp_probe(0);   // read-only
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
        return nfc_probe(argc > 2 ? std::atoi(argv[2]) : 30);
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
