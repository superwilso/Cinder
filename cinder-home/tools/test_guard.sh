#!/bin/bash
# test_guard.sh — deploy/cinder-guard.sh on the host.
#
# The guard runs from bootswitcher.sh before a single Sony service starts, so nothing on the player
# can test it without betting a boot on it. Here it runs against a scratch root: its absolute paths
# are rewritten into a temporary directory (and nothing else about the script changes), /xbin/busybox
# is a stub that hands each applet to the host's tool of the same name, and mount/sync are no-ops.
# Each "boot" is one run of the script; what cinder-home would have written between two boots is
# written by the case.
set -u
HERE="$(cd "$(dirname "$0")/.." && pwd)"
T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
pass=0; fail=0
ok()  { echo "  ok   $1"; pass=$((pass+1)); }
bad() { echo "  FAIL $1"; fail=$((fail+1)); }
check() { if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (got '$2', want '$3')"; fi; }

mkdir -p "$T/stub"
printf '#!/bin/sh\nexit 0\n' > "$T/stub/mount"; cp "$T/stub/mount" "$T/stub/sync"
chmod +x "$T/stub/mount" "$T/stub/sync"

R=; GUARD=
fresh() {   # fresh [nobusybox] — a new scratch player with Cinder installed and healthy
    R="$T/r$RANDOM$RANDOM"; mkdir -p "$R/db" "$R/data/cinder" "$R/system/vendor/sony/bin" \
        "$R/system/vendor/unknown321" "$R/system/bin" "$R/xbin"
    if [ "${1:-}" != nobusybox ]; then
        printf '#!/bin/sh\ncmd="$1"; shift\nexec "$cmd" "$@"\n' > "$R/xbin/busybox"; chmod +x "$R/xbin/busybox"
    fi
    GUARD="$R/guard.sh"
    sed -e "s#/db/cinder-guard#$R/db/cinder-guard#g" -e "s#/system/#$R/system/#g" \
        -e "s#/data/cinder#$R/data/cinder#g" -e "s#/xbin/busybox#$R/xbin/busybox#g" \
        "$HERE/deploy/cinder-guard.sh" > "$GUARD"
    printf 'name: HgrmMediaPlayerApp\ncommand: /system/vendor/unknown321/bin/cinderhome-launch.sh\n' \
        > "$R/system/vendor/sony/bin/HgrmMediaPlayerApp.appcfg"
    printf 'name: HgrmMediaPlayerApp\ncommand: HgrmMediaPlayerApp\n' \
        > "$R/system/vendor/unknown321/HgrmMediaPlayerApp.appcfg.real"
    echo 0 > "$R/data/cinder/bootcount"
    mkdir -p "$R/db/cinder-guard"; : > "$R/db/cinder-guard/backstop_on"
}
HAGO() { echo "$R/system/vendor/sony/bin/hagodaemon"; }
wrapper() {   # wrapper [bytes] — Sony's binary kept aside, a wrapper in its place
    [ -f "$(HAGO).real" ] || echo "sony hagodaemon" > "$(HAGO).real"
    echo "${1:-wrapper build one}" > "$(HAGO)"
}
boot()  { PATH="$T/stub:$PATH" sh "$GUARD" >/dev/null 2>&1; }
proof() { for f in "$@"; do : > "$R/data/cinder/$f"; done; }
which_hago() { if cmp -s "$(HAGO)" <(echo "sony hagodaemon"); then echo sony; else echo wrapper; fi; }
logged() { grep -c "$1" "$R/db/cinder-guard/log" 2>/dev/null || true; }
proofs() { local n=0 f; for f in "$R"/data/cinder/hago_*; do [ -e "$f" ] && n=$((n+1)); done; echo "$n"; }

echo "the wrapper's trial"
fresh; echo "sony hagodaemon" > "$(HAGO)"; boot
check "no wrapper installed: nothing said about one" "$(logged 'hagodaemon wrapper')" 0

fresh; wrapper; proof hago_ok; boot
check "first boot: the wrapper is left in place" "$(which_hago)" wrapper
check "first boot: proof from before it is cleared" "$(proofs)" 0
boot
check "second boot with no proof at all: Sony's binary is back" "$(which_hago)" sony
check "…and the kept copy is gone, so the player reads as having no wrapper" "$([ -e "$(HAGO).real" ] && echo kept || echo gone)" gone
boot
check "…and the boot after that does nothing more" "$(logged 'putting Sony')" 1

fresh; wrapper; boot; proof hago_up; boot
check "it booted, nothing played: still the wrapper" "$(which_hago)" wrapper
check "…and that boot's proof is spent" "$([ -e "$R/data/cinder/hago_up" ] && echo there || echo spent)" spent
boot
check "…so a boot that then proves nothing puts Sony's binary back" "$(which_hago)" sony

fresh; wrapper; boot; proof hago_up hago_play; boot
check "playback began and never proved itself: Sony's binary is back" "$(which_hago)" sony

fresh; wrapper; boot; proof hago_up hago_ok; boot
check "audio played: the wrapper is confirmed" "$(logged 'confirmed')" 1
for _ in 1 2 3 4 5 6 7 8 9 10; do boot; done
check "a confirmed wrapper survives ten boots with no proof" "$(which_hago)" wrapper
check "…and its proof files were cleared at confirmation" "$(proofs)" 0
wrapper "wrapper build two"; boot
check "a different build starts its own trial" "$(logged 'first boot of this build')" 2
boot
check "…and is put back when it proves nothing" "$(which_hago)" sony

fresh; wrapper; echo "wrapper build one" > "$(HAGO).real"; boot; boot
check "Sony's binary under both names: not treated as a wrapper" "$(logged 'hagodaemon wrapper')" 0

fresh; wrapper; : > "$(HAGO).real"; boot; boot
check "an empty kept copy is never copied over the wrapper" "$(which_hago)" wrapper

fresh nobusybox; wrapper; boot; proof hago_up hago_ok; boot
check "no busybox: audio played, but it cannot be confirmed" "$(logged 'stays on trial')" 1
check "…the wrapper stays for that boot" "$(which_hago)" wrapper
boot; boot
check "…and it is put back by the first boot that proves nothing" "$(which_hago)" sony
check "…with the kept copy left, since nothing could compare the two" "$([ -e "$(HAGO).real" ] && echo kept || echo gone)" kept

echo "the Home app backstop"
APP() { echo "$R/system/vendor/sony/bin/HgrmMediaPlayerApp.appcfg"; }
fresh; rm "$R/system/vendor/unknown321/HgrmMediaPlayerApp.appcfg.real"; echo 4 > "$R/data/cinder/bootcount"
boot; boot; boot; boot
check "Cinder not installed: the .appcfg is never touched" "$(grep -c cinderhome-launch "$(APP)")" 1

fresh; echo 2 > "$R/data/cinder/bootcount"; boot; boot
check "two unproven boots: still Cinder" "$(grep -c cinderhome-launch "$(APP)")" 1
boot
check "the third: Sony's Home app is back" "$(grep -c '^command: HgrmMediaPlayerApp$' "$(APP)")" 1
check "…readable by appmgr (0755)" "$(stat -c %a "$(APP)")" 755

fresh; echo 2 > "$R/data/cinder/bootcount"; boot; boot; echo 0 > "$R/data/cinder/bootcount"; boot
check "a healthy boot resets the count" "$(cat "$R/db/cinder-guard/count")" 0
echo 2 > "$R/data/cinder/bootcount"; boot; boot
check "…so two more unproven boots do not revert" "$(grep -c cinderhome-launch "$(APP)")" 1

fresh; rm "$R/data/cinder/bootcount"; boot; boot; boot
check "the launcher never ran at all: reverted on the third boot" "$(grep -c '^command: HgrmMediaPlayerApp$' "$(APP)")" 1

fresh; rm "$R/db/cinder-guard/backstop_on"; echo 3 > "$R/data/cinder/bootcount"; boot; boot; boot; boot
check "not asked for: any number of unproven boots leave Cinder in place" "$(grep -c cinderhome-launch "$(APP)")" 1
check "…and nothing is counted" "$([ -e "$R/db/cinder-guard/count" ] && echo counted || echo none)" none
wrapper; boot; boot
check "…while the wrapper's trial still runs without it" "$(which_hago)" sony

fresh; head -c 80000 /dev/zero | tr '\0' 'x' > "$R/db/cinder-guard/log"; boot
check "a log past 64 KB is cut back" "$([ "$(stat -c %s "$R/db/cinder-guard/log")" -lt 20000 ] && echo cut || echo long)" cut

echo; echo "guard: $pass ok, $fail FAIL"
[ "$fail" -eq 0 ]
