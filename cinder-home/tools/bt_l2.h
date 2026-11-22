// bt_l2.h — the kernel Bluetooth calls the Phase 4 tools share (btpair.c, btsource.c): the
// management socket (commands, answered pairing questions) and AVDTP signalling over L2CAP.
// Header-only, static: each tool is one file plus this.
#ifndef CINDER_BT_L2_H
#define CINDER_BT_L2_H
#include <errno.h>
#include <poll.h>
#include <stdio.h>
#include <string.h>
#include <sys/socket.h>
#include <time.h>
#include <unistd.h>

#define AF_BT 31
static int g_m = -1;
static unsigned char g_addr[6];   // little-endian, as on the wire

static inline long long now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (long long)ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
}

static inline void put16(unsigned char* b, unsigned v) { b[0] = (unsigned char)v; b[1] = (unsigned char)(v >> 8); }

static inline void mgmt_send(unsigned op, const unsigned char* p, unsigned n) {
    unsigned char b[64];
    put16(b, op);
    put16(b + 2, 0);   // controller index 0 = hci0
    put16(b + 4, n);
    memcpy(b + 6, p, n);
    if (write(g_m, b, 6 + n) != (ssize_t)(6 + n)) printf("mgmt write 0x%04x: %s\n", op, strerror(errno));
}

// Send a command and wait for its result, answering pairing questions on the way. Returns the
// status (0 = done), or -1 on timeout.
static inline int mgmt(unsigned op, const unsigned char* p, unsigned n, int timeout_ms, const char* what) {
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

static inline const char* codec_name(const unsigned char* c, int n) {
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

static inline int avdtp(int s, unsigned char label, unsigned char sig, const unsigned char* p, int n, unsigned char* out, int cap) {
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

#endif
