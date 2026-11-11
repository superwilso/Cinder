// lossless.h — FLAC and WAV for the network player (internet radio, Play to this player).
//
// MP3 and AAC arrive in frames the radio loop can hand to a decoder as they come. FLAC and WAV are
// read the other way round: the decoder PULLS bytes (`Read`), and the loop's reader blocks on the
// download until it has them, returning 0 only at the end of the stream. Output is interleaved
// 16-bit, as the rest of the radio path expects.
//
// FLAC by dr_flac (third_party/dr_flac, public domain), Ogg FLAC left out. WAV is parsed here:
// integer PCM, 8 to 32 bits, little-endian, plain or WAVE_FORMAT_EXTENSIBLE.
//
// ponytail: 24-bit is truncated to 16 without dither (-96 dB of noise); dither if anyone hears it.
#pragma once
#include <cstddef>
#include <cstring>

#define DR_FLAC_IMPLEMENTATION
#define DR_FLAC_NO_STDIO
#define DR_FLAC_NO_OGG
#define DR_FLAC_NO_WCHAR
#include "../third_party/dr_flac/dr_flac.h"

namespace lossless {

inline bool is_flac(const unsigned char* b, size_t n) { return n >= 4 && std::memcmp(b, "fLaC", 4) == 0; }
inline bool is_wav(const unsigned char* b, size_t n) {
    return n >= 12 && std::memcmp(b, "RIFF", 4) == 0 && std::memcmp(b + 8, "WAVE", 4) == 0;
}

inline unsigned le16(const unsigned char* p) { return p[0] | (unsigned)p[1] << 8; }
inline unsigned le32(const unsigned char* p) { return le16(p) | le16(p + 2) << 16; }

struct Wav {
    unsigned hz = 0;
    int ch = 0, bits = 0;
    size_t data_at = 0;           // offset of the first sample
    const char* problem = nullptr;
};

// The header of a WAV stream in b[0..n). 1 = parsed, -1 = needs more bytes, 0 = cannot play it
// (`problem` says why).
inline int wav_header(const unsigned char* b, size_t n, Wav* w) {
    bool fmt = false;
    for (size_t at = 12; at + 8 <= n;) {
        const unsigned size = le32(b + at + 4);
        if (std::memcmp(b + at, "data", 4) == 0) {
            if (!fmt) { w->problem = "This WAV has no format chunk before its audio"; return 0; }
            w->data_at = at + 8;
            return 1;
        }
        if (std::memcmp(b + at, "fmt ", 4) == 0) {
            if (at + 8 + 16 > n) return -1;
            if (size < 16) { w->problem = "This WAV's format is not supported"; return 0; }
            const unsigned char* f = b + at + 8;
            unsigned tag = le16(f);
            if (tag == 0xFFFE) {  // WAVE_FORMAT_EXTENSIBLE: the real format leads the sub-format GUID
                if (size < 40) { w->problem = "This WAV's format is not supported"; return 0; }
                if (at + 8 + 26 > n) return -1;
                tag = le16(f + 24);
            }
            w->ch = (int)le16(f + 2);
            w->hz = le32(f + 4);
            w->bits = (int)le16(f + 14);
            if (tag != 1) { w->problem = "This WAV is not integer PCM"; return 0; }
            if (w->ch < 1 || w->ch > 8 || w->hz < 8000 || w->hz > 384000 ||
                (w->bits != 8 && w->bits != 16 && w->bits != 24 && w->bits != 32)) {
                w->problem = "This WAV's format is not supported";
                return 0;
            }
            fmt = true;
        }
        // A chunk before the audio that is not all here yet (a big tag): keep reading. 64-bit, so a
        // size near 4 GB cannot wrap on the player's 32-bit size_t.
        const unsigned long long next = (unsigned long long)at + 8 + size + (size & 1);
        if (next > n) return -1;
        at = (size_t)next;
    }
    return -1;
}

// `samples` little-endian integer samples of `bits` to 16-bit (the top two bytes; 8-bit is unsigned).
inline void to_s16(const unsigned char* in, size_t samples, int bits, short* out) {
    const int bps = bits / 8;
    for (size_t i = 0; i < samples; i++, in += bps)
        out[i] = bps == 1 ? (short)((in[0] - 128) * 256) : (short)(in[bps - 1] << 8 | in[bps - 2]);
}

// A FLAC stream pulled through `read`, which blocks, may return fewer bytes than asked, and returns
// 0 only at the end.
struct Flac {
    typedef size_t (*Read)(void* ctx, unsigned char* buf, size_t n);
    Read read = nullptr;
    void* ctx = nullptr;
    drflac* f = nullptr;

    // dr_flac reads a short count as the end of the stream, so fill the whole request.
    static size_t on_read(void* u, void* out, size_t n) {
        Flac* s = (Flac*)u;
        size_t got = 0, k;
        while (got < n && (k = s->read(s->ctx, (unsigned char*)out + got, n - got)) > 0) got += k;
        return got;
    }
    // A download only goes forward: skipping a block (cover art) is reading past it.
    static drflac_bool32 on_seek(void* u, int off, drflac_seek_origin o) {
        if (o != DRFLAC_SEEK_CUR || off < 0) return DRFLAC_FALSE;
        Flac* s = (Flac*)u;
        unsigned char sink[4096];
        while (off > 0) {
            size_t k = s->read(s->ctx, sink, off < (int)sizeof sink ? (size_t)off : sizeof sink);
            if (!k) return DRFLAC_FALSE;
            off -= (int)k;
        }
        return DRFLAC_TRUE;
    }
    bool open(Read r, void* c) {
        read = r;
        ctx = c;
        f = drflac_open(on_read, on_seek, nullptr, this, nullptr);
        return f != nullptr;
    }
    unsigned hz() const { return f->sampleRate; }
    int ch() const { return f->channels; }
    int bits() const { return f->bitsPerSample; }
    // Up to `frames` frames of interleaved 16-bit; 0 = the end (or a stream it cannot go on with).
    size_t next(short* pcm, size_t frames) { return (size_t)drflac_read_pcm_frames_s16(f, frames, pcm); }
    ~Flac() { if (f) drflac_close(f); }
};

}  // namespace lossless
