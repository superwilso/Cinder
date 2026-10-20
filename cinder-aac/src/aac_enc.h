/* aac_enc.h — AAC-LC frames for A2DP, from the Fraunhofer encoder already on the player.
 *
 * The declarations below are the encoder's public C interface (aacEncOpen / aacEncoder_SetParam /
 * aacEncEncode / aacEncClose), the one both libFraunhoferAAC.so and upstream fdk-aac export. Only
 * what this project calls is declared. The firmware's copy is never shipped; Cinder's own build of
 * upstream fdk-aac 2.x (build_fdk.sh) is preferred when installed.
 */
#ifndef CINDER_AAC_ENC_H
#define CINDER_AAC_ENC_H

#include <stdint.h>

/* One A2DP AAC frame carries this many samples per channel. */
#define AAC_FRAME_SAMPLES 1024

struct aac_enc;

/* Opens an AAC-LC stereo encoder at `rate` Hz and about `bitrate` bit/s. `transmux` picks the
 * framing: AAC_LATM (A2DP's MP4A-LATM, the StreamMuxConfig in every frame) or AAC_LOAS (the same
 * with a sync word, which a file decoder can read; tests only). NULL on failure; `why` gets a
 * short reason. */
enum { AAC_LATM = 6, AAC_LOAS = 10 };
struct aac_enc* aac_enc_open(int rate, int bitrate, int transmux, const char** why);

/* Encodes exactly AAC_FRAME_SAMPLES stereo S16 frames (4096 bytes) from `pcm` into `out`.
 * Returns the bytes written (0 while the encoder is still filling its delay line), or -1. */
int aac_enc_frame(struct aac_enc* e, const int16_t* pcm, uint8_t* out, int out_cap);

/* aac_enc_frame for a packet that holds at most `fit` bytes. A frame that would not fit is
 * returned as -2 (and is lost), and the encoder's bitrate steps down 10% for the frames after it,
 * not below 96 kbit/s: this encoder build has no way to cap a single frame. */
int aac_enc_fit(struct aac_enc* e, const int16_t* pcm, uint8_t* out, int out_cap, int fit);

/* The bitrate the encoder is running at now. */
int aac_enc_bitrate(const struct aac_enc* e);

void aac_enc_close(struct aac_enc* e);

/* The encoder library in use (its dlopen name), NULL until one has loaded. */
extern const char* aac_enc_lib;

/* Whether the last aac_enc_open could cap single frames at the average (AACENC_PEAK_BITRATE). */
extern int aac_enc_peak_capped;

/* The bitrate that keeps every frame inside one A2DP packet of `payload` bytes at `rate` Hz,
 * capped at `cap` (0 = no cap). Leaves room for the LATM mux config and a content-protection
 * byte. */
int aac_bitrate_for(int payload, int rate, int cap);

#endif
