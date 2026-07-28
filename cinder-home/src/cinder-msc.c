/* cinder-msc — setuid-root helper for the USB mass-storage handoff.
 *
 * WHY THIS EXISTS. USB-MSC has never worked from Cinder, and every earlier fix aimed at the wrong
 * layer (trigger ordering, the gadget enable-cycle, a remount on exit). MEASURED on device
 * 2026-07-28, the real cause is that BOTH privileged steps are root-only and cinder-home runs as
 * uid `system` with an empty capability set:
 *
 *   1. BINDING THE LUN. Writing "/emmc@contents" to f_mass_storage/lun/file makes the KERNEL open
 *      the backing block device using the CALLER's credentials. /dev/block/mmcblk0p29 is
 *      `brw------- root root`, so that open is EACCES, the sysfs write fails, and the node stays
 *      empty — the host enumerates a reader with NO MEDIUM. The sysfs node itself is 0666
 *      system:system, so the write LOOKS permitted and `echo` returns 0 either way. That is why
 *      this presented as a race for weeks: it never was one.
 *        As root: write, read back "/dev/block/mmcblk0p29", host sees the 55.9 GB volume. First try.
 *
 *   2. SWITCHING THE GADGET. `setprop sys.sony.config msc` is refused by the property service for
 *      uid system — the property simply stays "adb", so init's `on property:sys.sony.config=msc`
 *      block never ran AT ALL. Cinder's log line "init never reported sys.usb.state=mass_storage,
 *      adb" was reporting exactly this and was read as a timeout.
 *
 * So the whole sequence moves in here, where it runs in one root context. Same shape as
 * cinder-umount / cinder-gpunode / cinder-power: static musl, chmod 4755 root, and exactly two
 * hard-coded verbs with nothing caller-supplied.
 *
 * ORDER MATTERS AND IS NOT ARBITRARY. /contents must be unmounted BEFORE the gadget binds it, or
 * the host and the kernel have the same vfat mounted twice and the volume corrupts. On the way
 * back, the LUN must be released BEFORE /contents is remounted, for the same reason.
 */
#include <sys/mount.h>
#include <sys/stat.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#include <stdlib.h>
#include <errno.h>

#define LUN0 "/sys/class/android_usb/android0/f_mass_storage/lun/file"
#define LUN1 "/sys/class/android_usb/android0/f_mass_storage/lun1/file"
#define INTERNAL "/emmc@contents"           /* -> /dev/block/mmcblk0p29 */
#define SDCARD   "/dev/block/mmcblk1p1"

/* Every child command carries its own environment INLINE rather than relying on ours.
 *
 * Being setuid is why: exec of a setuid binary sets AT_SECURE and the loader strips
 * LD_LIBRARY_PATH, so children inherit an environment with no library path at all. `setprop` and
 * `getprop` are toolbox applets that link /system/lib/libcutils.so, and on this hybrid device
 * /system/lib is NOT on the loader's default search path — so they die with
 *     setprop: error while loading shared libraries: libcutils.so: cannot open shared object file
 * and the gadget switch silently never happens. MEASURED 2026-07-28, twice: first as "MSC behaves
 * differently from inside the app than from an adb shell" (the shell is not setuid and keeps its
 * environment), then again after a setenv() in main failed to reach the child.
 *
 * Setting it in the command string is what actually works, because the SHELL applies it to the
 * child regardless of what our own environ contains. Absolute paths for the same reason: PATH is
 * no more trustworthy here than LD_LIBRARY_PATH. */
#define ENVP "LD_LIBRARY_PATH=/system/lib:/vendor/lib "
#define SETPROP ENVP "/system/bin/setprop "
#define GETPROP ENVP "/system/bin/getprop "

static int write_node(const char *path, const char *val)
{
    FILE *f = fopen(path, "w");
    if (!f) return -1;
    fputs(val, f);
    /* fclose reports the deferred write error — the whole point here, since a failed sysfs write
     * is exactly the silent failure this helper exists to make visible. */
    return fclose(f) == 0 ? 0 : -1;
}

static int node_is_bound(const char *path)
{
    char buf[192] = {0};
    FILE *f = fopen(path, "r");
    if (!f) return 0;
    if (!fgets(buf, sizeof buf, f)) buf[0] = 0;
    fclose(f);
    for (char *p = buf; *p; ++p)
        if (*p != ' ' && *p != '\t' && *p != '\n') return 1;
    return 0;
}

static int is_mounted(const char *mp)
{
    char line[512];
    FILE *f = fopen("/proc/mounts", "r");
    if (!f) return 0;
    int hit = 0;
    size_t n = strlen(mp);
    while (fgets(line, sizeof line, f)) {
        const char *sp = strchr(line, ' ');
        if (!sp) continue;
        if (strncmp(sp + 1, mp, n) == 0 && sp[1 + n] == ' ') { hit = 1; break; }
    }
    fclose(f);
    return hit;
}

/* Unmount, lazily if a holder is still closing. The lazy path is safe here because the gadget only
 * needs the block device free, and the kernel drops the last reference once the holder closes. */
static int unmount_hard(const char *mp)
{
    for (int i = 0; i < 12; ++i) {
        if (!is_mounted(mp)) return 0;
        if (umount(mp) == 0) return 0;
        if (umount2(mp, MNT_DETACH) == 0) return 0;
        usleep(250000);
    }
    return is_mounted(mp) ? -1 : 0;
}

static void wait_prop(const char *prop, const char *want, int tenths)
{
    char cmd[160], buf[96];
    snprintf(cmd, sizeof cmd, GETPROP "%s", prop);
    for (int i = 0; i < tenths; ++i) {
        FILE *p = popen(cmd, "r");
        if (p) {
            buf[0] = 0;
            if (fgets(buf, sizeof buf, p)) buf[strcspn(buf, "\r\n")] = 0;
            pclose(p);
            if (strcmp(buf, want) == 0) return;
        }
        usleep(100000);
    }
}

static int msc_on(void)
{
    int rc = 0;

    /* 1) Free the volumes FIRST. Handing a still-mounted vfat to the host corrupts it. */
    if (unmount_hard("/contents") != 0) {
        fprintf(stderr, "cinder-msc: /contents will not unmount — aborting, nothing changed\n");
        return 1;                         /* leave the gadget alone; the UI stays usable */
    }
    int had_ext = is_mounted("/contents_ext");
    if (had_ext && unmount_hard("/contents_ext") != 0)
        fprintf(stderr, "cinder-msc: /contents_ext busy — SD card will not be offered\n");

    /* 2) Switch the gadget through init, so adbd, idProduct and the function list end up exactly
     *    as stock expects. As root the property is accepted and the block actually runs. */
    if (system(SETPROP "sys.sony.config msc") != 0)
        fprintf(stderr, "cinder-msc: setprop returned non-zero\n");
    /* init's block ends with `setprop sys.usb.state $sys.usb.config`, so that is the completion
     * signal. It also does enable 0 -> functions -> enable 1, which CLEARS lun/file — which is why
     * the LUN is bound after this wait and never before it. */
    wait_prop("sys.usb.state", "mass_storage,adb", 60);

    /* 3) Bind the media. The LUN is removable, so this is a media-INSERT: the host sees the disk
     *    appear with no re-enumeration. Verify the readback — a failed write is silent. */
    for (int i = 0; i < 8 && !node_is_bound(LUN0); ++i) {
        write_node(LUN0, INTERNAL);
        if (node_is_bound(LUN0)) break;
        usleep(250000);
    }
    if (!node_is_bound(LUN0)) {
        fprintf(stderr, "cinder-msc: LUN0 would not bind %s — host will see no medium\n", INTERNAL);
        rc = 1;
    }
    /* SD card, best effort: a missing or busy card must not fail the internal handoff. */
    if (had_ext && !is_mounted("/contents_ext")) {
        struct stat st;
        if (stat(SDCARD, &st) == 0) write_node(LUN1, SDCARD);
    }
    return rc;
}

static int msc_off(void)
{
    /* Release the media BEFORE remounting, or the host and the kernel hold the same vfat at once. */
    write_node(LUN0, "\n");
    write_node(LUN1, "\n");

    /* Back to the stock adb composition. init's adb block ends with `start mount_msc1`, which is
     * what remounts /contents — but mount_msc1 is `oneshot`, so if it has already run this boot
     * init will NOT run it again and the mount silently never happens. Hence the explicit retry. */
    system(SETPROP "sys.sony.config adb");
    for (int i = 0; i < 50 && !is_mounted("/contents"); ++i) usleep(100000);

    if (!is_mounted("/contents")) {
        system(SETPROP "ctl.start mount_msc1");
        for (int i = 0; i < 50 && !is_mounted("/contents"); ++i) usleep(100000);
    }
    /* Last resort: mount it ourselves. We are root; there is no reason to leave the user's library
     * missing because an init service would not re-run.
     *
     * THE OPTIONS ARE NOT OPTIONAL. vfat defaults to fmask/dmask 0022-or-0077, i.e. root-only —
     * and cinder-home is uid `system`, so a "successful" default mount hands back a library it
     * cannot read. That failure looks exactly like an empty library rather than a mount problem.
     * These are stock's own options, copied from /proc/mounts on a healthy boot; MS_NOEXEC and
     * MS_NOATIME match too (and /data and /contents being noexec is load-bearing elsewhere). */
    static const char kVfat[] = "fmask=0000,dmask=0000,allow_utime=0022,codepage=437,"
                                "iocharset=iso8859-1,shortname=mixed,utf8,errors=remount-ro";
    char opts[256];
    if (!is_mounted("/contents")) {
        snprintf(opts, sizeof opts, "%s,discard", kVfat);   /* internal is mounted with discard */
        if (mount(INTERNAL, "/contents", "vfat", MS_NOEXEC | MS_NOATIME, opts) != 0)
            fprintf(stderr, "cinder-msc: mount /contents failed errno=%d\n", errno);
    }
    /* The SD is worth retrying rather than reporting once: the gadget can still be holding the
     * block device for a moment after the LUN is cleared, and a missing SD library looks to the
     * user like their music vanished. Nothing mounts /contents_ext except a Sony service at boot,
     * so if we give up here it stays gone for the rest of the boot. */
    for (int i = 0; i < 12 && !is_mounted("/contents_ext"); ++i) {
        if (mount(SDCARD, "/contents_ext", "vfat", MS_NOEXEC | MS_NOATIME, kVfat) == 0) break;
        if (i == 11) fprintf(stderr, "cinder-msc: mount /contents_ext failed errno=%d\n", errno);
        usleep(250000);
    }

    if (!is_mounted("/contents")) {
        fprintf(stderr, "cinder-msc: /contents did NOT come back\n");
        return 1;
    }
    return 0;
}

int main(int argc, char **argv)
{
    if (argc != 2) return 2;
    if (geteuid() != 0) {
        fprintf(stderr, "cinder-msc: not root (setuid bit lost?) — refusing\n");
        return 3;
    }
    /* MAKE THE REAL UID ROOT TOO, BEFORE SPAWNING ANYTHING.
     *
     * cinder-home is uid `system` and this binary is setuid root, so we run with ruid=1000,
     * euid=0. The kernel sets AT_SECURE on any exec where those differ — and that propagates to
     * EVERY descendant, so the loader strips LD_LIBRARY_PATH from the shell we spawn and from
     * `setprop` under it. That is why the toolbox applets kept dying with
     *     libcutils.so: cannot open shared object file
     * even after the path was set in our own environment AND inlined into the command string:
     * the loader was discarding it at exec, not failing to receive it. The identical command from
     * an adb shell worked throughout, because that shell is not setuid.
     *
     * setuid(0) with euid already 0 sets real, effective and saved uid to root, AT_SECURE is not
     * set for our children, and the environment survives. Everything this helper does already
     * requires root, so there is nothing being widened here. */
    if (setuid(0) != 0)
        fprintf(stderr, "cinder-msc: setuid(0) failed errno=%d — children may lose their env\n", errno);
    setenv("LD_LIBRARY_PATH", "/system/lib:/vendor/lib", 1);
    if (strcmp(argv[1], "on")  == 0) return msc_on();
    if (strcmp(argv[1], "off") == 0) return msc_off();
    return 2;
}
