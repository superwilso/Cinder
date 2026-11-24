// bt_enc_test.c — tools/bt_enc.h under qemu-arm against the player's own libraries
// (tools/test_bt_enc.sh): a second of a 1 kHz tone through each encoder.
//   SBC   the firmware's libsbc_enc.so; its own decoder reads the packets back, and the tone must
//         come out clean (proves the sbc_t layout bt_enc.h assumes, and the packing);
//   LDAC  libldac: every packet fits, carries 1-15 frames, and each frame starts with LDAC's sync.
#include <math.h>
#include <stdio.h>
#include <stdlib.h>

#include "bt_enc.h"

static int fails;
#define CHECK(c, ...) do { printf((c) ? "  ok   " : "  FAIL "); printf(__VA_ARGS__); printf("\n"); if (!(c)) fails++; } while (0)

static int16_t tone[48000 * 2];
static void make_tone(int rate) {
    for (int i = 0; i < rate; i++) tone[2 * i] = tone[2 * i + 1] = (int16_t)(16000 * sin(2 * M_PI * 1000.0 * i / rate));
}

// SNR in dB of x[0..n) against the best-fitting 1 kHz sine (any phase): delay does not matter.
static double tone_snr(const int16_t* x, int n, int stride, int rate) {
    double ss = 0, sc = 0, cc = 0, xs = 0, xc = 0;
    for (int i = 0; i < n; i++) {
        const double s = sin(2 * M_PI * 1000.0 * i / rate), c = cos(2 * M_PI * 1000.0 * i / rate);
        ss += s * s; cc += c * c; sc += s * c; xs += x[i * stride] * s; xc += x[i * stride] * c;
    }
    const double det = ss * cc - sc * sc, a = (xs * cc - xc * sc) / det, b = (xc * ss - xs * sc) / det;
    double sig = 0, err = 0;
    for (int i = 0; i < n; i++) {
        const double f = a * sin(2 * M_PI * 1000.0 * i / rate) + b * cos(2 * M_PI * 1000.0 * i / rate), d = x[i * stride] - f;
        sig += f * f; err += d * d;
    }
    return 10 * log10(sig / (err + 1e-9));
}

static void sbc_case(const unsigned char* cap, int rate, int mtu, const char* what) {
    struct bt_ep ep = {1, BT_SBC, 6, {0}};
    memcpy(ep.cap, cap, 6);
    struct bt_enc e;
    const char* why = "";
    if (bt_enc_open(&e, &ep, rate, mtu, 0, &why)) { CHECK(0, "%s: open (%s)", what, why); return; }
    void* lib = dlopen("libsbc_enc.so", RTLD_NOW);
    ssize_t (*dec)(bt_sbc_t*, const void*, size_t, void*, size_t, size_t*) = NULL;
    *(void**)&dec = dlsym(lib, "sbc_decode");
    bt_sbc_t d;
    e.sbc_init(&d, 0);
    make_tone(rate);
    static int16_t out[48000 * 2 + 4096];
    size_t got = 0;
    int pkts = 0, bad = 0, frames = 0;
    uint32_t last_ts = 0;
    for (int i = 0; i + e.samples <= rate; i += e.samples) {
        const int n = bt_enc_feed(&e, tone + 2 * i);
        if (n < 0) { bad++; continue; }
        if (!n) continue;
        if (n > mtu - 12 || (pkts && e.pkt_ts <= last_ts) || (e.pkt[0] & 0x0F) == 0) bad++;
        last_ts = e.pkt_ts;
        pkts++;
        frames += e.pkt[0] & 0x0F;
        const unsigned char* p = e.pkt + 1;
        size_t left = (size_t)n - 1;
        while (left) {
            size_t w = 0;
            const ssize_t used = dec(&d, p, left, (char*)out + got, sizeof out - got, &w);
            if (used <= 0) { bad++; break; }
            p += used; left -= (size_t)used; got += w;
        }
    }
    const int chans = e.sbc_mono ? 1 : 2, n = (int)(got / 2 / (size_t)chans);
    const double snr = n > rate / 2 ? tone_snr(out + chans * 2048, n - 4096, chans, rate) : 0;
    CHECK(!bad && pkts > 0 && snr > 40, "%s: %d packets, %d frames, %d samples back, tone SNR %.1f dB, bitpool %d",
          what, pkts, frames, n, snr, e.sbc.bitpool);
    bt_enc_close(&e);
}

static void ldac_case(int rate, int mtu) {
    const struct bt_ep ep = {1, BT_LDAC, 10, {0x00, 0xFF, 0x2D, 0x01, 0x00, 0x00, 0xAA, 0x00, 0x30, 0x01}};
    struct bt_enc e;
    const char* why = "";
    if (bt_enc_open(&e, &ep, rate, mtu, 0, &why)) { CHECK(0, "LDAC %d: open (%s)", rate, why); return; }
    make_tone(rate);
    int pkts = 0, frames = 0, bad = 0;
    long bytes = 0;
    for (int i = 0; i + e.samples <= rate; i += e.samples) {
        const int n = bt_enc_feed(&e, tone + 2 * i);
        if (n < 0) { bad++; continue; }
        if (!n) continue;
        const int f = e.pkt[0] & 0x0F;
        if (n > mtu - 12 || f < 1) bad++;
        // Frames are equal-sized within a packet at a fixed quality: each one starts with 0xAA.
        for (int k = 0; f && k < f; k++) if (e.pkt[1 + k * ((n - 1) / f)] != 0xAA) bad++;
        pkts++; frames += f; bytes += n;
    }
    CHECK(!bad && pkts > 0, "LDAC %d Hz, MTU %d: %d packets, %d frames, %ld kbit/s on the air, encoder says %d kbit/s",
          rate, mtu, pkts, frames, bytes * 8 / 1000, bt_enc_bitrate(&e) / 1000);
    bt_enc_close(&e);
}

int main(void) {
    const unsigned char all[6] = {0x00, 0x00, 0xFF, 0xFF, 0x02, 0x35};
    sbc_case(all, 44100, 895, "SBC 44.1 kHz joint stereo");
    sbc_case(all, 48000, 672, "SBC 48 kHz joint stereo, small MTU");
    const unsigned char mono[6] = {0x00, 0x00, 0x28, 0x15, 0x02, 0x20};
    sbc_case(mono, 44100, 672, "SBC mono-only headphones");
    ldac_case(44100, 1005);
    ldac_case(48000, 895);
    ldac_case(44100, 679 + 13);
    return fails ? 1 : 0;
}
