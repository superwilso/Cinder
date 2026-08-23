#!/usr/bin/env bash
# Cross-build cinder-home for the NW-A50 — a real, device-loadable ARM binary.
# VERIFIED 2026-06-24: links clean, needs only GLIBC_2.4/2.17 (device is glibc 2.23), every
# undefined symbol resolves against the device libs. ~2.9 MB stripped ARM PIE.
#
# GPU present path (2026-07-26): cinder-ffi's frame present is EGL + GLES2 on the device's Mali
# driver (libMali_linux.so — Mali-450 r0p0, glibc "linux" build; libEGL.so.1/libGLESv2.so.2 are
# just symlinks to it). We link -l:libMali_linux.so (staged in analysis/ramdisk/lib); the egl*/gl*
# symbols resolve there at runtime. If EGL won't init on device, cinder-ffi falls back to the
# software framebuffer (mmap + FBIOPUT), so there is no black-screen risk. See player/cinder-ffi/
# src/gpu.rs.
#
# ── The three things that make this work ──────────────────────────────────────────────
# 1. ABI = libc++ (std::__1), NOT libstdc++. Sony's easel/appmgr/PlayerService symbols are
#    libc++-mangled, so the C++ shell is compiled clang -stdlib=libc++ with the device's
#    libc++ headers, and -fno-rtti (easel::ApplicationBase's typeinfo is a LOCAL symbol in
#    libeaselcore, can't be linked against; we never typeid/dynamic_cast the app).
# 2. glibc 2.23 TARGET. The device is glibc 2.23 (2016); this host's cross-gcc is glibc 2.39.
#    Linking against 2.39 emits GLIBC_2.28..2.34 symbol refs the device's ld-2.23 REFUSES.
#    Fix: a glibc-2.23 sysroot (Ubuntu-16.04 "xenial" armhf .debs) for crt + libc, forced
#    via -B<crt> and the xenial libdirs first (clang otherwise grabs the gcc-13 glibc 2.39).
# 3. The Rust render staticlib (cinder-ffi) and its bundled SQLite must ALSO be 2.23-clean:
#    - SQLite compiled against the 2.23 headers (-nostdinc -isystem <xenial>) with LFS off
#      (-DSQLITE_DISABLE_LFS) and 32-bit time (-U_TIME_BITS) so it uses stat/fcntl/time, not
#      the *_time64 / stat64 / fcntl64 symbols that DON'T EXIST in glibc 2.23.
#    - glibc223_compat.c shims stat/fstat/lstat/fstatat (+64) -> __xstat/__fxstat/... which
#      glibc 2.23 exports as @GLIBC_2.4 (it doesn't export plain stat; SQLite takes &stat).
# See README.md and the project memory for the full derivation.
#
# ── PREREQUISITES (all offline, no device, no sudo) ───────────────────────────────────
#   * clang-18 (clang++-18).
#   * libc++-18 + libc++abi-18 HEADERS. Get without sudo:
#       cd <dir> && curl -fLO https://releases.llvm.org/3.9.0/libcxx-3.9.0.src.tar.xz
#       tar xf libcxx-3.9.0.src.tar.xz
#       LIBCXX_V1=<dir>/libcxx-3.9.0.src/include
#     MUST be libc++ 3.9.0 — the device is libcxx-3.9.0 (clang 3.9, Chromium-OS, 2016) and
#     its std::function layout (24B, functor ptr @+16) differs from modern libc++; building
#     with libc++18 corrupts the CuiAppModule callbacks -> hang in OnInitialize on device.
#   * The glibc-2.23 sysroot (xenial). Get without sudo:
#       mkdir -p $DEVSYS/.. && cd <dir>
#       B=http://ports.ubuntu.com/ubuntu-ports/pool/main/g/glibc
#       curl -fLO $B/libc6-dev_2.23-0ubuntu11.3_armhf.deb
#       curl -fLO $B/libc6_2.23-0ubuntu11.3_armhf.deb
#       for d in *.deb; do dpkg-deb -x "$d" sysroot; done   # -> $DEVSYS = .../sysroot
#   * The device runtime libs are already in the repo at analysis/ramdisk/lib (libc++.so.1,
#     libcxxrt.so.1, libgcc_s.so.1, libc-2.23/ld-2.23, libpthread/libdl/libm) and the easel/
#     PlayerService libs at artifacts/rootfs_mnt/vendor/sony/lib — NO device pull needed.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$HERE/.."
RAMLIB="$REPO/analysis/ramdisk/lib"               # device glibc 2.23 + libc++/libcxxrt/libgcc_s
SONYLIB="$REPO/artifacts/rootfs_mnt/vendor/sony/lib"  # easel + PlayerService
RUSTLIB="$REPO/player/target/arm-unknown-linux-gnueabihf/release"
AUDIO="$REPO/cinder-audio"
OUT="$HERE/cinder-home"

# ── build channel: `stable` (default) or `dev`. Same tree, one flag. ────────────────────────
#   stable: the lean player, no adb.
#   dev:    adds a visible "CINDER DEV" marker (cargo `dev` feature) AND the dev binary enables
#           adb at boot (guarded, dev-only) for push-and-run iteration. Both build from this tree;
#           artifacts land in dist/<channel>/ so they never clobber each other.
CHANNEL="${1:-stable}"
case "$CHANNEL" in
    stable) CARGO_FEATURES=();          CHANNEL_DEF=(); DISCOVER_MAIN=() ;;
    # dev links the discovery dump into cinder-home (auto-runs at boot); stable doesn't (the probe
    # links it in both channels regardless).
    dev)    CARGO_FEATURES=(--features dev); CHANNEL_DEF=(-DCINDER_DEV=1); DISCOVER_MAIN=("$HERE/discover.o") ;;
    *) echo "usage: build.sh [stable|dev]"; exit 1 ;;
esac
DIST="$HERE/dist/$CHANNEL"
echo "── CINDER build channel: $CHANNEL ──"

TARGET=arm-linux-gnueabihf
CXX=clang++-18
CC=arm-linux-gnueabihf-gcc

# --- configurable paths (override via env) -------------------------------------------------
: "${DEVSYS:=$HOME/toolchains/xenial-armhf-sysroot/sysroot}"   # glibc-2.23 sysroot
: "${LIBCXX_V1:=$HOME/toolchains/libcxx-3.9.0.src/include}"   # libc++ 3.9.0 headers (device ABI)
[ -f "$LIBCXX_V1/functional" ] || { echo "ERR: libc++ 3.9.0 headers not at $LIBCXX_V1 (see PREREQUISITES)"; exit 1; }
[ -d "$DEVSYS/usr/lib/arm-linux-gnueabihf" ] || { echo "ERR: glibc-2.23 sysroot not at $DEVSYS (see PREREQUISITES)"; exit 1; }

GCCINC="$($CC -print-file-name=include)"           # gcc builtin headers (stddef/stdarg)
KHDR=/usr/arm-linux-gnueabihf/include              # modern kernel uapi headers (asm/, linux/) — ABI-stable
# 2.23 header search for C: gcc builtins, then xenial glibc, then kernel uapi.
SYS223=(-nostdinc -isystem "$GCCINC" \
        -isystem "$DEVSYS/usr/include/arm-linux-gnueabihf" -isystem "$DEVSYS/usr/include" \
        -isystem "$KHDR")
# kill the cross-gcc's forced 64-bit time/offset (the device's glibc 2.23 has no time64).
T32=(-U_TIME_BITS -U_FILE_OFFSET_BITS)

INCLUDES=(-I"$HERE/src" -I"$REPO/player/cinder-ffi/include" -I"$AUDIO/include" -I"$AUDIO/src")

echo "[1/4] build cinder-ffi staticlib (Rust UI + SQLite vs glibc 2.23)…"
( cd "$REPO/player" && \
  CC_arm_unknown_linux_gnueabihf="$CC" \
  AR_arm_unknown_linux_gnueabihf=arm-linux-gnueabihf-ar \
  CFLAGS_arm_unknown_linux_gnueabihf="-DSQLITE_DISABLE_LFS ${T32[*]} ${SYS223[*]}" \
    cargo build -p cinder-ffi --release --target arm-unknown-linux-gnueabihf "${CARGO_FEATURES[@]}" )

echo "[2/4] compile C++ shell + audio shim (clang/libc++, -fno-rtti)…"
# Compile against the device libc++ headers AND the glibc-2.23 C headers. We must force the
# 2.23 C headers explicitly (-nostdinc + -isystem): --sysroot alone does NOT switch them, so
# clang would otherwise use the host's glibc-2.39 stdio/stdlib whose C23 redirects pull in
# __isoc23_scanf/__isoc23_strtol — symbols that DON'T EXIST on the device's glibc 2.23.
# Order: libc++ 3.9 C++ headers, then clang's builtin headers (stddef/stdarg), then xenial
# 2.23 glibc, then kernel uapi. T32 kills the forced 64-bit time/offset.
CLANGRES="$($CXX -print-resource-dir)"
CXXINC=(-nostdinc++ -isystem "$LIBCXX_V1" \
        -nostdinc -isystem "$CLANGRES/include" -isystem "$GCCINC" \
        -isystem "$DEVSYS/usr/include/arm-linux-gnueabihf" -isystem "$DEVSYS/usr/include" \
        -isystem "$KHDR")
for src in "$HERE/src/main.cpp:$HERE/main.o" \
           "$AUDIO/src/player_shim.cpp:$HERE/player_shim.o" \
           "$AUDIO/src/effect_shim.cpp:$HERE/effect_shim.o" \
           "$AUDIO/src/tuner_shim.cpp:$HERE/tuner_shim.o" \
           "$AUDIO/src/analyzer_shim.cpp:$HERE/analyzer_shim.o" \
           "$AUDIO/src/power_shim.cpp:$HERE/power_shim.o" \
           "$HERE/src/discover.cpp:$HERE/discover.o"; do
    # -I../ldac-bridge/include: the minimal ALSA shim. tuner_shim.cpp needs it — the FM scanner
    # measures the capture PCM directly, because Sony's own GetSignalLevel/StartAutoTuning cannot
    # find a station on this hardware (verified against one that was audible).
    $CXX --target=$TARGET -stdlib=libc++ "${CXXINC[@]}" "${T32[@]}" \
         -I"$HERE/../ldac-bridge/include" \
         -fPIC -O2 -Wall -std=c++14 -fno-rtti "${CHANNEL_DEF[@]}" "${INCLUDES[@]}" \
         -c "${src%%:*}" -o "${src##*:}"
done

echo "[3/4] compile glibc-2.23 compat shim (stat/* -> __xstat/*)…"
$CC -Os -fPIC "${T32[@]}" "${SYS223[@]}" \
    -c "$HERE/src/glibc223_compat.c" -o "$HERE/glibc223_compat.o"

echo "[4/4] link (xenial 2.23 crt forced via -B; device shared libs)…"
# crt override dir: force the xenial 2.23 start files (clang otherwise uses gcc-13/glibc-2.39).
CRT="$HERE/.crt223"; mkdir -p "$CRT"
cp -f "$DEVSYS/usr/lib/arm-linux-gnueabihf"/{Scrt1.o,crt1.o,crti.o,crtn.o} "$CRT/"
$CXX --target=$TARGET --sysroot="$DEVSYS" -B"$CRT" -nostdlib++ \
     -L"$DEVSYS/usr/lib/arm-linux-gnueabihf" -L"$DEVSYS/lib/arm-linux-gnueabihf" \
     "$HERE/main.o" "$HERE/player_shim.o" "$HERE/effect_shim.o" "$HERE/tuner_shim.o" "$HERE/analyzer_shim.o" "$HERE/power_shim.o" "${DISCOVER_MAIN[@]}" "$HERE/glibc223_compat.o" \
     -L"$SONYLIB" -L"$RAMLIB" -L"$RUSTLIB" \
     -Wl,--allow-shlib-undefined -Wl,-rpath-link,"$SONYLIB:$RAMLIB" \
     -leaselcore -leaselcui -lpstcore -lappmgrservice -lPlayerServiceClient -lPlayerServiceClientUtil -lEffectCtrlDmp -lPowerMgrServiceClient \
     -lUsbDeviceAudioPlayerService -lBtCommonService -lBtTransmitterService \
     -l:libc++.so.1 -l:libcxxrt.so.1 -lcinder_ffi \
     -l:libMali_linux.so \
     -l:libpthread.so.0 -l:libdl.so.2 -l:libm.so.6 \
     -o "$OUT"

gate_glibc() {  # gate_glibc <binary> : fail if it needs GLIBC newer than the device's 2.23
    local b="$1"
    local bad
    bad="$(${TARGET}-readelf -V "$b" 2>/dev/null | grep -oE 'GLIBC_[0-9.]+' | sort -uV | \
           awk -F_ '{split($2,a,"."); if (a[1]>2 || (a[1]==2 && a[2]>23)) print $0}')"
    if [ -n "$bad" ]; then echo "FAIL: $b needs GLIBC newer than device 2.23:"; echo "$bad"; exit 1; fi
    echo "OK: $(basename "$b") GLIBC needs = $(${TARGET}-readelf -V "$b" 2>/dev/null | grep -oE 'GLIBC_[0-9.]+' | sort -uV | tr '\n' ' ')"
}

echo "── self-test: crash+hang GUARD recovery (host) ──"
# Validates the run_guarded() pattern main.cpp relies on: a crash/hang inside a guarded call is
# caught and skipped, the process survives. Fast, host-native, no device.
if cc -O2 -o "$HERE/.guard_selftest" "$HERE/tools/guard_selftest.cpp" 2>/dev/null; then
    if "$HERE/.guard_selftest" >/dev/null 2>&1; then echo "OK: guard recovers crash + hang"; \
    else echo "FAIL: guard self-test did not recover"; exit 1; fi
    rm -f "$HERE/.guard_selftest"
else echo "(skip: no host cc)"; fi

echo "── self-test: volume rocker ramp (host) ──"
# The hold-to-accelerate curve in src/vol_ramp.h, checked against the SAME header main.cpp uses:
# a tap must stay one step, the ramp must not run away, and a full sweep of either scale must be a
# few seconds rather than fifteen. Fast, host-native, no device.
if cc -O2 -o "$HERE/.volramp_selftest" "$HERE/tools/volramp_selftest.cpp" -lstdc++ 2>/dev/null; then
    if "$HERE/.volramp_selftest" >/dev/null 2>&1; then echo "OK: volume ramp curve"; \
    else "$HERE/.volramp_selftest"; echo "FAIL: volume ramp self-test"; exit 1; fi
    rm -f "$HERE/.volramp_selftest"
else echo "(skip: no host cc)"; fi

echo "── self-test: headphone-unplug edge (host) ──"
# The rule in src/jack_edge.h, checked against the SAME header main.cpp uses: only the
# plugged->unplugged transition pauses, the first observation of a boot never acts, and plugging IN
# does nothing. It was written wrong the first time (it paused on plug-in), hence this file.
if cc -O2 -o "$HERE/.btedge_selftest" "$HERE/tools/btedge_selftest.cpp" -lstdc++ 2>/dev/null; then
    if "$HERE/.btedge_selftest" >/dev/null 2>&1; then echo "OK: bluetooth-disconnect edge"; \
    else "$HERE/.btedge_selftest"; echo "FAIL: bt edge self-test"; exit 1; fi
    rm -f "$HERE/.btedge_selftest"
fi
if cc -O2 -o "$HERE/.jackedge_selftest" "$HERE/tools/jackedge_selftest.cpp" -lstdc++ 2>/dev/null; then
    if "$HERE/.jackedge_selftest" >/dev/null 2>&1; then echo "OK: headphone-unplug edge"; \
    else "$HERE/.jackedge_selftest"; echo "FAIL: jack edge self-test"; exit 1; fi
    rm -f "$HERE/.jackedge_selftest"
else echo "(skip: no host cc)"; fi
# The library-database change rule (src/db_sig.h). Same reason as the two above: it is a handful of
# lines that decides something the user sees — whether music they just copied is ever picked up —
# and its first version (st_mtime on the main file alone) could be defeated by SQLite's WAL mode.
if cc -O2 -o "$HERE/.dbsig_selftest" "$HERE/tools/dbsig_selftest.cpp" -lstdc++ 2>/dev/null; then
    if "$HERE/.dbsig_selftest" >/dev/null 2>&1; then echo "OK: library database change rule"; \
    else "$HERE/.dbsig_selftest"; echo "FAIL: db signature self-test"; exit 1; fi
    rm -f "$HERE/.dbsig_selftest"
else echo "(skip: no host cc)"; fi

echo "── verify: device glibc compatibility gate ──"
gate_glibc "$OUT"
cp "$OUT" "$OUT.unstripped"; ${TARGET}-strip "$OUT"
echo "built: $OUT  ($(stat -c%s "$OUT") bytes)"; file "$OUT" | cut -d, -f1-3

echo "[5] build cinder-probe (standalone diagnostic — no easel lifecycle, no boot impact)…"
PROBE="$HERE/cinder-probe"
# -I../ldac-bridge/include: the minimal ALSA shim, so --ldac can probe capture-PCM availability
# without an armhf libasound2-dev on the host. The DEVICE's libasound.so is what gets linked.
$CXX --target=$TARGET -stdlib=libc++ "${CXXINC[@]}" "${T32[@]}" \
     -fPIC -O2 -Wall -std=c++14 -fno-rtti "${CHANNEL_DEF[@]}" "${INCLUDES[@]}" \
     -I"$HERE/../ldac-bridge/include" \
     -c "$HERE/src/probe.cpp" -o "$HERE/probe.o"
# Links WITHOUT easelcore/easelcui/appmgrservice — the probe never does the app lifecycle, only
# the render/DB/PlayerService calls, so it can't register as the Home app. It DOES link pstcore
# (2026-07-27) for --pump: pst::core::Framework::Pump() drives the event looper that delivers
# binder replies, which is what the "every PlayerService out-param is stack garbage" hunt needs.
$CXX --target=$TARGET --sysroot="$DEVSYS" -B"$CRT" -nostdlib++ \
     -L"$DEVSYS/usr/lib/arm-linux-gnueabihf" -L"$DEVSYS/lib/arm-linux-gnueabihf" \
     "$HERE/probe.o" "$HERE/player_shim.o" "$HERE/effect_shim.o" "$HERE/tuner_shim.o" "$HERE/analyzer_shim.o" "$HERE/discover.o" "$HERE/glibc223_compat.o" \
     -L"$SONYLIB" -L"$RAMLIB" -L"$RUSTLIB" \
     -Wl,--allow-shlib-undefined -Wl,-rpath-link,"$SONYLIB:$RAMLIB" \
     -lPlayerServiceClient -lPlayerServiceClientUtil -lpstcore -l:libc++.so.1 -l:libcxxrt.so.1 -lcinder_ffi \
     -lBtTransmitterService -lBtCommonService -lUsbDeviceAudioPlayerService \
     -lNfcService -lTunerPlayerService -lAudioInPlayerService \
     -lBtPlayerService \
     -lDisplayService \
     -lUsbMgrServiceFw \
     -lConnMgrService -lUsbDeviceConnectionService -lFuncMgrService \
     -lEffectCtrlDmp "$REPO/artifacts/rootfs_mnt/lib/libasound.so" \
     -l:libMali_linux.so \
     -l:libpthread.so.0 -l:libdl.so.2 -l:libm.so.6 \
     -o "$PROBE"
gate_glibc "$PROBE"
cp "$PROBE" "$PROBE.unstripped"; ${TARGET}-strip "$PROBE"
echo "built: $PROBE  ($(stat -c%s "$PROBE") bytes)"

# ── offline bring-up gate: construct the real device objects under qemu (no device needed) ──
# Catches std::function-ABI / ctor-signature / object-SIZE regressions BEFORE flashing.
if [ "${SKIP_PREFLIGHT:-0}" != "1" ]; then
    echo "── preflight: qemu construction gate ──"
    DEVSYS="$DEVSYS" LIBCXX_V1="$LIBCXX_V1" bash "$HERE/tools/preflight_qemu.sh"
fi

# ── recovery gate: the launcher is the one file whose bugs are UNRECOVERABLE ──────────────────
# It decides stock-vs-cinder before anything else runs, so a fault there is a boot loop with no
# way in (2026-07-26: a failed log redirect made sh exit without exec'ing → appmgr rebooted →
# wbrt restore). This drives the generated launcher through every escape and failure mode in a
# sandbox. Never ship without it green.
echo "── recovery gate: launcher escape matrix ──"
if ! LT_OUT="$(bash "$HERE/tools/test_launcher.sh" 2>&1)"; then
    echo "$LT_OUT"
    echo "FAIL: launcher recovery gate — a boot-escape path is broken. NOT packing."
    exit 1
fi
echo "$LT_OUT" | tail -1

# ── stage the channel binaries into dist/<channel>/ (the two channels never clobber each other).
# pack_upg.sh <channel> packs the matching install/uninstall .UPGs alongside them.
# cinder-umount: tiny static setuid-root helper (the only privileged op capless cinder-home needs
# for USB-MSC — see src/cinder-umount.c). Built static-musl so it has no libc-version dependency;
# installed chmod 4755 root:root by install_cinderhome.sh.
echo "[6] build cinder-umount (setuid-root umount helper, static)…"
UMOUNT_CC=""
for c in arm-linux-musleabihf-gcc "$HOME/arm-linux-musleabihf-cross/bin/arm-linux-musleabihf-gcc"; do
    command -v "$c" >/dev/null 2>&1 && { UMOUNT_CC="$c"; break; }
done
[ -n "$UMOUNT_CC" ] || { echo "ERROR: arm-linux-musleabihf-gcc not found (needed for cinder-umount)"; exit 1; }
"$UMOUNT_CC" -static -Os -Wall -o "$HERE/cinder-umount" "$HERE/src/cinder-umount.c"
echo "built: $HERE/cinder-umount ($(stat -c %s "$HERE/cinder-umount") bytes)"

# cinder-gpunode: second setuid-root helper — chmod 0666 on the four root-only GPU/display nodes
# (/dev/ion, /dev/mtkfb_vsync, /dev/mtk_disp, /dev/sw_sync) that uid-100 EGL needs. Same static-musl
# + chmod 4755 root:root install treatment as cinder-umount. See src/cinder-gpunode.c.
echo "[6b] build cinder-gpunode (setuid-root GPU node helper, static)…"
"$UMOUNT_CC" -static -Os -Wall -o "$HERE/cinder-gpunode" "$HERE/src/cinder-gpunode.c"
echo "built: $HERE/cinder-gpunode ($(stat -c %s "$HERE/cinder-gpunode") bytes)"

# cinder-power: third setuid-root helper — reboot(2) for Power off / Restart. Sony's own
# PowerMgrServiceClient cannot do it while Cinder is the Home app (its shutdown barrier waits on a
# service ACK we do not send: Reboot() froze the device, SetStatus(PowerOff) only slept it — see
# src/cinder-power.c). reboot(2) needs CAP_SYS_BOOT, which capless cinder-home does not have.
# Ships on BOTH channels: unlike cinder-gpunode this backs a feature that is always on, and it
# widens nothing — it grants two fixed verbs, no caller-supplied paths.
echo "[6c] build cinder-power (setuid-root power helper, static)…"
"$UMOUNT_CC" -static -Os -Wall -o "$HERE/cinder-power" "$HERE/src/cinder-power.c"
echo "built: $HERE/cinder-power ($(stat -c %s "$HERE/cinder-power") bytes)"

# cinder-clock: fifth setuid-root helper — set the system clock and the RTC. Both settimeofday(2)
# and the RTC_SET_TIME ioctl need CAP_SYS_TIME; cinder-home runs as uid 100 (system) with an empty
# capability set. NOTHING in vendor/sony/lib exposes a clock setter (a sweep of every library's
# demangled `virtual` prototypes finds none), so the kernel is the only route. See src/cinder-clock.c.
echo "[6e] build cinder-clock (setuid-root clock helper, static)…"
"$UMOUNT_CC" -static -Os -Wall -o "$HERE/cinder-clock" "$HERE/src/cinder-clock.c"
echo "built: $HERE/cinder-clock ($(stat -c %s "$HERE/cinder-clock") bytes)"

# cinder-voltable: seventh setuid-root helper — install one of Sony's OUTPUT VOLUME TABLES into
# /proc/icx_audio_cxd3778gf_data/{ovt,ovt_dsd}, which are root-only. The stock A50 curve has two
# dead zones (vol 40-60 and 100-120 do nothing, measured) and coarsens toward the top where the
# volume pop is worst; the NW-WM1A table Sony already ships has neither problem. Must run every
# boot, because load_sony_driver re-applies the stock table each time. See
# analysis/RE_volume_pop.md and src/cinder-voltable.c.
echo "[6g] build cinder-voltable (setuid-root volume-table helper, static)…"
"$UMOUNT_CC" -static -Os -Wall -o "$HERE/cinder-voltable" "$HERE/src/cinder-voltable.c"
echo "built: $HERE/cinder-voltable ($(stat -c %s "$HERE/cinder-voltable") bytes)"

# cinder-fm: sixth setuid-root helper — chmod 0666 on /proc/regmon/Si4708icx/{target,value}, the
# two kernel files through which Sony's own driver publishes the FM tuner's registers. That is what
# gives the radio a real signal meter, a one-second band scan and the chip's hardware seek; Sony's
# TunerPlayerService can provide none of them (GetSignalLevel is a constant 1, StartAutoTuning is a
# 48-byte stub). See src/cinder-fm.c and analysis/RE_fm_tuner.md.
echo "[6f] build cinder-fm (setuid-root FM register helper, static)…"
"$UMOUNT_CC" -static -Os -Wall -o "$HERE/cinder-fm" "$HERE/src/cinder-fm.c"
echo "built: $HERE/cinder-fm ($(stat -c %s "$HERE/cinder-fm") bytes)"

# cinder-msc: fourth setuid-root helper — the USB mass-storage handoff. BOTH privileged steps are
# root-only on this device (the LUN backing-file write opens the block device in the caller's
# credentials, and sys.sony.config is refused for uid system), which is why MSC never worked from
# capless cinder-home. Ships on BOTH channels — MSC is how the user gets files onto the device.
echo "[6d] build cinder-msc (setuid-root USB-MSC helper, static)…"
"$UMOUNT_CC" -static -Os -Wall -o "$HERE/cinder-msc" "$HERE/src/cinder-msc.c"
echo "built: $HERE/cinder-msc ($(stat -c %s "$HERE/cinder-msc") bytes)"

mkdir -p "$DIST"
cp -f "$OUT" "$DIST/cinder-home"
cp -f "$HERE/cinder-probe" "$DIST/cinder-probe"
# cinder-signature.sh: the on-device audio "sound signature" switcher (3-byte HAL patch — see
# analysis/RE_walkmanone_extract.md). A plain script, not a compiled helper, so it just gets copied.
cp -f "$HERE/deploy/cinder-signature.sh" "$DIST/cinder-signature.sh"
# The component selection. Generated by tools/configure.sh; created at defaults on first build so
# a plain `build.sh` still produces a complete, installable staging set.
[ -f "$DIST/cinder_components.conf" ] || bash "$HERE/tools/configure.sh" "$CHANNEL" --defaults >/dev/null
cp -f "$HERE/cinder-umount" "$DIST/cinder-umount"
cp -f "$HERE/cinder-power" "$DIST/cinder-power"
cp -f "$HERE/cinder-clock" "$DIST/cinder-clock"
cp -f "$HERE/cinder-msc" "$DIST/cinder-msc"
cp -f "$HERE/cinder-fm" "$DIST/cinder-fm"
cp -f "$HERE/cinder-voltable" "$DIST/cinder-voltable"
# cinder-gpunode ships on the DEV channel ONLY. It is setuid-root and its whole job is to make
# four kernel graphics nodes world-writable — real attack surface — in service of a GPU present
# path that is default OFF and measured 4.7x SLOWER than the software one (45.6 ms/present vs 9.6;
# FBIOPUT_VSCREENINFO contends with the Mali pipeline). The present thread superseded the reason it
# existed. Shipping it on the daily-use build would trade a permanent permission loosening for a
# feature nobody turns on, so stable does not get it; it stays available for GPU experiments on dev.
if [ "$CHANNEL" = "dev" ]; then
    cp -f "$HERE/cinder-gpunode" "$DIST/cinder-gpunode"
else
    rm -f "$DIST/cinder-gpunode"
fi
echo "staged $CHANNEL binaries -> $DIST/"
echo "── done ($CHANNEL). next: bash tools/pack_upg.sh $CHANNEL ──"
