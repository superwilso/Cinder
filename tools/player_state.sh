#!/usr/bin/env bash
# player_state.sh — save Cinder's state on the player under a name, and put a saved one back, so a
# bug can be tried against a known setup in a minute instead of by setting it up by hand.
#
#   tools/player_state.sh save NAME    copy the player's Cinder state into $CINDER_STATES/NAME
#   tools/player_state.sh load NAME    replace the player's with it, then restart (about a minute)
#   tools/player_state.sh load fresh   no state at all: first-start defaults, the introduction runs
#   tools/player_state.sh list
#
# $CINDER_STATES defaults to ~/.cinder/player-states. A state holds the owner's settings, likes and
# play counts: keep it out of any repository.
#
# WHAT A STATE IS: what a person changes by using Cinder — settings, Bluetooth and advanced
# settings, likes, play counts, palettes, playlists, the volume table, the queue and where it was.
# NEVER the recovery ladder or the install: the bad-boot counter, the latch, the cable escape and
# its pass, the mono shim's load count, the component switches and the binaries are not read and
# not written, so loading a state can never strand the player on stock or change what is installed.
#
# LOADING STOPS THE APP FIRST, as tools/cinder-install.sh does, so nothing it saves on the way out
# can land over the files going in: the files are staged in /data/local/tmp, no_respawn is armed,
# cinder-home is stopped, the files are put in place (and the ones the state does not have are
# removed), and the player restarts. Afterwards each file is read back and compared.
set -euo pipefail

states="${CINDER_STATES:-$HOME/.cinder/player-states}"
CONTENTS=(cinder_settings.conf cinder_bt.conf cinder_advanced.conf cinder_liked.conf
          cinder_loved.tsv cinder_stats.tsv cinder_voltable.conf cinder_ram_suspend)
CONTENTS_DIRS=(cinder_palettes cinder_playlists)
DATA=(queue.conf resume.conf bt_vol_answers)
STAGE=/data/local/tmp/cinder_state

die() { echo "player_state: $*" >&2; exit 1; }
need_adb() { [ "$(adb get-state 2>/dev/null)" = device ] || die "no player on adb"; }
# Every device path a state covers, files then directories.
paths() {
    for f in "${CONTENTS[@]}"; do echo "/contents/$f"; done
    for f in "${DATA[@]}"; do echo "/data/cinder/$f"; done
    for d in "${CONTENTS_DIRS[@]}"; do echo "/contents/$d/"; done
}

cmd="${1:-}"; name="${2:-}"
case "$cmd" in
list)
    [ -d "$states" ] && ls -1 "$states" || echo "(no states yet in $states)"
    ;;
save)
    [ -n "$name" ] && [ "$name" != fresh ] || die "save needs a NAME (not 'fresh')"
    need_adb
    dir="$states/$name"; rm -rf "$dir"; mkdir -p "$dir"
    n=0
    while read -r p; do
        if adb pull "${p%/}" "$dir/$(dirname "${p%/}" | tr / _)__$(basename "${p%/}")" >/dev/null 2>&1; then
            n=$((n + 1))
        fi
    done < <(paths)
    date -u +%Y-%m-%dT%H:%M:%SZ > "$dir/.saved"
    echo "player_state: saved $n items as '$name' ($dir)"
    ;;
load)
    [ -n "$name" ] || die "load needs a NAME, or 'fresh'"
    dir="$states/$name"
    [ "$name" = fresh ] || [ -d "$dir" ] || die "no state '$name' (try: list)"
    need_adb
    adb shell "/xbin/busybox rm -rf $STAGE; mkdir -p $STAGE" </dev/null
    if [ "$name" != fresh ]; then
        for f in "$dir"/*__*; do adb push "$f" "$STAGE/$(basename "$f")" >/dev/null; done
    fi
    # The device half. Item names are <dir with _ for />__<name>, as `save` wrote them.
    script=$(cat <<EOF
BB=/xbin/busybox
touch /data/cinder/no_respawn
PID=\$(ps | grep /system/vendor/cinder/bin/cinder-home | grep -v grep | \$BB awk '{print \$2}' | head -1)
[ -n "\$PID" ] && kill \$PID
for i in 1 2 3 4 5 6 7 8 9 10; do
    ps | grep /system/vendor/cinder/bin/cinder-home | grep -v grep >/dev/null || break
    sleep 1
done
$(paths | while read -r p; do
    q="${p%/}"; item="$(dirname "$q" | tr / _)__$(basename "$q")"
    echo "\$BB rm -rf '$q'"
    echo "[ -e '$STAGE/$item' ] && \$BB cp -a '$STAGE/$item' '$q'"
    case "$q" in /data/cinder/*) echo "[ -e '$q' ] && chown system:system '$q' && chmod 600 '$q'";; esac
done)
sync
rm /data/cinder/no_respawn
\$BB rm -rf $STAGE
sync
reboot
EOF
)
    echo "player_state: loading '$name' — the player restarts"
    adb shell "$script" </dev/null >/dev/null 2>&1 || true
    adb wait-for-device
    # adb shell exits 0 whatever the remote did: test what it prints. Up = cinder-home is running.
    for _ in $(seq 1 60); do
        adb shell 'ps | grep /system/vendor/cinder/bin/cinder-home | grep -v grep' </dev/null 2>/dev/null \
            | grep -q cinder-home && break
        sleep 2
    done
    # Read back: every file in the state must be on the player, byte for byte.
    bad=0
    if [ "$name" != fresh ]; then
        for f in "$dir"/*__*; do
            [ -f "$f" ] || continue
            base="$(basename "$f")"; q="$(echo "${base%%__*}" | tr _ /)/${base#*__}"
            want="$(md5sum < "$f" | cut -d' ' -f1)"
            got="$(adb shell "/xbin/busybox md5sum '$q'" </dev/null 2>/dev/null | cut -d' ' -f1 | tr -d '\r')"
            [ "$want" = "$got" ] || { echo "  differs after the restart: $q"; bad=$((bad + 1)); }
        done
    fi
    [ "$bad" = 0 ] && echo "player_state: '$name' is on the player" || die "$bad file(s) did not match"
    ;;
*)
    sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'
    exit 1
    ;;
esac
