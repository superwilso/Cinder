#!/system/bin/sh
# cinder-kernel.sh — the `kernel` option: Cinder's kernel in the boot partition, and Sony's back.
#
#   cinder-kernel.sh install ZIMAGE   save the original boot image (once), write Cinder's, read it back
#   cinder-kernel.sh remove           put the saved original back, when Cinder's is in place
#   cinder-kernel.sh status
#
# Run as root: by install_cinderhome.sh in Sony's updater, by uninstall_cinderhome.sh, or over adb.
# The partition is `bootimg` (p8). The bootloader (p7) and everything else are never touched.
#
# THE WAY BACK COMES FIRST. Before anything is written, the partition's own image is saved to the
# drive (SAVED) and read back against the partition. Without a saved original nothing is written:
# a kernel with no way back is not installed. The saved file is never replaced by a later install,
# because the only kernel that can be in the partition by then is Cinder's (the NW-A50 series has no
# more Sony updates; a Sony firmware install puts Sony's kernel back by itself).
#
# A WRITE IS CHECKED. After writing, the partition is read back against the image. If it differs,
# the original goes straight back. Images are put together by cinder-bootimg (src/cinder-bootimg.c)
# from the partition's own header and ramdisk; only the kernel changes, and the header is marked
# "cinder" so that a later run can tell whose kernel is in place.
#
# Host test: tools/test_kernel.sh (a file stands in for the partition).
BB=${BB:-/xbin/busybox}
[ -x "$BB" ] || BB=/system/xbin/busybox
[ -x "$BB" ] || BB=busybox
HELPER=${CINDER_BOOTIMG:-/system/vendor/cinder/bin/cinder-bootimg}
SAVED=${CINDER_BOOT_SAVED:-/contents/cinder_boot_original.img}
NEW=${CINDER_BOOT_NEW:-/contents/.cinder_boot_new.img}
say() { echo "kernel: $*"; }

# The bootimg partition: MediaTek's table names it; failing that, p8 if it is the 16 MiB it should be.
part() {
    if [ -n "${CINDER_BOOT_DEV:-}" ]; then echo "$CINDER_BOOT_DEV"; return; fi
    if [ -r /proc/dumchar_info ]; then
        while read -r name size _start _type map; do
            if [ "$name" = bootimg ] && [ "$size" = 0x0000000001000000 ]; then echo "$map"; return; fi
        done < /proc/dumchar_info
        return
    fi
    [ "$(cat /sys/block/mmcblk0/mmcblk0p8/size 2>/dev/null)" = 32768 ] && echo /dev/block/mmcblk0p8
}

# write IMG to DEV, then read it back
put() {
    "$BB" dd if="$1" of="$2" bs=4096 2>/dev/null
    sync
    "$HELPER" same "$2" "$1"
}

whose() {  # cinder | sony | none
    i="$("$HELPER" info "$1" 2>/dev/null)" || { echo none; return; }
    case "$i" in *cinder=1*) echo cinder ;; *) echo sony ;; esac
}

install() {
    z="$1"
    [ -s "$z" ] || { say "no kernel file at $z — nothing changed"; return 1; }
    P="$(part)"
    [ -n "$P" ] || { say "the boot partition was not found — nothing changed"; return 1; }
    cur="$(whose "$P")"
    [ "$cur" != none ] || { say "$P does not hold a boot image this understands — nothing changed"; return 1; }
    if [ "$cur" = sony ] && [ ! -s "$SAVED" ]; then
        if "$HELPER" copy "$P" "$SAVED" && "$HELPER" same "$P" "$SAVED"; then
            sync
            say "saved the original boot image as $SAVED"
        else
            "$BB" rm -f "$SAVED"
            say "could not save the original boot image — nothing changed"
            return 1
        fi
    fi
    if [ ! -s "$SAVED" ] || [ "$(whose "$SAVED")" != sony ]; then
        say "no saved original boot image at $SAVED, so there would be no way back — nothing changed"
        return 1
    fi
    if ! "$HELPER" swap "$P" "$z" "$NEW"; then
        "$BB" rm -f "$NEW"
        say "could not put the new boot image together — nothing changed"
        return 1
    fi
    if "$HELPER" same "$P" "$NEW"; then
        "$BB" rm -f "$NEW"
        say "Cinder's kernel is already in place"
        return 0
    fi
    if put "$NEW" "$P"; then
        "$BB" rm -f "$NEW"
        say "installed Cinder's kernel (read back and checked)"
        return 0
    fi
    "$BB" rm -f "$NEW"
    say "the boot partition did not read back as written — putting the original back"
    if put "$SAVED" "$P"; then
        say "the original kernel is back"
    else
        say "THE ORIGINAL DID NOT READ BACK EITHER. Do not restart; run the installer's Fix a problem, or restore bootimg with cinder-rescue"
    fi
    return 1
}

remove() {
    P="$(part)"
    [ -n "$P" ] || { say "the boot partition was not found — nothing changed"; return 1; }
    [ "$(whose "$P")" = cinder ] || { say "Cinder's kernel is not in place — nothing to do"; return 0; }
    if [ ! -s "$SAVED" ] || [ "$(whose "$SAVED")" != sony ]; then
        say "Cinder's kernel stays: no saved original at $SAVED. Sony's firmware installer or cinder-rescue puts Sony's back"
        return 1
    fi
    if put "$SAVED" "$P"; then
        say "put the original kernel back (read back and checked)"
        return 0
    fi
    say "THE ORIGINAL DID NOT READ BACK. Do not restart; run the installer again, or restore bootimg with cinder-rescue"
    return 1
}

case "${1:-}" in
    install) install "${2:-}" ;;
    remove) remove ;;
    status)
        P="$(part)"
        echo "partition: ${P:-not found}"
        [ -n "$P" ] && echo "in place: $(whose "$P")"
        if [ -s "$SAVED" ]; then echo "saved original: $SAVED ($(whose "$SAVED"))"; else echo "saved original: none"; fi
        ;;
    *) echo "usage: cinder-kernel.sh install ZIMAGE | remove | status" >&2; exit 2 ;;
esac
