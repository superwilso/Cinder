/* tz_offset.h — the time-zone offset `cinder-wifi timesync` keeps (src/cinder-wifi.c).
 *
 * The player's clock holds LOCAL wall time read as if it were UTC (Cinder has no time zone). Given
 * `measured` = clock − UTC in seconds, the offset is that rounded to the nearest 15 minutes: every
 * zone on Earth is a multiple of 15 minutes, so drift of up to ±7 minutes is corrected and the
 * zone is kept. Outside UTC−12..UTC+14 the clock was too far gone to measure (a flat RTC), so the
 * last good offset is used, or 0 without one. Tested by tools/tz_offset_selftest.c. */
#ifndef CINDER_TZ_OFFSET_H
#define CINDER_TZ_OFFSET_H
static inline long tz_offset(long measured, long saved, int have_saved)
{
    long off = (measured >= 0 ? measured + 450 : measured - 450) / 900 * 900;
    if (off < -12 * 3600 || off > 14 * 3600) return have_saved ? saved : 0;
    return off;
}
#endif
