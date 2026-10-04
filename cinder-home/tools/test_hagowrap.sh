#!/bin/bash
# test_hagowrap.sh — src/cinder-hagowrap.c on the host.
#
# The wrapper stands in front of every Sony service on a player that has it, so the rule it is held
# to is the launcher's: whatever it decides, Sony's binary runs with the arguments it was given.
# Built natively with its four paths pointed into a scratch directory; the "real" hagodaemon is a
# script that prints its arguments and its LD_PRELOAD.
set -u
HERE="$(cd "$(dirname "$0")/.." && pwd)"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
pass=0; fail=0
ok()  { echo "  ok   $1"; pass=$((pass+1)); }
bad() { echo "  FAIL $1"; fail=$((fail+1)); }

cat > "$T/real" <<'R'
#!/bin/sh
echo "args=$*"
echo "preload=${LD_PRELOAD-unset}"
R
chmod +x "$T/real"
build() {   # build <out> <real>
    ${CC:-cc} -Os -Wall -Wextra -Werror -o "$1" "$HERE/src/cinder-hagowrap.c" \
        -DHAGO_REAL="\"$2\"" -DHAGO_SHIM="\"$T/shim.so\"" \
        -DHAGO_OFF="\"$T/off\"" -DHAGO_OFF_MSC="\"$T/off_msc\"" || { echo "BUILD FAILED"; exit 1; }
}
build "$T/wrap" "$T/real"
reset() { rm -f "$T/off" "$T/off_msc"; echo shim > "$T/shim.so"; }
run()   { env -u LD_PRELOAD "$@" 2>&1; }

reset
out=$(run "$T/wrap" SoundServiceFw a b)
[[ "$out" == *"args=SoundServiceFw a b"* ]] && ok "arguments reach Sony's binary unchanged" || bad "arguments changed: $out"
[[ "$out" == *"preload=$T/shim.so"* ]] && ok "SoundServiceFw gets the preload" || bad "no preload for SoundServiceFw: $out"

out=$(run "$T/wrap" PlayerService x)
[[ "$out" == *"preload=unset"* && "$out" == *"args=PlayerService x"* ]] && ok "any other service is a plain pass-through" || bad "another service was touched: $out"

out=$(run "$T/wrap")
[[ "$out" == *"preload=unset"* ]] && ok "no arguments at all: pass-through" || bad "no-argument case: $out"

touch "$T/off"; out=$(run "$T/wrap" SoundServiceFw)
[[ "$out" == *"preload=unset"* ]] && ok "the persistent off switch" || bad "off switch ignored: $out"
reset; touch "$T/off_msc"; out=$(run "$T/wrap" SoundServiceFw)
[[ "$out" == *"preload=unset"* ]] && ok "the USB-settable off switch" || bad "MSC off switch ignored: $out"

reset; rm -f "$T/shim.so"; out=$(run "$T/wrap" SoundServiceFw)
[[ "$out" == *"preload=unset"* ]] && ok "shim missing: pass-through" || bad "preload set with no shim: $out"
: > "$T/shim.so"; out=$(run "$T/wrap" SoundServiceFw)
[[ "$out" == *"preload=unset"* ]] && ok "shim empty: pass-through" || bad "preload set for an empty shim: $out"

reset; out=$(LD_PRELOAD="" env LD_PRELOAD=/else.so "$T/wrap" SoundServiceFw 2>&1 | grep '^preload=')
[[ "$out" == "preload=/else.so $T/shim.so" ]] && ok "an existing preload is kept, ours after it" || bad "existing preload: $out"
out=$(env LD_PRELOAD="$T/shim.so" "$T/wrap" SoundServiceFw 2>&1 | grep '^preload=')
[[ "$out" == "preload=$T/shim.so" ]] && ok "already named: not added twice" || bad "added twice: $out"

build "$T/wrap_noreal" "$T/does-not-exist"
out=$(run "$T/wrap_noreal" SoundServiceFw); rc=$?
[[ $rc -eq 127 && "$out" == *"could not exec"* ]] && ok "Sony's binary missing: says so, exit 127" || bad "missing real: rc=$rc $out"

echo; echo "hagowrap: $pass ok, $fail FAIL"
[ "$fail" -eq 0 ]
