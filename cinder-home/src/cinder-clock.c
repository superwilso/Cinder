/* cinder-clock — tiny setuid-root helper: set the system clock and the RTC.
 *
 * WHY THIS EXISTS.
 * Cinder had no way to set the time at all. The clock on the status bar was read-only, and if the
 * battery ever went flat enough to lose the RTC there was no route back to a correct time short of
 * booting stock. Sony's own player can do it — HgrmMediaPlayerApp carries
 * `dmp/app/HgrmMediaPlayer/src/model/date_time/DateTime.cpp` with `resSetDateTimeResult(bool)` and
 * `OnResSetDateTimeResult(bool)` — but that is a request/response inside the app's own model, and
 * NOTHING in vendor/sony/lib exposes a clock setter: a sweep of every library's demangled `virtual`
 * prototypes (the .rodata trick from reference_sony_service_signatures) turns up TimerService's
 * OnResume and nothing else time-shaped. No service, no IPC to borrow. So we go to the kernel.
 *
 * WHY IT IS SETUID. cinder-home runs as uid 100 (system) — measured, not assumed — and both
 * settimeofday(2) and the RTC_SET_TIME ioctl require CAP_SYS_TIME, which appmgr does not grant it.
 * Same wall, and the same answer, as cinder-umount and cinder-power: static musl, setuid root
 * (chmod 4755, and chmod AFTER chown), one hard-coded verb, no paths or flags from the caller.
 *
 * BOTH CLOCKS, IN THIS ORDER. settimeofday moves the running system clock; the RTC ioctl makes it
 * survive a power cycle. Doing only the first means the correct time evaporates at the next boot,
 * which is exactly the failure this helper exists to end. The RTC write is best-effort: if it
 * fails, the system clock is still right for this session and that is strictly better than
 * refusing, so it is reported in the exit code rather than treated as fatal.
 *
 * 2038. time_t is 32-bit on this device (armv7, glibc 2.23) and the signed wrap is at
 * 2038-01-19 03:14:07 UTC. Setting a date past that would not produce a far-future clock, it would
 * produce a NEGATIVE one — 1901 — and every duration in the player would go strange. The accepted
 * range therefore stops a clear two weeks short of the wrap, and the UI clamps to the same bound
 * so the two cannot disagree.
 */
#include <sys/time.h>
#include <sys/ioctl.h>
#include <linux/rtc.h>
#include <fcntl.h>
#include <unistd.h>
#include <time.h>

/* 2001-01-01 00:00:00 UTC. Below this is not a clock the user meant to set; it is a failed parse
 * or a flat RTC, and accepting it would let a stray value quietly rewrite a good clock. */
#define EPOCH_MIN 978307200L
/* 2038-01-01 00:00:00 UTC — clear of the 32-bit signed wrap at 2038-01-19. See the note above. */
#define EPOCH_MAX 2145916800L

int main(int argc, char **argv)
{
    struct timeval tv;
    struct tm utc;
    struct rtc_time rt;
    const char *p;
    /* 64-BIT ACCUMULATOR, DELIBERATELY. `long` is 32 bits on this device (armv7), so accumulating
     * into one and then testing `secs > EPOCH_MAX` tests a value that has ALREADY wrapped — the
     * check passes and a nonsense date gets written. Measured 2026-08-17: `cinder-clock set
     * 9999999999` sailed past the range check and set the clock to 2014. The range test is only
     * meaningful in a type that cannot overflow within it. */
    long long secs = 0;
    int digits = 0, fd, rtc_ok;

    /* One verb, one argument. A setuid-root binary takes nothing else. */
    if (argc != 3) return 2;
    if (argv[1][0] != 's' || argv[1][1] != 'e' || argv[1][2] != 't' || argv[1][3] != 0) return 2;

    /* Parse by hand rather than with strtol: this is the ONLY caller-supplied value in the
     * program, so it is worth being explicit that nothing but ASCII digits is accepted — no sign,
     * no whitespace, no 0x, no trailing junk — and that the accumulator cannot run away. */
    for (p = argv[2]; *p; ++p) {
        if (*p < '0' || *p > '9') return 2;
        if (++digits > 10) return 2;          /* EPOCH_MAX is 10 digits; nothing longer is real */
        secs = secs * 10 + (*p - '0');
        if (secs > EPOCH_MAX) return 2;       /* safe now: secs is 64-bit and cannot have wrapped */
    }
    if (digits == 0) return 2;                /* empty string */
    if (secs < EPOCH_MIN) return 2;

    /* Check before touching anything — the realistic failure is the setuid bit not surviving
     * install, and a half-applied clock change is worse than none. */
    if (geteuid() != 0) return 3;

    tv.tv_sec  = (time_t)secs;
    tv.tv_usec = 0;
    if (settimeofday(&tv, 0) != 0) return 4;

    /* Persist to the RTC so the time survives a power cycle. The RTC keeps UTC — gmtime, not
     * localtime — and struct rtc_time is struct tm's layout minus the trailing fields. */
    rtc_ok = 0;
    fd = open("/dev/rtc0", O_WRONLY);
    if (fd >= 0) {
        if (gmtime_r(&tv.tv_sec, &utc)) {
            rt.tm_sec   = utc.tm_sec;
            rt.tm_min   = utc.tm_min;
            rt.tm_hour  = utc.tm_hour;
            rt.tm_mday  = utc.tm_mday;
            rt.tm_mon   = utc.tm_mon;
            rt.tm_year  = utc.tm_year;
            rt.tm_wday  = utc.tm_wday;
            rt.tm_yday  = utc.tm_yday;
            rt.tm_isdst = 0;
            if (ioctl(fd, RTC_SET_TIME, &rt) == 0) rtc_ok = 1;
        }
        close(fd);
    }

    /* 0 = both clocks set. 5 = the system clock is right for this session but will not survive a
     * power cycle; the caller logs the difference rather than claiming success it did not get. */
    return rtc_ok ? 0 : 5;
}
