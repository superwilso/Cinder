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

struct capture {
    snd_pcm_t *pcm;
    unsigned frame_bytes;   // channels * bytes_per_sample
};

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
