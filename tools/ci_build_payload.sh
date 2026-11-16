#!/usr/bin/env bash
# ci_build_payload.sh — build the whole device payload on a clean Ubuntu 24.04 machine (CI), with
# stand-in libraries instead of the firmware, and check it against cinder-home/dist/PAYLOAD.sha256.
#
#   tools/ci_build_payload.sh            set up, build both channels, pack the .UPGs, compare
#   tools/ci_build_payload.sh --setup    only install the toolchains (for a cache step)
#
# WHERE IT MUST RUN. The payload carries absolute paths: Rust's panic locations name the cargo
# registry and the toolchain's rust-src, and the Wi-Fi builds name their work folders. A build
# that is to come out byte for byte like the maintainer's must use the same ones, so this runs
# from /home/sony/sony with HOME=/home/sony (payload.yml makes that folder and moves the checkout
# there). Anywhere else it builds, but the comparison at the end will fail on those files.
#
# WHAT IT NEEDS THAT IS NOT IN GIT. Nothing private: the device libraries are replaced by stand-ins
# (cinder-home/tools/stubs.py, from the symbol lists in cinder-home/stubs/), and everything else is
# a pinned public download — Ubuntu's packages, the xenial glibc 2.23 sysroot, libc++ 3.9.0
# headers, musl.cc's cross compiler, Rust 1.100.0, Rockbox's upgtool, and Sony's GPL kernel tree
# through cinder-llusbdac. The qemu preflight is skipped: it RUNS the device libraries.
set -euo pipefail
cd "$(dirname "$0")/.."
REPO="$PWD"
SETUP_ONLY=; [ "${1:-}" = --setup ] && SETUP_ONLY=1
step() { echo; echo "── $* ──"; }
[ "$REPO" = /home/sony/sony ] && [ "$HOME" = /home/sony ] \
    || echo "WARN: not at /home/sony/sony with HOME=/home/sony — the build will not match the manifest byte for byte"

fetch() {  # fetch "URL [MIRROR...]" SHA256 FILE — the first that answers; the hash decides, not the host
    echo "$2  $3" | sha256sum -c --status 2>/dev/null && return 0
    local u
    for u in $1; do
        curl -fsSL --retry 2 --connect-timeout 30 -o "$3" "$u" && echo "$2  $3" | sha256sum -c && return 0
        echo "  (not from $u)"
    done
    echo "ERR: no source gave $3 with sha256 $2" >&2
    return 1
}

# ── 1. packages (the same Ubuntu 24.04 ones the maintainer's machine has) ─────────────────────
step "packages"
if [ -z "${SKIP_APT:-}" ]; then
    sudo apt-get update -qq
    sudo apt-get install -y -qq --no-install-recommends \
        clang-18 lld-18 gcc-arm-linux-gnueabihf g++-arm-linux-gnueabihf binutils-arm-linux-gnueabihf \
        qemu-user-static libcrypto++-dev zlib1g-dev autoconf automake libtool pkg-config bison flex bc cpio \
        python3 g++ make patch xz-utils lzop curl git >/dev/null
fi
# Versions, for the log: a difference here is the first suspect when a file does not match.
clang++-18 --version | head -1
arm-linux-gnueabihf-gcc --version | head -1
arm-linux-gnueabihf-ld --version | head -1

# ── 2. toolchains in $HOME, where build.sh looks by default ──────────────────────────────────
step "toolchains"
T="$HOME/toolchains"; mkdir -p "$T/xenial-armhf-sysroot"
B=http://ports.ubuntu.com/ubuntu-ports/pool/main/g/glibc
( cd "$T/xenial-armhf-sysroot"
  fetch $B/libc6-dev_2.23-0ubuntu11.3_armhf.deb ac50323a473234365ccec0713594936d93158f85ef80cef530e1bcebd3e8c870 libc6-dev_2.23-0ubuntu11.3_armhf.deb
  fetch $B/libc6_2.23-0ubuntu11.3_armhf.deb eaa2753c2ad57c293f2c74e5edcd8363f17ad5d6daa06017f0d7711f973dcaec libc6_2.23-0ubuntu11.3_armhf.deb
  [ -d sysroot/usr/lib/arm-linux-gnueabihf ] || for d in *.deb; do dpkg-deb -x "$d" sysroot; done )
( cd "$T"
  fetch https://releases.llvm.org/3.9.0/libcxx-3.9.0.src.tar.xz d0b38d51365c6322f5666a2a8105785f2e114430858de4c25a86b49f227f5b06 libcxx-3.9.0.src.tar.xz
  [ -f libcxx-3.9.0.src/include/functional ] || tar xf libcxx-3.9.0.src.tar.xz )
# musl.cc's URL is not versioned, so the archive is pinned by hash: if it ever changes, this stops
# rather than building the helpers with a different compiler. GitHub's runners cannot reach musl.cc
# at all (2026-11-14, four 135 s timeouts), so the same file, same hash, is also a release asset.
MUSL_MIRROR="${MUSL_MIRROR:-https://github.com/superwilso/Cinder/releases/download/toolchains/arm-linux-musleabihf-cross.tgz}"
( cd "$HOME"
  fetch "https://musl.cc/arm-linux-musleabihf-cross.tgz $MUSL_MIRROR" 11e155a090789c854afb5d91e57c81f441ccdb92f7f277789c3a6d5884a68e58 arm-linux-musleabihf-cross.tgz
  [ -x arm-linux-musleabihf-cross/bin/arm-linux-musleabihf-gcc ] || tar xzf arm-linux-musleabihf-cross.tgz )
export PATH="$HOME/arm-linux-musleabihf-cross/bin:$PATH"

# Rust 1.100.0 under the name `stable`: the toolchain's folder name is in the payload's panic
# locations (with rust-src installed, std's paths point into it), so it must be called what the
# maintainer's is called.
if ! "$HOME/.cargo/bin/rustc" --version 2>/dev/null | grep -q "^rustc 1.100.0 "; then
    curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none >/dev/null
    R="$HOME/.rustup/toolchains"
    "$HOME/.cargo/bin/rustup" toolchain install 1.100.0 --profile minimal -c rust-src -t arm-unknown-linux-gnueabihf
    rm -rf "$R/stable-x86_64-unknown-linux-gnu"
    mv "$R/1.100.0-x86_64-unknown-linux-gnu" "$R/stable-x86_64-unknown-linux-gnu"
    "$HOME/.cargo/bin/rustup" default stable
fi
export PATH="$HOME/.cargo/bin:$PATH"
rustc --version

# Rockbox's upgtool and exec_file.sh, for the .UPG packages, at the commit phase 1 used.
RB="$REPO/artifacts/repos/rockbox"
if [ ! -x "$REPO/artifacts/upgtool" ]; then
    mkdir -p "$REPO/artifacts/repos"
    [ -d "$RB/.git" ] || git clone -q --filter=blob:none --sparse https://github.com/Rockbox/rockbox "$RB"
    git -C "$RB" sparse-checkout set utils/nwztools
    git -C "$RB" -c advice.detachedHead=false checkout -q 9936d93fea727642692957ff4d1eb5cde6fa1e50
    make -C "$RB/utils/nwztools/upgtools" >/dev/null
    cp "$RB/utils/nwztools/upgtools/upgtool" "$REPO/artifacts/upgtool"
fi

# The Walkman One package is sealed with the upgtool in unknown321/nw-installer (`-w`), as
# tools/release.sh does on the maintainer's machine: run as a build tool, none of it is in Cinder.
NWI="$REPO/artifacts/repos/nw-installer"
if [ ! -x "$NWI/tools/upgtool/upgtools/upgtool" ]; then
    [ -d "$NWI/.git" ] || git clone -q https://github.com/unknown321/nw-installer "$NWI"
    git -C "$NWI" -c advice.detachedHead=false checkout -q 98d919ac985b6108e03f60a30bcec6b86a4858a0
    make -C "$NWI/tools/upgtool/upgtools" >/dev/null
fi

# Sony's GPL kernel tree, prepared, for the Wi-Fi modules: cinder-llusbdac's build makes it.
LL="$HOME/cinder-llusbdac"
if [ ! -f "$LL/work/kernel/.prepared" ]; then
    [ -d "$LL/.git" ] || git clone -q https://github.com/superwilso/cinder-llusbdac "$LL"
    git -C "$LL" -c advice.detachedHead=false checkout -q b83af7deeda6653088031ea3830ade693b2a1c5c
    bash "$LL/build.sh"
fi
[ -n "$SETUP_ONLY" ] && { echo "setup done"; exit 0; }

# ── 3. the stand-ins, then the same steps tools/release.sh takes ──────────────────────────────
step "stand-in libraries"
STUBS="$(mktemp -d)"
python3 cinder-home/tools/stubs.py build "$STUBS"
python3 cinder-home/tools/stubs.py check "$STUBS"
export CINDER_STUBS="$STUBS"

for ch in stable dev; do
    step "build $ch"
    bash cinder-home/build.sh "$ch"
    for model in nw-a50 nw-wm1a; do
        out="$(bash cinder-home/tools/pack_upg.sh "$ch" "$model" 2>&1)" || { echo "$out"; exit 1; }
    done
done
bash tools/check_arm_payload.sh cinder-home/dist/dev

# ── 4. the comparison ─────────────────────────────────────────────────────────────────────────
step "compare with PAYLOAD.sha256"
bash tools/verify_payload_manifest.sh --with-dev
