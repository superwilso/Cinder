/* hp_eq.h — the headphone EQ (Sound ▸ Advanced) for libcinder_mono.so (src/cinder-mono.c),
 * host-tested by tools/hpeq_selftest.cpp.
 *
 * WHAT IT PLAYS. A cascade of biquads whose coefficients cinder-home designs from a parametric EQ
 * profile (player/cinder-ui/src/hp_eq.rs) and writes to /tmp/cinder_hpeq, one line per sample rate:
 *
 *     44100 b0 b1 b2 a1 a2 b0 b1 b2 a1 a2 ...
 *
 * a1/a2 negated: y = b0 x + b1 x1 + b2 x2 + a1 y1 + a2 y2. The level is already in the first filter,
 * so the cascade never rises above 0 dB; the result is still clamped to the format, never wrapped.
 * A stream at a rate the file has no line for passes unchanged.
 *
 * Doubles throughout: a low shelf at 192 kHz has poles within 1e-5 of the unit circle, where a
 * float's state hisses. Costs roughly 2 % of a core per ten filters at 44.1 kHz (5 multiplies and 4
 * adds per filter, per sample).
 * ponytail: one rate's line parsed per stream start; fine while a file is a few KB.
 *
 * Same rules as crossfeed.h: linear PCM only, the caller keeps DSD over PCM away.
 */
#ifndef CINDER_HP_EQ_H
#define CINDER_HP_EQ_H

#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>
#include <string.h>

#include "mono_sum.h"

#define HPEQ_MAX 16
#define HPEQ_CH 2

typedef struct {
    unsigned rate;       /* the rate `c` was read for; 0 = none yet */
    unsigned gen;        /* the file generation `c` came from */
    int n;               /* filters in use; 0 = this rate passes unchanged */
    double c[HPEQ_MAX][5];
    double z[HPEQ_CH][HPEQ_MAX][2];
} hpeq_state;

/* The line for `rate` in the control file `b` (`len` bytes) into `c`; the number of filters, 0 if
 * the file has no line for that rate or the line is malformed. */
static inline int hpeq_parse(const char* b, size_t len, unsigned rate, double c[HPEQ_MAX][5])
{
    const char* end = b + len;
    const char* p = b;
    while (p < end) {
        const char* eol = (const char*)memchr(p, '\n', (size_t)(end - p));
        if (!eol) eol = end;
        char line[2048];
        const size_t ln = (size_t)(eol - p);
        if (ln > 0 && ln < sizeof line) {
            memcpy(line, p, ln);
            line[ln] = 0;
            char* q;
            const unsigned long r = strtoul(line, &q, 10);
            if (r == rate && q != line) {
                double v[HPEQ_MAX * 5];
                int k = 0;
                for (;;) {
                    char* e;
                    const double x = strtod(q, &e);
                    if (e == q) break;
                    if (k == HPEQ_MAX * 5) return 0;
                    v[k++] = x;
                    q = e;
                }
                if (k == 0 || k % 5 != 0) return 0;
                /* Every section finite and stable (poles inside the unit circle; a1, a2 are stored
                 * negated), or no EQ at all: a filter that runs away does so at full scale, in
                 * someone's ears. cinder-home only writes stable ones; this is the last guard. */
                for (int i = 0; i < k; i += 5) {
                    for (int j = 0; j < 5; j++)
                        if (!(v[i + j] > -1e6 && v[i + j] < 1e6)) return 0;
                    const double c1 = -v[i + 3], c2 = -v[i + 4];
                    if (!(c2 > -1.0 && c2 < 1.0 && c1 < 1.0 + c2 && -c1 < 1.0 + c2)) return 0;
                }
                memcpy(c, v, sizeof(double) * (size_t)k);
                return k / 5;
            }
        }
        p = eol + 1;
    }
    return 0;
}

static inline int32_t hpeq_load(const uint8_t* f, int fmt)
{
    int32_t v;
    switch (fmt) {
    case CM_FMT_S16_LE: return (int16_t)(f[0] | (f[1] << 8));
    case CM_FMT_S32_LE: memcpy(&v, f, 4); return v;
    default:            return cm_s24(f);   /* S24_3LE, or S24_LE's low three bytes */
    }
}

static inline void hpeq_store(uint8_t* f, int fmt, int sb, double x)
{
    double lo, hi;
    switch (fmt) {
    case CM_FMT_S16_LE: lo = -32768.0; hi = 32767.0; break;
    case CM_FMT_S32_LE: lo = -2147483648.0; hi = 2147483647.0; break;
    default:            lo = -8388608.0; hi = 8388607.0; break;
    }
    /* Rounded, inside the range; truncation of x ± 0.5 never passes `hi` or `lo`. */
    const int32_t w = x >= hi ? (int32_t)hi : x <= lo ? (int32_t)lo : (int32_t)(x + (x >= 0 ? 0.5 : -0.5));
    switch (fmt) {
    case CM_FMT_S16_LE: f[0] = (uint8_t)(w & 0xFF); f[1] = (uint8_t)((w >> 8) & 0xFF); break;
    case CM_FMT_S32_LE: memcpy(f, &w, 4); break;
    default:
        f[0] = (uint8_t)(w & 0xFF);
        f[1] = (uint8_t)((w >> 8) & 0xFF);
        f[2] = (uint8_t)((w >> 16) & 0xFF);
        if (sb == 4) f[3] = (uint8_t)(w < 0 ? 0xFF : 0x00);
        break;
    }
}

/* EQ `frames` of interleaved PCM (1 or 2 channels) in place, with the control file `b` of
 * generation `gen`. Returns 1 if it changed them. A new rate or a new file restarts the filters. */
static inline int hpeq_apply(hpeq_state* s, const char* b, size_t len, unsigned gen, uint8_t* p,
                             size_t frames, int fmt, int ch, unsigned rate)
{
    const int sb = cm_sample_bytes(fmt);
    if (sb == 0 || ch < 1 || ch > HPEQ_CH || rate == 0 || frames == 0) return 0;
    if (rate != s->rate || gen != s->gen) {
        s->rate = rate;
        s->gen = gen;
        s->n = hpeq_parse(b, len, rate, s->c);
        memset(s->z, 0, sizeof s->z);
    }
    const int n = s->n;
    if (n == 0) return 0;
    for (size_t i = 0; i < frames; i++) {
        for (int c = 0; c < ch; c++) {
            uint8_t* f = p + (i * (size_t)ch + (size_t)c) * (size_t)sb;
            double x = (double)hpeq_load(f, fmt);
            for (int k = 0; k < n; k++) {   /* transposed direct form II */
                const double* q = s->c[k];
                double* z = s->z[c][k];
                const double y = q[0] * x + z[0];
                z[0] = q[1] * x + q[3] * y + z[1];
                z[1] = q[2] * x + q[4] * y;
                x = y;
            }
            hpeq_store(f, fmt, sb, x);
        }
    }
    return 1;
}

#endif /* CINDER_HP_EQ_H */
