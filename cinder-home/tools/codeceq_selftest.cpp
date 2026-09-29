// codeceq_selftest — host test for the codec EQ table builder (src/codec_eq.h).
//
// WHY IT IS WORTH A TEST. The table goes into the codec's own DSP through a root-only proc node,
// straight to someone's ears. A wrong coefficient is not a wrong colour on a screen: a sign slip or
// a wrapped Q3.37 word is an unstable filter, and an unstable filter is a howl at full scale. None
// of that can be checked on the device without a jack rig, so the arithmetic is checked here, from
// the SAME header cinder-voltable uses.
//
// Sony's tables are not in the tree, so the "stock" table here is built the way every Sony table
// is laid out for ordinary headphones: nine blocks of identity biquads. The real tables were
// checked against the same functions offline (analysis/RE_codec_tone_table.md).
#include <cmath>
#include <cstdio>
#include <cstring>
#include "../src/codec_eq.h"

static int fails = 0;
static void check(bool ok, const char* what) {
    std::printf("  %-4s %s\n", ok ? "ok" : "FAIL", what);
    if (!ok) fails = 1;
}

static double word(const unsigned char* p) {
    long long v = 0;
    for (int i = 0; i < 5; i++) v = (v << 8) | p[i];
    if (v >= (1LL << 39)) v -= (1LL << 40);
    return (double)v / 137438953472.0;
}

// Decode one rate half of a block back into coefficients.
static void decode(const unsigned char* block, int half, double c[CODEC_EQ_BANDS][5]) {
    for (int b = 0; b < CODEC_EQ_BANDS; b++)
        for (int k = 0; k < 5; k++) c[b][k] = word(block + half * 160 + (b * 5 + k) * 5);
}

static void identity_table(unsigned char t[CODEC_EQ_TABLE_BYTES]) {
    std::memset(t, 0, CODEC_EQ_TABLE_BYTES);
    for (int blk = 0; blk < 9; blk++)
        for (int half = 0; half < 2; half++)
            for (int b = 0; b < CODEC_EQ_BANDS; b++)
                t[blk * 320 + half * 160 + b * 25] = 0x20;
    codec_eq_trailer(t, CODEC_EQ_BODY_BYTES, t + CODEC_EQ_BODY_BYTES);
}

static bool stable(const double c[CODEC_EQ_BANDS][5]) {
    // Denominator 1 - a1 z^-1 - a2 z^-2 (a1/a2 stored negated): the stability triangle.
    for (int b = 0; b < CODEC_EQ_BANDS; b++) {
        const double a1 = c[b][3], a2 = c[b][4];
        if (!(std::fabs(a2) < 1.0 && std::fabs(a1) < 1.0 - a2)) return false;
    }
    return true;
}

static double at(const unsigned char* block, int half, double hz) {
    double c[CODEC_EQ_BANDS][5];
    decode(block, half, c);
    return codec_eq_response_db(c, CODEC_EQ_BANDS, CODEC_EQ_FS[half], hz);
}

int main() {
    std::printf("test 1: the trailer is Sony's sum/xor\n");
    {
        const unsigned char v[5] = {1, 2, 3, 4, 5};
        unsigned char t[8];
        codec_eq_trailer(v, 5, t);
        // sum 15; xor 1 ^ 2<<8 ^ 3<<16 ^ 4<<24 ^ 5 = 0x04030204
        const unsigned char want[8] = {15, 0, 0, 0, 0x04, 0x02, 0x03, 0x04};
        check(std::memcmp(t, want, 8) == 0, "hand-computed vector");
    }

    std::printf("test 2: a flat EQ reproduces the source table byte for byte\n");
    {
        unsigned char src[CODEC_EQ_TABLE_BYTES], out[CODEC_EQ_TABLE_BYTES];
        identity_table(src);
        const int flat[CODEC_EQ_BANDS] = {0, 0, 0, 0, 0};
        codec_eq_table(src, flat, out);
        check(std::memcmp(src, out, CODEC_EQ_TABLE_BYTES) == 0, "all zero = identity, same trailer");
    }

    std::printf("test 3: only the two ordinary-headphone blocks change\n");
    {
        unsigned char src[CODEC_EQ_TABLE_BYTES], out[CODEC_EQ_TABLE_BYTES];
        identity_table(src);
        src[2 * 320 + 7] = 0x5a;  // a stand-in for Sony's NC compensation
        codec_eq_trailer(src, CODEC_EQ_BODY_BYTES, src + CODEC_EQ_BODY_BYTES);
        const int g[CODEC_EQ_BANDS] = {6, 0, -4, 0, 3};
        codec_eq_table(src, g, out);
        bool others = true;
        for (int blk = 0; blk < 9; blk++) {
            if (blk == CODEC_EQ_BLOCK_LINEAR || blk == CODEC_EQ_BLOCK_SMASTER) continue;
            if (std::memcmp(src + blk * 320, out + blk * 320, 320) != 0) others = false;
        }
        check(others, "blocks 0, 2-4, 6-8 untouched (NC compensation kept)");
        check(std::memcmp(out + 320, out + 5 * 320, 320) == 0, "linear and S-Master get the same EQ");
        check(std::memcmp(out + 320, src + 320, 320) != 0, "block 1 did change");
        unsigned char t[8];
        codec_eq_trailer(out, CODEC_EQ_BODY_BYTES, t);
        check(std::memcmp(t, out + CODEC_EQ_BODY_BYTES, 8) == 0, "trailer recomputed");
        bool pad = true;
        for (int half = 0; half < 2; half++)
            for (int i = 125; i < 160; i++)
                if (out[320 + half * 160 + i] != 0) pad = false;
        check(pad, "the seven unused words of each half stay zero");
    }

    std::printf("test 4: a cut is a cut, at the band's own frequency, in both rate halves\n");
    {
        unsigned char blk[320];
        const int g[CODEC_EQ_BANDS] = {-24, 0, 0, 0, 0};  // bass -12 dB
        codec_eq_block(g, blk);
        for (int half = 0; half < 2; half++) {
            char what[96];
            std::snprintf(what, sizeof what, "%s: 25 Hz near -12 dB (%.2f)",
                          half ? "48k" : "44.1k", at(blk, half, 25));
            check(std::fabs(at(blk, half, 25) + 12.0) < 0.6, what);
            std::snprintf(what, sizeof what, "%s: 3 kHz untouched (%.3f dB)",
                          half ? "48k" : "44.1k", at(blk, half, 3000));
            check(std::fabs(at(blk, half, 3000)) < 0.1, what);
        }
    }

    std::printf("test 5: a boost never goes above 0 dB; it lowers everything else instead\n");
    {
        unsigned char blk[320];
        const int g[CODEC_EQ_BANDS] = {0, 0, 0, 0, 12};  // treble +6 dB
        codec_eq_block(g, blk);
        for (int half = 0; half < 2; half++) {
            double c[CODEC_EQ_BANDS][5];
            decode(blk, half, c);
            const double peak = codec_eq_peak_db(c, CODEC_EQ_BANDS, CODEC_EQ_FS[half]);
            char what[96];
            std::snprintf(what, sizeof what, "%s: peak %.3f dB <= 0", half ? "48k" : "44.1k", peak);
            check(peak <= 0.01, what);
            const double tilt = at(blk, half, 16000) - at(blk, half, 200);
            std::snprintf(what, sizeof what, "%s: 16 kHz sits ~6 dB over 200 Hz (%.2f)",
                          half ? "48k" : "44.1k", tilt);
            check(std::fabs(tilt - 6.0) < 0.7, what);
        }
    }

    std::printf("test 6: each peak band lands on its own frequency\n");
    {
        for (int b = 1; b <= 3; b++) {
            unsigned char blk[320];
            int g[CODEC_EQ_BANDS] = {0, 0, 0, 0, 0};
            g[b] = -12;  // -6 dB
            codec_eq_block(g, blk);
            const double f = CODEC_EQ_BAND[b].hz;
            char what[96];
            std::snprintf(what, sizeof what, "band %d: -6 dB at %.0f Hz (%.2f)", b, f, at(blk, 0, f));
            check(std::fabs(at(blk, 0, f) + 6.0) < 0.05, what);
            std::snprintf(what, sizeof what, "band %d: 48k half agrees (%.2f)", b, at(blk, 1, f));
            check(std::fabs(at(blk, 1, f) + 6.0) < 0.05, what);
        }
    }

    std::printf("test 7: every extreme setting is a stable filter and survives Q3.37\n");
    {
        bool all_stable = true, round_trip = true;
        for (int m = 0; m < 243; m++) {  // every band at min, 0 or max: 3^5 combinations
            int g[CODEC_EQ_BANDS], x = m;
            for (int b = 0; b < CODEC_EQ_BANDS; b++) {
                const int pick = x % 3;
                x /= 3;
                g[b] = pick == 0 ? CODEC_EQ_GAIN_MIN : pick == 1 ? 0 : CODEC_EQ_GAIN_MAX;
            }
            unsigned char blk[320];
            codec_eq_block(g, blk);
            for (int half = 0; half < 2; half++) {
                double c[CODEC_EQ_BANDS][5], want[CODEC_EQ_BANDS][5];
                decode(blk, half, c);
                codec_eq_design(g, CODEC_EQ_FS[half], want);
                if (!stable(c)) all_stable = false;
                for (int b = 0; b < CODEC_EQ_BANDS; b++)
                    for (int k = 0; k < 5; k++)
                        if (std::fabs(c[b][k] - want[b][k]) > 1e-11) round_trip = false;
            }
        }
        check(all_stable, "243 combinations x 2 rates: every pole inside the unit circle");
        check(round_trip, "every coefficient decodes back to within 2^-37");
    }

    std::printf("test 8: out-of-range gains clamp, never wrap\n");
    {
        unsigned char a[320], b[320];
        const int wild[CODEC_EQ_BANDS] = {99, -99, 1000, -1000, 13};
        const int sane[CODEC_EQ_BANDS] = {CODEC_EQ_GAIN_MAX, CODEC_EQ_GAIN_MIN, CODEC_EQ_GAIN_MAX,
                                          CODEC_EQ_GAIN_MIN, CODEC_EQ_GAIN_MAX};
        codec_eq_block(wild, a);
        codec_eq_block(sane, b);
        check(std::memcmp(a, b, 320) == 0, "same bytes as the clamped gains");
        unsigned char w[5];
        codec_eq_word(100.0, w);
        check(w[0] == 0x7f && w[4] == 0xff, "a coefficient past +4 saturates, not wraps");
        codec_eq_word(-100.0, w);
        check(w[0] == 0x80 && w[4] == 0x00, "a coefficient past -4 saturates, not wraps");
        codec_eq_word(1.0, w);
        check(w[0] == 0x20 && w[1] == 0 && w[4] == 0, "1.0 is 0x2000000000, as in Sony's tables");
    }

    std::printf(fails ? "FAILED\n" : "all passed\n");
    return fails;
}
