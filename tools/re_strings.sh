#!/bin/bash
# One line per function: the strings it references. usage: tools/re_strings.sh <libname.so>
cd "$(dirname "$0")/.." || exit 1
# RE_FW=w1 reads Walkman One's binaries (artifacts/w1_rootfs) instead of stock 1.02 (artifacts/rootfs_mnt).
D=artifacts/decomp${RE_FW:+_$RE_FW}; R=artifacts/rootfs_mnt/vendor/sony; [ "${RE_FW:-}" = w1 ] && R=artifacts/w1_rootfs/sony
tools/re_resolve.py "$(ls $R/{lib,bin}/"$1" 2>/dev/null | head -1)" < "$D/$1.c" |
awk '/^\/\/ ========/{fn=$3" "$5; order[++n]=fn}
     /"/{l=$0; while(match(l,/"[^"]*"/)){s=substr(l,RSTART,RLENGTH); if(!((fn,s) in seen)){seen[fn,s]=1; out[fn]=out[fn]" "s}; l=substr(l,RSTART+RLENGTH)}}
     END{for(i=1;i<=n;i++) if(out[order[i]]!="") print order[i]":"out[order[i]]}'
