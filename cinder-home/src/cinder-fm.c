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
#include <sys/stat.h>
#include <unistd.h>

#ifndef O_PATH
#define O_PATH 010000000
#endif

static const char *const nodes[] = {
    "/proc/regmon/Si4708icx/target",
    "/proc/regmon/Si4708icx/value",
};

int main(void)
{
    int failed = 0;
    unsigned i;
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
