#!/bin/bash
# test_preload.sh — deploy/cinder-preload.sh on the host.
#
# The script puts a wrapper in front of every Sony service and six lines into the boot script init
# waits on. Neither can be tried on a player without betting a boot on it, so every path is run
# here against a scratch root: the script's absolute paths are rewritten into a temporary directory
# (nothing else about it changes), /xbin/busybox is a stub that hands each applet to the host's
# tool of the same name, and the two pinned hashes are replaced with those of a stand-in boot
# script. Where Sony's own bootswitcher.sh is on this machine (artifacts/, never in the repository)
# the real hashes are checked against it as well.
set -u
HERE="$(cd "$(dirname "$0")/.." && pwd)"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
pass=0; fail=0
ok()  { echo "  ok   $1"; pass=$((pass+1)); }
bad() { echo "  FAIL $1"; fail=$((fail+1)); }
check() { if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (got '$2', want '$3')"; fi; }

# A stand-in for Sony's boot script: the same 54 lines in shape, "# set properties" at line 47.
standin() { for i in $(seq 1 46); do echo "line $i"; done; echo "# set properties"; for i in $(seq 48 54); do echo "line $i"; done; }
hook() { sed -n '/^hook_text() {/,/^HOOK$/p' "$HERE/deploy/cinder-preload.sh" | sed '1,2d;$d'; }
standin > "$T/bs.stock"
{ head -n 46 "$T/bs.stock"; hook; tail -n +47 "$T/bs.stock"; } > "$T/bs.hooked"
S_STOCK="$(sha256sum "$T/bs.stock" | cut -d' ' -f1)"; S_HOOKED="$(sha256sum "$T/bs.hooked" | cut -d' ' -f1)"

echo "sony hagodaemon" > "$T/sony"
printf 'wrapper one\ncinder-hagowrap: could not exec\n' > "$T/wrap1"
printf 'wrapper two\ncinder-hagowrap: could not exec\n' > "$T/wrap2"
echo "the shim" > "$T/shim"
cp "$HERE/deploy/cinder-guard.sh" "$T/guard"

R=; P=; OUT=
fresh() {   # a scratch player: Sony's hagodaemon, Sony's boot script, no Wampy
    R="$T/r$RANDOM$RANDOM"
    mkdir -p "$R/system/vendor/sony/bin" "$R/system/vendor/unknown321" "$R/system/bin" "$R/xbin" "$R/db" "$R/data/cinder"
    cat > "$R/xbin/busybox" <<EOF
#!/bin/sh
cmd="\$1"; shift
case "\$cmd" in chown) exit 0 ;; esac
# FAIL_CMP names a file whose comparison is made to fail: a copy that did not land whole.
if [ "\$cmd" = cmp ] && [ -n "\${FAIL_CMP:-}" ]; then case " \$* " in *"\$FAIL_CMP"*) exit 1 ;; esac; fi
exec "\$cmd" "\$@"
EOF
    chmod +x "$R/xbin/busybox"
    P="$R/preload.sh"
    sed -e "s#/system/#$R/system/#g" -e "s#/xbin/busybox#$R/xbin/busybox#g" \
        -e "s#^BS_STOCK=.*#BS_STOCK=$S_STOCK#" -e "s#^BS_HOOKED=.*#BS_HOOKED=$S_HOOKED#" \
        "$HERE/deploy/cinder-preload.sh" > "$P"
    # The hook names the guard by its real path; put that line back so the hash is the pinned one.
    sed -i "s#$R/system/bin/cinder-guard.sh#/system/bin/cinder-guard.sh#g; s#^GUARD=.*#GUARD=$R/system/bin/cinder-guard.sh#" "$P"
    cp "$T/sony" "$R/system/vendor/sony/bin/hagodaemon"
    cp "$T/bs.stock" "$R/system/bin/bootswitcher.sh"
}
H()  { echo "$R/system/vendor/sony/bin/hagodaemon"; }
BS() { echo "$R/system/bin/bootswitcher.sh"; }
SH() { echo "$R/system/vendor/unknown321/lib/libcinder_mono.so"; }
G()  { echo "$R/system/bin/cinder-guard.sh"; }
run() { OUT="$(sh "$P" "$@" 2>&1)"; }
same() { cmp -s "$1" "$2" && echo same || echo differs; }
has()  { [ -e "$1" ] && echo there || echo absent; }
install() { run install "${1:-$T/wrap1}" "$T/shim" "$T/guard"; }

echo "install"
fresh; install; rc=$?
check "exit status 0" "$rc" 0
check "the wrapper is in Sony's place" "$(same "$(H)" "$T/wrap1")" same
check "Sony's binary is kept beside it, byte for byte" "$(same "$(H).real" "$T/sony")" same
check "the shim is installed" "$(same "$(SH)" "$T/shim")" same
check "the guard is installed" "$(same "$(G)" "$T/guard")" same
check "the boot script is the patched one" "$(same "$(BS)" "$T/bs.hooked")" same
check "Sony's boot script is kept" "$(same "$(BS).precinder" "$T/bs.stock")" same
check "no temporary files are left" "$(find "$R/system" -name '*.tmp' | wc -l)" 0

install; install
check "installed again, twice: Sony's kept copy is still Sony's" "$(same "$(H).real" "$T/sony")" same
check "…and the boot script was not patched a second time" "$(same "$(BS)" "$T/bs.hooked")" same
install "$T/wrap2"
check "a newer wrapper replaces the old one" "$(same "$(H)" "$T/wrap2")" same
check "…and the kept copy is still Sony's, not the old wrapper" "$(same "$(H).real" "$T/sony")" same

echo "refusals: nothing may be half installed"
fresh; echo "extra line" >> "$(BS)"; cp "$(BS)" "$T/bs.other"; install; rc=$?
check "a boot script this does not know: exit 1" "$rc" 1
check "…Sony's hagodaemon untouched" "$(same "$(H)" "$T/sony")" same
check "…the boot script untouched" "$(same "$(BS)" "$T/bs.other")" same
check "…and nothing else written" "$(has "$(H).real")$(has "$(G)")$(has "$(SH)")" absentabsentabsent

fresh; mkdir -p "$R/system/vendor/unknown321/lib"; echo wampy > "$R/system/vendor/unknown321/lib/libsound_service_fw.so"; install; rc=$?
check "Wampy is installed: nothing to do, exit 0" "$rc$(has "$(H).real")$(has "$(G)")" 0absentabsent
check "…the boot script is still Sony's" "$(same "$(BS)" "$T/bs.stock")" same

fresh; : > "$T/empty"; run install "$T/empty" "$T/shim" "$T/guard"; rc=$?
check "a staged file is empty: exit 1, nothing written" "$rc$(has "$(H).real")$(has "$(G)")" 1absentabsent
fresh; run install "$T/sony" "$T/shim" "$T/guard"; rc=$?
check "the staged wrapper is not a wrapper: exit 1, nothing written" "$rc$(has "$(H).real")$(has "$(G)")" 1absentabsent

fresh; cp "$T/wrap1" "$(H)"; install "$T/wrap2"; rc=$?
check "a wrapper with no Sony binary beside it: refused" "$rc$(same "$(H)" "$T/wrap1")" 1same
fresh; cp "$T/wrap1" "$(H)"; cp "$T/wrap1" "$(H).real"; install "$T/wrap2"; rc=$?
check "a kept copy that is itself a wrapper: refused" "$rc$(same "$(H).real" "$T/wrap1")" 1same

fresh; FAIL_CMP="hagodaemon.tmp" install; rc=$?
check "the wrapper copy does not land whole: exit 1" "$rc" 1
check "…and Sony's hagodaemon still runs" "$(same "$(H)" "$T/sony")" same
fresh; FAIL_CMP="hagodaemon.real.tmp" install; rc=$?
check "Sony's binary cannot be kept: the wrapper is not installed" "$rc$(same "$(H)" "$T/sony")$(has "$(H).real")" 1sameabsent
fresh; FAIL_CMP="libcinder_mono.so.tmp" install; rc=$?
check "the shim does not land: the wrapper is not installed" "$rc$(same "$(H)" "$T/sony")$(has "$(H).real")" 1sameabsent
fresh; FAIL_CMP="cinder-guard.sh.tmp" install; rc=$?
check "the guard does not land: nothing else is touched" "$rc$(same "$(H)" "$T/sony")$(same "$(BS)" "$T/bs.stock")" 1samesame

echo "remove"
fresh; install; run remove; rc=$?
check "exit status 0" "$rc" 0
check "Sony's hagodaemon is back" "$(same "$(H)" "$T/sony")" same
check "the kept copy, shim, guard and kept boot script are gone" "$(has "$(H).real")$(has "$(SH)")$(has "$(G)")$(has "$(BS).precinder")" absentabsentabsentabsent
check "the boot script is Sony's again" "$(same "$(BS)" "$T/bs.stock")" same
check "no temporary files are left" "$(find "$R/system" -name '*.tmp' | wc -l)" 0

fresh; run remove; rc=$?
check "nothing installed: remove changes nothing" "$rc$(same "$(H)" "$T/sony")$(same "$(BS)" "$T/bs.stock")" 0samesame

fresh; install; rm "$(BS).precinder"; run remove
check "no kept boot script: the wrapper still comes off" "$(same "$(H)" "$T/sony")" same
check "…the hook stays, and so does the guard it calls" "$(same "$(BS)" "$T/bs.hooked")$(has "$(G)")" samethere

fresh; install; FAIL_CMP="hagodaemon.tmp" run remove; rc=$?
check "Sony's binary cannot be put back: exit 1, the wrapper and its guard stay" "$rc$(same "$(H)" "$T/wrap1")$(has "$(G)")$(same "$(BS)" "$T/bs.hooked")" 1sametheresame

echo "with the guard"
fresh; install
GS="$R/guard.sh"
sed -e "s#/db/cinder-guard#$R/db/cinder-guard#g" -e "s#/system/#$R/system/#g" \
    -e "s#/data/cinder#$R/data/cinder#g" -e "s#/xbin/busybox#$R/xbin/busybox#g" "$(G)" > "$GS"
mkdir -p "$T/stub"; printf '#!/bin/sh\nexit 0\n' > "$T/stub/mount"; cp "$T/stub/mount" "$T/stub/sync"; chmod +x "$T/stub/mount" "$T/stub/sync"
PATH="$T/stub:$PATH" sh "$GS" >/dev/null 2>&1; PATH="$T/stub:$PATH" sh "$GS" >/dev/null 2>&1
check "installed, then two boots that prove nothing: the guard puts Sony's binary back" "$(same "$(H)" "$T/sony")" same
run status
check "…and status says so" "$(echo "$OUT" | grep -c 'wrapper: not installed')" 1
install
check "…and it can be installed again afterwards" "$(same "$(H)" "$T/wrap1")$(same "$(H).real" "$T/sony")" samesame

REAL_BS="$HERE/../artifacts/rootfs_mnt/bin/bootswitcher.sh"
if [ -f "$REAL_BS" ]; then
    echo "Sony's own bootswitcher.sh"
    W_STOCK="$(sed -n 's/^BS_STOCK=//p' "$HERE/deploy/cinder-preload.sh")"
    W_HOOKED="$(sed -n 's/^BS_HOOKED=//p' "$HERE/deploy/cinder-preload.sh")"
    W_SPLIT="$(sed -n 's/^BS_SPLIT=\([0-9]*\).*/\1/p' "$HERE/deploy/cinder-preload.sh")"
    check "the pinned hash is Sony's file" "$(sha256sum "$REAL_BS" | cut -d' ' -f1)" "$W_STOCK"
    check "Sony's file plus the hook is the pinned patched file" \
        "$({ head -n "$W_SPLIT" "$REAL_BS"; hook; tail -n +$((W_SPLIT + 1)) "$REAL_BS"; } | sha256sum | cut -d' ' -f1)" "$W_HOOKED"
    check "…and the patched file parses" "$({ head -n "$W_SPLIT" "$REAL_BS"; hook; tail -n +$((W_SPLIT + 1)) "$REAL_BS"; } | sh -n 2>&1 | wc -l)" 0
else
    echo "(Sony's bootswitcher.sh is not on this machine: the pinned hashes are not re-checked here)"
fi

echo; echo "preload: $pass ok, $fail FAIL"
[ "$fail" -eq 0 ]
