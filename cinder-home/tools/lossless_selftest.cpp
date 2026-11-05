// lossless_selftest.cpp — src/lossless.h: FLAC and WAV come out of a download that arrives in odd
// pieces as the samples that went in. The FLAC is written here (verbatim frames, a cover-art block
// to skip), so the test needs no file and no encoder.
#include <cstdio>
#include <cstdlib>
#include <vector>
#include "../src/lossless.h"

static int fails;
#define CHECK(c, what) do { if (c) std::printf("  ok   %s\n", what); else { std::printf("  FAIL %s\n", what); ++fails; } } while (0)

typedef std::vector<unsigned char> Bytes;

static void put(Bytes& b, unsigned long long v, int bytes) {
    for (int i = bytes - 1; i >= 0; i--) b.push_back((unsigned char)(v >> (8 * i)));
}
static unsigned crc8(const unsigned char* p, size_t n) {
    unsigned c = 0;
    for (size_t i = 0; i < n; i++) { c ^= p[i]; for (int k = 0; k < 8; k++) c = (c & 0x80) ? ((c << 1) ^ 0x07) & 0xFF : (c << 1) & 0xFF; }
    return c;
}
static unsigned crc16(const unsigned char* p, size_t n) {
    unsigned c = 0;
    for (size_t i = 0; i < n; i++) { c ^= (unsigned)p[i] << 8; for (int k = 0; k < 8; k++) c = (c & 0x8000) ? ((c << 1) ^ 0x8005) & 0xFFFF : (c << 1) & 0xFFFF; }
    return c;
}

// `pcm` interleaved, `bits` 16 or 24, frames of `block`.
static Bytes flac(const std::vector<int>& pcm, int ch, int bits, unsigned hz, int block) {
    const size_t frames = pcm.size() / ch;
    Bytes b = { 'f', 'L', 'a', 'C' };
    // STREAMINFO (not last), then a PICTURE block (last) the decoder has to read past.
    b.push_back(0x00); put(b, 34, 3);
    put(b, block, 2); put(b, block, 2); put(b, 0, 3); put(b, 0, 3);
    unsigned long long w = (unsigned long long)hz << 44 | (unsigned long long)(ch - 1) << 41 |
                           (unsigned long long)(bits - 1) << 36 | frames;
    put(b, w, 8);
    for (int i = 0; i < 16; i++) b.push_back(0);  // MD5 unknown
    const size_t art = 5000;
    b.push_back(0x80 | 6); put(b, art, 3);
    for (size_t i = 0; i < art; i++) b.push_back((unsigned char)(i * 7));
    for (size_t f0 = 0, num = 0; f0 < frames; f0 += block, num++) {
        const size_t n = frames - f0 < (size_t)block ? frames - f0 : (size_t)block;
        const size_t start = b.size();
        b.push_back(0xFF); b.push_back(0xF8);
        b.push_back(0x70);  // block size: 16-bit at the end of the header; rate from STREAMINFO
        b.push_back((unsigned char)((ch - 1) << 4 | (bits == 24 ? 6 : 4) << 1));
        b.push_back((unsigned char)num);  // frame number, UTF-8 coded (< 128)
        put(b, n - 1, 2);
        b.push_back((unsigned char)crc8(b.data() + start, b.size() - start));
        for (int c = 0; c < ch; c++) {
            b.push_back(0x02);  // VERBATIM
            for (size_t i = 0; i < n; i++) put(b, (unsigned)pcm[(f0 + i) * ch + c] & ((1u << bits) - 1), bits / 8);
        }
        put(b, crc16(b.data() + start, b.size() - start), 2);
    }
    return b;
}

// A download: hands out the bytes in pieces of 1..`max` bytes.
struct Feed { const Bytes* b; size_t at; size_t max; };
static size_t feed_read(void* c, unsigned char* out, size_t n) {
    Feed* f = (Feed*)c;
    size_t k = 1 + (size_t)std::rand() % f->max;
    if (k > n) k = n;
    if (k > f->b->size() - f->at) k = f->b->size() - f->at;
    std::memcpy(out, f->b->data() + f->at, k);
    f->at += k;
    return k;
}

static bool decodes(const Bytes& file, int ch, const std::vector<int>& want, int shift, size_t piece) {
    Feed fd = { &file, 0, piece };
    lossless::Flac f;
    if (!f.open(feed_read, &fd) || f.ch() != ch || f.hz() != 48000) return false;
    std::vector<short> got;
    short pcm[4096];
    size_t n;
    while ((n = f.next(pcm, 4096 / ch)) > 0) got.insert(got.end(), pcm, pcm + n * ch);
    if (got.size() != want.size()) return false;
    for (size_t i = 0; i < want.size(); i++)
        if (got[i] != (short)(want[i] >> shift)) return false;
    return true;
}

int main() {
    std::srand(7);
    std::puts("== flac");
    std::vector<int> st16, mono24;
    for (int i = 0; i < 3500; i++) {
        st16.push_back((i * 97) % 65536 - 32768);
        st16.push_back(-((i * 31) % 30000));
    }
    for (int i = 0; i < 2100; i++) mono24.push_back((i * 4099) % 16777216 - 8388608);
    const Bytes a = flac(st16, 2, 16, 48000, 1000), m = flac(mono24, 1, 24, 48000, 512);
    CHECK(lossless::is_flac(a.data(), a.size()) && !lossless::is_wav(a.data(), a.size()), "told by its first bytes");
    CHECK(decodes(a, 2, st16, 0, 1), "16-bit stereo, one byte at a time, past the cover art");
    CHECK(decodes(a, 2, st16, 0, 3000), "16-bit stereo, in big pieces");
    CHECK(decodes(m, 1, mono24, 8, 37), "24-bit mono comes out as its top 16 bits");
    {
        Bytes cut(a.begin(), a.begin() + (long)(a.size() * 2 / 3));
        Feed fd = { &cut, 0, 100 };
        lossless::Flac f;
        size_t total = 0, n;
        short pcm[4096];
        bool ok = f.open(feed_read, &fd);
        while (ok && (n = f.next(pcm, 2048)) > 0) total += n;
        CHECK(ok && total > 0 && total < 3500, "a download cut short ends early, without a crash");
    }
    {
        Bytes junk(4000, 0x55);
        Feed fd = { &junk, 0, 100 };
        lossless::Flac f;
        CHECK(!f.open(feed_read, &fd), "not FLAC: refused");
    }

    std::puts("== wav");
    auto wav = [](unsigned tag, int ch, unsigned hz, int bits, bool list, bool ext) {
        Bytes b = { 'R', 'I', 'F', 'F', 0xFF, 0xFF, 0xFF, 0xFF, 'W', 'A', 'V', 'E' };
        auto le = [&](unsigned v, int n) { for (int i = 0; i < n; i++) b.push_back((unsigned char)(v >> (8 * i))); };
        if (list) { b.insert(b.end(), { 'L', 'I', 'S', 'T' }); le(5, 4); le(0x41414141, 4); b.push_back('A'); b.push_back(0); }
        b.insert(b.end(), { 'f', 'm', 't', ' ' });
        le(ext ? 40 : 16, 4);
        le(ext ? 0xFFFE : tag, 2); le(ch, 2); le(hz, 4); le(hz * ch * bits / 8, 4); le(ch * bits / 8, 2); le(bits, 2);
        if (ext) { le(22, 2); le(bits, 2); le(3, 4); le(tag, 2); for (int i = 0; i < 14; i++) b.push_back(0); }
        b.insert(b.end(), { 'd', 'a', 't', 'a' });
        le(0xFFFFFFFF, 4);  // a stream: no length
        return b;
    };
    lossless::Wav w;
    Bytes h = wav(1, 2, 44100, 16, false, false);
    CHECK(lossless::is_wav(h.data(), h.size()) && lossless::wav_header(h.data(), h.size(), &w) == 1 &&
          w.hz == 44100 && w.ch == 2 && w.bits == 16 && w.data_at == h.size(), "16-bit stereo");
    h = wav(1, 2, 96000, 24, true, true);
    w = lossless::Wav();
    CHECK(lossless::wav_header(h.data(), h.size(), &w) == 1 && w.hz == 96000 && w.bits == 24 && w.data_at == h.size(),
          "24-bit EXTENSIBLE after a LIST chunk with an odd size");
    bool needs_more = true;
    for (size_t n = 0; n < h.size(); n++) { lossless::Wav x; needs_more &= lossless::wav_header(h.data(), n, &x) == -1; }
    CHECK(needs_more, "every cut-short header asks for more");
    h = wav(3, 2, 48000, 32, false, false);
    w = lossless::Wav();
    CHECK(lossless::wav_header(h.data(), h.size(), &w) == 0 && w.problem, "float: refused, with a reason");
    h = wav(3, 2, 48000, 32, false, true);
    w = lossless::Wav();
    CHECK(lossless::wav_header(h.data(), h.size(), &w) == 0 && w.problem, "EXTENSIBLE float: refused too");
    h = wav(1, 2, 48000, 12, false, false);
    w = lossless::Wav();
    CHECK(lossless::wav_header(h.data(), h.size(), &w) == 0 && w.problem, "12-bit: refused");
    const unsigned char s24[] = { 0x11, 0x22, 0x33, 0xFF, 0xFF, 0x80 }, s8[] = { 0x00, 0x80, 0xFF };
    short o[3];
    lossless::to_s16(s24, 2, 24, o);
    CHECK(o[0] == 0x3322 && o[1] == (short)0x80FF, "24-bit to 16: the top two bytes");
    lossless::to_s16(s8, 3, 8, o);
    CHECK(o[0] == -32768 && o[1] == 0 && o[2] == 0x7F00, "8-bit is unsigned");

    std::printf(fails ? "%d FAILED\n" : "all ok\n", fails);
    return fails ? 1 : 0;
}
