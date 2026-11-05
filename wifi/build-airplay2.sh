#!/usr/bin/env bash
# build-airplay2.sh — the AirPlay 2 receiver, static for the player, beside build-userland.sh's
# AirPlay 1 one:
#
#   out/shairport-sync-ap2   Shairport Sync 5.5.2 built --with-airplay-2 (MIT), audio to stdout,
#                            mDNS by its bundled tinysvcmdns (patched to announce _airplay._tcp too:
#                            airplay/tinysvcmdns-airplay2.patch)
#   out/nqptp                NQPTP 1.2.4 (GPL-2.0), the PTP timing AirPlay 2 needs; it must own
#                            UDP ports 319 and 320, so the player starts it as root (cinder-wifi),
#                            and it stops being root once they are bound
#                            (airplay/nqptp-drop-root.patch)
#
# Libraries, all static: libsodium (ISC), libgcrypt + libgpg-error (LGPL-2.1), libplist (LGPL-2.1),
# FFmpeg 7.1 cut down to the AAC and ALAC decoders and the resampler (LGPL-2.1), OpenSSL, popt and
# libconfig from build-userland.sh (run it first), and airplay/uuid (the three libuuid calls
# shairport-sync makes). The build machine needs autoconf, automake, libtool and xxd; plistutil is
# built here from libplist itself. Every download is pinned by sha256.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
MUSL="${MUSL:-$HOME/arm-linux-musleabihf-cross}"
export PATH="$MUSL/bin:$PATH"
HOST=arm-linux-musleabihf
CC=$HOST-gcc
UP="$HERE/work/userland/prefix"            # build-userland.sh's (OpenSSL, popt, libconfig)
W="$HERE/work/airplay2"; P="$W/prefix"; HP="$W/host"; SRC="$HERE/work/userland/src"; OUT="$HERE/out"
mkdir -p "$W" "$P/lib" "$P/include" "$HP" "$SRC" "$OUT"
[ -f "$UP/lib/libssl.a" ] || { echo "run build-userland.sh first (OpenSSL, popt, libconfig)"; exit 1; }
for t in autoreconf libtoolize xxd; do command -v $t >/dev/null || { echo "needs $t on the build machine"; exit 1; }; done
NPROC="$(nproc)"

fetch() {  # url sha256 [name]
    local f="$SRC/${3:-$(basename "$1")}"
    echo "$2  $f" | sha256sum -c --status 2>/dev/null || { curl -fsSL --retry 3 -o "$f" "$1"; echo "$2  $f" | sha256sum -c; }
}
fetch https://download.libsodium.org/libsodium/releases/libsodium-1.0.20.tar.gz ebb65ef6ca439333c2bb41a0c1990587288da07f6c7fd07cb3a18cc18d30ce19
fetch https://gnupg.org/ftp/gcrypt/libgpg-error/libgpg-error-1.51.tar.bz2 be0f1b2db6b93eed55369cdf79f19f72750c8c7c39fc20b577e724545427e6b2
# 1.10, not 1.11: 1.11's Classic McEliece defines gf_mul, as OpenSSL's curve448 does, and the two
# cannot be linked statically into one program.
fetch https://gnupg.org/ftp/gcrypt/libgcrypt/libgcrypt-1.10.3.tar.bz2 8b0870897ac5ac67ded568dcfadf45969cfa8a6beb0fd60af2a9eadc2a3272aa
fetch https://github.com/libimobiledevice/libplist/releases/download/2.6.0/libplist-2.6.0.tar.bz2 67be9ee3169366589c92dc7c22809b90f51911dd9de22520c39c9a64fb047c9c
fetch https://ffmpeg.org/releases/ffmpeg-7.1.tar.xz 40973d44970dbc83ef302b0609f2e74982be2d85916dd2ee7472d30678a7abe6
fetch https://github.com/mikebrady/nqptp/archive/refs/tags/1.2.4.tar.gz 1df1d5edd5b713010d6495b3abca4c1cf4ad8fa6029df0abeb9e4de8e0eb707a nqptp-1.2.4.tar.gz
fetch https://github.com/mikebrady/shairport-sync/archive/refs/tags/5.5.2.tar.gz abcdb59674b6eedb4f3f6228f3c702e65d2cdc037231b0c48617fd90891b49e9 shairport-sync-5.5.2.tar.gz

unpack() { [ -d "$W/$1" ] || tar xf "$SRC/$2" -C "$W"; }
# An autotools package for the player, into $P. $1 = its directory, the rest = configure options.
cross() {
    local d="$1"; shift
    ( cd "$W/$d" && ./configure --host=$HOST --prefix="$P" --disable-shared --enable-static "$@" \
          CC=$CC CFLAGS="-Os -fPIC" && make -j"$NPROC" && make install ) >"$W/$d.log" 2>&1 \
        || { tail -30 "$W/$d.log"; echo "FAILED: $d (log: $W/$d.log)"; exit 1; }
}

echo "== libsodium 1.0.20"
unpack libsodium-1.0.20 libsodium-1.0.20.tar.gz
[ -f "$P/lib/libsodium.a" ] || cross libsodium-1.0.20

echo "== libgpg-error 1.51 + libgcrypt 1.10.3"
unpack libgpg-error-1.51 libgpg-error-1.51.tar.bz2
[ -f "$P/lib/libgpg-error.a" ] || cross libgpg-error-1.51 --disable-nls --disable-doc --disable-tests --disable-languages
unpack libgcrypt-1.10.3 libgcrypt-1.10.3.tar.bz2
[ -f "$P/lib/libgcrypt.a" ] || cross libgcrypt-1.10.3 --with-libgpg-error-prefix="$P" --disable-doc --disable-asm

echo "== libplist 2.6.0 (the player's, and plistutil for this machine)"
unpack libplist-2.6.0 libplist-2.6.0.tar.bz2
if [ ! -x "$HP/bin/plistutil" ]; then
    rm -rf "$W/libplist-host" && cp -a "$W/libplist-2.6.0" "$W/libplist-host"
    ( cd "$W/libplist-host" && ./configure --prefix="$HP" --without-cython --disable-shared && make -j"$NPROC" && make install ) \
        >"$W/libplist-host.log" 2>&1 || { tail -30 "$W/libplist-host.log"; exit 1; }
fi
[ -f "$P/lib/libplist-2.0.a" ] || cross libplist-2.6.0 --without-cython --without-tests

echo "== FFmpeg 7.1 (AAC and ALAC decoders, the resampler; libavformat, empty, because configure asks)"
unpack ffmpeg-7.1 ffmpeg-7.1.tar.xz
if [ ! -f "$P/lib/libavcodec.a" ]; then
    ( cd "$W/ffmpeg-7.1" && ./configure --prefix="$P" --enable-cross-compile --cross-prefix=$HOST- --arch=arm \
          --cpu=cortex-a7 --target-os=linux --enable-static --disable-shared --disable-programs --disable-doc \
          --disable-autodetect --disable-everything --disable-network --enable-avformat --disable-avdevice \
          --disable-avfilter --disable-swscale --enable-avcodec --enable-avutil --enable-swresample \
          --enable-decoder=aac,alac --enable-parser=aac --extra-cflags="-Os -fPIC" \
          && make -j"$NPROC" && make install ) >"$W/ffmpeg.log" 2>&1 || { tail -30 "$W/ffmpeg.log"; exit 1; }
fi

echo "== uuid (airplay/uuid)"
mkdir -p "$P/include/uuid"
cp "$HERE/airplay/uuid/uuid.h" "$P/include/uuid/uuid.h"
$CC -Os -fPIC -Wall -Wextra -Wno-deprecated-declarations -I"$UP/include" -c -o "$W/uuid.o" "$HERE/airplay/uuid/uuid.c"
$HOST-ar rcs "$P/lib/libuuid.a" "$W/uuid.o"

echo "== nqptp 1.2.4"
if [ ! -d "$W/nqptp-1.2.4" ]; then
    tar xf "$SRC/nqptp-1.2.4.tar.gz" -C "$W"
    ( cd "$W/nqptp-1.2.4" && patch -p1 < "$HERE/airplay/nqptp-drop-root.patch" )
fi
if [ ! -f "$W/nqptp-1.2.4/nqptp" ]; then
    ( cd "$W/nqptp-1.2.4" && autoreconf -fi && ./configure --host=$HOST CC=$CC CFLAGS="-Os" LDFLAGS="-static" \
          && make -j"$NPROC" ) >"$W/nqptp.log" 2>&1 || { tail -30 "$W/nqptp.log"; exit 1; }
fi

echo "== shairport-sync 5.5.2 for AirPlay 2"
if [ ! -d "$W/shairport-sync-5.5.2" ]; then
    tar xf "$SRC/shairport-sync-5.5.2.tar.gz" -C "$W"
    ( cd "$W/shairport-sync-5.5.2" && patch -p1 < "$HERE/airplay/tinysvcmdns-airplay2.patch" )
fi
if [ ! -f "$W/shairport-sync-5.5.2/shairport-sync" ]; then
    ( cd "$W/shairport-sync-5.5.2" && autoreconf -fi \
          && PATH="$HP/bin:$PATH" PKG_CONFIG_LIBDIR="$P/lib/pkgconfig:$UP/lib/pkgconfig" ./configure --host=$HOST \
               --with-airplay-2 --with-ssl=openssl --with-tinysvcmdns --with-stdout --with-pipe \
               --without-alsa --without-soxr --without-avahi --without-dbus-interface --without-mpris-interface \
               CC=$CC CFLAGS="-Os -I$P/include -I$UP/include" LDFLAGS="-static -L$P/lib -L$UP/lib" \
               LIBS="-luuid -lcrypto -lgcrypt -lgpg-error -lsodium -lm -lpthread" \
          && PATH="$HP/bin:$PATH" make -j"$NPROC" ) >"$W/shairport.log" 2>&1 || { tail -40 "$W/shairport.log"; exit 1; }
fi

$HOST-strip -o "$OUT/shairport-sync-ap2" "$W/shairport-sync-5.5.2/shairport-sync"
$HOST-strip -o "$OUT/nqptp" "$W/nqptp-1.2.4/nqptp"
( cd "$OUT" && sha256sum shairport-sync-ap2 nqptp | tee SHA256SUMS.airplay2 )
ls -l "$OUT/shairport-sync-ap2" "$OUT/nqptp"
