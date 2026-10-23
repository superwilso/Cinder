/* fmrec_selftest.c — the FM recorder's WAV writer (src/fmrec.h), off the device.
 * Run by tools/test_fmrec.sh. Checks: nothing is written until the control file names a path; the
 * header is valid mid-recording (the five-second rewrite) and after stop; a file that cannot be
 * created is refused once, not retried every poll; an unknown sample format records nothing. */
#include <stdlib.h>
#include <unistd.h>

static char g_dir[64];
#define FMREC_CTL   (g_ctl)
#define FMREC_STATE (g_state)
static char g_ctl[96], g_state[96];
#include "fmrec.h"

static int fails;
#define CHECK(c, what) do { if (c) printf("  ok   %s\n", what); else { printf("  FAIL %s\n", what); ++fails; } } while (0)

static unsigned long rd32(const unsigned char* p) { return p[0] | p[1] << 8 | p[2] << 16 | (unsigned long)p[3] << 24; }

static long file_size(const char* p) { struct stat st; return stat(p, &st) == 0 ? (long)st.st_size : -1; }

/* Reads the header and returns the data size it claims, or -1 if it is not a 16-bit stereo WAV. */
static long header_data(const char* p, unsigned rate)
{
    unsigned char h[44];
    FILE* f = fopen(p, "rb");
    if (!f) return -1;
    size_t n = fread(h, 1, 44, f);
    fclose(f);
    if (n != 44 || memcmp(h, "RIFF", 4) || memcmp(h + 8, "WAVEfmt ", 8) || memcmp(h + 36, "data", 4)) return -1;
    if (rd32(h + 24) != rate || (h[22] | h[23] << 8) != 2 || (h[34] | h[35] << 8) != 16) return -1;
    if (rd32(h + 4) != 36 + rd32(h + 40)) return -1;
    return (long)rd32(h + 40);
}

static void ctl(const char* path)
{
    FILE* f = fopen(g_ctl, "w");
    fprintf(f, "%s\n", path);
    fclose(f);
}

int main(void)
{
    snprintf(g_dir, sizeof g_dir, "/tmp/fmrec_selftest.%d", (int)getpid());
    mkdir(g_dir, 0755);
    snprintf(g_ctl, sizeof g_ctl, "%s/ctl", g_dir);
    snprintf(g_state, sizeof g_state, "%s/state", g_dir);
    char wav[128];
    snprintf(wav, sizeof wav, "%s/rec.wav", g_dir);

    static short block[2 * 4410];   /* 0.1 s of 44.1 kHz stereo */
    for (int i = 0; i < 2 * 4410; ++i) block[i] = (short)(i * 7);
    struct fmrec r = {0};

    puts("== fmrec");
    for (int i = 0; i < 10; ++i) fmrec_feed(&r, block, 4410, 44100, 2, 2);
    CHECK(!r.f && file_size(wav) < 0, "no control file: nothing is recorded");

    ctl(wav);
    for (int i = 0; i < 3; ++i) fmrec_feed(&r, block, 4410, 44100, 2, 2);   /* poll lands */
    CHECK(r.f != NULL, "control file names a path: recording starts within a quarter second");
    for (int i = 0; i < 60; ++i) fmrec_feed(&r, block, 4410, 44100, 2, 2);  /* 6 s more */
    long mid = header_data(wav, 44100);
    CHECK(mid > 0 && mid % 4 == 0, "mid-recording the header is valid and counts whole frames");

    remove(g_ctl);
    for (int i = 0; i < 3; ++i) fmrec_feed(&r, block, 4410, 44100, 2, 2);
    CHECK(r.f == NULL, "control file removed: recording stops");
    long data = header_data(wav, 44100);
    CHECK(data > mid && file_size(wav) == 44 + data, "after stop the header counts every byte written");
    char st[64] = {0};
    FILE* s = fopen(g_state, "r");
    if (s) { if (!fgets(st, sizeof st, s)) st[0] = 0; fclose(s); }
    CHECK(strncmp(st, "0 stopped", 9) == 0, "state says it stopped");

    char bad[160];
    snprintf(bad, sizeof bad, "%s/no/such/dir/x.wav", g_dir);
    ctl(bad);
    for (int i = 0; i < 3; ++i) fmrec_feed(&r, block, 4410, 44100, 2, 2);
    CHECK(r.f == NULL && r.refused, "a file that cannot be created is refused");
    remove(g_state);
    for (int i = 0; i < 6; ++i) fmrec_feed(&r, block, 4410, 44100, 2, 2);
    CHECK(file_size(g_state) < 0, "and not retried while the same request stands");
    remove(g_ctl);
    fmrec_feed(&r, block, 4410, 44100, 2, 2);
    fmrec_feed(&r, block, 4410, 44100, 2, 2);
    fmrec_feed(&r, block, 4410, 44100, 2, 2);
    CHECK(!r.refused, "the refusal clears when the request is withdrawn");

    char w3[128];
    snprintf(w3, sizeof w3, "%s/three.wav", g_dir);
    ctl(w3);
    for (int i = 0; i < 6; ++i) fmrec_feed(&r, block, 4410, 44100, 2, 3);
    CHECK(!r.f && file_size(w3) < 0, "an unknown sample format records nothing");
    remove(g_ctl);

    remove(wav); remove(g_state); rmdir(g_dir);
    printf("fmrec: %s\n", fails ? "FAILED" : "all passed");
    return fails != 0;
}
