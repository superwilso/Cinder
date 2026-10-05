#!/usr/bin/env bash
# render_installer_screenshots.sh — the README's pictures of the Windows installer, from the real window.
#
#   tools/render_installer_screenshots.sh     build, render, recompress, copy into docs/screenshots
#
# WHY A REAL WINDOW. The installer is raw Win32 (installer/src/gui.rs): stock Windows controls in
# the system theme, which is most of how it looks. Nothing on Linux draws that faithfully, so this
# runs the .exe on the Windows that WSL sits on, through interop, in its --screenshots mode: a
# canned "Cinder is installed" player, no drive read, nothing written but the images.
#
# The build is a separate one (its own target dir) with CINDER_INSTALLER_AS_INVOKER set, because
# Windows refuses to start the real requireAdministrator .exe from WSL. See installer/build.rs.
#
# NO --check, unlike render_screenshots.sh: the pixels depend on the Windows doing the drawing
# (version, theme, display scaling, fonts), so two machines never agree byte for byte.
#
# Exit codes: 0 rendered, 2 could not build or render, 3 not on WSL with Windows interop.
set -uo pipefail
cd "$(dirname "$0")/.." || { echo "cannot reach the repo root" >&2; exit 2; }

command -v powershell.exe >/dev/null 2>&1 && command -v wslpath >/dev/null 2>&1 \
    || { echo "needs WSL with Windows interop — the installer window is Win32" >&2; exit 3; }
command -v python3 >/dev/null 2>&1 || { echo "needs python3 to write the PNGs" >&2; exit 2; }

TARGET=x86_64-pc-windows-gnu
( cd installer && CINDER_INSTALLER_AS_INVOKER=1 cargo build --quiet --release --target "$TARGET" --target-dir target/shots ) \
    || { echo "could not build the installer for $TARGET" >&2; exit 2; }
EXE="installer/target/shots/$TARGET/release/cinder-installer.exe"

OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
"$EXE" --screenshots "$(wslpath -w "$OUT")" || { echo "the installer could not render its pages" >&2; exit 2; }

# The installer writes PPM (it has no dependencies to compress with); this turns each into a PNG.
mkdir -p docs/screenshots
python3 - "$OUT" docs/screenshots <<'PY' || { echo "could not convert the screenshots" >&2; exit 2; }
import pathlib, struct, sys, zlib

src, dst = map(pathlib.Path, sys.argv[1:3])

def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

for name in ("installer-home", "installer-options", "installer-confirm"):
    magic, size, maxval, rgb = (src / f"{name}.ppm").read_bytes().split(b"\n", 3)  # gui.rs's header
    w, h = map(int, size.split())
    assert magic == b"P6" and maxval == b"255" and len(rgb) == w * h * 3, f"{name}.ppm is malformed"
    rows = b"".join(b"\0" + rgb[y * w * 3:(y + 1) * w * 3] for y in range(h))
    ihdr = struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)
    out = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(rows, 9)) + chunk(b"IEND", b"")
    (dst / f"{name}.png").write_bytes(out)
    print(f"  docs/screenshots/{name}.png  {w}x{h}  {len(out) // 1024} KB")
PY
