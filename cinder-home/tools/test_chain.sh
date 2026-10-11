#!/usr/bin/env bash
# Sandbox test for deploy/cinder-chain.sh (Clear Bass in Walkman One's effect chain). Sony's
# library is not ours to ship, so the script is pointed at a stand-in of the same size with the
# same byte at the same offset, and its two md5s are rewritten to the stand-in's. What is checked
# is the script's own logic: patch, verify, revert, and leaving an unknown library alone.
set -u
SP="$(mktemp -d "${TMPDIR:-/tmp}/cinder_chain_test.XXXXXX")"; trap 'rm -rf "$SP"' EXIT
SRC="$(cd "$(dirname "$0")/.." && pwd)/deploy/cinder-chain.sh"
PASS=0; FAIL=0
check() { if [ "$2" = "$3" ]; then printf '  ok    %-52s -> %s\n' "$1" "$2"; PASS=$((PASS+1))
  else printf '  FAIL  %-52s -> %s (want %s)\n' "$1" "$2" "$3"; FAIL=$((FAIL+1)); fi; }
md5() { md5sum < "$1" | cut -d' ' -f1; }

python3 - "$SP" <<'PY'
import sys
b = bytearray(3306868); b[456984] = 0xbe; open(sys.argv[1] + "/stock.so", "wb").write(b)
b[456984] = 0x8e; open(sys.argv[1] + "/cb.so", "wb").write(b)
PY
STOCK=$(md5 "$SP/stock.so"); CB=$(md5 "$SP/cb.so")
sed -e "s/^MD5_STOCK=[0-9a-f]*/MD5_STOCK=$STOCK/" -e "s/^MD5_CB=[0-9a-f]*/MD5_CB=$CB/" "$SRC" > "$SP/chain.sh"
grep -q "^MD5_STOCK=$STOCK" "$SP/chain.sh" && grep -q "^MD5_CB=$CB" "$SP/chain.sh" \
  || { echo "FAIL: the script's md5 lines changed shape; this test rewrote nothing"; exit 1; }
run() { CINDER_CHAIN_LIB="$SP/lib.so" sh "$SP/chain.sh" "$@" >"$SP/out" 2>&1; echo $?; }

cp "$SP/stock.so" "$SP/lib.so"
check "status on the default library"            "$(run status)" 0
check "set default on the default library"       "$(run set default)" 0
check "…changed nothing"                         "$(md5 "$SP/lib.so")" "$STOCK"
check "…and kept no backup it did not need"      "$([ -e "$SP/lib.so.stock" ] && echo yes || echo no)" no
check "set clearbass"                            "$(run set clearbass)" 0
check "…the live library is the patched one"     "$(md5 "$SP/lib.so")" "$CB"
check "…one byte differs from the original"      "$(cmp -l "$SP/lib.so" "$SP/stock.so" | wc -l | tr -d ' ')" 1
check "…the original is kept"                    "$(md5 "$SP/lib.so.stock")" "$STOCK"
check "set clearbass twice is a no-op"           "$(run set clearbass)" 0
check "set default undoes it"                    "$(run set default)" 0
check "…exactly"                                 "$(md5 "$SP/lib.so")" "$STOCK"
run set clearbass >/dev/null
check "revert undoes it too"                     "$(run revert)" 0
check "…exactly"                                 "$(md5 "$SP/lib.so")" "$STOCK"

# A library this script does not know: another firmware, or a truncated file.
echo junk >> "$SP/lib.so"; rm -f "$SP/lib.so.stock"; UNK=$(md5 "$SP/lib.so")
check "set clearbass refuses an unknown library" "$(run set clearbass)" 1
check "…and leaves it alone"                     "$(md5 "$SP/lib.so")" "$UNK"
check "set default is quiet on an unknown one"   "$(run set default)" 0
check "…and leaves it alone"                     "$(md5 "$SP/lib.so")" "$UNK"
check "status says it is unknown"                "$(run status)" 1
check "a bad verb is a usage error"              "$(run set vinyl)" 2

printf '\n%d passed, %d failed\n' "$PASS" "$FAIL"
[ "$FAIL" = 0 ]
