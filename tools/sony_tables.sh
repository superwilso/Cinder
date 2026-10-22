#!/usr/bin/env bash
# sony_tables.sh — fetch the Smooth (NW-WM1A) volume-curve tables from Sony's own firmware.
#
#   tools/sony_tables.sh [DEST]     DEST defaults to ./sony_tables; give the player's drive
#                                   (e.g. /mnt/e) to put them where the installer looks.
#
# The `wm1a` and `w1` volume curves (install.md, "The volume curve tables") need three of Sony's
# files that the NW-A50 firmware does not carry. They are in Sony's NW-WM1 series firmware, which
# Sony serves from the same update server as the A50's. This downloads that package to a temporary
# folder, unpacks it with 7z and upgtool (tools/cloud_setup.sh installs both), and copies out only
# the tables whose SHA-256 is the one install_cinderhome.sh accepts. Nothing of Sony's is shipped
# with Cinder: the bytes come from Sony, to the user's own PC.
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"
DEST="${1:-./sony_tables}"
URL=https://walkman.update.sony.net/fw/pc/WM1/J/NW-WM1_V3_02.dmg
UPG_SHA=314a500589aad3eb45177cf97abd1e964ae30338badd6f9d9f3668e4bc2ec4a2   # NW-WM1 3.02
# The same three hashes install_cinderhome.sh checks before a table is installed.
TABLES="ov_127x.tbl:b5dd878b0484c43312f3a93c6675d40d546f78ef7494684991eea0aef8651320
ov_dsd_127x.tbl:b777b7e2786f952797e82830d5d17565d03a4758555436a376bf6daff5e50ac7
ov_1280.tbl:0902981b0b00b5f98eb04d00713f4ddc96f4bb91af76289d8cce15486c30a9c7"

UPGTOOL="$REPO/artifacts/upgtool"
command -v 7z >/dev/null || { echo "needs 7z (apt install p7zip-full)"; exit 1; }
[ -x "$UPGTOOL" ] || { echo "needs $UPGTOOL — run tools/cloud_setup.sh once"; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "downloading Sony's NW-WM1 firmware 3.02 (119 MB)…"
curl -fsSL -o "$WORK/fw.dmg" "$URL"
7z e -y -o"$WORK" "$WORK/fw.dmg" '*/NW_WM_FW.UPG' -r >/dev/null
echo "$UPG_SHA  $WORK/NW_WM_FW.UPG" | sha256sum -c --quiet

# Sector 6 is the root filesystem (ext4 once decompressed); the tables are in usr/share/audio_dac.
"$UPGTOOL" -e -m nw-wm1a -z 6 -o "$WORK/" "$WORK/NW_WM_FW.UPG" >/dev/null
7z e -y -o"$WORK/audio_dac" "$WORK/6.bin" 'usr/share/audio_dac/ov_*.tbl' >/dev/null

mkdir -p "$DEST"
while IFS=: read -r name sha; do
    echo "$sha  $WORK/audio_dac/$name" | sha256sum -c --quiet
    cp "$WORK/audio_dac/$name" "$DEST/$name"
    echo "  $DEST/$name  (SHA-256 is Sony's)"
done <<< "$TABLES"
echo "done. On the player's drive they sit next to MUSIC; choose Smooth (NW-WM1A) and install."
