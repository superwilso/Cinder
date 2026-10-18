#!/bin/sh
# cinder-chain.sh — put Sony's Clear Bass six-band EQ back into Walkman One's effect chain.
#
# TRIAL (2026-10-11). Walkman One 3.02's libSoundServiceFw.so still carries the Clear Bass module,
# but its chain is a fixed list of eleven filters with no `eq6band` in it. Each entry is copied from
# a name through one PC-relative literal, so one byte points the `dcphaselinear` entry at `eq6band`
# instead: Clear Bass in, DC Phase Linearizer out. Derivation: analysis/RE_clear_bass.md section 6.
#
# Same safety model as cinder-signature.sh: only a library with a known md5 is touched, the
# original is kept as .stock, the patch is made on a copy that must hash to the expected result
# before it is moved into place, and `revert` restores the kept file. Takes effect at the next
# boot; nothing here restarts the sound service (stopping it reboots the player).
#
# USAGE:  cinder-chain.sh status | set clearbass | set default | revert
#         (`set default` is `revert` that stays quiet on a library it does not know: the
#         installer passes its option straight through on every player, Walkman One or not.)
set -u
LIB=${CINDER_CHAIN_LIB:-/system/vendor/sony/lib/libSoundServiceFw.so}
BAK=$LIB.stock
TMP=$LIB.new
MD5_STOCK=845e27e99efa57d30dde4800660a0add    # Walkman One 3.02
MD5_CB=6f5948825cc4f09f1a4a5fe1886a8f7e       # byte 456984 (0x6f918): 0xbe -> 0x8e
OFF=456984

BB=/xbin/busybox
[ -x "$BB" ] || BB=""
bb() { if [ -n "$BB" ]; then "$BB" "$@"; else "$@"; fi; }
md5() { bb md5sum "$1" 2>/dev/null | bb cut -d' ' -f1; }
place() {   # $1 = verified file to move over the live library
    bb chmod 755 "$1"; bb chown root:shell "$1" 2>/dev/null
    mv "$1" "$LIB" && sync
}

case "${1:-}" in
status)
    case "$(md5 "$LIB")" in
        "$MD5_STOCK") echo "chain: Walkman One default (DC Phase Linearizer, no Clear Bass)" ;;
        "$MD5_CB")    echo "chain: Clear Bass in place of DC Phase Linearizer" ;;
        *)            echo "chain: UNRECOGNISED library $(md5 "$LIB"); this script will not touch it"; exit 1 ;;
    esac ;;
set)
    if [ "${2:-}" = default ]; then
        case "$(md5 "$LIB")" in
            "$MD5_CB") exec sh "$0" revert ;;
            *) echo "chain: default (nothing to change)"; exit 0 ;;
        esac
    fi
    [ "${2:-}" = clearbass ] || { echo "chain: usage: set clearbass|default"; exit 2; }
    cur=$(md5 "$LIB")
    [ "$cur" = "$MD5_CB" ] && { echo "chain: already set"; exit 0; }
    [ "$cur" = "$MD5_STOCK" ] || { echo "chain: FAIL, library $cur is not Walkman One 3.02; not patching"; exit 1; }
    mount -o rw,remount /system 2>/dev/null
    bb cat "$LIB" > "$BAK" && [ "$(md5 "$BAK")" = "$MD5_STOCK" ] || { echo "chain: FAIL, backup did not verify"; bb rm -f "$BAK"; exit 1; }
    bb cat "$BAK" > "$TMP" && printf '\216' | bb dd of="$TMP" bs=1 seek=$OFF conv=notrunc 2>/dev/null
    [ "$(md5 "$TMP")" = "$MD5_CB" ] || { echo "chain: FAIL, patched copy did not verify; nothing changed"; bb rm -f "$TMP"; exit 1; }
    place "$TMP" || { echo "chain: FAIL, could not move the copy into place"; exit 1; }
    echo "chain: Clear Bass set. Reboot to load it. Original kept at $BAK" ;;
revert)
    [ "$(md5 "$LIB")" = "$MD5_STOCK" ] && { echo "chain: already the default"; exit 0; }
    [ "$(md5 "$BAK")" = "$MD5_STOCK" ] || { echo "chain: FAIL, no verified original at $BAK"; exit 1; }
    mount -o rw,remount /system 2>/dev/null
    bb cat "$BAK" > "$TMP" && [ "$(md5 "$TMP")" = "$MD5_STOCK" ] || { echo "chain: FAIL, copy did not verify"; bb rm -f "$TMP"; exit 1; }
    place "$TMP" || { echo "chain: FAIL, could not move the original into place"; exit 1; }
    echo "chain: default restored. Reboot to load it." ;;
*)  echo "usage: cinder-chain.sh status | set clearbass | set default | revert"; exit 2 ;;
esac
