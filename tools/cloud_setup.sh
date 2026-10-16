#!/usr/bin/env bash
# Everything `cinder-home/build.sh stable` and `tools/release.sh` need, installed into a fresh
# Linux container (Ubuntu 24.04, root, internet). Idempotent: a second run skips what is there.
#
#   bash tools/cloud_setup.sh          (from the repo root, or anywhere — it finds the repo)
#
# What it fetches, and from where:
#   * apt: the ARM gcc cross tools, kernel uapi headers, qemu (preflight gate), 7z + cpio (unpacking)
#   * libc++ 3.9.0 headers (llvm.org) and the glibc-2.23 xenial armhf sysroot (ports.ubuntu.com)
#     -> ~/toolchains, the default paths build.sh looks in
#   * the musl cross compiler (musl.cc) for the static setuid helpers -> ~/toolchains, on PATH
#   * rustc 1.95.0 + rust-src + the ARM target: the version the committed v0.3.16 payload was built
#     with (its .comment section), so a release from here is not a compiler upgrade by accident
#   * Sony's stock NW-A50 1.02 firmware, from Sony's own update server. The Mac .dmg, not the .exe:
#     the .exe is a Windows self-extractor nothing on Linux opens, the .dmg is a disk image 7z reads,
#     with NW_WM_FW.UPG inside as a plain file. Unpacked into the two places build.sh links against:
#     artifacts/rootfs_mnt (sector 6, the rootfs) and analysis/ramdisk (sector 2's boot ramdisk).
#     Both are gitignored: Sony's binaries are fetched, never committed.
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TC="$HOME/toolchains"
UPG_SHA=01df80bd560854cf6156dde5457bd385b7d0b40033790286546924abda7b8af7   # NW-A50 1.02
log() { printf '[cloud_setup] %s\n' "$*"; }
mkdir -p "$TC"

log "apt packages"
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq gcc-arm-linux-gnueabihf binutils-arm-linux-gnueabihf \
    linux-libc-dev-armhf-cross qemu-user-static p7zip-full cpio libcrypto++-dev >/dev/null

if [ ! -f "$TC/libcxx-3.9.0.src/include/functional" ]; then
    log "libc++ 3.9.0 headers"
    curl -fsSL https://releases.llvm.org/3.9.0/libcxx-3.9.0.src.tar.xz | tar xJ -C "$TC"
fi

SYSROOT="$TC/xenial-armhf-sysroot"
if [ ! -d "$SYSROOT/sysroot/usr/lib/arm-linux-gnueabihf" ]; then
    log "glibc 2.23 sysroot"
    mkdir -p "$SYSROOT"
    B=http://ports.ubuntu.com/ubuntu-ports/pool/main/g/glibc
    for d in libc6-dev_2.23-0ubuntu11.3_armhf.deb libc6_2.23-0ubuntu11.3_armhf.deb; do
        curl -fsSL -o "$SYSROOT/$d" "$B/$d"
        dpkg-deb -x "$SYSROOT/$d" "$SYSROOT/sysroot"
    done
fi

if [ ! -x "$TC/arm-linux-musleabihf-cross/bin/arm-linux-musleabihf-gcc" ]; then
    log "musl cross compiler"
    curl -fsSL https://musl.cc/arm-linux-musleabihf-cross.tgz | tar xz -C "$TC"
fi
# Every later shell (the Bash tool reads the profile) finds it.
grep -qs musleabihf-cross /etc/profile.d/cinder-toolchain.sh \
    || echo "export PATH=\"$TC/arm-linux-musleabihf-cross/bin:\$PATH\"" > /etc/profile.d/cinder-toolchain.sh
grep -qs cinder-toolchain "$HOME/.bashrc" || echo ". /etc/profile.d/cinder-toolchain.sh" >> "$HOME/.bashrc"

log "rust 1.95.0 (+ rust-src, ARM target)"
command -v rustup >/dev/null || curl -fsSL https://sh.rustup.rs | sh -s -- -y -q --profile minimal
. "$HOME/.cargo/env"
rustup -q toolchain install 1.95.0 --profile minimal --component rust-src \
    --target arm-unknown-linux-gnueabihf

if [ ! -f "$REPO/artifacts/rootfs_mnt/vendor/sony/lib/libeaselcore.so" ] \
   || [ ! -f "$REPO/analysis/ramdisk/lib/libcxxrt.so.1" ]; then
    WORK="$(mktemp -d)"
    UPG="$REPO/artifacts/stock/NW_WM_FW.UPG"
    mkdir -p "$REPO/artifacts/stock"
    if [ ! -f "$UPG" ]; then
        log "Sony stock firmware 1.02 (Mac image, 119 MB)"
        curl -fsSL -o "$WORK/fw.dmg" https://walkman.update.sony.net/fw/pc/A50/J/NW-A50_V1_02.dmg
        7z e -y -o"$WORK" "$WORK/fw.dmg" '*/NW_WM_FW.UPG' -r >/dev/null
        mv "$WORK/NW_WM_FW.UPG" "$UPG"
    fi
    echo "$UPG_SHA  $UPG" | sha256sum -c --quiet

    if [ ! -x "$REPO/artifacts/upgtool" ]; then
        log "upgtool (Rockbox nwztools)"
        RB="$REPO/artifacts/repos/rockbox"
        [ -d "$RB/.git" ] || { git clone -q --depth=1 --filter=blob:none --sparse \
            https://github.com/Rockbox/rockbox "$RB" && git -C "$RB" sparse-checkout set utils/nwztools; }
        make -s -C "$RB/utils/nwztools/upgtools" >/dev/null
        cp "$RB/utils/nwztools/upgtools/upgtool" "$REPO/artifacts/upgtool"
    fi

    log "unpacking: rootfs -> artifacts/rootfs_mnt, ramdisk -> analysis/ramdisk"
    "$REPO/artifacts/upgtool" -e -m nw-a50 -z 6 -o "$WORK/" "$UPG" >/dev/null
    rm -rf "$REPO/artifacts/rootfs_mnt" "$REPO/analysis/ramdisk"
    7z x -y -o"$REPO/artifacts/rootfs_mnt" "$WORK/6.bin" >/dev/null
    # Sector 2 is an Android boot image; its ramdisk carries a 512-byte MediaTek header before the gzip.
    python3 -I - "$WORK/2.bin" "$WORK/ramdisk.img" <<'EOF'
import struct, sys
d = open(sys.argv[1], "rb").read()
assert d[:8] == b"ANDROID!", "sector 2 is not a boot image"
ks, _, rs, _, _, _, _, ps = struct.unpack("<8I", d[8:40])
r0 = ps + (ks + ps - 1) // ps * ps
open(sys.argv[2], "wb").write(d[r0 + 512:r0 + rs])
EOF
    mkdir -p "$REPO/analysis/ramdisk"
    (cd "$REPO/analysis/ramdisk" && zcat "$WORK/ramdisk.img" | cpio -idm --quiet)
    rm -rf "$WORK"
fi

log "done. Build with:  RUSTUP_TOOLCHAIN=1.95.0 bash cinder-home/build.sh stable"
