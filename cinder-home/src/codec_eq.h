/* codec_eq.h — build a tone-control table that puts an EQ in the codec itself.
 *
 * WHAT THE "TONE TABLE" IS. /proc/icx_audio_cxd3778gf_data/tct is not Sony's Tone Control (that
 * runs in software, in SoundServiceFw). It is the CXD3778GF's own digital EQ: five biquads, applied
 * to both channels, written into codec RAM by the kernel's adjust_tone_control() every time the
 * output, the amp, the headphone type or the jack changes, and on every resume. Traced in the stock
 * A50 kernel, 2026-09-29: analysis/RE_codec_tone_table.md.
 *
 * Layout: 9 blocks of 320 bytes, then an 8-byte trailer (sum, xor; Wampy's checksum()).
 *   block 0            no headphones
 *   block 1            linear amp ("normal"), ordinary headphones
 *   blocks 2..4        linear amp, Sony NC headphones (NW500N, NW750N, NC31)
 *   block 5            S-Master ("smaster-se"/"-btl"), ordinary headphones
 *   blocks 6..8        S-Master, Sony NC headphones
 * A block is 64 words of 5 bytes: words 0..31 for 44.1 kHz-family rates, 32..63 for 48 kHz-family
 * rates. Each half holds five biquads (b0 b1 b2 a1 a2, 25 words) and seven unused words. A word is
 * a signed 40-bit big-endian Q3.37: 0x2000000000 is 1.0. The feedback terms are stored NEGATED:
 * y = b0 x + b1 x1 + b2 x2 + a1 y1 + a2 y2.
 *
 * In every table Sony ships (A50 tc_1291, NW-WM1A tc_127x, Walkman One's copies) blocks 0, 1 and 5
 * are the identity: for any ordinary headphones the codec EQ is there, switched on, and flat. This
 * header fills blocks 1 and 5 with a five-band EQ and leaves the NC blocks as the source had them.
 *
 * Plain C, no allocation, so the setuid helper and the host self-test share it.
 */
#ifndef CINDER_CODEC_EQ_H
#define CINDER_CODEC_EQ_H

#include <math.h>
#include <string.h>

#define CODEC_EQ_PI 3.14159265358979323846
#define CODEC_EQ_TABLE_BYTES 2888
#define CODEC_EQ_BODY_BYTES 2880
#define CODEC_EQ_BLOCK_BYTES 320
#define CODEC_EQ_BANDS 5
/* Gains are half-dB steps, the same unit as Sony's ten-band EQ. -12 dB .. +6 dB: a boost is paid
 * for by a matching cut in level (see codec_eq_pregain), so the ceiling is kept small. */
#define CODEC_EQ_GAIN_MIN (-24)
#define CODEC_EQ_GAIN_MAX 12

/* Blocks this EQ owns: ordinary headphones on either amp. */
#define CODEC_EQ_BLOCK_LINEAR 1
#define CODEC_EQ_BLOCK_SMASTER 5

enum { CODEC_EQ_LOWSHELF, CODEC_EQ_PEAK, CODEC_EQ_HIGHSHELF };

/* The five bands. Fixed, so the helper takes gains only and a caller never picks a frequency. */
static const struct { int kind; double hz; double q; } CODEC_EQ_BAND[CODEC_EQ_BANDS] = {
    { CODEC_EQ_LOWSHELF,    100.0, 0.707 },
    { CODEC_EQ_PEAK,        400.0, 0.9 },
    { CODEC_EQ_PEAK,       1500.0, 0.9 },
    { CODEC_EQ_PEAK,       4000.0, 0.9 },
    { CODEC_EQ_HIGHSHELF, 10000.0, 0.707 },
};

/* The two rate families a block carries, in block order. */
static const double CODEC_EQ_FS[2] = { 44100.0, 48000.0 };

static int codec_eq_clamp(int g)
{
    return g < CODEC_EQ_GAIN_MIN ? CODEC_EQ_GAIN_MIN : g > CODEC_EQ_GAIN_MAX ? CODEC_EQ_GAIN_MAX : g;
}

/* One band as codec coefficients {b0 b1 b2 a1 a2}, a1/a2 already negated. RBJ Audio EQ Cookbook.
 * A zero gain is the exact identity, not a filter that happens to be flat, so an all-zero EQ
 * reproduces Sony's bytes and trailer exactly. */
static void codec_eq_band(int band, int half_db, double fs, double c[5])
{
    const double db = half_db / 2.0;
    if (half_db == 0) {
        c[0] = 1.0; c[1] = c[2] = c[3] = c[4] = 0.0;
        return;
    }
    const double A = pow(10.0, db / 40.0);
    const double w0 = 2.0 * CODEC_EQ_PI * CODEC_EQ_BAND[band].hz / fs;
    const double cw = cos(w0), sw = sin(w0);
    const double alpha = sw / (2.0 * CODEC_EQ_BAND[band].q);
    const double sa = 2.0 * sqrt(A) * alpha;
    double b0, b1, b2, a0, a1, a2;

    switch (CODEC_EQ_BAND[band].kind) {
    case CODEC_EQ_LOWSHELF:
        b0 = A * ((A + 1) - (A - 1) * cw + sa);
        b1 = 2 * A * ((A - 1) - (A + 1) * cw);
        b2 = A * ((A + 1) - (A - 1) * cw - sa);
        a0 = (A + 1) + (A - 1) * cw + sa;
        a1 = -2 * ((A - 1) + (A + 1) * cw);
        a2 = (A + 1) + (A - 1) * cw - sa;
        break;
    case CODEC_EQ_HIGHSHELF:
        b0 = A * ((A + 1) + (A - 1) * cw + sa);
        b1 = -2 * A * ((A - 1) + (A + 1) * cw);
        b2 = A * ((A + 1) + (A - 1) * cw - sa);
        a0 = (A + 1) - (A - 1) * cw + sa;
        a1 = 2 * ((A - 1) - (A + 1) * cw);
        a2 = (A + 1) - (A - 1) * cw - sa;
        break;
    default:
        b0 = 1 + alpha * A;
        b1 = -2 * cw;
        b2 = 1 - alpha * A;
        a0 = 1 + alpha / A;
        a1 = -2 * cw;
        a2 = 1 - alpha / A;
        break;
    }
    c[0] = b0 / a0; c[1] = b1 / a0; c[2] = b2 / a0;
    c[3] = -a1 / a0; c[4] = -a2 / a0;
}

/* |H| in dB of a cascade of `n` codec biquads at `hz`. */
static double codec_eq_response_db(const double c[][5], int n, double fs, double hz)
{
    const double w = 2.0 * CODEC_EQ_PI * hz / fs;
    const double c1 = cos(w), s1 = -sin(w), c2 = cos(2 * w), s2 = -sin(2 * w);
    double db = 0.0;
    int i;
    for (i = 0; i < n; i++) {
        const double nr = c[i][0] + c[i][1] * c1 + c[i][2] * c2;
        const double ni = c[i][1] * s1 + c[i][2] * s2;
        const double dr = 1.0 - c[i][3] * c1 - c[i][4] * c2;
        const double di = -c[i][3] * s1 - c[i][4] * s2;
        db += 10.0 * log10((nr * nr + ni * ni) / (dr * dr + di * di));
    }
    return db;
}

/* The loudest point of the cascade, 20 Hz to 20 kHz (or just under Nyquist), 1/24 octave. */
static double codec_eq_peak_db(const double c[][5], int n, double fs)
{
    double peak = -1e9, hz;
    const double top = fs * 0.45 < 20000.0 ? fs * 0.45 : 20000.0;
    for (hz = 20.0; hz <= top; hz *= 1.0293022366) { /* 2^(1/24) */
        const double db = codec_eq_response_db(c, n, fs, hz);
        if (db > peak) peak = db;
    }
    return peak;
}

/* Five bands for one rate. If any band boosts, the first biquad's feed-forward terms are scaled so
 * the cascade never rises above 0 dB: a boost here is relative, and the codec path after the EQ has
 * no headroom anyone has measured. So +6 dB treble plays as treble flat and everything else 6 dB
 * down, never as a clipped +6. */
static void codec_eq_design(const int gains[CODEC_EQ_BANDS], double fs, double c[CODEC_EQ_BANDS][5])
{
    int i, boost = 0;
    for (i = 0; i < CODEC_EQ_BANDS; i++) {
        const int g = codec_eq_clamp(gains[i]);
        codec_eq_band(i, g, fs, c[i]);
        if (g > 0) boost = 1;
    }
    if (boost) {
        const double peak = codec_eq_peak_db((const double (*)[5])c, CODEC_EQ_BANDS, fs);
        if (peak > 0.0) {
            const double s = pow(10.0, -peak / 20.0);
            c[0][0] *= s; c[0][1] *= s; c[0][2] *= s;
        }
    }
}

/* One coefficient as a Q3.37 word. Saturates at the format's range rather than wrapping: a wrapped
 * coefficient is an unstable filter, which is a howl in someone's ears. */
static void codec_eq_word(double v, unsigned char out[5])
{
    const double lim = 549755813887.0; /* 2^39 - 1 */
    double q = floor(v * 137438953472.0 + 0.5); /* 2^37 */
    long long n;
    int i;
    if (q > lim) q = lim;
    if (q < -lim - 1) q = -lim - 1;
    n = (long long)q;
    for (i = 4; i >= 0; i--) {
        out[i] = (unsigned char)(n & 0xff);
        n >>= 8;
    }
}

/* Fill one 320-byte block with the EQ, both rate halves. */
static void codec_eq_block(const int gains[CODEC_EQ_BANDS], unsigned char block[CODEC_EQ_BLOCK_BYTES])
{
    int half, b, k;
    memset(block, 0, CODEC_EQ_BLOCK_BYTES);
    for (half = 0; half < 2; half++) {
        double c[CODEC_EQ_BANDS][5];
        codec_eq_design(gains, CODEC_EQ_FS[half], c);
        for (b = 0; b < CODEC_EQ_BANDS; b++)
            for (k = 0; k < 5; k++)
                codec_eq_word(c[b][k], block + half * 160 + (b * 5 + k) * 5);
    }
}

/* The 8-byte trailer every codec table ends with: a byte sum, then an xor of each byte shifted by
 * (n % 4) * 8, both little-endian u32. */
static void codec_eq_trailer(const unsigned char *body, int n, unsigned char out[8])
{
    unsigned int sum = 0, xr = 0;
    int i;
    for (i = 0; i < n; i++) {
        sum += body[i];
        xr ^= (unsigned int)body[i] << ((i % 4) * 8);
    }
    for (i = 0; i < 4; i++) {
        out[i] = (unsigned char)(sum >> (8 * i));
        out[4 + i] = (unsigned char)(xr >> (8 * i));
    }
}

/* A whole table: `src` (a verified Sony tone table) with blocks 1 and 5 replaced and the trailer
 * recomputed. `src` and `out` may be the same buffer. */
static void codec_eq_table(const unsigned char *src, const int gains[CODEC_EQ_BANDS],
                           unsigned char out[CODEC_EQ_TABLE_BYTES])
{
    unsigned char block[CODEC_EQ_BLOCK_BYTES];
    if (out != src)
        memcpy(out, src, CODEC_EQ_BODY_BYTES);
    codec_eq_block(gains, block);
    memcpy(out + CODEC_EQ_BLOCK_LINEAR * CODEC_EQ_BLOCK_BYTES, block, CODEC_EQ_BLOCK_BYTES);
    memcpy(out + CODEC_EQ_BLOCK_SMASTER * CODEC_EQ_BLOCK_BYTES, block, CODEC_EQ_BLOCK_BYTES);
    codec_eq_trailer(out, CODEC_EQ_BODY_BYTES, out + CODEC_EQ_BODY_BYTES);
}

#endif
