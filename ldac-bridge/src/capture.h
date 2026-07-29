// capture.h — USB-DAC PCM capture (ALSA).
#ifndef LDAC_BRIDGE_CAPTURE_H
#define LDAC_BRIDGE_CAPTURE_H

#include <stddef.h>

typedef struct capture capture_t;

// Discover the USB-DAC capture PCM ("hw:C,D") at runtime — the UAC gadget card index is
// dynamic, NOT always 4. Fills `out`; returns 0 on success, -1 if no USB-DAC card is up.
// `LDAC_CAP_DEV` env overrides. See the comment in capture.c.
int        capture_find_dev(char *out, size_t n);
// Open a capture stream. dev e.g. "hw:4,0"; bytes_per_sample 4 = S32_LE.
capture_t *capture_open(const char *dev, unsigned rate, unsigned channels, unsigned bytes_per_sample);
// Read up to max_bytes; returns bytes read (>0), 0, or a negative errno/ALSA code.
int        capture_read(capture_t *c, void *buf, int max_bytes);
// Try to recover from an xrun/suspend; returns 0 if recovered.
int        capture_recover(capture_t *c, int err);
void       capture_close(capture_t *c);

#endif
