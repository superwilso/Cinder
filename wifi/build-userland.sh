#!/usr/bin/env bash
# build-userland.sh — the programs the player needs beside the modules, static, so they do not
# depend on the player's glibc 2.23:
#
#   out/wpa_supplicant, out/wpa_cli   join a network (WPA2, WPA3/SAE, OWE), OpenSSL 3.0 inside
#   out/iw                            inspect the radio
#   out/wifiscan                      a scan with no libraries (tools/wifiscan.c)
#
# Needs an ARM musl cross compiler (MUSL, default ~/arm-linux-musleabihf-cross, from musl.cc) and,
# for libnl 3, bison and flex on the build machine. Every download is pinned by sha256.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
MUSL="${MUSL:-$HOME/arm-linux-musleabihf-cross}"
export PATH="$MUSL/bin:$PATH"
CC=arm-linux-musleabihf-gcc
W="$HERE/work/userland"; OUT="$HERE/out"; SRC="$W/src"; P="$W/prefix"
mkdir -p "$SRC" "$P" "$OUT"

fetch() {  # fetch URL SHA256
    local f="$SRC/$(basename "$1")"
    echo "$2  $f" | sha256sum -c --status 2>/dev/null || { curl -fsSL --retry 3 -o "$f" "$1"; echo "$2  $f" | sha256sum -c; }
}
fetch https://www.openssl.org/source/openssl-3.0.15.tar.gz 23c666d0edf20f14249b3d8f0368acaee9ab585b09e1de82107c66e1f3ec9533
fetch https://w1.fi/releases/wpa_supplicant-2.10.tar.gz 20df7ae5154b3830355f8ab4269123a87affdea59fe74fe9292a91d0d7e17b2f
fetch https://github.com/thom311/libnl/releases/download/libnl3_7_0/libnl-3.7.0.tar.gz 9fe43ccbeeea72c653bdcf8c93332583135cda46a79507bfd0a483bb57f65939
fetch https://mirrors.edge.kernel.org/pub/software/network/iw/iw-5.19.tar.xz f167bbe947dd53bb9ebc0c1dcef5db6ad73ac1d6084f2c6f9376c5c360cc4d4e
LIBNL_TINY_COMMIT=40493a655d8caa2ccf5206dde1e733abe2920432
[ -d "$SRC/libnl-tiny" ] || git clone -q https://github.com/openwrt/libnl-tiny.git "$SRC/libnl-tiny"
git -C "$SRC/libnl-tiny" -c advice.detachedHead=false checkout -q "$LIBNL_TINY_COMMIT"

echo "== OpenSSL 3.0.15"
if [ ! -f "$P/lib/libcrypto.a" ]; then
    tar xzf "$SRC/openssl-3.0.15.tar.gz" -C "$W"
    ( cd "$W/openssl-3.0.15" && ./Configure linux-armv4 no-shared no-tests no-engine no-dso -static \
          --prefix="$P" --cross-compile-prefix=arm-linux-musleabihf- && make -j"$(nproc)" && make install_sw ) >"$W/openssl.log" 2>&1
fi

echo "== libnl-tiny (for wpa_supplicant)"
T="$SRC/libnl-tiny"
( cd "$T" && mkdir -p obj && for f in attr cache cache_mngt error genl genl_ctrl genl_family genl_mngt handlers msg nl object socket unl; do
      $CC -Os -Iinclude -c "$f.c" -o "obj/$f.o"; done && arm-linux-musleabihf-ar rcs libnl-tiny.a obj/*.o ) 2>"$W/libnl-tiny.log"

echo "== wpa_supplicant 2.10"
WPA="$W/wpa_supplicant-2.10"
[ -d "$WPA" ] || tar xzf "$SRC/wpa_supplicant-2.10.tar.gz" -C "$W"
cat >"$WPA/wpa_supplicant/.config" <<'EOF'
CONFIG_DRIVER_NL80211=y
CONFIG_LIBNL_TINY=y
CONFIG_TLS=openssl
CONFIG_CTRL_IFACE=y
CONFIG_BACKEND=file
CONFIG_SAE=y
CONFIG_IEEE80211W=y
CONFIG_OWE=y
CONFIG_GETRANDOM=y
EOF
make -C "$WPA/wpa_supplicant" -j"$(nproc)" CC=$CC EXTRA_CFLAGS="-I$T/include -I$P/include -D_GNU_SOURCE" \
    LDFLAGS="-static -L$T -L$P/lib" >"$W/wpa.log" 2>&1

echo "== libnl 3.7 + iw 5.19"
if [ ! -f "$P/lib/libnl-genl-3.a" ]; then
    tar xzf "$SRC/libnl-3.7.0.tar.gz" -C "$W"
    ( cd "$W/libnl-3.7.0" && ./configure --host=arm-linux-musleabihf --prefix="$P" --enable-static \
          --disable-shared --disable-cli --disable-debug && make -j"$(nproc)" && make install ) >"$W/libnl.log" 2>&1
fi
[ -d "$W/iw-5.19" ] || tar xJf "$SRC/iw-5.19.tar.xz" -C "$W"
PKG_CONFIG_LIBDIR="$P/lib/pkgconfig" PKG_CONFIG_PATH= make -C "$W/iw-5.19" -j"$(nproc)" CC=$CC LDFLAGS=-static >"$W/iw.log" 2>&1

echo "== wifiscan"
$CC -static -Os -Wall -Wextra -o "$W/wifiscan" "$HERE/tools/wifiscan.c"

for b in "$WPA/wpa_supplicant/wpa_supplicant" "$WPA/wpa_supplicant/wpa_cli" "$W/iw-5.19/iw" "$W/wifiscan"; do
    arm-linux-musleabihf-strip -o "$OUT/$(basename "$b")" "$b"
done
( cd "$OUT" && sha256sum wpa_supplicant wpa_cli iw wifiscan | tee SHA256SUMS.userland )
