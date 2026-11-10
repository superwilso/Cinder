#!/usr/bin/env bash
# build-userland.sh — the programs the player needs beside the modules, static, so they do not
# depend on the player's glibc 2.23:
#
#   out/wpa_supplicant, out/wpa_cli   join a network (WPA2, WPA3/SAE, OWE), OpenSSL 3.0 inside
#   out/iw                            inspect the radio
#   out/wifiscan                      a scan with no libraries (tools/wifiscan.c)
#   out/curl, out/cacert.pem          HTTPS for scrobbling and update checks (curl licence; Mozilla's
#                                     CA list, MPL-2.0, as curl.se publishes it)
#   out/shairport-sync                the AirPlay receiver (MIT; popt MIT, libconfig LGPL-2.1, both
#                                     static) — AirPlay 1, OpenSSL, tinysvcmdns, audio to stdout
#   out/openssl                       OpenSSL's own command line (Apache-2.0), for `s_client`: the
#                                     TLS that Google Cast speakers talk (Settings ▸ Wi-Fi ▸ Speakers)
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
    local f; f="$SRC/$(basename "$1")"
    echo "$2  $f" | sha256sum -c --status 2>/dev/null || { curl -fsSL --retry 3 -o "$f" "$1"; echo "$2  $f" | sha256sum -c; }
}
fetch https://www.openssl.org/source/openssl-3.0.15.tar.gz 23c666d0edf20f14249b3d8f0368acaee9ab585b09e1de82107c66e1f3ec9533
fetch https://w1.fi/releases/wpa_supplicant-2.10.tar.gz 20df7ae5154b3830355f8ab4269123a87affdea59fe74fe9292a91d0d7e17b2f
fetch https://github.com/thom311/libnl/releases/download/libnl3_7_0/libnl-3.7.0.tar.gz 9fe43ccbeeea72c653bdcf8c93332583135cda46a79507bfd0a483bb57f65939
fetch https://mirrors.edge.kernel.org/pub/software/network/iw/iw-5.19.tar.xz f167bbe947dd53bb9ebc0c1dcef5db6ad73ac1d6084f2c6f9376c5c360cc4d4e
fetch https://curl.se/download/curl-8.22.0.tar.xz f7ef3ae8a22e521f289803fe93543eb64c329b58aa73a9e224dfd915a2a5f4f7
fetch https://curl.se/ca/cacert-2026-10-20.pem 796f1389bfea13c0ed78f663b765baaff54a508e5483786bd6ece73c5e3e3cbe
fetch https://ftp.osuosl.org/pub/rpm/popt/releases/popt-1.x/popt-1.19.tar.gz c25a4838fc8e4c1c8aacb8bd620edb3084a3d63bf8987fdad3ca2758c63240f9
fetch https://github.com/hyperrealm/libconfig/releases/download/v1.7.3/libconfig-1.7.3.tar.gz 545166d6cac037744381d1e9cc5a5405094e7bfad16a411699bcff40bbb31ee7
fetch https://github.com/mikebrady/shairport-sync/archive/refs/tags/5.5.2.tar.gz abcdb59674b6eedb4f3f6228f3c702e65d2cdc037231b0c48617fd90891b49e9
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
CONFIG_IEEE8021X_EAPOL=y
CONFIG_EAP_PEAP=y
CONFIG_EAP_TTLS=y
CONFIG_EAP_MSCHAPV2=y
CONFIG_EAP_GTC=y
EOF
# The methods are compiled in only when listed, and the objects do not notice a changed .config.
make -C "$WPA/wpa_supplicant" clean >/dev/null 2>&1
make -C "$WPA/wpa_supplicant" -j"$(nproc)" CC=$CC EXTRA_CFLAGS="-I$T/include -I$P/include -D_GNU_SOURCE" \
    LDFLAGS="-static -L$T -L$P/lib" >"$W/wpa.log" 2>&1

echo "== libnl 3.7 + iw 5.19"
if [ ! -f "$P/lib/libnl-genl-3.a" ]; then
    tar xzf "$SRC/libnl-3.7.0.tar.gz" -C "$W"
    ( cd "$W/libnl-3.7.0" && ./configure --host=arm-linux-musleabihf --prefix="$P" --enable-static \
          --disable-shared --disable-cli --disable-debug && make -j"$(nproc)" && make install ) >"$W/libnl.log" 2>&1
fi
[ -d "$W/iw-5.19" ] || tar xJf "$SRC/iw-5.19.tar.xz" -C "$W"
PKG_CONFIG_LIBDIR="$P/lib/pkgconfig" PKG_CONFIG_PATH='' make -C "$W/iw-5.19" -j"$(nproc)" CC=$CC LDFLAGS=-static >"$W/iw.log" 2>&1

echo "== curl 8.22.0 (HTTPS only)"
if [ ! -f "$W/curl-8.22.0/src/curl" ]; then
    [ -d "$W/curl-8.22.0" ] || tar xJf "$SRC/curl-8.22.0.tar.xz" -C "$W"
    ( cd "$W/curl-8.22.0" && ./configure --host=arm-linux-musleabihf --with-openssl="$P" --disable-shared \
          --enable-static --disable-ldap --disable-ftp --disable-file --disable-dict --disable-telnet \
          --disable-tftp --disable-pop3 --disable-imap --disable-smtp --disable-gopher --disable-mqtt \
          --disable-rtsp --disable-smb --disable-manual --disable-docs --without-libpsl --without-zlib \
          --without-brotli --without-zstd --without-nghttp2 --without-libidn2 --without-ca-bundle \
          --without-ca-path LDFLAGS="-static -L$P/lib" CPPFLAGS="-I$P/include" LIBS="-lssl -lcrypto" \
          && make -j"$(nproc)" LDFLAGS="-all-static -L$P/lib" ) >"$W/curl.log" 2>&1
fi
cp -f "$SRC/cacert-2026-10-20.pem" "$OUT/cacert.pem"

echo "== shairport-sync 5.5.2 (AirPlay 1), with popt 1.19 and libconfig 1.7.3"
for t in popt-1.19 libconfig-1.7.3; do [ -d "$W/$t" ] || tar xzf "$SRC/$t.tar.gz" -C "$W"; done
[ -f "$P/lib/libpopt.a" ] || ( cd "$W/popt-1.19" && ./configure --host=arm-linux-musleabihf --prefix="$P" \
    --enable-static --disable-shared --disable-nls && make -j"$(nproc)" && make install ) >"$W/popt.log" 2>&1
[ -f "$P/lib/libconfig.a" ] || ( cd "$W/libconfig-1.7.3" && ./configure --host=arm-linux-musleabihf --prefix="$P" \
    --enable-static --disable-shared --disable-cxx --disable-examples --disable-tests && make -j"$(nproc)" -C lib \
    && make -C lib install ) >"$W/libconfig.log" 2>&1
SS="$W/shairport-sync-5.5.2"
[ -d "$SS" ] || tar xzf "$SRC/5.5.2.tar.gz" -C "$W"
# No autotools: the GitHub archive has no configure script, and the build needs only the sources
# below and the config.h beside this script (airplay/config.h).
cp -f "$HERE/airplay/config.h" "$SS/config.h"
( cd "$SS" && $CC -Os -static -pthread -include config.h --include=utilities/debug.h -I. -I"$P/include" \
    -DSYSCONFDIR=\"/system/vendor/cinder/wifi\" -w -o "$W/shairport-sync" \
    shairport.c bonjour_strings.c mdns.c common.c rtp.c player.c audio.c loudness.c activity_monitor.c \
    rtsp.c utilities/debug.c utilities/network_utilities.c utilities/rtsp_message_utilities.c \
    utilities/string_utilities.c utilities/exit.c alac.c mdns_tinysvcmdns.c tinysvcmdns.c \
    audio_stdout.c audio_pipe.c -L"$P/lib" -lpopt -lconfig -lssl -lcrypto -lm ) >"$W/shairport.log" 2>&1

echo "== wifiscan"
$CC -static -Os -Wall -Wextra -o "$W/wifiscan" "$HERE/tools/wifiscan.c"

for b in "$WPA/wpa_supplicant/wpa_supplicant" "$WPA/wpa_supplicant/wpa_cli" "$W/iw-5.19/iw" "$W/wifiscan" \
         "$W/curl-8.22.0/src/curl" "$W/shairport-sync" "$W/openssl-3.0.15/apps/openssl"; do
    arm-linux-musleabihf-strip -o "$OUT/$(basename "$b")" "$b"
done
( cd "$OUT" && sha256sum wpa_supplicant wpa_cli iw wifiscan curl cacert.pem shairport-sync openssl | tee SHA256SUMS.userland )
