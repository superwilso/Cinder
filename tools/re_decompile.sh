#!/bin/bash
# Decompile Sony libraries with Ghidra, one project per binary, kept under artifacts/ (not tracked).
# Usage: tools/re_decompile.sh <binary>...     -> artifacts/decomp/<name>.c
#        RE_FW=w1 tools/re_decompile.sh ...    -> artifacts/decomp_w1/ (Walkman One's copy; see tools/re_fn.sh)
# Re-running skips a binary whose listing already exists.
set -u
cd "$(dirname "$0")/.." || exit 1
G=~/ghidra/support/analyzeHeadless
S=$PWD/analysis/E_usbdac_ldac/ghidra
D=artifacts/decomp${RE_FW:+_$RE_FW}; P=artifacts/ghidra_fw${RE_FW:+_$RE_FW}
mkdir -p $D $P
for b in "$@"; do
    n=$(basename "$b")
    [ -s "$D/$n.c" ] && continue
    "$G" $P "$n" -import "$b" -overwrite -scriptPath "$S" \
        -postScript DecompileAll.java "$PWD/$D/$n.c" > "$D/$n.log" 2>&1 \
        || echo "FAIL $n"
done
