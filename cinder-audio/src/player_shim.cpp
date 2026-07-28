// player_shim.cpp — C++ implementation of the cinder_audio.h C ABI. Wraps Sony's exported
// PlayerService client (libPlayerServiceClient.so). Built with clang -stdlib=libc++ and
// linked against the device libPlayerServiceClient/.../libc++ (same toolchain as cinder-home;
// see ../README.md). STATUS: built + linked into cinder-home/cinder-probe; control calls
// RE-verified; PlayStatus is read for the URI (+0x6c) — position/duration offsets pending the
// on-device hex dump (cinder_audio_dump_status, used by cinder-probe --discover).
#include "playerservice_abi.hpp"
#include "cinder_audio.h"

#include <atomic>
#include <cstdlib>
#include <functional>
#include <pthread.h>
#include <signal.h>
#include <time.h>
#include <memory>
#include <string>
#include <cstring>
#include <cstdio>

namespace ps = pst::services::playerservice;
namespace pl = pst::playservice;

namespace psu = pst::services::playerservice::util;

// libPlayerServiceClientUtil exports ONLY the base-object dtor (D2) for Node<UriInfo>, but our
// unique_ptr<Node>/delete emit complete-object (D1) calls. Node has no virtual bases, so D1==D2:
// define the mangled D1 as an extern "C" forwarder to Sony's exported D2.
extern "C" void _ZN3pst8services13playerservice4util4NodeINS2_7UriInfoEED2Ev(void*);
extern "C" void _ZN3pst8services13playerservice4util4NodeINS2_7UriInfoEED1Ev(void* p) {
    _ZN3pst8services13playerservice4util4NodeINS2_7UriInfoEED2Ev(p);
}

namespace pst { namespace core {
class Framework {
public:
    static Framework& GetReference();
    int  StartForApplication(std::function<void()> job, bool flag);
    bool Pump(bool short_timeout);
};
} }

namespace {
std::shared_ptr<ps::PlayController> g_ctrl;
// The active TrackSequence. SetTrackSequence ships only an int handle over IPC; the service
// PULLS tracks by calling back into this object (Alloc/AllocNext/OnNextTrack...) for as long
// as the sequence plays — so WE must keep it alive. Replaced (freeing the previous one) on
// every play_tracks call, dropped at shutdown.
std::shared_ptr<pl::TrackSequence> g_seq;
// The SAME object as g_seq, kept at its concrete type so repeat-one can be set on it. g_seq is an
// aliasing shared_ptr over this one, so there is exactly one object and one refcount.
std::shared_ptr<psu::NodeTrackSequence<psu::UriInfo>> g_nts;
// Sticky repeat-one preference. Applied to every sequence AT CONSTRUCTION (before the service is
// ever told about it, so nothing can be mid-read), and applied live when the user toggles it.
bool g_repeat_one = false;

// ── PlayEventListener (RE_playerservice_sound.md §2 — vtable MAPPED from the On* forwarders) ──
// Device result 2026-07-26: Connect(NULL) "poll mode" was a qemu-era assumption and it is WRONG —
// with a NULL listener the client never finishes registering (IsConnected stays false,
// GetCurrentStatus returns nonzero, SetTrackSequence is rejected). So we implement the listener.
// The controller stores it at this+0x30 and calls THROUGH ITS VTABLE only (no size assumption on
// our object beyond the vptr), with slots:
//   0,1 ~dtor(D1/D0)   2 onPlayStatusUpdated(uint, PlayStatus const&)
//   3 onPlayTimeUpdated(int cur_ms, int total_ms)   4 (unmapped)   5 onNextTrack(mode)
//   6 onPrevTrack(mode)
// A standalone clang/Itanium class with virtuals in exactly that order produces that vtable.
// Slots 7..10 are no-op padding: if the service ever forwards an unmapped event, it lands in a
// harmless empty virtual instead of past the end of the vtable. Callbacks arrive on a binder
// thread → everything they touch is atomic; readers poll the atomics.
std::atomic<int>      g_cb_pos_ms{-1};
std::atomic<int>      g_cb_dur_ms{-1};
std::atomic<unsigned> g_cb_state{0};
std::atomic<unsigned> g_cb_events{0};   // total callbacks seen — 0 = listener never fired
std::atomic<long long> g_cb_moved_at_ms{0};  // CLOCK_MONOTONIC ms when the position last changed

class CinderPlayListener {
public:
    virtual ~CinderPlayListener() {}                                         // slots 0,1
    virtual void onPlayStatusUpdated(unsigned state, const pl::PlayStatus&) { // slot 2
        g_cb_state.store(state, std::memory_order_relaxed);
        g_cb_events.fetch_add(1, std::memory_order_relaxed);
    }
    virtual void onPlayTimeUpdated(int currentMs, int totalMs) {              // slot 3
        // Note when the position last actually MOVED. The onPlayStatusUpdated state enum is not
        // calibrated yet (cinder_audio_play_state exposes it raw for that), and a moving position
        // is an unambiguous "really playing" regardless of what the enum turns out to mean:
        // the service keeps sending updates while paused, it just repeats the same value.
        if (currentMs != g_cb_pos_ms.load(std::memory_order_relaxed)) {
            struct timespec ts;
            clock_gettime(CLOCK_MONOTONIC, &ts);
            g_cb_moved_at_ms.store((long long)ts.tv_sec * 1000 + ts.tv_nsec / 1000000,
                                   std::memory_order_relaxed);
        }
        g_cb_pos_ms.store(currentMs, std::memory_order_relaxed);
        g_cb_dur_ms.store(totalMs, std::memory_order_relaxed);
        g_cb_events.fetch_add(1, std::memory_order_relaxed);
    }
    virtual void onUnmapped4() {}                                             // slot 4
    virtual void onNextTrack(int) { g_cb_events.fetch_add(1, std::memory_order_relaxed); } // 5
    virtual void onPrevTrack(int) { g_cb_events.fetch_add(1, std::memory_order_relaxed); } // 6
    virtual void pad7() {}
    virtual void pad8() {}
    virtual void pad9() {}
    virtual void pad10() {}
};
CinderPlayListener g_listener;

// ── pst::core::Framework pump (THE fix for "playback does nothing", 2026-07-27) ───────────────
// Sony's client proxies are asynchronous: a call marshals a request and the REPLY is delivered by
// pst::core::Framework's event looper. Nothing in Cinder ever drove that looper — main.cpp's own
// comment notes easel's pump never fires for a non-Qt CuiAppModule, but the consequence was
// missed: every PlayerService out-param stayed UNINITIALISED. Connect "returned" a 0xb6xxxxxx
// pointer, IsConnected read uninitialised stack as true, SetTrackSequence "failed with 99" — all
// garbage, and the service logged nothing because no transaction ever completed.
// Driving Framework::Pump() makes the same calls return real values (Connect rc=0,
// SetTrackSequence OK, listener callbacks with real position/duration).
// Wampy's pstserver does exactly this (artifacts/repos/wampy/pstserver/main.cpp): GetReference,
// StartForApplication, then Pump in a loop on its own thread.
//
// We do NOT call StartForApplication here: in cinder-home easel::ApplicationBase::run already
// constructs an easel::Framework, which calls it. Calling GetReference BEFORE that happens
// returns a zero-initialised singleton and Pump() then segfaults (proved in cinder-probe), so
// start this only once the app lifecycle is up — cinder-home starts it from deferred_up().

std::atomic<unsigned> g_pump_ticks{0};
volatile bool g_pump_run = false;
pthread_t     g_pump_th = 0;
int           g_pump_interval_ms = 20;

// Is the Framework actually STARTED? GetReference() is a Meyers singleton that happily hands back
// a zero-initialised object if nothing called StartForApplication yet, and Pump() then dereferences
// its event queue and segfaults (cinder-probe reproduced exactly that: SIGSEGV at addr 0x14).
// Pump reads that queue from `this+0x38` (disasm libpstcore @0x1e6a0 `ldr r0, [r4, #0x38]`), so a
// null there is the precise "not started yet" signal.
//
// This guard exists because a crash on THIS thread is not survivable the way the rest of our Sony
// calls are: run_guarded wraps the *spawn*, not the thread body, so a segfault here kills
// cinder-home, which feeds the launcher's bad-boot counter and eventually reverts the device to
// stock. Not pumping degrades to "no playback"; crashing costs the user a manual recovery.
// If the offset is ever wrong the check simply stops discriminating — it cannot make things worse.
bool framework_started(pst::core::Framework& fw) {
    void* queue = *reinterpret_cast<void* const*>(reinterpret_cast<const char*>(&fw) + 0x38);
    return queue != nullptr;
}

void* pump_main(void*) {
    // SIGALRM belongs to cinder-home's render worker (per-frame watchdog + run_guarded); a guard
    // alarm delivered here would fail its owner check and _exit the process.
    sigset_t sa; sigemptyset(&sa); sigaddset(&sa, SIGALRM);
    pthread_sigmask(SIG_BLOCK, &sa, nullptr);
    pst::core::Framework& fw = pst::core::Framework::GetReference();
    // Wait (up to ~5 s) for the app lifecycle to have started it, then refuse rather than crash.
    for (int i = 0; i < 50 && g_pump_run && !framework_started(fw); ++i) {
        struct timespec w; w.tv_sec = 0; w.tv_nsec = 100000000L;
        nanosleep(&w, nullptr);
    }
    // Log the probed word either way: if playback is ever dead with "never started" in the log,
    // this one line says whether the +0x38 guess went stale rather than leaving it a mystery.
    std::fprintf(stderr, "[cinder-audio] pump: Framework=%p queue@+0x38=%p\n", (void*)&fw,
                 *reinterpret_cast<void* const*>(reinterpret_cast<const char*>(&fw) + 0x38));
    if (!framework_started(fw)) {
        std::fprintf(stderr, "[cinder-audio] pump: Framework never started — NOT pumping "
                             "(playback + progress will be unavailable, but we stay alive)\n");
        g_pump_run = false;
        return nullptr;
    }
    while (g_pump_run) {
        fw.Pump(true);
        g_pump_ticks.fetch_add(1, std::memory_order_relaxed);
        // Re-read the interval every iteration so the shell can slow us down when the panel goes
        // dark (cinder_audio_pump_set_interval). Pump(true) returns immediately when idle — without
        // a sleep it spins a core flat (measured ~380k calls/s in cinder-probe).
        int ms = g_pump_interval_ms;
        if (ms < 1) ms = 1;
        struct timespec ts;
        ts.tv_sec  = ms / 1000;
        ts.tv_nsec = (long)(ms % 1000) * 1000000L;
        nanosleep(&ts, nullptr);
    }
    return nullptr;
}

inline int change_state(pl::playstate_t s) {
    if (!g_ctrl) return -1;
    return g_ctrl->ChangePlayState(s);
}

// Minimal JSON string escaping for the Node schema (paths can contain " \ and control chars).
void json_escape_into(std::string& out, const char* s) {
    for (; *s; ++s) {
        unsigned char c = static_cast<unsigned char>(*s);
        if (c == '"' || c == '\\') { out += '\\'; out += static_cast<char>(c); }
        else if (c < 0x20) { char b[8]; std::snprintf(b, sizeof b, "\\u%04x", c); out += b; }
        else out += static_cast<char>(c);
    }
}
} // namespace

extern "C" {

int cinder_audio_init(const char* name) {
    ps::PlayerService* svc = ps::PlayerService::GetInstance();
    if (!svc) return -1;
    g_ctrl = svc->getPlayController(name ? name : "cinder");
    if (!g_ctrl) return -2;
    // Real listener by default (Connect(NULL) never completes registration — see the listener
    // block above). CINDER_NOLISTENER=1 keeps the old NULL connect for on-device A/B via probe.
    pst::playservice::PlayEventListener* l = nullptr;
    const char* nol = getenv("CINDER_NOLISTENER");
    if (!(nol && nol[0] == '1'))
        l = reinterpret_cast<pst::playservice::PlayEventListener*>(&g_listener);
    // Connect's return is meaningful ONLY with the framework pump running: the wrapper reads an
    // out-param the reply fills, and returns 0 on success / that out-param on failure (disasm
    // @0x2f32: `if (out == 0) { this->listener = l; return 0; }`). With no pump the reply never
    // arrives and this is uninitialised stack. It is also the gate on the listener actually being
    // registered, so a nonzero rc means no position callbacks will ever fire — retry rather than
    // reporting success, which is what the old `Connect(nullptr); return 0;` did.
    int rc = -1;
    for (int attempt = 0; attempt < 5; ++attempt) {
        if (attempt) {
            struct timespec ts; ts.tv_sec = 0; ts.tv_nsec = 200000000L;  // 200 ms backoff
            nanosleep(&ts, nullptr);
        }
        rc = g_ctrl->Connect(l);
        std::fprintf(stderr, "[cinder-audio] Connect(%s) attempt=%d rc=%d\n",
                     l ? "listener" : "NULL", attempt, rc);
        if (rc == 0) break;
    }
    if (rc != 0) {
        std::fprintf(stderr, "[cinder-audio] Connect FAILED (rc=%d) — no transport, no listener. "
                             "Is the Framework pump running?\n", rc);
        g_ctrl.reset();
        return -3;
    }
    return 0;
}

int cinder_audio_pump_start(int interval_ms) {
    if (g_pump_run) return 0;
    if (const char* off = getenv("CINDER_NOPUMP")) if (off[0] == '1') return -2;
    if (interval_ms > 0) g_pump_interval_ms = interval_ms;
    g_pump_run = true;
    if (pthread_create(&g_pump_th, nullptr, pump_main, nullptr) != 0) {
        g_pump_run = false;
        std::fprintf(stderr, "[cinder-audio] pump: pthread_create FAILED\n");
        return -1;
    }
    std::fprintf(stderr, "[cinder-audio] pump: started (%d ms interval)\n", g_pump_interval_ms);
    return 0;
}

void cinder_audio_pump_stop(void) {
    if (!g_pump_run) return;
    g_pump_run = false;
    if (g_pump_th) { pthread_join(g_pump_th, nullptr); g_pump_th = 0; }
}

void cinder_audio_pump_set_interval(int interval_ms) {
    if (interval_ms > 0) g_pump_interval_ms = interval_ms;
}

unsigned cinder_audio_pump_ticks(void) {
    return g_pump_ticks.load(std::memory_order_relaxed);
}

int cinder_audio_is_connected(void) {
    return g_ctrl && g_ctrl->IsConnected() ? 1 : 0;
}

int cinder_audio_position(int* cur_ms, int* total_ms) {
    if (cur_ms)   *cur_ms   = g_cb_pos_ms.load(std::memory_order_relaxed);
    if (total_ms) *total_ms = g_cb_dur_ms.load(std::memory_order_relaxed);
    return g_cb_pos_ms.load(std::memory_order_relaxed) >= 0 ? 1 : 0;
}

unsigned cinder_audio_listener_events(void) {
    return g_cb_events.load(std::memory_order_relaxed);
}

unsigned cinder_audio_play_state(void) {
    return g_cb_state.load(std::memory_order_relaxed);
}

int cinder_audio_is_playing(void) {
    // "The position moved recently." onPlayTimeUpdated lands ~1x/sec, so 2500 ms tolerates one
    // dropped update without flickering the transport glyph. This is deliberately derived from
    // observed motion rather than from onPlayStatusUpdated's state int, whose encoding is not
    // calibrated — see cinder_audio_play_state.
    long long moved = g_cb_moved_at_ms.load(std::memory_order_relaxed);
    if (moved == 0) return 0;
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    long long now = (long long)ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
    return (now - moved) < 2500 ? 1 : 0;
}

int cinder_audio_resume(void) {
    if (!g_ctrl) return -1;
    int rc = g_ctrl->Resume();
    std::fprintf(stderr, "[cinder-audio] Resume rc=%d\n", rc);
    return rc;
}

int cinder_audio_suspend(void) {
    if (!g_ctrl) return -1;
    return g_ctrl->Suspend();
}

int cinder_audio_close_player(void) {
    if (!g_ctrl) return -1;
    int rc = g_ctrl->ClosePlayer();
    std::fprintf(stderr, "[cinder-audio] ClosePlayer rc=%d\n", rc);
    return rc;
}

void cinder_audio_shutdown(void) {
    // ClosePlayer BEFORE Disconnect: it releases the service-side player and with it the
    // SoundService "Music" track. Skipping it leaks that track inside hagodaemon (which outlives
    // us), and the next process to try to play gets "Cannot create multiple tracks that have
    // same type" -> WMX_AudioOutput::Open() 0x80001009 -> a loaded track that never makes sound.
    if (g_ctrl) { g_ctrl->ClosePlayer(); g_ctrl->Disconnect(); }
    g_ctrl.reset();
    g_seq.reset();
    // BOTH handles refer to the same object; dropping only one keeps it alive and would leave
    // cinder_audio_set_repeat_one writing into a sequence the service has already released.
    g_nts.reset();
}

int cinder_audio_play_tracks(const char* const* uris, int count, int start) {
    if (!g_ctrl || !uris || count <= 0) return -1;
    if (start < 0 || start >= count) start = 0;

    // Build the Node-tree JSON: a root container whose children are the track leaves, in play
    // order. "format" is the int Sony's own psk::FileUtil::GetFormatFromFilename maps from the
    // path (-1 = unsupported; the root container's format is never consulted, use -1 too).
    std::string json;
    json.reserve(64 + static_cast<size_t>(count) * 96);
    json += "{\"uri\":\"/\",\"format\":-1,\"children\":[";
    for (int i = 0; i < count; ++i) {
        if (!uris[i]) return -1;
        int fmt = psu::psk::FileUtil::GetFormatFromFilename(std::string(uris[i]));
        if (i) json += ',';
        json += "{\"uri\":\"";
        json_escape_into(json, uris[i]);
        json += "\",\"format\":";
        char n[16]; std::snprintf(n, sizeof n, "%d", fmt);
        json += n;
        json += '}';
    }
    json += "]}";

    psu::NodeJsonUtil<psu::UriInfo, psu::UriInfoPolicy> jsonUtil;
    std::unique_ptr<psu::Node<psu::UriInfo>> node = jsonUtil.ConvJsonStringToNode(json);
    if (!node) return -2;

    auto nts = std::make_shared<psu::NodeTrackSequence<psu::UriInfo>>(
        std::move(node), start,
        std::function<void(psu::UpdateReason, int)>([](psu::UpdateReason, int) {}));
    // Upcast to the TrackSequence base WITHOUT pointer adjustment: the C1 ctor disasm stores a
    // single primary vtable at object+0, so the base subobject is at offset 0. Aliasing
    // shared_ptr shares nts's control block (destruction still runs ~NodeTrackSequence).
    std::shared_ptr<pl::TrackSequence> seq(nts, reinterpret_cast<pl::TrackSequence*>(nts.get()));

    // Keep the sequence alive BEFORE handing it over. SetTrackSequence ships only
    // {playerId, startIndex, 0} over IPC (disasm @0x3245) — our object is never sent; the service
    // pulls tracks by calling back through the controller, which holds the seq at +0x38. So the
    // object must outlive the call, and a rejected call must not leave a dangling one either.
    g_seq = seq;
    g_nts = nts;
    // Apply the sticky repeat mode BEFORE SetTrackSequence: at this point the service has never
    // seen this object, so there is no reader to race with.
    nts->SetOneTrackMode(g_repeat_one ? psu::OneTrackMode::On : psu::OneTrackMode::Off);
    int rc = g_ctrl->SetTrackSequence(seq);
    if (rc != 0) {
        // The raw service code, not a flattened -3: this is a wire reject and its value is the
        // only thing that distinguishes "player busy" from "bad sequence" from "not connected".
        std::fprintf(stderr, "[cinder-audio] SetTrackSequence REJECTED rc=%d (0x%08x) "
                             "connected=%d count=%d start=%d\n",
                     rc, (unsigned)rc, (int)g_ctrl->IsConnected(), count, start);
        g_seq.reset();
        g_nts.reset();
        return -3;
    }
    // PAUSE, then PLAY — the OMX lifecycle, not an optimisation. Measured 2026-07-27:
    // SetTrackSequence leaves the graph at OMX_StateIdle (demuxer open, renderer Loaded->Idle).
    // ChangePlayState(Pause) takes Idle -> OMX_StatePause and is where SoundService actually
    // creates the Music track. Only from Pause is Executing reachable; going straight from Idle
    // to Play is what produced the binder error + reboot on the first attempt.
    // CINDER_PLAYSTATE overrides the prepare value for re-calibration.
    pl::playstate_t prep = pl::playstate_t::Pause;
    if (const char* ov = getenv("CINDER_PLAYSTATE"))
        prep = static_cast<pl::playstate_t>(atoi(ov));
    int pr = g_ctrl->ChangePlayState(prep);
    std::fprintf(stderr, "[cinder-audio] SetTrackSequence OK; prepare ChangePlayState(%d) rc=%d\n",
                 (int)prep, pr);
    if (pr != 0) return pr;
    int rr = g_ctrl->ChangePlayState(pl::playstate_t::Play);
    std::fprintf(stderr, "[cinder-audio] ChangePlayState(Play=2) rc=%d\n", rr);
    return rr;
}

int cinder_audio_play(void)  { return change_state(pl::playstate_t::Play); }
int cinder_audio_pause(void) { return change_state(pl::playstate_t::Pause); }
int cinder_audio_stop(void)  { return change_state(pl::playstate_t::Stop); }

// Stop playback AND drop our pinned track sequence, so PlayerService releases the current
// track's file descriptor. Needed before handing /contents to the PC over USB-MSC: a paused
// service keeps the media file open, and any open fd under /contents makes init's
// unmount_msc1 fail EBUSY → the LUN write fails → the PC sees a reader with no medium.
int cinder_audio_set_repeat_one(int on) {
    g_repeat_one = (on != 0);
    if (!g_nts) return 1;  // no sequence yet — it will be applied when the next one is built
    // Live change on a sequence the service is already pulling from. This is a single enum store
    // into an object we own; it is not synchronised, and it is the one part of this path that
    // wants a device to confirm rather than an argument. If it ever misbehaves, the fallback is to
    // drop this call and let the sticky flag apply from the next track onward.
    g_nts->SetOneTrackMode(g_repeat_one ? psu::OneTrackMode::On : psu::OneTrackMode::Off);
    return 0;
}

int cinder_audio_release_sequence(void) {
    int rc = change_state(pl::playstate_t::Stop);
    g_seq.reset();
    g_nts.reset();
    return rc;
}

int cinder_audio_next_track(void) { return g_ctrl ? g_ctrl->NextTrack() : -1; }
int cinder_audio_prev_track(void) { return g_ctrl ? g_ctrl->PrevTrack(nullptr) : -1; }
int cinder_audio_next_group(void) { return g_ctrl ? g_ctrl->NextGroup() : -1; }
int cinder_audio_prev_group(void) { return g_ctrl ? g_ctrl->PrevGroup(nullptr) : -1; }

int cinder_audio_seek_ms(int ms) {
    if (!g_ctrl) return -1;
    return g_ctrl->SeekTime(pl::media_origin_t::Begin, ms);
}

int cinder_audio_current_uri(char* buf, int cap) {
    if (!g_ctrl || !buf || cap <= 0) return -1;
    pl::PlayStatus st{};
    if (g_ctrl->GetCurrentStatus(st) != 0) return -2;
    // PlayStatus.uri is a real libc++ std::string at offset +0x6c — CONFIRMED by RE'ing
    // PlayerService::ConverPlayStatus (analysis/RE_playerservice_sound.md §1: dst+0x6c =
    // std::string assigned from the wire URI). GetCurrentStatus fills it, so it's a valid string
    // (empty SSO if no track; heap-allocated if the URI > ~22 chars).
    std::string* uri =
        reinterpret_cast<std::string*>(reinterpret_cast<char*>(&st) + 0x6c);
    int n = static_cast<int>(uri->size());
    if (n >= cap) n = cap - 1;
    if (n < 0) n = 0;
    std::memcpy(buf, uri->data(), static_cast<size_t>(n));
    buf[n] = '\0';
    // We model PlayStatus as an opaque blob (no dtor runs at scope exit), so a long (heap)
    // URI string would LEAK every poll (~1/s). Destruct it explicitly to free that heap.
    // (Safe for the empty-SSO case too — no-op.)
    uri->~basic_string();
    return n;
}

int cinder_audio_dump_status(char* buf, int cap) {
    if (!g_ctrl || !buf || cap < 8) return -1;
    pl::PlayStatus st{};
    if (g_ctrl->GetCurrentStatus(st) != 0) return -2;
    // Hex-dump the first 128 bytes of the filled struct (the int fields — playstate/position/
    // duration/track — live in 0..0x6c; the URI std::string is at +0x6c). 16 bytes per line with
    // the offset, so position/duration ms values can be matched by eye on device.
    const unsigned char* p = reinterpret_cast<const unsigned char*>(&st);
    int n = 0;
    for (int row = 0; row < 128; row += 16) {
        if (n > cap - 60) break;
        n += std::snprintf(buf + n, cap - n, "+0x%02x:", row);
        for (int i = 0; i < 16; ++i) n += std::snprintf(buf + n, cap - n, " %02x", p[row + i]);
        if (n < cap - 1) buf[n++] = '\n';
    }
    buf[n < cap ? n : cap - 1] = '\0';
    // Free the heap URI string at +0x6c (same leak fix as current_uri).
    auto* uri = reinterpret_cast<std::string*>(reinterpret_cast<char*>(&st) + 0x6c);
    uri->~basic_string();
    return n;
}

} // extern "C"
