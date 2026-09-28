/* cinder-power — tiny setuid-root helper: power the device off or restart it, and the two
 * root-only CPU settings Cinder wants (scheduler granularity, a screen-off clock cap).
 *
 * WHY THIS EXISTS, AND WHY IT DOES NOT GO THROUGH SONY.
 * The obvious route is PowerMgrServiceClient::Reboot() / SetStatus(PowerOff), and Cinder shipped
 * that first. On device (2026-07-28) Reboot() **froze the player** and SetStatus(PowerOff) only
 * put it to sleep. The reason is in libpstcore.so: shutdown is a two-phase barrier across every
 * registered service —
 *
 *     OnPreShutdown -> "All services preshutdowned!" -> OnShutdown -> "All services shutdowned!"
 *                   -> android_reboot
 *
 * (see also libPowerService.so: "Power state transition is stopping! Check all services and reboot
 * the system..."). Cinder-home replaced the Qt Home app but does not speak that protocol, so it
 * never acknowledges its phase, the barrier never clears, and the request hangs forever holding
 * the UI thread. Sony's own power-off literally cannot complete while we are the Home app.
 *
 * So we take the kernel route instead: reboot(2). It needs CAP_SYS_BOOT, and cinder-home is
 * launched by appmgr with an EMPTY capability set — the same wall that made cinder-umount
 * necessary. Same solution, same shape: static musl, setuid root (chmod 4755), no configurable
 * behaviour beyond one of two hard-coded verbs.
 *
 * DURABILITY. /contents is vfat and cinder-home writes its settings and log there. We sync, then
 * best-effort remount every writable mount read-only (which is what actually flushes and marks a
 * vfat volume clean), then sync again. A remount can legitimately fail with EBUSY while
 * cinder-home still holds its log open; the preceding sync is what makes that survivable, and it
 * is strictly better than the forced power-off the user is doing today.
 */
#include <sys/reboot.h>
#include <sys/mount.h>
#include <fcntl.h>
#include <unistd.h>
#include <string.h>

/* The mounts worth flushing before the power is cut. /contents and /contents_ext hold the user's
 * music, settings and log; /data holds the launcher's state. Anything else is read-only or tmpfs. */
static const char *const kFlush[] = { "/contents", "/contents_ext", "/data", 0 };

/* THE CPU VERBS (2026-09-28, docs/AUDIT_2026-09-28_power_sound_w1.md P6/P7). Both write nodes that
 * only root can: cinder-home runs as `system` with no capabilities.
 *
 *   sched  Kernel-default CFS granularity. Walkman One's init.rc sets sched_latency,
 *          min_granularity and wakeup_granularity to 0.1 ms; the audio pipeline then took 23 and
 *          46 involuntary preemptions a second and ~3% more CPU than at the defaults (A/B, same
 *          track). The ALSA buffer is a full second, so finer slices buy nothing that reaches the
 *          DAC. On stock these already are the values, so the write is a no-op there.
 *   cap    scaling_max_freq = 1040 MHz. 598, 747.5 and 1040 MHz all run at 1150 mV; 1300 needs
 *          1300 mV (cpufreq_ptpod_freq_volt), so the cap removes only the costly step. Stage-1 early
 *          suspend pins exactly this clock anyway; the cap covers the minute before it starts.
 *   uncap  scaling_max_freq = cpuinfo_max_freq, read back from the kernel, never a literal.
 *
 * Every value is hard-coded or read from the kernel: the caller chooses a verb, never a number. */
static int put(const char *path, const char *val)
{
    int fd = open(path, O_WRONLY);
    if (fd < 0) return 0;
    const size_t n = strlen(val);
    const int ok = write(fd, val, n) == (ssize_t)n;
    close(fd);
    return ok;
}

#define CPUFREQ "/sys/devices/system/cpu/cpu0/cpufreq/"

static int cpu_verb(const char *verb)
{
    if (strcmp(verb, "sched") == 0) {
        int ok = put("/proc/sys/kernel/sched_latency_ns", "6000000");
        ok &= put("/proc/sys/kernel/sched_min_granularity_ns", "750000");
        ok &= put("/proc/sys/kernel/sched_wakeup_granularity_ns", "1000000");
        return ok ? 0 : 4;
    }
    if (strcmp(verb, "cap") == 0)
        return put(CPUFREQ "scaling_max_freq", "1040000") ? 0 : 4;
    if (strcmp(verb, "uncap") == 0) {
        char buf[16] = {0};
        int fd = open(CPUFREQ "cpuinfo_max_freq", O_RDONLY);
        if (fd < 0) return 4;
        const ssize_t n = read(fd, buf, sizeof buf - 1);
        close(fd);
        if (n <= 0) return 4;
        return put(CPUFREQ "scaling_max_freq", buf) ? 0 : 4;
    }
    return 2;
}

int main(int argc, char **argv)
{
    int restart;

    /* A fixed set of verbs. A setuid-root binary takes no paths, no flags and no numbers from its
     * caller — if it is not one of these, do nothing at all. */
    if (argc != 2) return 2;
    if      (strcmp(argv[1], "off")     == 0) restart = 0;
    else if (strcmp(argv[1], "restart") == 0) restart = 1;
    else {
        if (geteuid() != 0) return 3;
        return cpu_verb(argv[1]);
    }

    /* CHECK BEFORE TOUCHING ANYTHING. The realistic failure here is the setuid bit not surviving
     * install (a FAT stage, a chmod that did not take), and the remount below is only safe if the
     * reboot that follows it is certain. Without this, a lost setuid bit would leave the running
     * system with /contents and /data read-only and no reboot to clear it — settings writes and
     * the log would start failing and the cause would be invisible. Bail out first instead. */
    if (geteuid() != 0) return 3;

    sync();
    for (int i = 0; kFlush[i]; ++i)
        mount(0, kFlush[i], 0, MS_REMOUNT | MS_RDONLY, 0);   /* best-effort; EBUSY is survivable */
    sync();

    reboot(restart ? RB_AUTOBOOT : RB_POWER_OFF);

    /* Only reached if the kernel refused (no CAP_SYS_BOOT — i.e. the setuid bit was lost during
     * install). Report it so cinder-home can log a real cause instead of a silent no-op. */
    return 1;
}
