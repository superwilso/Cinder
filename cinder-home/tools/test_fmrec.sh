#!/usr/bin/env bash
# test_fmrec.sh — the FM recorder's WAV writer (src/fmrec.h), built and run on the host.
set -eu
cd "$(dirname "$0")/../.."
OUT="$(mktemp -d)"
trap 'rm -rf "$OUT"' EXIT
${CC:-cc} -O1 -Wall -Werror -Icinder-home/src -o "$OUT/fmrec_selftest" cinder-home/tools/fmrec_selftest.c
"$OUT/fmrec_selftest"
