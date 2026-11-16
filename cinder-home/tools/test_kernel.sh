#!/usr/bin/env bash
# test_kernel.sh — host test of the `kernel` option: src/cinder-bootimg.c against kernel/bootimg.py,
# and deploy/cinder-kernel.sh against a file standing in for the boot partition.
set -u
HERE="$(cd "$(dirname "$0")/.." && pwd)"
REPO="$(cd "$HERE/.." && pwd)"
T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
pass=0; fail=0
check() { if [ "$2" = "$3" ]; then echo "  ok   $1"; pass=$((pass+1)); else echo "  FAIL $1 (got '$2', want '$3')"; fail=$((fail+1)); fi; }

cc -O2 -Wall -Wextra -o "$T/cinder-bootimg" "$HERE/src/cinder-bootimg.c" || { echo "cinder-bootimg did not build"; exit 1; }

# A Sony-shaped image (random kernel and ramdisk), a new zImage, and bootimg.py's image of the two.
python3 - "$REPO/kernel" "$T" <<'EOF'
import os, struct, sys
sys.path.insert(0, sys.argv[1])
import bootimg
t = sys.argv[2]
def zimage(n):
    z = bytearray(os.urandom(n))
    z[0x24:0x28] = struct.pack("<I", 0x016F2818)
    return bytes(z)
old, new, rd = zimage(300001), zimage(412345), os.urandom(123457)
open(f"{t}/sony.img", "wb").write(bootimg.pack(old, rd))
open(f"{t}/zImage", "wb").write(new)
open(f"{t}/want.img", "wb").write(bootimg.pack(new, rd, name="cinder"))
open(f"{t}/notz", "wb").write(os.urandom(5000))
EOF

H="$T/cinder-bootimg"
echo "== cinder-bootimg"
check "a Sony image is not marked" "$("$H" info "$T/sony.img" | sed 's/.*cinder=//')" 0
"$H" swap "$T/sony.img" "$T/zImage" "$T/got.img"
check "swap gives bootimg.py's image, byte for byte" "$(cmp -s "$T/got.img" "$T/want.img" && echo same)" same
check "the new image is marked" "$("$H" info "$T/got.img" | sed 's/.*cinder=//')" 1
check "a file that is not a zImage is refused" "$("$H" swap "$T/sony.img" "$T/notz" "$T/x.img" 2>/dev/null; echo $?)" 1
check "a file that is not a boot image is refused" "$("$H" info "$T/notz" >/dev/null 2>&1; echo $?)" 1
head -c 16777216 /dev/zero > "$T/dev"; dd if="$T/sony.img" of="$T/dev" conv=notrunc 2>/dev/null
check "same: a partition starts with its own image" "$("$H" same "$T/dev" "$T/sony.img"; echo $?)" 0
check "same: and not with another" "$("$H" same "$T/dev" "$T/want.img"; echo $?)" 1
"$H" copy "$T/dev" "$T/copy.img"
check "copy takes the image's own length" "$(cmp -s "$T/copy.img" "$T/sony.img" && echo same)" same

echo "== cinder-kernel.sh"
printf '#!/bin/sh\nexec "$@"\n' > "$T/bb"; chmod +x "$T/bb"
run() { BB="$T/bb" CINDER_BOOTIMG="$H" CINDER_BOOT_DEV="$T/dev" CINDER_BOOT_SAVED="$T/saved.img" \
        CINDER_BOOT_NEW="$T/new.img" sh "$HERE/deploy/cinder-kernel.sh" "$@"; }
check "status: Sony's in place" "$(run status | sed -n 's/^in place: //p')" sony
run install "$T/zImage" >/dev/null
check "install: the original is saved" "$(cmp -s "$T/saved.img" "$T/sony.img" && echo same)" same
check "install: the partition holds Cinder's image" "$("$H" same "$T/dev" "$T/want.img"; echo $?)" 0
check "install: the work file is gone" "$([ -e "$T/new.img" ] && echo left || echo gone)" gone
check "install again: nothing to write" "$(run install "$T/zImage")" "kernel: Cinder's kernel is already in place"
run remove >/dev/null
check "remove: the original is back" "$("$H" same "$T/dev" "$T/sony.img"; echo $?)" 0
check "remove again: nothing to do" "$(run remove)" "kernel: Cinder's kernel is not in place — nothing to do"
run install "$T/zImage" >/dev/null
rm "$T/saved.img"
check "no saved original: install refuses" "$(run install "$T/zImage" >/dev/null; echo $?)" 1
check "no saved original: remove refuses" "$(run remove >/dev/null; echo $?)" 1
check "...and Cinder's image stays" "$("$H" same "$T/dev" "$T/want.img"; echo $?)" 0
cp "$T/want.img" "$T/saved.img"
check "a saved image that is Cinder's own is not a way back" "$(run remove >/dev/null; echo $?)" 1
head -c 16777216 /dev/zero > "$T/dev"
rm -f "$T/saved.img"
check "an empty partition: install refuses" "$(run install "$T/zImage" >/dev/null; echo $?)" 1
check "...and writes nothing" "$(cmp -s "$T/dev" <(head -c 16777216 /dev/zero) && echo untouched)" untouched

echo "$pass passed, $fail failed"
[ "$fail" = 0 ]
