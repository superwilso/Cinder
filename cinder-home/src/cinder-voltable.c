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
 * WHY A HELPER: the tables are applied by writing them into /proc/icx_audio_cxd3778gf_data/, which
 * is `-rw------- root root`. cinder-home and its launcher both run as uid 100. `load_sony_driver`
 * re-applies the stock table on EVERY boot, so this has to run every boot too — it is not an
 * install-time patch.
 *
 * SAFETY. The argument is a keyword from a fixed whitelist, never a path: the caller cannot name a
 * file, so it cannot ask this to write arbitrary bytes into a kernel node. Sources are looked up by
 * fixed name in two fixed directories, both on /system and writable only by root; the one Cinder
 * owns is filled only by the installer, after the hash check. Each source is opened O_NOFOLLOW,
 * verified to be a regular file of the exact size every one of these tables has, and copied whole.
 * Nothing about the destination comes from the caller.
 *
 * This changes what every volume step does. It does NOT raise the maximum — both curves reach the
 * same ceiling — but at a given number the WM1A curve is quieter through the mid range, so it is a
 * change to tell the user about, not to slip in.
 *
 * Exit: 0 ok, 2 bad/absent argument, 3 not setuid root, 4 source unreadable/wrong, 5 write failed.
 */
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

/* Exact sizes, measured on the device — the PCM and DSD tables are different shapes and both are
 * fixed. Checking the size is what stops this writing something that is not a volume table into a
 * kernel node. */
#define PCM_BYTES 84950
#define DSD_BYTES 13076
#define TONE_BYTES 2888
#define DST "/proc/icx_audio_cxd3778gf_data/ovt"
#define DST_DSD "/proc/icx_audio_cxd3778gf_data/ovt_dsd"
#define DST_TONE "/proc/icx_audio_cxd3778gf_data/tct"

/* Where a table is looked for, in order: Sony's own directory, then Cinder's. Keep CINDER_DIR in
 * step with VT_DIR in install_cinderhome.sh and the launcher's check. */
#define SONY_DIR "/system/usr/share/audio_dac/"
#define CINDER_DIR "/system/vendor/unknown321/usr/share/cinder/audio_dac/"
static const char *const DIRS[] = { SONY_DIR, CINDER_DIR };

static const struct { const char *key, *pcm, *dsd; } TABLES[] = {
    { "stock", "ov_1291.tbl", "ov_dsd_1291.tbl" },
    { "w1",    "ov_1280.tbl", "ov_dsd_1280.tbl" },
    { "wm1a",  "ov_127x.tbl", "ov_dsd_127x.tbl" },
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
    { "eu",    "ov_1291_cew.tbl", "ov_dsd_1291_cew.tbl" },
};

/* Tone-control tables — the other half of what W1 calls a "sound signature", and the half nobody
 * had wired. Sony loads one of these at every boot alongside the volume table, into its own proc
 * node. Unlike the volume tables these have NO `_cew` variant, so tone is not region-restricted.
 * Kept as separate keys rather than folded into the entries above, so that applying a volume curve
 * does not silently also change tone. */
static const struct { const char *key, *tone; } TONE_TABLES[] = {
    { "tone-stock", "tc_1291.tbl" },
    { "tone-w1",    "tc_1280.tbl" },
    { "tone-wm1a",  "tc_127x.tbl" },
};

/* Copy one table file into one proc node. Returns 0 on success. */
static int copy_one(const char *src, const char *dst, off_t want)
{
    static char buf[PCM_BYTES];
    struct stat st;
    int in, out;
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
    if (n != (ssize_t)want)
        return 4;

    out = open(dst, O_WRONLY | O_CLOEXEC);
    if (out < 0)
        return 5;
    n = write(out, buf, (size_t)want);
    close(out);
    return (n == (ssize_t)want) ? 0 : 5;
}

/* Install the table called `name` from the first directory that has a valid copy. Returns 0 on
 * success, 4 when no directory has one, 5 when the write itself failed. */
static int install_one(const char *name, const char *dst, off_t want)
{
    char src[160];
    unsigned i;

    for (i = 0; i < sizeof DIRS / sizeof DIRS[0]; i++) {
        if (snprintf(src, sizeof src, "%s%s", DIRS[i], name) >= (int)sizeof src)
            return 4;
        int rc = copy_one(src, dst, want);
        if (rc != 4)
            return rc;
    }
    return 4;
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
    if (argc != 2) {
        fprintf(stderr, "usage: cinder-voltable stock|w1|wm1a|eu"
                        " | tone-stock|tone-w1|tone-wm1a\n");
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
