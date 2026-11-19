// btspike.c — Phase 4 go/no-go (docs/PLAN_hardware_first.md): can a stack other than Sony's drive
// the Bluetooth core? With Sony's stack stopped, open /dev/stpbt (WMT powers the core on), send the
// power-on commands Sony's libbtdrv.so sends (decoded 2026-11-18; built from /data/BT_Addr), then run
// an HCI inquiry and print every device that answers. Closing the node powers the core off again.
// Writes nothing anywhere; reads /data/BT_Addr.
//
//   btspike [seconds]      default 10 s of inquiry
//
// Needs /dev/stpbt free: Sony's mtkbt holds it while Sony's Bluetooth is running (EBUSY otherwise).
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>

static int g_fd = -1;
static unsigned char g_rx[4096];
static size_t g_have = 0;

static long long now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return (long long)ts.tv_sec * 1000 + ts.tv_nsec / 1000000;
}

// One whole H4 event (type 0x04) into ev (code, len, params...), or 0 on timeout. Other packet
// types are skipped.
static int read_event(unsigned char* ev, int timeout_ms) {
    const long long end = now_ms() + timeout_ms;
    for (;;) {
        while (g_have > 0) {
            if (g_rx[0] == 0x04 && g_have >= 3 && g_have >= (size_t)3 + g_rx[2]) {
                const size_t n = 2 + g_rx[2];
                memcpy(ev, g_rx + 1, n);
                memmove(g_rx, g_rx + 1 + n, g_have - 1 - n);
                g_have -= 1 + n;
                return 1;
            }
            if (g_rx[0] == 0x04) break;   // incomplete event: read more
            // ACL (0x02, 4-byte header with 16-bit length) or garbage: drop it.
            if (g_rx[0] == 0x02 && g_have >= 5) {
                const size_t n = 5 + (g_rx[3] | g_rx[4] << 8);
                if (g_have < n) break;
                memmove(g_rx, g_rx + n, g_have - n);
                g_have -= n;
                continue;
            }
            if (g_rx[0] == 0x02) break;
            memmove(g_rx, g_rx + 1, --g_have);
        }
        const int left = (int)(end - now_ms());
        if (left <= 0) return 0;
        struct pollfd p = {g_fd, POLLIN, 0};
        if (poll(&p, 1, left) <= 0) return 0;
        const ssize_t r = read(g_fd, g_rx + g_have, sizeof g_rx - g_have);
        if (r <= 0) return 0;
        g_have += (size_t)r;
    }
}

// Send an HCI command; wait for its Command Complete (or Command Status). Returns the status byte,
// or -1 on timeout.
static int cmd(unsigned short op, const unsigned char* p, unsigned char n, const char* what) {
    unsigned char b[260] = {0x01, (unsigned char)op, (unsigned char)(op >> 8), n};
    memcpy(b + 4, p, n);
    if (write(g_fd, b, 4u + n) != 4 + n) {
        printf("%-22s 0x%04x: write failed (%s)\n", what, op, strerror(errno));
        return -1;
    }
    unsigned char ev[258];
    const long long end = now_ms() + 3000;
    while (now_ms() < end) {
        if (!read_event(ev, (int)(end - now_ms()))) break;
        if (ev[0] == 0x0e && (ev[3] | ev[4] << 8) == op) {
            printf("%-22s 0x%04x: status 0x%02x", what, op, ev[5]);
            if (op == 0x1009 && ev[1] >= 10)
                printf("  chip address %02x:%02x:%02x:%02x:%02x:%02x", ev[11], ev[10], ev[9], ev[8], ev[7], ev[6]);
            printf("\n");
            return ev[5];
        }
        if (ev[0] == 0x0f && (ev[4] | ev[5] << 8) == op) {
            printf("%-22s 0x%04x: status 0x%02x (command status)\n", what, op, ev[2]);
            return ev[2];
        }
    }
    printf("%-22s 0x%04x: no answer in 3 s\n", what, op);
    return -1;
}

int main(int argc, char** argv) {
    const int secs = argc > 1 ? atoi(argv[1]) : 10;
    unsigned char nv[64] = {0};
    int f = open("/data/BT_Addr", O_RDONLY);
    if (f < 0 || read(f, nv, sizeof nv) != (ssize_t)sizeof nv) {
        printf("cannot read the 64-byte /data/BT_Addr record (%s)\n", strerror(errno));
        return 1;
    }
    close(f);
    g_fd = open("/dev/stpbt", O_RDWR | O_NOCTTY);
    if (g_fd < 0) {
        printf("open /dev/stpbt: %s%s\n", strerror(errno),
               errno == EBUSY ? " — Sony's Bluetooth stack still holds it; stop it first" : "");
        return 1;
    }
    printf("/dev/stpbt open: the Bluetooth core is powered\n");
    // The power-on sequence, in libbtdrv.so's order (bt_init_script_consys). Offsets are into the
    // record: address 0x00 (6), radio 0x0C (6), sleep 0x12 (7), TX power offset 0x1B (3).
    unsigned char addr[6];
    for (int i = 0; i < 6; i++) addr[i] = nv[5 - i];   // HCI wants the address little-endian
    cmd(0x1009, NULL, 0, "read chip address");
    cmd(0xfc1a, addr, 6, "set address");
    cmd(0xfc79, nv + 0x0c, 6, "radio settings");
    cmd(0xfc93, nv + 0x1b, 3, "TX power offset");
    cmd(0xfc7a, nv + 0x12, 7, "sleep mode");
    if (cmd(0x0c03, NULL, 0, "reset") != 0) {
        printf("NO-GO: the core did not complete a reset\n");
        close(g_fd);
        return 2;
    }
    const unsigned char mode = 1;   // inquiry results with RSSI
    cmd(0x0c45, &mode, 1, "inquiry mode");
    unsigned char inq[5] = {0x33, 0x8b, 0x9e, (unsigned char)(secs * 100 / 128 + 1), 0};
    if (cmd(0x0401, inq, 5, "inquiry") != 0) {
        printf("NO-GO: inquiry refused\n");
        close(g_fd);
        return 2;
    }
    int found = 0;
    unsigned char ev[258];
    const long long end = now_ms() + secs * 1000 + 3000;
    while (now_ms() < end && read_event(ev, (int)(end - now_ms()))) {
        if (ev[0] == 0x01) break;   // Inquiry Complete
        if (ev[0] == 0x02 || ev[0] == 0x22 || ev[0] == 0x2f) {
            const int n = ev[0] == 0x2f ? 1 : ev[2];
            for (int i = 0; i < n; i++) {
                const unsigned char* a = ev + 3 + i * 6;
                printf("found %02x:%02x:%02x:%02x:%02x:%02x", a[5], a[4], a[3], a[2], a[1], a[0]);
                // One response: address 6, page scan mode 1, reserved 1, class 3, clock offset 2, then RSSI.
                if (ev[0] != 0x02 && n == 1) printf("  RSSI %d dBm", (signed char)ev[16]);
                printf("\n");
                found++;
            }
        }
    }
    printf("%s: %d answer(s) to the inquiry\n", found ? "GO" : "NO ANSWERS (is anything discoverable nearby?)", found);
    close(g_fd);   // WMT powers the core off
    return found ? 0 : 3;
}
