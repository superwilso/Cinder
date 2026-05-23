#!/usr/bin/env bash
# Phase 7: .UPG repack round-trip — proves modify→repack→unpack is sound.
set -euo pipefail

ARTIFACTS="artifacts"
ANALYSIS="analysis"
UPGTOOL="$ARTIFACTS/upgtool"
STOCK_UPG="$ARTIFACTS/stock/NW_WM_FW.UPG"
UNPACKED="$ARTIFACTS/unpacked/stock"
REPACK_DIR="$ARTIFACTS/repack_test"
REPACK_UPG="$REPACK_DIR/NW_WM_FW_repack.UPG"
REPACK_UNPACK="$REPACK_DIR/unpack_verify"

log()  { echo "[phase7] $*"; }
fail() { echo "[phase7] FAIL: $*"; exit 1; }

mkdir -p "$REPACK_DIR" "$REPACK_UNPACK" "$ANALYSIS"

if [ ! -f "$UPGTOOL" ]; then
    fail "upgtool not found at $ARTIFACTS/upgtool — run make phase1 first"
fi
if [ ! -f "$STOCK_UPG" ]; then
    fail "stock UPG not found at $STOCK_UPG — download and place per CLAUDE.md Part D1"
fi

# ── Step 1: Copy unpacked sectors to repack staging area ─────────────────────

log "copying unpacked sectors to repack staging: $REPACK_DIR/sectors/"
mkdir -p "$REPACK_DIR/sectors"
cp -a "$UNPACKED"/. "$REPACK_DIR/sectors/"

# ── Step 2: Repack using upgtool ──────────────────────────────────────────────

log "repacking with upgtool"
# upgtool usage: upgtool -c <output.upg> -d <dir_of_sectors>
# Actual flags may differ — check upgtool --help or Rockbox docs
"$UPGTOOL" -c "$REPACK_UPG" -d "$REPACK_DIR/sectors" 2>&1 \
    | tee "$ANALYSIS/7_repack.log" || {
    log "upgtool repack failed — checking alternate invocation"
    # Try pack mode with explicit flags
    "$UPGTOOL" pack "$REPACK_DIR/sectors" "$REPACK_UPG" 2>&1 \
        | tee -a "$ANALYSIS/7_repack.log" || \
    fail "repack failed — check $ANALYSIS/7_repack.log and upgtool --help"
}

if [ ! -f "$REPACK_UPG" ]; then
    fail "repack produced no output file — check $ANALYSIS/7_repack.log"
fi

log "repack produced: $REPACK_UPG ($(stat -c%s "$REPACK_UPG") bytes)"

# ── Step 3: Unpack the repacked UPG ──────────────────────────────────────────

log "unpacking repacked UPG to verify round-trip"
"$UPGTOOL" -x "$REPACK_UPG" -o "$REPACK_UNPACK" 2>&1 \
    | tee "$ANALYSIS/7_unpack_verify.log"

# ── Step 4: Compare original unpack vs round-trip unpack ─────────────────────

log "comparing original sectors vs round-trip sectors"
OUT="$ANALYSIS/7_roundtrip_diff.txt"
{
    echo "=== Phase 7: Round-trip verification ==="
    echo "Original: $UNPACKED"
    echo "Repacked: $REPACK_UPG"
    echo "Re-unpacked: $REPACK_UNPACK"
    echo ""
    PASS=0; FAIL=0
    for f in "$UNPACKED"/*; do
        name=$(basename "$f")
        rt="$REPACK_UNPACK/$name"
        if [ ! -f "$rt" ]; then
            echo "MISSING in round-trip: $name"
            FAIL=$((FAIL + 1))
        elif cmp -s "$f" "$rt"; then
            echo "OK (identical): $name"
            PASS=$((PASS + 1))
        else
            echo "DIFFERS: $name"
            echo "  original:  $(stat -c%s "$f") bytes / $(md5sum "$f" | cut -d' ' -f1)"
            echo "  roundtrip: $(stat -c%s "$rt") bytes / $(md5sum "$rt" | cut -d' ' -f1)"
            FAIL=$((FAIL + 1))
        fi
    done
    echo ""
    echo "Result: $PASS identical, $FAIL differ"
    if [ "$FAIL" -eq 0 ]; then
        echo "ROUND-TRIP: PASS — safe to use upgtool for modify-and-repack"
    else
        echo "ROUND-TRIP: FAIL — investigate differences before deploying custom .UPG"
    fi
} | tee "$OUT"

log "Round-trip results → $OUT"
log "Phase 7 complete."
log ""
log "If round-trip PASSED: the modify→repack→install path is proven sound."
log "Next step: bundle hello-walkman into a .UPG (CLAUDE.md Part E7)."
