#!/bin/sh
# cinder-preload.sh — mono, and soundscapes over music, on a player with NO Wampy.
#
#   cinder-preload.sh install <cinder-hagowrap> <libcinder_mono.so> <cinder-guard.sh>
#   cinder-preload.sh remove
#   cinder-preload.sh status
#
# Run as root with /system writable: by install_cinderhome.sh in Sony's updater, by
# uninstall_cinderhome.sh, and by tools/cinder-install.sh --preload on a live player. This is the
# `preload` component (deploy/components.conf), OFF by default.
#
# WHAT IT PUTS ON THE PLAYER, in this order, each one copied to a temporary name, compared byte for
# byte and renamed into place:
#   1. /system/bin/cinder-guard.sh        the boot guard (deploy/cinder-guard.sh)
#   2. /system/bin/bootswitcher.sh        Sony's script plus six lines that call the guard; the
#                                         original kept as bootswitcher.sh.precinder
#   3. $LIBDIR/libcinder_mono.so          the shim (src/cinder-mono.c)
#   4. $SONYBIN/hagodaemon                src/cinder-hagowrap.c; Sony's kept as hagodaemon.real
# The wrapper goes LAST. It stands in front of every Sony service, and the guard is what takes it
# away again if the player does not come up, so it is never installed on a player where steps 1
# to 3 did not all succeed. A failure at any step leaves the player with Sony's hagodaemon.
#
# THE BOOT SCRIPT IS PATCHED ONLY WHEN IT IS EXACTLY THE ONE THIS WAS WRITTEN FOR. init waits on
# bootswitcher.sh, and its last lines are what start Sony's services, so nothing is edited by
# pattern: the file must hash to Sony's (the same on stock 1.02 and Walkman One 3.02), and the
# result must hash to the patched file that has booted a player since 2026-09-21. Any other
# bootswitcher.sh, and nothing at all is installed.
#
# The wrapper is then on trial until the player has played audio through it: see cinder-guard.sh.

BB=/xbin/busybox
[ -x "$BB" ] || BB=/system/xbin/busybox
[ -x "$BB" ] || BB=busybox

SONYBIN=/system/vendor/sony/bin
HAGO=$SONYBIN/hagodaemon
LIBDIR=/system/vendor/unknown321/lib
SHIM=$LIBDIR/libcinder_mono.so
WAMPY=$LIBDIR/libsound_service_fw.so
BS=/system/bin/bootswitcher.sh
GUARD=/system/bin/cinder-guard.sh
BS_STOCK=7675f7d234b0abadf2909f2e9a274d51fe1d02620d776c4ac6d2ce865ecac358
BS_HOOKED=9ce70f838ef0cdc092acd2028294cc7f2d4d78f619185a67476082fe251de6c6
BS_SPLIT=46          # the hook goes after this line of Sony's script, before "# set properties"
MARK=cinder-hagowrap # a string only the wrapper carries (its own error message)

say() { echo "preload: $*"; }
sha() { "$BB" sha256sum "$1" 2>/dev/null | "$BB" cut -d' ' -f1; }
is_wrapper() { "$BB" grep -q "$MARK" "$1" 2>/dev/null; }

# put <src> <dst> <mode> <owner> — temporary name, compared, renamed. Never writes in place: on a
# live player the running services have the old file mapped.
put() {
    "$BB" cat "$1" > "$2.tmp" 2>/dev/null
    if [ -s "$2.tmp" ] && "$BB" cmp -s "$1" "$2.tmp"; then
        "$BB" chown "$4" "$2.tmp" 2>/dev/null
        "$BB" chmod "$3" "$2.tmp"
        "$BB" mv -f "$2.tmp" "$2" && return 0
    fi
    "$BB" rm -f "$2.tmp" 2>/dev/null
    return 1
}

hook_text() {
    "$BB" cat <<'HOOK'
# Cinder boot guard. Must run BEFORE the bootmode property below, because setting it is what
# starts class hagoromo and therefore appmgr — i.e. the last moment that is still upstream of every
# Home app. Guarded by -x, so a missing guard simply skips and the boot is unaffected.
if [ -x /system/bin/cinder-guard.sh ]; then
    /bin/sh /system/bin/cinder-guard.sh
fi

HOOK
}

# 0 when bootswitcher.sh calls the guard afterwards (it already did, or it was Sony's and now does).
hook_install() {
    H="$(sha "$BS")"
    if [ "$H" = "$BS_HOOKED" ]; then
        say "bootswitcher.sh already calls the guard"
        return 0
    fi
    if [ "$H" != "$BS_STOCK" ]; then
        say "bootswitcher.sh is not the script this was written for (sha256 ${H:-unreadable})"
        return 1
    fi
    { "$BB" head -n "$BS_SPLIT" "$BS"; hook_text; "$BB" tail -n +$((BS_SPLIT + 1)) "$BS"; } > "$BS.tmp" 2>/dev/null
    if [ "$(sha "$BS.tmp")" != "$BS_HOOKED" ]; then
        "$BB" rm -f "$BS.tmp" 2>/dev/null
        say "the patched bootswitcher.sh did not come out as expected - left as Sony's"
        return 1
    fi
    if ! put "$BS" "$BS.precinder" 755 0:0 || [ "$(sha "$BS.precinder")" != "$BS_STOCK" ]; then
        "$BB" rm -f "$BS.tmp" 2>/dev/null
        say "could not keep a copy of Sony's bootswitcher.sh - left as Sony's"
        return 1
    fi
    "$BB" chown 0:2000 "$BS.tmp" 2>/dev/null
    "$BB" chmod 755 "$BS.tmp"
    "$BB" mv -f "$BS.tmp" "$BS" || return 1
    say "bootswitcher.sh now calls the guard (Sony's kept as $BS.precinder)"
    return 0
}

hook_remove() {
    [ "$(sha "$BS")" = "$BS_HOOKED" ] || return 0
    if [ "$(sha "$BS.precinder")" != "$BS_STOCK" ]; then
        say "no copy of Sony's bootswitcher.sh to put back - the hook stays (it does nothing without the guard)"
        return 0
    fi
    "$BB" cat "$BS.precinder" > "$BS.tmp" 2>/dev/null
    if [ "$(sha "$BS.tmp")" = "$BS_STOCK" ]; then
        "$BB" chown 0:2000 "$BS.tmp" 2>/dev/null
        "$BB" chmod 755 "$BS.tmp"
        "$BB" mv -f "$BS.tmp" "$BS" && "$BB" rm -f "$BS.precinder" && say "bootswitcher.sh is Sony's again"
    else
        "$BB" rm -f "$BS.tmp" 2>/dev/null
        say "could not restore bootswitcher.sh - the hook stays (it does nothing without the guard)"
    fi
}

wrapper_remove() {
    if [ -s "$HAGO.real" ] && is_wrapper "$HAGO" && ! is_wrapper "$HAGO.real"; then
        if put "$HAGO.real" "$HAGO" 755 0:2000; then
            "$BB" rm -f "$HAGO.real"
            say "Sony's hagodaemon is back in place"
        else
            say "COULD NOT put Sony's hagodaemon back - the wrapper stays (it only passes through)"
            return 1
        fi
    elif [ -e "$HAGO.real" ] && ! is_wrapper "$HAGO"; then
        "$BB" rm -f "$HAGO.real"        # Sony's binary already runs; the kept copy is spare
    fi
    return 0
}

case "${1:-}" in
install)
    WRAP="${2:-}"; NEWSHIM="${3:-}"; NEWGUARD="${4:-}"
    for f in "$WRAP" "$NEWSHIM" "$NEWGUARD"; do
        if [ ! -s "$f" ]; then say "'$f' is not staged - nothing installed"; exit 1; fi
    done
    if ! is_wrapper "$WRAP"; then say "'$WRAP' is not the wrapper - nothing installed"; exit 1; fi
    if [ -e "$WAMPY" ]; then
        say "Wampy is installed: the mono component already reaches the sound service - nothing to do"
        exit 0
    fi
    if [ ! -s "$HAGO" ]; then say "$HAGO not found - nothing installed"; exit 1; fi
    # A wrapper with no Sony binary kept beside it, or a kept copy that is itself a wrapper: the
    # one state this cannot repair, because the only copy of Sony's binary is gone.
    if is_wrapper "$HAGO" && { [ ! -s "$HAGO.real" ] || is_wrapper "$HAGO.real"; }; then
        say "$HAGO is a wrapper and Sony's binary is not beside it - not touching it"
        exit 1
    fi
    # Asked before anything is written: on a boot script this does not know, nothing is installed.
    case "$(sha "$BS")" in
        "$BS_STOCK"|"$BS_HOOKED") ;;
        *) say "bootswitcher.sh is not the script this was written for (sha256 $(sha "$BS")) - nothing installed"
           exit 1 ;;
    esac
    put "$NEWGUARD" "$GUARD" 755 0:0 || { say "could not install the guard - nothing else installed"; exit 1; }
    hook_install || { say "without the guard in the boot script the wrapper is not installed"; exit 1; }
    "$BB" mkdir -p "$LIBDIR" 2>/dev/null
    put "$NEWSHIM" "$SHIM" 755 0:0 || { say "could not install the shim - the wrapper is not installed"; exit 1; }
    if ! is_wrapper "$HAGO"; then
        if ! put "$HAGO" "$HAGO.real" 755 0:2000; then
            say "could not keep a copy of Sony's hagodaemon - the wrapper is not installed"
            exit 1
        fi
    fi
    if put "$WRAP" "$HAGO" 755 0:2000; then
        say "installed: wrapper $("$BB" wc -c < "$HAGO" | "$BB" tr -cd '0-9') bytes in front of Sony's hagodaemon (kept as $HAGO.real)"
        say "on trial until the player has played audio once; the guard puts Sony's binary back otherwise"
    else
        say "could not install the wrapper - Sony's hagodaemon is unchanged"
        exit 1
    fi
    ;;
remove)
    # The wrapper first and the guard last: while a wrapper is in place, so is what removes it.
    wrapper_remove || exit 1
    if [ -f "$SHIM" ] && [ ! -e "$WAMPY" ]; then "$BB" rm -f "$SHIM" && say "shim removed"; fi
    hook_remove
    if [ "$(sha "$BS")" != "$BS_HOOKED" ] && [ -f "$GUARD" ]; then "$BB" rm -f "$GUARD" && say "guard removed"; fi
    ;;
status)
    if is_wrapper "$HAGO"; then say "wrapper: installed"; else say "wrapper: not installed"; fi
    if [ -s "$HAGO.real" ]; then say "Sony's hagodaemon: kept as $HAGO.real"; fi
    case "$(sha "$BS")" in
        "$BS_HOOKED") say "bootswitcher.sh: calls the guard" ;;
        "$BS_STOCK")  say "bootswitcher.sh: Sony's" ;;
        *)            say "bootswitcher.sh: neither Sony's nor the patched one" ;;
    esac
    if [ -s "$GUARD" ]; then say "guard: present"; else say "guard: absent"; fi
    if [ -s "$SHIM" ]; then say "shim: present"; else say "shim: absent"; fi
    ;;
*)
    echo "usage: cinder-preload.sh install <cinder-hagowrap> <libcinder_mono.so> <cinder-guard.sh> | remove | status"
    exit 2
    ;;
esac
exit 0
