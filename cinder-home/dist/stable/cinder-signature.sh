#!/bin/sh
# cinder-signature.sh — pick the audio "sound signature" on an NW-A50, on-device, no flash.
#
# WHAT THIS IS (see analysis/RE_walkmanone_extract.md for the full derivation):
# Walkman One's paid "sound signature" is a THREE-BYTE patch to the stock audio HAL,
# /system/vendor/sony/lib/libaudiohal-adleralsa.so. The bytes are ASCII digits inside string
# literals — no code changes at all. They control exactly two things:
#
#   1. which ALSA PCM device the output stream opens   (two path strings: hw:0,0 and hw:0,4)
#   2. the CPU clock floor held during playback        (scaling_min_freq: 1040000 vs 1300000)
#
# The A50's stock library is BYTE-IDENTICAL to Walkman One's own `normal` baseline, so the same
# offsets apply here and the patch is exact and reversible. This script splits the two effects
# apart, which the original mod does not — `clock` gives the CPU floor with the signal path left
# alone, and `hw1`/`hw2` give the path change without the battery cost.
#
# SAFETY MODEL — this file is loaded by the audio service at play time, so a truncated or
# half-written copy would break playback with no obvious cause:
#   * the live library is verified against a known md5 BEFORE anything is written; an unknown
#     hash aborts (never patch something we do not recognise)
#   * the pristine original is backed up to .stock on first use, and `revert` restores THAT file
#     rather than re-patching, so a revert is exact even if this script's tables are wrong
#   * patching happens on a TEMP COPY which is md5-verified against the expected result; only a
#     matching copy is moved into place. An interrupted run leaves the live library untouched.
#   * every write goes through mv (atomic within a filesystem) + sync
#
# A change takes effect when the HAL is next LOADED — i.e. after a reboot. Nothing here restarts
# the audio service or reboots; that is deliberate, and left to the caller.
#
# USAGE:  cinder-signature.sh status
#         cinder-signature.sh set   stock|pv1|pv2|clock|hw1|hw2
#         cinder-signature.sh revert
set -u

LIB=/system/vendor/sony/lib/libaudiohal-adleralsa.so
BAK=$LIB.stock
TMP=$LIB.new

# busybox anchor — the updater's ambient tools are unreliable (see install_cinderhome.sh).
BB=/xbin/busybox
[ -x "$BB" ] || BB=/system/xbin/busybox
[ -x "$BB" ] || BB=""
bb() { if [ -n "$BB" ]; then "$BB" "$@"; else "$@"; fi; }

SIZE_EXPECT=155068
MD5_STOCK=c8de2a65cf4f5a65b19db8de62a752b7

# variant -> md5 of the finished library
md5_for() {
    case "$1" in
        stock) echo c8de2a65cf4f5a65b19db8de62a752b7 ;;
        pv1)   echo 6baf1bf0dcf6ddba504fe8a7b11b1c8b ;;
        pv2)   echo 32c9f4359dd1628f780c47ecb189bbce ;;
        clock) echo 60e2b8e781dfed155656463e95db1279 ;;
        hw1)   echo 6c118ac4a3586a72e10f661b0135fa71 ;;
        hw2)   echo b885e2815e3692f3f47b35b1f0aa8c87 ;;
        *)     echo "" ;;
    esac
}

describe() {
    case "$1" in
        stock) echo "stock  — hw:0,0 + hw:0,4, CPU floor 1.04 GHz (Sony original)" ;;
        pv1)   echo "pv1    — both paths hw:0,0, CPU floor 1.3 GHz  (Walkman One)" ;;
        pv2)   echo "pv2    — both paths hw:0,4, CPU floor 1.3 GHz  (Walkman One)" ;;
        clock) echo "clock  — CPU floor 1.3 GHz only, signal path untouched" ;;
        hw1)   echo "hw1    — both paths hw:0,0, CPU floor left at 1.04 GHz" ;;
        hw2)   echo "hw2    — both paths hw:0,4, CPU floor left at 1.04 GHz" ;;
        *)     echo "unknown" ;;
    esac
}

# byte edits per variant: "<offset> <expected-octal> <new-octal>", offsets 0-based into the
# STOCK library. 0x30='0' 0x33='3' 0x34='4'  ->  octal 060 063 064.
#   139610  first  hw:0,N  digit      139617  second hw:0,N  digit
#   139946/139947  the "1040000" -> "1300000" scaling_min_freq literal
edits_for() {
    case "$1" in
        stock) echo "" ;;
        pv1)   echo "139617 064 060|139946 060 063|139947 064 060" ;;
        pv2)   echo "139610 060 064|139946 060 063|139947 064 060" ;;
        clock) echo "139946 060 063|139947 064 060" ;;
        hw1)   echo "139617 064 060" ;;
        hw2)   echo "139610 060 064" ;;
        *)     echo "" ;;
    esac
}

live_md5() { bb md5sum "$LIB" 2>/dev/null | bb cut -d' ' -f1; }

name_of_md5() {
    for v in stock pv1 pv2 clock hw1 hw2; do
        [ "$1" = "$(md5_for $v)" ] && { echo "$v"; return 0; }
    done
    echo ""
}

do_status() {
    [ -f "$LIB" ] || { echo "signature: FAIL — $LIB missing"; return 1; }
    cur="$(live_md5)"
    nm="$(name_of_md5 "$cur")"
    echo "signature: library $LIB"
    echo "signature: size    $(bb wc -c < "$LIB" 2>/dev/null | bb tr -d ' ')  (expect $SIZE_EXPECT)"
    echo "signature: md5     $cur"
    if [ -n "$nm" ]; then
        echo "signature: active  $(describe "$nm")"
    else
        echo "signature: active  UNRECOGNISED — not a hash this script knows."
        echo "signature:         Refusing to patch. Restore with 'revert' if a backup exists."
    fi
    if [ -f "$BAK" ]; then
        echo "signature: backup  present ($(bb md5sum "$BAK" 2>/dev/null | bb cut -d' ' -f1))"
    else
        echo "signature: backup  none yet (created on first 'set')"
    fi
    [ -n "$nm" ]
}

remount_rw() { mount -o rw,remount /system 2>/dev/null; }

do_set() {
    want="$1"
    exp="$(md5_for "$want")"
    [ -n "$exp" ] || { echo "signature: FAIL — unknown variant '$want'"; usage; return 2; }
    [ -f "$LIB" ] || { echo "signature: FAIL — $LIB missing"; return 1; }

    cur="$(live_md5)"
    curname="$(name_of_md5 "$cur")"
    if [ -z "$curname" ]; then
        echo "signature: FAIL — live library md5 $cur is not recognised."
        echo "signature:        Not patching an unknown binary. Use 'revert' or reinstall the FW."
        return 1
    fi
    if [ "$cur" = "$exp" ]; then
        echo "signature: already $want — nothing to do."
        return 0
    fi

    remount_rw

    # first ever patch: keep the pristine original so revert never depends on these tables
    if [ ! -f "$BAK" ]; then
        if [ "$cur" != "$MD5_STOCK" ]; then
            echo "signature: FAIL — no backup yet and the live library is not stock ($curname)."
            echo "signature:        Refusing to snapshot a modified library as 'stock'."
            return 1
        fi
        bb cat "$LIB" > "$BAK" || { echo "signature: FAIL — backup write failed"; return 1; }
        sync
        if [ "$(bb md5sum "$BAK" | bb cut -d' ' -f1)" != "$MD5_STOCK" ]; then
            echo "signature: FAIL — backup verify failed; removing it"; bb rm -f "$BAK"; return 1
        fi
        echo "signature: backed up pristine library -> $BAK"
    fi

    # always build the new library from the PRISTINE original, so variants never stack
    bb rm -f "$TMP"
    bb cat "$BAK" > "$TMP" || { echo "signature: FAIL — temp copy failed"; bb rm -f "$TMP"; return 1; }

    edits="$(edits_for "$want")"
    if [ -n "$edits" ]; then
        echo "$edits" | bb tr '|' '\n' | while read -r off oldoct newoct; do
            [ -n "$off" ] || continue
            # verify the byte we are about to overwrite is what we expect
            got="$(bb dd if="$TMP" bs=1 skip="$off" count=1 2>/dev/null | bb od -An -b | bb tr -d ' \n')"
            if [ "$got" != "$oldoct" ]; then
                echo "signature: FAIL — offset $off holds \\$got, expected \\$oldoct"
                exit 1
            fi
            # dd MUST be busybox's: the ambient dd on this device answers "conv option disabled"
            # and exits 1, so a bare dd here silently never patches anything.
            printf "\\$newoct" | bb dd of="$TMP" bs=1 seek="$off" count=1 conv=notrunc 2>/dev/null \
                || { echo "signature: FAIL — write at $off failed"; exit 1; }
        done || { bb rm -f "$TMP"; return 1; }
    fi
    sync

    got="$(bb md5sum "$TMP" | bb cut -d' ' -f1)"
    if [ "$got" != "$exp" ]; then
        echo "signature: FAIL — patched copy md5 $got != expected $exp. Live library untouched."
        bb rm -f "$TMP"; return 1
    fi

    bb chmod 755 "$TMP"
    bb chown root:shell "$TMP" 2>/dev/null
    mv "$TMP" "$LIB" || { echo "signature: FAIL — mv into place failed"; bb rm -f "$TMP"; return 1; }
    sync
    echo "signature: set $want  ($(describe "$want"))"
    echo "signature: takes effect on the next REBOOT (the HAL is loaded at play time)."
    return 0
}

do_revert() {
    [ -f "$BAK" ] || { echo "signature: nothing to revert — no backup at $BAK"; return 1; }
    if [ "$(bb md5sum "$BAK" | bb cut -d' ' -f1)" != "$MD5_STOCK" ]; then
        echo "signature: FAIL — backup is not the pristine stock library. Refusing to restore."
        return 1
    fi
    remount_rw
    bb rm -f "$TMP"
    bb cat "$BAK" > "$TMP" || { echo "signature: FAIL — temp copy failed"; return 1; }
    sync
    bb chmod 755 "$TMP"; bb chown root:shell "$TMP" 2>/dev/null
    mv "$TMP" "$LIB" || { echo "signature: FAIL — mv failed"; bb rm -f "$TMP"; return 1; }
    sync
    echo "signature: reverted to stock. Takes effect on the next REBOOT."
    return 0
}

usage() {
    echo "usage: cinder-signature.sh status"
    echo "       cinder-signature.sh set <variant>"
    echo "       cinder-signature.sh revert"
    echo "variants:"
    for v in stock pv1 pv2 clock hw1 hw2; do echo "  $(describe $v)"; done
}

case "${1:-status}" in
    status) do_status ;;
    set)    [ $# -ge 2 ] || { usage; exit 2; }; do_set "$2" ;;
    revert) do_revert ;;
    *)      usage; exit 2 ;;
esac
