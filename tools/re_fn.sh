#!/bin/bash
# Print one decompiled function with its strings resolved.
# usage: tools/re_fn.sh [-s] <libname.so> <ghidra-address-or-name-regex>     (listing from tools/re_decompile.sh)
# -s: skeleton only (calls, strings, branches), which is what a first read of a large function needs.
cd "$(dirname "$0")/.." || exit 1
# RE_FW=w1 reads Walkman One's binaries (artifacts/w1_rootfs) instead of stock 1.02 (artifacts/rootfs_mnt).
D=artifacts/decomp${RE_FW:+_$RE_FW}; R=artifacts/rootfs_mnt/vendor/sony; [ "${RE_FW:-}" = w1 ] && R=artifacts/w1_rootfs/sony
skel() { cat; }
[ "$1" = -s ] && { shift; skel() { grep -E '^//|::[A-Za-z_~]+[<(]|"|\(\*\(code|[a-z_0-9]+\(.*\);$|^ *(if|switch|case|while|for|return)' | grep -v stack_chk; }; }
awk -v k="$2" '/^\/\/ ======== /{p = ($0 ~ k)} p' "$D/$1.c" |
    tools/re_resolve.py "$(ls $R/{lib,bin}/"$1" 2>/dev/null | head -1)" | skel
