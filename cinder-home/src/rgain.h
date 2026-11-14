/* rgain.h — ReplayGain for libcinder_mono.so (src/cinder-mono.c), host-tested by
 * tools/rgain_selftest.cpp.
 *
 * The player (cinder-ffi, replaygain_flag) reads the song's ReplayGain tags and writes the gain to
 * a tmpfs file as a Q24 multiplier: 16777216 is unchanged, 8388608 is -6 dB. This scales the PCM
 * by it. The player has already held the gain under the song's peak, so a boost does not clip;
 * the clamp here is for the songs whose tags are wrong.
 *
 * A change of gain (a new song, the setting switched) is RAMPED across one buffer, never stepped:
 * a step in the middle of a waveform is a click.
 *
 * Same safety rules as mono_sum.h: linear PCM only, never DSD over PCM (the caller checks).
 */
#ifndef CINDER_RGAIN_H
#define CINDER_RGAIN_H

#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include "mono_sum.h"

#define RG_UNITY (1 << 24)

typedef struct {
    int32_t q;   /* the gain the last buffer ended at, Q24; 0 before the first buffer (= unity) */
} rg_state;

/* The control file's text: one decimal Q24 number. Anything else, or out of 1/64..4x, is unity. */
static inline int32_t rg_parse(const char* b, size_t n)
{
    int64_t v = 0;
    size_t i = 0;
    for (; i < n && b[i] >= '0' && b[i] <= '9'; i++) {
        v = v * 10 + (b[i] - '0');
        if (v > 4LL * RG_UNITY) return RG_UNITY;
    }
    if (i == 0 || v < RG_UNITY / 64) return RG_UNITY;
    for (; i < n; i++)
        if (b[i] != '\n' && b[i] != '\r' && b[i] != ' ') return RG_UNITY;
    return (int32_t)v;
}

static inline int rg_busy(const rg_state* s, int32_t target)
{
    const int32_t from = s->q ? s->q : RG_UNITY;
    return from != RG_UNITY || target != RG_UNITY;
}

/* Scale `frames` of `ch`-channel PCM in place, by a gain moving from where the last buffer ended
 * to `target` across this one. Returns 1 if it changed anything, 0 for unity or a format it does
 * not know. */
static inline int rg_apply(rg_state* s, uint8_t* p, size_t frames, int ch, int fmt, int32_t target)
{
    const int sb = cm_sample_bytes(fmt);
    const int32_t from = s->q ? s->q : RG_UNITY;
    if (sb == 0 || ch <= 0 || frames == 0) return 0;
    s->q = target;
    if (from == RG_UNITY && target == RG_UNITY) return 0;
    int64_t lo, hi;
    switch (fmt) {
    case CM_FMT_S16_LE: lo = -32768; hi = 32767; break;
    case CM_FMT_S32_LE: lo = INT32_MIN; hi = INT32_MAX; break;
    default:            lo = -8388608; hi = 8388607; break;   /* the two 24-bit layouts */
    }
    const int64_t step = (int64_t)target - from;
    for (size_t i = 0; i < frames; i++) {
        const int64_t g = from + step * (int64_t)(i + 1) / (int64_t)frames;
        for (int c = 0; c < ch; c++) {
            uint8_t* f = p + (i * (size_t)ch + (size_t)c) * (size_t)sb;
            int32_t v;
            switch (fmt) {
            case CM_FMT_S16_LE: v = (int16_t)(f[0] | (f[1] << 8)); break;
            case CM_FMT_S32_LE: memcpy(&v, f, 4); break;
            default:            v = cm_s24(f); break;   /* S24_3LE, or S24_LE's low three bytes */
            }
            int64_t o = ((int64_t)v * g + (1 << 23)) >> 24;
            o = o < lo ? lo : o > hi ? hi : o;
            const int32_t w = (int32_t)o;
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
    }
    return 1;
}

#endif /* CINDER_RGAIN_H */
