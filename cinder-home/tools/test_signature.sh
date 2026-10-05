#!/usr/bin/env bash
# Sandbox test for deploy/cinder-signature.sh. Sony's library is not ours to ship, so the script
# is pointed at a stand-in of the same size with the same four digits at the same offsets, and its
# md5 table is rewritten to the stand-in's. What is checked is the script's own logic: patch,
# verify, revert, and the Walkman One copy that W1's boot script reloads at every boot.
set -u
SP="$(mktemp -d "${TMPDIR:-/tmp}/cinder_sig_test.XXXXXX")"; trap 'rm -rf "$SP"' EXIT
SRC="$(cd "$(dirname "$0")/.." && pwd)/deploy/cinder-signature.sh"
PASS=0; FAIL=0
check() { if [ "$2" = "$3" ]; then printf '  ok    %-56s -> %s\n' "$1" "$2"; PASS=$((PASS+1))
  else printf '  FAIL  %-56s -> %s (want %s)\n' "$1" "$2" "$3"; FAIL=$((FAIL+1)); fi; }

# variant -> the stand-in with that variant's bytes, as edits_for describes them
mk() { python3 - "$1" "$2" <<'PY'
import sys
b = bytearray(155068); b[139610] = 0x30; b[139617] = 0x34; b[139946] = 0x30; b[139947] = 0x34
v = sys.argv[2]
if v in ("pv1", "hw1"): b[139617] = 0x30
if v in ("pv2", "hw2"): b[139610] = 0x34
if v in ("pv1", "pv2", "clock"): b[139946] = 0x33; b[139947] = 0x30
open(sys.argv[1], "wb").write(b)
PY
}
SED=()
for v in stock pv1 pv2 clock hw1 hw2; do
  mk "$SP/$v.so" "$v"
  real="$(sed -n "s/^ *$v) *echo \([0-9a-f]\{32\}\) ;;$/\1/p" "$SRC")"
  SED+=(-e "s/$real/$(md5sum < "$SP/$v.so" | cut -d' ' -f1)/g")
done
md5() { md5sum < "$1" | cut -d' ' -f1; }

# $1 = 1 for a Walkman One player (the normal_nt copy exists)
fresh() {
  R="$(mktemp -d "$SP/r.XXXXXX")"; mkdir -p "$R/lib" "$R/nt"
  cp "$SP/stock.so" "$R/lib/libaudiohal-adleralsa.so"
  [ "$1" = 1 ] && cp "$SP/stock.so" "$R/nt/libaudiohal-adleralsa.so"
  sed "${SED[@]}" -e "s#^LIB=.*#LIB=$R/lib/libaudiohal-adleralsa.so#" \
      -e "s#^W1_NT=.*#W1_NT=$R/nt/libaudiohal-adleralsa.so#" -e 's#^BB=/xbin/busybox#BB=#' \
      -e 's#^remount_rw() .*#remount_rw() { :; }#' "$SRC" > "$R/sig.sh"
}
LIVE() { md5 "$R/lib/libaudiohal-adleralsa.so"; }
NT()   { md5 "$R/nt/libaudiohal-adleralsa.so"; }

echo "── stock firmware: no Walkman One copy ──"
fresh 0
sh "$R/sig.sh" set pv2 >/dev/null;  check "set pv2 patches the live library"   "$(LIVE)" "$(md5 "$SP/pv2.so")"
check "the pristine library is kept as .stock"   "$(md5 "$R/lib/libaudiohal-adleralsa.so.stock")" "$(md5 "$SP/stock.so")"
check "nothing is created where W1 would keep its copy" "$(ls "$R/nt" | wc -l)" "0"
sh "$R/sig.sh" set hw1 >/dev/null;  check "a second set starts from the pristine copy" "$(LIVE)" "$(md5 "$SP/hw1.so")"
sh "$R/sig.sh" revert >/dev/null;   check "revert restores stock"              "$(LIVE)" "$(md5 "$SP/stock.so")"

echo "── Walkman One, no tuning: its boot script reloads normal_nt every boot ──"
fresh 1
sh "$R/sig.sh" set pv2 >/dev/null
check "set pv2 patches the live library"        "$(LIVE)" "$(md5 "$SP/pv2.so")"
check "…and the copy W1 reloads"                "$(NT)"   "$(md5 "$SP/pv2.so")"
cp "$R/nt/libaudiohal-adleralsa.so" "$R/lib/libaudiohal-adleralsa.so"   # what W1's boot does
check "after W1's boot copy the choice is still in force" "$(LIVE)" "$(md5 "$SP/pv2.so")"
cp "$SP/stock.so" "$R/nt/libaudiohal-adleralsa.so"                      # a W1 reinstall
sh "$R/sig.sh" set pv2 >/dev/null;  check "'already pv2' still repairs W1's copy" "$(NT)" "$(md5 "$SP/pv2.so")"
sh "$R/sig.sh" revert >/dev/null
check "revert restores the live library"        "$(LIVE)" "$(md5 "$SP/stock.so")"
check "…and W1's copy"                          "$(NT)"   "$(md5 "$SP/stock.so")"

echo "── a W1 copy this script does not recognise is left alone ──"
fresh 1; echo junk > "$R/nt/libaudiohal-adleralsa.so"
o="$(sh "$R/sig.sh" set pv1)"
check "the live library is still set"           "$(LIVE)" "$(md5 "$SP/pv1.so")"
check "W1's unknown copy is untouched"          "$(cat "$R/nt/libaudiohal-adleralsa.so")" "junk"
check "and it says so"                          "$(echo "$o" | grep -c 'WARN — Walkman One')" "1"

echo "── an unrecognised live library is never patched ──"
fresh 1; echo junk > "$R/lib/libaudiohal-adleralsa.so"
sh "$R/sig.sh" set pv2 >/dev/null; check "refused, exit status" "$?" "1"
check "W1's copy untouched too"                 "$(NT)" "$(md5 "$SP/stock.so")"

echo; printf '%s passed, %s failed\n' "$PASS" "$FAIL"; [ "$FAIL" = 0 ]
