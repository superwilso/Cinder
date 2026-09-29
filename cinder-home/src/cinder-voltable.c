/* cinder-voltable — install one of Sony's output volume tables. Setuid root.
 *
 * WHY: the wired volume curve is a table the codec driver loads at boot, and the one this model
 * gets is measurably poor (analysis/RE_volume_pop.md): 40 of the 120 UI steps do nothing —
 * vol 40..60 and vol 100..120 are both dead — and the live steps get coarser toward the top, which
 * is where the volume-change pop is worst.
 *
 * There is a better one. `ov_127x.tbl` is the NW-WM1A's own curve: no dead zones, the whole range
 * usable, and smaller steps at the top. BUT IT IS NOT ON A STOCK PLAYER: only the A50's own 1291
 * tables ship in /system/usr/share/audio_dac, and Cinder cannot ship Sony's files. The installer
 * copies a user's own copy into CINDER_DIR below — from the top of the drive, or from Wampy's
 * sound_settings — and only when its SHA-256 is Sony's (install_cinderhome.sh, 1f3b). With neither,
 * `wm1a` and `w1` fail with rc 4 (source missing) and the stock curve stays.
 * Measured, same instrument, on this unit while the file was there:
 *
 *     vol      0   20   40   60   80   90  100  110  120
 *     stock    4   80  100  100  148  188  228  228  228     <- two dead zones
 *     wm1a     4   44   84  124  164  184  204  224  228     <- monotonic
 *
 * (`ov_1280.tbl`, Walkman One's, measured IDENTICAL to stock — the model swap does not change the
 * volume curve. It is offered here only so that can be re-checked without a reinstall.)
 *
 * A FILE NAME DOES NOT SAY WHICH CURVE IT HOLDS. Walkman One copies its "gain mode" table over
 * /system/usr/share/audio_dac/ov_127x.tbl on every boot, and in its default mode that is the A50's
 * own curve renamed, so `wm1a` read by name loaded the stock curve on every Walkman One player.
 * Each key therefore names the table by CONTENT — the 8-byte sum/xor every table ends with — and
 * tries a fixed list of places, W1's untouched copies in /system/etc/.mod/gain included, taking the
 * first whose trailer matches. analysis/RE_walkmanone_installers.md, 2026-09-29.
 *
 * WHY A HELPER: the tables are applied by writing them into /proc/icx_audio_cxd3778gf_data/, which
 * is `-rw------- root root`. cinder-home and its launcher both run as uid 100. `load_sony_driver`
 * re-applies the stock table on EVERY boot, so this has to run every boot too — it is not an
 * install-time patch.
 *
 * SAFETY. The argument is a keyword from a fixed whitelist, never a path: the caller cannot name a
 * file, so it cannot ask this to write arbitrary bytes into a kernel node. Sources are fixed paths
 * on /system, writable only by root; Cinder's own directory is filled only by the installer, after
 * a SHA-256 check. Each source is opened O_NOFOLLOW, verified to be a regular file of the exact size
 * every one of these tables has and to end with the trailer of the table the key names, and copied
 * whole. Nothing about the destination comes from the caller.
 *
 * This changes what every volume step does. It does NOT raise the maximum — both curves reach the
 * same ceiling — but at a given number the WM1A curve is quieter through the mid range, so it is a
 * change to tell the user about, not to slip in.
 *
 * `eq G1..G5` is the one form that takes more than a keyword: five whole numbers, half-dB steps in
 * a fixed range, from which the helper computes a codec EQ table itself (src/codec_eq.h). Still no
 * path and no bytes from the caller; the base is a Sony tone table found and checked as above.
 *
 * Exit: 0 ok, 2 bad/absent argument, 3 not setuid root, 4 source unreadable/wrong, 5 write failed.
 */
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#include "codec_eq.h"

/* Exact sizes, measured on the device — the PCM and DSD tables are different shapes and both are
 * fixed. Checking the size is what stops this writing something that is not a volume table into a
 * kernel node. */
#define PCM_BYTES 84950
#define DSD_BYTES 13076
#define TONE_BYTES 2888
#define DST "/proc/icx_audio_cxd3778gf_data/ovt"
#define DST_DSD "/proc/icx_audio_cxd3778gf_data/ovt_dsd"
#define DST_TONE "/proc/icx_audio_cxd3778gf_data/tct"

/* Where tables live. SONY_DIR is the firmware's own; CINDER_DIR is filled by the installer (keep it
 * in step with VT_DIR in install_cinderhome.sh and the launcher's check); W1_N/W1_L are Walkman
 * One's pristine gain-mode copies, which W1 never rewrites. */
#define SONY_DIR "/system/usr/share/audio_dac/"
#define CINDER_DIR "/system/vendor/unknown321/usr/share/cinder/audio_dac/"
#define W1_N "/system/etc/.mod/gain/gain_n/"
#define W1_L "/system/etc/.mod/gain/gain_l/"

/* One table by content: its trailer, and the places a copy may sit, tried in order. The trailer is
 * the table's own 8-byte sum/xor (Wampy's cxd3778gf_table.h); md5 given for cross-reference with
 * Wampy's tunings/uniq.txt. Identical bytes under different names share one entry. */
struct src { const unsigned char tail[8]; const char *path[5]; };

/* A50 plain curve, md5 bb5ccae7. W1 ships it as gain_n's ov_127x (and in both _cew slots). */
static const struct src A50_PCM = { { 0x5e,0x55,0x35,0x00,0x96,0xb4,0x3d,0xab },
    { SONY_DIR "ov_1291.tbl", W1_N "ov_127x.tbl" } };
/* A50 plain DSD, md5 05858758 — also the NW-WM1A's ov_dsd_1280. */
static const struct src A50_DSD = { { 0x2a,0x06,0x04,0x00,0x2a,0x8a,0x00,0x00 },
    { SONY_DIR "ov_dsd_1291.tbl", SONY_DIR "ov_dsd_1280.tbl", W1_N "ov_dsd_127x.tbl" } };
/* A50 region (_cew) pair, md5 4ab93bdc / 741e6d91. W1 does not carry the PCM one. */
static const struct src CEW_PCM = { { 0x85,0x4a,0x39,0x00,0x04,0x66,0xb7,0x3c },
    { SONY_DIR "ov_1291_cew.tbl" } };
static const struct src CEW_DSD = { { 0x84,0xa2,0x03,0x00,0x40,0x3e,0x00,0x00 },
    { SONY_DIR "ov_dsd_1291_cew.tbl", SONY_DIR "ov_dsd_1280_cew.tbl" } };
/* NW-WM1A curve, md5 39a60adc / 142c8a33. */
static const struct src WM1A_PCM = { { 0xcc,0xd6,0x35,0x00,0xeb,0x50,0x88,0xdd },
    { SONY_DIR "ov_127x.tbl", CINDER_DIR "ov_127x.tbl", W1_L "ov_127x.tbl" } };
static const struct src WM1A_DSD = { { 0x76,0xbe,0x03,0x00,0xdc,0x0c,0x00,0x00 },
    { SONY_DIR "ov_dsd_127x.tbl", CINDER_DIR "ov_dsd_127x.tbl", W1_L "ov_dsd_127x.tbl" } };
/* Walkman One's ov_1280 (= NW-WM1A's), md5 5bf930c0. Its DSD partner is A50_DSD's bytes. */
static const struct src W1_PCM = { { 0xca,0xa1,0x38,0x00,0x18,0x32,0xf2,0x30 },
    { SONY_DIR "ov_1280.tbl", CINDER_DIR "ov_1280.tbl" } };
/* Tone control. A50 tc_1291 md5 05bcde3d; NW-WM1A tc_127x = tc_1280 = ZX300 tc_1288, md5 f678cb93. */
static const struct src TONE_A50 = { { 0x1b,0x91,0x01,0x00,0xa9,0x8f,0xf1,0x18 },
    { SONY_DIR "tc_1291.tbl", CINDER_DIR "tc_1291.tbl" } };
static const struct src TONE_WM1A = { { 0xf9,0x89,0x01,0x00,0xa9,0x8f,0xf1,0x18 },
    { SONY_DIR "tc_127x.tbl", SONY_DIR "tc_1280.tbl", CINDER_DIR "tc_127x.tbl", CINDER_DIR "tc_1280.tbl" } };

static const struct { const char *key; const struct src *pcm, *dsd; } TABLES[] = {
    { "stock", &A50_PCM,  &A50_DSD },
    { "w1",    &W1_PCM,   &A50_DSD },
    { "wm1a",  &WM1A_PCM, &WM1A_DSD },
    /* The region pair. Every model's volume table ships twice, plain and `_cew`, and `dacdat auto`
     * picks between them from the NVP `shp` flag (this unit reads 0x00000006, swid letter E).
     * Layout (Wampy's src/dac/cxd3778gf_table.h): sound effect off/on x 27 output tables x 121
     * steps x 13 one-byte register values, then an 8-byte checksum — exactly PCM_BYTES.
     *
     * `_cew` changes only DIGITAL stages. In the S-Master headphone tables `play` and `sdin2` each
     * move 17 codes at every step and `hpout` — PHV, the analogue attenuator — is IDENTICAL. So
     * `cinder-probe --volcurve`, which reads PHV, reports the two as the same curve whatever they
     * do to the signal; it cannot settle this. Wampy measured CEW2/KR3 units quieter at the jack.
     * Compare `eu` with `stock` at the jack. analysis/RE_volume_tables.md, amended 2026-09-13.
     *
     * Every other key here is a PLAIN table. On a unit that boots `_cew`, any of them removes the
     * region restriction. */
    { "eu",    &CEW_PCM,  &CEW_DSD },
};

/* Tone-control tables. Despite the name these are NOT Sony's Tone Control (that runs in software):
 * each is the codec's own five-biquad EQ, one block per amp x headphone type (src/codec_eq.h). In
 * every table Sony ships, the blocks for ordinary headphones are FLAT; the A50 and NW-WM1A tables
 * differ only in the blocks for Sony's NW500N noise-cancelling headphones. So `tone-w1`/`tone-wm1a`
 * change nothing with any other headphones — measured by decoding, 2026-09-29
 * (analysis/RE_codec_tone_table.md). They stay for NW500N owners and so existing configs keep
 * working. `eq` below is the key that puts something audible in the ordinary-headphone blocks. */
static const struct { const char *key; const struct src *tone; } TONE_TABLES[] = {
    { "tone-stock", &TONE_A50 },
    { "tone-w1",    &TONE_WM1A },
    { "tone-wm1a",  &TONE_WM1A },
};

/* Read one table file, if it is the table `tail` names. Returns 0 on success, 4 otherwise. */
static int read_one(const char *src, const unsigned char *tail, off_t want, unsigned char *buf)
{
    struct stat st;
    int in;
    ssize_t n;

    in = open(src, O_RDONLY | O_NOFOLLOW | O_CLOEXEC);
    if (in < 0)
        return 4;
    if (fstat(in, &st) != 0 || !S_ISREG(st.st_mode) || st.st_size != want) {
        close(in);
        return 4;
    }
    n = read(in, buf, (size_t)want);
    close(in);
    if (n != (ssize_t)want || memcmp(buf + want - 8, tail, 8) != 0)
        return 4;
    return 0;
}

/* Write a whole table into one proc node. Returns 0 on success, 5 otherwise. */
static int write_node(const char *dst, const unsigned char *buf, off_t want)
{
    int out = open(dst, O_WRONLY | O_CLOEXEC);
    ssize_t n;

    if (out < 0)
        return 5;
    n = write(out, buf, (size_t)want);
    close(out);
    return (n == (ssize_t)want) ? 0 : 5;
}

/* Find the table `t` in the first place that has a matching copy. Returns 0, or 4 when no place
 * has one. */
static int find_one(const struct src *t, off_t want, unsigned char *buf)
{
    unsigned i;

    for (i = 0; i < sizeof t->path / sizeof t->path[0] && t->path[i]; i++)
        if (read_one(t->path[i], t->tail, want, buf) == 0)
            return 0;
    return 4;
}

/* Install the table `t` from the first place that has a matching copy. Returns 0 on success, 4
 * when no place has one, 5 when the write itself failed. */
static int install_one(const struct src *t, const char *dst, off_t want)
{
    static unsigned char buf[PCM_BYTES];

    if (find_one(t, want, buf) != 0)
        return 4;
    return write_node(dst, buf, want);
}

/* One gain argument: a whole number of half-dB steps in the codec EQ's range, nothing else. */
static int parse_gain(const char *s, int *out)
{
    char *end;
    long v = strtol(s, &end, 10);

    if (end == s || *end != '\0' || v < CODEC_EQ_GAIN_MIN || v > CODEC_EQ_GAIN_MAX)
        return -1;
    *out = (int)v;
    return 0;
}

/* `eq G1 G2 G3 G4 G5`: the codec's own EQ (src/codec_eq.h). Built from whichever Sony tone table
 * this player carries — the A50's, or the NW-WM1A's that Walkman One ships in its place; they
 * differ only in the NW500N noise-cancelling blocks, which this keeps as found. The arguments are
 * five bounded integers, never bytes or paths: the helper computes the table itself. */
static int apply_eq(char **arg)
{
    static unsigned char buf[TONE_BYTES];
    int g[CODEC_EQ_BANDS], i;

    for (i = 0; i < CODEC_EQ_BANDS; i++)
        if (parse_gain(arg[i], &g[i]) != 0) {
            fprintf(stderr, "cinder-voltable: eq gain '%s' is not a whole number in %d..%d\n",
                    arg[i], CODEC_EQ_GAIN_MIN, CODEC_EQ_GAIN_MAX);
            return 2;
        }
    if (find_one(&TONE_A50, TONE_BYTES, buf) != 0 && find_one(&TONE_WM1A, TONE_BYTES, buf) != 0) {
        fprintf(stderr, "cinder-voltable: eq failed (4): no Sony tone table found\n");
        return 4;
    }
    codec_eq_table(buf, g, buf);
    if (write_node(DST_TONE, buf, TONE_BYTES) != 0) {
        fprintf(stderr, "cinder-voltable: eq failed (5)\n");
        return 5;
    }
    fprintf(stderr, "cinder-voltable: eq %d %d %d %d %d applied\n", g[0], g[1], g[2], g[3], g[4]);
    return 0;
}

int main(int argc, char **argv)
{
    unsigned i;

    /* Regain root before anything else — a setuid binary starts with the real uid still the
     * caller's, and these proc nodes are root-only. Same rule as the other helpers. */
    if (setuid(0) != 0 || geteuid() != 0) {
        fprintf(stderr, "cinder-voltable: not root (setuid bit lost?)\n");
        return 3;
    }
    if (argc == 2 + CODEC_EQ_BANDS && strcmp(argv[1], "eq") == 0)
        return apply_eq(argv + 2);
    if (argc != 2) {
        fprintf(stderr, "usage: cinder-voltable stock|w1|wm1a|eu"
                        " | tone-stock|tone-w1|tone-wm1a | eq G1 G2 G3 G4 G5\n");
        return 2;
    }
    for (i = 0; i < sizeof TONE_TABLES / sizeof TONE_TABLES[0]; i++) {
        if (strcmp(argv[1], TONE_TABLES[i].key) != 0)
            continue;
        int rc = install_one(TONE_TABLES[i].tone, DST_TONE, TONE_BYTES);
        if (rc != 0) {
            fprintf(stderr, "cinder-voltable: %s failed (%d)\n", TONE_TABLES[i].key, rc);
            return rc;
        }
        fprintf(stderr, "cinder-voltable: %s applied\n", TONE_TABLES[i].key);
        return 0;
    }
    for (i = 0; i < sizeof TABLES / sizeof TABLES[0]; i++) {
        if (strcmp(argv[1], TABLES[i].key) != 0)
            continue;
        int rc = install_one(TABLES[i].pcm, DST, PCM_BYTES);
        if (rc != 0) {
            fprintf(stderr, "cinder-voltable: %s PCM table failed (%d)\n", TABLES[i].key, rc);
            return rc;
        }
        /* The DSD curve is a separate table and the WM1A has its own; a failure here is worth
         * reporting but must not undo the PCM one, which is the part that matters. */
        if (install_one(TABLES[i].dsd, DST_DSD, DSD_BYTES) != 0)
            fprintf(stderr, "cinder-voltable: %s DSD table failed (PCM applied)\n", TABLES[i].key);
        fprintf(stderr, "cinder-voltable: %s applied\n", TABLES[i].key);
        return 0;
    }
    fprintf(stderr, "cinder-voltable: unknown table '%s'\n", argv[1]);
    return 2;
}
