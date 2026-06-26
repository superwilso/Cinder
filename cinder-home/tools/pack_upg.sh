#!/usr/bin/env bash
# pack_upg.sh — reproducibly build the cinder-home install/uninstall .UPG payloads and
# refresh the pushable binary into cinder-home/dist/.  No device, no sudo.
#
# A NWZ .UPG built for exec_file delivery is just two files packed with the model KAS:
#   file[0] = exec_file.sh  (Rockbox bootstrap: clears the fw-upgrade flag — brick-safe —
#             then extracts file[1] to /tmp/exec and runs it as root in the updater)
#   file[1] = our payload    (install_cinderhome.sh / uninstall_cinderhome.sh)
# The cinder-home BINARY is NOT in the .UPG — it's pushed separately to /contents/cinder-home
# (tools/flash.sh --push) and the installer copies it into /system. So a code change only
# needs the binary refreshed; the .UPG only changes when a deploy script changes.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CH="$HERE/.."                                  # cinder-home/
REPO="$CH/.."
UPGTOOL="$REPO/artifacts/upgtool"
EXECFILE="$REPO/artifacts/repos/rockbox/utils/nwztools/scripts/exec_file.sh"
MODEL=nw-a50
DIST="$CH/dist"; mkdir -p "$DIST"

[ -x "$UPGTOOL" ] || { echo "ERR: upgtool not built at $UPGTOOL (run: make phase1)"; exit 1; }
[ -f "$EXECFILE" ] || { echo "ERR: exec_file.sh template missing at $EXECFILE"; exit 1; }

pack() {  # pack <payload.sh> <out.upg>
    local payload="$1" out="$2" tmp; tmp="$(mktemp -d)"
    cp "$EXECFILE" "$tmp/exec_file.sh"
    cp "$payload"  "$tmp/payload.sh"
    # create: `upgtool -c <OUTPUT.UPG> <file0> <file1>` — first positional is the OUTPUT file,
    # then the two members in index order (0 = exec_file bootstrap, 1 = payload). (-o is for
    # extract only; create needs ≥2 input files or it dereferences files[1] and crashes.)
    "$UPGTOOL" -m "$MODEL" --create "$tmp/fw.UPG" "$tmp/exec_file.sh" "$tmp/payload.sh" >/dev/null
    [ -f "$tmp/fw.UPG" ] || { echo "ERR: upgtool produced no .UPG for $payload"; ls -la "$tmp"; exit 1; }
    cp "$tmp/fw.UPG" "$out"
    # round-trip verify: extract (-n no-color) and confirm both members come back intact
    "$UPGTOOL" -n -e -m "$MODEL" -o "$tmp/rt" "$out" >/dev/null 2>&1 || true
    { [ -f "$tmp/rt0.bin" ] && [ -f "$tmp/rt1.bin" ]; } \
        || { echo "ERR: round-trip verify failed for $out"; ls -la "$tmp"; exit 1; }
    # extracted members are block-padded; compare only the original payload length
    local plen; plen="$(stat -c%s "$tmp/payload.sh")"
    cmp -s -n "$plen" "$tmp/rt1.bin" "$tmp/payload.sh" \
        || { echo "ERR: round-trip payload mismatch for $out"; exit 1; }
    rm -rf "$tmp"
    echo "  packed $(basename "$out")  ($(stat -c%s "$out") bytes, round-trip OK)"
}

echo "[pack_upg] building install/uninstall .UPG ($MODEL)…"
pack "$CH/deploy/install_cinderhome.sh"   "$DIST/cinder_home_install.upg"
pack "$CH/deploy/uninstall_cinderhome.sh" "$DIST/cinder_home_uninstall.upg"

# refresh the pushable binary (built by build.sh)
if [ -f "$CH/cinder-home" ]; then
    cp -f "$CH/cinder-home" "$DIST/cinder-home"
    echo "  refreshed dist/cinder-home ($(stat -c%s "$DIST/cinder-home") bytes)"
else
    echo "  WARN: $CH/cinder-home not built yet (run build.sh first)"
fi
echo "[pack_upg] dist ready:"; ls -la "$DIST"
