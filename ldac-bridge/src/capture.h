// capture.h — USB-DAC PCM capture (ALSA).
#ifndef LDAC_BRIDGE_CAPTURE_H
#define LDAC_BRIDGE_CAPTURE_H

typedef struct capture capture_t;

// Open a capture stream. dev e.g. "hw:4,0"; bytes_per_sample 4 = S32_LE.
capture_t *capture_open(const char *dev, unsigned rate, unsigned channels, unsigned bytes_per_sample);
// Read up to max_bytes; returns bytes read (>0), 0, or a negative errno/ALSA code.
int        capture_read(capture_t *c, void *buf, int max_bytes);
// Try to recover from an xrun/suspend; returns 0 if recovered.
int        capture_recover(capture_t *c, int err);
void       capture_close(capture_t *c);

#endif
