// ldac-bridge — USB-DAC input -> LDAC (Bluetooth) output bridge for NW-A50.
//
// Pipeline (Strategy B, see analysis/E_usbdac_ldac/RE_findings.md):
//   USB-DAC capture (ALSA card4/pcm0c, 44100 S32_LE 2ch)
//     -> this daemon
//       -> abstract AF_UNIX socket "\0"+BtTransmitterService::GetSocketName()
//         -> BtTransmitterService (reads recv(), LDAC-encodes, sends to BT chip)
//
// Control plane (btclient.c): instantiate the Sony BtTransmitterServiceClient via
// its exported factory, then NotifyOpenAudio / SetLdac / SetLdacSoundQuality /
// GetSocketName / NotifyPcmPreferredSize.
//
// STATUS: scaffold. The abstract-socket writer below is complete. capture_open()
// and the btclient_* control plane need on-device completion (see README).
#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>
#include <sys/socket.h>
#include <sys/un.h>

#include "btclient.h"
#include "capture.h"

// Connect an AF_UNIX SOCK_STREAM socket to an ABSTRACT-namespace name (leading
// NUL in sun_path), matching what BtTransmitterService binds/listens on. Returns
// a connected fd, or -1. (Decompiled server side: socket(AF_UNIX,SOCK_STREAM);
// sun_path[0]='\0'; name copied after; bind(addrlen 110); listen(1); accept().)
static int bt_audio_connect(const char *name) {
    int fd = socket(AF_UNIX, SOCK_STREAM, 0);
    if (fd < 0) { perror("socket"); return -1; }
    struct sockaddr_un addr;
    memset(&addr, 0, sizeof(addr));
    addr.sun_family = AF_UNIX;
    // abstract namespace: sun_path[0] stays NUL, name follows
    size_t n = strlen(name);
    if (n + 1 > sizeof(addr.sun_path)) { fprintf(stderr, "socket name too long\n"); close(fd); return -1; }
    memcpy(addr.sun_path + 1, name, n);
    socklen_t len = (socklen_t)(offsetof(struct sockaddr_un, sun_path) + 1 + n);
    if (connect(fd, (struct sockaddr *)&addr, len) < 0) {
        fprintf(stderr, "connect(@%s): %s\n", name, strerror(errno));
        close(fd);
        return -1;
    }
    return fd;
}

// Write exactly len bytes (handle short writes).
static int write_all(int fd, const void *buf, size_t len) {
    const char *p = buf;
    while (len) {
        ssize_t w = write(fd, p, len);
        if (w < 0) { if (errno == EINTR) continue; return -1; }
        p += w; len -= (size_t)w;
    }
    return 0;
}

int main(void) {
    fprintf(stderr, "ldac-bridge: starting\n");

    // 1. Control plane: get the transmitter client and open its audio pipe.
    bt_client_t *bt = btclient_create();          // factory CreateInstance + Connect
    if (!bt) { fprintf(stderr, "btclient_create failed\n"); return 1; }
    btclient_set_ldac(bt, true);                   // SetLdac(true)
    btclient_set_ldac_quality(bt, BT_LDAC_AUTO);   // SetLdacSoundQuality(Auto)
    btclient_notify_open_audio(bt);                // NotifyOpenAudio() -> server starts listening

    uint16_t chunk = 0;
    char sockname[128] = {0};
    btclient_get_socket_name(bt, sockname, sizeof(sockname));  // GetSocketName()
    chunk = btclient_pcm_preferred_size(bt);       // NotifyPcmPreferredSize / negotiated chunk
    if (chunk == 0) chunk = 4096;
    fprintf(stderr, "ldac-bridge: socket='@%s' chunk=%u\n", sockname, chunk);

    // 2. Connect to the audio socket the server just opened.
    int sock = bt_audio_connect(sockname);
    if (sock < 0) { fprintf(stderr, "bt_audio_connect failed\n"); return 1; }

    // 3. Data plane: open the USB-DAC capture (44100 S32_LE 2ch). NOTE: in stock
    // USB-DAC mode the UAC service already owns card4/pcm0c — see README; this may
    // require stopping/redirecting that service first.
    capture_t *cap = capture_open("hw:4,0", 44100, 2, /*S32_LE*/ 4);
    if (!cap) { fprintf(stderr, "capture_open failed\n"); return 1; }

    // 4. Pump: read PCM frames -> write to the BT audio socket in chunk-sized writes.
    unsigned char *buf = malloc(chunk);
    if (!buf) return 1;
    fprintf(stderr, "ldac-bridge: bridging USB-DAC -> LDAC\n");
    for (;;) {
        int got = capture_read(cap, buf, chunk);   // bytes
        if (got <= 0) {
            if (capture_recover(cap, got) == 0) continue;
            fprintf(stderr, "capture error %d, stopping\n", got);
            break;
        }
        if (write_all(sock, buf, (size_t)got) < 0) {
            fprintf(stderr, "socket write error: %s\n", strerror(errno));
            break;
        }
    }

    free(buf);
    capture_close(cap);
    close(sock);
    btclient_notify_close_audio(bt);
    btclient_destroy(bt);
    return 0;
}
