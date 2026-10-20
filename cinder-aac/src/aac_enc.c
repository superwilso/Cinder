/* aac_enc.c — see aac_enc.h. */
#include "aac_enc.h"

#include <dlfcn.h>
#include <stdlib.h>

/* The encoder's C interface, as libFraunhoferAAC.so exports it. */
typedef struct AACENCODER* HANDLE_AACENCODER;
typedef struct {
    int numBufs;
    void** bufs;
    int* bufferIdentifiers;
    int* bufSizes;
    int* bufElSizes;
} AACENC_BufDesc;
typedef struct {
    int numInSamples;
    int numAncBytes;
} AACENC_InArgs;
typedef struct {
    int numOutBytes;
    int numInSamples;
    int numAncBytes;
    int reserved[5]; /* newer releases add fields here; room so an older or newer one cannot overrun */
} AACENC_OutArgs;

/* Reached through dlopen, privately, the first time an encoder is opened: the library and its C++
 * runtime stay out of the Bluetooth service until AAC is actually used, and out of its global
 * symbol scope always. */
static struct {
    int (*open)(HANDLE_AACENCODER* h, unsigned modules, unsigned max_channels);
    int (*set)(HANDLE_AACENCODER h, int param, unsigned value);
    int (*encode)(HANDLE_AACENCODER h, const AACENC_BufDesc* in, const AACENC_BufDesc* out,
                  const AACENC_InArgs* in_args, AACENC_OutArgs* out_args);
    int (*close)(HANDLE_AACENCODER* h);
} fdk;

const char* aac_enc_lib;

/* Cinder's build of upstream fdk-aac 2.x first (build_fdk.sh; it can cap single frames), then the
 * firmware's own older encoder. The bare name is for tests, which find it on LD_LIBRARY_PATH. */
static const char* const libs[] = {"/system/vendor/unknown321/lib/libfdk-aac-enc.so",
                                   "libfdk-aac-enc.so", "libFraunhoferAAC.so"};

static int load(void)
{
    if (fdk.close) return 1;
    void* lib = NULL;
    for (unsigned i = 0; !lib && i < sizeof libs / sizeof *libs; i++)
        if ((lib = dlopen(libs[i], RTLD_NOW | RTLD_LOCAL))) aac_enc_lib = libs[i];
    if (!lib) return 0;
    fdk.open = dlsym(lib, "aacEncOpen");
    fdk.set = dlsym(lib, "aacEncoder_SetParam");
    fdk.encode = dlsym(lib, "aacEncEncode");
    fdk.close = (fdk.open && fdk.set && fdk.encode) ? dlsym(lib, "aacEncClose") : NULL;
    return fdk.close != NULL;
}

enum {
    AACENC_AOT = 0x0100,
    AACENC_BITRATE = 0x0101,
    AACENC_BITRATEMODE = 0x0102,
    AACENC_SAMPLERATE = 0x0103,
    AACENC_GRANULE_LENGTH = 0x0105,
    AACENC_CHANNELMODE = 0x0106,
    AACENC_AFTERBURNER = 0x0200,
    AACENC_PEAK_BITRATE = 0x0207,
    AACENC_TRANSMUX = 0x0300,
    AACENC_HEADER_PERIOD = 0x0301,
    AOT_AAC_LC = 2,
    MODE_2 = 2,
    IN_AUDIO_DATA = 0,
    OUT_BITSTREAM_DATA = 3,
};

int aac_enc_peak_capped;

struct aac_enc {
    HANDLE_AACENCODER h;
    int bitrate;
};

struct aac_enc* aac_enc_open(int rate, int bitrate, int transmux, const char** why)
{
    struct aac_enc* e = calloc(1, sizeof *e);
    const char* w = "out of memory";
    if (!e) goto fail;
    w = "no AAC encoder library loadable";
    if (!load()) goto fail;
    w = "aacEncOpen";
    if (fdk.open(&e->h, 0, 2) != 0) goto fail;
    w = "a parameter was refused";
    if (fdk.set(e->h, AACENC_AOT, AOT_AAC_LC) != 0) goto fail;
    if (fdk.set(e->h, AACENC_SAMPLERATE, (unsigned)rate) != 0) goto fail;
    if (fdk.set(e->h, AACENC_CHANNELMODE, MODE_2) != 0) goto fail;
    if (fdk.set(e->h, AACENC_GRANULE_LENGTH, AAC_FRAME_SAMPLES) != 0) goto fail;
    if (fdk.set(e->h, AACENC_BITRATEMODE, 0) != 0) goto fail; /* constant */
    if (fdk.set(e->h, AACENC_BITRATE, (unsigned)bitrate) != 0) goto fail;
    if (fdk.set(e->h, AACENC_TRANSMUX, (unsigned)transmux) != 0) goto fail;
    if (fdk.set(e->h, AACENC_HEADER_PERIOD, 1) != 0) goto fail;
    if (fdk.set(e->h, AACENC_AFTERBURNER, 1) != 0) goto fail;
    /* Caps any one frame at the average, so the bit reservoir cannot grow a frame past the
     * packet. Older encoder releases do not know the parameter; then the margin in
     * aac_bitrate_for is all there is. */
    aac_enc_peak_capped = fdk.set(e->h, AACENC_PEAK_BITRATE, (unsigned)bitrate) == 0;
    e->bitrate = bitrate;
    w = "encoder initialisation";
    if (fdk.encode(e->h, NULL, NULL, NULL, NULL) != 0) goto fail;
    return e;
fail:
    if (why) *why = w;
    aac_enc_close(e);
    return NULL;
}

int aac_enc_frame(struct aac_enc* e, const int16_t* pcm, uint8_t* out, int out_cap)
{
    void* ibuf = (void*)pcm;
    int iid = IN_AUDIO_DATA, isz = AAC_FRAME_SAMPLES * 2 * 2, iel = 2;
    void* obuf = out;
    int oid = OUT_BITSTREAM_DATA, osz = out_cap, oel = 1;
    AACENC_BufDesc in = {1, &ibuf, &iid, &isz, &iel};
    AACENC_BufDesc o = {1, &obuf, &oid, &osz, &oel};
    AACENC_InArgs ia = {AAC_FRAME_SAMPLES * 2, 0};
    AACENC_OutArgs oa = {0};
    if (fdk.encode(e->h, &in, &o, &ia, &oa) != 0) return -1;
    return oa.numOutBytes;
}

int aac_enc_fit(struct aac_enc* e, const int16_t* pcm, uint8_t* out, int out_cap, int fit)
{
    enum { FLOOR = 96000 };
    int n = aac_enc_frame(e, pcm, out, out_cap);
    if (n <= fit) return n;
    /* Too big for the packet: this frame is lost, so the next ones get 10% less. */
    if (e->bitrate > FLOOR) {
        int br = e->bitrate * 9 / 10;
        if (br < FLOOR) br = FLOOR;
        if (fdk.set(e->h, AACENC_BITRATE, (unsigned)br) == 0) e->bitrate = br;
    }
    return -2;
}

int aac_enc_bitrate(const struct aac_enc* e) { return e->bitrate; }

void aac_enc_close(struct aac_enc* e)
{
    if (!e) return;
    if (e->h) fdk.close(&e->h);
    free(e);
}

int aac_bitrate_for(int payload, int rate, int cap)
{
    /* 16 bytes: the StreamMuxConfig LATM repeats in every frame, its length bytes, and one
     * content-protection byte; then 10% for frame-to-frame variation. */
    long long room = payload - 16;
    if (room < 64) room = 64;
    /* 70% for the firmware's encoder: it cannot cap single frames (no AACENC_PEAK_BITRATE), and
     * with its bit reservoir a burst after a quiet passage reaches ~1.45x the average
     * (test/enc_test.c). fdk-aac 2.x caps them, so 90% leaves only rounding room. */
    load();
    int pct = aac_enc_lib && aac_enc_lib != libs[2] ? 90 : 70;
    long long br = room * 8 * rate / AAC_FRAME_SAMPLES * pct / 100;
    if (cap > 0 && br > cap) br = cap;
    return (int)br;
}
