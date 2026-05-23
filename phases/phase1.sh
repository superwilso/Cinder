#!/usr/bin/env bash
# Phase 1: Clone reference repos, build upgtool, unpack both .UPG files.
set -euo pipefail

ARTIFACTS="artifacts"
REPOS="$ARTIFACTS/repos"
STOCK_UPG="$ARTIFACTS/stock/NW_WM_FW.UPG"
W1_UPG="$ARTIFACTS/walkmanone/WalkmanOne.UPG"

log() { echo "[phase1] $*"; }

mkdir -p "$REPOS" "$ARTIFACTS/stock" "$ARTIFACTS/walkmanone" \
         "$ARTIFACTS/unpacked/stock" "$ARTIFACTS/unpacked/walkmanone"

# ── Clone reference repos ─────────────────────────────────────────────────────

clone_or_update() {
    local url="$1" dir="$2" flags="${3:---depth=1}"
    if [ -d "$dir/.git" ]; then
        log "already cloned: $dir (skipping)"
    else
        log "cloning $url → $dir"
        git clone $flags "$url" "$dir"
    fi
}

clone_or_update https://github.com/unknown321/wampy        "$REPOS/wampy"
clone_or_update https://github.com/unknown321/scrobbler    "$REPOS/scrobbler"
clone_or_update https://github.com/unknown321/wbrt         "$REPOS/wbrt"
clone_or_update https://github.com/roobscoob/SonyWalkmanFirmwarePatcher \
                "$REPOS/SonyWalkmanFirmwarePatcher"

# Rockbox — sparse checkout of nwztools only (full repo is huge)
if [ ! -d "$REPOS/rockbox/.git" ]; then
    log "cloning Rockbox (sparse: utils/nwztools)"
    git clone --depth=1 --filter=blob:none --sparse \
        https://github.com/Rockbox/rockbox "$REPOS/rockbox"
    git -C "$REPOS/rockbox" sparse-checkout set utils/nwztools
else
    log "already cloned: $REPOS/rockbox (skipping)"
fi

# llusbdac — add manually as documented in CLAUDE.md Part C
clone_or_update https://github.com/zhangboyang/llusbdac    "$REPOS/llusbdac"

# ── Build upgtool ─────────────────────────────────────────────────────────────

UPGTOOL_SRC="$REPOS/rockbox/utils/nwztools/upgtools"
UPGTOOL_BIN="$ARTIFACTS/upgtool"

if [ -f "$UPGTOOL_BIN" ]; then
    log "upgtool already built at $UPGTOOL_BIN"
else
    log "building upgtool"
    if [ ! -d "$UPGTOOL_SRC" ]; then
        echo "ERROR: $UPGTOOL_SRC not found — Rockbox sparse checkout may have failed."
        exit 1
    fi
    (cd "$UPGTOOL_SRC" && make)
    cp "$UPGTOOL_SRC/upgtool" "$UPGTOOL_BIN"
    log "upgtool built: $UPGTOOL_BIN"
fi

# ── Unpack firmware images ────────────────────────────────────────────────────

unpack_upg() {
    local upg="$1" outdir="$2" label="$3"
    if [ ! -f "$upg" ]; then
        echo ""
        echo "WARNING: $upg not found."
        echo "  Download $label and place it at $upg"
        echo "  See CLAUDE.md Part D1 for instructions."
        echo "  Skipping unpack for now — re-run make phase1 after placing the file."
        echo ""
        return 0
    fi
    if [ -n "$(ls -A "$outdir" 2>/dev/null)" ]; then
        log "$label already unpacked to $outdir (skipping)"
        return 0
    fi
    log "unpacking $label → $outdir"
    "$ARTIFACTS/upgtool" -x "$upg" -o "$outdir"
    log "$label unpacked: $(ls "$outdir" | wc -l) files"
}

unpack_upg "$STOCK_UPG"   "$ARTIFACTS/unpacked/stock"     "Stock NW-A55 firmware (NW_WM_FW.UPG)"
unpack_upg "$W1_UPG"      "$ARTIFACTS/unpacked/walkmanone" "Walkman One firmware (WalkmanOne.UPG)"

log "Phase 1 complete."
