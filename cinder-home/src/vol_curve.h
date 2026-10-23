/* vol_curve.h — a custom headphone volume curve, built from one of Sony's tables (`curve` in
 * cinder-voltable; Sound ▸ Advanced ▸ Volume curve).
 *
 * A Sony PCM volume table is sound effect off/on x 27 output tables x 121 volume steps x 13 one-byte
 * register values, then an 8-byte trailer (analysis/RE_volume_tables.md, amended 2026-09-13). A
 * custom curve here INVENTS NO REGISTER VALUE: step s of the new table is a whole 13-byte row of the
 * base table, the row `map[s]`. So every step plays exactly what some step of Sony's own curve
 * plays, in every output table, and the only thing the owner chooses is which step goes where. A
 * curve that goes down as the number goes up is refused, so the volume control can never run
 * backwards.
 *
 * The owner edits seven points, at volume 0, 20, 40 … 120; each names the base row that step plays.
 * The steps between are a straight line between the two points either side.
 *
 * TRAILER, worked out against ov_1291.tbl, ov_1291_cew.tbl and ov_dsd_1291.tbl (10-23): the byte
 * sum of everything before it, then the XOR of it read as little-endian 32-bit words (zero-padded),
 * each stored little-endian. The kernel module checks it, so it is recomputed after the remap.
 *
 * Header-only and plain C: cinder-voltable (setuid, static) and tools/volcurve_selftest.c share it.
 */
#ifndef CINDER_VOL_CURVE_H
#define CINDER_VOL_CURVE_H

#include <string.h>

#define VOL_STEPS 121
#define VOL_ROW 13
#define VOL_TABLES (2 * 27)
#define VOL_PCM_BYTES (VOL_TABLES * VOL_STEPS * VOL_ROW + 8)   /* 84950 */
#define VOL_CURVE_POINTS 7                                     /* volume 0, 20, … 120 */

/* Points -> the row each of the 121 steps plays. 0 on success, -1 if a point is outside 0..120 or
 * the curve goes down anywhere. */
static int vol_curve_map(const int pts[VOL_CURVE_POINTS], unsigned char map[VOL_STEPS])
{
    int k, s;

    for (k = 0; k < VOL_CURVE_POINTS; k++) {
        if (pts[k] < 0 || pts[k] > VOL_STEPS - 1)
            return -1;
        if (k > 0 && pts[k] < pts[k - 1])
            return -1;
    }
    for (s = 0; s < VOL_STEPS; s++) {
        const int seg = s / 20 < VOL_CURVE_POINTS - 1 ? s / 20 : VOL_CURVE_POINTS - 2;
        const int a = pts[seg], b = pts[seg + 1], off = s - seg * 20;
        map[s] = (unsigned char)(a + ((b - a) * off * 2 + 20) / 40);   /* rounded */
    }
    return 0;
}

static void vol_le32(unsigned char *p, unsigned v)
{
    p[0] = (unsigned char)v; p[1] = (unsigned char)(v >> 8);
    p[2] = (unsigned char)(v >> 16); p[3] = (unsigned char)(v >> 24);
}

/* Recompute the 8-byte trailer of a table `len` bytes long (trailer included). */
static void vol_trailer(unsigned char *t, unsigned len)
{
    unsigned sum = 0, x = 0, i;
    const unsigned n = len - 8;

    for (i = 0; i < n; i++)
        sum += t[i];
    for (i = 0; i < n; i += 4) {
        unsigned w = 0, k;
        for (k = 0; k < 4 && i + k < n; k++)
            w |= (unsigned)t[i + k] << (8 * k);
        x ^= w;
    }
    vol_le32(t + n, sum);
    vol_le32(t + n + 4, x);
}

/* out = base with every table's step s replaced by its row map[s]. `out` may not alias `base`. */
static void vol_curve_table(const unsigned char *base, const unsigned char map[VOL_STEPS],
                            unsigned char *out)
{
    int t, s;

    for (t = 0; t < VOL_TABLES; t++)
        for (s = 0; s < VOL_STEPS; s++)
            memcpy(out + (t * VOL_STEPS + s) * VOL_ROW, base + (t * VOL_STEPS + map[s]) * VOL_ROW,
                   VOL_ROW);
    vol_trailer(out, VOL_PCM_BYTES);
}

#endif
