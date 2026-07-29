#!/usr/bin/env bash
# Phase 4: Rootfs deep-dive — 4a through 4h.
# Requires rootfs mounted at artifacts/rootfs_mnt (see phase2 + mount_rootfs.sh).
set -eu
# Phase 4 is purely diagnostic dumping (strings|grep|head); SIGPIPE from head
# closing early should not abort the run, so leave pipefail off.

ARTIFACTS="artifacts"
ROOTFS="$ARTIFACTS/rootfs_mnt"
ANALYSIS="analysis"

log() { echo "[phase4] $*"; }
warn() { echo "[phase4] WARN: $*"; }

mkdir -p "$ANALYSIS"

# Accept either a real mount or a 7z-extracted directory (phase2 produces the latter).
if ! mountpoint -q "$ROOTFS" 2>/dev/null && [ ! -f "$ROOTFS/build.prop" ]; then
    warn "Rootfs not available at $ROOTFS"
    warn "Run phase2 first, or: sudo bash $ARTIFACTS/mount_rootfs.sh"
    exit 1
fi

# ── 4a: Init system overview ──────────────────────────────────────────────────

log "4a: init system"
OUT="$ANALYSIS/4a_init_system.txt"
{
    echo "=== Phase 4a: Init System ==="
    echo ""
    for rc in "$ROOTFS"/init.rc "$ROOTFS"/init.hagoromo.rc "$ROOTFS"/init.*.rc; do
        [ -f "$rc" ] || continue
        echo "--- $(basename "$rc") ---"
        cat "$rc"
        echo ""
    done
} > "$OUT"
log "  → $OUT"

# ── 4b: HgrmMediaPlayerApp service definition ────────────────────────────────

log "4b: HgrmMediaPlayerApp service"
OUT="$ANALYSIS/4b_init_flow.txt"
{
    echo "=== Phase 4b: HgrmMediaPlayerApp init service ==="
    echo ""
    grep -rn "HgrmMediaPlayerApp\|HgrmLauncher\|hgrm" "$ROOTFS"/init*.rc \
        "$ROOTFS"/etc/init*.rc 2>/dev/null || echo "(not found in init.rc files)"
    echo ""
    echo "=== Binary location ==="
    find "$ROOTFS" -name "HgrmMediaPlayerApp" -o -name "hgrm*" 2>/dev/null | head -20
    echo ""
    echo "=== Linked libraries (if readable) ==="
    bin=$(find "$ROOTFS" -name "HgrmMediaPlayerApp" 2>/dev/null | head -1)
    if [ -n "$bin" ]; then
        readelf -d "$bin" 2>/dev/null | grep NEEDED || echo "(readelf failed)"
    fi
} > "$OUT"
log "  → $OUT"

# ── 4c: Sony clang / compiler version ────────────────────────────────────────

log "4c: toolchain/compiler version"
OUT_C="$ANALYSIS/4c_compiler_version.txt"
OUT_ABI="$ANALYSIS/4c_abi_info.txt"
{
    echo "=== Phase 4c: Compiler version strings ==="
    echo ""
    echo "--- HgrmMediaPlayerApp ---"
    bin=$(find "$ROOTFS" -name "HgrmMediaPlayerApp" 2>/dev/null | head -1)
    if [ -n "$bin" ]; then
        strings "$bin" | grep -iE "clang|gcc|llvm|android ndk|arm-linux" | head -30
        echo ""
        echo "--- ELF header ---"
        readelf -h "$bin" 2>/dev/null | grep -E "Class|Machine|ABI|Entry|Type"
    fi
    echo ""
    echo "--- libSoundServiceFw.so ---"
    lib=$(find "$ROOTFS" -name "libSoundServiceFw.so" 2>/dev/null | head -1)
    if [ -n "$lib" ]; then
        strings "$lib" | grep -iE "clang|gcc|llvm|ndk" | head -20
    fi
} > "$OUT_C"

{
    echo "=== Phase 4c: ABI compatibility info ==="
    echo ""
    echo "Key question: clang/LLVM vs GCC C++ ABI — affects libstdc++ linkage"
    echo "for the replacement player's C shim."
    echo ""
    for lib in "$ROOTFS"/lib/libstdc++* "$ROOTFS"/system/lib/libstdc++* \
               "$ROOTFS"/usr/lib/libstdc++*; do
        [ -f "$lib" ] || continue
        echo "Found: $lib"
        strings "$lib" | grep -i "GLIBCXX\|CXXABI\|version" | head -10
    done
} > "$OUT_ABI"
log "  → $OUT_C $OUT_ABI"

# ── 4d: View-model surface (Qt/QML) ──────────────────────────────────────────

log "4d: view-model / Qt QML surface"
OUT="$ANALYSIS/4d_qt_viewmodel.txt"
{
    echo "=== Phase 4d: Qt / QML view-model surface ==="
    echo ""
    echo "--- QML files ---"
    find "$ROOTFS" -name "*.qml" 2>/dev/null | head -50
    echo ""
    echo "--- Qt version strings ---"
    for lib in "$ROOTFS"/lib/libQt5*.so* "$ROOTFS"/usr/lib/libQt5*.so*; do
        [ -f "$lib" ] || continue
        ver=$(strings "$lib" | grep -m1 "Qt [0-9]\." || true)
        [ -n "$ver" ] && echo "  $(basename "$lib"): $ver"
    done
    echo ""
    echo "--- Property/signal strings in HgrmMediaPlayerApp ---"
    bin=$(find "$ROOTFS" -name "HgrmMediaPlayerApp" 2>/dev/null | head -1)
    if [ -n "$bin" ]; then
        strings "$bin" | grep -E "^[a-z][a-zA-Z]+[A-Z]|Changed$|Property$|Signal$" \
            | head -60
    fi
} > "$OUT"
log "  → $OUT"

# ── 4e: libSoundServiceFw symbols ────────────────────────────────────────────

log "4e: libSoundServiceFw symbol table"
OUT="$ANALYSIS/4e_soundservice_symbols.txt"
{
    echo "=== Phase 4e: libSoundServiceFw.so ==="
    echo ""
    lib=$(find "$ROOTFS" -name "libSoundServiceFw.so" 2>/dev/null | head -1)
    if [ -z "$lib" ]; then
        echo "libSoundServiceFw.so NOT FOUND in rootfs"
    else
        echo "Path: $lib"
        echo ""
        echo "--- Exported symbols (nm) ---"
        nm -D "$lib" 2>/dev/null | grep " T \| W " | head -100
        echo ""
        echo "--- Filter/EQ-related strings ---"
        strings "$lib" | grep -iE "filter|eq|equalizer|tone|vinyl|vpt|dsee|dc_phase|noise" \
            | sort -u | head -60
        echo ""
        echo "--- Volume table / model strings ---"
        strings "$lib" | grep -iE "WM1Z|WM1A|A50|A55|NW-|model|volume_table" \
            | sort -u | head -40
    fi
} > "$OUT"
log "  → $OUT"

# ── 4f: USB-DAC routing analysis ─────────────────────────────────────────────

log "4f: USB-DAC routing"
OUT="$ANALYSIS/4f_usb_dac_routing.txt"
{
    echo "=== Phase 4f: USB-DAC routing investigation ==="
    echo "See docs/baseline_v1.4.md §5.10 for the three enforcement candidates."
    echo ""
    echo "--- llusbdac.ko strings ---"
    ko=$(find "$ROOTFS" "$ARTIFACTS" -name "llusbdac.ko" 2>/dev/null | head -1)
    if [ -n "$ko" ]; then
        echo "Found: $ko"
        strings "$ko" | grep -iE "alsa|pcm|sink|route|bt|bluetooth|bluez|ldac|cxd" \
            | sort -u
    else
        echo "llusbdac.ko not found — check artifacts/repos/llusbdac/ for source"
    fi
    echo ""
    echo "--- Bluetooth exclusivity strings in HgrmMediaPlayerApp ---"
    bin=$(find "$ROOTFS" -name "HgrmMediaPlayerApp" 2>/dev/null | head -1)
    if [ -n "$bin" ]; then
        strings "$bin" | grep -iE "usb.dac|usb_dac|usbdac|bluetooth|bluez|ldac|exclusive\
|cannot use|not available|disconnect" | sort -u | head -40
    fi
    echo ""
    echo "--- libSoundServiceFw routing strings ---"
    lib=$(find "$ROOTFS" -name "libSoundServiceFw.so" 2>/dev/null | head -1)
    if [ -n "$lib" ]; then
        strings "$lib" | grep -iE "source|sink|route|usb|bluetooth|bluez|ldac|exclusive\
|conflict|mode" | sort -u | head -40
    fi
    echo ""
    echo "=== Working hypothesis ==="
    echo "Candidate 1 (app policy) most likely — Sony shows a confirmation dialog"
    echo "when switching modes. On-device strace will confirm (CLAUDE.md Part E5)."
    echo "Do not treat USB-DAC+LDAC as a guaranteed feature before E4/E5."
} > "$OUT"
log "  → $OUT"

# ── 4g: Hold switch / hardware button mapping ─────────────────────────────────

log "4g: hold switch"
OUT="$ANALYSIS/4g_hold_switch.txt"
{
    echo "=== Phase 4g: Hold switch / input event mapping ==="
    echo ""
    echo "--- Input device nodes ---"
    find "$ROOTFS" -path "*/input/event*" -o -name "*.idc" 2>/dev/null | head -20
    echo ""
    echo "--- keylayout / keychars files ---"
    find "$ROOTFS" -name "*.kl" -o -name "*.kcm" 2>/dev/null | head -20
    for f in $(find "$ROOTFS" -name "*.kl" 2>/dev/null | head -5); do
        echo "--- $f ---"
        cat "$f"
    done
    echo ""
    echo "--- Hold/lock strings in HgrmMediaPlayerApp ---"
    bin=$(find "$ROOTFS" -name "HgrmMediaPlayerApp" 2>/dev/null | head -1)
    if [ -n "$bin" ]; then
        strings "$bin" | grep -iE "hold|lock|switch|key_lock|slide" | sort -u | head -20
    fi
} > "$OUT"
log "  → $OUT"

# ── 4h: 2038 / time_t boundary analysis ──────────────────────────────────────

log "4h: 2038 / time_t analysis"
OUT="$ANALYSIS/4h_2038_risk.txt"
{
    echo "=== Phase 4h: Year 2038 / time_t boundary analysis ==="
    echo ""
    echo "Platform: Linux 3.10, 32-bit ARM, time_t is 32-bit."
    echo "Risk: kernel and libc syscalls will overflow in Jan 2038."
    echo "Rust mitigation: SystemTime → u64 via as_secs() is safe for scrobbler"
    echo "timestamps; does NOT fix kernel RTC, filesystem timestamps, or"
    echo "any Sony library that accepts/returns time_t."
    echo ""
    echo "--- Kernel version (from strings) ---"
    vmlinuz=$(find "$ARTIFACTS" -name "zImage" -o -name "vmlinuz" -o -name "kernel" \
              2>/dev/null | head -1)
    if [ -n "$vmlinuz" ]; then
        strings "$vmlinuz" | grep -m1 "Linux version" || echo "(not found)"
    fi
    echo ""
    echo "--- time_t / mktime / localtime usage in libSoundServiceFw ---"
    lib=$(find "$ROOTFS" -name "libSoundServiceFw.so" 2>/dev/null | head -1)
    if [ -n "$lib" ]; then
        nm -D "$lib" 2>/dev/null | grep -iE "time|mktime|localtime|gmtime" | head -20
    fi
    echo ""
    echo "--- time_t exposure in HgrmMediaPlayerApp ---"
    bin=$(find "$ROOTFS" -name "HgrmMediaPlayerApp" 2>/dev/null | head -1)
    if [ -n "$bin" ]; then
        nm -D "$bin" 2>/dev/null | grep -iE "time|mktime|localtime|gmtime" | head -20
    fi
    echo ""
    echo "Mitigation rule: never pass time_t across the Rust↔C shim boundary."
    echo "All timestamps in the replacement player must stay in u64 (Unix seconds)."
    echo "See docs/baseline_v1.4.md §5.4 and §8 risk register."
} > "$OUT"
log "  → $OUT"

log ""
log "Phase 4 complete. Results in analysis/4[a-h]_*.txt"
log "Key outputs:"
log "  4b_init_flow.txt      — HgrmMediaPlayerApp service definition"
log "  4e_soundservice_symbols.txt — libSoundServiceFw symbol table"
log "  4f_usb_dac_routing.txt — routing analysis (narrows, does not close)"
log "  4h_2038_risk.txt       — 2038 exposure points"
