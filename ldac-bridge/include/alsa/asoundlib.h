/* Minimal ALSA declaration shim for cross-building ldac-bridge WITHOUT the full
 * libasound2-dev package. It declares only the symbols capture.c uses, with the
 * stable ALSA ABI constant values, and links the DEVICE's libasound.so at link
 * time (arm-linux-gnueabihf, from artifacts/rootfs_mnt/lib/libasound.so).
 *
 * build.sh uses the real <alsa/asoundlib.h> automatically if libasound2-dev is
 * installed; this file is the fallback so the build needs no apt/sudo. If you
 * extend capture.c, either install libasound2-dev or add the new decls here.
 */
#ifndef LDAC_BRIDGE_ALSA_SHIM_H
#define LDAC_BRIDGE_ALSA_SHIM_H

#ifdef __cplusplus
extern "C" {
#endif

typedef struct _snd_pcm snd_pcm_t;          /* opaque */

/* snd_pcm_open() mode flags. Only NONBLOCK matters here: an availability probe must never park
   on a device another process owns. */
#define SND_PCM_NONBLOCK 0x00000001

typedef unsigned long  snd_pcm_uframes_t;
typedef signed long    snd_pcm_sframes_t;

/* snd_pcm_stream_t */
typedef enum {
    SND_PCM_STREAM_PLAYBACK = 0,
    SND_PCM_STREAM_CAPTURE  = 1
} snd_pcm_stream_t;

/* snd_pcm_access_t */
typedef enum {
    SND_PCM_ACCESS_MMAP_INTERLEAVED    = 0,
    SND_PCM_ACCESS_MMAP_NONINTERLEAVED = 1,
    SND_PCM_ACCESS_MMAP_COMPLEX        = 2,
    SND_PCM_ACCESS_RW_INTERLEAVED      = 3,
    SND_PCM_ACCESS_RW_NONINTERLEAVED   = 4
} snd_pcm_access_t;

/* snd_pcm_format_t (subset used here; values are ALSA-stable) */
typedef enum {
    SND_PCM_FORMAT_S16_LE = 2,
    SND_PCM_FORMAT_S24_LE = 6,   /* 24-bit in 4 bytes, low three */
    SND_PCM_FORMAT_S32_LE = 10
} snd_pcm_format_t;

int  snd_pcm_open(snd_pcm_t **pcm, const char *name, snd_pcm_stream_t stream, int mode);
int  snd_pcm_set_params(snd_pcm_t *pcm, snd_pcm_format_t format, snd_pcm_access_t access,
                        unsigned int channels, unsigned int rate,
                        int soft_resample, unsigned int latency);
snd_pcm_sframes_t snd_pcm_readi(snd_pcm_t *pcm, void *buffer, snd_pcm_uframes_t size);
int  snd_pcm_recover(snd_pcm_t *pcm, int err, int silent);
int  snd_pcm_close(snd_pcm_t *pcm);
const char *snd_strerror(int errnum);

#ifdef __cplusplus
}
#endif
#endif /* LDAC_BRIDGE_ALSA_SHIM_H */
