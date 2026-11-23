// radio_out.h — where internet radio, DLNA and AirPlay audio goes: the jack (ALSA hw:0,4, then
// hw:0,0) or Cinder's Bluetooth bridge socket, resampling a stream whose rate the output will not take.
//
// Moved out of main.cpp (2026-11-23). Not self-contained: included once, inside main.cpp's
// anonymous namespace, after g_alsa / alsa_load(), ldac_connect_socket() and g_radio_xruns.
#pragma once

struct RadioOut {
    snd_pcm_t* pcm = nullptr;
    int fd = -1;
    unsigned rate = 0, src_rate = 0;
    double pos = 0;               // resampler: position in the source, for Bluetooth's 44.1 kHz
    short last[2] = { 0, 0 };
    std::vector<short> tmp;
};

static bool radio_out_open(RadioOut& o, unsigned src_rate, bool bt) {
    o.src_rate = src_rate;
    if (bt) {
        o.fd = ldac_connect_socket(44100, 2);
        o.rate = 44100;
        return o.fd >= 0;
    }
    // libasound is loaded on first use, and AirPlay can be that first use: without this its first
    // song called a null snd_pcm_open and took the app down (2026-11-23).
    if (!alsa_load()) return false;
    // The stream's own rate first; a rate the jack will not take (a hi-res file) is resampled to
    // its family's base rate.
    const unsigned base = src_rate % 11025 == 0 ? 44100 : 48000;
    for (unsigned rate : { src_rate, base }) {
        for (const char* d : { "hw:0,4", "hw:0,0" }) {
            if (g_alsa.open(&o.pcm, d, /*PLAYBACK*/0, 0) < 0) { o.pcm = nullptr; continue; }
            if (g_alsa.set_params(o.pcm, /*S16_LE*/2, /*RW_INTERLEAVED*/3, 2, rate, 1, 250000) >= 0) {
                o.rate = rate;
                return true;
            }
            g_alsa.close(o.pcm);
            o.pcm = nullptr;
        }
    }
    return false;
}

// Write interleaved stereo. A stream at another rate than the output's (any stream not 44.1 kHz
// over Bluetooth, a hi-res one on the jack) is resampled linearly.
// ponytail: linear interpolation; a windowed-sinc resampler if 48 kHz stations sound dull over BT.
static bool radio_out_write(RadioOut& o, const short* pcm, unsigned frames) {
    if (!o.pcm && o.fd < 0) return false;
    const short* out = pcm;
    size_t n = frames;
    if (o.src_rate != o.rate) {
        const double step = (double)o.src_rate / o.rate;
        o.tmp.clear();
        for (; o.pos < frames; o.pos += step) {
            unsigned i = (unsigned)o.pos;
            double fr = o.pos - i;
            for (int c = 0; c < 2; c++) {
                short a = i == 0 ? o.last[c] : pcm[(i - 1) * 2 + c];
                short b = pcm[i * 2 + c];
                o.tmp.push_back((short)(a + (b - a) * fr));
            }
        }
        o.pos -= frames;
        o.last[0] = pcm[(frames - 1) * 2];
        o.last[1] = pcm[(frames - 1) * 2 + 1];
        out = o.tmp.data();
        n = o.tmp.size() / 2;
    }
    if (o.pcm) {
        while (n) {
            long w = g_alsa.writei(o.pcm, out, n);
            if (w == -EPIPE || w == -ESTRPIPE || w == -EINTR) {
                if (w == -EPIPE) ++g_radio_xruns;
                g_alsa.prepare(o.pcm);
                continue;
            }
            if (w < 0) return false;
            out += w * 2;
            n -= (size_t)w;
        }
        return true;
    }
    const unsigned char* p = (const unsigned char*)out;
    size_t want = n * 4;
    while (want) {
        ssize_t w = ::send(o.fd, p, want, MSG_NOSIGNAL);
        if (w < 0) { if (errno == EINTR) continue; return false; }
        p += w;
        want -= (size_t)w;
    }
    return true;
}

static void radio_out_close(RadioOut& o) {
    if (o.pcm) { if (g_alsa.drop) g_alsa.drop(o.pcm); g_alsa.close(o.pcm); o.pcm = nullptr; }
    if (o.fd >= 0) { ::close(o.fd); o.fd = -1; }
}
