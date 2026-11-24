// airplay_rx.h — the AirPlay receiver: starting and stopping shairport-sync, reading its audio
// onto the jack or Bluetooth (radio_out.h), its metadata pipe into Now Playing, and the 1 s tick
// that decides when it may play.
//
// Moved out of main.cpp (2026-11-23). Not self-contained: included once, inside main.cpp's
// anonymous namespace, after radio_out.h, the Wi-Fi state and g_ap_out_open.
#pragma once

// ── AirPlay receiver (Settings > Wi-Fi > AirPlay) ─────────────────────────────────────────────────
// shairport-sync (the Wi-Fi option's; MIT) runs while the switch is on and Wi-Fi is connected, as
// cinder-home's own user (in the inet group: RTSP on TCP 5000, mDNS on UDP 5353, RTP above 6000, all
// unprivileged). Its `stdout` output is raw 44.1 kHz S16 stereo — AirPlay's own rate, and the rate
// the Bluetooth transmitter takes, so nothing is resampled. A reader thread drains it always (a
// sender that is not read stalls); when audio arrives it asks the render thread for the output
// (airplay_tick does what radio_play does: music pauses, the radio and a soundscape stop), plays it
// through the radio's output code, and gives the output back after 2 s of silence. Music, FM and
// the rest take it back through ambient_yield_for_music, like the radio's.
//
// AIRPLAY 2. When the Wi-Fi option carries the AirPlay 2 build (shairport-sync-ap2) and nqptp, its
// timing companion, the receiver's child first has the helper start nqptp (it must bind UDP 319 and
// 320, so it starts as root and then becomes cinder-home's user), then runs the AirPlay 2 build,
// which also takes classic AirPlay senders. Without nqptp running it falls back to AirPlay 1 by
// itself; without the files the AirPlay 1 build runs as before.
static const char* const AP_BIN = "/system/vendor/cinder/wifi/shairport-sync";
static const char* const AP2_BIN = "/system/vendor/cinder/wifi/shairport-sync-ap2";
static const char* const AP2_NQPTP = "/system/vendor/cinder/wifi/nqptp";
static const char* const AP_NAME = "Walkman (Cinder)";
static pid_t g_ap_pid = -1;
static int g_ap_fd = -1;
static volatile sig_atomic_t g_ap_reader_alive = 0, g_ap_stop = 0;
static volatile sig_atomic_t g_ap_want = 0;     // reader: audio is arriving, may I play it
static volatile sig_atomic_t g_ap_granted = 0;  // tick: yes (route in g_ap_bt)
static volatile sig_atomic_t g_ap_yielded = 0;  // someone else took the output: drop until silence
static bool g_ap_bt = false, g_ap_source_held = false, g_ap_source_want = false;
static long long g_ap_retry_at = 0;
static std::string g_ap_note_sent = "-";
// THE SENDER'S SONG. A build with metadata (`-M -g --metadata-pipename`) writes the song's title,
// artist, album and cover into this FIFO (airplay_meta.h). It is made and opened read-write here
// before the receiver starts — read-write, so it never reads as ended between senders, and the
// receiver's own nonblocking open always finds a reader. The reader thread polls it beside the
// audio. A build without metadata refuses the options, so they are passed only to one that has them.
static const char* const AP_META = "/tmp/cinder_airplay_meta";
static int g_ap_meta_fd = -1;
// ITS OWN OUTPUT ON THE JACK (11-23). Through the stdout pipe, AirPlay 2's clock could only see
// Cinder's estimate of what was queued after the pipe, and every estimate error past 50 ms was a
// resync (a stutter). An AirPlay 2 build with ALSA plays to hw:0,4 itself and keeps time from the
// card's own delay: on the jack it runs `-o alsa` with the config below, and Cinder learns that it
// is playing from the metadata pipe (`pbeg`/`pres`/`prsm` .. `paus`/`pend`, airplay_meta.h). On
// Bluetooth it keeps the stdout pipe into Cinder's bridge. The output cannot be taken from it, so
// handing the jack to something else stops the receiver (the sender's session ends, as Play does)
// and starts it again 2 s later. The low-power ring needs Cinder's own sizes (shairport-sync's
// default 11.9 s buffer never moved), and the card's position moves in 46 ms steps, so the drift
// tolerance is half of one (at 2 ms it corrected all the time, heard as crackle).
static const char* const AP_ALSA_CONF = "/tmp/cinder_alsa.conf";
static const char* const AP_SPS_CONF = "/tmp/cinder_shairport.conf";
static const char kApAlsaConf[] =
    "pcm.hw {\n\t@args [ CARD DEV SUBDEV ]\n\t@args.CARD { type string default 0 }\n"
    "\t@args.DEV { type integer default 0 }\n\t@args.SUBDEV { type integer default -1 }\n"
    "\ttype hw\n\tcard $CARD\n\tdevice $DEV\n\tsubdevice $SUBDEV\n}\n"
    "ctl.hw {\n\t@args [ CARD ]\n\t@args.CARD { type string default 0 }\n\ttype hw\n\tcard $CARD\n}\n";
static const char kApSpsConf[] =
    "general = {\n\tdrift_tolerance_in_seconds = 0.025;\n};\n"
    "alsa = {\n\toutput_device = \"hw:0,4\";\n\toutput_rate = 44100;\n\toutput_format = \"S16_LE\";\n"
    "\tperiod_size = 1378;\n\tbuffer_size = 11025;\n\tuse_mmap_if_available = \"no\";\n};\n";
static volatile sig_atomic_t g_ap_direct = 0;    // this receiver plays to the jack itself
static volatile sig_atomic_t g_ap_playing = 0;   // its sender says it is playing (metadata)
static int g_ap_has_alsa = -1;                   // the AirPlay 2 build has ALSA: -1 not asked yet

// Is `what` in this receiver binary (an option it takes, an output it has)? Its strings are in it.
static bool airplay_bin_has(const char* bin, const char* what) {
    int fd = ::open(bin, O_RDONLY | O_CLOEXEC);
    if (fd < 0) return false;
    const size_t len = strlen(what);
    char buf[65536 + 64];
    if (len >= 64) { ::close(fd); return false; }
    size_t keep = 0;
    bool found = false;
    for (;;) {
        ssize_t n = ::read(fd, buf + keep, sizeof buf - keep);
        if (n <= 0) break;
        size_t have = keep + (size_t)n;
        if (memmem(buf, have, what, len)) { found = true; break; }
        keep = std::min(have, len);
        std::memmove(buf, buf + have - keep, keep);
    }
    ::close(fd);
    return found;
}

static void airplay_meta_read(apmeta::Parser& parser, apmeta::Song& song) {
    char buf[16384];
    for (;;) {
        ssize_t n = ::read(g_ap_meta_fd, buf, sizeof buf);
        if (n <= 0) return;   // EAGAIN: all read
        parser.feed(buf, (size_t)n, [&](const apmeta::Item& it) {
            const int changed = apmeta::apply(song, it);
            if (changed & apmeta::NAMES) {
                cinder_set_airplay_meta(song.title.c_str(), song.artist.c_str(), song.album.c_str());
                char m[200];
                std::snprintf(m, sizeof m, "airplay: the sender is playing \"%.60s\" by \"%.60s\"",
                              song.title.c_str(), song.artist.c_str());
                clog_(m);
            }
            if (changed & apmeta::PICTURE)
                cinder_set_airplay_art((const unsigned char*)song.pict.data(), (int)song.pict.size());
            if (changed & apmeta::PROGRESS) cinder_set_airplay_progress(song.pos_ms, song.dur_ms);
            if (changed & apmeta::PLAYING) {
                g_ap_playing = song.playing;
                if (g_ap_direct) clog_(song.playing ? "airplay: the sender is playing" : "airplay: the sender stopped");
            }
            // With the AirPlay debug switch on, every item the sender sends, for what it does not.
            static const bool dbg = ::access("/contents/cinder_airplay_debug", F_OK) == 0;
            if (dbg) {
                char m[160], t[5] = { char(it.type >> 24), char(it.type >> 16), char(it.type >> 8), char(it.type), 0 },
                     c[5] = { char(it.code >> 24), char(it.code >> 16), char(it.code >> 8), char(it.code), 0 };
                bool text = it.data.size() <= 60;
                for (char ch : it.data) text = text && ch >= 32 && ch < 127;
                std::snprintf(m, sizeof m, "airplay: item %s/%s %zu bytes%s%.60s", t, c, it.data.size(),
                              text ? " " : "", text ? it.data.c_str() : "");
                clog_(m);
            }
        });
    }
}

// WHAT IS QUEUED AFTER THE PIPE. AirPlay 2 keeps time by asking its output how much audio is still
// to be heard; shairport-sync's stdout output asks this file (Cinder's patch to audio_stdout.c,
// wifi/airplay/). Without it the receiver threw away a packet about once a second to catch up,
// heard as crackle and speed-ups (11-23). The layout is that patch's `struct cinder_queue`; seq
// and seq2 match when the record is whole. Frames 0 off the jack (Bluetooth, or not playing):
// then only the pipe counts.
struct ApQueue { uint32_t seq, frames; int64_t at_ns; uint32_t seq2, rate; };
static_assert(sizeof(ApQueue) == 24, "ApQueue must match shairport-sync's struct cinder_queue");
static const char* const AP_QUEUE = "/tmp/cinder_airplay_queue";
static int g_ap_queue_fd = -1;

// For the 10 s report: the lowest and highest queue published, and of the pipe beside it (what the
// receiver adds), in frames.
static long g_apq_min = -1, g_apq_max = -1, g_apq_pipe_min = -1, g_apq_pipe_max = -1, g_apq_sum_min = -1,
            g_apq_sum_max = -1;

static void airplay_queue_publish(long frames) {
    if (g_ap_queue_fd < 0) return;
    int in_pipe = 0;
    if (g_ap_fd >= 0 && ::ioctl(g_ap_fd, FIONREAD, &in_pipe) == 0) {
        const long p = in_pipe / 4, sum = p + frames;
        if (g_apq_min < 0 || frames < g_apq_min) g_apq_min = frames;
        if (frames > g_apq_max) g_apq_max = frames;
        if (g_apq_pipe_min < 0 || p < g_apq_pipe_min) g_apq_pipe_min = p;
        if (p > g_apq_pipe_max) g_apq_pipe_max = p;
        if (g_apq_sum_min < 0 || sum < g_apq_sum_min) g_apq_sum_min = sum;
        if (sum > g_apq_sum_max) g_apq_sum_max = sum;
    }
    static uint32_t seq = 0;
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    ApQueue q;
    q.seq = q.seq2 = ++seq;
    q.frames = frames > 0 ? (uint32_t)frames : 0;
    q.at_ns = (int64_t)ts.tv_sec * 1000000000 + ts.tv_nsec;
    q.rate = 44100;
    ssize_t w = ::pwrite(g_ap_queue_fd, &q, sizeof q, 0); (void)w;
}

// THE JACK'S QUEUE, SMOOTHED. hw:0,4's position moves in steps of about 2030 frames (46 ms,
// measured 11-23), so snd_pcm_delay alone reads up to 46 ms long between steps and then jumps: the
// receiver chased that jitter one sample at a time (crackle) and resynced past 50 ms (a stutter).
// The position right after a step is exact, and the DAC moves on at 44.1 kHz from there, so the
// best estimate of where it is now is the largest of the last 2 s of readings, each moved on by
// the time since it was taken. The queue is what was written less that.
struct JackClock {
    long long written = 0;                    // frames given to ALSA since the output opened
    struct Seen { long long at_us, pos; } seen[48];
    int n = 0, head = 0;
    void reset() { written = 0; n = 0; head = 0; }
    static long long now_us() {
        struct timespec ts;
        clock_gettime(CLOCK_MONOTONIC, &ts);
        return (long long)ts.tv_sec * 1000000 + ts.tv_nsec / 1000;
    }
    void observe(long delay) {
        seen[head] = { now_us(), written - (delay > 0 ? delay : 0) };
        head = (head + 1) % 48;
        if (n < 48) ++n;
    }
    long queue() const {
        const long long t = now_us();
        long long pos = -1;
        for (int i = 0; i < n; i++) {
            if (t - seen[i].at_us > 2000000) continue;
            const long long p = seen[i].pos + (t - seen[i].at_us) * 44100 / 1000000;
            if (p > pos) pos = p;
        }
        if (pos < 0) return 0;
        if (pos > written) pos = written;    // it cannot have played what it was not given
        return (long)(written - pos);
    }
};

// The jack's queue now, plus `in_hand` frames read but not yet written; 0 when it cannot say.
static long airplay_jack_queue(RadioOut& out, JackClock& jc, size_t in_hand) {
    long d = 0;
    if (!out.pcm || !g_alsa.delay || g_alsa.delay(out.pcm, &d) != 0) return out.pcm ? (long)in_hand : 0;
    // Not started yet (ALSA starts once its buffer is full): nothing is playing, so nothing may be
    // counted as played. Counting it was a glitch at the start of every song.
    if (g_alsa.state && g_alsa.state(out.pcm) != 3 /*SND_PCM_STATE_RUNNING*/) {
        jc.n = 0;
        return (d > 0 ? d : 0) + (long)in_hand;
    }
    jc.observe(d);
    return jc.queue() + (long)in_hand;
}

static void* airplay_reader(void*) {
    g_ap_reader_alive = 1;
    static unsigned char buf[17640];          // 100 ms of 44.1 kHz S16 stereo
    RadioOut out;
    JackClock jack;
    long long last_audio = 0;
    size_t carry = 0;                         // a frame split across reads (4 bytes a frame)
    apmeta::Parser meta_parser;
    apmeta::Song meta_song;
    // Every 10 s while the output is open: frames the receiver sent, the longest wait between its
    // writes, and underruns on the jack. The sender's clock and the jack's should agree on 44100/s.
    long long stat_at = 0, last_read = 0, max_gap = 0;
    unsigned long long stat_frames = 0;
    unsigned stat_xruns = g_radio_xruns;
    while (!g_ap_stop) {
        if (g_ap_out_open && stat_at && now_ms() - stat_at >= 10000) {
            const long long span = now_ms() - stat_at;
            char m[160];
            std::snprintf(m, sizeof m, "airplay: %llu frames/s, longest wait %lld ms, %u underruns in %lld s",
                          stat_frames * 1000 / (unsigned long long)span, max_gap, g_radio_xruns - stat_xruns,
                          span / 1000);
            clog_(m);
            std::snprintf(m, sizeof m, "airplay: queued ms — jack %ld..%ld, pipe %ld..%ld, both %ld..%ld",
                          g_apq_min * 10 / 441, g_apq_max * 10 / 441, g_apq_pipe_min * 10 / 441,
                          g_apq_pipe_max * 10 / 441, g_apq_sum_min * 10 / 441, g_apq_sum_max * 10 / 441);
            clog_(m);
            g_apq_min = g_apq_max = g_apq_pipe_min = g_apq_pipe_max = g_apq_sum_min = g_apq_sum_max = -1;
            stat_at = now_ms(); stat_frames = 0; max_gap = 0; stat_xruns = g_radio_xruns;
        }
        struct pollfd pf[2] = { { g_ap_fd, POLLIN, 0 }, { g_ap_meta_fd, POLLIN, 0 } };
        int r = ::poll(pf, g_ap_meta_fd >= 0 ? 2 : 1, 500);
        long long now = now_ms();
        if (r > 0 && g_ap_meta_fd >= 0 && (pf[1].revents & POLLIN)) airplay_meta_read(meta_parser, meta_song);
        // Its own output: the sender's word stands for the audio arriving. The tick takes the jack
        // for it; a yield stops the receiver (airplay_yield), as nothing else can take it back.
        if (g_ap_direct && g_ap_playing) {
            last_audio = now;
            if (!g_ap_yielded) {
                if (!g_ap_granted) g_ap_want = 1;
                else if (!g_ap_out_open) { g_ap_out_open = 1; clog_("airplay: playing on the jack (its own output, hw:0,4)"); }
            }
        }
        if (r > 0 && (pf[0].revents & (POLLIN | POLLHUP))) {
            ssize_t n = ::read(g_ap_fd, buf + carry, sizeof buf - carry);
            if (n <= 0) break;                // the receiver exited
            size_t have = carry + (size_t)n, frames = have / 4;
            last_audio = now;
            if (g_ap_out_open) {
                if (!stat_at) { stat_at = now; stat_xruns = g_radio_xruns; }
                else if (now - last_read > max_gap) max_gap = now - last_read;
                stat_frames += frames;
            } else {
                stat_at = 0;
            }
            last_read = now;
            if (g_ap_yielded) {
                if (g_ap_out_open) { radio_out_close(out); g_ap_out_open = 0; }
            } else if (!g_ap_granted) {
                g_ap_want = 1;                // the first ~100 ms is dropped while the tick decides
            } else {
                if (!g_ap_out_open) {
                    if (!radio_out_open(out, 44100, g_ap_bt)) { g_ap_yielded = 1; clog_("airplay: the output would not open"); }
                    else {
                        g_ap_out_open = 1;
                        jack.reset();
                        char m[96];
                        std::snprintf(m, sizeof m, "airplay: playing %s%s", g_ap_bt ? "over Bluetooth" : "on the jack, ",
                                      g_ap_bt ? "" : out.dev);
                        clog_(m);
                    }
                }
                if (g_ap_out_open && out.pcm) airplay_queue_publish(airplay_jack_queue(out, jack, frames));
                const bool wrote = g_ap_out_open && radio_out_write(out, (const short*)buf, (unsigned)frames);
                if (wrote) jack.written += (long long)frames;
                if (g_ap_out_open) airplay_queue_publish(airplay_jack_queue(out, jack, 0));
                if (g_ap_out_open && !wrote) {
                    radio_out_close(out);
                    g_ap_out_open = 0;
                    g_ap_yielded = 1;
                }
            }
            carry = have - frames * 4;
            if (carry) std::memmove(buf, buf + frames * 4, carry);
        }
        // Two seconds without audio: the sender paused or left. Give the output back.
        if ((g_ap_want || g_ap_granted || g_ap_yielded) && now - last_audio > 2000) {
            if (g_ap_out_open) { radio_out_close(out); g_ap_out_open = 0; clog_("airplay: idle — the output is free"); }
            g_ap_want = 0;
            g_ap_granted = 0;
            g_ap_yielded = 0;
        }
    }
    if (g_ap_out_open) { radio_out_close(out); g_ap_out_open = 0; }
    g_ap_want = 0;
    g_ap_granted = 0;
    g_ap_reader_alive = 0;
    return nullptr;
}

static void airplay_bt_source(bool on) {
    g_ap_source_want = on;
    amb_guarded("airplay: SetCurrentSource", []() {
        enum { VIDX_SetCurrentSource = 12 };
        void* x = bt_xmit();
        if (!x) return;
        bool v = g_ap_source_want;
        typedef int (*fnb)(void*, const bool*);
        ((fnb)bt_slot(x, VIDX_SetCurrentSource))(x, &v);
    });
    g_ap_source_held = on;
}

static void airplay_stop(const char* why) {
    if (g_ap_pid < 0) return;
    g_ap_stop = 1;
    ::kill(g_ap_pid, SIGTERM);
    // The AirPlay 2 receiver takes ~9 s to exit on SIGTERM (measured 11-24), and this runs on the
    // UI thread: Wi-Fi off froze the screen 7.5 s. 0.2 s, then SIGKILL; the kernel frees the
    // jack and its sockets, and the only loss is the mDNS goodbye.
    int i = 0;
    while (i < 20 && ::waitpid(g_ap_pid, nullptr, WNOHANG) == 0) { usleep(10000); i++; }
    if (i == 20) { ::kill(g_ap_pid, SIGKILL); ::waitpid(g_ap_pid, nullptr, 0); }
    for (int k = 0; k < 200 && g_ap_reader_alive; k++) usleep(10000);
    if (g_ap_fd >= 0) ::close(g_ap_fd);
    g_ap_pid = -1;
    g_ap_fd = -1;
    if (g_ap_meta_fd >= 0) { ::close(g_ap_meta_fd); g_ap_meta_fd = -1; ::unlink(AP_META); }
    if (g_ap_queue_fd >= 0) { ::close(g_ap_queue_fd); g_ap_queue_fd = -1; ::unlink(AP_QUEUE); }
    g_ap_direct = 0;
    g_ap_playing = 0;
    cinder_set_airplay_meta(nullptr, nullptr, nullptr);
    cinder_set_airplay_art(nullptr, 0);
    if (g_ap_bt) { g_ldac_run = 0; g_ldac_alive = 0; }
    if (g_ap_source_held) airplay_bt_source(false);
    if (::access(AP2_NQPTP, X_OK) == 0) wifi_helper("nqptp-stop", nullptr, nullptr);
    char m[96];
    std::snprintf(m, sizeof m, "airplay: receiver stopped (%s)", why);
    clog_(m);
}

static bool airplay_start() {
    const bool ap2 = ::access(AP2_BIN, X_OK) == 0 && ::access(AP2_NQPTP, X_OK) == 0;
    const bool meta = airplay_bin_has(ap2 ? AP2_BIN : AP_BIN, "metadata-pipename");
    if (g_ap_has_alsa < 0) g_ap_has_alsa = ap2 && airplay_bin_has(AP2_BIN, "alsa_device_init") ? 1 : 0;
    // On the jack an AirPlay 2 build with ALSA plays by itself; it needs the metadata to say when.
    const bool direct = ap2 && meta && g_ap_has_alsa == 1 && g_bt_link_last != 1;
    if (direct) {
        auto put = [](const char* path, const char* text) {
            FILE* f = std::fopen(path, "w");
            if (!f) return false;
            const bool ok = std::fputs(text, f) >= 0;
            return std::fclose(f) == 0 && ok;
        };
        if (!put(AP_ALSA_CONF, kApAlsaConf) || !put(AP_SPS_CONF, kApSpsConf)) {
            clog_("airplay: could not write its ALSA config — playing through Cinder instead");
            g_ap_has_alsa = 0;
        }
    }
    g_ap_direct = direct && g_ap_has_alsa == 1;
    if (meta) {
        ::unlink(AP_META);
        if (::mkfifo(AP_META, 0600) == 0) g_ap_meta_fd = ::open(AP_META, O_RDWR | O_NONBLOCK | O_CLOEXEC);
        if (g_ap_meta_fd < 0) clog_("airplay: no metadata pipe — the sender's song will not be shown");
    }
    if (!g_ap_direct && g_ap_queue_fd < 0) {   // the receiver's clock reads it (airplay_queue_publish)
        g_ap_queue_fd = ::open(AP_QUEUE, O_RDWR | O_CREAT | O_TRUNC | O_CLOEXEC, 0644);
        airplay_queue_publish(0);
    }
    static const char kMetaOpt[] = "--metadata-pipename=/tmp/cinder_airplay_meta";
    const bool use_meta = g_ap_meta_fd >= 0;
    auto drop_meta = []() { if (g_ap_meta_fd >= 0) { ::close(g_ap_meta_fd); g_ap_meta_fd = -1; ::unlink(AP_META); } g_ap_direct = 0; };
    const bool to_jack = g_ap_direct;
    int fd[2];
    if (::pipe(fd) < 0) { drop_meta(); return false; }
    pid_t p = ::fork();
    if (p < 0) { ::close(fd[0]); ::close(fd[1]); drop_meta(); return false; }
    if (p == 0) {
        ::dup2(fd[1], 1);
        int nul = ::open("/dev/null", O_WRONLY);
        // Dev switch: /contents/cinder_airplay_debug makes the receiver say what it is doing (-vv)
        // into /tmp/cinder_airplay.log, for a sender that sees the player but cannot play to it.
        const bool dbg = ::access("/contents/cinder_airplay_debug", F_OK) == 0;
        int err = dbg ? ::open("/tmp/cinder_airplay.log", O_WRONLY | O_CREAT | O_TRUNC, 0644) : -1;
        if (err >= 0) { ::dup2(err, 2); ::close(err); }
        else if (nul >= 0) ::dup2(nul, 2);
        for (int i = 3; i < 256; i++) if (i != nul) ::close(i);
        ::chdir("/tmp");
        const char* argv[14] = { "shairport-sync", "-a", AP_NAME, "-o", to_jack ? "alsa" : "stdout" };
        int argc = 5;
        if (to_jack) {
            argv[argc++] = "-c";
            argv[argc++] = AP_SPS_CONF;
            ::setenv("ALSA_CONFIG_PATH", AP_ALSA_CONF, 1);
        }
        if (use_meta) { argv[argc++] = "-M"; argv[argc++] = "-g"; argv[argc++] = kMetaOpt; }
        if (err >= 0) argv[argc++] = "-vv";
        argv[argc] = nullptr;
        if (ap2) {
            // nqptp first (the receiver looks for it once, at start). The helper's own words go
            // to /dev/null, not into the audio pipe on stdout.
            pid_t h = ::fork();
            if (h == 0) {
                if (nul >= 0) ::dup2(nul, 1);
                ::execl(WIFI_HELPER, "cinder-wifi", "nqptp-start", (char*)nullptr);
                ::_exit(127);
            }
            if (h > 0) ::waitpid(h, nullptr, 0);
            if (nul >= 0) ::close(nul);
            ::execv(AP2_BIN, (char* const*)argv);
            if (to_jack) ::_exit(127);   // the AirPlay 1 build has no ALSA to fall back to
        }
        if (nul >= 0) ::close(nul);
        ::execv(AP_BIN, (char* const*)argv);
        ::_exit(127);
    }
    ::close(fd[1]);
    g_ap_pid = p;
    g_ap_fd = fd[0];
    g_ap_stop = 0;
    pthread_attr_t at;
    pthread_attr_init(&at);
    pthread_attr_setdetachstate(&at, PTHREAD_CREATE_DETACHED);
    pthread_attr_setstacksize(&at, 128 * 1024);
    pthread_t th;
    bool ok = pthread_create(&th, &at, airplay_reader, nullptr) == 0;
    pthread_attr_destroy(&at);
    if (!ok) { airplay_stop("no reader thread"); return false; }
    clog_(!ap2 ? "airplay: receiver started (AirPlay 1)"
          : to_jack ? "airplay: receiver started (AirPlay 2 build, nqptp, its own output on the jack)"
                    : "airplay: receiver started (AirPlay 2 build, nqptp)");
    if (use_meta) clog_("airplay: the sender's song and cover come through its metadata pipe");
    return true;
}

// Music, FM, USB-DAC and the rest want the output: AirPlay lets go until its sender goes quiet.
static void airplay_yield() {
    // Playing to the jack itself, it cannot be asked to let go: stop it, start it again soon.
    if (g_ap_direct && (g_ap_granted || g_ap_out_open || g_ap_playing || g_ap_want)) {
        airplay_stop("the output is wanted");
        g_ap_retry_at = now_ms() + 2000;
        return;
    }
    if (!g_ap_granted && !g_ap_out_open) return;
    g_ap_yielded = 1;
    for (int i = 0; i < 200 && g_ap_out_open; i++) usleep(5000);
}

// Render thread, once a second: run the receiver when it should run, and hand it the output.
static void airplay_tick(long long now) {
    const bool want = cinder_wifi_airplay() && g_wifi_connected;
    if (g_ap_pid >= 0 && !g_ap_reader_alive) {           // it exited on its own
        airplay_stop("it exited");
        g_ap_retry_at = now + 30000;
    }
    if (!want && g_ap_pid >= 0) airplay_stop(cinder_wifi_airplay() ? "Wi-Fi is not connected" : "switched off");
    // The route changed under it: on the jack by itself while headphones are now connected (stop it
    // now, or the music stays on the jack), or through Cinder while the jack is free again (only
    // when idle: Cinder's own path still plays well enough to finish the song).
    if (g_ap_pid >= 0 && g_ap_has_alsa == 1) {
        const bool to_jack = g_bt_link_last != 1;
        if (g_ap_direct && !to_jack) { airplay_stop("Bluetooth connected"); g_ap_retry_at = now; }
        else if (!g_ap_direct && to_jack && g_ap_meta_fd >= 0 && !g_ap_out_open && !g_ap_want && !g_ap_granted) {
            airplay_stop("Bluetooth disconnected");
            g_ap_retry_at = now;
        }
    }
    if (want && g_ap_pid < 0 && now >= g_ap_retry_at && ::access(AP_BIN, X_OK) == 0) {
        if (!airplay_start()) g_ap_retry_at = now + 30000;
    }
    if (g_ap_want && !g_ap_granted && !g_ap_yielded) {
        const bool bridge = g_ldac_alive && !g_amb_bt_alive && !(g_ap_bt && g_ap_out_open);
        if (g_msc_active || g_rx_active || g_fm_on || bridge) {
            g_ap_yielded = 1;
            clog_("airplay: the output is in use — not playing this stream");
            if (g_ap_direct) {   // it would play over them: stop it
                airplay_stop("the output is in use");
                g_ap_retry_at = now + 2000;
            }
        } else {
            if (g_playing) {
                set_transport(false);
                run_guarded("airplay: pause", 6, []() { cinder_audio_pause(); });
            }
            radio_yield("Stopped for AirPlay");
            if (g_amb_alive) g_amb_run = 0;
            if (g_amb_bt_alive) { g_amb_bt_run = 0; g_ldac_run = 0; }
            for (int i = 0; i < 60 && (g_amb_alive || g_amb_bt_alive); i++) usleep(5000);
            if (g_amb_bt_source_held && !g_amb_bt_alive) amb_bt_source(false);
            g_ap_bt = g_bt_link_last == 1;
            if (g_ap_bt) {
                amb_guarded("airplay: codec", apply_bt_codec);
                airplay_bt_source(true);
                g_ldac_run = 1;
                g_ldac_alive = 1;
            }
            g_ap_granted = 1;
        }
    }
    if (g_ap_bt && !g_ap_granted && !g_ap_out_open && (g_ap_source_held || g_ldac_alive)) {
        g_ldac_run = 0;
        g_ldac_alive = 0;
        if (g_ap_source_held) airplay_bt_source(false);
        g_ap_bt = false;
    }
    const char* note = g_ap_pid < 0 ? "" : g_ap_out_open ? "Playing from AirPlay"
                     : g_ap_yielded ? "AirPlay is waiting: the output is in use" : "Shows up as Walkman (Cinder)";
    if (g_ap_note_sent != note) { g_ap_note_sent = note; cinder_set_airplay_note(note); }
}
