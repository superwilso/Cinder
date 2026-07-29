#!/usr/bin/env bash
# screenshot.sh — grab what is CURRENTLY on the Walkman's screen, as a PNG on the host.
#
# Three routes, tried in order (or force one with --route):
#
#   app    Ask the running cinder-home to save its own frame. Drops a trigger file, waits for the
#          PNG, pulls it. This is the ONLY faithful route when the GPU/EGL present path is active,
#          because under EGL the Mali swapchain owns the panel and /dev/graphics/fb0 no longer
#          reflects what's displayed. Requires cinder-home to be running and painting.
#   sony   Sony's own stock tool /system/vendor/sony/bin/screenshot (mmaps fb0, writes a timestamped
#          PNG via QImage). Works even when cinder isn't running — e.g. on the stock UI. Reads fb0,
#          so same GPU caveat as `raw`.
#   raw    Read /dev/graphics/fb0 directly and convert host-side. Last resort; needs no on-device
#          tooling at all, so it works on a wedged device. Same GPU caveat.
#
# Usage:  tools/screenshot.sh [out.png] [--route app|sony|raw|auto]
set -uo pipefail

OUT="${1:-}"; [ "${OUT:0:2}" = "--" ] && OUT=""
OUT="${OUT:-cinder_screen_$(date +%Y%m%d_%H%M%S).png}"
ROUTE="auto"
while [ $# -gt 0 ]; do [ "$1" = "--route" ] && { ROUTE="$2"; shift; }; shift; done

adb_up() { [ "$(adb get-state 2>/dev/null)" = "device" ]; }
adb_up || { echo "ERROR: no adb device. (Is the device booted into Cinder with dev-channel adb?)" >&2; exit 1; }

# ── route: app ────────────────────────────────────────────────────────────────────────────────
try_app() {
    # /tmp (tmpfs) is the primary path: it survives USB-MSC, which unmounts /contents and hands it
    # to the PC — an adb connection alone can trigger that, so a /contents screenshot can vanish
    # mid-pull. `ps | grep` because this busybox has no pidof/pgrep.
    adb shell 'ps 2>/dev/null | grep -q "[c]inder-home" && echo yes' 2>/dev/null | grep -q yes \
        || { echo "  (app: cinder-home not running)" >&2; return 1; }
    adb shell 'rm /tmp/cinder_screen.png 2>/dev/null; touch /tmp/cinder_screenshot.req' >/dev/null 2>&1
    # adb drops are common on this device (gadget re-enumeration); keep retrying rather than failing.
    for _ in $(seq 1 30); do
        sleep 0.5
        adb_up || { sleep 2; continue; }
        if adb shell '[ -f /tmp/cinder_screen.png ] && echo yes' 2>/dev/null | grep -q yes; then
            adb pull /tmp/cinder_screen.png "$OUT" >/dev/null 2>&1 && return 0
        fi
    done
    echo "  (app: timed out waiting for the frame)" >&2; return 1
}

# ── route: sony (stock on-device tool) ────────────────────────────────────────────────────────
try_sony() {
    local BIN=/system/vendor/sony/bin/screenshot
    adb shell "[ -x $BIN ] && echo yes" 2>/dev/null | grep -q yes || { echo "  (sony: tool absent)" >&2; return 1; }
    adb shell "rm -f /contents/screenshot_*.png 2>/dev/null; $BIN >/dev/null 2>&1; sync" >/dev/null 2>&1
    local f
    f=$(adb shell 'ls -1 /contents/screenshot_*.png 2>/dev/null | tail -1' 2>/dev/null | tr -d '\r')
    [ -n "$f" ] || { echo "  (sony: tool produced no file)" >&2; return 1; }
    adb pull "$f" "$OUT" >/dev/null 2>&1 || return 1
    adb shell "rm -f '$f'" >/dev/null 2>&1
    return 0
}

# ── route: raw (/dev/graphics/fb0) ────────────────────────────────────────────────────────────
# 480x800, 32bpp, stride 1920. Virtual height is 2400 (3 pages) but page 0 is the visible window
# and blit() writes the same frame to every page, so the first 1920*800 bytes are the whole picture.
# Bytes are BGRX (Canvas u32 0x00RRGGBB little-endian) -> reorder to RGB.
try_raw() {
    local tmp; tmp=$(mktemp)
    adb exec-out "dd if=/dev/graphics/fb0 bs=1920 count=800 2>/dev/null" > "$tmp" 2>/dev/null
    [ -s "$tmp" ] || { echo "  (raw: empty read)" >&2; rm -f "$tmp"; return 1; }
    python3 - "$tmp" "$OUT" <<'PY' || { rm -f "$tmp"; return 1; }
import sys, struct, zlib
raw, out = sys.argv[1], sys.argv[2]
W, H, STRIDE = 480, 800, 1920
data = open(raw, 'rb').read()
if len(data) < STRIDE * H:
    sys.exit("raw: short read (%d bytes)" % len(data))
rows = []
for y in range(H):
    row = data[y*STRIDE : y*STRIDE + W*4]
    # little-endian 0x00RRGGBB => bytes [B,G,R,X]; take R,G,B
    rows.append(b'\x00' + bytes(b for i in range(0, len(row), 4)
                                for b in (row[i+2], row[i+1], row[i])))
def chunk(tag, payload):
    c = struct.pack('>I', len(payload)) + tag + payload
    return c + struct.pack('>I', zlib.crc32(tag + payload) & 0xffffffff)
png = (b'\x89PNG\r\n\x1a\n'
       + chunk(b'IHDR', struct.pack('>IIBBBBB', W, H, 8, 2, 0, 0, 0))
       + chunk(b'IDAT', zlib.compress(b''.join(rows), 6))
       + chunk(b'IEND', b''))
open(out, 'wb').write(png)
PY
    rm -f "$tmp"; return 0
}

case "$ROUTE" in
    app)  try_app  && { echo "$OUT (route: app)";  exit 0; }; exit 1 ;;
    sony) try_sony && { echo "$OUT (route: sony)"; exit 0; }; exit 1 ;;
    raw)  try_raw  && { echo "$OUT (route: raw)";  exit 0; }; exit 1 ;;
esac

# auto: app first (only route that is correct under the GPU path), then the fb0-based fallbacks.
try_app  && { echo "$OUT (route: app)";  exit 0; }
try_sony && { echo "$OUT (route: sony — NOTE: reads fb0, may be stale under the GPU path)"; exit 0; }
try_raw  && { echo "$OUT (route: raw — NOTE: reads fb0, may be stale under the GPU path)";  exit 0; }
echo "ERROR: all screenshot routes failed." >&2; exit 1
