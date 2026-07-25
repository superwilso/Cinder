// capture.c — USB-DAC PCM capture via libasound (the device ships /lib/libasound.so;
// aplay links it). Interleaved S32_LE stereo @ 44100 from card4/pcm0c.
//
// !!! KEY ON-DEVICE UNKNOWN: in stock USB-DAC mode the Sony UAC service
// (UsbDeviceAudioPlayerService / libaudiohal-uacalsasingletrack) already has
// card4/pcm0c OPEN (routing it to card0). PCM capture substreams are exclusive, so
// this open() will likely fail with -EBUSY until that service is stopped or its
// routing is redirected. Resolving this is the core data-plane task — options:
//   (a) stop/suspend the stock UAC routing so card4 capture is free for us;
//   (b) replace libaudiohal-uacalsasingletrack so the UAC path writes to our socket;
//   (c) capture from a loopback the UAC path is pointed at.
// See analysis/E_usbdac_ldac/RE_findings.md.
#include "capture.h"
#include <alsa/asoundlib.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

struct capture {
    snd_pcm_t *pcm;
    unsigned frame_bytes;   // channels * bytes_per_sample
};

// Discover the USB-DAC capture PCM at runtime instead of assuming "hw:4,0".
//
// On-device probe (2026-07-25): outside UAC mode ONLY card0 ("sonysoccard", the built-in
// cxd3778gf codec) exists. The UAC gadget registers a SEPARATE ALSA card only while the
// gadget is in UAC mode, and the kernel gives it the next FREE index — which is NOT
// guaranteed to be 4. Hardcoding hw:4,0 was therefore fragile (wrong card, or no card at
// all). Strategy: scan /proc/asound/cards for a capture-capable card whose id is NOT the
// built-in codec, and return its first capture PCM as an ALSA "hw:C,D" name.
//
// `LDAC_CAP_DEV` in the environment overrides discovery (test harness / odd firmware).
// Returns 0 and fills `out` on success; -1 if no USB-DAC capture card is present.
int capture_find_dev(char *out, size_t n) {
    const char *env = getenv("LDAC_CAP_DEV");
    if (env && *env) { snprintf(out, n, "%s", env); return 0; }

    FILE *f = fopen("/proc/asound/cards", "r");
    if (!f) return -1;
    char line[256];
    int rc = -1;
    while (rc != 0 && fgets(line, sizeof line, f)) {
        // format: " 4 [Gadget         ]: usb-audio - ..."
        int idx;
        char id[64];
        if (sscanf(line, " %d [%63[^]]", &idx, id) != 2) continue;
        char *e = id + strlen(id);
        while (e > id && e[-1] == ' ') *--e = 0;      // trim trailing pad spaces
        if (strcmp(id, "sonysoccard") == 0) continue; // built-in codec — not the USB-DAC
        for (int d = 0; d < 8; ++d) {                 // find its first capture pcm device
            char p[64];
            snprintf(p, sizeof p, "/proc/asound/card%d/pcm%dc", idx, d);
            if (access(p, F_OK) == 0) { snprintf(out, n, "hw:%d,%d", idx, d); rc = 0; break; }
        }
    }
    fclose(f);
    return rc;
}

capture_t *capture_open(const char *dev, unsigned rate, unsigned channels, unsigned bps) {
    snd_pcm_t *pcm = NULL;
    int err = snd_pcm_open(&pcm, dev, SND_PCM_STREAM_CAPTURE, 0);
    if (err < 0) { fprintf(stderr, "snd_pcm_open(%s): %s\n", dev, snd_strerror(err)); return NULL; }
    snd_pcm_format_t fmt = (bps == 4) ? SND_PCM_FORMAT_S32_LE
                         : (bps == 2) ? SND_PCM_FORMAT_S16_LE
                                      : SND_PCM_FORMAT_S24_LE;
    err = snd_pcm_set_params(pcm, fmt, SND_PCM_ACCESS_RW_INTERLEAVED,
                             channels, rate, /*soft_resample*/ 1, /*latency us*/ 100000);
    if (err < 0) { fprintf(stderr, "snd_pcm_set_params: %s\n", snd_strerror(err)); snd_pcm_close(pcm); return NULL; }
    capture_t *c = calloc(1, sizeof(*c));
    if (!c) { snd_pcm_close(pcm); return NULL; }
    c->pcm = pcm;
    c->frame_bytes = channels * bps;
    return c;
}

int capture_read(capture_t *c, void *buf, int max_bytes) {
    snd_pcm_uframes_t want = (snd_pcm_uframes_t)(max_bytes / c->frame_bytes);
    snd_pcm_sframes_t got = snd_pcm_readi(c->pcm, buf, want);
    if (got < 0) return (int)got;          // negative ALSA error
    return (int)((snd_pcm_uframes_t)got * c->frame_bytes);
}

int capture_recover(capture_t *c, int err) {
    return snd_pcm_recover(c->pcm, err, /*silent*/ 1);
}

void capture_close(capture_t *c) {
    if (!c) return;
    if (c->pcm) snd_pcm_close(c->pcm);
    free(c);
}
