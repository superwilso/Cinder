/* mono_sum.h — the pure half of system-wide mono (libcinder_mono.so, src/cinder-mono.c).
 *
 * Two places carry every sample this player makes, and both live in Sony's SoundServiceFw:
 *
 *   * the 3.5 mm jack: libaudiohal-adleralsa.so calls snd_pcm_writei (libasound is DT_NEEDED, no
 *     dlopen — read off the binary 2026-09-16), after Sony's whole DSP chain;
 *   * Bluetooth: libaudiohal-a2dpsnksingletrack.so writes to the abstract socket
 *     `pst::services::bttransmitterservice` (SoundServiceFw held the client end while LDAC played,
 *     2026-09-16). That stream is one type-1 handshake frame, then raw S16_LE interleaved PCM
 *     (analysis/E_usbdac_ldac, memory reference_bt_transmitter_socket).
 *
 * Summing there reaches every route, after every effect. This header is the part that can be
 * wrong in a way a listener cannot hear until it is too late, so it is host-tested
 * (tools/mono_selftest.cpp) and has no libc dependency beyond memcpy.
 *
 * RULES THAT KEEP IT SAFE
 *   * The sum is HALVED, never clipped: L+R of a correlated mix is up to +6 dB.
 *   * Anything not plainly 2-channel linear PCM passes through untouched — including DSD over PCM
 *     (DoP), where "summing" the marker bytes would turn a DSD stream into full-scale noise.
 *   * On the Bluetooth socket, a byte is only ever changed once the handshake has been seen and the
 *     stream is known to be PCM. A byte changed while the service is still parsing frames would be
 *     read as a length, and a bad length there reboots the device (measured 2026-08-11).
 *   * A frame split across two writes is sent whole-original or whole-summed, never half of each.
 */
#ifndef CINDER_MONO_SUM_H
#define CINDER_MONO_SUM_H

#include <stddef.h>
#include <stdint.h>
#include <string.h>

/* snd_pcm_format_t values (alsa/pcm.h), the only ones summed. */
enum {
    CM_FMT_S16_LE  = 2,
    CM_FMT_S24_LE  = 6,    /* 24 bits in the low three bytes of a four-byte slot */
    CM_FMT_S32_LE  = 10,
    CM_FMT_S24_3LE = 32,
};

/* Bytes per SAMPLE of a summable format, 0 for anything else. */
static inline int cm_sample_bytes(int fmt)
{
    switch (fmt) {
    case CM_FMT_S16_LE:  return 2;
    case CM_FMT_S24_LE:  return 4;
    case CM_FMT_S32_LE:  return 4;
    case CM_FMT_S24_3LE: return 3;
    }
    return 0;
}

/* The byte a DoP marker would sit in, within one sample. */
static inline int cm_dop_marker_at(int fmt)
{
    switch (fmt) {
    case CM_FMT_S24_3LE: return 2;
    case CM_FMT_S24_LE:  return 2;
    case CM_FMT_S32_LE:  return 3;   /* 24 bits left-justified in 32 */
    }
    return -1;                       /* S16 cannot carry DoP */
}

/* Does this stereo buffer look like DSD over PCM? DoP puts 0x05 or 0xFA in the top byte of every
 * 24-bit sample, the same on both channels, alternating frame to frame. Sixteen frames of that is
 * not music. A buffer too short to tell is treated as DoP when what little there is agrees — the
 * cost of a wrong "yes" is one buffer in stereo, the cost of a wrong "no" is noise. */
static inline int cm_looks_like_dop(const uint8_t* p, size_t frames, int fmt)
{
    const int at = cm_dop_marker_at(fmt);
    const int sb = cm_sample_bytes(fmt);
    if (at < 0 || sb == 0 || frames == 0) return 0;
    size_t n = frames < 16 ? frames : 16;
    uint8_t prev = 0;
    for (size_t i = 0; i < n; i++) {
        const uint8_t l = p[i * 2 * sb + at];
        const uint8_t r = p[i * 2 * sb + sb + at];
        if ((l != 0x05 && l != 0xFA) || r != l) return 0;
        if (i > 0 && l == prev) return 0;
        prev = l;
    }
    return 1;
}

static inline int32_t cm_s24(const uint8_t* s)
{
    int32_t v = (int32_t)s[0] | ((int32_t)s[1] << 8) | ((int32_t)s[2] << 16);
    return (v & 0x800000) ? v - 0x1000000 : v;
}

/* Sum one stereo frame in place. */
static inline void cm_sum_frame(uint8_t* f, int fmt)
{
    switch (fmt) {
    case CM_FMT_S16_LE: {
        const int l = (int16_t)(f[0] | (f[1] << 8));
        const int r = (int16_t)(f[2] | (f[3] << 8));
        const int m = (l + r) / 2;
        f[0] = f[2] = (uint8_t)(m & 0xFF);
        f[1] = f[3] = (uint8_t)((m >> 8) & 0xFF);
        break;
    }
    case CM_FMT_S24_3LE: {
        const int32_t m = (cm_s24(f) + cm_s24(f + 3)) / 2;
        f[0] = f[3] = (uint8_t)(m & 0xFF);
        f[1] = f[4] = (uint8_t)((m >> 8) & 0xFF);
        f[2] = f[5] = (uint8_t)((m >> 16) & 0xFF);
        break;
    }
    case CM_FMT_S24_LE: {
        const int32_t m = (cm_s24(f) + cm_s24(f + 4)) / 2;
        f[0] = f[4] = (uint8_t)(m & 0xFF);
        f[1] = f[5] = (uint8_t)((m >> 8) & 0xFF);
        f[2] = f[6] = (uint8_t)((m >> 16) & 0xFF);
        f[3] = f[7] = (uint8_t)(m < 0 ? 0xFF : 0x00);
        break;
    }
    case CM_FMT_S32_LE: {
        int32_t l, r;
        memcpy(&l, f, 4);
        memcpy(&r, f + 4, 4);
        const int32_t m = (int32_t)(((int64_t)l + (int64_t)r) / 2);
        memcpy(f, &m, 4);
        memcpy(f + 4, &m, 4);
        break;
    }
    }
}

/* Sum a stereo buffer in place. Returns 1 if it did, 0 if the buffer was left alone. */
static inline int cm_sum_buffer(uint8_t* p, size_t frames, int fmt, int channels)
{
    const int sb = cm_sample_bytes(fmt);
    if (channels != 2 || sb == 0 || frames == 0) return 0;
    if (cm_looks_like_dop(p, frames, fmt)) return 0;
    const size_t fb = (size_t)(2 * sb);
    for (size_t i = 0; i < frames; i++) cm_sum_frame(p + i * fb, fmt);
    return 1;
}

/* ── the Bluetooth transmitter socket ─────────────────────────────────────────────────────────── */

enum { CM_BT_FRAMES = 0, CM_BT_PCM = 1, CM_BT_GIVEUP = 2 };

typedef struct {
    uint8_t  mode;
    uint8_t  hdr[8];
    uint32_t hdr_got;
    uint32_t type, len, pay_got;
    uint8_t  pay[28];
    uint32_t channels;
    uint32_t rate;
    uint32_t align;        /* bytes of the current 4-byte PCM frame already written */
    uint8_t  partial[4];   /* its ORIGINAL bytes */
    uint8_t  partial_raw;  /* 1: its first bytes went out unsummed, so the rest must too */
} cm_bt_stream;

static inline uint32_t cm_le32(const uint8_t* p)
{
    return (uint32_t)p[0] | ((uint32_t)p[1] << 8) | ((uint32_t)p[2] << 16) | ((uint32_t)p[3] << 24);
}

/* Walk `n_do` bytes of one write through the stream state. `n_avail` (>= n_do) is how much of the
 * buffer exists, so a frame that the WRITE splits but the BUFFER holds whole is still summed.
 *
 * Called twice per write, with the same `mono`: once on a COPY of the state with `out` set, to
 * produce the bytes to send; then on the real state with `out` NULL and `n_do` = what the kernel
 * accepted, to advance it. The walk is deterministic, so both agree about the bytes they share. */
static inline void cm_bt_walk(cm_bt_stream* st, const uint8_t* in, size_t n_avail, size_t n_do,
                              uint8_t* out, int mono)
{
    size_t i = 0;
    while (i < n_do) {
        if (st->mode == CM_BT_FRAMES) {
            const uint8_t b = in[i];
            if (out) out[i] = b;
            i++;
            if (st->hdr_got < 8) {
                st->hdr[st->hdr_got++] = b;
                if (st->hdr_got == 8) {
                    st->type = cm_le32(st->hdr);
                    st->len = cm_le32(st->hdr + 4);
                    st->pay_got = 0;
                    const int known = (st->type == 0 && st->len == 0) || (st->type == 1 && st->len == 28)
                                   || (st->type == 2 && st->len == 12);
                    if (!known) st->mode = CM_BT_GIVEUP;      /* the service closes on these */
                    else if (st->len == 0) st->hdr_got = 0;   /* a bare type-0 frame */
                }
            } else {
                if (st->type == 1 && st->pay_got < 28) st->pay[st->pay_got] = b;
                st->pay_got++;
                if (st->pay_got == st->len) {
                    if (st->type == 1) {
                        st->channels = cm_le32(st->pay + 4);
                        st->rate = cm_le32(st->pay + 24);
                        st->mode = CM_BT_PCM;
                        st->align = 0;
                        st->partial_raw = 0;
                    }
                    st->hdr_got = 0;
                }
            }
            continue;
        }
        if (st->mode != CM_BT_PCM || st->channels != 2) {   /* not ours to touch */
            if (out) memcpy(out + i, in + i, n_do - i);
            i = n_do;
            continue;
        }
        if (st->align == 0 && n_do - i >= 4) {               /* whole frames: the ordinary case */
            size_t frames = (n_do - i) / 4;
            if (out) {
                memcpy(out + i, in + i, frames * 4);
                if (mono) for (size_t k = 0; k < frames; k++) cm_sum_frame(out + i + k * 4, CM_FMT_S16_LE);
            }
            i += frames * 4;
            continue;
        }
        /* A frame that straddles this write's start or end. */
        if (st->align == 0) {
            /* Starts here, ends past n_do. If the buffer holds the rest it is summed whole (the
             * kernel just took part of it); if not, it goes out as it came. */
            const int whole = (n_avail - i) >= 4;
            uint8_t f[4];
            if (whole) {
                memcpy(f, in + i, 4);
                memcpy(st->partial, in + i, 4);
                if (mono) cm_sum_frame(f, CM_FMT_S16_LE);
            }
            const size_t have = n_do - i;                    /* 1..3 */
            for (size_t k = 0; k < have; k++) {
                if (!whole) st->partial[k] = in[i + k];
                if (out) out[i + k] = whole ? f[k] : in[i + k];
            }
            st->partial_raw = (uint8_t)!(whole && mono);   /* what the head actually went out as */
            st->align = (uint32_t)have;
            i = n_do;
            continue;
        }
        /* Continues a frame begun by an earlier write. */
        {
            const size_t need = 4 - st->align;
            const size_t take = (n_do - i) < need ? (n_do - i) : need;
            for (size_t k = 0; k < take; k++) st->partial[st->align + k] = in[i + k];
            if (out) {
                if (!st->partial_raw && mono) {
                    /* The earlier write saw this frame whole and sent its head summed; `partial`
                     * holds all four original bytes, so the tail is the same sum. */
                    uint8_t f[4];
                    memcpy(f, st->partial, 4);
                    cm_sum_frame(f, CM_FMT_S16_LE);
                    memcpy(out + i, f + st->align, take);
                } else {
                    memcpy(out + i, in + i, take);
                }
            }
            st->align = (uint32_t)((st->align + take) % 4);
            if (st->align == 0) st->partial_raw = 0;
            i += take;
        }
    }
}

#endif /* CINDER_MONO_SUM_H */
