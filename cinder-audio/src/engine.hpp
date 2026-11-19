// engine.hpp — Cinder's own playback engine (docs/PLAN_hardware_first.md, Phase 3).
//
// Included by player_shim.cpp after its anonymous namespace: it shares that file's callback
// atomics (g_cb_pos_ms, g_cb_dur_ms, g_cb_moved_at_ms, g_cb_events, g_halted_at_ms), so to the rest
// of Cinder a song played here looks exactly like one PlayerService played.
//
// WHAT IT DOES. A queue in which EVERY file is FLAC is decoded by cinder-ffi's own decoder
// (src/flac.rs) and written into Cinder's own track in Sony's sound service — CreateTrack(type 0),
// the way AudioInPlayerService feeds the radio — so Sony's effects, volume and Bluetooth routing
// all still apply (device-verified 2026-11-18). Any other queue stays with PlayerService.
// ponytail: all-FLAC queues only; a mixed queue (FLAC + MP3) goes to PlayerService whole. Hand over
// per song if mixed libraries need the engine.
//
// Songs of one format follow each other in one track: gapless. A change of rate or depth waits for
// the queue to drain and opens a new track. Position is wall-clock time since the audio started
// flowing, capped at what was written, which tracks the ear within the service's small queue.
//
// THE TRAP. Sony's sound service never frees a track whose process died without deleting it, and
// while it exists no other player can make sound (two tracks of one type are refused). Every
// deliberate exit deletes it (cinder_audio_shutdown / close_player / release_sequence stop the
// engine); a crash of cinder-home ends in a reboot anyway (fault_handler), which clears it.
// Off unless the shell enables it (`own_engine` in cinder_advanced.conf).

#include <dlfcn.h>
#include <pthread.h>
#include <signal.h>

#include <strings.h>
#include <unistd.h>

#include <algorithm>
#include <chrono>
#include <condition_variable>
#include <deque>
#include <mutex>

extern "C" {
void* cinder_flac_open(const char* path);
void cinder_flac_info(const void* h, unsigned* rate, unsigned* channels, unsigned* bits,
                      unsigned long long* total);
long cinder_flac_read(void* h, int* out, size_t frames);
int cinder_flac_seek(void* h, unsigned long long sample);
void cinder_flac_close(void* h);
}

namespace eng {

static long long now_ms() {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (long long)ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
}

// ── the sound-service track (ABI: analysis notes in PLAN_hardware_first.md, Phase 3) ──────────
typedef void (*ss_ctor_fn)(void*);
typedef void (*ss_create_fn)(void** out, void* self, int type, const void* param, int* err,
                             const std::vector<std::string>* names);
struct TrackBuffer { const void* ptr; unsigned size; };

struct Track {
    void* t = nullptr;
    unsigned rate = 0, bits = 0, bytes = 0, period = 8192;
};

static ss_create_fn s_create = nullptr;
alignas(8) static unsigned char s_service[256];

static void** vt(const Track& k) { return *(void***)k.t; }
static int call0(const Track& k, unsigned off) { return ((int (*)(void*))vt(k)[off / 4])(k.t); }

static bool track_open(Track& k, unsigned rate, unsigned bits) {
    if (!s_create) {
        void* lib = dlopen("libSoundService.so", RTLD_NOW);
        ss_ctor_fn ctor = lib ? (ss_ctor_fn)dlsym(lib, "_ZN3pst8services5sound12SoundServiceC1Ev") : nullptr;
        s_create = lib ? (ss_create_fn)dlsym(lib,
            "_ZN3pst8services5sound12SoundService11CreateTrackENS1_9TrackTypeERKNS1_10TrackParamEPNS1_"
            "3ErrERKNSt3__16vectorINS9_12basic_stringIcNS9_11char_traitsIcEENS9_9allocatorIcEEEENSE_ISG_EEEE")
            : nullptr;
        if (!ctor || !s_create) {
            s_create = nullptr;
            std::fprintf(stderr, "[cinder-audio] engine: libSoundService.so unusable\n");
            return false;
        }
        ctor(s_service);
    }
    const unsigned bytes = bits <= 16 ? 2 : bits <= 24 ? 3 : 4;
    int param[25] = {0};
    param[0] = (int)rate;  param[1] = (int)(bytes * 8);  param[2] = 2;  param[3] = 2;
    param[10] = 1;  param[11] = (int)bytes;  param[12] = 1;  param[13] = 1;  param[14] = 3;
    const std::vector<std::string> names;
    int err = -1;
    void* t = nullptr;
    s_create(&t, s_service, 0, param, &err, &names);
    if (!t || err != 0) {
        std::fprintf(stderr, "[cinder-audio] engine: CreateTrack(%u Hz, %u-bit) refused err=%d "
                             "(another player holds the music track?)\n", rate, bits, err);
        return false;
    }
    k.t = t;
    unsigned env[16] = {0};
    if (call0(k, 0x08) != 0 || ((int (*)(void*, void*))vt(k)[0x0c / 4])(t, env) != 0 || call0(k, 0x10) != 0) {
        std::fprintf(stderr, "[cinder-audio] engine: track Init/Start failed\n");
        call0(k, 0x30);
        ((void (*)(void*))vt(k)[0x04 / 4])(t);
        k.t = nullptr;
        return false;
    }
    k.rate = rate;
    k.bits = bits;
    k.bytes = bytes;
    const unsigned fb = bytes * 2;
    k.period = env[0] >= fb ? env[0] / fb * fb : 8192 / fb * fb;
    std::fprintf(stderr, "[cinder-audio] engine: track open %u Hz %u-bit, period %u bytes\n", rate, bits, k.period);
    return true;
}

static void track_close(Track& k) {
    if (!k.t) return;
    call0(k, 0x2c);   // Drop
    call0(k, 0x30);   // Stop
    ((void (*)(void*))vt(k)[0x04 / 4])(k.t);   // delete: the service destroys the track
    k.t = nullptr;
    std::fprintf(stderr, "[cinder-audio] engine: track closed\n");
}

// Throw away what is queued and run again (a seek, a skip, a resume).
static void track_flush(Track& k) {
    if (!k.t) return;
    call0(k, 0x2c);   // Drop: leaves the track stopped
    call0(k, 0x10);   // Start
}

static bool track_write(Track& k, const unsigned char* p, unsigned n) {
    while (n) {
        TrackBuffer b = { p, n < k.period ? n : k.period };
        unsigned done = 0;
        if (((int (*)(void*, const TrackBuffer*, unsigned*))vt(k)[0x14 / 4])(k.t, &b, &done) != 0) return false;
        if (!done) { usleep(5000); continue; }
        p += done;
        n -= done;
    }
    return true;
}

// ── the engine ────────────────────────────────────────────────────────────────────────────────
enum Cmd { NONE, PLAY_AT, PAUSE, RESUME, STOP };

struct Song { long long at_written_ms; int idx; long long offset_ms; long long dur_ms; };

struct Engine {
    std::mutex mu;
    std::condition_variable cv;
    pthread_t th = 0;
    bool thread_up = false;
    // Shared, under mu.
    std::vector<std::string> uris;
    int want_idx = 0;
    long long want_ms = 0;
    Cmd cmd = NONE;
    bool repeat_one = false;
    int crossfade_ms = 0;   // 0 = gapless, the default
    // The audible song, for current_uri; under mu.
    int audible_idx = -1;
    bool active = false;    // a queue is loaded
    bool playing = false;
    // A song the engine cannot play (not FLAC) was reached: PlayerService takes the queue from
    // here. Set by the engine thread, acted on by the shell's thread (cinder_audio_engine_poll),
    // because Sony's player is driven from that thread only.
    int handover_idx = -1;
    long long handover_ms = 0;
};

static Engine E;
static bool s_enabled = false;

static bool is_flac(const char* u) {
    const size_t n = std::strlen(u);
    return n > 5 && strcasecmp(u + n - 5, ".flac") == 0;
}

// The engine takes a queue whose first song is FLAC; at the first song that is not, it hands the
// queue to PlayerService (handover_idx). ponytail: one way only — PlayerService keeps a queue it
// was handed, FLAC songs after it included. Hand back if long mixed queues need the engine.
static bool takes(const char* const* uris, int count, int start) {
    return s_enabled && start >= 0 && start < count && uris[start] && is_flac(uris[start]);
}

static void publish_pos(long long pos_ms, long long dur_ms) {
    const int p = (int)pos_ms;
    if (p != g_cb_pos_ms.load(std::memory_order_relaxed)) g_cb_moved_at_ms.store(now_ms(), std::memory_order_relaxed);
    g_cb_pos_ms.store(p, std::memory_order_relaxed);
    g_cb_dur_ms.store((int)dur_ms, std::memory_order_relaxed);
    g_cb_events.fetch_add(1, std::memory_order_relaxed);
}

// Up to `frames` frames from `d` as interleaved stereo in `buf` (2 × frames ints): mono plays on
// both sides.
static long read_stereo(void* d, unsigned ch, int* buf, long frames) {
    const long n = cinder_flac_read(d, buf, (size_t)frames);
    if (n > 0 && ch == 1)
        for (long i = n - 1; i >= 0; i--) buf[2 * i] = buf[2 * i + 1] = buf[i];
    return n;
}

static void* run(void*) {
    sigset_t sa;
    sigemptyset(&sa);
    sigaddset(&sa, SIGALRM);   // the shell's render watchdog owns SIGALRM
    pthread_sigmask(SIG_BLOCK, &sa, nullptr);

    Track k;
    void* dec = nullptr;
    int dec_idx = -1;
    unsigned rate = 0, ch = 0, bits = 0;
    unsigned long long total = 0, dpos = 0;   // dpos: frames of this song decoded so far
    // Crossfade: the next song's decoder, mixed in over this song's last `fade_len` frames.
    void* dec2 = nullptr;
    int dec2_idx = -1, fade_tried = -1;
    unsigned ch2 = 0;
    unsigned long long total2 = 0, d2pos = 0;
    long long fade_len = 0, fade_done = 0;
    static int pcm[2048 * 2], pcm2[2048 * 2];
    static unsigned char out[2048 * 2 * 4];
    // Timing: wf = frames written to the track since the last flush (frames, not ms: 2048 frames
    // is 46.44 ms and rounding each block would drift); t0 = when that started playing (wall
    // clock); songs = where each song begins, in written ms.
    long long wf = 0, t0 = 0, paused_at = 0;
    auto written_ms = [&]() -> long long { return k.rate ? wf * 1000 / k.rate : 0; };
    auto dur_of = [&]() -> long long { return total && rate ? (long long)(total * 1000 / rate) : 0; };
    std::deque<Song> songs;
    bool playing = false;
    long long last_pub = 0;

    auto played = [&]() -> long long {
        const long long w = (playing ? now_ms() : paused_at) - t0, wr = written_ms();
        return w < 0 ? 0 : w > wr ? wr : w;
    };
    auto audible = [&]() -> Song {
        const long long p = played();
        while (songs.size() > 1 && songs[1].at_written_ms <= p) songs.pop_front();
        return songs.empty() ? Song{0, -1, 0, 0} : songs.front();
    };
    auto close_dec2 = [&]() { if (dec2) { cinder_flac_close(dec2); dec2 = nullptr; dec2_idx = -1; } };
    auto close_dec = [&]() { close_dec2(); if (dec) { cinder_flac_close(dec); dec = nullptr; dec_idx = -1; } };
    auto open_dec = [&](int idx, const std::string& u) -> bool {
        close_dec();
        dec = cinder_flac_open(u.c_str());
        if (!dec) return false;
        cinder_flac_info(dec, &rate, &ch, &bits, &total);
        if (ch != 1 && ch != 2) { close_dec(); return false; }   // mono plays on both sides
        dec_idx = idx;
        dpos = 0;
        fade_tried = -1;
        return true;
    };

    std::unique_lock<std::mutex> lk(E.mu);
    // Called with lk held and the track already closed: PlayerService takes the queue at `idx`.
    auto hand_over = [&](int idx, long long ms) {
        std::fprintf(stderr, "[cinder-audio] engine: #%d is not FLAC — PlayerService takes the queue\n", idx);
        E.handover_idx = idx;
        E.handover_ms = ms;
        E.active = false;
        E.playing = playing = false;
        E.audible_idx = -1;
        songs.clear();
        E.cv.notify_all();
    };
    for (;;) {
        // ── commands ──
        if (E.cmd != NONE) {
            const Cmd c = E.cmd;
            E.cmd = NONE;
            if (c == STOP) {
                lk.unlock();
                close_dec();
                track_close(k);
                lk.lock();
                E.active = false;
                E.playing = false;
                E.audible_idx = -1;
                playing = false;
                songs.clear();
                E.cv.notify_all();
                continue;
            }
            if (c == PAUSE && playing) {
                paused_at = now_ms();
                const Song s = audible();
                E.want_idx = s.idx;
                E.want_ms = s.offset_ms + (played() - s.at_written_ms);
                playing = false;
                E.playing = false;
                lk.unlock();
                close_dec2();   // a crossfade in progress restarts as a plain song on resume
                if (k.t) call0(k, 0x28);   // Pause
                lk.lock();
                continue;
            }
            if (c == PLAY_AT || c == RESUME) {
                const int idx = E.want_idx;
                const long long ms = E.want_ms;
                const std::string u = idx >= 0 && idx < (int)E.uris.size() ? E.uris[idx] : std::string();
                lk.unlock();
                if (!u.empty() && !is_flac(u.c_str())) {
                    close_dec();
                    track_close(k);
                    lk.lock();
                    hand_over(idx, ms);
                    continue;
                }
                close_dec2();
                bool ok = !u.empty() && (dec_idx == idx || open_dec(idx, u));
                if (ok && k.t && (k.rate != rate || k.bits != bits)) track_close(k);
                if (ok && !k.t) ok = track_open(k, rate, bits);
                else if (ok) track_flush(k);
                if (ok) {
                    cinder_flac_seek(dec, (unsigned long long)(ms * rate / 1000));
                    dpos = (unsigned long long)(ms * rate / 1000);
                }
                lk.lock();
                if (!ok) {
                    std::fprintf(stderr, "[cinder-audio] engine: cannot play %s\n", u.c_str());
                    E.playing = playing = false;
                    g_halted_at_ms.store(now_ms(), std::memory_order_relaxed);
                    continue;
                }
                wf = 0;
                t0 = now_ms();
                songs.assign(1, Song{0, idx, ms, dur_of()});
                E.audible_idx = idx;
                E.playing = playing = true;
                g_halted_at_ms.store(0, std::memory_order_relaxed);
                continue;
            }
        }
        if (!playing || !dec) {
            E.cv.wait(lk);
            continue;
        }
        // ── one block ──
        const int qn = (int)E.uris.size();
        const bool rep1 = E.repeat_one;
        // Crossfade: once this song is within `crossfade_ms` of its end, open the next one.
        std::string next_u;
        if (E.crossfade_ms > 0 && !rep1 && !dec2 && fade_tried != dec_idx && total && dec_idx + 1 < qn
            && (long long)(total - dpos) <= (long long)E.crossfade_ms * rate / 1000) {
            fade_tried = dec_idx;
            next_u = E.uris[dec_idx + 1];
        }
        lk.unlock();
        if (!next_u.empty() && is_flac(next_u.c_str()) && (dec2 = cinder_flac_open(next_u.c_str()))) {
            unsigned r2 = 0, b2 = 0;
            cinder_flac_info(dec2, &r2, &ch2, &b2, &total2);
            if (r2 != rate || b2 != bits || (ch2 != 1 && ch2 != 2)) {
                close_dec2();   // another format: no fade, the usual change at the end
            } else {
                dec2_idx = dec_idx + 1;
                d2pos = 0;
                fade_len = (long long)(total - dpos);
                fade_done = 0;
                std::rename("/tmp/cinder_rg_next", "/tmp/cinder_rg");
                songs.push_back(Song{written_ms(), dec2_idx, 0, (long long)(total2 * 1000 / r2)});
            }
        }
        long n = read_stereo(dec, ch, pcm, 2048);
        if (dec2) {
            // Mix: the old song fades out as the new one fades in (linear gains).
            // ponytail: linear, not equal-power; switch if the middle of a fade sounds dipped.
            const long m = n > 0 ? n : 0;
            long n2 = read_stereo(dec2, ch2, pcm2, m ? m : 2048);
            if (n2 < 0) n2 = 0;
            const long frames = m > n2 ? m : n2;
            for (long i = 0; i < frames; i++) {
                const double g = fade_len > 0 ? std::min(1.0, (double)(fade_done + i) / (double)fade_len) : 1.0;
                for (int c = 0; c < 2; c++) {
                    const double a = i < m ? pcm[2 * i + c] : 0, b = i < n2 ? pcm2[2 * i + c] : 0;
                    pcm[2 * i + c] = (int)(a * (1.0 - g) + b * g);
                }
            }
            fade_done += m;
            dpos += (unsigned long long)m;
            d2pos += (unsigned long long)n2;
            if (n <= 0) {
                // The old song is over: the next one carries on alone.
                cinder_flac_close(dec);
                dec = dec2;
                dec_idx = dec2_idx;
                ch = ch2;
                total = total2;
                dpos = d2pos;
                dec2 = nullptr;
                dec2_idx = -1;
            }
            n = frames;
            if (n == 0) { lk.lock(); continue; }
        } else if (n > 0) {
            dpos += (unsigned long long)n;
        }
        if (n <= 0) {
            // End of this song (or a damaged file): the next one, gapless if its format matches.
            int next = rep1 ? dec_idx : dec_idx + 1;
            bool ok = false, other = false;
            while (next < qn) {
                lk.lock();
                const std::string u = next < (int)E.uris.size() ? E.uris[next] : std::string();
                lk.unlock();
                if (!u.empty() && !is_flac(u.c_str())) { other = true; break; }
                if (!u.empty() && open_dec(next, u)) { ok = true; break; }
                std::fprintf(stderr, "[cinder-audio] engine: skipping %s\n", u.c_str());
                ++next;
            }
            if (ok && (k.rate != rate || k.bits != bits)) {
                // A different format: let the queue play out, then a new track.
                while (played() < written_ms()) usleep(20000);
                track_close(k);
                ok = track_open(k, rate, bits);
                wf = 0;
                t0 = now_ms();
                songs.clear();
            }
            // The song after this one is now in the sound service — Sony's "reason 2" moment,
            // which is when ReplayGain's next gain becomes the current one (on_sequence).
            if (ok) std::rename("/tmp/cinder_rg_next", "/tmp/cinder_rg");
            if (!ok) {
                // End of the queue, or a song PlayerService must play: let this one play out,
                // then free the track for everyone else.
                while (played() < written_ms()) usleep(20000);
                close_dec();
                track_close(k);
                lk.lock();
                if (other) {
                    hand_over(next, 0);
                    continue;
                }
                E.playing = playing = false;
                E.active = false;
                g_halted_at_ms.store(now_ms(), std::memory_order_relaxed);
                continue;
            }
            lk.lock();
            songs.push_back(Song{written_ms(), next, 0, dur_of()});
            continue;
        }
        unsigned char* o = out;
        for (long i = 0; i < n * 2; i++)
            for (unsigned b = 0; b < k.bytes; b++) *o++ = (unsigned char)(pcm[i] >> (8 * b));
        const bool wrote = track_write(k, out, (unsigned)(o - out));
        wf += n;
        const long long now = now_ms();
        lk.lock();
        if (!wrote) {
            std::fprintf(stderr, "[cinder-audio] engine: Write failed — stopping\n");
            E.cmd = STOP;
            continue;
        }
        if (now - last_pub >= 250) {
            last_pub = now;
            const Song s = audible();
            if (s.idx != E.audible_idx)
                std::fprintf(stderr, "[cinder-audio] engine: %lld.%03lld now #%d (%lld ms) %s\n", now / 1000, now % 1000, s.idx, s.dur_ms,
                             s.idx >= 0 && s.idx < (int)E.uris.size() ? E.uris[s.idx].c_str() : "?");
            E.audible_idx = s.idx;
            publish_pos(s.offset_ms + (played() - s.at_written_ms), s.dur_ms);
        }
    }
    return nullptr;
}

// For the shell's thread: a handover the engine asked for, with the queue to give PlayerService.
static bool take_handover(std::vector<std::string>& uris, int& idx, long long& ms) {
    std::lock_guard<std::mutex> g(E.mu);
    if (E.handover_idx < 0) return false;
    uris = E.uris;
    idx = E.handover_idx;
    ms = E.handover_ms;
    E.handover_idx = -1;
    return true;
}

static void ensure_thread() {
    if (E.thread_up) return;
    E.thread_up = pthread_create(&E.th, nullptr, run, nullptr) == 0;
}

static void command(Cmd c) {
    std::lock_guard<std::mutex> g(E.mu);
    E.cmd = c;
    E.cv.notify_all();
}

static int play(const char* const* uris, int count, int start) {
    ensure_thread();
    if (!E.thread_up) return -1;
    {
        std::lock_guard<std::mutex> g(E.mu);
        E.uris.assign(uris, uris + count);
        E.want_idx = start < 0 || start >= count ? 0 : start;
        E.want_ms = 0;
        E.active = true;
        E.cmd = PLAY_AT;
        E.cv.notify_all();
    }
    std::fprintf(stderr, "[cinder-audio] engine: playing %d FLAC song(s) from #%d\n", count, start);
    return 0;
}

static bool active() {
    std::lock_guard<std::mutex> g(E.mu);
    return E.active;
}

static void stop() {
    if (!E.thread_up) return;
    std::unique_lock<std::mutex> lk(E.mu);
    if (!E.active) return;
    E.cmd = STOP;
    E.cv.notify_all();
    E.cv.wait_for(lk, std::chrono::seconds(3), [] { return !E.active; });
}

static int pause() { command(PAUSE); g_halted_at_ms.store(now_ms(), std::memory_order_relaxed); return 0; }

static int resume() {
    std::lock_guard<std::mutex> g(E.mu);
    if (E.playing) return 0;
    E.cmd = RESUME;   // want_idx/want_ms hold where PAUSE left it
    E.cv.notify_all();
    return 0;
}

// Jump to song `idx` at `ms` (a seek within the audible song when idx is that song).
static int go(int idx, long long ms) {
    std::lock_guard<std::mutex> g(E.mu);
    if (idx < 0 || idx >= (int)E.uris.size()) return -1;
    E.want_idx = idx;
    E.want_ms = ms < 0 ? 0 : ms;
    if (E.playing) E.cmd = PLAY_AT;   // paused: the next resume starts here
    E.cv.notify_all();
    return 0;
}

static int audible() {
    std::lock_guard<std::mutex> g(E.mu);
    return E.audible_idx >= 0 ? E.audible_idx : E.want_idx;
}

static int seek_ms(int ms) { return go(audible(), ms); }
static int next_track() { return go(audible() + 1, 0); }
static int prev_track() {
    // Sony's rule: more than 3 s in goes back to the start of the song.
    const int pos = g_cb_pos_ms.load(std::memory_order_relaxed);
    const int i = audible();
    return go(pos > 3000 || i == 0 ? i : i - 1, 0);
}

static std::string dir_of(const std::string& u) { return u.substr(0, u.rfind('/')); }

static int next_group() {
    int i = audible();
    std::string d;
    {
        std::lock_guard<std::mutex> g(E.mu);
        if (i < 0 || i >= (int)E.uris.size()) return -1;
        d = dir_of(E.uris[i]);
        while (i < (int)E.uris.size() && dir_of(E.uris[i]) == d) ++i;
    }
    return go(i, 0);
}

static int prev_group() {
    int i = audible();
    {
        std::lock_guard<std::mutex> g(E.mu);
        if (i < 0 || i >= (int)E.uris.size()) return -1;
        const std::string d = dir_of(E.uris[i]);
        while (i > 0 && dir_of(E.uris[i - 1]) == d) --i;   // start of this album
        if (g_cb_pos_ms.load(std::memory_order_relaxed) <= 3000 && i > 0) {
            const std::string p = dir_of(E.uris[i - 1]);
            --i;
            while (i > 0 && dir_of(E.uris[i - 1]) == p) --i;
        }
    }
    return go(i, 0);
}

// The queue changed around the playing song (Up Next edits): same song, new list.
static int edit(const char* const* uris, int count, int start) {
    std::lock_guard<std::mutex> g(E.mu);
    if (!E.active || start < 0 || start >= count) return -1;
    const int a = E.audible_idx;
    const std::string cur = a >= 0 && a < (int)E.uris.size() ? E.uris[a] : std::string();
    if (cur.empty() || cur != uris[start]) return -1;
    E.uris.assign(uris, uris + count);
    // ponytail: a song already queued gapless past the edit still plays; the next end reads the new list.
    E.audible_idx = start;
    return 0;
}

static int current_uri(char* buf, int cap) {
    std::lock_guard<std::mutex> g(E.mu);
    const int i = E.audible_idx >= 0 ? E.audible_idx : E.want_idx;
    if (i < 0 || i >= (int)E.uris.size() || cap <= 0) return -1;
    int n = (int)E.uris[i].size();
    if (n >= cap) n = cap - 1;
    std::memcpy(buf, E.uris[i].data(), (size_t)n);
    buf[n] = '\0';
    return n;
}

} // namespace eng

extern "C" {
void cinder_audio_engine_crossfade(int ms) {
    std::lock_guard<std::mutex> g(eng::E.mu);
    eng::E.crossfade_ms = ms > 0 ? ms : 0;
}

void cinder_audio_engine_enable(int on) {
    eng::s_enabled = on != 0;
    std::fprintf(stderr, "[cinder-audio] engine: %s\n", on ? "ON for all-FLAC queues" : "off");
}
} // extern "C"
