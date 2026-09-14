#!/usr/bin/env bash
# render_installer_screenshots.sh — the README's pictures of the Windows installer, from the real window.
#
#   tools/render_installer_screenshots.sh     build, render, recompress, copy into docs/screenshots
#
# WHY A REAL WINDOW. The installer is raw Win32 (installer/src/gui.rs): stock Windows controls in
# the system theme, which is most of how it looks. Nothing on Linux draws that faithfully, so this
# runs the .exe on the Windows that WSL sits on, through interop, in its --screenshots mode: a
# canned "Cinder is installed" player, no drive read, nothing written but the PNGs.
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
command -v python3 >/dev/null 2>&1 || { echo "needs python3 to recompress the images" >&2; exit 2; }

TARGET=x86_64-pc-windows-gnu
( cd installer && CINDER_INSTALLER_AS_INVOKER=1 cargo build --quiet --release --target "$TARGET" --target-dir target/shots ) \
    || { echo "could not build the installer for $TARGET" >&2; exit 2; }
EXE="installer/target/shots/$TARGET/release/cinder-installer.exe"

OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
"$EXE" --screenshots "$(wslpath -w "$OUT")" || { echo "the installer could not render its pages" >&2; exit 2; }

# The installer stores its pixels uncompressed (it has no dependencies to compress them with).
# Recompressing here is also a check on that encoder: zlib.decompress verifies the Adler-32, and a
# missing page raises.
mkdir -p docs/screenshots
python3 - "$OUT" docs/screenshots <<'PY' || { echo "could not recompress the screenshots" >&2; exit 2; }
import pathlib, struct, sys, zlib

src, dst = map(pathlib.Path, sys.argv[1:3])

def chunks(png):
    assert png[:8] == b"\x89PNG\r\n\x1a\n", "not a PNG"
    i = 8
    while i < len(png):
        (n,) = struct.unpack(">I", png[i:i + 4])
        yield png[i + 4:i + 8], png[i + 8:i + 8 + n]
        i += 12 + n

def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

for name in ("installer-home", "installer-options", "installer-confirm"):
    png = (src / f"{name}.png").read_bytes()
    parts = list(chunks(png))
    ihdr = next(d for k, d in parts if k == b"IHDR")
    pixels = zlib.decompress(b"".join(d for k, d in parts if k == b"IDAT"))
    out = png[:8] + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(pixels, 9)) + chunk(b"IEND", b"")
    (dst / f"{name}.png").write_bytes(out)
    w, h = struct.unpack(">II", ihdr[:8])
    print(f"  docs/screenshots/{name}.png  {w}x{h}  {len(out) // 1024} KB")
PY
