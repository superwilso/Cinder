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
 * caller: no argv, no environment use, fixed path list. Each path must already be a
 * character device (lstat, not stat, so a symlink planted at the path is rejected rather
 * than followed) before it is chmod'd to 0666.
 *
 * SECURITY TRADE-OFF (deliberate, documented in ROADMAP): 0666 on these nodes exposes
 * graphics memory + display control to every local uid. On this single-user music player
 * there are no untrusted local processes, so the practical risk is low — but it is a
 * loosening of kernel device permissions and is confined to exactly these four nodes.
 *
 * Exit code = number of nodes that could not be made accessible (0 = all good).
 */
#include <sys/stat.h>

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
        /* lstat: refuse to operate through a symlink planted at the path. */
        if (lstat(nodes[i], &st) != 0 || !S_ISCHR(st.st_mode)) {
            failed++;
            continue;
        }
        if ((st.st_mode & 0666) == 0666)
            continue; /* already accessible */
        if (chmod(nodes[i], 0666) != 0)
            failed++;
    }
    return failed;
}
