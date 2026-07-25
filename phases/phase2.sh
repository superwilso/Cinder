#!/usr/bin/env bash
# Phase 2: binwalk every unpacked sector, extract boot image, loop-mount rootfs.
set -euo pipefail

ARTIFACTS="artifacts"
ANALYSIS="analysis"
UNPACKED="$ARTIFACTS/unpacked/stock"
BINWALK_OUT="$ANALYSIS/binwalk"
ROOTFS_IMG="$ARTIFACTS/rootfs.img"
ROOTFS_MNT="$ARTIFACTS/rootfs_mnt"
MOUNT_SCRIPT="$ARTIFACTS/mount_rootfs.sh"

log() { echo "[phase2] $*"; }

mkdir -p "$BINWALK_OUT" "$ROOTFS_MNT" "$ANALYSIS"

# ── Validate phase1 output ────────────────────────────────────────────────────

if [ -z "$(ls -A "$UNPACKED" 2>/dev/null)" ]; then
    echo "ERROR: $UNPACKED is empty — run make phase1 first."
    exit 1
fi

# ── binwalk every sector ──────────────────────────────────────────────────────

log "running binwalk on all unpacked sectors"
for f in "$UNPACKED"/*; do
    name=$(basename "$f")
    out="$BINWALK_OUT/$name"
    if [ -d "$out" ]; then
        log "  already scanned: $name (skipping)"
        continue
    fi
    log "  scanning: $name"
    binwalk --extract --directory="$out" "$f" 2>&1 | tee "$ANALYSIS/binwalk_${name}.log" || true
done
log "binwalk complete — results in $BINWALK_OUT/"

# ── Identify and extract boot image ──────────────────────────────────────────

log "searching for boot image (Android boot / MediaTek format)"
BOOT_IMG=""
# upgtool outputs numbered files (0.bin, 1.bin, ...). Detect by content.
for candidate in "$UNPACKED"/*.bin "$UNPACKED"/boot* "$UNPACKED"/*boot* "$UNPACKED"/*.img; do
    if [ -f "$candidate" ]; then
        type=$(file "$candidate" 2>/dev/null || echo "")
        if echo "$type" | grep -qi "android bootimg\|MediaTek bootimg\|bootimg.*kernel"; then
            BOOT_IMG="$candidate"
            log "  found boot image: $candidate ($type)"
            break
        fi
    fi
done

if [ -n "$BOOT_IMG" ]; then
    log "extracting boot image components"
    mkdir -p "$ANALYSIS/boot_image"
    # Use binwalk to extract kernel + initrd from boot image
    binwalk --extract --directory="$ANALYSIS/boot_image" "$BOOT_IMG" \
        2>&1 | tee "$ANALYSIS/boot_image_extract.log" || true
    log "boot image extracted → $ANALYSIS/boot_image/"
else
    log "WARNING: no Android/MediaTek boot image found in $UNPACKED — check binwalk output"
fi

# ── Find and loop-mount the rootfs ────────────────────────────────────────────

ROOTFS_CANDIDATE=""
for candidate in "$UNPACKED"/*.bin "$UNPACKED"/rootfs* "$UNPACKED"/*rootfs* \
                 "$UNPACKED"/*.ext* "$UNPACKED"/system* "$UNPACKED"/*system*; do
    if [ -f "$candidate" ]; then
        type=$(file "$candidate" 2>/dev/null || echo "")
        if echo "$type" | grep -qi "ext[234]\|Linux.*filesystem"; then
            ROOTFS_CANDIDATE="$candidate"
            log "found rootfs image: $candidate"
            break
        fi
    fi
done

if [ -n "$ROOTFS_CANDIDATE" ]; then
    cp "$ROOTFS_CANDIDATE" "$ROOTFS_IMG"

    # Extract rootfs with 7z (no sudo required — 7z reads ext4 natively).
    if [ -z "$(ls -A "$ROOTFS_MNT" 2>/dev/null)" ]; then
        log "extracting rootfs to $ROOTFS_MNT/ via 7z (no sudo)"
        7z x "$ROOTFS_IMG" -o"$ROOTFS_MNT" -y >/dev/null
        log "  extracted: $(find "$ROOTFS_MNT" -type f | wc -l) files, $(find "$ROOTFS_MNT" -type d | wc -l) dirs"
    else
        log "rootfs already extracted at $ROOTFS_MNT (skipping)"
    fi

    # Also leave a sudo loop-mount helper for users who want a real mount.
    cat > "$MOUNT_SCRIPT" <<EOF
#!/usr/bin/env bash
# Optional: real loop-mount (instead of 7z extraction). Run: sudo bash $MOUNT_SCRIPT
set -e
LOOP=\$(losetup -f)
losetup -r "\$LOOP" "$ROOTFS_IMG"
mkdir -p "$ROOTFS_MNT"
mount -o ro,loop "\$LOOP" "$ROOTFS_MNT"
echo "Rootfs mounted read-only at $ROOTFS_MNT"
echo "To unmount: sudo umount $ROOTFS_MNT && sudo losetup -d \$LOOP"
EOF
    chmod +x "$MOUNT_SCRIPT"

    log "rootfs available at $ROOTFS_MNT/  (image: $ROOTFS_IMG)"
else
    log "WARNING: no ext2/3/4 rootfs image found — check $BINWALK_OUT for extracted filesystems"
    log "Check $BINWALK_OUT/ for extracted squashfs or other container formats"
fi

log "Phase 2 complete."
