/* crossfeed.h — headphone crossfeed for libcinder_mono.so (src/cinder-mono.c), host-tested by
 * tools/crossfeed_selftest.cpp.
 *
 * WHAT IT DOES. On speakers each ear hears both speakers; on headphones a hard-panned instrument
 * is in one ear only, which tires over an album. Crossfeed lets a little of each channel's low end
 * reach the other ear:
 *
 *     out_L = L + C * (LP(R) - LP(L))        out_R = R + C * (LP(L) - LP(R))
 *
 * LP is a first-order low-pass at CF_FC. Written this way, anything in the centre (L = R) passes
 * EXACTLY unchanged — vocals and bass keep their level and tone — and only the difference between
 * the channels is blended, below about CF_FC. With C = 0.373 a hard-panned low note arrives
 * 4.5 dB quieter in the far ear than the near one (1 - C : C), the separation of a pair of
 * speakers; the low-pass's own delay (about 0.2 ms at low frequencies) stands in for the time the
 * sound takes round the head. Above a few kHz the channels are untouched.
 *
 * Same safety rules as mono_sum.h: only 2-channel linear PCM, never DSD over PCM (the caller
 * checks), and the result is clamped to the format's range, never wrapped.
 */
#ifndef CINDER_CROSSFEED_H
#define CINDER_CROSSFEED_H

#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include "mono_sum.h"

#define CF_FC 700.0
#define CF_C 0.373f

typedef struct {
    float lp_l, lp_r;   /* the two low-passes, in samples of the stream's own scale */
    float a;            /* exp(-2 pi fc / rate) */
    unsigned rate;
} cf_state;

/* exp(-x) for 0 <= x <= 1 without libm (the shim links nothing but libc and libdl): a Taylor
 * series, to well under float precision in that range. x is 2 pi fc / rate: 0.55 at 8 kHz. */
static inline double cf_exp_neg(double x)
{
    double term = 1.0, sum = 1.0;
    for (int k = 1; k < 20; k++) {
        term *= -x / k;
        sum += term;
    }
    return sum;
}

static inline int32_t cf_clamp(int64_t v, int64_t lo, int64_t hi)
{
    return (int32_t)(v < lo ? lo : v > hi ? hi : v);
}

/* Crossfeed `frames` of stereo PCM in place. Returns 1 if it did, 0 if it left them alone (a
 * format it does not know, or no rate). A new rate restarts the filters. */
static inline int cf_apply(cf_state* s, uint8_t* p, size_t frames, int fmt, unsigned rate)
{
    const int sb = cm_sample_bytes(fmt);
    if (sb == 0 || rate < 8000 || frames == 0) return 0;
    if (rate != s->rate) {
        s->rate = rate;
        s->a = (float)cf_exp_neg(2.0 * 3.14159265358979323846 * CF_FC / (double)rate);
        s->lp_l = s->lp_r = 0.0f;
    }
    int64_t lo, hi;
    switch (fmt) {
    case CM_FMT_S16_LE: lo = -32768; hi = 32767; break;
    case CM_FMT_S32_LE: lo = INT32_MIN; hi = INT32_MAX; break;
    default:            lo = -8388608; hi = 8388607; break;   /* the two 24-bit layouts */
    }
    const float a = s->a, b = 1.0f - a;
    float ll = s->lp_l, lr = s->lp_r;
    for (size_t i = 0; i < frames; i++) {
        uint8_t* f = p + i * (size_t)(2 * sb);
        int32_t l, r;
        switch (fmt) {
        case CM_FMT_S16_LE:
            l = (int16_t)(f[0] | (f[1] << 8));
            r = (int16_t)(f[2] | (f[3] << 8));
            break;
        case CM_FMT_S32_LE:
            memcpy(&l, f, 4);
            memcpy(&r, f + 4, 4);
            break;
        default:   /* S24_3LE, or S24_LE's low three bytes */
            l = cm_s24(f);
            r = cm_s24(f + sb);
            break;
        }
        ll = b * (float)l + a * ll;
        lr = b * (float)r + a * lr;
        const float df = CF_C * (lr - ll);
        const int64_t d = (int64_t)(df + (df >= 0.0f ? 0.5f : -0.5f));
        const int32_t ol = cf_clamp((int64_t)l + d, lo, hi);
        const int32_t orr = cf_clamp((int64_t)r - d, lo, hi);
        switch (fmt) {
        case CM_FMT_S16_LE:
            f[0] = (uint8_t)(ol & 0xFF); f[1] = (uint8_t)((ol >> 8) & 0xFF);
            f[2] = (uint8_t)(orr & 0xFF); f[3] = (uint8_t)((orr >> 8) & 0xFF);
            break;
        case CM_FMT_S32_LE:
            memcpy(f, &ol, 4);
            memcpy(f + 4, &orr, 4);
            break;
        default: {
            uint8_t* g[2] = { f, f + sb };
            const int32_t v[2] = { ol, orr };
            for (int c = 0; c < 2; c++) {
                g[c][0] = (uint8_t)(v[c] & 0xFF);
                g[c][1] = (uint8_t)((v[c] >> 8) & 0xFF);
                g[c][2] = (uint8_t)((v[c] >> 16) & 0xFF);
                if (sb == 4) g[c][3] = (uint8_t)(v[c] < 0 ? 0xFF : 0x00);
            }
            break;
        }
        }
    }
    s->lp_l = ll;
    s->lp_r = lr;
    return 1;
}

#endif /* CINDER_CROSSFEED_H */
