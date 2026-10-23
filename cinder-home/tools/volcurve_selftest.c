/* volcurve_selftest.c — the custom volume curve (src/vol_curve.h), off the device.
 * Run by tools/test_volcurve.sh. Checks: the points interpolate and are refused when they go down
 * or out of range; every step of the built table is a whole row of the base, in every output table;
 * and, given Sony's ov_1291.tbl as argv[1], the identity curve rebuilds it byte for byte, which is
 * what proves the trailer is the one the kernel module checks. */
#include <stdio.h>
#include <stdlib.h>
#include "vol_curve.h"

static int fails;
#define CHECK(c, what) do { if (c) printf("  ok   %s\n", what); else { printf("  FAIL %s\n", what); ++fails; } } while (0)

static unsigned char base[VOL_PCM_BYTES], out[VOL_PCM_BYTES];

int main(int argc, char **argv)
{
    unsigned char map[VOL_STEPS];
    int t, s, ok;

    puts("== volcurve");
    const int line[VOL_CURVE_POINTS] = {0, 20, 40, 60, 80, 100, 120};
    ok = vol_curve_map(line, map) == 0;
    for (s = 0; s < VOL_STEPS; s++) ok &= map[s] == s;
    CHECK(ok, "the straight line plays every step as itself");

    const int lift[VOL_CURVE_POINTS] = {0, 30, 50, 70, 85, 100, 100};
    ok = vol_curve_map(lift, map) == 0 && map[10] == 15 && map[20] == 30 && map[110] == 100
         && map[120] == 100;
    for (s = 1; s < VOL_STEPS; s++) ok &= map[s] >= map[s - 1];
    CHECK(ok, "points in between are a straight line, and never go down");

    const int down[VOL_CURVE_POINTS] = {0, 30, 20, 60, 80, 100, 120};
    const int high[VOL_CURVE_POINTS] = {0, 20, 40, 60, 80, 100, 121};
    const int neg[VOL_CURVE_POINTS] = {-1, 20, 40, 60, 80, 100, 120};
    CHECK(vol_curve_map(down, map) != 0, "a curve that goes down is refused");
    CHECK(vol_curve_map(high, map) != 0 && vol_curve_map(neg, map) != 0,
          "a point outside 0..120 is refused");

    /* A base whose every row says which table and step it came from. */
    for (t = 0; t < VOL_TABLES; t++)
        for (s = 0; s < VOL_STEPS; s++) {
            unsigned char *r = base + (t * VOL_STEPS + s) * VOL_ROW;
            r[0] = (unsigned char)t; r[1] = (unsigned char)s;
            memset(r + 2, (t * 7 + s) & 0xff, VOL_ROW - 2);
        }
    vol_trailer(base, VOL_PCM_BYTES);
    vol_curve_map(lift, map);
    vol_curve_table(base, map, out);
    ok = 1;
    for (t = 0; t < VOL_TABLES; t++)
        for (s = 0; s < VOL_STEPS; s++)
            ok &= memcmp(out + (t * VOL_STEPS + s) * VOL_ROW,
                         base + (t * VOL_STEPS + map[s]) * VOL_ROW, VOL_ROW) == 0;
    CHECK(ok, "every step is the base table's own row, in all 54 output tables");

    if (argc > 1) {
        FILE *f = fopen(argv[1], "rb");
        size_t n = f ? fread(base, 1, sizeof base, f) : 0;
        if (f) fclose(f);
        if (n == VOL_PCM_BYTES) {
            vol_curve_map(line, map);
            vol_curve_table(base, map, out);
            CHECK(memcmp(out, base, VOL_PCM_BYTES) == 0,
                  "Sony's table rebuilt by the straight line is byte-identical (trailer included)");
        } else {
            CHECK(0, "argv[1] is an 84950-byte Sony table");
        }
    } else {
        puts("  skip Sony's own table (pass ov_1291.tbl to check the trailer)");
    }
    printf("volcurve: %s\n", fails ? "FAILED" : "all passed");
    return fails != 0;
}
