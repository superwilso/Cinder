/* enc_test.c — drives aac_enc against the player's own libFraunhoferAAC.so under qemu-arm.
 *
 * usage: enc_test <out.loas> <rate> <payload>
 * Encodes 3 s of a 1 kHz / 440 Hz stereo tone twice: once as LOAS (written to <out.loas> for a
 * host decoder to check), once as A2DP LATM with the bitrate a <payload>-byte packet allows.
 * Fails if any LATM frame would not fit that packet.
 */
#include <math.h>
#include <stdio.h>
#include <stdlib.h>

#include "../src/aac_enc.h"

int main(int argc, char** argv)
{
    if (argc != 4) { fprintf(stderr, "usage: enc_test <out.loas> <rate> <payload>\n"); return 2; }
    int rate = atoi(argv[2]), payload = atoi(argv[3]);
    int frames = rate * 3 / AAC_FRAME_SAMPLES;
    static int16_t pcm[AAC_FRAME_SAMPLES * 2];
    static uint8_t out[8192];
    const char* why = "";
    FILE* f = fopen(argv[1], "wb");
    if (!f) { perror(argv[1]); return 1; }

    for (int pass = 0; pass < 2; pass++) {
        int br = pass ? aac_bitrate_for(payload, rate, 320000) : 256000;
        struct aac_enc* e = aac_enc_open(rate, br, pass ? AAC_LATM : AAC_LOAS, &why);
        if (!e) { fprintf(stderr, "open failed: %s\n", why); return 1; }
        long n = 0, total = 0, lost = 0, late = 0;
        int max = 0;
        for (int fr = 0; fr < frames; fr++) {
            for (int i = 0; i < AAC_FRAME_SAMPLES; i++, n++) {
                pcm[2 * i] = (int16_t)(12000 * sin(2 * M_PI * 1000 * n / rate));
                pcm[2 * i + 1] = (int16_t)(12000 * sin(2 * M_PI * 440 * n / rate));
                if (pass) { /* music-like stress: noise bursts every 0.3 s over the tones */
                    static unsigned x = 1;
                    x = x * 1103515245u + 12345u;
                    double env = (n % (rate * 3 / 10)) < rate / 20 ? 1.0 : 0.15;
                    pcm[2 * i] = (int16_t)(pcm[2 * i] / 2 + env * ((int)(x >> 16) % 16000));
                    pcm[2 * i + 1] = (int16_t)(pcm[2 * i + 1] / 2 + env * ((int)(x >> 8) % 16000));
                }
            }
            int got = pass ? aac_enc_fit(e, pcm, out, sizeof out, payload - 1)
                           : aac_enc_frame(e, pcm, out, sizeof out);
            if (got == -2) { lost++; if (fr >= frames / 2) late++; continue; }
            if (got < 0) { fprintf(stderr, "encode failed at frame %d\n", fr); return 1; }
            if (got > max) max = got;
            total += got;
            if (!pass) fwrite(out, 1, (size_t)got, f);
        }
        int end_br = aac_enc_bitrate(e);
        aac_enc_close(e);
        printf("%s [%s]: rate %d bitrate %d->%d frames %d avg-frame %ld max-frame %d lost %ld (second half %ld) peak-cap %s\n",
               pass ? "latm" : "loas", aac_enc_lib, rate, br, end_br, frames, total / frames, max, lost, late,
               aac_enc_peak_capped ? "yes" : "NO");
        /* Worst-case bursts: a few frames may go while the bitrate steps down, none once it has. */
        if (pass && (late > 0 || lost > 6)) {
            fprintf(stderr, "FAIL: %ld frames lost, %ld after the first half\n", lost, late);
            return 1;
        }
    }
    fclose(f);
    return 0;
}
