#!/usr/bin/env bash
# check_arm_payload.sh — what a runner CAN say about the committed ARM payload without building it.
#
#   tools/check_arm_payload.sh [DIR]      (default: cinder-home/dist/stable)
#
# WHY THIS IS NOT A LINKING BUILD. cinder-home and cinder-probe link against a dozen of Sony's own
# libraries — libeaselcore, libpstcore, the PlayerService, Bluetooth and MediaStore clients — taken
# from the player's firmware. Those are Sony's files and cannot be put on a public runner. So the
# link gate is tools/release.sh, which rebuilds the payload from source before every tag and refuses
# to tag unless the committed bytes match (docs/SHORTCOMINGS.md A1 and D4).
#
# What CAN be checked from the binaries alone is whether they fit the player they are going to, and
# every check here is a way a payload can pass every other gate and still fail on the device:
#   * the ARCHITECTURE — a host build committed by accident installs cleanly and cannot run;
#   * the GLIBC FLOOR — the player has glibc 2.23. A symbol versioned GLIBC_2.24 or later fails at
#     exec, on the device, with a loader error nobody sees;
#   * the LIBRARIES — a new NEEDED entry the firmware does not have fails the same way. The list
#     below is every library the two binaries use today; adding one should be a decision, so it is
#     an edit here rather than something that slips through;
#   * STATIC HELPERS — build.sh builds every setuid helper static (musl). A dynamic one means the
#     build changed underneath them, and a setuid binary that loads libraries is not what was
#     reviewed.
#
# A script, not inline YAML, for the rule this repository keeps relearning: a check CI runs and a
# contributor cannot is a check that fails for the first time on a runner.
set -uo pipefail
cd "$(dirname "$0")/.." || { echo "cannot reach the repo root" >&2; exit 2; }

DIR="${1:-cinder-home/dist/stable}"
[ -d "$DIR" ] || { echo "no payload directory: $DIR" >&2; exit 2; }
command -v readelf >/dev/null || { echo "readelf (binutils) is needed" >&2; exit 2; }
command -v file >/dev/null || { echo "file is needed" >&2; exit 2; }

DYNAMIC=(cinder-home cinder-probe)
HELPERS=(cinder-umount cinder-power cinder-msc cinder-clock cinder-fm cinder-voltable cinder-battery)
# cinder-gpunode is dev-channel only; checked when present.
[ -f "$DIR/cinder-gpunode" ] && HELPERS+=(cinder-gpunode)

# Every shared library the dynamic binaries may need: Sony's service clients from vendor/sony/lib,
# the firmware's libc++ 3.9 runtime, the Mali driver, ALSA, and glibc 2.23 itself.
ALLOWED_LIBS="
libeaselcore.so libeaselcui.so libpstcore.so libappmgrservice.so
libPlayerServiceClient.so libPlayerServiceClientUtil.so libEffectCtrlDmp.so libPowerMgrServiceClient.so
libUsbDeviceAudioPlayerService.so libBtCommonService.so libBtTransmitterService.so libVolumeService.so
libMediaStoreServiceClient.so libNfcService.so libTunerPlayerService.so libAudioInPlayerService.so
libBtPlayerService.so libDisplayService.so libUsbMgrServiceFw.so libConnMgrService.so
libUsbDeviceConnectionService.so libFuncMgrService.so
libc++.so.1 libcxxrt.so.1 libgcc_s.so.1 libMali_linux.so libasound.so
libpthread.so.0 libdl.so.2 libm.so.6 libc.so.6 ld-linux-armhf.so.3
"
LOADER=/lib/ld-linux-armhf.so.3

FAIL=0
bad()  { printf '  FAIL  %-16s %s\n' "$1" "$2"; FAIL=1; }
good() { printf '  ok    %-16s %s\n' "$1" "$2"; }

is_arm() { case "$(file -b "$1")" in *"ELF 32-bit LSB"*ARM*EABI5*) return 0 ;; *) return 1 ;; esac; }

for b in "${DYNAMIC[@]}"; do
    f="$DIR/$b"
    [ -f "$f" ] || { bad "$b" "missing"; continue; }
    is_arm "$f" || { bad "$b" "not a 32-bit ARM EABI5 ELF: $(file -b "$f")"; continue; }

    interp="$(readelf -l "$f" 2>/dev/null | sed -n 's/.*Requesting program interpreter: \(.*\)\]/\1/p')"
    [ "$interp" = "$LOADER" ] || bad "$b" "interpreter is '${interp:-none}', the player's is $LOADER"

    newest="$(readelf -V "$f" 2>/dev/null | grep -oE 'GLIBC_[0-9]+\.[0-9]+(\.[0-9]+)?' | sort -uV | tail -1)"
    too_new="$(readelf -V "$f" 2>/dev/null | grep -oE 'GLIBC_[0-9]+\.[0-9]+' | sort -uV \
               | awk -F'[_.]' '$2 > 2 || ($2 == 2 && $3 > 23)')"
    [ -z "$too_new" ] || bad "$b" "needs glibc newer than the player's 2.23: $(echo $too_new)"

    cxx="$(readelf -V "$f" 2>/dev/null | grep -oE '(GLIBCXX|CXXABI)_[0-9.]+' | sort -u | head -3)"
    [ -z "$cxx" ] || bad "$b" "needs libstdc++ ($(echo $cxx)); the player's C++ runtime is libc++ 3.9"

    unknown=""
    for lib in $(readelf -d "$f" 2>/dev/null | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p'); do
        case " $(echo $ALLOWED_LIBS) " in *" $lib "*) ;; *) unknown="$unknown $lib" ;; esac
    done
    [ -z "$unknown" ] || bad "$b" "needs libraries the player is not known to have:$unknown"

    [ -z "$too_new$cxx$unknown" ] && [ "$interp" = "$LOADER" ] \
        && good "$b" "ARM, $LOADER, newest ${newest:-no glibc symbols}, $(readelf -d "$f" | grep -c NEEDED) libraries all known"
done

for b in "${HELPERS[@]}"; do
    f="$DIR/$b"
    [ -f "$f" ] || { bad "$b" "missing"; continue; }
    is_arm "$f" || { bad "$b" "not a 32-bit ARM EABI5 ELF: $(file -b "$f")"; continue; }
    if readelf -l "$f" 2>/dev/null | grep -q 'program interpreter' \
       || readelf -d "$f" 2>/dev/null | grep -q '(NEEDED)'; then
        bad "$b" "is dynamically linked; every setuid helper is built static"
    else
        good "$b" "ARM, static"
    fi
done

if [ "$FAIL" = 0 ]; then
    echo "ARM payload fits the player's runtime ($DIR)."
else
    echo "::error::the ARM payload in $DIR does not fit the player's runtime (see FAIL lines)"
fi
exit "$FAIL"
