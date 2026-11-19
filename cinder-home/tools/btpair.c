// btpair.c — Phase 4 step three (docs/PLAN_hardware_first.md): pair with a device through the
// kernel's Bluetooth management interface, then ask it over A2DP signalling (AVDTP, L2CAP PSM 25)
// which audio endpoints and codecs it has. No BlueZ daemon: the kernel keeps the link key for this
// boot only. Run while `btbridge -w` holds hci0 (with CINDER_BT_ALT=1 for a test pairing).
//
//   btpair AA:BB:CC:DD:EE:FF [SECONDS]   SECONDS > 0: then play a quiet 440 Hz tone over AAC
//
// Built against the player's glibc (cinder-aac/build.sh's recipe) so the tone can load Cinder's
// AAC encoder (cinder-aac/src/aac_enc.c, /system/vendor/cinder/lib/libfdk-aac-enc.so).
#include <errno.h>
#include <math.h>
#include <stdint.h>
#include "../../cinder-aac/src/aac_enc.h"
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <time.h>
#include <unistd.h>

#define AF_BT 31
static int g_m = -1;
static unsigned char g_addr[6];   // little-endian, as on the wire

static long long now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (long long)ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
}

static void put16(unsigned char* b, unsigned v) { b[0] = (unsigned char)v; b[1] = (unsigned char)(v >> 8); }

static void mgmt_send(unsigned op, const unsigned char* p, unsigned n) {
    unsigned char b[64];
    put16(b, op);
    put16(b + 2, 0);   // controller index 0 = hci0
    put16(b + 4, n);
    memcpy(b + 6, p, n);
    if (write(g_m, b, 6 + n) != (ssize_t)(6 + n)) printf("mgmt write 0x%04x: %s\n", op, strerror(errno));
}

// Send a command and wait for its result, answering pairing questions on the way. Returns the
// status (0 = done), or -1 on timeout.
static int mgmt(unsigned op, const unsigned char* p, unsigned n, int timeout_ms, const char* what) {
    mgmt_send(op, p, n);
    const long long end = now_ms() + timeout_ms;
    unsigned char b[512];
    while (now_ms() < end) {
        struct pollfd pf = {g_m, POLLIN, 0};
        if (poll(&pf, 1, (int)(end - now_ms())) <= 0) break;
        ssize_t r = read(g_m, b, sizeof b);
        if (r < 6) continue;
        const unsigned ev = b[0] | b[1] << 8;
        const unsigned char* d = b + 6;
        if ((ev == 0x0001 || ev == 0x0002) && (unsigned)(d[0] | d[1] << 8) == op) {
            printf("%-20s status 0x%02x\n", what, d[2]);
            return d[2];
        }
        unsigned char rep[32];
        switch (ev) {
        case 0x000e:   // PIN code request (a legacy device): 0000
            memcpy(rep, d, 7);
            rep[7] = 4;
            memset(rep + 8, 0, 16);
            memcpy(rep + 8, "0000", 4);
            mgmt_send(0x0016, rep, 24);
            printf("  answered a PIN request with 0000\n");
            break;
        case 0x000f:   // confirm a pairing number: yes (we have no screen in this test)
            memcpy(rep, d, 7);
            mgmt_send(0x001c, rep, 7);
            printf("  confirmed the pairing number\n");
            break;
        case 0x0011:
            printf("  authentication failed, status 0x%02x\n", d[7]);
            break;
        case 0x000b:
            printf("  connected\n");
            break;
        }
    }
    printf("%-20s no answer\n", what);
    return -1;
}

static const char* codec_name(const unsigned char* c, int n) {
    if (n < 2) return "?";
    switch (c[1]) {
    case 0x00: return "SBC";
    case 0x01: return "MP3";
    case 0x02: return "AAC";
    case 0xff:
        if (n >= 8) {
            const unsigned vid = c[2] | c[3] << 8 | (unsigned)c[4] << 16 | (unsigned)c[5] << 24, cid = c[6] | c[7] << 8;
            if (vid == 0x012d && cid == 0x00aa) return "LDAC";
            if (vid == 0x004f && cid == 0x0001) return "aptX";
            if (vid == 0x00d7 && cid == 0x0024) return "aptX HD";
            if (vid == 0x000a && cid == 0x0002) return "aptX Adaptive";
            static char v[40];
            snprintf(v, sizeof v, "vendor 0x%04x codec 0x%04x", vid, cid);
            return v;
        }
        return "vendor";
    default: return "other";
    }
}

static int avdtp(int s, unsigned char label, unsigned char sig, const unsigned char* p, int n, unsigned char* out, int cap) {
    unsigned char b[64] = {(unsigned char)(label << 4), sig};
    memcpy(b + 2, p, (size_t)n);
    if (send(s, b, (size_t)n + 2, 0) < 0) return -1;
    const long long end = now_ms() + 5000;
    while (now_ms() < end) {
        struct pollfd pf = {s, POLLIN, 0};
        if (poll(&pf, 1, (int)(end - now_ms())) <= 0) return -1;
        int r = (int)recv(s, out, (size_t)cap, 0);
        if (r < 2) return -1;
        if ((out[0] >> 4) == label && (out[1] & 0x3f) == sig) return (out[0] & 3) == 2 ? r : -2;
    }
    return -1;
}

// A2DP AAC, 44.1 kHz stereo, the core of an A2DP source: configure the sink endpoint `seid` (its
// codec capability, from the media-type byte on, in `cap`), open the media channel, start, and send
// `secs` of a 440 Hz tone at -30 dBFS as RTP packets of LATM frames, one frame (1024 samples) per
// 23.2 ms. Returns 0 when every frame went out.
static int play_tone(int sig, int seid, const unsigned char* cap, int secs) {
    unsigned max = (unsigned)(cap[5] & 0x7f) << 16 | cap[6] << 8 | cap[7];
    if (max == 0 || max > 256000) max = 256000;
    const unsigned char ot = cap[2] & 0x80 ? 0x80 : 0x40;   // MPEG-2 AAC LC, else MPEG-4 AAC LC
    unsigned char cfg[14] = {(unsigned char)(seid << 2), 1 << 2,   // their endpoint, ours (1)
                             1, 0,                                  // media transport
                             7, 8, 0x00, 0x02, ot, 0x01, 0x04,      // AAC, 44.1 kHz, 2 channels
                             (unsigned char)(max >> 16), (unsigned char)(max >> 8), (unsigned char)max};
    unsigned char r[256];
    const unsigned char se = (unsigned char)(seid << 2);
    if (avdtp(sig, 9, 0x03, cfg, sizeof cfg, r, sizeof r) < 0) { printf("NO-GO: set configuration refused\n"); return 2; }
    if (avdtp(sig, 10, 0x06, &se, 1, r, sizeof r) < 0) { printf("NO-GO: open refused\n"); return 2; }
    int m = socket(AF_BT, SOCK_SEQPACKET, 0);
    unsigned char la[14] = {AF_BT, 0}, ra[14] = {AF_BT, 0, 25, 0};
    memcpy(ra + 4, g_addr, 6);
    if (m < 0 || bind(m, (struct sockaddr*)la, sizeof la) < 0 || connect(m, (struct sockaddr*)ra, sizeof ra) < 0) {
        printf("NO-GO: media channel: %s\n", strerror(errno));
        return 2;
    }
    unsigned short opt[6] = {0};   // struct l2cap_options: omtu first
    socklen_t ol = sizeof opt;
    getsockopt(m, 6 /* SOL_L2CAP */, 1 /* L2CAP_OPTIONS */, opt, &ol);
    const int mtu = opt[0] ? opt[0] : 672;
    if (avdtp(sig, 11, 0x07, &se, 1, r, sizeof r) < 0) { printf("NO-GO: start refused\n"); return 2; }
    int br = aac_bitrate_for(mtu - 12, 44100, (int)max);
    const char* why = "";
    struct aac_enc* e = aac_enc_open(44100, br, AAC_LATM, &why);
    if (!e) { printf("NO-GO: AAC encoder: %s\n", why); return 2; }
    printf("streaming: AAC %d kb/s, media MTU %d, encoder %s\n", br / 1000, mtu, aac_enc_lib);
    static int16_t pcm[AAC_FRAME_SAMPLES * 2];
    static unsigned char pkt[2048];
    const int frames = secs * 44100 / AAC_FRAME_SAMPLES;
    unsigned seq = 0, ts = 0;
    long long ph = 0;
    int sent = 0, lost = 0;
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    for (int f = 0; f < frames; f++) {
        for (int i = 0; i < AAC_FRAME_SAMPLES; i++, ph++)
            pcm[2 * i] = pcm[2 * i + 1] = (int16_t)(1036 * sin(2 * M_PI * 440 * (double)ph / 44100));
        int n = aac_enc_fit(e, pcm, pkt + 12, (int)sizeof pkt - 12, mtu - 12);
        if (n > 0 && n <= mtu - 12) {
            const unsigned char h[12] = {0x80, 0x60, (unsigned char)(seq >> 8), (unsigned char)seq,
                                         (unsigned char)(ts >> 24), (unsigned char)(ts >> 16),
                                         (unsigned char)(ts >> 8), (unsigned char)ts, 0, 0, 0, 1};
            memcpy(pkt, h, 12);
            if (send(m, pkt, (size_t)n + 12, 0) == n + 12) sent++; else lost++;
            seq++;
        } else if (n > 0) {
            lost++;
        }
        ts += AAC_FRAME_SAMPLES;
        // Pace by the clock, not by the sleep: frame f+1 is due at start + (f+1) x 1024/44100 s.
        long long due = (long long)(f + 1) * AAC_FRAME_SAMPLES * 1000000000LL / 44100;
        struct timespec d = {t.tv_sec + (time_t)(due / 1000000000LL), t.tv_nsec + (long)(due % 1000000000LL)};
        if (d.tv_nsec >= 1000000000L) { d.tv_sec++; d.tv_nsec -= 1000000000L; }
        clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, &d, NULL);
    }
    printf("sent %d frames (%d lost), last bitrate %d kb/s\n", sent, lost, aac_enc_bitrate(e) / 1000);
    aac_enc_close(e);
    avdtp(sig, 12, 0x08, &se, 1, r, sizeof r);   // CLOSE
    close(m);
    return lost ? 3 : 0;
}

int main(int argc, char** argv) {
    setvbuf(stdout, NULL, _IOLBF, 0);
    unsigned a[6];
    if (argc < 2 || sscanf(argv[1], "%x:%x:%x:%x:%x:%x", &a[0], &a[1], &a[2], &a[3], &a[4], &a[5]) != 6) {
        printf("usage: btpair AA:BB:CC:DD:EE:FF\n");
        return 1;
    }
    for (int i = 0; i < 6; i++) g_addr[i] = (unsigned char)a[5 - i];
    g_m = socket(AF_BT, SOCK_RAW | SOCK_CLOEXEC, 1);
    unsigned char sa[6] = {AF_BT, 0, 0xff, 0xff, 3, 0};   // HCI_DEV_NONE, HCI_CHANNEL_CONTROL
    if (g_m < 0 || bind(g_m, (struct sockaddr*)sa, sizeof sa) < 0) {
        printf("management socket: %s\n", strerror(errno));
        return 1;
    }
    const unsigned char on = 1, noio = 3;
    mgmt(0x0005, &on, 1, 5000, "power on");
    mgmt(0x0009, &on, 1, 3000, "bondable");
    mgmt(0x000b, &on, 1, 3000, "secure simple pair");
    mgmt(0x0018, &noio, 1, 3000, "no keyboard/screen");
    unsigned char pd[8];
    memcpy(pd, g_addr, 6);
    pd[6] = 0;      // BR/EDR
    pd[7] = noio;
    const long long t0 = now_ms();
    int st = mgmt(0x0019, pd, 8, 40000, "pair");
    if (st != 0) {
        printf("NO-GO: pairing failed\n");
        return 2;
    }
    printf("paired in %lld ms\n", now_ms() - t0);

    int s = socket(AF_BT, SOCK_SEQPACKET, 0);   // L2CAP
    unsigned char la[14] = {AF_BT, 0};
    unsigned char ra[14] = {AF_BT, 0, 25, 0};   // PSM 25 = AVDTP
    memcpy(ra + 4, g_addr, 6);
    if (s < 0 || bind(s, (struct sockaddr*)la, sizeof la) < 0 || connect(s, (struct sockaddr*)ra, sizeof ra) < 0) {
        printf("NO-GO: A2DP signalling channel: %s\n", strerror(errno));
        return 2;
    }
    printf("A2DP signalling channel open\n");
    unsigned char r[512];
    int n = avdtp(s, 1, 0x01, NULL, 0, r, sizeof r);   // DISCOVER
    if (n < 2) {
        printf("NO-GO: discover failed (%d)\n", n);
        return 2;
    }
    int aac_seid = 0;
    unsigned char aac_cap[8] = {0};
    for (int i = 2; i + 1 < n; i += 2) {
        const int seid = r[i] >> 2, inuse = (r[i] >> 1) & 1, sink = (r[i + 1] >> 3) & 1, media = r[i + 1] >> 4;
        unsigned char q = (unsigned char)(seid << 2), c[256];
        int m = avdtp(s, (unsigned char)(2 + i / 2), 0x02, &q, 1, c, sizeof c);   // GET_CAPABILITIES
        const char* codec = "?";
        for (int k = 2; m > 0 && k + 1 < m; k += 2 + c[k + 1])
            if (c[k] == 7) {
                codec = codec_name(c + k + 2, c[k + 1]);
                if (sink && !inuse && c[k + 3] == 0x02 && c[k + 1] >= 8 && !aac_seid) {
                    aac_seid = seid;
                    memcpy(aac_cap, c + k + 2, 8);
                }
            }
        printf("endpoint %d: %s %s, %s%s\n", seid, media == 0 ? "audio" : "other", sink ? "sink" : "source", codec,
               inuse ? " (in use)" : "");
    }
    printf("GO: paired and read the endpoints over A2DP signalling\n");
    const int secs = argc > 2 ? atoi(argv[2]) : 0;
    int rc = 0;
    if (secs > 0) {
        if (!aac_seid) {
            printf("NO-GO: no free AAC sink endpoint\n");
            rc = 2;
        } else {
            rc = play_tone(s, aac_seid, aac_cap, secs);
            if (!rc) printf("GO: %d s of AAC sent\n", secs);
        }
    }
    close(s);
    return rc;
}
