// btbridge.c — Phase 4 step two (docs/PLAN_hardware_first.md): the Linux Bluetooth stack driving the
// core. Powers the core through /dev/stpbt with Sony's power-on commands (as btspike), opens
// /dev/vhci (hci_vhci.ko) so the kernel registers an hci device, and copies HCI packets between the
// two. Then, through the kernel's HCI socket, brings the device up and runs an inquiry. Closing
// both nodes removes the device and powers the core off. Reads /data/BT_Addr; writes nothing.
//
//   btbridge [seconds]      needs compat.ko, bluetooth.ko, hci_vhci.ko loaded; Sony's BT off
//   btbridge -w SECONDS     bridge only, for SECONDS (another tool drives hci0, e.g. btpair)
//   CINDER_BT_ALT=1         use a different address (test pairings leave the real one alone)
//   CINDER_BT_SNOOP=FILE    every packet both ways as btsnoop (Wireshark, analysis/tools/btsnoop_decode.py)
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/socket.h>
#include <time.h>
#include <unistd.h>

#define AF_BT 31
#define BTPROTO_HCI 1
#define HCIDEVUP _IOW('H', 201, int)
#define HCIDEVDOWN _IOW('H', 202, int)
#define HCIINQUIRY _IOR('H', 240, int)

static int g_bt = -1, g_vhci = -1, g_snoop = -1;
static pthread_mutex_t g_snoop_mu = PTHREAD_MUTEX_INITIALIZER;

// One btsnoop record (datalink 1002, H4). `in` = controller to host.
static void snoop(const unsigned char* p, size_t n, int in) {
    if (g_snoop < 0 || n == 0) return;
    struct timespec ts;
    clock_gettime(CLOCK_REALTIME, &ts);
    const unsigned long long us = (unsigned long long)ts.tv_sec * 1000000ULL + (unsigned long long)ts.tv_nsec / 1000 + 0x00dcddb30f2f8000ULL;
    const unsigned flags = (unsigned)in | (p[0] == 0x01 || p[0] == 0x04 ? 2u : 0u);
    unsigned char h[24];
    const unsigned v[4] = {(unsigned)n, (unsigned)n, flags, 0};
    for (int i = 0; i < 4; i++) for (int k = 0; k < 4; k++) h[i * 4 + k] = (unsigned char)(v[i] >> (24 - 8 * k));
    for (int k = 0; k < 8; k++) h[16 + k] = (unsigned char)(us >> (56 - 8 * k));
    pthread_mutex_lock(&g_snoop_mu);
    (void)!write(g_snoop, h, sizeof h);
    (void)!write(g_snoop, p, n);
    pthread_mutex_unlock(&g_snoop_mu);
}
static volatile int g_stop = 0;

static long long now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (long long)ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
}

// Length of the whole H4 packet at b (n bytes so far), 0 if more bytes are needed, -1 if b[0] is
// not a packet type the core sends.
static long h4_len(const unsigned char* b, size_t n) {
    switch (b[0]) {
    case 0x04: return n >= 3 ? 3 + b[2] : 0;
    case 0x02: return n >= 5 ? 5 + (b[3] | b[4] << 8) : 0;
    case 0x03: return n >= 4 ? 4 + b[3] : 0;
    default: return -1;
    }
}

// Power-on, as btspike: one command, wait for its Command Complete. Before the bridge starts.
static int init_cmd(unsigned short op, const unsigned char* p, unsigned char n) {
    unsigned char b[64] = {0x01, (unsigned char)op, (unsigned char)(op >> 8), n};
    memcpy(b + 4, p, n);
    if (write(g_bt, b, 4u + n) != 4 + n) return -1;
    unsigned char rx[512];
    size_t have = 0;
    const long long end = now_ms() + 3000;
    while (now_ms() < end) {
        struct pollfd pf = {g_bt, POLLIN, 0};
        if (poll(&pf, 1, (int)(end - now_ms())) <= 0) break;
        ssize_t r = read(g_bt, rx + have, sizeof rx - have);
        if (r <= 0) break;
        have += (size_t)r;
        while (have > 0) {
            long len = h4_len(rx, have);
            if (len < 0) { memmove(rx, rx + 1, --have); continue; }
            if (len == 0 || (size_t)len > have) break;
            int done = rx[0] == 0x04 && rx[1] == 0x0e && (rx[4] | rx[5] << 8) == op;
            int st = rx[6];
            memmove(rx, rx + len, have - (size_t)len);
            have -= (size_t)len;
            if (done) return st;
        }
    }
    return -1;
}

// Core → kernel: whole packets only (vhci takes one packet a write).
static void* up_thread(void* arg) {
    (void)arg;
    static unsigned char rx[4096];
    size_t have = 0;
    while (!g_stop) {
        struct pollfd pf = {g_bt, POLLIN, 0};
        if (poll(&pf, 1, 200) <= 0) continue;
        ssize_t r = read(g_bt, rx + have, sizeof rx - have);
        if (r <= 0) break;
        have += (size_t)r;
        while (have > 0) {
            long len = h4_len(rx, have);
            if (len < 0) { memmove(rx, rx + 1, --have); continue; }
            if (len == 0 || (size_t)len > have) break;
            // The core says it has feature page 2, then refuses to read it (status 0x30), and the
            // kernel's start-up fails on that (hci0 up: ENOSYS, 11-19). Its last page is 1.
            if (rx[0] == 0x04 && rx[1] == 0x0e && len >= 9 && (rx[4] | rx[5] << 8) == 0x1004 && rx[8] > 1)
                rx[8] = 1;
            // Every command's answer, so a failed start-up names the command and the status.
            if (rx[0] == 0x04 && rx[1] == 0x0e && len >= 7 && rx[6])
                printf("  done 0x%04x status 0x%02x\n", rx[4] | rx[5] << 8, rx[6]);
            if (rx[0] == 0x04 && rx[1] == 0x0f && len >= 7 && rx[3])
                printf("  cmd status 0x%04x status 0x%02x\n", rx[5] | rx[6] << 8, rx[3]);
            snoop(rx, (size_t)len, 1);
            if (write(g_vhci, rx, (size_t)len) != len) fprintf(stderr, "vhci write: %s\n", strerror(errno));
            memmove(rx, rx + len, have - (size_t)len);
            have -= (size_t)len;
        }
    }
    return NULL;
}

// Kernel → core: one packet a read; vhci's own 0xff vendor packet (the device index) stays here.
static void* down_thread(void* arg) {
    (void)arg;
    static unsigned char b[2048];
    while (!g_stop) {
        struct pollfd pf = {g_vhci, POLLIN, 0};
        if (poll(&pf, 1, 200) <= 0) continue;
        ssize_t r = read(g_vhci, b, sizeof b);
        if (r <= 0) break;
        if (b[0] == 0xff) {
            printf("vhci: kernel registered hci%d\n", r >= 4 ? b[2] | b[3] << 8 : -1);
            continue;
        }
        snoop(b, (size_t)r, 0);
        if (write(g_bt, b, (size_t)r) != r) fprintf(stderr, "stpbt write: %s\n", strerror(errno));
    }
    return NULL;
}

int main(int argc, char** argv) {
    setvbuf(stdout, NULL, _IOLBF, 0);
    const int wait_only = argc > 2 && !strcmp(argv[1], "-w");
    const int secs = atoi(argc > 2 ? argv[2] : argc > 1 ? argv[1] : "10") ?: 10;
    unsigned char nv[64];
    int f = open("/data/BT_Addr", O_RDONLY);
    if (f < 0 || read(f, nv, sizeof nv) != (ssize_t)sizeof nv) { printf("cannot read /data/BT_Addr\n"); return 1; }
    close(f);
    g_bt = open("/dev/stpbt", O_RDWR | O_NOCTTY);
    if (g_bt < 0) { printf("open /dev/stpbt: %s\n", strerror(errno)); return 1; }
    unsigned char addr[6];
    for (int i = 0; i < 6; i++) addr[i] = nv[5 - i];
    // CINDER_BT_ALT=1: a different address, so a test pairing cannot replace the key the owner's
    // headphones hold for this player under Sony's stack (it would break their pairing).
    if (getenv("CINDER_BT_ALT")) addr[0] ^= 0x5a;
    int st = init_cmd(0xfc1a, addr, 6) | init_cmd(0xfc79, nv + 0x0c, 6) | init_cmd(0xfc93, nv + 0x1b, 3)
        | init_cmd(0xfc7a, nv + 0x12, 7);
    printf("power-on commands: %s\n", st == 0 ? "all status 0" : "one failed");
    if (getenv("CINDER_BT_SNOOP")) {
        g_snoop = open(getenv("CINDER_BT_SNOOP"), O_WRONLY | O_CREAT | O_TRUNC, 0644);
        if (g_snoop >= 0) (void)!write(g_snoop, "btsnoop\0\0\0\0\1\0\0\x03\xea", 16);
    }
    g_vhci = open("/dev/vhci", O_RDWR);
    if (g_vhci < 0) { printf("open /dev/vhci: %s (hci_vhci.ko loaded?)\n", strerror(errno)); return 1; }
    pthread_t a, b;
    pthread_create(&a, NULL, up_thread, NULL);
    pthread_create(&b, NULL, down_thread, NULL);
    sleep(2);   // vhci registers the device ~1 s after open
    int rc = 0;
    if (wait_only) {
        sleep(secs);
    } else {
        int s = socket(AF_BT, SOCK_RAW | SOCK_CLOEXEC, BTPROTO_HCI);
        if (s < 0) { printf("HCI socket: %s\n", strerror(errno)); rc = 1; goto out; }
        const long long t0 = now_ms();
        if (ioctl(s, HCIDEVUP, 0) < 0 && errno != EALREADY) {
            printf("NO-GO: hci0 up: %s\n", strerror(errno));
            rc = 2;
            close(s);
            goto out;
        }
        printf("hci0 up in %lld ms (the kernel's own init ran over the bridge)\n", now_ms() - t0);
        // struct hci_inquiry_req is u16 dev_id, u16 flags, u8 lap[3], u8 length, u8 num_rsp, padded
        // to 10 bytes; the kernel writes inquiry_info (14 bytes each) after those 10.
        static unsigned char q[10 + 32 * 14];
        memset(q, 0, sizeof q);
        q[0] = 0; q[1] = 0; q[2] = 1; q[3] = 0;
        q[4] = 0x33; q[5] = 0x8b; q[6] = 0x9e;
        q[7] = (unsigned char)(secs * 100 / 128 + 1);
        q[8] = 32;
        if (ioctl(s, HCIINQUIRY, q) < 0) {
            printf("NO-GO: inquiry through the kernel: %s\n", strerror(errno));
            rc = 2;
        } else {
            printf("%s: %d answer(s) through the kernel stack\n", q[8] ? "GO" : "NO ANSWERS", q[8]);
            for (int i = 0; i < q[8]; i++) {
                const unsigned char* d = q + 10 + i * 14;
                printf("found %02x:%02x:%02x:%02x:%02x:%02x  class %02x%02x%02x\n", d[5], d[4], d[3], d[2],
                       d[1], d[0], d[11], d[10], d[9]);
            }
            if (!q[8]) rc = 3;
        }
        ioctl(s, HCIDEVDOWN, 0);
        close(s);
    }
out:
    g_stop = 1;
    pthread_join(a, NULL);
    pthread_join(b, NULL);
    close(g_vhci);   // the kernel drops hci0
    close(g_bt);     // WMT powers the core off
    return rc;
}
