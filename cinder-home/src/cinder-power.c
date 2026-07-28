/* cinder-power — tiny setuid-root helper: power the device off, or restart it.
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
#include <unistd.h>
#include <string.h>

/* The mounts worth flushing before the power is cut. /contents and /contents_ext hold the user's
 * music, settings and log; /data holds the launcher's state. Anything else is read-only or tmpfs. */
static const char *const kFlush[] = { "/contents", "/contents_ext", "/data", 0 };

int main(int argc, char **argv)
{
    int restart;

    /* Exactly two accepted verbs. A setuid-root binary takes no paths, no flags and no numbers
     * from its caller — if it is not one of these two, do nothing at all. */
    if (argc != 2) return 2;
    if      (strcmp(argv[1], "off")     == 0) restart = 0;
    else if (strcmp(argv[1], "restart") == 0) restart = 1;
    else return 2;

    sync();
    for (int i = 0; kFlush[i]; ++i)
        mount(0, kFlush[i], 0, MS_REMOUNT | MS_RDONLY, 0);   /* best-effort; EBUSY is survivable */
    sync();

    reboot(restart ? RB_AUTOBOOT : RB_POWER_OFF);

    /* Only reached if the kernel refused (no CAP_SYS_BOOT — i.e. the setuid bit was lost during
     * install). Report it so cinder-home can log a real cause instead of a silent no-op. */
    return 1;
}
