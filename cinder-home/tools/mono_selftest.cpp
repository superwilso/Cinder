// mono_selftest — host test for the sums and the Bluetooth stream walk in src/mono_sum.h.
//
// The two ways this feature can hurt someone are both silent in a unit test of "does L+R/2 work":
// full-scale noise from summing something that is not PCM (DSD over PCM), and a device reboot from
// changing a byte the transmitter still reads as a frame length. So most of this file is about the
// bytes that must NOT change.
#include "../src/mono_sum.h"
#include <cstdio>
#include <cstdint>
#include <cstring>
#include <vector>

static int fails = 0;
static void check(bool ok, const char* what) {
    std::printf("%s  %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok) fails++;
}

static void put16(std::vector<uint8_t>& v, int16_t x) { v.push_back((uint8_t)(x & 0xFF)); v.push_back((uint8_t)((x >> 8) & 0xFF)); }
static void put32le(std::vector<uint8_t>& v, uint32_t x) { for (int i = 0; i < 4; i++) v.push_back((uint8_t)(x >> (8 * i))); }
static int16_t get16(const uint8_t* p) { return (int16_t)(p[0] | (p[1] << 8)); }

// The handshake the LDAC bridge and Sony's HAL both send: [1][28][payload], channels at +4, rate at +24.
static std::vector<uint8_t> handshake(uint32_t ch, uint32_t rate) {
    std::vector<uint8_t> v;
    put32le(v, 1); put32le(v, 28);
    std::vector<uint8_t> pay(28, 0);
    std::memcpy(&pay[4], &ch, 4);
    std::memcpy(&pay[24], &rate, 4);
    v.insert(v.end(), pay.begin(), pay.end());
    return v;
}

static std::vector<uint8_t> stereo16(int frames, int16_t l0, int16_t r0) {
    std::vector<uint8_t> v;
    for (int i = 0; i < frames; i++) { put16(v, (int16_t)(l0 + i)); put16(v, (int16_t)(r0 - i)); }
    return v;
}

// Push `stream` through the walk in writes of the given sizes, with a kernel that accepts at most
// `accept` bytes per write (the producer retrying the rest), and return what reached the socket.
static std::vector<uint8_t> pump(const std::vector<uint8_t>& stream, const std::vector<size_t>& writes,
                                 size_t accept, int mono, cm_bt_stream* end_state = nullptr,
                                 const cm_mix* mix = nullptr) {
    cm_bt_stream st;
    std::memset(&st, 0, sizeof st);
    std::vector<uint8_t> wire;
    size_t pos = 0, wi = 0;
    while (pos < stream.size()) {
        size_t want = wi < writes.size() ? writes[wi++] : stream.size() - pos;
        if (want > stream.size() - pos) want = stream.size() - pos;
        size_t sent = 0;
        while (sent < want) {                         // the producer loops until its buffer is gone
            const uint8_t* in = &stream[pos + sent];
            const size_t n = want - sent;
            std::vector<uint8_t> out(n);
            cm_bt_stream tmp = st;
            cm_bt_walk(&tmp, in, n, n, out.data(), mono, mix, nullptr);
            const size_t took = n < accept ? n : accept;
            wire.insert(wire.end(), out.begin(), out.begin() + (long)took);
            cm_bt_walk(&st, in, n, took, nullptr, mono, nullptr, out.data());
            sent += took;
        }
        pos += want;
    }
    if (end_state) *end_state = st;
    return wire;
}

int main() {
    // ── the sums ────────────────────────────────────────────────────────────────────────────
    {
        uint8_t f[4];
        std::vector<uint8_t> v; put16(v, 1000); put16(v, -3000);
        std::memcpy(f, v.data(), 4);
        cm_sum_frame(f, CM_FMT_S16_LE);
        check(get16(f) == -1000 && get16(f + 2) == -1000, "S16: (1000 + -3000) / 2 in both channels");
        v.clear(); put16(v, 32767); put16(v, 32767); std::memcpy(f, v.data(), 4);
        cm_sum_frame(f, CM_FMT_S16_LE);
        check(get16(f) == 32767 && get16(f + 2) == 32767, "S16: full scale in both stays full scale (halved, not clipped)");
        v.clear(); put16(v, -32768); put16(v, -32768); std::memcpy(f, v.data(), 4);
        cm_sum_frame(f, CM_FMT_S16_LE);
        check(get16(f) == -32768, "S16: negative full scale does not wrap");
    }
    {
        uint8_t f[6] = { 0xFF, 0xFF, 0x7F, 0x00, 0x00, 0x80 };   // +8388607, -8388608
        cm_sum_frame(f, CM_FMT_S24_3LE);
        check(cm_s24(f) == 0 && cm_s24(f + 3) == 0, "S24_3LE: +max and -max sum to 0 ((a+b)/2 truncates)");
        uint8_t g[6] = { 0x00, 0x00, 0x10, 0x00, 0x00, 0x30 };   // 0x100000, 0x300000
        cm_sum_frame(g, CM_FMT_S24_3LE);
        check(cm_s24(g) == 0x200000 && cm_s24(g + 3) == 0x200000, "S24_3LE: average in both channels");
    }
    {
        int32_t l = 2000000000, r = 2000000000;
        uint8_t f[8];
        std::memcpy(f, &l, 4); std::memcpy(f + 4, &r, 4);
        cm_sum_frame(f, CM_FMT_S32_LE);
        int32_t a, b; std::memcpy(&a, f, 4); std::memcpy(&b, f + 4, 4);
        check(a == 2000000000 && b == 2000000000, "S32: no 32-bit overflow in the sum");
        l = -100; r = 50;
        std::memcpy(f, &l, 4); std::memcpy(f + 4, &r, 4);
        cm_sum_frame(f, CM_FMT_S32_LE);
        std::memcpy(&a, f, 4);
        check(a == -25, "S32: signed average");
    }
    {
        uint8_t f[8] = { 0x00, 0x00, 0xF0, 0xFF, 0x00, 0x00, 0xF0, 0xFF };   // -0x100000 both, sign-extended
        cm_sum_frame(f, CM_FMT_S24_LE);
        check(cm_s24(f) == -0x100000 && f[3] == 0xFF && f[7] == 0xFF, "S24_LE: negative keeps its sign-extension byte");
    }

    // ── what must be left alone ─────────────────────────────────────────────────────────────
    {
        std::vector<uint8_t> dop;
        for (int i = 0; i < 32; i++) {
            const uint8_t m = (i % 2) ? 0xFA : 0x05;
            for (int c = 0; c < 2; c++) { dop.push_back((uint8_t)(0x69 + c)); dop.push_back(0x96); dop.push_back(m); }
        }
        std::vector<uint8_t> copy = dop;
        check(cm_sum_buffer(copy.data(), 32, CM_FMT_S24_3LE, 2) == 0 && copy == dop, "DoP (S24_3LE) passes through byte for byte");
        std::vector<uint8_t> dop32;
        for (int i = 0; i < 32; i++) {
            const uint8_t m = (i % 2) ? 0xFA : 0x05;
            for (int c = 0; c < 2; c++) { dop32.push_back(0); dop32.push_back(0x69); dop32.push_back(0x96); dop32.push_back(m); }
        }
        copy = dop32;
        check(cm_sum_buffer(copy.data(), 32, CM_FMT_S32_LE, 2) == 0 && copy == dop32, "DoP in S32_LE passes through");
        // A PCM buffer whose top bytes happen to be 0x05 once is still PCM.
        std::vector<uint8_t> pcm;
        for (int i = 0; i < 32; i++)
            for (int c = 0; c < 2; c++) { pcm.push_back((uint8_t)i); pcm.push_back((uint8_t)(c * 7)); pcm.push_back(0x05); }
        copy = pcm;
        check(cm_sum_buffer(copy.data(), 32, CM_FMT_S24_3LE, 2) == 1, "0x05 without the alternation is PCM, and is summed");
    }
    {
        std::vector<uint8_t> six(36, 0x11);
        std::vector<uint8_t> copy = six;
        check(cm_sum_buffer(copy.data(), 6, CM_FMT_S16_LE, 3) == 0 && copy == six, "3 channels: left alone");
        check(cm_sum_buffer(copy.data(), 6, 48 /* DSD_U8 */, 2) == 0 && copy == six, "native DSD format: left alone");
    }

    // ── the Bluetooth stream ────────────────────────────────────────────────────────────────
    const std::vector<uint8_t> hs = handshake(2, 44100);
    const std::vector<uint8_t> pcm = stereo16(64, 100, -100);
    std::vector<uint8_t> stream = hs;
    stream.insert(stream.end(), pcm.begin(), pcm.end());

    {
        cm_bt_stream end;
        const std::vector<uint8_t> wire = pump(stream, { stream.size() }, 1 << 20, 1, &end);
        check(std::equal(hs.begin(), hs.end(), wire.begin()), "the handshake reaches the socket unchanged");
        bool summed = true;
        for (size_t i = hs.size(); i + 4 <= wire.size(); i += 4) {
            const int k = (int)(i - hs.size()) / 4;
            const int16_t want = (int16_t)(((100 + k) + (-100 - k)) / 2);
            if (get16(&wire[i]) != want || get16(&wire[i + 2]) != want) summed = false;
        }
        check(summed, "PCM after the handshake is summed");
        check(end.mode == CM_BT_PCM && end.channels == 2 && end.rate == 44100, "the walk read 2 ch / 44100 Hz from the handshake");
    }
    {
        // Every byte the kernel takes one at a time, and writes of awkward sizes: the output must be
        // the same as one big write.
        const std::vector<uint8_t> ref = pump(stream, { stream.size() }, 1 << 20, 1);
        // Writes cut the handshake anywhere, and the PCM on frame boundaries (a producer buffer that
        // splits a FRAME is the case below, and is not expected to match).
        check(pump(stream, { 3, 7, 1, 25, 8, 20, 12 }, 1 << 20, 1) == ref, "odd write sizes: same bytes on the wire");
        check(pump(stream, { stream.size() }, 1, 1) == ref, "a kernel that takes one byte at a time: same bytes");
        check(pump(stream, { 13, 6, 17, 28, 4 }, 3, 1) == ref, "odd writes AND partial accepts: same bytes");
    }
    {
        // A frame split by the PRODUCER's buffer (not by the kernel) cannot be summed as it goes; it
        // must go out whole-original, never half-summed.
        std::vector<uint8_t> s = hs;
        std::vector<uint8_t> two = stereo16(2, 1000, -3000);   // (1000,-3000) (1001,-3001)
        s.insert(s.end(), two.begin(), two.end());
        const std::vector<uint8_t> wire = pump(s, { hs.size() + 1, 7 }, 1 << 20, 1);
        const uint8_t* f0 = &wire[hs.size()];
        check(get16(f0) == 1000 && get16(f0 + 2) == -3000, "a frame split by the producer goes out whole-original");
        check(get16(f0 + 4) == -1000 && get16(f0 + 6) == -1000, "…and the next whole frame is summed again");
    }
    {
        // Mono off: the stream is untouched, but the walk still counts bytes, so switching on later
        // pairs the right samples.
        check(pump(stream, { 17, 5, 100 }, 7, 0) == stream, "mono off: every byte as it came");
        std::vector<uint8_t> s = hs;
        std::vector<uint8_t> a = stereo16(3, 10, 20);
        s.insert(s.end(), a.begin(), a.end());
        cm_bt_stream st; std::memset(&st, 0, sizeof st);
        cm_bt_walk(&st, s.data(), s.size(), hs.size() + 6, nullptr, 0, nullptr, nullptr);        // off, stops mid-frame
        std::vector<uint8_t> rest(s.begin() + (long)(hs.size() + 6), s.end());
        std::vector<uint8_t> out(rest.size());
        cm_bt_stream tmp = st;
        cm_bt_walk(&tmp, rest.data(), rest.size(), rest.size(), out.data(), 1, nullptr, nullptr);  // on
        check(get16(&out[2]) == 15 && get16(&out[4]) == 15, "switched on mid-stream: the next whole frame is (12+18)/2 in both");
        check(out[0] == rest[0] && out[1] == rest[1], "…and the half-sent frame finishes as it began");
    }
    {
        // Frames the walk does not know stop it for good; a mono stream is left alone.
        std::vector<uint8_t> bad; put32le(bad, 7); put32le(bad, 4); put16(bad, 1); put16(bad, 2);
        cm_bt_stream end;
        check(pump(bad, { bad.size() }, 1 << 20, 1, &end) == bad && end.mode == CM_BT_GIVEUP, "unknown frame type: left alone for good");
        std::vector<uint8_t> m1 = handshake(1, 48000);
        std::vector<uint8_t> tail = stereo16(4, 5, 9);
        m1.insert(m1.end(), tail.begin(), tail.end());
        check(pump(m1, { m1.size() }, 1 << 20, 1) == m1, "a 1-channel stream is not touched");
        std::vector<uint8_t> t0; put32le(t0, 0); put32le(t0, 0);
        std::vector<uint8_t> t2; put32le(t2, 2); put32le(t2, 12); for (int i = 0; i < 12; i++) t2.push_back(0xAB);
        std::vector<uint8_t> seq = t0; seq.insert(seq.end(), t2.begin(), t2.end()); seq.insert(seq.end(), stream.begin(), stream.end());
        const std::vector<uint8_t> w = pump(seq, { 5, 11, 40, 3 }, 1 << 20, 1);
        check(std::equal(seq.begin(), seq.begin() + (long)(t0.size() + t2.size() + hs.size()), w.begin()),
              "type-0 and type-2 frames before the handshake pass untouched");
        check(get16(&w[t0.size() + t2.size() + hs.size()]) == 0, "…and the PCM after them is summed ((100 + -100)/2)");
    }

    // ── the mix hook (soundscapes): PCM only, whole frames only, the handshake's rate ──────────────
    {
        struct Seen { uint32_t rate = 0; size_t frames = 0; };
        static Seen seen;
        cm_mix add100 = { [](void*, uint8_t* p, size_t n, uint32_t rate) {
                              seen.rate = rate;
                              seen.frames += n;
                              for (size_t i = 0; i < n * 2; i++) {
                                  const int v = get16(p + 2 * i) + 100;
                                  p[2 * i] = (uint8_t)(v & 0xFF);
                                  p[2 * i + 1] = (uint8_t)((v >> 8) & 0xFF);
                              }
                          }, nullptr };
        const std::vector<uint8_t> hs2 = handshake(2, 48000);
        std::vector<uint8_t> st2 = hs2;
        const std::vector<uint8_t> pcm = stereo16(64, 1000, -1000);
        st2.insert(st2.end(), pcm.begin(), pcm.end());
        const std::vector<uint8_t> w = pump(st2, { st2.size() }, 1 << 20, 0, nullptr, &add100);
        check(std::equal(hs2.begin(), hs2.end(), w.begin()), "mix: the handshake goes out byte for byte");
        check(get16(&w[hs2.size()]) == 1100 && get16(&w[hs2.size() + 2]) == -900, "mix: the PCM after it carries the mix");
        check(seen.rate == 48000 && seen.frames == 64, "mix: called with the handshake's rate, for every whole frame");
        // Odd writes and partial accepts: the frames the hook skips (split ones) go out as they came,
        // every other one carries the mix, and alignment never slips.
        seen = Seen();
        const std::vector<uint8_t> w2 = pump(st2, { 13, 6, 17, 28, 4, 9 }, 5, 0, nullptr, &add100);
        int mixed = 0, plain = 0, wrong = 0;
        for (size_t f = 0; f < 64; f++) {
            const int l = get16(&w2[hs2.size() + f * 4]), r = get16(&w2[hs2.size() + f * 4 + 2]);
            const int l0 = 1000 + (int)f, r0 = -1000 - (int)f;
            if (l == l0 + 100 && r == r0 + 100) mixed++;
            else if (l == l0 && r == r0) plain++;
            else wrong++;
        }
        check(wrong == 0 && mixed > 0, "mix with odd writes and partial accepts: every frame whole-mixed or whole-plain");
        // and with a kernel that takes ONE byte at a time, every frame is still one or the other
        const std::vector<uint8_t> w1 = pump(st2, { st2.size() }, 1, 0, nullptr, &add100);
        wrong = 0;
        for (size_t f = 0; f < 64; f++) {
            const int l = get16(&w1[hs2.size() + f * 4]), r = get16(&w1[hs2.size() + f * 4 + 2]);
            const int l0 = 1000 + (int)f, r0 = -1000 - (int)f;
            wrong += !((l == l0 + 100 && r == r0 + 100) || (l == l0 && r == r0));
        }
        check(wrong == 0, "mix, one byte accepted per write: no frame leaves torn");
        // Mono and the mix together: summed first, then mixed.
        const std::vector<uint8_t> w3 = pump(st2, { st2.size() }, 1 << 20, 1, nullptr, &add100);
        check(get16(&w3[hs2.size()]) == 100 && get16(&w3[hs2.size() + 2]) == 100, "mono then mix: (1000 + -1000)/2 + 100");
        // Never before the handshake, never on a stream the walk gave up on.
        seen = Seen();
        std::vector<uint8_t> bad; put32le(bad, 7); put32le(bad, 4); put16(bad, 1); put16(bad, 2);
        check(pump(bad, { bad.size() }, 1 << 20, 0, nullptr, &add100) == bad && seen.frames == 0,
              "mix: a stream the walk does not know is never touched");
    }

    std::printf(fails ? "\nmono_selftest: %d FAILED\n" : "\nmono_selftest: all passed\n", fails);
    return fails ? 1 : 0;
}
