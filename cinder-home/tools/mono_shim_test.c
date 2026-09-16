/* mono_shim_test.c — drives libcinder_mono.so through LD_PRELOAD the way SoundServiceFw would.
 *
 * mono_selftest proves the sums; this proves the INTERPOSITION: that the preloaded write/send/
 * connect/close and snd_pcm_* really are the ones a program calls, that the real ones are reached
 * underneath, and that nothing outside the watched socket changes. Run it with argv[1] =
 * "SoundServiceFw" (the shim only wakes up in a process whose command line says that) and a second
 * run with any other name to prove it stays asleep. tools/test_mono_shim.sh builds it for the host
 * and for the device's glibc under qemu.
 *
 * The libasound half links against libfakeasound.so (built from this file with -DFAKE_ASOUND),
 * which records what reached "the codec".
 */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <time.h>
#include <unistd.h>

#ifdef FAKE_WAMPY
/* ── stands in for Wampy's libsound_service_fw.so: proves the chain loaded it ── */
__attribute__((constructor)) static void fake_wampy(void)
{
    const char* d = getenv("MONO_TEST_DIR");
    char p[512];
    if (!d) return;
    snprintf(p, sizeof p, "%s/wampy_loaded", d);
    int fd = open(p, O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd >= 0) close(fd);
}
#elif defined(FAKE_ASOUND)
/* ── the fake libasound ── */
static unsigned char g_last[4096];
static unsigned long g_last_frames;
int snd_pcm_hw_params_set_format(void* pcm, void* params, int fmt) { (void)pcm; (void)params; (void)fmt; return 0; }
int snd_pcm_hw_params_set_channels(void* pcm, void* params, unsigned int ch) { (void)pcm; (void)params; (void)ch; return 0; }
int snd_pcm_close(void* pcm) { (void)pcm; return 0; }
long snd_pcm_writei(void* pcm, const void* buf, unsigned long frames)
{
    (void)pcm;
    size_t n = frames * 4 < sizeof g_last ? frames * 4 : sizeof g_last;
    memcpy(g_last, buf, n);
    g_last_frames = frames;
    return (long)frames;
}
const unsigned char* fake_last(void) { return g_last; }
#else

int snd_pcm_hw_params_set_format(void* pcm, void* params, int fmt);
int snd_pcm_hw_params_set_channels(void* pcm, void* params, unsigned int ch);
int snd_pcm_close(void* pcm);
long snd_pcm_writei(void* pcm, const void* buf, unsigned long frames);
const unsigned char* fake_last(void);

static int fails = 0;
static void check(int ok, const char* what)
{
    printf("%s  %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok) fails++;
}
static int16_t get16(const unsigned char* p) { return (int16_t)(p[0] | (p[1] << 8)); }
static void put16(unsigned char* p, int16_t v) { p[0] = (unsigned char)(v & 0xFF); p[1] = (unsigned char)((v >> 8) & 0xFF); }
static void put32(unsigned char* p, uint32_t v) { for (int i = 0; i < 4; i++) p[i] = (unsigned char)(v >> (8 * i)); }
static void nap(int ms) { struct timespec t = { 0, ms * 1000000L }; nanosleep(&t, NULL); }

static void flag(const char* dir, int on)
{
    char p[512];
    snprintf(p, sizeof p, "%s/cinder_mono", dir);
    if (on) { int fd = open(p, O_WRONLY | O_CREAT, 0644); if (fd >= 0) close(fd); }
    else unlink(p);
    nap(300);   /* the shim re-reads the flag at most every 250 ms */
}

/* Send `n` bytes through the client and read the same number back off the server end. */
static int roundtrip(int cli, int srv, const unsigned char* buf, size_t n, unsigned char* got, int use_send)
{
    size_t off = 0;
    while (off < n) {
        ssize_t w = use_send ? send(cli, buf + off, n - off, 0) : write(cli, buf + off, n - off);
        if (w <= 0) return -1;
        off += (size_t)w;
    }
    size_t r = 0;
    while (r < n) {
        ssize_t k = read(srv, got + r, n - r);
        if (k <= 0) return -1;
        r += (size_t)k;
    }
    return 0;
}

int main(int argc, char** argv)
{
    /* MONO_EXPECT=inactive: the script has put the shim in safe mode or switched its hooks off. */
    const char* ex = getenv("MONO_EXPECT");
    const int expect_active = argc > 1 && strcmp(argv[1], "SoundServiceFw") == 0
                           && !(ex && strcmp(ex, "inactive") == 0);
    const char* dir = getenv("MONO_TEST_DIR");
    if (!dir) { fprintf(stderr, "MONO_TEST_DIR not set\n"); return 2; }
    flag(dir, 0);
    int ok;

    /* MONO_TEST_NO_BT: on the player itself, where binding the transmitter's name would collide with
     * the real service — and a connect to the real one would put this test's bytes on the air. */
    if (!getenv("MONO_TEST_NO_BT")) {
    /* ── Bluetooth ── */
    static const char name[] = "pst::services::bttransmitterservice";
    struct sockaddr_un a;
    memset(&a, 0, sizeof a);
    a.sun_family = AF_UNIX;
    memcpy(a.sun_path + 1, name, sizeof name - 1);
    const socklen_t alen = 110;   /* Sony's addrlen, NUL-padded */
    int ls = socket(AF_UNIX, SOCK_STREAM, 0);
    if (bind(ls, (struct sockaddr*)&a, alen) != 0 || listen(ls, 1) != 0) { perror("bind/listen"); return 2; }
    int cli = socket(AF_UNIX, SOCK_STREAM, 0);
    if (connect(cli, (struct sockaddr*)&a, alen) != 0) { perror("connect"); return 2; }
    int srv = accept(ls, NULL, NULL);

    unsigned char hs[36];
    memset(hs, 0, sizeof hs);
    put32(hs, 1); put32(hs + 4, 28); put32(hs + 8 + 4, 2); put32(hs + 8 + 24, 44100);
    unsigned char got[4096];

    flag(dir, 1);
    check(roundtrip(cli, srv, hs, sizeof hs, got, 0) == 0 && memcmp(got, hs, sizeof hs) == 0,
          "bt: the handshake arrives unchanged, mono ON");
    unsigned char pcm[400];
    for (int i = 0; i < 100; i++) { put16(pcm + i * 4, (int16_t)(1000 + i)); put16(pcm + i * 4 + 2, (int16_t)(-3000 - i)); }
    ok = roundtrip(cli, srv, pcm, sizeof pcm, got, 0) == 0;
    int summed = ok, untouched = ok;
    for (int i = 0; ok && i < 100; i++) {
        const int16_t m = (int16_t)(((1000 + i) + (-3000 - i)) / 2);
        if (get16(got + i * 4) != m || get16(got + i * 4 + 2) != m) summed = 0;
        if (memcmp(got + i * 4, pcm + i * 4, 4) != 0) untouched = 0;
    }
    if (expect_active) check(summed, "bt: PCM written with write() arrives summed");
    else check(untouched, "bt: in any other process the PCM arrives untouched");
    ok = roundtrip(cli, srv, pcm, 6, got, 1) == 0 && roundtrip(cli, srv, pcm + 6, sizeof pcm - 6, got + 6, 1) == 0;
    if (expect_active)
        check(ok && get16(got) == -1000 && get16(got + 4) == 1001 && get16(got + 6) == -3001
                 && get16(got + 8) == -1000,
              "bt: send() split mid-frame: whole frames summed, the split one whole-original");
    flag(dir, 0);
    ok = roundtrip(cli, srv, pcm, sizeof pcm, got, 0) == 0;
    check(ok && memcmp(got, pcm, sizeof pcm) == 0, "bt: mono OFF: PCM arrives untouched");

    /* An fd that never connected to the transmitter is never touched. */
    int pair[2];
    socketpair(AF_UNIX, SOCK_STREAM, 0, pair);
    flag(dir, 1);
    ok = roundtrip(pair[0], pair[1], pcm, sizeof pcm, got, 0) == 0;
    check(ok && memcmp(got, pcm, sizeof pcm) == 0, "bt: any other socket is untouched with mono ON");
    close(cli); close(srv); close(ls); close(pair[0]); close(pair[1]);
    }

    /* ── the jack ── */
    void* pcmh = (void*)0x1234;
    snd_pcm_hw_params_set_format(pcmh, NULL, 2 /* S16_LE */);
    snd_pcm_hw_params_set_channels(pcmh, NULL, 2);
    unsigned char pcm[400];
    for (int i = 0; i < 100; i++) { put16(pcm + i * 4, (int16_t)(1000 + i)); put16(pcm + i * 4 + 2, (int16_t)(-3000 - i)); }
    flag(dir, 1);
    long wr = snd_pcm_writei(pcmh, pcm, 100);
    const unsigned char* last = fake_last();
    if (expect_active)
        check(wr == 100 && get16(last) == -1000 && get16(last + 2) == -1000, "jack: snd_pcm_writei reaches the codec summed");
    else
        check(wr == 100 && memcmp(last, pcm, 400) == 0, "jack: in any other process it reaches the codec untouched");
    check(memcmp(pcm, (unsigned char[]){ 0xE8, 0x03 }, 2) == 0, "jack: the caller's own buffer is never written to");
    flag(dir, 0);
    snd_pcm_writei(pcmh, pcm, 100);
    check(memcmp(fake_last(), pcm, 400) == 0, "jack: mono OFF: untouched");
    snd_pcm_close(pcmh);

    char alive[512];
    snprintf(alive, sizeof alive, "%s/cinder_mono_shim", dir);
    struct stat sb;
    check((stat(alive, &sb) == 0) == expect_active, expect_active ? "the shim announced itself" : "…and did not announce itself");
    unlink(alive);

    printf(fails ? "mono_shim_test (%s): %d FAILED\n" : "mono_shim_test (%s): all passed\n",
           expect_active ? "SoundServiceFw" : "other process", fails);
    return fails ? 1 : 0;
}
#endif
