// bt_codec_selftest.cpp — tools/bt_codec.h: btsource picks LDAC, then AAC, then SBC from the
// headphones' endpoints, and configures each with the bytes the A2DP specs (and Android's LDAC
// sender) use. Capabilities below are typical sink payloads (media type, codec type, codec bytes).
#include <cstdio>
#include <cstring>

#include "bt_codec.h"

static int fails;
#define CHECK(c, what) do { if (c) std::printf("  ok   %s\n", what); else { std::printf("  FAIL %s\n", what); ++fails; } } while (0)

static bt_ep ep(int seid, const unsigned char* p, int n) {
    bt_ep e{};
    e.seid = seid;
    e.len = n;
    std::memcpy(e.cap, p, (size_t)n);
    e.codec = bt_codec_of(p, n);
    return e;
}

int main() {
    // SBC: all rates and modes, 4-16 blocks, both subbands and allocations, bitpool 2-53.
    const unsigned char sbc[6] = {0x00, 0x00, 0xFF, 0xFF, 0x02, 0x35};
    // AAC: MPEG-2 LC, 44.1 kHz, 48 kHz and 2 channels, VBR, 320 kbit/s.
    const unsigned char aac[8] = {0x00, 0x02, 0x80, 0x01, 0x8C, 0x84, 0xE2, 0x00};
    // LDAC: 44.1/48/88.2/96 kHz, stereo, dual and mono.
    const unsigned char ldac[10] = {0x00, 0xFF, 0x2D, 0x01, 0x00, 0x00, 0xAA, 0x00, 0x3C, 0x07};
    // aptX (vendor 0x4F, codec 0x01): not one btsource sends.
    const unsigned char aptx[9] = {0x00, 0xFF, 0x4F, 0x00, 0x00, 0x00, 0x01, 0x00, 0x22};

    CHECK(bt_codec_of(sbc, 6) == BT_SBC && bt_codec_of(aac, 8) == BT_AAC && bt_codec_of(ldac, 10) == BT_LDAC,
          "SBC, AAC and LDAC capabilities are recognised");
    CHECK(bt_codec_of(aptx, 9) == BT_NONE, "aptX is passed over");
    CHECK(bt_codec_of(ldac, 9) == BT_NONE, "a short LDAC element is passed over");
    const unsigned char video[6] = {0x10, 0x00, 0xFF, 0xFF, 0x02, 0x35};
    CHECK(bt_codec_of(video, 6) == BT_NONE, "a non-audio endpoint is passed over");

    const bt_ep all[4] = {ep(1, sbc, 6), ep(2, aac, 8), ep(3, aptx, 9), ep(4, ldac, 10)};
    CHECK(bt_pick(all, 4, 44100, 1) == 3, "LDAC is chosen when offered");
    CHECK(bt_pick(all, 4, 44100, 0) == 1, "AAC when LDAC must be passed over (small MTU)");
    CHECK(bt_pick(all, 1, 48000, 1) == 0, "SBC when it is all they have");
    CHECK(bt_pick(all + 2, 1, 44100, 1) == -1, "nothing for aptX-only headphones");

    const unsigned char ldac44[10] = {0x00, 0xFF, 0x2D, 0x01, 0x00, 0x00, 0xAA, 0x00, 0x20, 0x01};
    const bt_ep only44[2] = {ep(1, sbc, 6), ep(2, ldac44, 10)};
    CHECK(bt_pick(only44, 2, 48000, 1) == 0, "LDAC that lacks 48 kHz is passed over for a 48 kHz stream");
    const unsigned char aac44[8] = {0x00, 0x02, 0x80, 0x01, 0x04, 0x84, 0xE2, 0x00};
    const bt_ep a44 = ep(1, aac44, 8);
    CHECK(bt_ep_takes(&a44, 44100) && !bt_ep_takes(&a44, 48000), "AAC's 48 kHz bit is honoured");

    unsigned char out[16];
    int n = bt_config(&all[3], 44100, 0, out);
    const unsigned char want_ldac[10] = {0x00, 0xFF, 0x2D, 0x01, 0x00, 0x00, 0xAA, 0x00, 0x20, 0x01};
    CHECK(n == 10 && !std::memcmp(out, want_ldac, 10), "LDAC configuration: 44.1 kHz, stereo");
    n = bt_config(&all[3], 48000, 0, out);
    CHECK(n == 10 && out[8] == 0x10, "LDAC configuration: 48 kHz");
    const unsigned char dual[10] = {0x00, 0xFF, 0x2D, 0x01, 0x00, 0x00, 0xAA, 0x00, 0x20, 0x02};
    const bt_ep d = ep(1, dual, 10);
    CHECK(bt_ldac_mode(&d) == 0x02, "LDAC dual channel when stereo is not offered");

    n = bt_config(&all[0], 44100, 0, out);
    const unsigned char want_sbc[6] = {0x00, 0x00, 0x21, 0x15, 0x02, 0x35};
    CHECK(n == 6 && !std::memcmp(out, want_sbc, 6), "SBC configuration: 44.1 kHz joint stereo, 16/8/loudness, bitpool 2-53");
    n = bt_config(&all[0], 48000, 0, out);
    CHECK(n == 6 && out[2] == 0x11 && out[5] == 51, "SBC at 48 kHz: bitpool 51");
    const unsigned char sbc35[6] = {0x00, 0x00, 0x22, 0x15, 0x04, 0x23};
    const bt_ep s35 = ep(1, sbc35, 6);
    n = bt_config(&s35, 44100, 0, out);
    CHECK(n == 6 && out[2] == 0x22 && out[4] == 4 && out[5] == 35, "SBC: stereo when joint is not offered, their bitpool range");
    const unsigned char sbc12[6] = {0x00, 0x00, 0x21, 0x25, 0x02, 0x35};   // 12 blocks only
    const bt_ep s12 = ep(1, sbc12, 6);
    CHECK(!bt_ep_takes(&s12, 44100), "SBC without 16 blocks is passed over");

    n = bt_config(&all[1], 44100, 320000, out);
    const unsigned char want_aac[8] = {0x00, 0x02, 0x80, 0x01, 0x04, 0x04, 0xE2, 0x00};
    CHECK(n == 8 && !std::memcmp(out, want_aac, 8), "AAC configuration: as btsource sent it before");

    CHECK(bt_payload_header(BT_AAC, 3, out) == 0, "AAC has no payload header");
    CHECK(bt_payload_header(BT_LDAC, 2, out) == 1 && out[0] == 2, "LDAC payload header: frame count");
    CHECK(bt_payload_header(BT_SBC, 17, out) == 1 && out[0] == 1, "SBC frame count stays in four bits");
    CHECK(bt_ldac_mtu(1200) == BT_LDAC_MTU_MAX && bt_ldac_mtu(1005) == 992 && bt_ldac_mtu(895) == 882, "LDAC transmit size: MTU less 13, capped at 3-DH5");

    if (fails) std::printf("%d failed\n", fails);
    return fails ? 1 : 0;
}
