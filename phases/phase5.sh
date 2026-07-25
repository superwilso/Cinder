#!/usr/bin/env bash
# Phase 5: Diff stock firmware vs Walkman One — "what W1 changes" recipe.
set -euo pipefail

ARTIFACTS="artifacts"
ANALYSIS="analysis"
STOCK="$ARTIFACTS/unpacked/stock"
W1="$ARTIFACTS/unpacked/walkmanone"

log() { echo "[phase5] $*"; }
warn() { echo "[phase5] WARN: $*"; }

mkdir -p "$ANALYSIS"

if [ -z "$(ls -A "$STOCK" 2>/dev/null)" ]; then
    warn "$STOCK is empty — ensure stock firmware was unpacked (make phase1)"
    exit 1
fi
if [ -z "$(ls -A "$W1" 2>/dev/null)" ]; then
    warn "$W1 is empty — ensure Walkman One firmware was unpacked (make phase1)"
    warn "Download WalkmanOne.UPG from mrwalkman.com and place at $ARTIFACTS/walkmanone/WalkmanOne.UPG"
    exit 1
fi

OUT="$ANALYSIS/5_stock_vs_w1_diff.txt"
OUT_FILES="$ANALYSIS/5_changed_files.txt"

log "diffing $STOCK vs $W1 (sector-level)"
{
    echo "=== Phase 5: Stock vs Walkman One Diff ==="
    echo "Stock: $STOCK"
    echo "Walkman One: $W1"
    echo "Date: $(date)"
    echo ""
    echo "=== Sector-level file list diff ==="
    diff <(ls -la "$STOCK" | sort) <(ls -la "$W1" | sort) || true
    echo ""
    echo "=== Size changes ==="
    for f in "$STOCK"/*; do
        name=$(basename "$f")
        w1f="$W1/$name"
        if [ -f "$w1f" ]; then
            s1=$(stat -c%s "$f" 2>/dev/null || echo 0)
            s2=$(stat -c%s "$w1f" 2>/dev/null || echo 0)
            if [ "$s1" != "$s2" ]; then
                echo "  CHANGED: $name ($s1 → $s2 bytes, delta $((s2 - s1)))"
            fi
        else
            echo "  REMOVED in W1: $name"
        fi
    done
    for f in "$W1"/*; do
        name=$(basename "$f")
        if [ ! -f "$STOCK/$name" ]; then
            echo "  ADDED in W1: $name"
        fi
    done
} > "$OUT"

# Track which specific sectors changed (for rootfs overlay focus)
{
    echo "Files changed between stock and Walkman One:"
    for f in "$STOCK"/*; do
        name=$(basename "$f")
        w1f="$W1/$name"
        if [ -f "$w1f" ] && ! cmp -s "$f" "$w1f"; then
            echo "  $name"
        fi
    done
} > "$OUT_FILES"

log "Diff complete → $OUT"
log "Changed files list → $OUT_FILES"
log ""
log "Next: examine changed sectors in the mounted rootfs to find W1's patches."
log "The main W1 change is typically the rootfs overlay (modified system files)"
log "and potentially a changed boot image."
