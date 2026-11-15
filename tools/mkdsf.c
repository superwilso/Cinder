/* Write a DSD64 stereo .dsf test file: the same multitone on both channels, for measuring the
 * player's DSD->PCM conversion (gain and filter, checklist §50) from the headphone jack.
 *
 *   cc -O2 -o mkdsf tools/mkdsf.c -lm && ./mkdsf out.dsf 40
 *
 * Tones at 1, 5, 10, 14, 17, 19, 20, 21 kHz, each at -24.4 dBFS (sum peaks at -6 dB). Second-order
 * delta-sigma, 1 bit; DSF layout: 4096-byte blocks per channel, LSB first. */
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

enum { FS = 2822400, BLK = 4096 };
static const double kTones[] = { 1000, 5000, 10000, 14000, 17000, 19000, 20000, 21000 };

static void put32(FILE* f, uint32_t v) { fwrite(&v, 4, 1, f); }
static void put64(FILE* f, uint64_t v) { fwrite(&v, 8, 1, f); }

int main(int argc, char** argv)
{
    if (argc < 2) { fprintf(stderr, "usage: mkdsf OUT.dsf [seconds]\n"); return 2; }
    const double secs = argc > 2 ? atof(argv[2]) : 40;
    uint64_t n = (uint64_t)(FS * secs);
    n -= n % (BLK * 8);
    const size_t bytes = n / 8;                    /* per channel */
    uint8_t* bits = calloc(bytes, 1);
    if (!bits) return 1;
    const int nt = sizeof kTones / sizeof *kTones;
    const double amp = 0.5 / nt;
    double i1 = 0, i2 = 0, y = 0;
    for (uint64_t k = 0; k < n; k++) {
        double x = 0;
        for (int t = 0; t < nt; t++) x += amp * sin(2 * M_PI * kTones[t] * (double)k / FS);
        i1 += x - y;
        i2 += i1 - y;
        const int b = i2 >= 0;
        y = b ? 1.0 : -1.0;
        if (b) bits[k / 8] |= (uint8_t)(1u << (k % 8));   /* LSB first */
    }
    FILE* f = fopen(argv[1], "wb");
    if (!f) return 1;
    const uint64_t data = 12 + 2 * (uint64_t)bytes, total = 28 + 52 + data;
    fwrite("DSD ", 4, 1, f); put64(f, 28); put64(f, total); put64(f, 0);
    fwrite("fmt ", 4, 1, f); put64(f, 52);
    put32(f, 1); put32(f, 0); put32(f, 2); put32(f, 2); put32(f, FS); put32(f, 1);
    put64(f, n); put32(f, BLK); put32(f, 0);
    fwrite("data", 4, 1, f); put64(f, data);
    for (size_t b = 0; b < bytes; b += BLK) {      /* the same signal in both channels */
        fwrite(bits + b, BLK, 1, f);
        fwrite(bits + b, BLK, 1, f);
    }
    fclose(f);
    /* Self-check: 1 kHz back out of the first second (a 64-tap boxcar decimation). */
    double re = 0, im = 0, s = 0;
    for (int k = 0; k < FS / 64; k++) {
        int c = 0;
        for (int j = 0; j < 64; j++) { const uint64_t q = (uint64_t)k * 64 + j; c += (bits[q / 8] >> (q % 8)) & 1; }
        s = (2.0 * c - 64) / 64;
        re += s * cos(2 * M_PI * 1000.0 * k * 64 / FS);
        im += s * sin(2 * M_PI * 1000.0 * k * 64 / FS);
    }
    const double a = 2 * sqrt(re * re + im * im) / (FS / 64);
    printf("%s: %.1f s, 1 kHz back at %.1f dBFS (want %.1f)\n", argv[1], (double)n / FS, 20 * log10(a),
           20 * log10(amp));
    free(bits);
    return fabs(20 * log10(a) - 20 * log10(amp)) < 0.5 ? 0 : 1;
}
