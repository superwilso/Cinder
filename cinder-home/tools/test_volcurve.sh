#!/usr/bin/env bash
# test_volcurve.sh — the custom volume curve (src/vol_curve.h), built and run on the host. Pass
# Sony's ov_1291.tbl (or let it find the extracted firmware) to check the trailer as well.
set -eu
cd "$(dirname "$0")/../.."
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
${CC:-cc} -O1 -Wall -Werror -Icinder-home/src -o "$OUT/volcurve_selftest" cinder-home/tools/volcurve_selftest.c
TBL="${1:-artifacts/rootfs_mnt/usr/share/audio_dac/ov_1291.tbl}"
if [ -f "$TBL" ]; then "$OUT/volcurve_selftest" "$TBL"; else "$OUT/volcurve_selftest"; fi
