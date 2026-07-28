/* cinder-gpunode — tiny setuid-root helper (sibling of cinder-umount).
 *
 * WHY THIS EXISTS: cinder-home runs as uid 100 with an empty capability set. The Mali EGL
 * driver needs to open four device nodes that ship root-only (crw------- root root):
 *
 *     /dev/ion          (graphics buffer allocator)
 *     /dev/mtkfb_vsync  (vsync wait)
 *     /dev/mtk_disp     (display controller)
 *     /dev/sw_sync      (sync fences)
 *
 * Without them, uid-100 EGL init HANGS inside the driver (confirmed on-device 2026-07-26,
 * which tripped the bad-boot counter). /dev/mali and /dev/graphics/fb0 are already
 * system-owned and accessible; only these four need widening.
 *
 * Like cinder-umount, this does exactly one hard-coded thing and takes no input from the
 * caller: no argv, no environment use, fixed path list.
 *
 * RACE-FREE BY CONSTRUCTION. An earlier version did lstat() and then chmod(), and claimed the
 * lstat rejected a planted symlink — it did not. chmod() resolves the path again, and follows
 * symlinks, so anything that could replace the path between the two calls would have been chmod'd
 * instead. That is the textbook setuid TOCTOU, and while /dev on this device is root-owned (so it
 * was not reachable in practice), "not reachable in practice" is not the standard setuid code
 * should be held to. The check and the change are now bound to the same inode: O_PATH|O_NOFOLLOW
 * opens the path WITHOUT opening the device itself (no driver side effects, no blocking), fstat
 * verifies it really is a character device, and the chmod goes through /proc/self/fd — so it
 * cannot land anywhere other than the thing that was inspected.
 *
 * SECURITY TRADE-OFF (deliberate, documented in ROADMAP): 0666 on these nodes exposes
 * graphics memory + display control to every local uid. On this single-user music player
 * there are no untrusted local processes, so the practical risk is low — but it is a
 * loosening of kernel device permissions and is confined to exactly these four nodes.
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
    "/dev/ion",
    "/dev/mtkfb_vsync",
    "/dev/mtk_disp",
    "/dev/sw_sync",
};

int main(void)
{
    int failed = 0;
    unsigned i;
    for (i = 0; i < sizeof(nodes) / sizeof(nodes[0]); i++) {
        struct stat st;
        char fdpath[64];
        /* O_PATH: resolve the name to an inode without opening the DEVICE. O_NOFOLLOW: a symlink
         * at the path fails here rather than being followed. */
        int fd = open(nodes[i], O_PATH | O_NOFOLLOW | O_CLOEXEC);
        if (fd < 0) {
            failed++;
            continue;
        }
        if (fstat(fd, &st) != 0 || !S_ISCHR(st.st_mode)) {
            close(fd);
            failed++;
            continue;
        }
        if ((st.st_mode & 0666) == 0666) {
            close(fd); /* already accessible */
            continue;
        }
        /* chmod the OPEN inode, not the name. fchmod() is not permitted on an O_PATH descriptor,
         * so go via /proc/self/fd, which refers to the same inode the fstat above inspected. */
        snprintf(fdpath, sizeof fdpath, "/proc/self/fd/%d", fd);
        if (chmod(fdpath, 0666) != 0)
            failed++;
        close(fd);
    }
    return failed;
}
