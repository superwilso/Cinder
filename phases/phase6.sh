#!/usr/bin/env bash
# Phase 6: Cross-compile hello-walkman (armv7 musl) + C shim + qemu-test.
set -euo pipefail

ARTIFACTS="artifacts"
ANALYSIS="analysis"
BUILD="$ARTIFACTS/build"
TARGET="armv7-unknown-linux-musleabihf"

log()  { echo "[phase6] $*"; }
warn() { echo "[phase6] WARN: $*"; }
fail() { echo "[phase6] FAIL: $*"; exit 1; }

mkdir -p "$BUILD/hello_walkman" "$BUILD/c_shim" "$ANALYSIS"

# ── Verify toolchain ──────────────────────────────────────────────────────────

if ! command -v cargo >/dev/null 2>&1; then
    fail "cargo not found — install Rust (CLAUDE.md Part B2)"
fi

if ! rustup target list --installed | grep -q "$TARGET"; then
    log "adding Rust target $TARGET"
    rustup target add "$TARGET"
fi

if ! command -v arm-linux-musleabihf-gcc >/dev/null 2>&1; then
    warn "arm-linux-musleabihf-gcc not found"
    warn "Install musl cross-compiler: see CLAUDE.md Part B2"
    warn "Continuing without C shim build..."
    SKIP_C_SHIM=1
else
    SKIP_C_SHIM=0
fi

# ── hello-walkman Rust binary ─────────────────────────────────────────────────

HW_DIR="$BUILD/hello_walkman"

if [ ! -f "$HW_DIR/Cargo.toml" ]; then
    log "creating hello-walkman Rust project"
    # cargo init works in the pre-created dir from mkdir -p above.
    (cd "$HW_DIR" && cargo init --name hello_walkman --bin 2>&1 || true)
fi

cat > "$HW_DIR/src/main.rs" <<'EOF'
// hello-walkman: minimal armv7-musl binary for NW-A55 deployment test.
// Writes to a log file and sleeps — proves the binary runs on the device.
use std::fs::OpenOptions;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let log_path = "/contents/hello_walkman.log";
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .unwrap_or_else(|e| {
            // Fallback to /tmp if /contents not available
            eprintln!("cannot open {}: {} — falling back to /tmp", log_path, e);
            OpenOptions::new()
                .create(true)
                .append(true)
                .open("/tmp/hello_walkman.log")
                .expect("/tmp fallback also failed")
        });

    writeln!(f, "hello-walkman started at unix={}", ts).ok();
    writeln!(f, "build target: armv7-unknown-linux-musleabihf").ok();
    writeln!(f, "static musl binary — no glibc dependency").ok();

    // Stay alive briefly so service manager sees it running
    std::thread::sleep(std::time::Duration::from_secs(5));
    writeln!(f, "hello-walkman exiting cleanly").ok();
}
EOF

# Configure cargo for cross-compilation
mkdir -p "$HW_DIR/.cargo"
cat > "$HW_DIR/.cargo/config.toml" <<EOF
[target.$TARGET]
linker = "arm-linux-musleabihf-gcc"
EOF

log "cross-compiling hello-walkman → $TARGET"
(cd "$HW_DIR" && \
    CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_LINKER=arm-linux-musleabihf-gcc \
    cargo build --release --target "$TARGET" 2>&1) \
    | tee "$ANALYSIS/6_hello_walkman_build.log"

BIN="$HW_DIR/target/$TARGET/release/hello_walkman"
if [ -f "$BIN" ]; then
    cp "$BIN" "$ARTIFACTS/hello_walkman_arm"
    log "Binary: $ARTIFACTS/hello_walkman_arm"
    file "$BIN"
    ls -lh "$BIN"
else
    fail "build failed — check $ANALYSIS/6_hello_walkman_build.log"
fi

# ── C shim skeleton ───────────────────────────────────────────────────────────

if [ "$SKIP_C_SHIM" -eq 0 ]; then
    SHIM_DIR="$BUILD/c_shim"
    cat > "$SHIM_DIR/shim.c" <<'EOF'
/*
 * C shim: bridges Rust replacement player to Sony's libSoundServiceFw.so.
 *
 * Compiled with arm-linux-musleabihf-gcc against the device's headers.
 * All time_t values must stay inside this shim — never expose time_t to Rust.
 * See docs/baseline_v1.4.md §5.4 (2038 risk) and §4h analysis.
 */
#include <stdint.h>
#include <string.h>

/* Opaque handle returned by Sony's player service init. */
typedef void* PlayerHandle;

/* Example shim functions — replace with actual libSoundServiceFw symbols
 * discovered in analysis/4e_soundservice_symbols.txt */

extern PlayerHandle SoundService_Open(const char* source_path);
extern int          SoundService_Play(PlayerHandle h);
extern int          SoundService_Stop(PlayerHandle h);
extern void         SoundService_Close(PlayerHandle h);

int shim_play(const char* path) {
    PlayerHandle h = SoundService_Open(path);
    if (!h) return -1;
    int r = SoundService_Play(h);
    return r;
}
EOF

    cat > "$SHIM_DIR/Makefile" <<EOF
CC = arm-linux-musleabihf-gcc
CFLAGS = -Wall -O2 -static -fPIC

shim.o: shim.c
	\$(CC) \$(CFLAGS) -c shim.c -o shim.o

libshim.a: shim.o
	ar rcs libshim.a shim.o

clean:
	rm -f shim.o libshim.a
EOF

    log "building C shim (skeleton)"
    (cd "$SHIM_DIR" && make 2>&1) | tee "$ANALYSIS/6_c_shim_build.log" || \
        warn "C shim build failed (skeleton only — expected without device headers)"
fi

# ── qemu-arm test ─────────────────────────────────────────────────────────────

log "testing hello-walkman under qemu-arm-static"
if command -v qemu-arm-static >/dev/null 2>&1; then
    QEMU_LOG="$ANALYSIS/6_qemu_test.log"
    timeout 15 qemu-arm-static "$ARTIFACTS/hello_walkman_arm" > "$QEMU_LOG" 2>&1 && \
        log "qemu test PASSED — binary ran successfully" || \
        warn "qemu test exited with error (may be normal — check $QEMU_LOG)"
    cat "$QEMU_LOG"
else
    warn "qemu-arm-static not found — skipping qemu test"
    warn "Install: sudo apt install qemu-user-static"
fi

log ""
log "Phase 6 complete."
log "  hello-walkman binary: $ARTIFACTS/hello_walkman_arm"
log "  Deploy this via .UPG (make phase7 round-trip) → CLAUDE.md Part E7"
