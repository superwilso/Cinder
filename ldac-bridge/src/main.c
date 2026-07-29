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
// STATUS: scaffold, and BLOCKED as a standalone daemon — see the banner in main().
//
// THE BLOCKER (found 2026-07-29, host-side): `libBtTransmitterService` is a `pst::services::*`
// client like every other one on this device, so its calls are ASYNCHRONOUS — the request is
// marshalled over binder and the reply is delivered by `pst::core::Framework`'s event looper.
// Nothing dispatches that looper unless someone drives `Framework::Pump()`, and this process
// starts no framework at all. Sony's wrappers do not initialise their out-params before the IPC,
// so with no pump a call does not fail cleanly: it returns whatever was on the stack. That is the
// same trap that cost weeks on PlayerService (`Connect()` "returned" 0xb6xxxxxx; `IsConnected()`
// read garbage and said true), and here it would surface as `GetSocketName returned empty` —
// indistinguishable from "the control-plane RE is wrong", which is the wrong thing to go fix.
//
// So the bring-up questions are answered by `cinder-probe --ldac` instead: it already starts the
// framework (`StartForApplication`) and runs a pump thread, and it needs an adb push rather than a
// .UPG flash. See ldac-bridge/TEST.md. Once those answers are in, the pump belongs here too (or,
// more likely, this whole pipeline moves inside cinder-home, which is an easel app and therefore
// has a live framework already).
#define _GNU_SOURCE
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>
#include <signal.h>
#include <stddef.h>
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
    // A write to a socket the transmitter has closed must be an EPIPE we can LOG, not a SIGPIPE
    // that kills the process with nothing in the log to say why. This runs unattended under
    // ldac-run.sh, so a silent death is the one failure mode we can't debug afterwards.
    signal(SIGPIPE, SIG_IGN);
    // Say the known blocker out loud, at the top of every log, so a run that fails at
    // GetSocketName is not read as "the control-plane RE is wrong" (see the file header).
    fprintf(stderr, "ldac-bridge: WARNING — no pst::core::Framework is started in this process, so "
                    "BtTransmitterService replies have nothing to deliver them and every call "
                    "below may return uninitialised stack. Use `cinder-probe --ldac` for bring-up.\n");

    // 1. Control plane: get the transmitter client and arm the LDAC source. Per RE
    // (RE_findings.md): the server opens the audio socket INTERNALLY, triggered by the
    // SetLdac/SetCurrentSource path — there is no client-side NotifyOpenAudio. So the
    // sequence is SetLdac(true) -> SetLdacSoundQuality -> SetCurrentSource(true).
    bt_client_t *bt = btclient_create();          // factory CreateInstance + Connect
    if (!bt) { fprintf(stderr, "btclient_create failed\n"); return 1; }
    btclient_set_ldac(bt, true);                   // SetLdac(true)
    btclient_set_ldac_quality(bt, BT_LDAC_AUTO);   // SetLdacSoundQuality(Auto)
    btclient_set_current_source(bt, true);         // SetCurrentSource(true) -> server opens the socket

    char sockname[128] = {0};
    btclient_get_socket_name(bt, sockname, sizeof(sockname));  // GetSocketName()
    if (sockname[0] == '\0') { fprintf(stderr, "GetSocketName returned empty\n"); return 1; }
    uint16_t chunk = btclient_pcm_preferred_size(bt);          // negotiated chunk, if any
    if (chunk == 0) chunk = 4096;
    fprintf(stderr, "ldac-bridge: socket='@%s' chunk=%u\n", sockname, chunk);

    // 2. Connect to the audio socket the server just opened. The open is async after
    // SetCurrentSource, so retry briefly until the server's listen() is up.
    int sock = -1;
    for (int attempt = 0; attempt < 20 && sock < 0; attempt++) {
        sock = bt_audio_connect(sockname);
        if (sock < 0) usleep(100 * 1000);          // 100 ms; ~2 s total
    }
    if (sock < 0) { fprintf(stderr, "bt_audio_connect failed after retries\n"); return 1; }

    // 3. Data plane: open the USB-DAC capture (44100 S32_LE 2ch). The UAC gadget card index
    // is DYNAMIC (on-device: card4 does not exist outside UAC mode) — discover it at runtime
    // rather than assuming hw:4,0. NOTE: in stock USB-DAC mode the Sony UAC service may already
    // own that capture PCM — see README; this may require stopping/redirecting it first.
    char capdev[32];
    if (capture_find_dev(capdev, sizeof capdev) != 0) {
        fprintf(stderr, "no USB-DAC capture card found — is the gadget in UAC mode "
                        "(setprop sys.sony.config uac) with a PC feeding audio? "
                        "falling back to hw:4,0\n");
        snprintf(capdev, sizeof capdev, "hw:4,0");
    } else {
        fprintf(stderr, "ldac-bridge: USB-DAC capture device = %s\n", capdev);
    }
    capture_t *cap = capture_open(capdev, 44100, 2, /*S32_LE*/ 4);
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
    btclient_set_current_source(bt, false);        // release the source -> server closes the socket
    btclient_destroy(bt);
    return 0;
}
