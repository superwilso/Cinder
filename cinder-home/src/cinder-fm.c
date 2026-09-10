/* cinder-fm — tiny setuid-root helper (sibling of cinder-gpunode).
 *
 * WHY THIS EXISTS: the Si4708 FM tuner's registers are exposed by Sony's own driver through the
 * kernel's generic register monitor:
 *
 *     /proc/regmon/Si4708icx/target   write = select a register, read = the register-name table
 *     /proc/regmon/Si4708icx/value    read/write THAT register, over I2C, live
 *
 * Both ship `-rw------- root root`, and cinder-home runs as uid 100 with an empty capability set.
 * Widening them is the whole job — after that the shell talks to the chip with plain file I/O and
 * no further privilege, exactly as it talks to the GPU nodes after cinder-gpunode has run.
 *
 * WHAT THIS UNLOCKS (measured 2026-08-18, analysis/RE_fm_tuner.md): a real graded RSSI meter, the
 * chip's own hardware seek, and `STC` tune-complete — none of which Sony's TunerPlayerService can
 * deliver (`GetSignalLevel` returns a constant 1, `StartAutoTuning` is a stub). It replaces a
 * ~90-second audio-spectral band scan with one that takes a second.
 *
 * Like cinder-umount and cinder-gpunode this does exactly one hard-coded thing and takes no input
 * from the caller: no argv, no environment use, fixed path list.
 *
 * RACE-FREE BY CONSTRUCTION, the same way cinder-gpunode is: O_PATH|O_NOFOLLOW binds the name to
 * an inode without opening it, fstat verifies it is a regular file, and the chmod goes through
 * /proc/self/fd so it cannot land on anything other than the inode that was inspected. A symlink
 * planted at either path fails at open() rather than being followed.
 *
 * SECURITY TRADE-OFF (deliberate, and narrower than cinder-gpunode's): 0666 here lets any local
 * uid read and write the FM tuner's I2C registers. That is a radio receiver — it cannot reach the
 * filesystem, the network, or another device on the bus, because the driver's regmon node is bound
 * to this one chip at address 0x10. The registers a bad write could damage are the tuner's own
 * configuration, which the driver rewrites on its next Open(). On this single-user music player the
 * practical risk is low; it is still a loosening of kernel permissions, confined to two files.
 *
 * NOT the i2c bus itself. /dev/i2c-2 stays untouched — and it must, because that bus also carries
 * the bq24262 battery charger and the NFC controller.
 *
 * Exit code = number of nodes that could not be made accessible (0 = all good).
 */
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <unistd.h>

#ifndef O_PATH
#define O_PATH 010000000
#endif

static const char *const nodes[] = {
    "/proc/regmon/Si4708icx/target",
    "/proc/regmon/Si4708icx/value",
};

/* The tuner driver. SONY SHIPS THIS MODULE but stock never loads it: there is no `insmod` for it
 * anywhere in the stock ramdisk (checked against the extracted boot image — init.rc,
 * init.project.rc, init.hagoromo.rc, init.usbcfg*.rc). The only thing on this device that ever
 * loaded it was Wampy's `init.wampy.rc`, so a Cinder install on stock firmware — or on a device
 * Wampy has been removed from — has no /proc/regmon/Si4708icx at all, and every FM feature that
 * depends on it silently degrades to the ~90-second audio band scan.
 *
 * Loading it here rather than from an init script keeps Cinder out of the boot image: the ramdisk
 * is regenerated from the boot partition on every boot, so an edit there does not persist without
 * a flash. This helper is already setuid-root and already the thing that owns the tuner's
 * permissions, so the module it needs is its business.
 */
static const char *const MODULE_PATH = "/system/lib/modules/radio-si4708icx.ko";
static const char *const REGMON_DIR = "/proc/regmon/Si4708icx";

/* Load the tuner module if its regmon directory is not already there.
 *
 * Deliberately narrow, and checked before it is trusted: ONE hard-coded path, no argv, no
 * environment. The file must be a regular file owned by root and not writable by group or other —
 * loading a module is the most privileged thing in this binary, so a .ko that anyone else could
 * have replaced is refused rather than loaded. O_NOFOLLOW means a symlink planted at the path
 * fails at open() instead of being followed.
 *
 * Silent and best-effort: a kernel without the module, or one that already has it, both leave the
 * chmod loop below to do its job. Returns nothing because nothing here should stop that.
 */
static void load_tuner_module(void)
{
    struct stat st;
    int fd;

    /* Already loaded (or the driver is built in) — nothing to do. */
    if (stat(REGMON_DIR, &st) == 0 && S_ISDIR(st.st_mode))
        return;

    fd = open(MODULE_PATH, O_RDONLY | O_NOFOLLOW | O_CLOEXEC);
    if (fd < 0)
        return;
    if (fstat(fd, &st) != 0 || !S_ISREG(st.st_mode) || st.st_uid != 0 || (st.st_mode & 022)) {
        close(fd);
        return;
    }

    /* finit_module(2) FIRST — it hands the kernel the descriptor we just verified, so there is no
     * window between the check and the load. It is not available everywhere: this device's MTK
     * 3.10 kernel has the symbol in its headers but rejects the call (measured 2026-09-10 —
     * finit_module failed while `insmod`, which uses the older call, succeeded on the same file).
     */
#ifdef __NR_finit_module
    if (syscall(__NR_finit_module, fd, "", 0) == 0) {
        close(fd);
        return;
    }
#endif

    /* init_module(2) — the pre-3.8 call, which takes the image in memory. The size is bounded
     * before allocating so a wrong path cannot be turned into an arbitrary allocation; the real
     * module is ~17 KB.
     */
#ifdef __NR_init_module
    if (st.st_size > 0 && st.st_size <= 4 * 1024 * 1024) {
        size_t len = (size_t)st.st_size;
        unsigned char *buf = malloc(len);
        if (buf) {
            size_t got = 0;
            while (got < len) {
                ssize_t n = read(fd, buf + got, len - got);
                if (n <= 0)
                    break;
                got += (size_t)n;
            }
            /* A short read means the file changed under us — do not hand a truncated image to
             * the kernel. */
            if (got == len)
                (void)syscall(__NR_init_module, buf, len, "");
            free(buf);
        }
    }
#endif
    close(fd);
}

int main(void)
{
    int failed = 0;
    unsigned i;

    /* FIRST: the nodes below cannot be widened if the driver that creates them was never loaded. */
    load_tuner_module();

    for (i = 0; i < sizeof(nodes) / sizeof(nodes[0]); i++) {
        struct stat st;
        char fdpath[64];
        /* O_PATH: resolve the name to an inode without opening it — no driver side effects, and
         * on procfs no read of the register either. O_NOFOLLOW: a symlink here fails. */
        int fd = open(nodes[i], O_PATH | O_NOFOLLOW | O_CLOEXEC);
        if (fd < 0) {
            failed++;
            continue;
        }
        /* procfs entries are regular files. A device node or directory at these paths means
         * something is very wrong, and is not something to chmod. */
        if (fstat(fd, &st) != 0 || !S_ISREG(st.st_mode)) {
            close(fd);
            failed++;
            continue;
        }
        if ((st.st_mode & 0666) == 0666) {
            close(fd); /* already accessible */
            continue;
        }
        /* fchmod() is not permitted on an O_PATH descriptor, so go via /proc/self/fd — which
         * refers to the same inode the fstat above inspected. */
        snprintf(fdpath, sizeof fdpath, "/proc/self/fd/%d", fd);
        if (chmod(fdpath, 0666) != 0)
            failed++;
        close(fd);
    }
    return failed;
}
