// bt_codec.h — which codec btsource streams in, and the A2DP bytes for it (Phase 4b,
// docs/PLAN_hardware_first.md). Pure: no I/O, so tools/bt_codec_selftest.cpp drives it on the host.
//
// The headphones list their endpoints (AVDTP DISCOVER), each with a media codec capability
// (GET_CAPABILITIES, category 7). Preference: LDAC, then AAC, then SBC, among free sink endpoints
// that take the stream's rate. LDAC also needs a media channel MTU of at least 679 bytes (its
// encoder refuses less), so the choice is made again once the media channel is open.
#ifndef CINDER_BT_CODEC_H
#define CINDER_BT_CODEC_H
#include <string.h>

enum bt_codec { BT_NONE = -1, BT_SBC = 0, BT_AAC = 1, BT_LDAC = 2 };

// One endpoint's media codec capability: the element's payload (media type, codec type, then the
// codec's own bytes), at most 16 bytes kept.
struct bt_ep { int seid, codec, len; unsigned char cap[16]; };

enum { BT_LDAC_MTU_MIN = 679, BT_LDAC_MTU_MAX = 990 + 3 };   // libldac: LDACBT_MTU_REQUIRED, _3DH5

static inline const char* bt_codec_name(int c) { return c == BT_LDAC ? "LDAC" : c == BT_AAC ? "AAC" : c == BT_SBC ? "SBC" : "none"; }

// The codec of a capability payload `p` (n bytes), or BT_NONE for one btsource cannot send.
static inline int bt_codec_of(const unsigned char* p, int n) {
    if (n < 2 || p[0] != 0x00) return BT_NONE;   // audio only
    if (p[1] == 0x00 && n >= 6) return BT_SBC;
    if (p[1] == 0x02 && n >= 8) return BT_AAC;
    // Vendor codec: vendor 0x0000012D (Sony), codec 0x00AA (LDAC), both little-endian.
    if (p[1] == 0xFF && n >= 10 && p[2] == 0x2D && p[3] == 0x01 && !p[4] && !p[5] && p[6] == 0xAA && !p[7])
        return BT_LDAC;
    return BT_NONE;
}

// Whether the endpoint takes `rate` (44100 or 48000) with two channels or a mode btsource can feed.
static inline int bt_ep_takes(const struct bt_ep* e, int rate) {
    const unsigned char* c = e->cap;
    switch (e->codec) {
    case BT_SBC: return (c[2] & (rate == 44100 ? 0x20 : 0x10)) && (c[2] & 0x0F) && (c[3] & 0x04) && (c[3] & 0x10);   // 8 subbands, 16 blocks
    case BT_AAC: return rate == 44100 ? (c[3] & 0x01) : (c[4] & 0x80);
    case BT_LDAC: return (c[8] & (rate == 44100 ? 0x20 : 0x10)) && (c[9] & 0x07);
    }
    return 0;
}

// Index into eps[0..n) of the endpoint to use for `rate`, or -1. `ldac_ok` is 0 when LDAC must be
// passed over (the media channel's MTU is too small, or LDAC was turned off).
static inline int bt_pick(const struct bt_ep* eps, int n, int rate, int ldac_ok) {
    int best = -1;
    for (int i = 0; i < n; i++) {
        if (eps[i].codec == BT_NONE || (eps[i].codec == BT_LDAC && !ldac_ok) || !bt_ep_takes(&eps[i], rate)) continue;
        if (best < 0 || eps[i].codec > eps[best].codec) best = i;
    }
    return best;
}

// The SBC bitpool for a rate: High Quality in the A2DP spec's table (Joint Stereo, 16 blocks, 8
// subbands): 53 at 44.1 kHz (328 kbit/s), 51 at 48 kHz (345 kbit/s), clamped to the headphones' range.
static inline int bt_sbc_bitpool(const struct bt_ep* e, int rate) {
    int bp = rate == 44100 ? 53 : 51;
    if (bp > e->cap[5]) bp = e->cap[5];
    if (bp < e->cap[4]) bp = e->cap[4];
    return bp;
}

// The SBC channel mode to use, as the capability bit: Joint Stereo, else Stereo, Dual, Mono.
static inline int bt_sbc_mode(const struct bt_ep* e) {
    const int m = e->cap[2] & 0x0F;
    return m & 0x01 ? 0x01 : m & 0x02 ? 0x02 : m & 0x04 ? 0x04 : 0x08;
}

// The LDAC channel mode: Stereo, else Dual Channel, else Mono (libldac's LDACBT_CHANNEL_MODE_*).
static inline int bt_ldac_mode(const struct bt_ep* e) {
    const int m = e->cap[9];
    return m & 0x01 ? 0x01 : m & 0x02 ? 0x02 : 0x04;
}

// The media codec element payload for SET_CONFIGURATION into out (16 bytes room); its length.
// aac_max: the AAC bit rate to name (the headphones' cap, already bounded by the caller).
static inline int bt_config(const struct bt_ep* e, int rate, unsigned aac_max, unsigned char* out) {
    const unsigned char* c = e->cap;
    switch (e->codec) {
    case BT_SBC: {
        const unsigned char o[6] = {0x00, 0x00, (unsigned char)((rate == 44100 ? 0x20 : 0x10) | bt_sbc_mode(e)),
                                    (unsigned char)(0x10 | 0x04 | 0x01),   // 16 blocks, 8 subbands, loudness
                                    (unsigned char)(c[4] > 2 ? c[4] : 2), (unsigned char)bt_sbc_bitpool(e, rate)};
        memcpy(out, o, 6);
        return 6;
    }
    case BT_AAC: {
        const unsigned char ot = c[2] & 0x80 ? 0x80 : 0x40;   // MPEG-2 AAC LC if offered, else MPEG-4
        const unsigned char o[8] = {0x00, 0x02, ot, (unsigned char)(rate == 44100 ? 0x01 : 0x00),
                                    (unsigned char)((rate == 48000 ? 0x80 : 0x00) | 0x04),   // two channels
                                    (unsigned char)(aac_max >> 16), (unsigned char)(aac_max >> 8), (unsigned char)aac_max};
        memcpy(out, o, 8);
        return 8;
    }
    case BT_LDAC: {
        const unsigned char o[10] = {0x00, 0xFF, 0x2D, 0x01, 0x00, 0x00, 0xAA, 0x00,
                                     (unsigned char)(rate == 44100 ? 0x20 : 0x10), (unsigned char)bt_ldac_mode(e)};
        memcpy(out, o, 10);
        return 10;
    }
    }
    return 0;
}

// The one-byte media payload header SBC and LDAC put after RTP: the number of frames (AAC has none).
static inline int bt_payload_header(int codec, int frames, unsigned char* out) {
    if (codec == BT_AAC) return 0;
    out[0] = (unsigned char)(frames & 0x0F);
    return 1;
}

// RTP payload type: AAC and SBC streams use 96 (dynamic); LDAC's sender in Android uses 96 too.
enum { BT_RTP_PT = 0x60 };

// The LDAC transmit size for libldac: the media MTU less RTP and the payload header, capped at 3-DH5.
static inline int bt_ldac_mtu(int media_mtu) {
    int m = media_mtu - 12 - 1;
    return m > BT_LDAC_MTU_MAX ? BT_LDAC_MTU_MAX : m;
}
#endif
