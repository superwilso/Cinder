/* tz_offset_selftest.c — the offset `cinder-wifi timesync` keeps (src/tz_offset.h), off the device.
 * Run by tools/test_wifi_install.sh. */
#include <stdio.h>
#include "tz_offset.h"

static int fails;
#define CHECK(c, what) do { if (c) printf("  ok   %s\n", what); else { printf("  FAIL %s\n", what); ++fails; } } while (0)

int main(void)
{
    puts("== tz_offset");
    CHECK(tz_offset(0, 0, 0) == 0, "a clock on UTC stays on UTC");
    CHECK(tz_offset(3600 - 200, 0, 0) == 3600, "UTC+1, 200 s slow: corrected, zone kept");
    CHECK(tz_offset(3600 + 400, 0, 0) == 3600, "UTC+1, 400 s fast: corrected, zone kept");
    CHECK(tz_offset(-5 * 3600 - 10, 0, 0) == -5 * 3600, "UTC-5 a little fast");
    CHECK(tz_offset(5 * 3600 + 1800 + 30, 0, 0) == 5 * 3600 + 1800, "UTC+5:30 (India)");
    CHECK(tz_offset(5 * 3600 + 2700 - 60, 0, 0) == 5 * 3600 + 2700, "UTC+5:45 (Nepal)");
    CHECK(tz_offset(14 * 3600, 0, 0) == 14 * 3600, "UTC+14 is a real zone");
    CHECK(tz_offset(-26L * 365 * 86400, 3600, 1) == 3600, "a flat RTC (2000) uses the saved offset");
    CHECK(tz_offset(-26L * 365 * 86400, 0, 0) == 0, "a flat RTC and nothing saved: UTC");
    CHECK(tz_offset(15 * 3600, -3600, 1) == -3600, "past UTC+14: the saved offset");
    printf("%d failed\n", fails);
    return fails != 0;
}
