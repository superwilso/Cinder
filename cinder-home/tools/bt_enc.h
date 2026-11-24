// bt_enc.h — btsource's encoders behind one call (Phase 4b, docs/PLAN_hardware_first.md):
//   AAC   cinder-aac/src/aac_enc.c (fdk-aac), one access unit per packet, as before;
//   LDAC  AOSP's libldac (Apache 2.0, cinder-aac/build_ldac.sh), linked in;
//   SBC   the firmware's own libsbc_enc.so (BlueZ's libsbc interface), opened at run time.
// Feed it `samples` stereo S16 frames at a time; when a packet is ready it returns its payload
// length (the codec's payload header included) in e->pkt, and e->pkt_ts is the RTP timestamp of
// its first sample. Header-only; everything runs on btsource's streaming thread.
#ifndef CINDER_BT_ENC_H
#define CINDER_BT_ENC_H
#include <dlfcn.h>
#include <stdint.h>
#include <string.h>
#include <sys/types.h>

#include "../../cinder-aac/src/aac_enc.h"
#include "../../cinder-aac/third_party/libldac/inc/ldacBT.h"
#include "bt_codec.h"

// BlueZ libsbc's sbc_t (sbc.h), which the firmware's build keeps: checked under qemu against the
// real library by tools/test_bt_enc.sh (encode, then its own decoder, then the SNR).
typedef struct { unsigned long flags; uint8_t frequency, blocks, subbands, mode, allocation, bitpool, endian; void *priv, *priv_alloc_base; } bt_sbc_t;
enum { SBC_F_44100 = 0x02, SBC_F_48000 = 0x03, SBC_BLK16 = 0x03, SBC_SB8 = 0x01, SBC_AM_LOUD = 0x00,
       SBC_M_MONO = 0x00, SBC_M_DUAL = 0x01, SBC_M_STEREO = 0x02, SBC_M_JOINT = 0x03 };

struct bt_enc {
    int codec, rate, samples, room;   // room: payload bytes a packet may carry (media MTU less RTP)
    struct aac_enc* aac;
    HANDLE_LDAC_BT ldac;
    bt_sbc_t sbc;
    int sbc_mono;
    int (*sbc_init)(bt_sbc_t*, unsigned long);
    ssize_t (*sbc_encode)(bt_sbc_t*, const void*, size_t, void*, size_t, ssize_t*);
    void (*sbc_finish)(bt_sbc_t*);
    uint32_t ts, pkt_ts;               // samples fed; timestamp of the packet being filled
    int fill, frames;                  // bytes and frames in the packet being filled (SBC: in acc)
    uint32_t acc_ts;
    unsigned char pkt[1024], acc[1024];
};

static inline int bt_enc_open(struct bt_enc* e, const struct bt_ep* ep, int rate, int media_mtu, int aac_max, const char** why) {
    memset(e, 0, sizeof *e);
    e->codec = ep->codec;
    e->rate = rate;
    e->room = media_mtu - 12 < (int)sizeof e->pkt ? media_mtu - 12 : (int)sizeof e->pkt;   // LDAC's 3-DH5 cap fits
    switch (ep->codec) {
    case BT_AAC:
        e->samples = AAC_FRAME_SAMPLES;
        e->aac = aac_enc_open(rate, aac_bitrate_for(e->room, rate, aac_max), AAC_LATM, why);
        return e->aac ? 0 : -1;
    case BT_LDAC: {
        e->samples = LDACBT_ENC_LSU;
        e->ldac = ldacBT_get_handle();
        if (!e->ldac) { *why = "no memory"; return -1; }
        if (ldacBT_init_handle_encode(e->ldac, bt_ldac_mtu(media_mtu), LDACBT_EQMID_HQ, bt_ldac_mode(ep), LDACBT_SMPL_FMT_S16, rate)) {
            *why = "libldac refused the settings";
            ldacBT_free_handle(e->ldac);
            e->ldac = NULL;
            return -1;
        }
        return 0;
    }
    case BT_SBC: {
        void* lib = dlopen("libsbc_enc.so", RTLD_NOW);
        if (lib) {
            *(void**)&e->sbc_init = dlsym(lib, "sbc_init");
            *(void**)&e->sbc_encode = dlsym(lib, "sbc_encode");
            *(void**)&e->sbc_finish = dlsym(lib, "sbc_finish");
        }
        if (!e->sbc_init || !e->sbc_encode || !e->sbc_finish) { *why = "the firmware's libsbc_enc.so is missing"; return -1; }
        e->sbc_init(&e->sbc, 0);
        const int m = bt_sbc_mode(ep);
        e->sbc.frequency = rate == 44100 ? SBC_F_44100 : SBC_F_48000;
        e->sbc.blocks = SBC_BLK16;
        e->sbc.subbands = SBC_SB8;
        e->sbc.allocation = SBC_AM_LOUD;
        e->sbc.mode = m == 0x01 ? SBC_M_JOINT : m == 0x02 ? SBC_M_STEREO : m == 0x04 ? SBC_M_DUAL : SBC_M_MONO;
        e->sbc.bitpool = (uint8_t)bt_sbc_bitpool(ep, rate);
        e->sbc_mono = m == 0x08;
        e->samples = 16 * 8;   // blocks × subbands: one SBC frame
        return 0;
    }
    }
    *why = "no codec";
    return -1;
}

// Encode `pcm` (e->samples stereo S16 frames). Returns the payload length ready in e->pkt (from
// e->pkt_ts), 0 when nothing is ready yet, -2 when a frame was too big for the packet (dropped,
// AAC), -1 on an encoder error.
static inline int bt_enc_feed(struct bt_enc* e, const int16_t* pcm) {
    const uint32_t at = e->ts;
    e->ts += (uint32_t)e->samples;
    switch (e->codec) {
    case BT_AAC: {
        e->pkt_ts = at;
        const int n = aac_enc_fit(e->aac, pcm, e->pkt, e->room, e->room);
        return n == -2 ? -2 : n < 0 ? -1 : n;
    }
    case BT_LDAC: {
        int used = 0, wrote = 0, frames = 0;
        if (!e->fill) { e->pkt_ts = at; e->fill = 1; }
        if (ldacBT_encode(e->ldac, (void*)pcm, &used, e->pkt + 1, &wrote, &frames)) return -1;
        if (wrote <= 0) return 0;
        e->fill = 0;
        bt_payload_header(BT_LDAC, frames, e->pkt);
        return wrote + 1;
    }
    case BT_SBC: {
        static int16_t mono[16 * 8];
        const void* in = pcm;
        size_t in_len = (size_t)e->samples * 4;
        if (e->sbc_mono) {
            for (int i = 0; i < e->samples; i++) mono[i] = (int16_t)((pcm[2 * i] + pcm[2 * i + 1]) / 2);
            in = mono;
            in_len /= 2;
        }
        unsigned char f[512];
        ssize_t wrote = 0;
        if (e->sbc_encode(&e->sbc, in, in_len, f, sizeof f, &wrote) <= 0 || wrote <= 0) return -1;
        int out = 0;
        if (e->frames && (1 + e->fill + wrote > e->room || e->frames == 15)) {   // it does not fit: send what is there
            bt_payload_header(BT_SBC, e->frames, e->pkt);
            memcpy(e->pkt + 1, e->acc, (size_t)e->fill);
            e->pkt_ts = e->acc_ts;
            out = 1 + e->fill;
            e->fill = e->frames = 0;
        }
        if (!e->frames) e->acc_ts = at;
        memcpy(e->acc + e->fill, f, (size_t)wrote);
        e->fill += (int)wrote;
        e->frames++;
        return out;
    }
    }
    return -1;
}

static inline int bt_enc_bitrate(const struct bt_enc* e) {
    if (e->codec == BT_AAC) return aac_enc_bitrate(e->aac);
    if (e->codec == BT_LDAC) return ldacBT_get_bitrate(e->ldac) * 1000;
    return 0;
}

static inline void bt_enc_close(struct bt_enc* e) {
    if (e->aac) aac_enc_close(e->aac);
    if (e->ldac) { ldacBT_close_handle(e->ldac); ldacBT_free_handle(e->ldac); }
    if (e->sbc_finish && e->codec == BT_SBC) e->sbc_finish(&e->sbc);
    memset(e, 0, sizeof *e);
}
#endif
