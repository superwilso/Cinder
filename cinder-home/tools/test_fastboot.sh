#!/bin/bash
# test_fastboot.sh — deploy/cinder-fastboot.sh on the host.
#
# Every install path and the skip script itself, against a scratch root: the script's absolute
# paths are rewritten into a temporary directory, busybox is a stub that hands each applet to the
# host's tool, and the pinned hash is replaced with that of a stand-in for Sony's script. The
# stand-in prints what Sony's three checks print when they have nothing to do (copied from a
# traced boot of an NW-A55, 2026-11-15) and leaves a mark each time it runs.
set -u
HERE="$(cd "$(dirname "$0")/.." && pwd)"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
pass=0; fail=0
check() { if [ "$2" = "$3" ]; then echo "  ok   $1"; pass=$((pass+1)); else echo "  FAIL $1 (got '$2', want '$3')"; fail=$((fail+1)); fi; }

mkdir -p "$T/bin"
printf '#!/bin/sh\ncmd="$1"; shift\ncase "$cmd" in chown) exit 0 ;; esac\nexec "$cmd" "$@"\n' > "$T/bin/busybox"
chmod +x "$T/bin/busybox"
export PATH="$T/bin:$PATH"

R=; F=
fresh() {   # a scratch player with Sony's script and firmware; $1 = what the checks print
    R="$T/r$RANDOM$RANDOM"
    mkdir -p "$R/system/usr/bin" "$R/system/etc/firmware" "$R/data/cinder" "$R/xbin"
    cp "$T/bin/busybox" "$R/xbin/busybox"
    cat > "$R/system/usr/bin/optproc_for_boot.sh" <<SONY
#!/bin/sh
echo ran >> $R/ran
echo "Touchpanel FW update: FW Update ${1:-Not Exec}"
echo "cxd3778gf ucom FW update: FW is latest"
echo "[ERROR] 11-15 22:47:07.922: ERR nfc_hal_fw_check_header: skip downloading. NVM " >&2
SONY
    for f in optproc_for_boot_1291.sh sony_tovis_updater.sh cxd3778gf_ucom_fwupdate.sh nfc_cxd225x_updater.sh; do echo "# $f" > "$R/system/usr/bin/$f"; done
    for f in Sony_Tovis_ICX1291_C21_2017-02-09.bin cxd3778gf_ucom_fw_00_16.bin cxd225x_firmware.bin; do echo "$f" > "$R/system/etc/firmware/$f"; done
    STOCK="$(sha256sum "$R/system/usr/bin/optproc_for_boot.sh" | cut -d' ' -f1)"
    F="$R/fastboot.sh"
    sed -e "s#/system/#$R/system/#g" -e "s#/data/#$R/data/#g" -e "s#/xbin/busybox#$R/xbin/busybox#g" \
        -e "s#^OPT_STOCK=.*#OPT_STOCK=$STOCK#" "$HERE/deploy/cinder-fastboot.sh" > "$F"
    cp "$R/system/usr/bin/optproc_for_boot.sh" "$R/sony.sh"
}
O() { echo "$R/system/usr/bin/optproc_for_boot.sh"; }
run() { sh "$F" "$@" >/dev/null 2>&1; }
boot() { sh "$(O)" > "$R/boot.out" 2>&1; }       # what init does at boot
runs() { [ -f "$R/ran" ] && wc -l < "$R/ran" | tr -d ' ' || echo 0; }
same() { cmp -s "$1" "$2" && echo same || echo differs; }
has() { [ -e "$1" ] && echo there || echo absent; }

echo "install / remove"
fresh; run install
check "install succeeds" "$?" 0
check "Sony's script is kept" "$(same "$(O).precinder" "$R/sony.sh")" same
check "the skip script is in its place" "$(grep -c cinder-fastboot "$(O)" | tr -d ' ' | sed 's/[1-9][0-9]*/yes/')" yes
check "it is executable" "$([ -x "$(O)" ] && echo yes)" yes
run install
check "installing again keeps it" "$(same "$(O).precinder" "$R/sony.sh")$(grep -q cinder-fastboot "$(O)" && echo ' ours')" "same ours"
run remove
check "remove puts Sony's script back" "$(same "$(O)" "$R/sony.sh")" same
check "and drops the kept copy" "$(has "$(O).precinder")" absent
run remove
check "remove when not installed does nothing" "$?$(same "$(O)" "$R/sony.sh")" 0same

fresh; echo "someone else's" >> "$(O)"; cp "$(O)" "$R/other.sh"; run install
check "an unknown script is not replaced" "$(same "$(O)" "$R/other.sh")" same
check "and nothing is kept" "$(has "$(O).precinder")" absent

fresh; run install; rm "$(O).precinder"; run remove
check "remove without Sony's copy leaves ours in place" "$(grep -q cinder-fastboot "$(O)" && echo ours)" ours

echo "the skip script at boot"
fresh; run install
boot; check "first boot runs Sony's checks" "$(runs)" 1
check "and keeps a stamp" "$(has "$R/data/cinder/fwcheck")" there
boot; check "second boot skips them" "$(runs)" 1
grep -q skipped "$R/boot.out"; check "and says so" "$?" 0
echo changed >> "$R/system/etc/firmware/cxd225x_firmware.bin"
boot; check "a changed firmware file runs them again" "$(runs)" 2
boot; check "then skips again" "$(runs)" 2
run remove
check "remove drops the stamp" "$(has "$R/data/cinder/fwcheck")" absent

fresh "OK"; run install   # the touch panel was updated: not "nothing to do"
boot; boot
check "an update keeps the checks on every boot" "$(runs)" 2
check "and no stamp is kept" "$(has "$R/data/cinder/fwcheck")" absent

echo "$pass passed, $fail failed"
[ "$fail" = 0 ]
