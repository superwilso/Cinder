/* cinder-fmrec.c — FM recording on the headphone jack (the `fmrec` component).
 *
 * When the radio plays on the jack, Sony's AudioInPlayerService reads it from hw:0,1 with
 * snd_pcm_readi and plays it back through the sound service. This library is preloaded into that
 * service by Cinder's hagodaemon wrapper (HAGO_DIR/AudioInPlayerService.so, src/cinder-hagowrap.c)
 * and copies every block it reads to fmrec_feed() (src/fmrec.h), which writes a WAV file while
 * cinder-home asks for one. With BT OUT on, cinder-home reads hw:0,1 itself and feeds the same
 * writer, so this library sees no reads then.
 *
 * The service picks the format itself (libAudioInPlayerService.so: set_format, set_channels,
 * set_rate_near), so those three are hooked too, only to learn what it chose. snd_pcm_drop and
 * snd_pcm_close finish the file when the radio stops under a recording.
 *
 * Every hook calls Sony's original first and returns its result untouched. The originals come from
 * libasound itself, not RTLD_NEXT: hagodaemon dlopens the service library privately, so libasound
 * is not in the global scope and RTLD_NEXT finds nothing after us. Measured 2026-10-22: with
 * RTLD_NEXT alone every hook answered -ENOSYS and AudioIn's Play() returned 1, so the radio went
 * silent. cinder-mono.c met the same thing in SoundServiceFw (alsa_sym there).
 */
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#include <dlfcn.h>
#include "fmrec.h"

typedef struct _snd_pcm snd_pcm_t;
typedef struct _snd_pcm_hw_params snd_pcm_hw_params_t;

static struct fmrec g_rec;
static unsigned g_rate = 44100, g_ch = 2, g_bps = 2;

typedef long (*readi_fn)(snd_pcm_t*, void*, unsigned long);
typedef int (*format_fn)(snd_pcm_t*, snd_pcm_hw_params_t*, int);
typedef int (*channels_fn)(snd_pcm_t*, snd_pcm_hw_params_t*, unsigned);
typedef int (*rate_fn)(snd_pcm_t*, snd_pcm_hw_params_t*, unsigned*, int*);
typedef int (*pcm_fn)(snd_pcm_t*);

/* RTLD_NOLOAD: libasound is loaded by the time the service calls any of these, and this must never
 * be what loads it. A handle lookup searches libasound and its needs, never our own hooks. */
static void* alsa_sym(const char* name)
{
    static void* lib;
    void* p = dlsym(RTLD_NEXT, name);
    if (p) return p;
    if (!lib) lib = dlopen("libasound.so", RTLD_NOW | RTLD_NOLOAD);
    return lib ? dlsym(lib, name) : NULL;
}

#define NEXT(name, type) \
    static type real; \
    if (!real) real = (type)alsa_sym(name)

long snd_pcm_readi(snd_pcm_t* pcm, void* buf, unsigned long frames)
{
    NEXT("snd_pcm_readi", readi_fn);
    if (!real) return -38;   /* -ENOSYS */
    const long got = real(pcm, buf, frames);
    if (got > 0) fmrec_feed(&g_rec, buf, (unsigned long)got, g_rate, g_ch, g_bps);
    return got;
}

int snd_pcm_hw_params_set_format(snd_pcm_t* pcm, snd_pcm_hw_params_t* p, int fmt)
{
    NEXT("snd_pcm_hw_params_set_format", format_fn);
    if (!real) return -38;
    const int rc = real(pcm, p, fmt);
    /* SND_PCM_FORMAT_S16_LE = 2, S32_LE = 10. Anything else: bps 0, which fmrec_feed refuses. */
    if (rc == 0) g_bps = fmt == 2 ? 2 : fmt == 10 ? 4 : 0;
    return rc;
}

int snd_pcm_hw_params_set_channels(snd_pcm_t* pcm, snd_pcm_hw_params_t* p, unsigned ch)
{
    NEXT("snd_pcm_hw_params_set_channels", channels_fn);
    if (!real) return -38;
    const int rc = real(pcm, p, ch);
    if (rc == 0) g_ch = ch;
    return rc;
}

int snd_pcm_hw_params_set_rate_near(snd_pcm_t* pcm, snd_pcm_hw_params_t* p, unsigned* val, int* dir)
{
    NEXT("snd_pcm_hw_params_set_rate_near", rate_fn);
    if (!real) return -38;
    const int rc = real(pcm, p, val, dir);
    if (rc == 0 && val) g_rate = *val;
    return rc;
}

int snd_pcm_drop(snd_pcm_t* pcm)
{
    NEXT("snd_pcm_drop", pcm_fn);
    if (!real) return -38;
    fmrec_stop(&g_rec, "the radio stopped");
    return real(pcm);
}

int snd_pcm_close(snd_pcm_t* pcm)
{
    NEXT("snd_pcm_close", pcm_fn);
    if (!real) return -38;
    fmrec_stop(&g_rec, "the radio stopped");
    return real(pcm);
}
