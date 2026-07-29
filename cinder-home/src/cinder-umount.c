/* cinder-umount — tiny setuid-root helper.
 *
 * WHY THIS EXISTS: cinder-home is launched by appmgr as uid 100 with an EMPTY capability set
 * (CapEff=0). It therefore cannot umount(2) /contents itself (EPERM) — yet handing the internal
 * storage to a PC over USB-MSC REQUIRES /contents to be unmounted first so the mass_storage gadget
 * can bind the raw block device exclusively. Everything else in the MSC switch (LUN backing file,
 * gadget functions/idProduct/enable) is world-writable 0666 sysfs that uid 100 can drive directly.
 * So this is the ONE privileged primitive cinder needs.
 *
 * The device has no SELinux and /system is not mounted nosuid, so a classic setuid-root binary
 * (chmod 4755, owner root) regains full caps on exec even when the caller is capless uid 100 —
 * verified on-device: a uid-100 process execing this unmounts /contents (rc 0).
 *
 * Deliberately does exactly one thing and nothing configurable — it's setuid root, so its whole
 * job is the single hard-coded umount, no argv, no paths from the caller. */
#include <sys/mount.h>

int main(void)
{
    if (umount("/contents") == 0)
        return 0;
    /* A lingering holder (e.g. a media file mid-close) — detach lazily so the gadget can still
     * bind; the kernel frees the block device once the last reference drops. */
    if (umount2("/contents", MNT_DETACH) == 0)
        return 0;
    return 1;
}
