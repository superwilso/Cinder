/* cinder-voltable — install one of Sony's output volume tables. Setuid root.
 *
 * WHY: the wired volume curve is a table the codec driver loads at boot, and the one this model
 * gets is measurably poor (analysis/RE_volume_pop.md): 40 of the 120 UI steps do nothing —
 * vol 40..60 and vol 100..120 are both dead — and the live steps get coarser toward the top, which
 * is where the volume-change pop is worst.
 *
 * Sony ships a better one on every stock device. `ov_127x.tbl` is the NW-WM1A's own curve: no dead
 * zones, the whole range usable, and smaller steps at the top. Measured, same instrument, on this
 * unit:
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
 * file, so it cannot ask this to write arbitrary bytes into a kernel node. The source is opened
 * O_NOFOLLOW under a hardcoded directory, verified to be a regular file of the exact size every one
 * of these tables has, and copied whole. Nothing about the destination comes from the caller.
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

static const struct { const char *key, *pcm, *dsd; } TABLES[] = {
    { "stock", "/system/usr/share/audio_dac/ov_1291.tbl", "/system/usr/share/audio_dac/ov_dsd_1291.tbl" },
    { "w1",    "/system/usr/share/audio_dac/ov_1280.tbl", "/system/usr/share/audio_dac/ov_dsd_1280.tbl" },
    { "wm1a",  "/system/usr/share/audio_dac/ov_127x.tbl", "/system/usr/share/audio_dac/ov_dsd_127x.tbl" },
    /* The region pair. Every model's volume table ships twice, plain and `_cew`, and `dacdat auto`
     * picks between them from the NVP `shp` flag (this unit reads 0x00000006, swid letter E).
     * `_cew` is uniformly the QUIETER of the two: the tables differ in 7576 bytes, in a repeating
     * 13-byte record, and at every difference the `_cew` value is lower (e.g. 0xb3ef -> 0xa200,
     * about -10%). So `_cew` is the region-restricted curve and the plain file is the unrestricted
     * one.
     *
     * WHICH ONE THIS DEVICE BOOTS WITH IS NOT ESTABLISHED. /proc/.../ovt is write-only (a read
     * returns nothing), and dacdat reaches its path strings PC-relatively rather than through a
     * literal pool, so the selection was not read out of it. Settle it by measurement, not by
     * assumption: `cinder-probe --volcurve` sweeps the volume and reads the analogue attenuator
     * (PHV_L) at each step, which is the curve. Apply `eu`, sweep, apply `stock`, sweep, compare.
     * If the two sweeps differ, whichever matches the untouched boot state is what dacdat chose. */
    { "eu",    "/system/usr/share/audio_dac/ov_1291_cew.tbl", "/system/usr/share/audio_dac/ov_dsd_1291_cew.tbl" },
};

/* Tone-control tables — the other half of what W1 calls a "sound signature", and the half nobody
 * had wired. Sony loads one of these at every boot alongside the volume table, into its own proc
 * node. Unlike the volume tables these have NO `_cew` variant, so tone is not region-restricted.
 * Kept as separate keys rather than folded into the entries above, so that applying a volume curve
 * does not silently also change tone. */
static const struct { const char *key, *tone; } TONE_TABLES[] = {
    { "tone-stock", "/system/usr/share/audio_dac/tc_1291.tbl" },
    { "tone-w1",    "/system/usr/share/audio_dac/tc_1280.tbl" },
    { "tone-wm1a",  "/system/usr/share/audio_dac/tc_127x.tbl" },
};

/* Copy one table into one proc node. Returns 0 on success. */
static int install_one(const char *src, const char *dst, off_t want)
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
